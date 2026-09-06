//! 首屏聚合层：扫描扩展 → 并行拉取 → 合并为可渲染的 [`PanelItem`] 列表。
//!
//! 对齐 [`docs/protocol.md`](../../docs/protocol.md) §5（握手）/ §6.1（顶层命令）
//! 与 [`docs/implementation.md`](../../docs/implementation.md) M1「首屏聚合」任务：
//! - **并行**：每扩展一个线程（进程对象线程独占），互不阻塞（A12 能力调用不阻塞 UI）；
//! - **错误隔离**：单个扩展失败只记入 [`SourceSummary`]，不影响其他扩展与整体渲染；
//! - **内置扩展常驻**（M4 P4 `ensure_builtins`）：`dd-ext-apps/calc/system/websearch/shell`
//!   由宿主**内存自注册**（manifest-schema §10：内置同样走清单注册，MVP 无安装器 →
//!   宿主启动时直接构造 `LoadedExtension`）。其可执行文件来源见
//!   [`load_extension_sources`]：打包后走**内嵌物化**（单文件 `dd-run.exe`），
//!   开发期回退宿主 exe 同目录；
//!   扩展目录中的第三方清单与其**并存**，同 id 以内置优先。
//!
//! M3 缓存与懒加载（见 [`docs/implementation.md`](../../docs/implementation.md) §M3）：
//! - **frozen + 磁盘桩命中** → [`ExtItems::Stub`]：**不拉起进程**（A6），首屏读桩渲染；
//! - **frozen 无桩**（首次运行）→ 照常 spawn 拉取并**落盘**（下次冷启动读桩）；
//! - **fresh**（`frozen=false`）→ spawn 拉取，**不落盘**；
//! - M4 宿主 fallback 轮补充：握手后 `provider.has_fallback == true`（设计文档 §6.3
//!   "含兜底能力者一律视为 fresh"）→ **即使清单标 frozen 也不落盘**，并清除历史桩，
//!   保证进程恒 warm 可响应 `fallback_commands`；
//! - 源状态三态：Warm（进程活）/ Stub（仅桩）/ Failed（失败），供页脚展示与 A6 观察。

use std::path::PathBuf;
use std::thread;

use dd_host::builtin::{ensure_builtins, merge_builtins};
use dd_host::cache::{FrozenCache, FrozenSnapshot};
use dd_host::manifest::{self, LoadedExtension, ScanOptions};
use dd_host::process::ExtensionProcess;
use dd_protocol::messages::InitializeResult;
use dd_protocol::model::CommandItem;

use crate::state::PanelItem;
use crate::text;
use dd_gui::settings::Lang;

/// 协议版本（protocol.md §13：`MAJOR.MINOR` 两段）。
pub const PROTOCOL_VERSION: &str = "1.0";
/// 宿主版本（semver，`initialize` 的 `host.version`）。
pub const HOST_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 单个扩展的拉取结果（**不携带进程**，便于纯逻辑单测构造）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtItems {
    /// 进程保活、命令已取回（warm）。
    Ready {
        id: String,
        name: String,
        items: Vec<CommandItem>,
    },
    /// 磁盘桩命中（frozen 冷启动，**无进程**，A6）；点击其命令触发复热。
    Stub {
        id: String,
        name: String,
        items: Vec<CommandItem>,
    },
    Failed {
        id: String,
        name: String,
        error: String,
    },
}

/// 一次聚合的完整结果。
pub struct CollectResult {
    /// 成功拉取后**保活的进程**（顺序与 [`ExtItems::Ready`] 一一对应，
    /// 供 M2 的 `invoke` 复用；不手动 `close`，随宿主退出由 Drop 清理）。
    pub processes: Vec<ExtensionProcess>,
    /// 每个扩展的拉取结果（含失败项）。
    pub per_ext: Vec<ExtItems>,
}

/// 扩展源状态汇总（页脚/列表尾展示）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSummary {
    pub id: String,
    pub name: String,
    pub status: SourceStatus,
}

