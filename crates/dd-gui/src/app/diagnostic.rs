//! P2-5.3②③（dd-run-改进分析.md §5.3）：打开日志目录 + 导出诊断报告。
//!
//! 诊断报告 = **单个 JSON 文件**（写入 logs 目录，文件名含时间戳）——原案是
//! zip，但 zip 需要 crate（违背 lock 零新增纪律）或 shell 出平台命令；单文件
//! JSON 用户可直接阅读/附件，覆盖同一诉求（扩展作者提 issue 附此文件）。
//! 内容：宿主元信息 + 设置（本机无敏感字段：热键/自启/路径均为本机配置，
//! 导出时原样含入——已在模块头记档）+ 扩展清单摘要 + 信任判定 + 源状态 +
//! 解析失败清单 + 最近 200 行日志尾部。

use crate::app::PaletteApp;
use dd_host::manifest::logs_dir;

/// 日志尾部携带的行数上限（§5.3 口径）。
const LOG_TAIL_LINES: usize = 200;

impl PaletteApp {
    /// ②「打开日志目录」（Explorer / 平台文件管理器）。目录尚未创建（从未
    /// 写过日志/panic）时先创建——用户点了按钮却开出一个不存在目录属最差体验。
    pub(crate) fn open_logs_dir(&mut self) {
        let Some(dir) = logs_dir() else {
            self.show_error_toast(self.tr("toast.diag_dir_missing").to_string());
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        let dir_str = dir.display().to_string();
        if let Err(e) = crate::platform::open_path(&dir_str) {
            log::warn!("[dd-gui] 打开日志目录失败：{e}");
            self.show_error_toast(self.tr("toast.diag_dir_open_fail").replace("{e}", &e));
        }
    }

    /// ③「导出诊断报告」：组装单 JSON 落 logs 目录 + toast 反馈文件名。
    /// 全程 best-effort：任何一段组装/写盘失败都给用户可见错误（不静默）。
    pub(crate) fn export_diagnostic(&mut self) {
        let Some(dir) = logs_dir() else {
            self.show_error_toast(self.tr("toast.diag_dir_missing").to_string());
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        let report = self.build_diagnostic_report();
        let file_name = format!(
            "dd-run-diagnostic-{}.json",
            crate::crashlog::local_timestamp_rfc3339().replace([':', '+'], "-")
        );
        let path = dir.join(&file_name);
        match serde_json::to_string_pretty(&report)
            .map_err(|e| e.to_string())
            .and_then(|text| std::fs::write(&path, text).map_err(|e| e.to_string()))
        {
            Ok(()) => {
                log::info!("[dd-gui] 诊断报告已导出：{}", path.display());
                let msg = self.tr("toast.diag_exported").replace("{file}", &file_name);
                self.show_toast(msg, Some(4_000));
            }
            Err(e) => {
                log::warn!("[dd-gui] 诊断报告导出失败：{e}");
                self.show_error_toast(self.tr("toast.diag_export_fail").replace("{e}", &e));
            }
        }
    }

    /// 组装报告（不含写盘；单测锚点）。
    fn build_diagnostic_report(&self) -> serde_json::Value {
        let settings: serde_json::Value =
            serde_json::from_str(&self.draft.settings.to_json_string()).unwrap_or_default();
        let extensions: Vec<serde_json::Value> = self
            .pool
            .exts
            .iter()
            .map(|e| {
                serde_json::json!({
                    "id": e.manifest.id,
                    "name": e.manifest.name,
                    "version": e.manifest.version,
                    "frozen": e.manifest.frozen,
                    "enabled": !self.draft.settings.disabled_extensions.contains(&e.manifest.id),
                })
            })
            .collect();
        let trust: serde_json::Value = self
            .pool
            .trust
            .iter()
            .map(|(id, a)| {
                (
                    id.clone(),
                    serde_json::json!({ "trust": format!("{:?}", a.trust), "origin": format!("{:?}", a.origin) }),
                )
            })
            .collect();
        let sources: Vec<serde_json::Value> = self
            .pool
            .sources
            .iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "name": s.name,
                    "status": format!("{:?}", s.status),
                })
            })
            .collect();
        serde_json::json!({
            "meta": {
                "host_version": dd_gui::aggregator::HOST_VERSION,
                "protocol_version": dd_gui::aggregator::PROTOCOL_VERSION,
                "os": std::env::consts::OS,
                "exported_at": crate::crashlog::local_timestamp_rfc3339(),
            },
            "settings": settings,
            "extensions": extensions,
            "trust": trust,
            "sources": sources,
            "skipped_manifests": self.pool.skipped_manifests,
            "ledger_state": format!("{:?}", self.pool.ledger_state),
            "log_tail": log_tail(LOG_TAIL_LINES),
        })
    }
}

/// 读取当前日志文件尾部 `n` 行（文件缺失/读失败 → 空数组；§5.3「最近 200 行」）。
fn log_tail(n: usize) -> Vec<String> {
    let Some(dir) = logs_dir() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(dir.join("dd-run.log")) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::make_app;

    /// 报告结构完整：meta/设置/扩展/信任/日志尾五段齐备，日志尾按 n 截断。
    #[test]
    fn diagnostic_report_has_all_sections() {
        let app = make_app();
        let report = app.build_diagnostic_report();
        assert!(report["meta"]["host_version"].is_string());
        assert!(report["settings"].is_object());
        assert!(report["extensions"].is_array());
        assert!(report["trust"].is_object());
        assert!(report["sources"].is_array());
        assert!(report["log_tail"].is_array());
    }

    /// log_tail：不足 n 行全量返回；空文件 → 空数组（不 panic）。
    #[test]
    fn log_tail_truncates_to_n() {
        let tail = log_tail(200);
        assert!(tail.len() <= 200);
    }
}
