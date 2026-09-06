//! dd-ext-system —— 内置「系统」扩展（`com.ddrun.system`，⚙️ 平台相关）。
//!
//! 功能（M4 A10 核对点：System 锁屏等系统命令）：
//! - 顶层固定命令（frozen）：锁屏 / 睡眠 / 关机 / 重启 / 注销；
//! - 关机 / 重启 / 注销为**危险操作** → invoke 先回 `Confirm{is_critical:true}`，
//!   宿主带 `context.confirmed=true` 重发后才真正执行（§8.3）；
//! - 执行成功 → `Dismiss`（关闭面板）；spawn 失败 → `ShowToast` 报错。
//!
//! 平台策略（P4 决策：Windows 优先）：Windows 实现 `rundll32` / `shutdown`
//! 真实命令（本机真机验证）；macOS / Linux 分支为**编译恒成立占位**
//! （顶层命令空 + invoke 提示未实现），待相应平台轮实现
//! （macOS `pmset`/`osascript`；Linux `systemctl`，见设计文档 §7 平台列）。
//! 参考实现：[`docs/m4-record.md`](../../docs/m4-record.md) P4 决策。

use dd_ext::{i18n::tr, run, ExtensionSpec};
use dd_protocol::messages::InvokeParams;
use dd_protocol::model::{CommandItem, CommandRef, CommandResult, Icon, IconKind};

fn main() {
    run(&spec());
}

fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: "com.ddrun.system",
        display_name: tr("系统", "System"),
        description: tr(
            "锁屏 / 睡眠 / 关机 / 重启 / 注销",
            "Lock / Sleep / Shut down / Restart / Sign out",
        ),
        frozen: true,
        has_fallback: false,
        capabilities: &[],
        log_tag: "dd-ext-system",
        top_level: sys::top_level_commands,
        fallback: None,
        invoke: sys::handle_invoke,
    }
}

/// 平台命令表与执行逻辑（按 OS 分实现）。
#[cfg(windows)]
mod sys {
    use super::*;
    use dd_ext::Effect;

