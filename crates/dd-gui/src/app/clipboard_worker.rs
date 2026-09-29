//! R-07：剪贴板写入常驻工作线程。
//!
//! 根因：`host/set_clipboard` 原实现 `thread::spawn(...).join()` 在 **UI 线程**
//! 等待写入完成——arboard 打开 Win32 剪贴板（与剪贴板管理器争 `OpenClipboard`）
//! 期间整个面板冻结，spawn 毫无收益。现改为：请求经 mpsc 入队，常驻工作线程
//! 消费写入，结果回传 UI 线程经既有 toast 基建反馈（与 R-16 同一套失败提示）。
//!
//! 线程创建失败对齐 R-05 降级口径：`log::error!` 后 `host/set_clipboard`
//! 降级为不可用（面板其余功能不受影响）。

use std::sync::mpsc::{self, Receiver, Sender};

/// 一条待写入请求（UI 线程 → 工作线程）。
pub(crate) struct ClipboardRequest {
    pub(crate) ext_id: String,
    pub(crate) text: String,
}

/// 写入结果（工作线程 → UI 线程）。
pub(crate) enum ClipboardOutcome {
    /// 写入成功（S-07 既有反馈口径：info 日志 + 轻量 toast）
    Written { ext_id: String, len: usize },
    /// 写入失败（toast 失败提示，文案键 `toast.clipboard_fail`）
    Failed { ext_id: String, error: String },
}

/// 启动常驻工作线程，返回 (请求发送端, 结果接收端)。
///
/// 线程创建失败（R-05 降级口径）→ 发送端为 `None`（`host/set_clipboard`
/// 降级为不可用，调用方提示失败），接收端恒可用（空）。
pub(crate) fn spawn() -> (Option<Sender<ClipboardRequest>>, Receiver<ClipboardOutcome>) {
    let (req_tx, req_rx) = mpsc::channel::<ClipboardRequest>();
    let (out_tx, out_rx) = mpsc::channel::<ClipboardOutcome>();
    match std::thread::Builder::new()
        .name("dd-clipboard".into())
        .spawn(move || run_worker_loop(req_rx, out_tx, real_write))
    {
        Ok(_handle) => (Some(req_tx), out_rx),
        Err(e) => {
            log::error!("[dd-gui] 剪贴板工作线程创建失败：{e} —— host/set_clipboard 降级为不可用");
            (None, out_rx)
        }
    }
}

/// 工作线程主循环（写入函数可注入，单测用假写入）：逐条消费请求、写入、
/// 把结果回传 UI 线程；请求端全部 drop（进程退出中）后自然退出。
pub(crate) fn run_worker_loop(
    rx: Receiver<ClipboardRequest>,
    out_tx: Sender<ClipboardOutcome>,
    write: impl Fn(&str) -> Result<(), String>,
) {
    for req in rx.iter() {
        let outcome = match write(&req.text) {
            Ok(()) => ClipboardOutcome::Written {
                ext_id: req.ext_id,
                len: req.text.len(),
            },
            Err(error) => ClipboardOutcome::Failed {
                ext_id: req.ext_id,
                error,
            },
        };
        // UI 端已关闭（进程退出中）时丢弃结果即可，不算错误
        let _ = out_tx.send(outcome);
    }
}

/// 真实写入：每次请求新建 arboard 实例（与改前行为一致——Win32 剪贴板
/// 打开/关闭成对，持锁窗口期缩到单次写入）。
fn real_write(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text.to_string()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{run_worker_loop, ClipboardOutcome, ClipboardRequest};
    use std::sync::mpsc;

    /// R-07：请求入队 → 工作线程消费 → 成功/失败结果回传 UI 线程
    /// （假写入注入：含 "bad" 触发失败、其余成功）。
    #[test]
    fn r07_clipboard_worker_roundtrip() {
        let (req_tx, req_rx) = mpsc::channel::<ClipboardRequest>();
        let (out_tx, out_rx) = mpsc::channel::<ClipboardOutcome>();
        let handle = std::thread::spawn(move || {
            run_worker_loop(req_rx, out_tx, |text| {
                if text == "bad" {
                    Err("clipboard busy".to_string())
                } else {
                    Ok(())
                }
            })
        });

        req_tx
            .send(ClipboardRequest {
                ext_id: "ext.a".into(),
                text: "hello 剪贴板".into(),
            })
            .unwrap();
        req_tx
            .send(ClipboardRequest {
                ext_id: "ext.b".into(),
                text: "bad".into(),
            })
            .unwrap();
        drop(req_tx); // 关闭请求端 → 工作线程自然退出

        match out_rx.recv().unwrap() {
            ClipboardOutcome::Written { ext_id, len } => {
                assert_eq!(ext_id, "ext.a");
                assert_eq!(len, "hello 剪贴板".len(), "成功结果携带实际字节数");
            }
            ClipboardOutcome::Failed { .. } => panic!("成功写入不应回传 Failed"),
        }
        match out_rx.recv().unwrap() {
            ClipboardOutcome::Failed { ext_id, error } => {
                assert_eq!(ext_id, "ext.b");
                assert_eq!(error, "clipboard busy");
            }
            ClipboardOutcome::Written { .. } => panic!("失败写入不应回传 Written"),
        }

        handle.join().unwrap(); // 请求端 drop 后循环退出（常驻线程可终止）
    }
}
