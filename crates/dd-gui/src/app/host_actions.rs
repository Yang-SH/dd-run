//! 扩展宿主请求（ShowToast/Clipboard/OpenUrl 等）与 8 种结果裁决落地。

use crate::app::toast_state::ConfirmDialog;
use crate::app::PaletteApp;
use crate::result;
use crate::result::HostAction;
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

// ── N14：Ctrl+Enter 分派（纯逻辑，单测锚点）────────────────────────────

/// N14：Ctrl+Enter 的分派动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CtrlEnterKind {
    /// 打开所在位置（`explorer /select`——右键「打开所在位置」同款平台动作）。
    Reveal(String),
    /// 复制文本（URL 副标题 → 链接；其余 → 标题文本回落）。
    Copy(String),
}

/// N14：按副标题形态分派——**形状门控**（真机核对轮 2026-11-22 修正：原实现
/// 按「类别 ∈ {应用, 文件}」门控 Reveal，但文件搜索页行 `result_category`
/// 缺省（sidecar 只设 section）、N10 直达行类别为「直达」，两者按旧门控误落
/// Copy(标题)，与设计 §4.9「应用/文件/路径直达行 = 打开所在位置」不符）。
/// 路径形态本身即充分判据：副标题为盘符/UNC 路径 → 定位；URL → 复制链接；
/// 其余 → 复制标题回落。纯函数可单测。
pub(crate) fn ctrl_enter_kind(subtitle: &str, title: &str) -> CtrlEnterKind {
    if let Some(path) = crate::text::path_like(subtitle) {
        return CtrlEnterKind::Reveal(path);
    }
    let text = crate::text::url_like(subtitle).unwrap_or_else(|| title.to_string());
    CtrlEnterKind::Copy(text)
}