/// 单个扩展源的展示状态（M3 起为**三态**，供页脚展示与 A6 真机观察）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceStatus {
    /// 进程保活中（warm）：冷启动 fresh 扩展、或桩项复热成功后。
    Warm {
        commands: usize,
    },
    /// 仅磁盘桩、无活进程（frozen 冷启动读桩；LRU 驱逐后回落）。点击其命令会触发复热。
    Stub {
        commands: usize,
    },
    Failed {
        error: String,
    },
}

impl SourceStatus {
    /// 是否处于失败态。
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    /// 是否处于桩态（无进程，点击需复热）。
    pub fn is_stub(&self) -> bool {
        matches!(self, Self::Stub { .. })
    }
}

impl ExtItems {
    /// 扩展清单 id（`Ready` / `Stub` / `Failed` 均有，供进程配对与诊断）。
    pub fn id(&self) -> &str {
        match self {
            ExtItems::Ready { id, .. }
            | ExtItems::Stub { id, .. }
            | ExtItems::Failed { id, .. } => id,
        }
    }

    /// 是否已成功拉取并保活进程（warm）。
    pub fn is_ready(&self) -> bool {
        matches!(self, ExtItems::Ready { .. })
    }
}

/// 把搜索引擎配置注入内置 websearch 扩展的进程环境（`DD_WEBSEARCH_ENGINES`）。
///
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

/// 扫描扩展目录并**合并内置扩展**（M4 P4 `ensure_builtins`）。
///
/// 返回 `(扩展列表, 备注)`。内置 5 个（exe 存在者）**恒注册**，扩展目录中的
/// 第三方清单与其并存（同 id 以内置优先，`merge_builtins` 去重）。
/// 备注仅在异常时非空（找不到内置 exe / 目录不可读），供 UI 提示。
///
/// 内置扩展的可执行文件来源（单文件分发后）：
/// - **优先**：宿主内嵌的扩展 exe（经 [`crate::embedded::materialize`] 物化到
///   `%APPDATA%/dd-run/cache/embedded/`）——这是打包后的 `dd-run.exe` 路径；
/// - **回退**：与宿主 exe 同目录的 `dd-ext-*.exe`（开发期 / 未打包的多文件部署）。
pub fn load_extension_sources() -> (Vec<LoadedExtension>, String) {
    // 内置扩展目录：先尝试内嵌物化目录，其次宿主 exe 同目录（cargo 把 workspace
    // 所有 bin 放同一目录，供开发期直接 cargo run / 测试使用）。
    let exe_dir: Option<PathBuf> = crate::embedded::materialize().or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    });
    let mut note = String::new();
    let builtins = match &exe_dir {
        Some(d) => {
            let exts = ensure_builtins(d);
            if exts.is_empty() {
                note = format!(
                    "未找到内置扩展可执行文件（{} 下无 dd-ext-*.exe）",
                    d.display()
                );
            }
            exts
        }
        None => {
            note = "无法定位内置扩展可执行文件目录，内置扩展未注册".to_string();
            Vec::new()
        }
    };

    // 第三方/磁盘扩展：extensions.d 扫描结果并入（同 id 内置优先）
    let scanned = match manifest::extensions_dir() {
        Some(d) => {
            let outcome = manifest::scan_dir(&d, &ScanOptions::default());
            if let Some(err) = &outcome.dir_error {
                if !note.is_empty() {
                    note.push('；');
                }
                note.push_str(&format!("扩展目录不可读：{err}"));
            }
            outcome.loaded
        }
        None => {
            if !note.is_empty() {
                note.push('；');
            }
            note.push_str("无法定位扩展目录（home 环境变量缺失）");
            Vec::new()
        }
    };

    let merged = merge_builtins(builtins, scanned);
    if merged.is_empty() && note.is_empty() {
        note = "无可用扩展（内置与扩展目录均为空）".to_string();
    }
    (merged, note)
}

