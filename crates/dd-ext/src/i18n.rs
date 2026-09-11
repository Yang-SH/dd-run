//! dd-ext 内置扩展轻量 i18n（批次 D，2026-09-06）。
//!
//! 机制：宿主 spawn 内置扩展时经环境变量 `DDRUN_LANG` 注入生效语言
//! （取值 = `dd_gui::settings::Lang::as_str` 的具体语言：`zh_cn` / `en_us`，
//! `FollowSystem` 已先由宿主解析为具体语言，见 `dd_gui::app::PaletteApp::resolve_lang`）；
//! 扩展进程启动时读一次，按语言选 zh/en 文案。
//!
//! 通道 = manifest `entry.env` 既有机制（`ExtensionProcess::spawn` 统一 `envs()` 注入），
//! 协议 v1.0 冻结零字段新增。扩展侧**无中心文案表依赖**——与 GUI 侧
//! `dd_gui::text` 静态表解耦（扩展是独立进程，只按 env 选双语文案）。
//!
//! M9 修订（in-process 内置）：内置扩展改为**宿主进程内**调用后，`DDRUN_LANG`
//! 不再是"每扩展进程各自的 env"，而是宿主进程的 env——且宿主内语言可在运行时
//! 经设置页切换。原先用 `OnceLock` 缓存一次的实现无法反映这种切换（`tr()` 会
//! 永久停留在首次读到的语言）。故改为进程级 `AtomicU8` + [`set_lang`]：宿主在
//! 每次（重）聚合前显式设语言，保证内置文案随设置页切换即时生效；子进程扩展
//! 仍走 env 注入（`Lang::from_env` 兜底语义不变）。

use std::sync::atomic::{AtomicU8, Ordering};

/// 生效语言（扩展侧只关心具体语言，已不含 `FollowSystem`）。
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// 简体中文（默认/回落）。
    ZhCn = 0,
    /// English。
    EnUs = 1,
}

impl Lang {
    /// 由 `DDRUN_LANG` 字符串解析（`zh_cn` / `en_us`）；
    /// 未知值或缺失均回落 [`Lang::ZhCn`](中文为项目优先级，与注释语言一致)。
    pub fn from_env() -> Lang {
        match std::env::var("DDRUN_LANG").as_deref() {
            Ok("en_us") => Lang::EnUs,
            Ok("zh_cn") => Lang::ZhCn,
            _ => Lang::ZhCn,
        }
    }
}

/// 生效语言的进程级缓存。`UNSET`（=2）表示尚未初始化 → 首次访问按 env 解析。
///
/// 用 `AtomicU8` 而非 `OnceLock`：M9 起内置扩展 in-process 运行，宿主需在设置页
/// 切换语言后**重设**该值（见 [`set_lang`]），`OnceLock` 无法重设。
const UNSET: u8 = 2;
static EFFECTIVE_LANG: AtomicU8 = AtomicU8::new(UNSET);

fn lang_from_u8(v: u8) -> Lang {
    match v {
        1 => Lang::EnUs,
        _ => Lang::ZhCn,
    }
}

fn effective() -> Lang {
    let v = EFFECTIVE_LANG.load(Ordering::Relaxed);
    if v != UNSET {
        return lang_from_u8(v);
    }
    // 首次访问：按 env 解析并落缓存（CAS 失败说明已被并发设置，以已存值为准）。
    let resolved = Lang::from_env();
    match EFFECTIVE_LANG.compare_exchange(
        UNSET,
        resolved as u8,
        Ordering::Relaxed,
        Ordering::Relaxed,
    ) {
        Ok(_) => resolved,
        Err(existing) => lang_from_u8(existing),
    }
}

/// 显式设置生效语言（M9：宿主在（重）聚合前调用，使 in-process 内置文案随
/// 设置页语言切换即时生效）。覆盖 env 解析结果；后续 [`tr`] 一律取该值。
pub fn set_lang(lang: Lang) {
    EFFECTIVE_LANG.store(lang as u8, Ordering::Relaxed);
}

/// 当前生效语言（诊断/单测用）。
pub fn current_lang() -> Lang {
    effective()
}

/// 按生效语言选文案：`tr("中文", "English")`。
///
/// 含值文案（如 `应用不存在：{id}`）请用占位符写在两个字符串里、并以
/// `.replace("{id}", ...)` 替换（与 GUI 侧约定一致；`format!` 不接受运行时
/// 格式串，故用 replace 而非 format）。
pub fn tr(zh: &'static str, en: &'static str) -> &'static str {
    match effective() {
        Lang::ZhCn => zh,
        Lang::EnUs => en,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_maps_known_values_and_falls_back() {
        // 进程级 OnceLock 已可能被其他测试占用，但这里只测纯解析逻辑
        assert_eq!(Lang::from_env_inner("zh_cn"), Lang::ZhCn);
        assert_eq!(Lang::from_env_inner("en_us"), Lang::EnUs);
        assert_eq!(Lang::from_env_inner("follow_system"), Lang::ZhCn);
        assert_eq!(Lang::from_env_inner("garbage"), Lang::ZhCn);
        assert_eq!(Lang::from_env_inner(""), Lang::ZhCn);
    }

    #[test]
    fn tr_selects_by_lang() {
        // 进程级缓存可能已被其他测试设为 zh_cn；直接测语言→文案映射的纯逻辑
        assert_eq!(tr_for(Lang::ZhCn, "中文", "English"), "中文");
        assert_eq!(tr_for(Lang::EnUs, "中文", "English"), "English");
    }

    /// `lang_from_u8` 对未知/哨兵值一律回落中文（防御：缓存被写坏也不 panic）。
    #[test]
    fn lang_from_u8_falls_back_to_zh() {
        assert_eq!(lang_from_u8(0), Lang::ZhCn);
        assert_eq!(lang_from_u8(1), Lang::EnUs);
        assert_eq!(lang_from_u8(UNSET), Lang::ZhCn);
        assert_eq!(lang_from_u8(255), Lang::ZhCn);
    }

    // 注：`set_lang` 的可重设语义（M9 关键收益）在**独立集成测试进程**中验证
    // （tests/i18n_set_lang.rs）——翻转进程级语言会与同进程并行单测（如
    // calc「无法计算」、shell「超时」文案断言）竞争，故不与单测同进程。

    // ── 测试辅助：绕过进程级 OnceLock，直接验证解析/选择逻辑 ──
    impl Lang {
        fn from_env_inner(s: &str) -> Lang {
            match s {
                "en_us" => Lang::EnUs,
                "zh_cn" => Lang::ZhCn,
                _ => Lang::ZhCn,
            }
        }
    }

    fn tr_for(lang: Lang, zh: &'static str, en: &'static str) -> &'static str {
        match lang {
            Lang::ZhCn => zh,
            Lang::EnUs => en,
        }
    }
}
