//! `items_changed` 合并刷新调度与通知轮询。

use crate::app::PaletteApp;
use std::time::Duration;
use std::time::Instant;

/// §6.3 + A9：`items_changed` 通知的合并窗口（窗口内多次通知只重拉一次）。
pub(crate) const REFRESH_WINDOW: Duration = Duration::from_millis(100);

/// 嵌套页「页内二次输入」的去抖窗口（v3.3 边打边搜）：停止输入该时长后
/// 才按最新 query 重拉 `get_items`——避免每个按键都启动扩展搜索（Everything
/// 单次 <50ms 但 IPC 仍有开销；`f ` 前缀自动进页不受影响，仍即时拉取）。
pub(crate) const PAGE_QUERY_DEBOUNCE: Duration = Duration::from_millis(200);

/// `items_changed` 的合并刷新调度。
pub(crate) struct RefreshState {
    pub(crate) page_id: String,
    pub(crate) ready_at: Instant,
    /// 是否为顶层（`items_changed` 无 page_id）刷新：到期时触发 Root 全量重聚合
    ///（`restart_aggregation`）而非单页重拉（§6.3 + A9）。
    pub(crate) top: bool,
}

impl PaletteApp {
    /// `items_changed` 通知轮询：命中当前页则进入 100ms 合并窗口；顶层变更
    /// 同样进入合并窗口，到期触发 Root 全量重聚合。同时收集扩展发来的 `host/*`
    /// 请求（M4 P2）——执行端见 [`Self::poll_host_requests`]。
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
                    // `None` = 顶层命令变了 → 进合并窗口，到期 Root 全量重聚合（A9）
                    None => top_changed = true,
                    Some(pid) if Some(pid.as_str()) == current.as_deref() => hit = Some(pid),
                    Some(_) => {}
                }
            }
        }
        if top_changed {
            eprintln!(
                "[dd-gui] 收到顶层 items_changed → {}ms 后 Root 全量重聚合（A9）",
                REFRESH_WINDOW.as_millis()
            );
            // 进入 100ms 合并窗口（与页级同口径）。已存在页级刷新则升级为顶层
            // （顶层重聚合包含页重拉，覆盖更全）；已为顶层则幂等不变。
            if self.refresh.is_none() {
                self.refresh = Some(RefreshState {
                    page_id: String::new(),
                    ready_at: Instant::now() + REFRESH_WINDOW,
                    top: true,
                });
            } else if let Some(r) = self.refresh.as_mut() {
                r.top = true;
            }
            self.show_toast(self.tr("toast.cmd_updated"), Some(1_500));
        }
        if let Some(pid) = hit {
            eprintln!(
                "[dd-gui] 收到 items_changed page={pid} → {}ms 后全量重拉",
                REFRESH_WINDOW.as_millis()
            );
            // 仅当尚无待处理刷新时记录（顶层优先：已有顶层刷新不降级为页级）。
            if self.refresh.is_none() {
                self.refresh = Some(RefreshState {
                    page_id: pid,
                    ready_at: Instant::now() + REFRESH_WINDOW,
                    top: false,
                });
            }
        }
    }
}

impl PaletteApp {
    /// 合并窗口到期：顶层刷新 → Root 全量重聚合；页级刷新 → 重拉当前页
    ///（**全量**，协议层无增量推送）。
    pub(crate) fn tick_refresh(&mut self) {
        // 先处理 items_changed 合并刷新（原有语义，优先级更高——它是扩展侧的显式通知）
        if let Some(refresh) = &self.refresh {
            if Instant::now() >= refresh.ready_at {
                let top = refresh.top;
                let page_id = refresh.page_id.clone();
                self.refresh = None;
                if top {
                    eprintln!("[dd-gui] 顶层 items_changed 合并窗口到期 → Root 全量重聚合（A9）");
                    self.restart_aggregation();
                } else {
                    self.refetch_page_if_current(&page_id);
                }
            }
        }
        // 再去抖处理嵌套页「页内二次输入」：到期用当前页最新 query 重拉
        if let Some(due) = &self.page_query_debounce {
            if Instant::now() >= *due {
                self.page_query_debounce = None;
                eprintln!("[dd-gui] 页内二次输入去抖到期 → 按最新 query 重拉当前页（v3.3 边打边搜）");
                let page = self.stack.current();
                if let Some(page_id) = page.page_id.clone() {
                    self.refetch_page_if_current(&page_id);
                }
            }
        }
    }