/// 单个扩展线程的原始结果（携带进程，跨线程回传）。
enum ExtOutcome {
    Ready {
        proc: Box<ExtensionProcess>,
        id: String,
        name: String,
        items: Vec<CommandItem>,
    },
    /// 磁盘桩命中：无进程（A6）。
    Stub {
        id: String,
        name: String,
        items: Vec<CommandItem>,
    },
    Failed {
        id: String,
        name: String,
        error: String,
    },
}

/// 并行收集首屏：每扩展一个线程，进程对象线程独占，join 回传。
///
/// M3 分流（见模块文档）：frozen + 磁盘桩命中 → [`ExtOutcome::Stub`]（不 spawn）；
/// frozen 无桩（首启）→ spawn 拉取并落盘；fresh → spawn 拉取不落盘。
/// `cache` 用 scoped thread 共享只读借用（`FrozenCache` 仅含目录路径，无内部状态）。
pub fn collect_top_level(exts: &[LoadedExtension], cache: Option<&FrozenCache>) -> CollectResult {
    let mut processes = Vec::new();
    let mut per_ext = Vec::with_capacity(exts.len());
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(exts.len());
        for ext in exts {
            let ext = ext.clone();
            handles.push(scope.spawn(move || load_one(ext, cache)));
        }
        for handle in handles {
            match handle.join() {
                Ok(ExtOutcome::Ready {
                    proc,
                    id,
                    name,
                    items,
                }) => {
                    processes.push(*proc);
                    per_ext.push(ExtItems::Ready { id, name, items });
                }
                Ok(ExtOutcome::Stub { id, name, items }) => {
                    per_ext.push(ExtItems::Stub { id, name, items });
                }
                Ok(ExtOutcome::Failed { id, name, error }) => {
                    per_ext.push(ExtItems::Failed { id, name, error });
                }
                Err(_) => per_ext.push(ExtItems::Failed {
                    id: "unknown".to_string(),
                    name: "扩展线程".to_string(),
                    error: "拉取线程 panic".to_string(),
                }),
            }
        }
    });
    CollectResult { processes, per_ext }
}

/// spawn → `initialize`（握手+版本协商）。供首屏拉取与 **GUI 桩复热链路**复用。
pub fn spawn_and_initialize(ext: &LoadedExtension) -> Result<ExtensionProcess, String> {
    spawn_and_initialize_with_info(ext).map(|(proc, _)| proc)
}

/// 同 [`spawn_and_initialize`]，但一并返回握手结果（`ProviderInfo`）。
///
/// 用途：宿主需据 `provider.has_fallback` 决定是否落 frozen 桩
/// （设计文档 §6.3：含兜底能力者一律视为 fresh，不落桩）。
pub fn spawn_and_initialize_with_info(
    ext: &LoadedExtension,
) -> Result<(ExtensionProcess, InitializeResult), String> {
    let mut spawned = ExtensionProcess::spawn(ext).map_err(|e| format!("spawn 失败：{e}"))?;
    let init = spawned
        .initialize(PROTOCOL_VERSION, HOST_VERSION)
        .map_err(|e| format!("initialize 失败：{e}"))?;
    Ok((spawned, init))
}

