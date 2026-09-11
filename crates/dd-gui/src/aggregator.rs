//! 首屏聚合层：扫描扩展 → 并行拉取 → 合并为可渲染的 [`PanelItem`] 列表。
//!
//! 对齐 [`docs/protocol.md`](../../docs/protocol.md) §5（握手）/ §6.1（顶层命令）
//! 与 [`docs/implementation.md`](../../docs/implementation.md) M1「首屏聚合」任务：
//! - **并行**：每扩展一个线程（进程对象线程独占），互不阻塞（A12 能力调用不阻塞 UI）；
//! - **错误隔离**：单个扩展失败只记入 [`SourceSummary`]，不影响其他扩展与整体渲染；
//! - **内置扩展常驻**（M4 P4 → M9 in-process）：`dd-ext-apps/calc/system/websearch/shell`
//!   由宿主**内存自注册**（manifest-schema §10：内置同样走清单注册，MVP 无安装器 →
//!   宿主启动时直接构造 `LoadedExtension`）。**M9 起内置扩展以 in-process 方式运行**
//!   （宿主进程内直接调 `dd_ext::serve_line`，见 [`crate::ext_client`]），注册不再
//!   依赖磁盘 `dd-ext-*.exe`（[`dd_host::builtin::builtin_registrations`]），也
//!   不再物化内嵌 exe；第三方 / sidecar 仍为子进程，扩展目录中的清单与其**并存**，
//!   同 id 以内置优先。
//! - **扩展清单扫描双位置**（M7 批次 7.5）：用户数据目录 `extensions.d/`（manifest-schema
//!   §2 主位置）+ **宿主 exe 同目录 `extensions.d/` 便携 sidecar**（免安装 zip「解压即用」）；
//!   两处按 id 去重——用户目录优先覆盖分发版，sidecar 独有追加，内置仍最优先。
//!
//! M3 缓存与懒加载（见 [`docs/implementation.md`](../../docs/implementation.md) §M3）：
//! - **frozen + 磁盘桩命中** → [`ExtItems::Stub`]：**不拉起进程**（A6），首屏读桩渲染；
//! - **frozen 无桩**（首次运行）→ 照常 spawn 拉取并**落盘**（下次冷启动读桩）；
//! - **fresh**（`frozen=false`）→ spawn 拉取，**不落盘**；
//! - M4 宿主 fallback 轮补充：握手后 `provider.has_fallback == true`（设计文档 §6.3
//!   "含兜底能力者一律视为 fresh"）→ **即使清单标 frozen 也不落盘**，并清除历史桩，
//!   保证进程恒 warm 可响应 `fallback_commands`；
//! - 源状态三态：Warm（进程活）/ Stub（仅桩）/ Failed（失败），供页脚展示与 A6 观察。

use std::collections::HashMap;
use std::path::PathBuf;
use std::thread;

use dd_ext::ExtensionSpec;
use dd_host::builtin::merge_builtins;
use dd_host::cache::{FrozenCache, FrozenSnapshot};
use dd_host::manifest::{self, LoadedExtension, ScanOptions};
use dd_host::process::ExtensionProcess;
use dd_protocol::messages::InitializeResult;
use dd_protocol::model::CommandItem;

use crate::ext_client::ExtClient;
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
    /// 成功拉取后**保活的客户端**（顺序与 [`ExtItems::Ready`] 一一对应，
    /// 供 M2 的 `invoke` 复用；不手动 `close`，随宿主退出由 Drop 清理）。
    /// M9：包含内置 in-process 与第三方子进程两类后端（[`ExtClient`]）。
    pub processes: Vec<ExtClient>,
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

