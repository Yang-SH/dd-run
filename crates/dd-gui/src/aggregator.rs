//! 首屏聚合层：扫描扩展 → 并行 `spawn → initialize → top_level_commands`
//! → 合并为可渲染的 [`PanelItem`] 列表。
//!
//! 对齐 [`docs/protocol.md`](../../docs/protocol.md) §5（握手）/ §6.1（顶层命令）
//! 与 [`docs/implementation.md`](../../docs/implementation.md) M1「首屏聚合」任务：
//! - **并行**：每扩展一个线程（进程对象线程独占），互不阻塞（A12 能力调用不阻塞 UI）；
//! - **错误隔离**：单个扩展失败只记入 [`SourceSummary`]，不影响其他扩展与整体渲染；
//! - **兜底**：扩展目录无清单时，回退到与 `dd-gui` 同目录的 `dd-ext-sample.exe`
//!   （M0 `--roundtrip` 同款思路，保证无扩展环境的开发验收路径）。

use std::path::PathBuf;
use std::thread;

use dd_host::manifest::{self, LoadedExtension, ScanOptions};
use dd_host::process::ExtensionProcess;
use dd_protocol::model::CommandItem;

use crate::state::PanelItem;

/// 协议版本（protocol.md §13：`MAJOR.MINOR` 两段）。
pub const PROTOCOL_VERSION: &str = "1.0";
/// 宿主版本（semver，`initialize` 的 `host.version`）。
pub const HOST_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 单个扩展的拉取结果（**不携带进程**，便于纯逻辑单测构造）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtItems {
    Ready {
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

/// 单个扩展源的展示状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceStatus {
    Ready { commands: usize },
    Failed { error: String },
}

impl SourceStatus {
    /// 是否处于失败态。
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

impl ExtItems {
    /// 扩展清单 id（`Ready` / `Failed` 均有，供进程配对与诊断）。
    pub fn id(&self) -> &str {
        match self {
            ExtItems::Ready { id, .. } | ExtItems::Failed { id, .. } => id,
        }
    }

    /// 是否成功拉取。
    pub fn is_ready(&self) -> bool {
        matches!(self, ExtItems::Ready { .. })
    }
}

/// 扫描扩展目录；**目录无扩展时**用与 `dd-gui` 同目录的示例扩展兜底。
///
/// 返回 `(扩展列表, 备注)`。备注说明扩展来源（真实扫描 / 兜底 / 均不可用），
/// 供 UI 提示，避免把「内置兜底」误读为「扫描发现」。
pub fn load_extension_sources() -> (Vec<LoadedExtension>, String) {
    let dir = manifest::extensions_dir();
    match &dir {
        Some(d) => {
            let outcome = manifest::scan_dir(d, &ScanOptions::default());
            if let Some(err) = &outcome.dir_error {
                return fallback(d, format!("未找到扩展目录：{err}"));
            }
            if !outcome.loaded.is_empty() {
                return (outcome.loaded, String::new());
            }
            // 目录存在但无清单 → 兜底示例扩展
            fallback(d, "扩展目录为空，未发现清单".to_string())
        }
        None => {
            let note = "无法定位扩展目录（home 环境变量缺失）".to_string();
            match find_sample_executable() {
                Some(sample) => {
                    let ext = manifest::from_executable(sample, "dev.sample-ext", "Sample Ext");
                    (vec![ext], format!("{note}；已回退内置示例扩展"))
                }
                None => (Vec::new(), format!("{note}，且未找到示例扩展")),
            }
        }
    }
}

/// 目录不可用/为空时回退到示例扩展。
fn fallback(dir: &std::path::Path, reason: String) -> (Vec<LoadedExtension>, String) {
    match find_sample_executable() {
        Some(sample) => {
            let ext = manifest::from_executable(sample, "dev.sample-ext", "Sample Ext");
            (
                vec![ext],
                format!("{reason}（{}）；已回退内置示例扩展", dir.display()),
            )
        }
        None => (Vec::new(), format!("{reason}，且未找到示例扩展可执行文件")),
    }
}

/// 与 `dd-gui` 同目录的 `dd-ext-sample.exe`（cargo 会把 workspace 内各 crate
/// 的可执行文件都输出到同一 `target/<profile>/` 目录）。
fn find_sample_executable() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let candidate = exe_dir.join(if cfg!(windows) {
        "dd-ext-sample.exe"
    } else {
        "dd-ext-sample"
    });
    candidate.is_file().then_some(candidate)
}

/// 单个扩展线程的原始结果（携带进程，跨线程回传）。
enum ExtOutcome {
    Ready {
        proc: Box<ExtensionProcess>,
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

/// 并行拉取：每扩展一个线程，进程对象线程独占，join 回传。
pub fn collect_top_level(exts: &[LoadedExtension]) -> CollectResult {
    let mut handles = Vec::with_capacity(exts.len());
    for ext in exts {
        let ext = ext.clone();
        handles.push(thread::spawn(move || load_one(ext)));
    }

    let mut processes = Vec::new();
    let mut per_ext = Vec::with_capacity(handles.len());
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
    CollectResult { processes, per_ext }
}

/// 一个扩展的完整链路：spawn → initialize（握手+版本协商）→ top_level_commands。
fn load_one(ext: LoadedExtension) -> ExtOutcome {
    let id = ext.manifest.id.clone();
    let name = ext.manifest.name.clone();
    let mut spawned = match ExtensionProcess::spawn(&ext) {
        Ok(p) => p,
        Err(e) => {
            return ExtOutcome::Failed {
                id,
                name,
                error: format!("spawn 失败：{e}"),
            }
        }
    };
    match spawned.initialize(PROTOCOL_VERSION, HOST_VERSION) {
        Ok(_) => {}
        Err(e) => {
            return ExtOutcome::Failed {
                id,
                name,
                error: format!("initialize 失败：{e}"),
            }
        }
    }
    match spawned.top_level_commands() {
        Ok(items) => ExtOutcome::Ready {
            proc: Box::new(spawned),
            id,
            name,
            items,
        },
        Err(e) => ExtOutcome::Failed {
            id,
            name,
            error: format!("top_level_commands 失败：{e}"),
        },
    }
}

/// 把各扩展结果合并为可渲染列表 + 源状态汇总（纯函数，可单测）。
pub fn flatten(per_ext: &[ExtItems]) -> (Vec<PanelItem>, Vec<SourceSummary>) {
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
                    status: SourceStatus::Ready {
                        commands: cmds.len(),
                    },
                });
                for cmd in cmds {
                    items.push(to_panel_item(cmd, id, name));
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
pub fn to_panel_item(cmd: &CommandItem, ext_id: &str, fallback_section: &str) -> PanelItem {
    PanelItem {
        id: cmd.id.clone(),
        ext_id: ext_id.to_string(),
        title: cmd.title.clone(),
        subtitle: cmd.subtitle.clone().unwrap_or_default(),
        section: cmd
            .section
            .clone()
            .unwrap_or_else(|| fallback_section.to_string()),
        tags: cmd.tags.clone().unwrap_or_default(),
        command: cmd.command.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_protocol::model::CommandRef;

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
        let panel = to_panel_item(&item, "com.example.a", "ExtA");
        assert_eq!(panel.id, "a.1", "id 透传");
        assert_eq!(panel.ext_id, "com.example.a", "来源扩展 id 透传");
        assert_eq!(panel.title, "Hello");
        assert_eq!(panel.section, "ExtA", "section 缺省用扩展名兜底");
        assert_eq!(panel.command, CommandRef::Invoke, "command 透传");

        let with_section = cmd("a.2", "Bye", Some("系统"));
        let panel = to_panel_item(&with_section, "com.example.a", "ExtA");
        assert_eq!(panel.section, "系统", "扩展返回的 section 优先");
    }

    #[test]
    fn flatten_merges_ready_and_keeps_failed_isolated() {
        let per_ext = vec![
            ready(
                "com.example.a",
                "Ext A",
                vec![cmd("a.1", "A1", None), cmd("a.2", "A2", Some("系统"))],
            ),
            ExtItems::Failed {
                id: "com.example.b".to_string(),
                name: "Ext B".to_string(),
                error: "initialize 失败：超时".to_string(),
            },
            ready("com.example.c", "Ext C", vec![]),
        ];

        let (items, sources) = flatten(&per_ext);

        // 合并 2 条 + 兜底 section；B 的失败不影响 A/C
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].section, "Ext A");
        assert_eq!(items[0].ext_id, "com.example.a", "项带来源扩展 id");
        assert_eq!(items[1].section, "系统");

        assert_eq!(sources.len(), 3);
        assert_eq!(sources[0].status, SourceStatus::Ready { commands: 2 });
        assert_eq!(
            sources[1].status,
            SourceStatus::Failed {
                error: "initialize 失败：超时".to_string()
            }
        );
        assert!(sources[1].status.is_failed());
        assert_eq!(sources[2].status, SourceStatus::Ready { commands: 0 });
    }

    #[test]
    fn flatten_empty_input() {
        let (items, sources) = flatten(&[]);
        assert!(items.is_empty());
        assert!(sources.is_empty());
    }
}