/// 一个扩展的完整链路：M3 分流后 spawn → initialize → top_level_commands（+落盘）。
fn load_one(ext: LoadedExtension, cache: Option<&FrozenCache>) -> ExtOutcome {
    let id = ext.manifest.id.clone();
    let name = ext.manifest.name.clone();
    let version = ext.manifest.version.clone();

    // M3：frozen 且磁盘桩命中（键 = id + version，`FrozenCache::load` 已按当前
    // version 精确定位）→ **不拉起进程**（A6），首屏直接渲染桩。
    if ext.manifest.frozen {
        if let Some(snap) = cache.and_then(|c| c.load(&id, &version)) {
            return ExtOutcome::Stub {
                id,
                name,
                items: snap.commands,
            };
        }
    }

    // 无桩（frozen 首启）或 fresh：spawn → initialize → top_level_commands。
    let (mut spawned, init) = match spawn_and_initialize_with_info(&ext) {
        Ok(pair) => pair,
        Err(e) => return ExtOutcome::Failed { id, name, error: e },
    };
    // §6.3：含兜底能力者一律视为 fresh——不落桩；若历史桩存在则清除，
    // 避免下次冷启动读桩（无进程 → fallback_commands 拉不到）。
    let has_fallback = init.provider.has_fallback;
    match spawned.top_level_commands() {
        Ok(items) => {
            if ext.manifest.frozen && !has_fallback {
                // M3：frozen 成功拉取 → 落盘桩（下次冷启动读桩不拉起）。
                // 先清同 id 的旧版本桩，避免旧文件残留；落盘失败不致命（本次仍 warm 服务，
                // 仅下次冷启动退化为再拉一次）。
                if let Some(c) = cache {
                    c.invalidate_if_version_changed(&id, &version);
                    let snap = FrozenSnapshot {
                        ext_id: id.clone(),
                        version: version.clone(),
                        commands: items.clone(),
                    };
                    let _ = c.save(&snap);
                }
            } else if has_fallback {
                // fresh（含兜底）：确保磁盘上没有它的桩文件
                if let Some(c) = cache {
                    let _ = c.remove(&id);
                }
            }
            ExtOutcome::Ready {
                proc: Box::new(spawned),
                id,
                name,
                items,
            }
        }
        Err(e) => ExtOutcome::Failed {
            id,
            name,
            error: format!("top_level_commands 失败：{e}"),
        },
    }
}

/// 把各扩展结果合并为可渲染列表 + 源状态汇总（纯函数，可单测）。
/// `lang` 传入为渲染时的生效语言（v4.14 D40：类别徽标随 GUI 语言切换），
/// `lang_dirty` 离开设置页触发的重聚合会随之更新徽标。
pub fn flatten(per_ext: &[ExtItems], lang: Lang) -> (Vec<PanelItem>, Vec<SourceSummary>) {
    let mut items = Vec::new();
    let mut sources = Vec::with_capacity(per_ext.len());
    for ext in per_ext {
        match ext {
            ExtItems::Ready {
                id,
                name,
                items: cmds,
            } => {
                sources.push(SourceSummary {
                    id: id.clone(),
                    name: name.clone(),
                    status: SourceStatus::Warm {
                        commands: cmds.len(),
                    },
                });
                for cmd in cmds {
                    items.push(to_panel_item(cmd, id, name, lang));
                }
            }
            ExtItems::Stub {
                id,
                name,
                items: cmds,
            } => {
                sources.push(SourceSummary {
                    id: id.clone(),
                    name: name.clone(),
                    status: SourceStatus::Stub {
                        commands: cmds.len(),
                    },
                });
                for cmd in cmds {
                    items.push(to_panel_item(cmd, id, name, lang));
                }
            }
            ExtItems::Failed { id, name, error } => {
                sources.push(SourceSummary {
                    id: id.clone(),
                    name: name.clone(),
                    status: SourceStatus::Failed {
                        error: error.clone(),
                    },
                });
            }
        }
    }
    (items, sources)
}