/// 扫描扩展目录并**合并内置扩展**（M4 P4 → M9 in-process）。
///
/// 返回 `(扩展列表, 内置 in-process 规格表, 备注)`：
/// - **扩展列表**：内置 5 个（**恒注册，不依赖磁盘 exe**）+ 扩展目录中的第三方
///   清单（同 id 以内置优先，`merge_builtins` 去重）。列表用于 UI 展示与
///   id/名称查询；内置项的 `command` 为名义路径，**不会被 spawn**。
/// - **规格表**：`id → ExtensionSpec`，仅内置 5 个。宿主据此以 in-process 方式
///   驱动内置扩展（[`crate::ext_client::ExtClient::open_builtin`]）。
/// - **备注**：仅在异常时非空（目录不可读），供 UI 提示。M9 起内置恒可用，
///   不再有"找不到内置 exe"类备注。
pub fn load_extension_sources(
    lang: Lang,
) -> (Vec<LoadedExtension>, HashMap<String, ExtensionSpec>, String) {
    // M9：内置扩展恒注册（in-process，无需 exe / 无需物化内嵌 exe）。
    let builtins = dd_host::builtin::builtin_registrations();
    // 运行期规格（按生效语言构造；宿主已在聚合前经 `dd_ext::i18n::set_lang` 设语言）。
    let specs: HashMap<String, ExtensionSpec> = dd_ext::builtins::builtin_specs()
        .into_iter()
        .map(|s| (s.id.to_string(), s))
        .collect();

    let mut note = String::new();
    let mut merged = merge_builtins(
        builtins,
        merge_sidecar_scan(manifest::extensions_dir(), &mut note),
    );
    // 显示名本地化：清单 `name` 是单串、无 i18n（schema v1.0 冻结），故宿主自有
    // 扩展的名称在注册后统一覆盖（内置取自述 display_name）。
    apply_owned_names(&mut merged, &specs, lang);
    if merged.is_empty() && note.is_empty() {
        note = "无可用扩展（内置与扩展目录均为空）".to_string();
    }
    (merged, specs, note)
}

/// 覆盖**宿主自有扩展**的显示名（本地化）。
///
/// - **内置 5 个**：用扩展自述的 `display_name`（`dd_ext::builtins::builtin_specs()`
///   已按生效语言构造）——宿主不重复维护名称（单一事实来源）。
/// - **随包 sidecar**（非内置，如文件搜索 `com.ddrun.filesearch`）：清单 `name`
///   无 i18n 字段（manifest-schema v1.0 冻结），故对宿主自有 id 用宿主文案表覆盖。
/// - **第三方清单名按作者提供原样**——宿主不臆测翻译。
fn apply_owned_names(
    exts: &mut [LoadedExtension],
    specs: &HashMap<String, ExtensionSpec>,
    lang: Lang,
) {
    for ext in exts {
        if let Some(spec) = specs.get(&ext.manifest.id) {
            ext.manifest.name = spec.display_name.to_string();
        } else if let Some(key) = owned_sidecar_name_key(&ext.manifest.id) {
            ext.manifest.name = text::t(lang, key).to_string();
        }
    }
}

/// 宿主自有 sidecar（随包分发、非内置）的本地化名文案键。
fn owned_sidecar_name_key(id: &str) -> Option<&'static str> {
    match id {
        "com.ddrun.filesearch" => Some("ext.name.filesearch"),
        _ => None,
    }
}

/// 便携 sidecar 扩展目录：宿主 exe 同目录的 `extensions.d/`（M7 批次 7.5）。
///
/// 免安装分发的 zip 布局为 `dd-run-<ver>.exe + extensions.d/`（file-search
/// sidecar 随包携带），解压后与 exe 的相对位置不变——扫描此目录即可
/// 「解压即用」，无需先把清单拷入用户数据目录。
/// 开发期 / 内嵌物化目录下无此子目录：`scan_dir` 对不存在目录视作空（非错误）。
fn sidecar_extensions_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("extensions.d")))
}

