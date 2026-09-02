//! 命令执行结果的裁决层（纯逻辑，不依赖 egui，可单测）。
//!
//! 对齐 design 文档 §5.2 / protocol.md §8.3：扩展 `invoke` 后返回 8 种
//! `CommandResultKind`，宿主据此裁决"该做什么"——关闭/隐藏/回首页/返回/
//! 保持打开/跳页/弹提示/二次确认（对应验收 **A4**，单测覆盖全部 8 种）。
//!
//! 另有：
//! - [`PendingConfirm`]：`Confirm` 结果的挂起状态。宿主向用户确认后
//!   **重新 invoke**（`context.confirmed = true`，protocol.md §8.3 注），
//!   本类型持有重发所需的命令 id / sender / 原始 context；
//! - [`invoke_params`]：`invoke` 请求参数构造（§6.5 字段表）。

use dd_protocol::messages::{InvokeContext, InvokeParams};
use dd_protocol::model::{CommandResult, Sender};

/// 8 种 Kind 裁决出的宿主动作（框架无关，UI 层据此驱动窗口/页面栈）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostAction {
    /// 关闭面板（`Dismiss`）
    Dismiss,
    /// 隐藏（不关闭，保留状态）（`Hide`）
    Hide,
    /// 回根视图（`GoHome`）
    GoHome,
    /// 返回上一级（`GoBack`）
    GoBack,
    /// 保持打开（`KeepOpen`）
    KeepOpen,
    /// 跳转到某页（`GoToPage`）
    GoToPage { page_id: String },
    /// 弹提示（`ShowToast`）
    ShowToast {
        message: String,
        duration_ms: Option<u64>,
    },
    /// 需二次确认（`Confirm`）——UI 弹确认框，确认后经 [`PendingConfirm`] 重发
    Confirm {
        title: String,
        description: String,
        confirm_label: String,
        is_critical: bool,
    },
}

/// 把协议层的 `CommandResult` 裁决为宿主动作（8 种一一对应）。
pub fn resolve(result: &CommandResult) -> HostAction {
    match result {
        CommandResult::Dismiss => HostAction::Dismiss,
        CommandResult::GoHome => HostAction::GoHome,
        CommandResult::GoBack => HostAction::GoBack,
        CommandResult::Hide => HostAction::Hide,
        CommandResult::KeepOpen => HostAction::KeepOpen,
        CommandResult::GoToPage { page_id } => HostAction::GoToPage {
            page_id: page_id.clone(),
        },
        CommandResult::ShowToast {
            message,
            duration_ms,
        } => HostAction::ShowToast {
            message: message.clone(),
            duration_ms: *duration_ms,
        },
        CommandResult::Confirm {
            title,
            description,
            confirm_label,
            is_critical,
        } => HostAction::Confirm {
            title: title.clone(),
            description: description.clone(),
            confirm_label: confirm_label.clone(),
            is_critical: *is_critical,
        },
    }
}

/// `Confirm` 确认后的重发挂起状态。
#[derive(Debug, Clone, PartialEq)]
pub struct PendingConfirm {
    pub command_id: String,
    pub sender: Sender,
    pub context: Option<InvokeContext>,
}

impl PendingConfirm {
    /// 用户确认后构造重新 `invoke` 的参数（`context.confirmed = true`）。
    pub fn confirmed_params(&self) -> InvokeParams {
        let mut context = self.context.clone().unwrap_or(InvokeContext {
            query: None,
            selected_item_id: None,
            form_data: None,
            confirmed: None,
        });
        context.confirmed = Some(true);
        InvokeParams {
            id: self.command_id.clone(),
            sender: self.sender,
            context: Some(context),
        }
    }
}

/// §6.5 `invoke` 请求参数构造（顶层命令场景）。
///
/// `sender = top_level`；`context.query` 带当前搜索词（可为空串，
/// 但 `None` 表示"未提供"，二者在协议上有别，这里统一传 `Some`）。
pub fn invoke_params(id: &str, query: &str) -> InvokeParams {
    InvokeParams {
        id: id.to_string(),
        sender: Sender::TopLevel,
        context: Some(InvokeContext {
            query: Some(query.to_string()),
            selected_item_id: None,
            form_data: None,
            confirmed: None,
        }),
    }
}

