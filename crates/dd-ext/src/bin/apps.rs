//! dd-ext-apps —— 内置「应用启动」扩展（`com.ddrun.apps`，⚙️ 平台相关）。
//!
//! 功能（M4 A10 核对点：Apps 枚举真实应用）：
//! - 顶层命令 = 枚举的本地应用列表（进程内 `OnceLock` 缓存一次）；
//! - **Windows**：`%APPDATA%` / `%ProgramData%` 的「开始菜单\Programs」递归 `*.lnk`
//!   + `PATH` 各目录根层 `*.exe`（去重、上限防爆表）；
//! - invoke：`.lnk` → `cmd /c start "" <lnk>`；`.exe` → 直接 spawn → `Dismiss`。
//!
//! `frozen=false`：应用列表随安装/卸载变化，属 fresh——宿主不落 frozen 桩、
//! 每次冷启动 warm 拉取（与 [`docs/m4-record.md`](../../docs/m4-record.md) P4 语义一致）。
//!
//! 平台策略（P4 决策：Windows 优先）：macOS（`/Applications`）与 Linux
//! （`.desktop` + PATH）为**编译恒成立占位**，待对应平台轮实现。
//! 参考实现：[`docs/m4-record.md`](../../docs/m4-record.md) P4 决策。

use dd_ext::{run, Effect, ExtensionSpec};
use dd_protocol::messages::InvokeParams;
use dd_protocol::model::{CommandItem, CommandRef, CommandResult, Icon, IconKind};

/// 结果列表上限：避免 PATH 全量扫描产生超大首屏（截断并记日志）。
const MAX_APPS: usize = 400;

fn main() {
    run(&spec());
}

fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: "com.ddrun.apps",
        display_name: "Apps",
        description: "枚举并启动本地应用（开始菜单 + PATH）",
        // 应用列表随安装/卸载变化 → fresh（不落 frozen 桩，见模块文档）
        frozen: false,
        has_fallback: false,
        capabilities: &[],
        log_tag: "dd-ext-apps",
        top_level: sys::top_level_commands,
        fallback: None,
        invoke: sys::handle_invoke,
    }
}

/// 平台枚举与启动（按 OS 分实现）。
#[cfg(windows)]
mod sys {
    use super::*;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    /// 一条应用项：命令 id 后缀（`apps.run.<n>`）+ 原始启动描述。
    struct App {
        /// 显示名（不含 .lnk/.exe 后缀）
        title: String,
        /// 是否为快捷方式（需 `cmd /c start` 间接启动）
        is_shortcut: bool,
        /// 绝对路径
        path: PathBuf,
    }

    static APP_CACHE: OnceLock<Vec<App>> = OnceLock::new();

    fn app_list() -> &'static Vec<App> {
        APP_CACHE.get_or_init(|| {
            let mut seen: HashSet<String> = HashSet::new();
            let mut apps: Vec<App> = Vec::new();

            // ① 开始菜单 .lnk（两个根，递归）
            for root in start_menu_roots() {
                let mut stack = vec![root];
                while let Some(dir) = stack.pop() {
                    let Ok(entries) = std::fs::read_dir(&dir) else {
                        continue;
                    };
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            stack.push(path);
                        } else if path.extension().and_then(|s| s.to_str()) == Some("lnk") {
                            let Some(title) = file_stem(&path) else {
                                continue;
                            };
                            if seen.insert(title.to_lowercase()) {
                                apps.push(App {
                                    title,
                                    is_shortcut: true,
                                    path,
                                });
                            }
                        }
                    }
                }
            }