/// 合并用户目录与便携 sidecar 两处扫描结果：**同 id 保留 `first`**（用户目录
/// 优先——用户手动放置的版本覆盖分发自带版本），`second` 其余按原序追加。
/// 与内置扩展的去重（内置最优先）由 [`merge_builtins`] 负责。
fn merge_scanned_dirs(
    first: Vec<LoadedExtension>,
    second: Vec<LoadedExtension>,
) -> Vec<LoadedExtension> {
    let mut merged = first;
    for ext in second {
        if !merged.iter().any(|e| e.manifest.id == ext.manifest.id) {
            merged.push(ext);
        }
    }
    merged
}

/// 扫描用户数据目录 `extensions.d` + 便携 sidecar，异常记入 `note`。
fn merge_sidecar_scan(user_dir: Option<PathBuf>, note: &mut String) -> Vec<LoadedExtension> {
    let mut scanned = match user_dir {
        Some(d) => {
            let outcome = manifest::scan_dir(&d, &ScanOptions::default());
            if let Some(err) = &outcome.dir_error {
                push_note(note, &format!("扩展目录不可读：{err}"));
            }
            outcome.loaded
        }
        None => {
            push_note(note, "无法定位扩展目录（home 环境变量缺失）");
            Vec::new()
        }
    };

    // 便携 sidecar（M7 批次 7.5）：同 id 用户目录优先。
    if let Some(dir) = sidecar_extensions_dir() {
        let outcome = manifest::scan_dir(&dir, &ScanOptions::default());
        if let Some(err) = &outcome.dir_error {
            push_note(note, &format!("扩展目录不可读：{err}"));
        }
        scanned = merge_scanned_dirs(scanned, outcome.loaded);
    }
    scanned
}

fn push_note(note: &mut String, msg: &str) {
    if !note.is_empty() {
        note.push('；');
    }
    note.push_str(msg);
}

