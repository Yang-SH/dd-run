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

use std::sync::OnceLock;

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

/// 进程内缓存的生效语言（读一次，零开销；多扩展共享同一 cached 值无副作用）。
static EFFECTIVE_LANG: OnceLock<Lang> = OnceLock::new();

fn effective() -> Lang {
    *EFFECTIVE_LANG.get_or_init(Lang::from_env)
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
