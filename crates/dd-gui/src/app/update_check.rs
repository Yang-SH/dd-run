//! §5.2（dd-run-改进分析.md）：检查更新的应用侧装配——手动按钮 + 启动自动检查
//! （`settings.check_updates`，默认关）+ 结果轮询转 toast。
//!
//! 拉取在后台线程（系统 `curl`，`--max-time 3`），结果经 mpsc 回 UI 线程——
//! 与 invoke / page 轮询同范式；**不做自动安装**（原案口径：便携版静默替换
//! exe 风险大于收益），结果只到 toast + 打开 Releases 页的提示。

use super::*;
use crate::app::toast_state::ToastKind;
use crate::update_check::{self, UpdateStatus};
use std::path::Path;
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

    /// 轮询检查/下载结果 → toast（在 logic() 与其余 poll 同点调用）。
    /// O17：`Downloading` 为进行态消息——通道保持待终态；终态清通道与进行位；
    /// `Available` 态留存版本号（关于卡「下载新版」按钮数据源）。
    pub(crate) fn poll_update_check(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.view.update_rx else {
            return;
        };
        let status = match rx.try_recv() {
            Ok(status) => status,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.view.update_rx = None;
                self.view.update_downloading = false;
                return;
            }
        };
        // 进度消息：置进行位、保留通道（下载线程随后发终态）。
        if matches!(status, UpdateStatus::Downloading) {
            self.view.update_downloading = true;
            ctx.request_repaint();
            return;
        }
        self.view.update_rx = None;
        self.view.update_downloading = false;
        let lang = self.view.lang_effective;
        let (message, kind) = match &status {
            UpdateStatus::UpToDate => (
                crate::text::t(lang, "toast.update_latest").to_string(),
                ToastKind::Info,
            ),
            UpdateStatus::Available { version } => (
                crate::text::t(lang, "toast.update_available").replace("{version}", version),
                ToastKind::Info,
            ),
            UpdateStatus::Failed(e) => (
                crate::text::t(lang, "toast.update_failed").replace("{e}", e),
                ToastKind::Error,
            ),
            UpdateStatus::Downloading => unreachable!("进度消息已在上方提前返回"),
            UpdateStatus::Downloaded { .. } => (
                crate::text::t(lang, "toast.update_downloaded").to_string(),
                ToastKind::Success,
            ),
            UpdateStatus::VerifyFailed(e) => (
                crate::text::t(lang, "toast.update_verify_failed").replace("{e}", e),
                ToastKind::Error,
            ),
        };
        // Available 态留存版本号供下载按钮；其余清空。
        match &status {
            UpdateStatus::Available { version } => {
                self.view.update_available = Some(version.clone());
            }
            UpdateStatus::UpToDate | UpdateStatus::Failed(_) => {
                self.view.update_available = None;
            }
            _ => {}
        }
        let duration_ms = match &status {
            UpdateStatus::Downloaded { .. } => 6_000, // 指引文案需读时间
            _ => 4_000,
        };
        self.show_toast_kind(kind, message, Some(duration_ms));
        if let UpdateStatus::Downloaded { path } = &status {
            // 「打开所在目录」：explorer /select 定位 zip（S-08 路径校验既有）；
            // 失败仅记日志——zip 已在校验通过的路径上，指引文案随 toast 呈现。
            if let Err(e) = crate::platform::reveal_in_folder(path) {
                log::warn!("[dd-gui] 定位已下载更新失败：{e}");
            }
        }
        ctx.request_repaint();
    }

    /// O17：发起下载（Available 态「下载新版」按钮；在途时重复触发 no-op）。
    /// 检查与下载共用 `update_rx` 单槽通道（两阶段顺序发生，互不并发）。
    pub(crate) fn start_update_download(&mut self) {
        if self.view.update_rx.is_some() || self.view.update_downloading {
            return; // 检查或下载在途——curl 超时兜底，无需取消机制
        }
        let Some(version) = self.view.update_available.clone() else {
            return; // 无 Available 态——按钮本不该出现，防御
        };
        let Some(config) = dd_host::manifest::config_file() else {
            let lang = self.view.lang_effective;
            self.show_toast_kind(
                ToastKind::Error,
                crate::text::t(lang, "toast.update_verify_failed")
                    .replace("{e}", "无法定位数据目录"),
                Some(4_000),
            );
            return;
        };
        let dir = config
            .parent()
            .map(|root| root.join("cache").join("updates"))
            .unwrap_or_else(|| std::path::PathBuf::from("cache").join("updates"));
        let zip_path = dir.join(update_check::portable_zip_asset(&version));
        let sums_path = dir.join(update_check::SHA256SUMS_ASSET);
        let (tx, rx) = mpsc::channel();
        self.view.update_rx = Some(rx);
        thread::spawn(move || {
            let _ = tx.send(UpdateStatus::Downloading);
            let status = download_and_verify(&version, &zip_path, &sums_path);
            let _ = tx.send(status);
        });
        let lang = self.view.lang_effective;
        self.show_toast_kind(
            ToastKind::Info,
            crate::text::t(lang, "toast.update_downloading"),
            Some(4_000),
        );
    }

    /// 启动自动检查（bin 入口装配；`settings.check_updates` 默认关）。
    /// `pub`：调用方是 bin（`main.rs`），在 lib 外。
    pub fn maybe_autostart_update_check(&mut self) {
        if self.draft.settings.check_updates {
            self.start_update_check();
        }
    }
}