    /// 页级刷新（合并窗口到期 / 页内输入去抖到期）统一入口：目标页仍是当前页时
    /// 用该页当前 query 重拉 `get_items`。
    fn refetch_page_if_current(&mut self, page_id: &str) {
        let page = self.stack.current();
        // 用户可能已离开通知来源页（如已 GoBack）→ 目标页非当前页时丢弃，
        // 避免拉取一个不可见的页（结果也只会被 poll_page 作废）。
        if page.page_id.as_deref() != Some(page_id) {
            eprintln!("[dd-gui] 页级刷新作废：已离开 page={page_id}");
            return;
        }
        let (ext_id, query) = (page.ext_id.clone(), page.list.query().to_owned());
        if ext_id.is_empty() {
            return;
        }
        let search = (!query.is_empty()).then_some(query);
        // M3：warm 直发 / 进程被驱逐则走复热；`command_id=None`（刷新非命令点击）
        self.dispatch_fetch_page(&ext_id, page_id, search, None);
    }

    /// 调度嵌套页「页内二次输入」的去抖重拉：由 [`draw_searchbar`] 检测到页内
    /// query 变化后调用（见 panel.rs）。仅在有实际查询文本、且页面列表非空（非加载态）
    /// 时生效；连续输入会刷新到期时刻（天然去抖）。
    pub(crate) fn schedule_page_query_debounce(&mut self) {
        // 仅嵌套页的输入共生（Root 的输入走 fallback/聚合，不在此列）
        let page = self.stack.current();
        if page.page_id.is_none() || page.is_settings {
            return;
        }
        if page.is_loading {
            return; // 正在拉取：等待落地后自然再比较，避免叠加
        }
        if !self.aggregating {
            self.page_query_debounce = Some(Instant::now() + PAGE_QUERY_DEBOUNCE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::make_app;

    /// L3（M6.4）：顶层 `items_changed`（合并窗口到期）应触发 Root 全量重聚合，
    /// 而非仅弹 Toast。验证 `tick_refresh` 顶层分支正确调用 `restart_aggregation`。
    #[test]
    fn tick_refresh_top_triggers_restart_aggregation() {
        let mut app = make_app();
        app.aggregating = false; // 明确初始态，验证顶层分支确实触发重聚合
                                 // 模拟已收到顶层 items_changed 且合并窗口已到期
        app.refresh = Some(RefreshState {
            page_id: String::new(),
            ready_at: Instant::now(),
            top: true,
        });
        app.tick_refresh();
        assert!(
            app.aggregating,
            "顶层 items_changed 到期应触发 Root 全量重聚合（restart_aggregation）"
        );
        assert!(
            app.aggregate_rx.is_some(),
            "重聚合应建立聚合结果接收端（poll_aggregate 落地新 Root）"
        );
    }

    /// 页级刷新到期走原单页重拉路径，不触发全量重聚合（与顶层分支区分）。
    #[test]
    fn tick_refresh_page_does_not_restart_aggregation() {
        let mut app = make_app();
        app.aggregating = false; // 明确初始态，验证页级分支不触发重聚合
        app.refresh = Some(RefreshState {
            page_id: "some.page".to_string(),
            ready_at: Instant::now(),
            top: false,
        });
        app.tick_refresh();
        assert!(
            !app.aggregating,
            "页级 items_changed 刷新不应触发全量重聚合（走 dispatch_fetch_page）"
        );
    }
}