/// 为 `Confirm` 结果构造挂起重发状态（协议 §8.3：确认后**重新 invoke**，
/// 且仅在 context 上补 `confirmed = true`）。
///
/// 关键：**沿用原始 `invoke` 的 `sender` 与 `context`**（搜索词 /
/// `selected_item_id` / `form_data` 等上下文连续性），而不是全新空 context——
/// 否则真实扩展在确认时会丢失用户当时所处的上下文。`last_invoke` 为 `None`
/// （正常不该发生，兜底）时回退到 `top_level` + 空 context。
pub fn pending_confirm_for(command_id: &str, last_invoke: Option<&InvokeParams>) -> PendingConfirm {
    match last_invoke {
        Some(p) => PendingConfirm {
            command_id: command_id.to_string(),
            sender: p.sender,
            context: p.context.clone(),
        },
        None => PendingConfirm {
            command_id: command_id.to_string(),
            sender: Sender::TopLevel,
            context: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_protocol::model::CommandResult;

    #[test]
    fn resolves_all_eight_kinds() {
        // A4：8 种 Kind 一一裁决
        assert_eq!(resolve(&CommandResult::Dismiss), HostAction::Dismiss);
        assert_eq!(resolve(&CommandResult::GoHome), HostAction::GoHome);
        assert_eq!(resolve(&CommandResult::GoBack), HostAction::GoBack);
        assert_eq!(resolve(&CommandResult::Hide), HostAction::Hide);
        assert_eq!(resolve(&CommandResult::KeepOpen), HostAction::KeepOpen);
        assert_eq!(
            resolve(&CommandResult::GoToPage {
                page_id: "p1".to_string()
            }),
            HostAction::GoToPage {
                page_id: "p1".to_string()
            }
        );
        assert_eq!(
            resolve(&CommandResult::ShowToast {
                message: "hi".to_string(),
                duration_ms: Some(2000),
            }),
            HostAction::ShowToast {
                message: "hi".to_string(),
                duration_ms: Some(2000),
            }
        );
        assert_eq!(
            resolve(&CommandResult::ShowToast {
                message: "hi".to_string(),
                duration_ms: None,
            }),
            HostAction::ShowToast {
                message: "hi".to_string(),
                duration_ms: None,
            }
        );
        assert_eq!(
            resolve(&CommandResult::Confirm {
                title: "删除？".to_string(),
                description: "不可恢复".to_string(),
                confirm_label: "删除".to_string(),
                is_critical: true,
            }),
            HostAction::Confirm {
                title: "删除？".to_string(),
                description: "不可恢复".to_string(),
                confirm_label: "删除".to_string(),
                is_critical: true,
            }
        );
    }

    #[test]
    fn pending_confirm_reinvokes_with_confirmed_flag() {
        let pending = PendingConfirm {
            command_id: "file.delete".to_string(),
            sender: Sender::ListItem,
            context: Some(InvokeContext {
                query: Some("del".to_string()),
                selected_item_id: Some("file.1".to_string()),
                form_data: None,
                confirmed: None,
            }),
        };
        let params = pending.confirmed_params();
        assert_eq!(params.id, "file.delete");
        assert_eq!(params.sender, Sender::ListItem);
        let ctx = params.context.expect("重发必带 context");
        assert_eq!(ctx.confirmed, Some(true), "确认后 confirmed=true");
        assert_eq!(ctx.query.as_deref(), Some("del"), "原始查询保留");
        assert_eq!(
            ctx.selected_item_id.as_deref(),
            Some("file.1"),
            "目标项 id 保留"
        );
    }

    #[test]
    fn pending_confirm_without_context_still_confirms() {
        let pending = PendingConfirm {
            command_id: "x".to_string(),
            sender: Sender::TopLevel,
            context: None,
        };
        let params = pending.confirmed_params();
        assert_eq!(
            params.context.as_ref().and_then(|c| c.confirmed),
            Some(true)
        );
    }

    #[test]
    fn invoke_params_carry_id_sender_and_query() {
        let params = invoke_params("calc.eval", "1+1");
        assert_eq!(params.id, "calc.eval");
        assert_eq!(params.sender, Sender::TopLevel);
        assert_eq!(
            params.context.as_ref().and_then(|c| c.query.as_deref()),
            Some("1+1")
        );
        assert_eq!(
            params.context.as_ref().and_then(|c| c.confirmed),
            None,
            "首次 invoke 不带 confirmed"
        );
    }

    #[test]
    fn pending_confirm_for_preserves_original_sender_and_context() {
        // P1 修复回归（协议 §8.3）：确认重发必须保留原始 sender + context
        let invoke = InvokeParams {
            id: "file.delete".to_string(),
            sender: Sender::ListItem,
            context: Some(InvokeContext {
                query: Some("del".to_string()),
                selected_item_id: Some("file.1".to_string()),
                form_data: None,
                confirmed: None,
            }),
        };
        let pending = pending_confirm_for("file.delete", Some(&invoke));
        assert_eq!(pending.sender, Sender::ListItem, "保留原 sender");
        assert_eq!(
            pending.context.as_ref().and_then(|c| c.query.as_deref()),
            Some("del"),
            "保留原 query"
        );
        assert_eq!(
            pending
                .context
                .as_ref()
                .and_then(|c| c.selected_item_id.as_deref()),
            Some("file.1"),
            "保留原 selected_item_id"
        );
        // 重发仍仅补 confirmed=true（不覆盖原 context）
        assert_eq!(
            pending.confirmed_params().context.unwrap().confirmed,
            Some(true)
        );
    }

    #[test]
    fn pending_confirm_for_falls_back_when_no_history() {
        let pending = pending_confirm_for("x", None);
        assert_eq!(pending.sender, Sender::TopLevel);
        assert_eq!(pending.context, None);
    }
}