/// [`CommandItem`] → [`PanelItem`]；`section` 缺省时用扩展名兜底分组。
///
/// `ext_id` 记录命令来源扩展（`invoke` / `get_items` 时定位子进程）。
/// `icon` 从 `CommandItem.icon`（§8.6 三态）**透传**——渲染层决定如何显示；
/// 列表/嵌套页/fallback 模板均经此函数，保证全链路图标一致（M5 UI 批次 2）。
///
/// M5 批次 3.9：按 `ext_id` 去 `com.ddrun.` 前缀推导通用类别标签（设计文档 §6.2），
/// 内置扩展映射为「应用/命令/设置/网页」，第三方回退「命令」。
/// 语言按当前生效语言解析（v4.14 D40，与 GUI 其他徽标/页脚口径一致）——
/// `lang_dirty` 离开设置页触发的重聚合会随之更新类别标签。
pub fn to_panel_item(
    cmd: &CommandItem,
    ext_id: &str,
    fallback_section: &str,
    lang: Lang,
) -> PanelItem {
    PanelItem {
        id: cmd.id.clone(),
        ext_id: ext_id.to_string(),
        title: cmd.title.clone(),
        subtitle: cmd.subtitle.clone().unwrap_or_default(),
        section: cmd
            .section
            .clone()
            .unwrap_or_else(|| fallback_section.to_string()),
        icon: cmd.icon.clone(),
        tags: cmd.tags.clone().unwrap_or_default(),
        result_category: Some(category_label_for(ext_id, lang).to_string()),
        // M6 批次 6.1（L4）：预计算拼音匹配索引（全拼+首字母），协议层零改动
        pinyin: crate::state::pinyin_haystack(&cmd.title),
        command: cmd.command.clone(),
    }
}