impl PaletteApp {
    /// M4 P2：消费扩展的 `host/*` 请求并执行真实副作用（协议 §7.2–§7.4）。
    /// `host/show_status` → Toast；`host/set_clipboard` → 剪贴板；`host/open_url` → 浏览器。
    /// 应答由 dd-host 完成（§7.4 能力前置：未声明回 `-32601`），此处只做执行端。
    pub(crate) fn poll_host_requests(&mut self) {
        if self.pool.processes.is_empty() {
            return;
        }
        // 收集期间只借用 `self.pool.processes`；执行副作用需要 `&mut self`，故先取走再统一处理。
        let requests: Vec<(String, RawMessage)> = self
            .pool
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
                match self.view.clipboard_tx.as_ref() {
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
            let msg = crate::text::t(self.view.lang_effective, "toast.open_url_blocked");
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
    /// `item_id` = `custom:{keyword}`（[`crate::aggregator::CUSTOM_ITEM_PREFIX`]）。
    /// `url` 走 [`Self::open_url_execute`]（S-03 白名单天然生效）；`path` 走
    /// ShellExecute「双击等价」（与 `file://` 同一执行函数，含 UNC）。
    /// 配置中已无该关键词（删除后列表未刷新的竞态）→ 记日志忽略。
    pub(crate) fn run_custom_command(&mut self, item_id: &str) {
        // N10：URL/路径直达条目（`direct:<目标>`）——执行点重分类（目标内嵌
        // id，配置无关、无删除竞态面）；URL 走 S-03 白名单，路径走「双击等价」。
        if let Some(target) = item_id.strip_prefix(crate::aggregator::DIRECT_ITEM_PREFIX) {
            self.run_direct_item(target);
            return;
        }
        let Some(keyword) = item_id.strip_prefix(crate::aggregator::CUSTOM_ITEM_PREFIX) else {
            return;
        };
        // N19：条目 id 携带**主关键词**（第一段）——多关键词命令按主键匹配。
        let Some(cmd) = self
            .draft
            .settings
            .custom_commands
            .iter()
            .find(|c| c.primary_keyword() == keyword)
            .cloned()
        else {
            log::warn!("[dd-gui] 自定义命令 {item_id} 已不存在（配置已删除），忽略执行");
            return;
        };
        match cmd.kind {
            crate::settings::CustomCommandKind::Url => {
                self.open_url_execute(&cmd.target, "custom");
            }
            crate::settings::CustomCommandKind::Path => {
                // N19：失效检测——target 不存在 → 拒绝执行（toast 与聚合条目
                // ⚠ 副标题同口径；避免 ShellExecute 盲打无效路径）。
                if !std::path::Path::new(&cmd.target).exists() {
                    self.show_error_toast(
                        self.tr("n19.path_missing_invoke")
                            .replace("{target}", &cmd.target),
                    );
                    return;
                }
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
    /// `engines_dirty` 的重聚合由 `ui()` 收口点消费）。N19：keyword 形态两可
    /// ——主关键词（右键 id 派生）或全串（设置卡行派生）均命中。
    pub(crate) fn delete_custom_command(&mut self, keyword: &str) {
        self.draft
            .settings
            .custom_commands
            .retain(|c| c.keyword != keyword && c.primary_keyword() != keyword);
        self.save_settings_with_feedback();
        self.draft.engines_dirty = true; // 重聚合消费点复用（N1：含自定义命令变更）
        log::debug!("[dd-gui] 自定义命令已删除：{keyword}");
    }

    /// N10（2026-11-21）：执行直达条目目标——与聚合层同一 `url_like` /
    /// `path_like` 重分类；URL 走 [`Self::open_url_execute`]（`info=direct`
    /// 溯源），路径走 `platform::open_path`「双击等价」（含 UNC）。形态失效
    /// （列表未刷新的陈旧条目）→ 记日志忽略。
    fn run_direct_item(&mut self, target: &str) {
        if let Some(url) = crate::text::url_like(target) {
            self.open_url_execute(&url, "direct");
            return;
        }
        if let Some(path) = crate::text::path_like(target) {
            // 与 file:// 腿同一「双击等价」执行函数（ShellExecute，含 UNC）
            match crate::platform::open_path(&path) {
                Ok(()) => log::info!("[dd-gui] 直达条目 path 打开（target={path}）"),
                Err(e) => {
                    let msg = self.tr("toast.open_fail").replace("{e}", &e);
                    self.show_error_toast(msg);
                }
            }
            return;
        }
        log::warn!("[dd-gui] 直达条目目标形态失效（{target}），忽略执行");
    }

    /// N13（2026-11-21）：右键菜单「屏蔽此应用」——把显示名追加进 apps 用户
    /// 屏蔽名单（N2 通道，输出层过滤、下一次重聚合生效），落盘 + 置聚合脏
    /// 标记（镜像 [`Self::delete_custom_command`]）。含逗号的显示名拒绝（会
    /// 破坏逗号分隔片段语义——引导走设置页编辑器）；已在名单中则幂等跳过。
    pub(crate) fn hide_app_in_blocklist(&mut self, title: &str) {
        const APPS_EXT_ID: &str = "com.ddrun.apps";
        let name = title.trim();
        if name.is_empty() {
            return;
        }
        if name.contains(',') {
            self.show_error_toast(self.tr("ctx.hide_app_comma").to_string());
            return;
        }
        let inner = self
            .draft
            .settings
            .ext_settings
            .entry(APPS_EXT_ID.to_string())
            .or_default();
        let current = inner.get("blocklist").cloned().unwrap_or_default();
        let Some(new_list) = append_blocklist_fragment(&current, name) else {
            // 已在名单：名单匹配为大小写不敏感片段包含（N2 口径），幂等跳过
            self.show_toast(self.tr("ctx.hide_app_done").replace("{title}", name), None);
            return;
        };
        inner.insert("blocklist".to_string(), new_list);
        self.save_settings_with_feedback();
        self.draft.engines_dirty = true; // 重聚合消费点复用（输出层过滤即时生效）
        self.show_toast(self.tr("ctx.hide_app_done").replace("{title}", name), None);
        log::debug!("[dd-gui] 应用已加入屏蔽名单：{name}");
    }

    /// N11（2026-11-22）：置顶/取消置顶切换——pinned_keys 幂等切换（达上限
    /// 淘汰最旧）+ 落盘 + 重聚合脏标记（下一次重聚合后置顶分组即时生效）。
    pub(crate) fn toggle_pinned(&mut self, key: String) {
        let added = crate::settings::toggle_pinned_key(&mut self.draft.settings.pinned_keys, &key);
        self.save_settings_with_feedback();
        self.draft.engines_dirty = true;
        self.show_toast(
            self.tr(if added {
                "toast.pinned"
            } else {
                "toast.unpinned"
            }),
            None,
        );
        log::debug!("[dd-gui] 置顶切换：{key} → {added}");
    }

    /// N11：打开最近文件条目（`recent:<路径>` → `platform::open_path` 双击
    /// 等价；失效/失败 → 既有错误 toast——方案 §4.6「失效 lnk 走既有失败
    /// toast」口径）。
    pub(crate) fn open_recent_file(&mut self, path: &str) {
        match crate::platform::open_path(path) {
            Ok(()) => log::debug!("[dd-gui] 打开最近文件：{path}"),
            Err(e) => self.show_error_toast(self.tr("toast.recent_open_fail").replace("{e}", &e)),
        }
    }

    /// N18：复制片段到剪贴板（R-07 工作线程队列——UI 线程不阻塞；结果 toast
    /// 由 poll_clipboard_results 统一反馈；S-07 上限语义沿用）。
    pub(crate) fn copy_snippet(&mut self, keyword: &str) {
        let Some(sn) = self
            .draft
            .settings
            .snippets
            .iter()
            .find(|x| x.keyword == keyword)
            .cloned()
        else {
            log::warn!("[dd-gui] 片段 {keyword} 已不存在（配置已删除），忽略复制");
            return;
        };
        match self.view.clipboard_tx.as_ref() {
            Some(tx) => {
                let _ = tx.send(crate::app::clipboard_worker::ClipboardRequest {
                    ext_id: crate::aggregator::HOST_CUSTOM_EXT_ID.to_string(),
                    text: sn.text,
                });
            }
            None => {
                self.show_error_toast(
                    self.tr("toast.clipboard_fail")
                        .replace("{id}", "snippet")
                        .replace("{e}", "clipboard worker unavailable"),
                );
            }
        }
    }

    /// N19（2026-11-21）：一键转自定义命令——把来源条目的 title/target/kind
    /// **预填**进设置卡输入缓冲（不静默写配置；关键词留空由用户填写）。
    pub(crate) fn prefill_custom_command(
        &mut self,
        title: &str,
        target: &str,
        kind: crate::settings::CustomCommandKind,
    ) {
        self.draft.custom_title_buf = title.to_string();
        self.draft.custom_target_buf = target.to_string();
        self.draft.custom_keyword_buf.clear();
        self.draft.custom_add_err = None;
        self.draft.custom_kind_idx = match kind {
            crate::settings::CustomCommandKind::Url => 0,
            crate::settings::CustomCommandKind::Path => 1,
        };
        log::debug!("[dd-gui] 自定义命令预填：title={title} target={target}");
    }

    /// N14（2026-11-21）：Ctrl+Enter——选中项按 [`ctrl_enter_kind`] 分派：
    /// 路径形态副标题 → 打开所在位置（错误 toast 复用 `ctx.locate_fail`）；
    /// URL → 复制链接；其余 → 复制文本（toast 复用 `ctx.copied`）——右键菜单
    /// 同款动作的键盘可达路径。
    pub(crate) fn ctrl_enter_selected(&mut self, ctx: &egui::Context) {
        let Some(item) = self.view.stack.current().list.selected_item().cloned() else {
            return;
        };
        match ctrl_enter_kind(&item.subtitle, &item.title) {
            CtrlEnterKind::Reveal(path) => match crate::platform::reveal_in_folder(&path) {
                Ok(()) => log::debug!("[dd-gui] Ctrl+Enter 定位：{path}"),
                Err(e) => self.show_error_toast(self.tr("ctx.locate_fail").replace("{e}", &e)),
            },
            CtrlEnterKind::Copy(text) => {
                ctx.copy_text(text);
                self.show_toast(self.tr("ctx.copied"), None);
            }
        }
    }
}

/// N13 纯逻辑：把显示名追加进逗号分隔的屏蔽名单片段串。
/// 返回 `Some(新串)` = 已追加；`None` = 已存在（大小写不敏感等值，幂等跳过）。
/// 调用方先做逗号拒绝（片段分隔语义由逗号承载，本函数不再校验）。
fn append_blocklist_fragment(current: &str, name: &str) -> Option<String> {
    let mut parts: Vec<String> = current
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    if parts.iter().any(|p| p.eq_ignore_ascii_case(name)) {
        return None;
    }
    parts.push(name.to_string());
    Some(parts.join(", "))
}

impl PaletteApp {
    /// 应用 8 种 Kind 裁决出的宿主动作（A4）。
    pub(crate) fn apply_action(&mut self, ctx: &egui::Context, action: HostAction, ext_id: &str) {
        match action {
            HostAction::Dismiss => self.dismiss(ctx),
            HostAction::Hide => self.hide_keep_state(ctx),
            HostAction::GoHome => self.view.stack.go_home(),
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
                let command_id = self.pool.last_command_id.clone().unwrap_or_default();
                let pending =
                    result::pending_confirm_for(&command_id, self.pool.last_invoke.as_ref());
                self.overlay.confirm = Some(ConfirmDialog {
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
        while let Ok(outcome) = self.view.clipboard_rx.try_recv() {
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

    /// N13：屏蔽名单片段追加——追加 / 去重（大小写不敏感幂等） / 空名单与
    /// 空片段容忍（含逗号的拒绝在调用方，见 `hide_app_in_blocklist`）。
    #[test]
    fn append_blocklist_fragment_appends_dedups_and_tolerates() {
        assert_eq!(
            append_blocklist_fragment("", "7-Zip").as_deref(),
            Some("7-Zip"),
            "空名单首条"
        );
        assert_eq!(
            append_blocklist_fragment("卸载, update", "7-Zip").as_deref(),
            Some("卸载, update, 7-Zip"),
            "既有片段保留、新片段追加"
        );
        assert_eq!(
            append_blocklist_fragment("7-Zip", "7-zip"),
            None,
            "大小写不敏感幂等"
        );
        assert_eq!(
            append_blocklist_fragment("a ,, b", "c").as_deref(),
            Some("a, b, c"),
            "空片段与多余逗号被规整"
        );
    }

    /// N14：Ctrl+Enter 分派——路径形态副标题 → Reveal（**形状门控**：应用/
    /// 文件/直达/类别缺省行一视同仁）；URL 副标题 → Copy(链接)；无路径形态
    /// 应用 → Copy(标题回落)；普通文本行 → Copy(标题)。
    #[test]
    fn n14_ctrl_enter_kind_dispatch() {
        use CtrlEnterKind as K;
        assert!(matches!(
            ctrl_enter_kind(r"C:\x\y.exe", "App"),
            K::Reveal(_)
        ));
        assert!(matches!(
            ctrl_enter_kind(r"G:\a\报告.txt", "报告"),
            K::Reveal(_)
        ));
        // 真机核对轮修正回归：类别缺省（文件搜索页行）与「直达」类别（N10 行）
        // 携路径副标题 → Reveal（旧类别门控误落 Copy）
        assert!(
            matches!(
                ctrl_enter_kind(r"C:\Users\me\report.txt", "报告"),
                K::Reveal(_)
            ),
            "类别缺省（文件搜索页行）路径 → Reveal"
        );
        assert!(
            matches!(
                ctrl_enter_kind(r"C:\Windows", "打开 C:\\Windows"),
                K::Reveal(_)
            ),
            "直达行路径 → Reveal"
        );
        assert_eq!(
            ctrl_enter_kind("https://example.com", "标题"),
            K::Copy("https://example.com".into()),
            "URL 副标题复制链接"
        );
        assert_eq!(
            ctrl_enter_kind("", "无路径应用"),
            K::Copy("无路径应用".into()),
            "无路径回落复制标题"
        );
        assert_eq!(
            ctrl_enter_kind("计算结果 4", "= 4"),
            K::Copy("= 4".into()),
            "普通文本复制标题"
        );
    }
}