    /// 一条系统命令：id / 标题 / 副标题 / 确认描述 / 是否危险 / 程序 / 参数。
    pub struct SystemCommand {
        pub id: &'static str,
        pub title: &'static str,
        pub subtitle: &'static str,
        /// Confirm 对话框描述（仅危险项使用）
        pub confirm_description: &'static str,
        /// 危险操作（关机/重启/注销）需 `Confirm` 二次确认
        pub dangerous: bool,
        pub program: &'static str,
        pub args: &'static [&'static str],
    }

    /// 系统命令表（批次 D：文案经 `tr` 按生效语言选 zh/en）。用函数 + `OnceLock`
    /// 而非 `const`，因 `tr` 是运行时函数调用、`const` 上下文不允许。
    pub fn commands() -> &'static [SystemCommand] {
        static CELL: std::sync::OnceLock<Vec<SystemCommand>> = std::sync::OnceLock::new();
        CELL.get_or_init(|| {
            vec![
                SystemCommand {
                    id: "system.lock",
                    title: tr("锁屏", "Lock Screen"),
                    subtitle: tr(
                        "立即锁定工作站（rundll32 LockWorkStation）",
                        "Lock the workstation immediately (rundll32 LockWorkStation)",
                    ),
                    confirm_description: "",
                    dangerous: false,
                    program: "rundll32.exe",
                    args: &["user32.dll,LockWorkStation"],
                },
                SystemCommand {
                    id: "system.sleep",
                    title: tr("睡眠", "Sleep"),
                    subtitle: tr(
                        "使计算机进入睡眠（可唤醒）",
                        "Put the computer to sleep (wakeable)",
                    ),
                    confirm_description: "",
                    dangerous: false,
                    program: "rundll32.exe",
                    args: &["powrprof.dll,SetSuspendState", "0,1,0"],
                },
                SystemCommand {
                    id: "system.shutdown",
                    title: tr("关机", "Shut Down"),
                    subtitle: tr(
                        "关闭计算机（⚠ 危险，需二次确认）",
                        "Shut down the computer (⚠ dangerous, needs confirmation)",
                    ),
                    confirm_description: tr(
                        "计算机会立即关闭，未保存的工作将丢失。",
                        "The computer will shut down immediately; unsaved work will be lost.",
                    ),
                    dangerous: true,
                    program: "shutdown.exe",
                    args: &["/s", "/t", "0"],
                },
                SystemCommand {
                    id: "system.restart",
                    title: tr("重启", "Restart"),
                    subtitle: tr(
                        "重启计算机（⚠ 危险，需二次确认）",
                        "Restart the computer (⚠ dangerous, needs confirmation)",
                    ),
                    confirm_description: tr(
                        "计算机会立即重启，未保存的工作将丢失。",
                        "The computer will restart immediately; unsaved work will be lost.",
                    ),
                    dangerous: true,
                    program: "shutdown.exe",
                    args: &["/r", "/t", "0"],
                },
                SystemCommand {
                    id: "system.logoff",
                    title: tr("注销", "Sign Out"),
                    subtitle: tr(
                        "注销当前用户（⚠ 危险，需二次确认）",
                        "Sign out the current user (⚠ dangerous, needs confirmation)",
                    ),
                    confirm_description: tr(
                        "当前会话将结束，未保存的工作可能丢失。",
                        "The current session will end; unsaved work may be lost.",
                    ),
                    dangerous: true,
                    program: "shutdown.exe",
                    args: &["/l"],
                },
            ]
        })
        .as_slice()
    }

    pub fn top_level_commands() -> Vec<CommandItem> {
        commands()
            .iter()
            .map(|c| CommandItem {
                id: c.id.to_string(),
                title: c.title.to_string(),
                subtitle: Some(c.subtitle.to_string()),
                icon: Some(Icon {
                    kind: IconKind::Glyph,
                    value: "\u{E7E8}".to_string(), // PowerButton
                }),
                section: Some(tr("系统", "System").to_string()),
                tags: Some(if c.dangerous {
                    vec!["system".to_string(), "danger".to_string()]
                } else {
                    vec!["system".to_string()]
                }),
                details: None,
                text_to_suggest: None,
                more_commands: None,
                command: CommandRef::Invoke,
            })
            .collect()
    }

    /// 纯决策层（无副作用，可安全单测）：
    /// 未知 id → `Err(ShowToast)`；危险命令未经确认 → `Err(Confirm{is_critical:true})`；
    /// 否则 `Ok(cmd)` 交由 [`launch`] 真正执行（launch 只在真机轮 A10 人工验证）。
    fn decide(params: &InvokeParams) -> Result<&'static SystemCommand, CommandResult> {
        let confirmed = params
            .context
            .as_ref()
            .and_then(|c| c.confirmed)
            .unwrap_or(false);
        let Some(cmd) = commands().iter().find(|c| c.id == params.id) else {
            return Err(CommandResult::ShowToast {
                message: tr("未知系统命令：{}", "Unknown system command: {}")
                    .replace("{}", &params.id),
                duration_ms: Some(2_500),
            });
        };
        // §8.3：确认后宿主带 confirmed=true 重发 invoke
        if cmd.dangerous && !confirmed {
            return Err(CommandResult::Confirm {
                title: tr("确认{}？", "Confirm {}?").replace("{}", cmd.title),
                description: tr(
                    "{}该操作无法撤销，确认要执行吗？",
                    "{} This action cannot be undone. Continue?",
                )
                .replace("{}", cmd.confirm_description),
                confirm_label: tr("执行{}", "Execute {}").replace("{}", cmd.title),
                is_critical: true,
            });
        }
        Ok(cmd)
    }

    pub fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
        let cmd = match decide(params) {
            Ok(cmd) => cmd,
            Err(result) => return (result, Vec::new()),
        };
        match launch(cmd.program, cmd.args) {
            Ok(()) => (CommandResult::Dismiss, Vec::new()),
            Err(e) => (
                CommandResult::ShowToast {
                    message: tr("执行 {} 失败：{e}", "Failed to run {}: {e}")
                        .replace("{e}", &e.to_string())
                        .replace("{}", cmd.program),
                    duration_ms: Some(3_000),
                },
                Vec::new(),
            ),
        }
    }

    /// 启动系统命令（隐藏窗口，避免控制台闪现）。
    fn launch(program: &str, args: &[&str]) -> Result<(), String> {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new(program);
        cmd.args(args);
        // CREATE_NO_WINDOW = 0x08000000
        cmd.creation_flags(0x0800_0000);
        cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn catalog_has_five_known_commands() {
            assert_eq!(commands().len(), 5);
            let ids: Vec<&str> = commands().iter().map(|c| c.id).collect();
            assert_eq!(
                ids,
                vec![
                    "system.lock",
                    "system.sleep",
                    "system.shutdown",
                    "system.restart",
                    "system.logoff"
                ]
            );
            // 危险项恰好三项
            assert_eq!(commands().iter().filter(|c| c.dangerous).count(), 3);
        }

        #[test]
        fn dangerous_command_first_invoke_confirms() {
            // 决策层纯函数单测：不触发真实系统副作用
            let p = dd_protocol::messages::InvokeParams {
                id: "system.shutdown".into(),
                sender: dd_protocol::model::Sender::TopLevel,
                context: None,
            };
            let result = decide(&p);
            assert!(matches!(
                result,
                Err(CommandResult::Confirm {
                    is_critical: true,
                    ..
                })
            ));
        }

        #[test]
        fn dangerous_command_confirmed_then_passes_gate() {
            // confirmed=true → 放行到执行层（仅断言 decide 返回 Ok，不真执行）
            let ctx = dd_protocol::messages::InvokeContext {
                query: None,
                selected_item_id: None,
                form_data: None,
                confirmed: Some(true),
            };
            let p = dd_protocol::messages::InvokeParams {
                id: "system.shutdown".into(),
                sender: dd_protocol::model::Sender::TopLevel,
                context: Some(ctx),
            };
            let cmd = decide(&p).expect("confirmed=true 应绕过确认门禁");
            assert_eq!(cmd.id, "system.shutdown");
        }

        #[test]
        fn safe_command_passes_gate_without_confirm() {
            let p = dd_protocol::messages::InvokeParams {
                id: "system.lock".into(),
                sender: dd_protocol::model::Sender::TopLevel,
                context: None,
            };
            let cmd = decide(&p).expect("安全命令无需确认");
            assert_eq!(cmd.id, "system.lock");
        }

        #[test]
        fn unknown_id_toasts() {
            let p = dd_protocol::messages::InvokeParams {
                id: "system.nope".into(),
                sender: dd_protocol::model::Sender::TopLevel,
                context: None,
            };
            let result = decide(&p);
            assert!(matches!(result, Err(CommandResult::ShowToast { .. })));
        }
    }
}

/// 非 Windows 占位（P4 Windows 优先）：编译恒成立，功能待对应平台轮实现。
#[cfg(not(windows))]
mod sys {
    use super::*;
    use dd_ext::Effect;

    pub fn top_level_commands() -> Vec<CommandItem> {
        // macOS（pmset/osascript）与 Linux（systemctl）命令表：TODO 对应平台轮
        Vec::new()
    }

    pub fn handle_invoke(_params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
        (
            CommandResult::ShowToast {
                message: "系统命令：当前平台尚未实现（P4 Windows 优先）".to_string(),
                duration_ms: Some(2_500),
            },
            Vec::new(),
        )
    }
}