/// O17：下载 zip + SHA256SUMS → 校验；失败清理残留并给可读原因。
/// 边界（承原案）：只下载 + 校验 + 指引，**不做自替换**。
fn download_and_verify(version: &str, zip_path: &Path, sums_path: &Path) -> UpdateStatus {
    let asset = update_check::portable_zip_asset(version);
    if let Some(dir) = zip_path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            return UpdateStatus::VerifyFailed(format!("无法创建下载目录：{e}"));
        }
    }
    let cleanup = || {
        let _ = std::fs::remove_file(zip_path);
        let _ = std::fs::remove_file(sums_path);
    };
    let zip_url = update_check::release_asset_url(&asset);
    if let Err(e) = curl_to_file(&zip_url, zip_path, 300) {
        cleanup();
        return UpdateStatus::VerifyFailed(format!("下载失败：{e}"));
    }
    let sums_url = update_check::release_asset_url(update_check::SHA256SUMS_ASSET);
    if let Err(e) = curl_to_file(&sums_url, sums_path, 30) {
        cleanup();
        return UpdateStatus::VerifyFailed(format!("SHA256SUMS 拉取失败：{e}"));
    }
    let result = (|| -> Result<(), String> {
        let sums =
            std::fs::read_to_string(sums_path).map_err(|e| format!("SHA256SUMS 读取失败：{e}"))?;
        let bytes = std::fs::read(zip_path).map_err(|e| format!("zip 读取失败：{e}"))?;
        update_check::verify_zip_bytes(&sums, &asset, &bytes)
    })();
    match result {
        Ok(()) => UpdateStatus::Downloaded {
            path: zip_path.display().to_string(),
        },
        Err(e) => {
            cleanup();
            UpdateStatus::VerifyFailed(e)
        }
    }
}

/// 经系统 curl 下载 URL 到目标文件（失败时尽力删除半截文件）。
fn curl_to_file(url: &str, dest: &Path, max_time_secs: u64) -> Result<(), String> {
    let output = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            &max_time_secs.to_string(),
            "-A",
            &format!("dd-run/{}", update_check::CURRENT_VERSION),
            "-o",
        ])
        .arg(dest)
        .arg(url)
        .output()
        .map_err(|e| format!("无法启动 curl（{e}）"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let _ = std::fs::remove_file(dest);
        return Err(format!(
            "curl 退出码 {:?}{}",
            output.status.code(),
            detail
                .trim()
                .is_empty()
                .then(String::new)
                .unwrap_or_else(|| format!("：{}", detail.trim()))
        ));
    }
    if !dest.is_file() {
        return Err("curl 成功但未产出文件".to_string());
    }
    Ok(())
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

    // ── O17：下载编排轮询 ────────────────────────────────────────────────────

    /// 进度消息保持通道 + 置进行位；终态清位（下载线程两段发送的消费语义）。
    #[test]
    fn poll_keeps_channel_on_progress_and_clears_on_terminal() {
        let mut app = make_app();
        let c = ctx();
        let (tx, rx) = mpsc::channel::<UpdateStatus>();
        app.view.update_rx = Some(rx);
        tx.send(UpdateStatus::Downloading).unwrap();
        app.poll_update_check(&c);
        assert!(app.view.update_rx.is_some(), "进度消息不清通道");
        assert!(app.view.update_downloading, "进行位置位");

        tx.send(UpdateStatus::VerifyFailed("模拟失败".into()))
            .unwrap();
        app.poll_update_check(&c);
        assert!(app.view.update_rx.is_none(), "终态清通道");
        assert!(!app.view.update_downloading, "进行位清零");
    }

    /// Available 结果留存版本号供下载按钮；UpToDate 清空。
    #[test]
    fn poll_retains_available_version_for_download_button() {
        let mut app = make_app();
        let c = ctx();
        let (tx, rx) = mpsc::channel::<UpdateStatus>();
        app.view.update_rx = Some(rx);
        tx.send(UpdateStatus::Available {
            version: "9.9.9".into(),
        })
        .unwrap();
        app.poll_update_check(&c);
        assert_eq!(
            app.view.update_available.as_deref(),
            Some("9.9.9"),
            "Available 留存版本号"
        );

        let (tx, rx) = mpsc::channel::<UpdateStatus>();
        app.view.update_rx = Some(rx);
        tx.send(UpdateStatus::UpToDate).unwrap();
        app.poll_update_check(&c);
        assert!(app.view.update_available.is_none(), "UpToDate 清留存");
    }
}
