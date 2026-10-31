//! §5.2（dd-run-改进分析.md）：检查更新的应用侧装配——手动按钮 + 启动自动检查
//! （`settings.check_updates`，默认关）+ 结果轮询转 toast。
//!
//! 拉取在后台线程（系统 `curl`，`--max-time 3`），结果经 mpsc 回 UI 线程——
//! 与 invoke / page 轮询同范式；**不做自动安装**（原案口径：便携版静默替换
//! exe 风险大于收益），结果只到 toast + 打开 Releases 页的提示。

use super::*;
use crate::app::toast_state::ToastKind;
use crate::update_check::{self, UpdateStatus};
use std::sync::mpsc;
use std::thread;

impl PaletteApp {
    /// 发起一次检查（按钮 / 启动自动检查共用；在途时重复点击为 no-op）。
    pub(crate) fn start_update_check(&mut self) {
        if self.view.update_rx.is_some() {
            return; // 已在途——curl --max-time 3 兜底，无需取消机制
        }
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let status = match update_check::fetch_latest_tag(3) {
                Ok(tag) => update_check::classify(update_check::CURRENT_VERSION, &tag),
                Err(e) => UpdateStatus::Failed(e),
            };
            let _ = tx.send(status);
        });
        self.view.update_rx = Some(rx);
        let lang = self.view.lang_effective;
        self.show_toast_kind(
            ToastKind::Info,
            crate::text::t(lang, "toast.update_checking"),
            Some(3_000),
        );
    }

    /// 轮询检查结果 → toast（在 logic() 与其余 poll 同点调用）。
    pub(crate) fn poll_update_check(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.view.update_rx else {
            return;
        };
        let status = match rx.try_recv() {
            Ok(status) => status,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.view.update_rx = None;
                return;
            }
        };
        self.view.update_rx = None;
        let lang = self.view.lang_effective;
        let message = match &status {
            UpdateStatus::UpToDate => crate::text::t(lang, "toast.update_latest").to_string(),
            UpdateStatus::Available { version } => {
                crate::text::t(lang, "toast.update_available").replace("{version}", version)
            }
            UpdateStatus::Failed(e) => {
                crate::text::t(lang, "toast.update_failed").replace("{e}", e)
            }
        };
        let kind = match &status {
            UpdateStatus::Failed(_) => ToastKind::Error,
            _ => ToastKind::Info,
        };
        self.show_toast_kind(kind, message, Some(4_000));
        ctx.request_repaint();
    }

    /// 启动自动检查（bin 入口装配；`settings.check_updates` 默认关）。
    /// `pub`：调用方是 bin（`main.rs`），在 lib 外。
    pub fn maybe_autostart_update_check(&mut self) {
        if self.draft.settings.check_updates {
            self.start_update_check();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ctx, make_app};
    use std::sync::mpsc;

    /// 在途时重复触发 no-op（真实拉取不能进测试——curl 会打真网络；
    /// 去重逻辑本身极简，这里只验证状态位）。
    #[test]
    fn poll_consumes_result_and_clears_inflight() {
        let mut app = make_app();
        let c = ctx();
        // Disconnected（发送端已弃）→ 清位、不出 toast
        let (tx, rx) = mpsc::channel::<UpdateStatus>();
        drop(tx);
        app.view.update_rx = Some(rx);
        app.poll_update_check(&c);
        assert!(app.view.update_rx.is_none(), "断开通道应清位");

        // Empty（仍在途）→ 保留（注意：`_` 模式会立即 drop 发送端——须命名保活）
        let (_tx_keep, rx) = mpsc::channel::<UpdateStatus>();
        app.view.update_rx = Some(rx);
        app.poll_update_check(&c);
        assert!(app.view.update_rx.is_some(), "空轮询应保持在途");
    }
}
