//! 扩展宿主请求（ShowToast/Clipboard/OpenUrl 等）与 8 种结果裁决落地。

use crate::app::toast::ConfirmDialog;
use crate::app::PaletteApp;
use dd_gui::result;
use dd_gui::result::HostAction;
use dd_protocol::messages::{OpenUrlParams, RawMessage, SetClipboardParams, ShowStatusParams};
use dd_protocol::methods::{
    METHOD_HOST_OPEN_URL, METHOD_HOST_SET_CLIPBOARD, METHOD_HOST_SHOW_STATUS,
};
use eframe::egui;

/// S-07（2026-09-24）：`host/set_clipboard` 文本长度上限（UTF-8 字节数）。
/// 超限**拒绝**（非截断——半截账号/地址比不写入更危险）。
const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;

/// S-07 纯决策（单测锚点）：文本是否允许写入剪贴板。
fn clipboard_text_allowed(text: &str) -> bool {
    text.len() <= MAX_CLIPBOARD_BYTES
}

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
            METHOD_HOST_SHOW_STATUS => {
                let Ok(params) = serde_json::from_value::<ShowStatusParams>(
                    msg.params.clone().unwrap_or(serde_json::Value::Null),
                ) else {
                    log::warn!("[dd-gui] host/show_status 参数解析失败（ext={ext_id}）");
                    return;
                };
                log::debug!(
                    "[dd-gui] host/show_status（ext={ext_id}）：{} state={:?}",
                    params.message,
                    params.state
                );
                self.show_toast(params.message, params.duration_ms);
            }
            METHOD_HOST_SET_CLIPBOARD => {
                let Ok(params) = serde_json::from_value::<SetClipboardParams>(
                    msg.params.clone().unwrap_or(serde_json::Value::Null),
                ) else {
                    log::warn!("[dd-gui] host/set_clipboard 参数解析失败（ext={ext_id}）");
                    return;
                };
                // S-07（2026-09-24）：长度上限前置——超限拒绝且**不静默**
                //（warn 含扩展 id 与实际长度 + 一次性 toast）。
                if !clipboard_text_allowed(&params.text) {
                    log::warn!(
                        "[dd-gui] host/set_clipboard 已拒绝（{} 字节超上限 {MAX_CLIPBOARD_BYTES}）ext={ext_id}",
                        params.text.len()
                    );
                    let msg = self.tr("toast.clipboard_oversize").replace("{id}", ext_id);
                    self.show_toast(msg, Some(2_500));
                    return;
                }
                let len = params.text.len();
                // R-07：移交常驻工作线程——UI 线程不再 `spawn().join()` 等待
                // 写入（arboard 打开 Win32 剪贴板与剪贴板管理器争用时面板冻结）。
                // 结果由 [`Self::poll_clipboard_results`] 消费并 toast 反馈。
                match self.clipboard_tx.as_ref() {
                    Some(tx) => {
                        let _ = tx.send(crate::app::clipboard_worker::ClipboardRequest {
                            ext_id: ext_id.to_string(),
                            text: params.text,
                        });
                        log::debug!("[dd-gui] host/set_clipboard（ext={ext_id}）入队：{len} 字节");
                    }
                    None => {
                        // 工作线程创建失败（R-05 降级口径）——不静默，提示失败
                        log::warn!(
                            "[dd-gui] 剪贴板工作线程不可用，host/set_clipboard（ext={ext_id}）被丢弃"
                        );
                        let msg = self
                            .tr("toast.clipboard_fail")
                            .replace("{id}", ext_id)
                            .replace("{e}", "clipboard worker unavailable");
                        self.show_error_toast(msg);
                    }
                }
            }
            METHOD_HOST_OPEN_URL => {
                let Ok(params) = serde_json::from_value::<OpenUrlParams>(
                    msg.params.clone().unwrap_or(serde_json::Value::Null),
                ) else {
                    log::warn!("[dd-gui] host/open_url 参数解析失败（ext={ext_id}）");
                    return;
                };
                self.open_url_execute(&params.url, ext_id);
            }
            other => log::debug!("[dd-gui] 未知 host/* 请求：{other}（ext={ext_id}，已应答忽略）"),
        }
    }

    /// 打开 URL/文件目标的统一执行端（N1 自宿主 `host/open_url` 执行体抽出，
    /// 2026-10-04——自定义直达命令复用同一函数，行为逐位一致）。
    ///
    /// `source` 为溯源标识（扩展清单 id / `custom`）。口径：
    /// - S-03（2026-09-23）：scheme 白名单前置——只放行 `http` / `https` / `file`；
    ///   拒绝时**不静默**（warn + 一次性 toast）。
    /// - `file://` → ShellExecute「双击等价」（目录 → Explorer；文件 → 关联程序；
    ///   含 UNC 与候选 + 存在性优选，v3.3 P2 §9.6）。
    /// - 其余（http/https）→ `webbrowser::open` 默认浏览器。
    /// - 失败不静默（R-16 toast）。
    pub(crate) fn open_url_execute(&mut self, url: &str, source: &str) {
        if !crate::platform::is_allowed_open_url(url) {
            log::warn!("[dd-gui] host/open_url 已拦截（scheme 不在白名单）src={source} url={url}");
            let msg = crate::text::t(self.lang_effective, "toast.open_url_blocked");
            self.show_toast(msg.to_string(), Some(2_500));
            return;
        }
        log::debug!("[dd-gui] host/open_url（src={source}）：{url}");
        if let Some(path) = crate::platform::resolve_file_url_to_path(url) {
            // S-03：`file://` = 「双击等价」（设计如此，文件搜索的打开动作依赖它）。
            // 记 info 落**来源 + 目标路径**——该能力无法由宿主验证"是否用户手势"，
            // 故至少保证可溯源（EDR/日志归因）。见审计文档 §4.2。
            log::info!("[dd-gui] host/open_url file:// 打开（src={source}, path={path}）");
            if let Err(e) = crate::platform::open_path(&path) {
                // R-16：失败不静默——面板多已 Dismiss，无提示即「命令被吃了」。
                log::warn!(
                    "[dd-gui] host/open_url ShellExecute 失败（src={source}, path={path}）：{e}"
                );
                let msg = self.tr("toast.open_fail").replace("{e}", &e);
                self.show_error_toast(msg);
            }
        } else if let Err(e) = webbrowser::open(url) {
            // R-16：同上——无默认浏览器 / 启动失败必须可见。
            log::warn!("[dd-gui] host/open_url 浏览器打开失败（src={source}）：{e}");
            let msg = self.tr("toast.open_fail").replace("{e}", &e.to_string());
            self.show_error_toast(msg);
        }
    }

    /// N1（2026-10-04）：执行自定义直达命令（宿主内部分发，§3 惯例 ⑤）。
    ///
    /// `item_id` = `custom:{keyword}`（[`dd_gui::aggregator::CUSTOM_ITEM_PREFIX`]）。
    /// `url` 走 [`Self::open_url_execute`]（S-03 白名单天然生效）；`path` 走
    /// ShellExecute「双击等价」（与 `file://` 同一执行函数，含 UNC）。
    /// 配置中已无该关键词（删除后列表未刷新的竞态）→ 记日志忽略。
    pub(crate) fn run_custom_command(&mut self, item_id: &str) {
        let Some(keyword) = item_id.strip_prefix(dd_gui::aggregator::CUSTOM_ITEM_PREFIX) else {
            return;
        };
        let Some(cmd) = self
            .settings
            .custom_commands
            .iter()
            .find(|c| c.keyword == keyword)
            .cloned()
        else {
            log::warn!("[dd-gui] 自定义命令 {item_id} 已不存在（配置已删除），忽略执行");
            return;
        };
        match cmd.kind {
            dd_gui::settings::CustomCommandKind::Url => {
                self.open_url_execute(&cmd.target, "custom");
            }
            dd_gui::settings::CustomCommandKind::Path => {
                // 与 file:// 腿同一「双击等价」执行函数（ShellExecute，含 UNC）；
                // info 落目标路径可溯源（同 S-03 §4.2 口径）。
                log::info!(
                    "[dd-gui] 自定义命令 path 打开（keyword={}, path={}）",
                    cmd.keyword,
                    cmd.target
                );
                if let Err(e) = crate::platform::open_path(&cmd.target) {
                    log::warn!(
                        "[dd-gui] 自定义命令 ShellExecute 失败（keyword={}，path={}）：{e}",
                        cmd.keyword,
                        cmd.target
                    );
                    let msg = self.tr("toast.open_fail").replace("{e}", &e);
                    self.show_error_toast(msg);
                }
            }
        }
    }

    /// N1（2026-10-04）：右键菜单「删除」自定义直达命令——从配置移除 + 落盘 +
    /// 置聚合脏标记（离开设置页/下一帧重聚合后条目消失；此处面板尚在 Root 页，
    /// `engines_dirty` 的重聚合由 `ui()` 收口点消费）。
    pub(crate) fn delete_custom_command(&mut self, keyword: &str) {
        self.settings
            .custom_commands
            .retain(|c| c.keyword != keyword);
        self.save_settings_with_feedback();
        self.engines_dirty = true; // 重聚合消费点复用（N1：含自定义命令变更）
        log::debug!("[dd-gui] 自定义命令已删除：{keyword}");
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
                self.go_back_focused(); // 返回 + 聚焦回落后页面的搜索框
            }
            HostAction::KeepOpen => {}
            HostAction::GoToPage { page_id } => {
                let ext_id = ext_id.to_string();
                // 扩展主动 `GoToPage`：没有「被点击项标题」可用，页标题留空
                // → placeholder 回落「筛选命令…」（不把原始 page_id 显示给用户）。
                self.open_page(&ext_id, &page_id, None, None, None);
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

impl PaletteApp {
    /// R-07：消费剪贴板工作线程的写入结果——成功沿用 S-07 既有口径
    ///（info 日志 + 轻量 toast），失败经 `show_error_toast` 反馈（与 R-16
    /// 同一套失败提示基建，i18n 键 `toast.clipboard_fail`）。
    /// 在 `poll_host_requests` 同帧调用（ui 循环，见 app/mod.rs）。
    pub(crate) fn poll_clipboard_results(&mut self) {
        while let Ok(outcome) = self.clipboard_rx.try_recv() {
            match outcome {
                crate::app::clipboard_worker::ClipboardOutcome::Written { ext_id, len } => {
                    // S-07：不再静默——info 落**扩展 id + 长度**（可溯源），
                    // 并给一次轻量 toast（覆盖用户复制中的账号/地址前可见）。
                    log::info!("[dd-gui] host/set_clipboard（ext={ext_id}）成功：{len} 字节");
                    let msg = self.tr("toast.clipboard_written").replace("{id}", &ext_id);
                    self.show_toast(msg, Some(2_000));
                }
                crate::app::clipboard_worker::ClipboardOutcome::Failed { ext_id, error } => {
                    log::warn!("[dd-gui] host/set_clipboard（ext={ext_id}）失败：{error}");
                    let msg = self
                        .tr("toast.clipboard_fail")
                        .replace("{id}", &ext_id)
                        .replace("{e}", &error);
                    self.show_error_toast(msg);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// S-07：1 MiB 以内放行。
    #[test]
    fn clipboard_text_allows_normal_and_boundary_sizes() {
        assert!(clipboard_text_allowed(""));
        assert!(clipboard_text_allowed("hello 剪贴板"));
        // 恰好等于上限：放行（≤ 判据）
        assert!(clipboard_text_allowed(&"a".repeat(MAX_CLIPBOARD_BYTES)));
    }

    /// S-07：超限拒绝（验收判据「超长文本被拒绝」）。
    #[test]
    fn clipboard_text_rejects_oversize() {
        assert!(!clipboard_text_allowed(
            &"a".repeat(MAX_CLIPBOARD_BYTES + 1)
        ));
        // 多字节字符按 UTF-8 字节数计（10 万个汉字 ≈ 300 KB，放行；
        // 40 万个汉字 ≈ 1.2 MB，拒绝）
        assert!(clipboard_text_allowed(&"账".repeat(100_000)));
        assert!(!clipboard_text_allowed(&"账".repeat(400_000)));
    }
}
