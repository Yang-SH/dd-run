//! Toast 与确认对话框状态（意图类型、时长策略）。

use crate::app::PaletteApp;
use dd_gui::result::PendingConfirm;
use dd_gui::theme;
use eframe::egui;
use std::time::Duration;
use std::time::Instant;

/// Toast 默认显示时长（扩展未指定 `duration_ms` 时）。
pub(crate) const TOAST_DEFAULT_MS: u64 = 2_000;

/// 失败路径(Error) Toast 固定时长：设计稿 §9.1 明令「执行失败路径固定 3000ms」。
/// Error 为宿主侧语义（扩展只能发 Info，见 [`DdGui::show_toast`]），故强制固定、忽略调用方传入值。
pub(crate) const TOAST_ERROR_MS: u64 = 3_000;

/// 解析 Toast 最终显示时长（毫秒）。
/// - Error 失败路径强制 [`TOAST_ERROR_MS`]，调用方传入值无效（设计稿 §9.1）；
/// - 其余（Info/Success）默认 [`TOAST_DEFAULT_MS`]，扩展显式 `duration_ms` 时尊重。
pub(crate) fn toast_duration_ms(kind: ToastKind, requested: Option<u64>) -> u64 {
    match kind {
        ToastKind::Error => TOAST_ERROR_MS,
        _ => requested.unwrap_or(TOAST_DEFAULT_MS),
    }
}

/// Toast 意图（设计稿 §09，C 组批次 C3）：success / error / info 三意图。
///
/// 协议层**不携带**意图字段（`ShowToast` 结果仅 message + duration）——
/// 扩展产生的 Toast 默认 info（C2 验收口径「未接意图字段前默认 info」）；
/// 宿主自身路径按语义标注（命令失败 → Error 等）。意图字段即"留接口"：
/// 后续协议若扩展意图，只需在此枚举加映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastKind {
    /// 接口预留（协议 `ShowToast` 暂无意图字段；宿主暂无确定 success 语义
    /// 路径——calc 求值结果由扩展侧 Toast 文本呈现）。协议扩展意图后启用。
    #[allow(dead_code)]
    Success,
    Error,
    Info,
}

impl ToastKind {
    /// 意图图标（§9.1：success E73E / error E783 / info E946，16px）。
    pub(crate) fn glyph(self) -> char {
        match self {
            ToastKind::Success => '\u{E73E}',
            ToastKind::Error => '\u{E783}',
            ToastKind::Info => '\u{E946}',
        }
    }

    /// 意图语义色（§9.1：success → --success / error → --danger / info → --text-2）。
    pub(crate) fn color(self, p: &theme::Palette) -> egui::Color32 {
        match self {
            ToastKind::Success => p.success,
            ToastKind::Error => p.danger,
            ToastKind::Info => p.text2,
        }
    }
}

/// Toast 提示条（过期即清除）。
pub(crate) struct ToastState {
    pub(crate) message: String,
    pub(crate) expires: Instant,
    pub(crate) kind: ToastKind,
}

/// 待用户二次确认的对话框。
pub(crate) struct ConfirmDialog {
    /// 发起该命令的扩展（确认后据此重发 `invoke`）。
    pub(crate) ext_id: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) confirm_label: String,
    pub(crate) is_critical: bool,
    /// 确认后重发 `invoke` 所需的原始请求。
    pub(crate) pending: PendingConfirm,
}

impl PaletteApp {
    pub(crate) fn show_toast(&mut self, message: impl Into<String>, duration_ms: Option<u64>) {
        self.show_toast_kind(ToastKind::Info, message, duration_ms);
    }

    /// 失败路径 Toast：固定 [`TOAST_ERROR_MS`] = 3000ms（设计稿 §9.1），
    /// 不接收时长参数，杜绝调用方传入偏离值。
    pub(crate) fn show_error_toast(&mut self, message: impl Into<String>) {
        self.show_toast_kind(ToastKind::Error, message, None);
    }

    /// 带意图的 Toast（C 组批次 C3）：宿主自身路径按语义标注意图；
    /// 扩展 `ShowToast` 路径走默认 info 的 [`Self::show_toast`]。
    pub(crate) fn show_toast_kind(
        &mut self,
        kind: ToastKind,
        message: impl Into<String>,
        duration_ms: Option<u64>,
    ) {
        let ms = toast_duration_ms(kind, duration_ms);
        self.toast = Some(ToastState {
            message: message.into(),
            expires: Instant::now() + Duration::from_millis(ms),
            kind,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── C 组批次 C3：Error Toast 失败路径固定 3000ms（设计稿 §9.1） ──

    #[test]
    fn error_toast_is_fixed_at_3000ms_regardless_of_requested() {
        // §9.1：执行失败路径固定 3000ms；Error 是宿主侧语义、扩展只能发 Info，
        // 故强制覆盖任何传入值（杜绝此前 6 条 Error 路径 3000/4000/2000/2500/2000/2000 偏离）。
        assert_eq!(toast_duration_ms(ToastKind::Error, None), 3_000);
        assert_eq!(toast_duration_ms(ToastKind::Error, Some(2_000)), 3_000);
        assert_eq!(toast_duration_ms(ToastKind::Error, Some(9_999)), 3_000);
    }

    #[test]
    fn non_error_toasts_respect_default_or_explicit() {
        // Info/Success 默认 2000ms，显式值被尊重（扩展 ShowToast 可自定义时长）。
        assert_eq!(toast_duration_ms(ToastKind::Info, None), 2_000);
        assert_eq!(toast_duration_ms(ToastKind::Info, Some(5_000)), 5_000);
        assert_eq!(toast_duration_ms(ToastKind::Success, None), 2_000);
        assert_eq!(toast_duration_ms(ToastKind::Success, Some(1_500)), 1_500);
    }
}
