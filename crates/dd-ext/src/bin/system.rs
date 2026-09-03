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

use dd_ext::{run, ExtensionSpec};
use dd_protocol::messages::InvokeParams;
use dd_protocol::model::{CommandItem, CommandRef, CommandResult, Icon, IconKind};

fn main() {
    run(&spec());
}

fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: "com.ddrun.system",
        display_name: "System",
        description: "锁屏 / 睡眠 / 关机 / 重启 / 注销",
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

    /// M5 UI 批次 2 UI 验收项 id：一条真实走完「协议 icon 字段 → 宿主透传 →
    /// PNG 解码 → 20×20 纹理渲染」链路的演示命令。**不进 [`COMMANDS`] 目录**
    /// （保持"5 条系统命令"的目录不变式，目录测试不受影响），仅由
    /// [`top_level_commands`] 末尾追加；点击无系统副作用（Toast 提示），
    /// 专用于验收 Path 图标渲染（设计稿 04）。
    pub const DEMO_ICON_ITEM_ID: &str = "system.ui_accept.png_icon";

    /// 演示用 PNG 资产：编译期锚定 `CARGO_MANIFEST_DIR`（= `crates/dd-ext`），
    /// 换检出目录/盘符也能定位，不硬编码绝对路径。
    pub const DEMO_ICON_PATH: &str =
        concat!(env!("CARGO_MANIFEST_DIR"), r"\assets\ui-accept-icon.png");

    pub const COMMANDS: &[SystemCommand] = &[
        SystemCommand {
            id: "system.lock",
            title: "Lock Screen",
            subtitle: "立即锁定工作站（rundll32 LockWorkStation）",
            confirm_description: "",
            dangerous: false,
            program: "rundll32.exe",
            args: &["user32.dll,LockWorkStation"],
        },
        SystemCommand {
            id: "system.sleep",
            title: "Sleep",
            subtitle: "使计算机进入睡眠（可唤醒）",
            confirm_description: "",
            dangerous: false,
            program: "rundll32.exe",
            args: &["powrprof.dll,SetSuspendState", "0,1,0"],
        },
        SystemCommand {
            id: "system.shutdown",
            title: "Shut Down",
            subtitle: "关闭计算机（⚠ 危险，需二次确认）",
            confirm_description: "计算机会立即关闭，未保存的工作将丢失。",
            dangerous: true,
            program: "shutdown.exe",
            args: &["/s", "/t", "0"],
        },
        SystemCommand {
            id: "system.restart",
            title: "Restart",
            subtitle: "重启计算机（⚠ 危险，需二次确认）",
            confirm_description: "计算机会立即重启，未保存的工作将丢失。",
            dangerous: true,
            program: "shutdown.exe",
            args: &["/r", "/t", "0"],
        },
        SystemCommand {
            id: "system.logoff",
            title: "Sign Out",
            subtitle: "注销当前用户（⚠ 危险，需二次确认）",
            confirm_description: "当前会话将结束，未保存的工作可能丢失。",
            dangerous: true,
            program: "shutdown.exe",
            args: &["/l"],
        },
    ];

    pub fn top_level_commands() -> Vec<CommandItem> {
        let mut items: Vec<CommandItem> = COMMANDS
            .iter()
            .map(|c| CommandItem {
                id: c.id.to_string(),
                title: c.title.to_string(),
                subtitle: Some(c.subtitle.to_string()),
                icon: Some(Icon {
                    kind: IconKind::Glyph,
                    value: "\u{E7E8}".to_string(), // PowerButton
                }),
                section: Some("系统".to_string()),
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
            .collect();
        // M5 UI 批次 2：追加 PNG Path 图标验收项（唯一走 Path 态的真实命令；
        // 其余内置扩展均为 Glyph——本项用于验收宿主 20×20 PNG 渲染链路）。
        items.push(CommandItem {
            id: DEMO_ICON_ITEM_ID.to_string(),
            title: "UI 验收：PNG 图标".to_string(),
            subtitle: Some("Path 图标链路（PNG 资产 → 宿主解码 → 20×20 渲染）".to_string()),
            icon: Some(Icon {
                kind: IconKind::Path,
                value: DEMO_ICON_PATH.to_string(),
            }),
            section: Some("系统".to_string()),
            tags: Some(vec!["ui".to_string(), "demo".to_string()]),
            details: None,
            text_to_suggest: None,
            more_commands: None,
            command: CommandRef::Invoke,
        });
        items
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
        // M5 UI 批次 2：验收项点击 → 提示性 Toast，不执行任何系统操作
        // （与"未知 id → 报错 Toast"区分开，语义为"演示成功"而非错误）。
        if params.id == DEMO_ICON_ITEM_ID {
            return Err(CommandResult::ShowToast {
                message: "UI 验收：PNG 图标渲染正常（演示项无操作）".to_string(),
                duration_ms: Some(1_800),
            });
        }
        let Some(cmd) = COMMANDS.iter().find(|c| c.id == params.id) else {
            return Err(CommandResult::ShowToast {
                message: format!("未知系统命令：{}", params.id),
                duration_ms: Some(2_500),
            });
        };
        // §8.3：确认后宿主带 confirmed=true 重发 invoke
        if cmd.dangerous && !confirmed {
            return Err(CommandResult::Confirm {
                title: format!("确认{}？", cmd.title),
                description: format!("{}该操作无法撤销，确认要执行吗？", cmd.confirm_description),
                confirm_label: format!("执行{}", cmd.title),
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
                    message: format!("执行 {} 失败：{e}", cmd.program),
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
            assert_eq!(COMMANDS.len(), 5);
            let ids: Vec<&str> = COMMANDS.iter().map(|c| c.id).collect();
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
            assert_eq!(COMMANDS.iter().filter(|c| c.dangerous).count(), 3);
        }

        #[test]
        fn top_level_has_png_path_icon_demo_with_existing_asset() {
            // M5 UI 批次 2：验收项 = 第 6 条（5 系统命令 + 1 demo），且其 icon
            // 为 Path 态、资产文件在编译期路径上真实存在——锁住"宿主有真实
            // Path 图标可渲染"这一验收前提（其余内置扩展全是 Glyph）。
            let items = top_level_commands();
            assert_eq!(items.len(), COMMANDS.len() + 1);
            let demo = items
                .iter()
                .find(|i| i.id == DEMO_ICON_ITEM_ID)
                .expect("验收 demo 应出现在顶层命令中");
            assert_eq!(demo.title, "UI 验收：PNG 图标");
            let Some(Icon {
                kind: IconKind::Path,
                value,
            }) = &demo.icon
            else {
                panic!("demo icon 应为 Path 态");
            };
            let asset = std::path::Path::new(value);
            assert!(asset.is_file(), "PNG 资产应存在（随 crate 分发）：{value}");
            let len = std::fs::metadata(asset).expect("读取资产元数据").len();
            assert!(len > 0, "PNG 资产不应为空文件");
        }

        #[test]
        fn demo_item_invoke_toasts_without_confirm() {
            // 点击验收项不触发任何系统副作用：直接 Toast（无需 Confirm 门禁）
            let p = dd_protocol::messages::InvokeParams {
                id: DEMO_ICON_ITEM_ID.into(),
                sender: dd_protocol::model::Sender::TopLevel,
                context: None,
            };
            let result = decide(&p);
            assert!(matches!(result, Err(CommandResult::ShowToast { .. })));
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