            // ② PATH 各目录根层 *.exe（去重：与 .lnk 同显示名者跳过）
            for dir in path_dirs() {
                if apps.len() >= MAX_APPS {
                    break;
                }
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    if apps.len() >= MAX_APPS {
                        break;
                    }
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("exe") {
                        let Some(title) = file_stem(&path) else {
                            continue;
                        };
                        if seen.insert(title.to_lowercase()) {
                            apps.push(App {
                                title,
                                is_shortcut: false,
                                path,
                            });
                        }
                    }
                }
            }

            // 按显示名排序（列表稳定、可预测）
            apps.sort_by_key(|a| a.title.to_lowercase());
            eprintln!(
                "[dd-ext-apps] 枚举到 {} 个应用{}",
                apps.len(),
                if apps.len() >= MAX_APPS {
                    format!("（达到上限 {MAX_APPS}，已截断）")
                } else {
                    String::new()
                }
            );
            apps
        })
    }

    fn file_stem(path: &Path) -> Option<String> {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
    }

    /// 开始菜单两个根（用户 + 公共）。
    fn start_menu_roots() -> Vec<PathBuf> {
        let mut roots = Vec::new();
        for (env, default) in [
            ("APPDATA", r"%USERPROFILE%\AppData\Roaming"),
            ("PROGRAMDATA", r"C:\ProgramData"),
        ] {
            let base = std::env::var_os(env)
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("USERPROFILE").map(|_| PathBuf::from(default)))
                .unwrap_or_default();
            roots.push(
                base.join("Microsoft")
                    .join("Windows")
                    .join("Start Menu")
                    .join("Programs"),
            );
        }
        roots
    }

    /// PATH 拆分出的目录列表。
    fn path_dirs() -> Vec<PathBuf> {
        std::env::var_os("PATH")
            .map(|p| {
                std::env::split_paths(&p)
                    .filter(|d| d.is_dir())
                    .collect::<Vec<PathBuf>>()
            })
            .unwrap_or_default()
    }

    pub fn top_level_commands() -> Vec<CommandItem> {
        app_list()
            .iter()
            .enumerate()
            .map(|(i, app)| CommandItem {
                id: format!("apps.run.{i}"),
                title: app.title.clone(),
                subtitle: Some(if app.is_shortcut {
                    "快捷方式".to_string()
                } else {
                    app.path.display().to_string()
                }),
                icon: Some(Icon {
                    kind: IconKind::Glyph,
                    value: "\u{E7C4}".to_string(), // Apps
                }),
                section: Some("应用".to_string()),
                tags: None,
                details: None,
                text_to_suggest: None,
                more_commands: None,
                command: CommandRef::Invoke,
            })
            .collect()
    }

    pub fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
        let idx: usize = params
            .id
            .strip_prefix("apps.run.")
            .and_then(|s| s.parse().ok())
            .unwrap_or(usize::MAX);
        let Some(app) = app_list().get(idx) else {
            return (
                CommandResult::ShowToast {
                    message: format!("应用不存在或列表已变化：{}", params.id),
                    duration_ms: Some(2_500),
                },
                Vec::new(),
            );
        };

        let spawned = if app.is_shortcut {
            launch_shortcut(&app.path)
        } else {
            launch_executable(&app.path)
        };
        match spawned {
            Ok(()) => (CommandResult::Dismiss, Vec::new()),
            Err(e) => (
                CommandResult::ShowToast {
                    message: format!("启动 {} 失败：{e}", app.title),
                    duration_ms: Some(3_000),
                },
                Vec::new(),
            ),
        }
    }

    /// 启动 .lnk：`cmd /c start "" "<lnk>"`（CreateProcess 不解析 .lnk，start 负责）。
    fn launch_shortcut(path: &Path) -> Result<(), String> {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("cmd.exe");
        cmd.args(["/C", "start", "", &path.to_string_lossy()]);
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW：隐藏 cmd 自身窗口
        cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
    }

    /// 启动 .exe：直接 spawn（继承宿主无 console 环境，GUI 程序正常）。
    fn launch_executable(path: &Path) -> Result<(), String> {
        let mut cmd = std::process::Command::new(path);
        cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use dd_protocol::model::Sender;

        #[test]
        fn windows_app_list_is_non_empty_and_unique() {
            // 真机/CI 的 Windows 环境必有开始菜单与 PATH exe；列表非空且标题唯一（小写不重复）
            let apps = app_list();
            assert!(!apps.is_empty(), "Windows 上应至少枚举到一个应用");
            let mut lower: Vec<String> = apps.iter().map(|a| a.title.to_lowercase()).collect();
            lower.sort();
            lower.dedup();
            assert_eq!(lower.len(), apps.len(), "应用标题不应重复");
            assert!(apps.len() <= MAX_APPS);
        }

        #[test]
        fn top_level_ids_map_back_into_list() {
            let cmds = top_level_commands();
            assert!(!cmds.is_empty());
            // id 为 apps.run.<index>，index 必须能回查 app_list()
            let last = cmds.last().unwrap().id.clone();
            let idx: usize = last.strip_prefix("apps.run.").unwrap().parse().unwrap();
            assert!(app_list().get(idx).is_some());
        }

        #[test]
        fn start_menu_roots_point_under_windows_dirs() {
            // 只验证目录构造（不触碰文件系统副作用）：两个根都应是 …\Start Menu\Programs
            let roots = start_menu_roots();
            assert_eq!(roots.len(), 2);
            for r in &roots {
                assert!(
                    r.to_string_lossy().contains("Start Menu")
                        && r.to_string_lossy().ends_with("Programs"),
                    "got {}",
                    r.display()
                );
            }
        }

        #[test]
        fn invoke_unknown_index_toasts() {
            let p = InvokeParams {
                id: "apps.run.999999".into(),
                sender: Sender::TopLevel,
                context: None,
            };
            let (result, _) = handle_invoke(&p);
            assert!(matches!(result, CommandResult::ShowToast { .. }));
        }
    }
}

/// 非 Windows 占位（P4 Windows 优先）：编译恒成立，功能待对应平台轮实现。
#[cfg(not(windows))]
mod sys {
    use super::*;
    use dd_ext::Effect;

    pub fn top_level_commands() -> Vec<CommandItem> {
        // macOS 扫描 /Applications；Linux 读 .desktop + PATH：TODO 对应平台轮
        Vec::new()
    }

    pub fn handle_invoke(_params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
        (
            CommandResult::ShowToast {
                message: "应用枚举：当前平台尚未实现（P4 Windows 优先）".to_string(),
                duration_ms: Some(2_500),
            },
            Vec::new(),
        )
    }
}