/// 按扩展清单 id 推导结果类别显示标签（设计文档 §6.2 映射表）。
///
/// 内置扩展使用全限定 id，去 `com.ddrun.` 前缀后匹配；未知第三方统一回退「命令」。
/// 文案经 `text::t` 按生效语言解析（v4.14 D40）。
fn category_label_for(ext_id: &str, lang: Lang) -> &'static str {
    let short = ext_id.strip_prefix("com.ddrun.").unwrap_or(ext_id);
    let key = match short {
        "apps" => "cat.apps",
        "system" => "cat.system",
        "websearch" => "cat.websearch",
        // calc / shell / 第三方统一归为「命令」
        _ => "cat.command",
    };
    text::t(lang, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_protocol::model::{CommandRef, Icon, IconKind};

    fn cmd(id: &str, title: &str, section: Option<&str>) -> CommandItem {
        CommandItem {
            id: id.to_string(),
            title: title.to_string(),
            subtitle: None,
            icon: None,
            section: section.map(|s| s.to_string()),
            tags: None,
            details: None,
            text_to_suggest: None,
            more_commands: None,
            command: CommandRef::Invoke,
        }
    }

    fn ready(id: &str, name: &str, items: Vec<CommandItem>) -> ExtItems {
        ExtItems::Ready {
            id: id.to_string(),
            name: name.to_string(),
            items,
        }
    }

    #[test]
    fn maps_command_item_fields_and_fallback_section() {
        let item = cmd("a.1", "Hello", None);
        let panel = to_panel_item(&item, "com.example.a", "ExtA", Lang::ZhCn);
        assert_eq!(panel.id, "a.1", "id 透传");
        assert_eq!(panel.ext_id, "com.example.a", "来源扩展 id 透传");
        assert_eq!(panel.title, "Hello");
        assert_eq!(panel.section, "ExtA", "section 缺省用扩展名兜底");
        assert_eq!(panel.command, CommandRef::Invoke, "command 透传");

        let with_section = cmd("a.2", "Bye", Some("系统"));
        let panel = to_panel_item(&with_section, "com.example.a", "ExtA", Lang::ZhCn);
        assert_eq!(panel.section, "系统", "扩展返回的 section 优先");
    }

    #[test]
    fn category_label_is_derived_from_ext_id() {
        // M5 批次 3.9：内置扩展按去前缀映射，第三方回退「命令」。
        // v4.14 D40：类别徽标按生效语言解析（zh/en 双口径都覆盖）。
        let zh_cases = [
            ("com.ddrun.apps", "应用"),
            ("com.ddrun.calc", "命令"),
            ("com.ddrun.system", "设置"),
            ("com.ddrun.websearch", "网页"),
            ("com.ddrun.shell", "命令"),
            ("com.example.unknown", "命令"),
        ];
        for (ext_id, expected) in zh_cases {
            let panel = to_panel_item(&cmd("x", "X", None), ext_id, "Sec", Lang::ZhCn);
            assert_eq!(
                panel.result_category.as_deref(),
                Some(expected),
                "{ext_id} zh 应映射为 {expected}"
            );
        }
        let en_cases = [
            ("com.ddrun.apps", "Apps"),
            ("com.ddrun.calc", "Command"),
            ("com.ddrun.system", "Settings"),
            ("com.ddrun.websearch", "Web"),
            ("com.ddrun.shell", "Command"),
            ("com.example.unknown", "Command"),
        ];
        for (ext_id, expected) in en_cases {
            let panel = to_panel_item(&cmd("x", "X", None), ext_id, "Sec", Lang::EnUs);
            assert_eq!(
                panel.result_category.as_deref(),
                Some(expected),
                "{ext_id} en 应映射为 {expected}"
            );
        }
    }

    #[test]
    fn icon_is_passed_through_from_command_item() {
        // M5 UI 批次 2：§8.6 icon 三态（glyph/path/url）都应透传到 PanelItem，
        // 由渲染层按态渲染——此断言锁住"宿主不再丢弃 icon"这一修复。
        let kinds = [IconKind::Glyph, IconKind::Path, IconKind::Url];
        for kind in kinds {
            let icon = Icon {
                kind,
                value: match kind {
                    IconKind::Glyph => "\u{E8C8}".to_string(),
                    IconKind::Path => r"C:\demo\icon.png".to_string(),
                    IconKind::Url => "https://example.com/icon.png".to_string(),
                },
            };
            let mut item = cmd("a.icon", "Iconed", None);
            item.icon = Some(icon.clone());
            let panel = to_panel_item(&item, "com.example.a", "ExtA", Lang::ZhCn);
            assert_eq!(panel.icon, Some(icon), "{kind:?} 应透传");
        }
        // 无 icon → None（渲染空列，不 panic）
        let plain = to_panel_item(
            &cmd("b", "NoIcon", None),
            "com.example.a",
            "ExtA",
            Lang::ZhCn,
        );
        assert_eq!(plain.icon, None);
    }

    #[test]
    fn flatten_merges_warm_stub_and_keeps_failed_isolated() {
        let per_ext = vec![
            ready(
                "com.example.a",
                "Ext A",
                vec![cmd("a.1", "A1", None), cmd("a.2", "A2", Some("系统"))],
            ),
            // M3：磁盘桩（frozen 冷启动，无进程）也并入列表，源状态为 Stub
            ExtItems::Stub {
                id: "com.example.b".to_string(),
                name: "Ext B".to_string(),
                items: vec![cmd("b.1", "B1", None)],
            },
            ExtItems::Failed {
                id: "com.example.c".to_string(),
                name: "Ext C".to_string(),
                error: "initialize 失败：超时".to_string(),
            },
            ready("com.example.d", "Ext D", vec![]),
        ];

        let (items, sources) = flatten(&per_ext, Lang::ZhCn);

        // 合并 2 条(warm) + 1 条(stub) = 3；C 的失败不影响 A/B/D
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].section, "Ext A");
        assert_eq!(items[0].ext_id, "com.example.a", "项带来源扩展 id");
        assert_eq!(items[1].section, "系统");
        assert_eq!(items[2].section, "Ext B", "桩项并入列表并带 section");

        assert_eq!(sources.len(), 4);
        assert_eq!(sources[0].status, SourceStatus::Warm { commands: 2 });
        assert_eq!(sources[1].status, SourceStatus::Stub { commands: 1 });
        assert!(sources[1].status.is_stub());
        assert_eq!(
            sources[2].status,
            SourceStatus::Failed {
                error: "initialize 失败：超时".to_string()
            }
        );
        assert!(sources[2].status.is_failed());
        assert_eq!(sources[3].status, SourceStatus::Warm { commands: 0 });
    }

    #[test]
    fn flatten_empty_input() {
        let (items, sources) = flatten(&[], Lang::ZhCn);
        assert!(items.is_empty());
        assert!(sources.is_empty());
    }
}
