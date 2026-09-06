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
            self.show_toast("扩展命令已更新", Some(1_500));
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
        let Some(refresh) = &self.refresh else {
            return;
        };
        if Instant::now() < refresh.ready_at {
            return;
        }
        // 顶层刷新（items_changed 无 page_id）：到期触发 Root 全量重聚合（A9）。
        // 只替换 root，嵌套页不受影响；不依赖当前页（与页级分支脱离）。
        if refresh.top {
            self.refresh = None;
            eprintln!("[dd-gui] 顶层 items_changed 合并窗口到期 → Root 全量重聚合（A9）");
            self.restart_aggregation();
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
