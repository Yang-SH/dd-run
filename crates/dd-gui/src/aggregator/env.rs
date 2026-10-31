//! 批次 B（refactor-structure-2026-10-21.md §4.2）：扩展进程环境注入。
//! 自 `aggregator.rs` 原块（148–217 行）逐字节搬入、零逻辑改动；
//! 外部路径经 `aggregator/mod.rs` 的 `pub use` 桥保持不变。

use std::collections::BTreeMap;

use dd_host::manifest::LoadedExtension;

/// 配置通道 = manifest `entry.env` 既有机制（`ExtensionProcess::spawn` 统一
/// `envs()` 注入），协议 v1.0 冻结零字段新增；扩展侧未注入/解析失败时回落
/// 其内置默认引擎表。`engines_json` 格式见 `Settings::search_engines_env`。
pub fn inject_websearch_env(exts: &mut [LoadedExtension], engines_json: &str) {
    for ext in exts
        .iter_mut()
        .filter(|e| e.manifest.id == "com.ddrun.websearch")
    {
        ext.manifest
            .entry
            .env
            .insert("DD_WEBSEARCH_ENGINES".to_string(), engines_json.to_string());
    }
}

/// N2（2026-10-04）：扩展用户可调配置的环境变量名前缀——`DD_EXT_CFG_<KEY>`。
/// 前缀约定使结果名**结构性避开** S-10 保护名单（`PATH` 等宿主关键变量）。
pub const EXT_CFG_ENV_PREFIX: &str = "DD_EXT_CFG_";

/// N2：配置键 → 环境变量名（纯函数，单测锚点）。键仅接受 ASCII 字母/数字/
/// 下划线（1–32），大写化拼接前缀；非法键 → `None`。
pub(super) fn ext_cfg_env_name(key: &str) -> Option<String> {
    if key.is_empty()
        || key.len() > 32
        || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    Some(format!("{EXT_CFG_ENV_PREFIX}{}", key.to_ascii_uppercase()))
}

/// N2（2026-10-04）：把用户可调扩展配置合并注入各扩展 `entry.env` **内存副本**
///（§3 惯例 ① 泛化——`inject_websearch_env` 的「多扩展 + 多键」版；运行时
/// 内存改写不触碰 S-05 信任台账，其哈希对象是清单**文件字节流**）。
///
/// - `cfg` = `ext_id → (键 → 值)`（`Settings.ext_settings`）；不在表内的扩展
///   零改动。in-process 内置对 `entry.env` 不敏感，宿主另走内存通道直接注入
///   （websearch 引擎表先例，见 `app/aggregate.rs`）。
/// - S-10 边界双保险：键名白名单（见 [`ext_cfg_env_name`]）+ 结果名对
///   `dd_host::process::PROTECTED_ENV_KEYS` 复查（命中记日志跳过——纵深防御，
///   前缀约定本身已使碰撞不可能）。
/// - 子进程 spawn 路径仍有 `filter_env_overrides` 终检（S-10 既有第二道门）。
pub fn inject_ext_settings(
    exts: &mut [LoadedExtension],
    cfg: &BTreeMap<String, BTreeMap<String, String>>,
) {
    for (ext_id, kv) in cfg {
        let Some(ext) = exts.iter_mut().find(|e| &e.manifest.id == ext_id) else {
            continue;
        };
        for (key, value) in kv {
            let Some(name) = ext_cfg_env_name(key) else {
                log::warn!(
                    "[dd-gui] 扩展配置键 {key:?} 非法（仅限字母/数字/下划线，1–32），已跳过（ext={ext_id}）"
                );
                continue;
            };
            if dd_host::process::PROTECTED_ENV_KEYS
                .iter()
                .any(|p| p.eq_ignore_ascii_case(&name))
            {
                log::warn!(
                    "[dd-gui] 扩展配置键 {key:?} 映射到受保护环境变量 {name}，已跳过（S-10 纵深防御，ext={ext_id}）"
                );
                continue;
            }
            ext.manifest.entry.env.insert(name, value.clone());
        }
    }
}
