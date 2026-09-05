//! `items_changed` 合并刷新调度与通知轮询。

use crate::app::PaletteApp;
use std::time::Duration;
use std::time::Instant;

/// §6.3 + A9：`items_changed` 通知的合并窗口（窗口内多次通知只重拉一次）。
pub(crate) const REFRESH_WINDOW: Duration = Duration::from_millis(100);

/// `items_changed` 的合并刷新调度。
pub(crate) struct RefreshState {
    pub(crate) page_id: String,
    pub(crate) ready_at: Instant,
}

impl PaletteApp {
    /// `items_changed` 通知轮询：命中当前页则进入 100ms 合并窗口。
    /// 同时收集扩展发来的 `host/*` 请求（M4 P2）——执行端见 [`Self::poll_host_requests`]。
    pub(crate) fn poll_notifications(&mut self) {
        if self.processes.is_empty() {
            return;
        }
        let current = self.stack.current().page_id.clone();
        let mut hit: Option<String> = None;
        let mut top_changed = false;
        // 注意：循环期间只借用 `self.processes`，不调用 `self.show_toast`
        // （后者可变借用整个 `self`，会与此处冲突）。
        for (_, proc) in self.processes.iter_mut() {
            for changed in proc.poll_notifications() {
                match changed {
                    // `None` = 顶层命令变了（当前仅提示，见 m2-record.md §5）
                    None => top_changed = true,
                    Some(pid) if Some(pid.as_str()) == current.as_deref() => hit = Some(pid),
                    Some(_) => {}
                }
            }
        }
        if top_changed {
            eprintln!("[dd-gui] 收到顶层 items_changed（Root 重聚合属遗留，仅提示）");
            self.show_toast("扩展命令已更新", Some(1_500));
        }
        if let Some(pid) = hit {
            eprintln!(
                "[dd-gui] 收到 items_changed page={pid} → {}ms 后全量重拉",
                REFRESH_WINDOW.as_millis()
            );
            if self.refresh.is_none() {
                self.refresh = Some(RefreshState {
                    page_id: pid,
                    ready_at: Instant::now() + REFRESH_WINDOW,
                });
            }
        }
    }
}

impl PaletteApp {
    /// 合并窗口到期：重拉当前页（**全量**，协议层无增量推送）。
    pub(crate) fn tick_refresh(&mut self) {
        let Some(refresh) = &self.refresh else {
            return;
        };
        if Instant::now() < refresh.ready_at {
            return;
        }
        let page_id = refresh.page_id.clone();
        self.refresh = None;

        let page = self.stack.current();
        // 用户可能已离开通知来源页（如已 GoBack）→ 目标页非当前页时丢弃，
        // 避免拉取一个不可见的页（结果也只会被 poll_page 作废）。
        if page.page_id.as_deref() != Some(page_id.as_str()) {
            eprintln!("[dd-gui] items_changed 刷新作废：已离开 page={page_id}");
            return;
        }
        let (ext_id, query) = (page.ext_id.clone(), page.list.query().to_owned());
        if ext_id.is_empty() {
            return;
        }
        let search = (!query.is_empty()).then_some(query);
        // M3：warm 直发 / 进程被驱逐则走复热；`command_id=None`（刷新非命令点击）
        self.dispatch_fetch_page(&ext_id, &page_id, search, None);
    }
}
