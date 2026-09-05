//! 扩展宿主请求（ShowToast/Clipboard/OpenUrl 等）与 8 种结果裁决落地。

use crate::app::toast::ConfirmDialog;
use crate::app::PaletteApp;
use dd_gui::result;
use dd_gui::result::HostAction;
use dd_protocol::messages::{OpenUrlParams, RawMessage, SetClipboardParams, ShowStatusParams};
use eframe::egui;

impl PaletteApp {
    /// M4 P2：消费扩展的 `host/*` 请求并执行真实副作用（协议 §7.2–§7.4）。
    /// `host/show_status` → Toast；`host/set_clipboard` → 剪贴板；`host/open_url` → 浏览器。
    /// 应答由 dd-host 完成（§7.4 能力前置：未声明回 `-32601`），此处只做执行端。
    pub(crate) fn poll_host_requests(&mut self) {
        if self.processes.is_empty() {
            return;
        }
        // 收集期间只借用 `self.processes`；执行副作用需要 `&mut self`，故先取走再统一处理。
        let requests: Vec<(String, RawMessage)> = self
            .processes
            .iter_mut()
            .flat_map(|(ext_id, proc)| {
                proc.drain_host_requests()
                    .into_iter()
                    .map(move |msg| (ext_id.clone(), msg))
            })
            .collect();
        for (ext_id, msg) in requests {
            self.execute_host_request(&ext_id, &msg);
        }
    }

    /// 单个 `host/*` 请求的执行端（M4 P2）。未知方法记日志不报错（dd-host 已应答）。
    pub(crate) fn execute_host_request(&mut self, ext_id: &str, msg: &RawMessage) {
        let Some(method) = msg.method.as_deref() else {
            return;
        };
        match method {
            "host/show_status" => {
                let Ok(params) = serde_json::from_value::<ShowStatusParams>(
                    msg.params.clone().unwrap_or(serde_json::Value::Null),
                ) else {
                    eprintln!("[dd-gui] host/show_status 参数解析失败（ext={ext_id}）");
                    return;
                };
                eprintln!(
                    "[dd-gui] host/show_status（ext={ext_id}）：{} state={:?}",
                    params.message, params.state
                );
                self.show_toast(params.message, params.duration_ms);
            }
            "host/set_clipboard" => {
                let Ok(params) = serde_json::from_value::<SetClipboardParams>(
                    msg.params.clone().unwrap_or(serde_json::Value::Null),
                ) else {
                    eprintln!("[dd-gui] host/set_clipboard 参数解析失败（ext={ext_id}）");
                    return;
                };
                let result = std::thread::spawn(move || {
                    let mut cb = arboard::Clipboard::new()?;
                    cb.set_text(params.text)?;
                    Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                })
                .join();
                match result {
                    Ok(Ok(())) => eprintln!("[dd-gui] host/set_clipboard（ext={ext_id}）成功"),
                    Ok(Err(e)) => {
                        eprintln!("[dd-gui] host/set_clipboard（ext={ext_id}）失败：{e}")
                    }
                    Err(_) => eprintln!("[dd-gui] host/set_clipboard 线程异常（ext={ext_id}）"),
                }
            }
            "host/open_url" => {
                let Ok(params) = serde_json::from_value::<OpenUrlParams>(
                    msg.params.clone().unwrap_or(serde_json::Value::Null),
                ) else {
                    eprintln!("[dd-gui] host/open_url 参数解析失败（ext={ext_id}）");
                    return;
                };
                eprintln!("[dd-gui] host/open_url（ext={ext_id}）：{}", params.url);
                if let Err(e) = webbrowser::open(&params.url) {
                    eprintln!("[dd-gui] host/open_url（ext={ext_id}）失败：{e}");
                }
            }
            other => eprintln!("[dd-gui] 未知 host/* 请求：{other}（ext={ext_id}，已应答忽略）"),
        }
    }
}

impl PaletteApp {
    /// 应用 8 种 Kind 裁决出的宿主动作（A4）。
    pub(crate) fn apply_action(&mut self, ctx: &egui::Context, action: HostAction, ext_id: &str) {
        match action {
            HostAction::Dismiss => self.dismiss(ctx),
            HostAction::Hide => self.hide_keep_state(ctx),
            HostAction::GoHome => self.stack.go_home(),
            HostAction::GoBack => {
                self.stack.go_back();
            }
            HostAction::KeepOpen => {}
            HostAction::GoToPage { page_id } => {
                let ext_id = ext_id.to_string();
                self.open_page(&ext_id, &page_id, None, None);
            }
            HostAction::ShowToast {
                message,
                duration_ms,
            } => self.show_toast(message, duration_ms),
            HostAction::Confirm {
                title,
                description,
                confirm_label,
                is_critical,
            } => {
                // §8.3 注：确认后宿主带 `context.confirmed = true` 重新 invoke。
                // 沿用原 invoke 的 sender/context（`pending_confirm_for` 保证
                // 不丢失搜索词/选中项，仅补 confirmed=true）。
                let command_id = self.last_command_id.clone().unwrap_or_default();
                let pending = result::pending_confirm_for(&command_id, self.last_invoke.as_ref());
                self.confirm = Some(ConfirmDialog {
                    ext_id: ext_id.to_string(),
                    title,
                    description,
                    confirm_label,
                    is_critical,
                    pending,
                });
            }
        }
    }
}