/// 单个扩展线程的原始结果（携带客户端，跨线程回传）。
enum ExtOutcome {
    Ready {
        proc: Box<ExtClient>,
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

/// 并行收集首屏：每扩展一个线程，客户端对象线程独占，join 回传。
///
/// M3 分流（见模块文档，**仅子进程**）：frozen + 磁盘桩命中 → [`ExtOutcome::Stub`]
/// （不 spawn）；frozen 无桩（首启）→ spawn 拉取并落盘；fresh → spawn 拉取不落盘。
/// M9：内置扩展（`specs` 命中者）走 in-process，**无 spawn / 无桩**（纯函数调用恒瞬时）。
/// `cache` 用 scoped thread 共享只读借用（`FrozenCache` 仅含目录路径，无内部状态）。
pub fn collect_top_level(
    exts: &[LoadedExtension],
    specs: &HashMap<String, ExtensionSpec>,
    cache: Option<&FrozenCache>,
) -> CollectResult {
    let mut processes: Vec<ExtClient> = Vec::new();
    let mut per_ext = Vec::with_capacity(exts.len());
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(exts.len());
        for ext in exts {
            let ext = ext.clone();
            handles.push(scope.spawn(move || load_one(ext, specs, cache)));
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
    // 失败信息一律带**可操作线索**：spawn 失败附被尝试的命令路径（PATH / 路径类
    // 问题一眼可见），握手失败附扩展 stderr 末行（根因通常就在那里）。
    // 2026-09-10 真机反馈驱动：此前只报「spawn 失败：os error 2」，排查只能靠猜。
    let mut spawned = ExtensionProcess::spawn(ext)
        .map_err(|e| format!("spawn 失败：{e}（命令：{}）", ext.command.display()))?;
    let init = match spawned.initialize(PROTOCOL_VERSION, HOST_VERSION) {
        Ok(init) => init,
        Err(e) => {
            return Err(format!(
                "initialize 失败：{e}{}",
                detail_suffix(spawned.failure_detail())
            ))
        }
    };
    Ok((spawned, init))
}

/// 追加诊断后缀：`（诊断：退出码 1；stderr: ...）`；无诊断则空串。
fn detail_suffix(detail: Option<String>) -> String {
    detail.map(|d| format!("（诊断：{d}）")).unwrap_or_default()
}

/// 一个扩展的完整链路：M3 分流（仅子进程）后 open → initialize → top_level_commands（+落盘）。
///
/// M9：内置扩展（`specs` 命中）走 in-process——不 spawn、不读/落磁盘桩（调用为纯
/// 函数，恒"瞬时可用"，`SourceStatus::Warm`），顶层命令拉到即 Ready。
fn load_one(
    ext: LoadedExtension,
    specs: &HashMap<String, ExtensionSpec>,
    cache: Option<&FrozenCache>,
) -> ExtOutcome {
    let id = ext.manifest.id.clone();
    let name = ext.manifest.name.clone();
    let version = ext.manifest.version.clone();
    let in_process = specs.contains_key(&id);

    // M3（仅子进程）：frozen 且磁盘桩命中（键 = id + version，`FrozenCache::load`
    // 已按当前 version 精确定位）→ **不拉起进程**（A6），首屏直接渲染桩。
    if !in_process && ext.manifest.frozen {
        if let Some(snap) = cache.and_then(|c| c.load(&id, &version)) {
            return ExtOutcome::Stub {
                id,
                name,
                items: snap.commands,
            };
        }
    }

    // 无桩（frozen 首启）/ fresh / 内置 in-process：open → initialize → top_level_commands。
    let (mut client, init) = match crate::ext_client::open(specs.get(&id).cloned(), Some(&ext)) {
        Ok(pair) => pair,
        Err(e) => return ExtOutcome::Failed { id, name, error: e },
    };
    // §6.3：含兜底能力者一律视为 fresh——不落桩；若历史桩存在则清除，
    // 避免下次冷启动读桩（无进程 → fallback_commands 拉不到）。
    let has_fallback = init.provider.has_fallback;
    match client.top_level_commands() {
        Ok(items) => {
            // M9：内置 in-process 不参与磁盘桩缓存（无 spawn 成本，读桩反而多一次
            // 文件读取且可能拿到旧文案）；仅子进程走 M3 落桩/清桩。
            if !in_process {
                if ext.manifest.frozen && !has_fallback {
                    // M3：frozen 成功拉取 → 落盘桩（下次冷启动读桩不拉起）。
                    // 先清同 id 的旧版本桩，避免旧文件残留；落盘失败不致命（本次仍 warm
                    // 服务，仅下次冷启动退化为再拉一次）。
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
            }
            ExtOutcome::Ready {
                proc: Box::new(client),
                id,
                name,
                items,
            }
        }
        Err(e) => ExtOutcome::Failed {
            id,
            name,
            error: format!(
                "top_level_commands 失败：{e}{}",
                detail_suffix(client.failure_detail())
            ),
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
        // v3.3 P1：`more_commands` 透传（§8.1 上下文菜单动作，右键菜单渲染 +
        // `sender=context_menu` 回调 invoke）
        more_commands: cmd.more_commands.clone().unwrap_or_default(),
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
    fn to_panel_item_carries_more_commands() {
        // v3.3 P1：§8.1 more_commands 透传进 PanelItem（右键菜单渲染源）
        let mut c = cmd("files.open.1", "报告.txt", Some("文件"));
        c.more_commands = Some(vec![
            cmd("files.reveal.1", "显示所在目录", None),
            cmd("files.copy.1", "复制路径", None),
        ]);
        let panel = to_panel_item(&c, "com.ddrun.filesearch", "文件", Lang::ZhCn);
        assert_eq!(panel.more_commands.len(), 2);
        assert_eq!(panel.more_commands[0].id, "files.reveal.1");
        assert_eq!(panel.more_commands[1].id, "files.copy.1");
        // None → 空表（无 more_commands 的项菜单走静态映射）
        let plain = to_panel_item(&cmd("a", "b", None), "com.ddrun.calc", "", Lang::ZhCn);
        assert!(plain.more_commands.is_empty());
    }

    /// 最小可加载扩展夹具：`path` 携带 `tag` 以区分「用户目录 / sidecar」来源。
    fn loaded_ext(id: &str, tag: &str) -> LoadedExtension {
        use dd_host::manifest::{Entry, Manifest};
        LoadedExtension {
            manifest: Manifest {
                schema_version: "1.0".to_string(),
                id: id.to_string(),
                name: format!("Ext {id}"),
                version: "0.1.0".to_string(),
                description: String::new(),
                author: String::new(),
                license: String::new(),
                homepage: String::new(),
                icon: None,
                entry: Entry {
                    command: "ext.exe".to_string(),
                    args: Vec::new(),
                    env: Default::default(),
                    cwd: None,
                },
                frozen: true,
                capabilities: Vec::new(),
                platforms: None,
                min_host_version: None,
            },
            path: PathBuf::from(format!(r"{tag}\{id}.json")),
            dir: PathBuf::from(tag),
            command: PathBuf::from(format!(r"{tag}\ext.exe")),
            cwd: PathBuf::from(tag),
        }
    }

    /// M9 修复：宿主自有扩展的显示名被本地化覆盖——**内置**取扩展自述
    /// `display_name`（不依赖具体语言）；**随包 sidecar** 取宿主文案键；
    /// **第三方**保持清单原样（宿主不臆造翻译）。
    #[test]
    fn apply_owned_names_localizes_only_owned_extensions() {
        let specs: HashMap<String, dd_ext::ExtensionSpec> = dd_ext::builtins::builtin_specs()
            .into_iter()
            .map(|s| (s.id.to_string(), s))
            .collect();
        let mut exts = vec![
            loaded_ext("com.ddrun.calc", "builtin"),
            loaded_ext("com.ddrun.filesearch", "sidecar"),
            loaded_ext("com.example.thirdparty", "user"),
        ];
        apply_owned_names(&mut exts, &specs, Lang::ZhCn);

        // 内置：覆盖为规格 display_name（与语言无关的等价断言）
        let calc_name = specs.get("com.ddrun.calc").unwrap().display_name;
        assert_eq!(exts[0].manifest.name, calc_name);
        assert_ne!(
            exts[0].manifest.name, "Ext com.ddrun.calc",
            "内置名应被自述 display_name 覆盖"
        );
        // 随包 sidecar：宿主文案键（中文模式）
        assert_eq!(exts[1].manifest.name, "文件搜索");
        // 第三方：清单名原样
        assert_eq!(exts[2].manifest.name, "Ext com.example.thirdparty");
        // 英文模式（sidecar 名随语言切换）
        apply_owned_names(&mut exts, &specs, Lang::EnUs);
        assert_eq!(exts[1].manifest.name, "File Search");
    }

    /// 宿主自有 sidecar 的本地化名映射只覆盖随包扩展。
    #[test]
    fn owned_sidecar_name_key_maps_bundled_only() {
        assert_eq!(
            owned_sidecar_name_key("com.ddrun.filesearch"),
            Some("ext.name.filesearch")
        );
        assert_eq!(owned_sidecar_name_key("com.example.x"), None);
    }

    /// M9 B3：内置扩展（spec 命中）走 **in-process** —— 聚合产出 Ready、且保活集
    /// 中必须是 in-process 后端（**不 spawn 子进程**，与 D3 验收"代码路径验证"对齐）；
    /// 同时无磁盘桩（即使 `frozen=true` 也不读桩）。
    #[test]
    fn collect_top_level_builtin_uses_in_process_backend() {
        use dd_ext::ExtensionSpec;
        use dd_protocol::model::CommandRef;

        let spec = ExtensionSpec {
            id: "com.ddrun.fixture",
            display_name: "Fixture",
            description: "单测夹具",
            frozen: true,
            has_fallback: false,
            capabilities: &[],
            log_tag: "dd-ext-fixture",
            top_level: || {
                vec![CommandItem {
                    id: "fix.hello".into(),
                    title: "Hello".into(),
                    subtitle: None,
                    icon: None,
                    section: None,
                    tags: None,
                    details: None,
                    text_to_suggest: None,
                    more_commands: None,
                    command: CommandRef::Invoke,
                }]
            },
            fallback: None,
            invoke: |_| (dd_protocol::model::CommandResult::Dismiss, Vec::new()),
            pages: None,
        };
        let ext = dd_host::manifest::from_builtin(
            PathBuf::from("dd-ext-fixture.exe"), // 名义路径：不存在也无妨（不 spawn）
            "com.ddrun.fixture",
            "Fixture",
            true,
            &[],
            "0.1.1",
        );
        let mut specs = HashMap::new();
        specs.insert("com.ddrun.fixture".to_string(), spec);

        let result = collect_top_level(std::slice::from_ref(&ext), &specs, None);

        assert_eq!(result.per_ext.len(), 1);
        assert!(
            result.per_ext[0].is_ready(),
            "内置 in-process 应直接 Ready（无 spawn / 无桩），实际 {:?}",
            result.per_ext[0]
        );
        assert_eq!(result.processes.len(), 1);
        assert!(
            result.processes[0].is_in_process(),
            "内置扩展必须走 in-process 后端（不再 spawn 子进程）"
        );
    }

    /// 对照：`specs` 未命中（第三方/sidecar）仍走子进程——exe 不存在 → Failed
    /// （不 panic、不误判为 Ready）。
    #[test]
    fn collect_top_level_non_builtin_still_uses_subprocess() {
        let ext = loaded_ext("com.example.third", "user");
        let specs: HashMap<String, ExtensionSpec> = HashMap::new();

        let result = collect_top_level(std::slice::from_ref(&ext), &specs, None);

        assert_eq!(result.per_ext.len(), 1);
        assert!(
            matches!(result.per_ext[0], ExtItems::Failed { .. }),
            "非内置、exe 不存在 → spawn 失败 → Failed，实际 {:?}",
            result.per_ext[0]
        );
        assert!(result.processes.is_empty(), "失败项无保活客户端");
    }

    #[test]
    fn merge_scanned_dirs_user_dir_wins_and_sidecar_appends() {
        // M7 批次 7.5：便携 sidecar 合并语义——同 id 用户目录优先（覆盖分发版）、
        // sidecar 独有扩展按原序追加；first 顺序保持。
        let user = vec![loaded_ext("com.a", "user"), loaded_ext("com.b", "user")];
        let sidecar = vec![
            loaded_ext("com.b", "sidecar"),
            loaded_ext("com.c", "sidecar"),
        ];
        let merged = merge_scanned_dirs(user, sidecar);
        let ids: Vec<&str> = merged.iter().map(|e| e.manifest.id.as_str()).collect();
        assert_eq!(
            ids,
            ["com.a", "com.b", "com.c"],
            "同 id 去重 + sidecar 新增按原序追加"
        );
        // com.b 保留的是用户目录那份（path 前缀区分来源）
        assert_eq!(
            merged[1].path,
            PathBuf::from(r"user\com.b.json"),
            "同 id 应保留用户目录版本（覆盖分发版）"
        );
    }

    #[test]
    fn merge_scanned_dirs_empty_sidecar_keeps_user_order() {
        // sidecar 无此扩展（zip 未带 / 目录不存在）→ 用户目录原样保留。
        let user = vec![loaded_ext("com.a", "user"), loaded_ext("com.b", "user")];
        let merged = merge_scanned_dirs(user, Vec::new());
        let ids: Vec<&str> = merged.iter().map(|e| e.manifest.id.as_str()).collect();
        assert_eq!(ids, ["com.a", "com.b"]);
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

    /// 诊断后缀：有诊断才加括号，无诊断不留空壳（失败文案不得出现「（诊断：）」）。
    #[test]
    fn detail_suffix_only_when_present() {
        assert_eq!(detail_suffix(None), "");
        assert_eq!(
            detail_suffix(Some("退出码 1；stderr: boom".to_string())),
            "（诊断：退出码 1；stderr: boom）"
        );
    }
}
