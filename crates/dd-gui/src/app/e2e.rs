//! E2E 首屏计时（A-33-05 感知指标「输入 → 首屏 ≤ 200 ms」的宿主侧埋点）。
//!
//! **口径**（验收报告 §5 #4 此前只有扩展侧往返，无 GUI 段）：
//!
//! | 段 | 起点 | 终点 | 含义 |
//! |---|---|---|---|
//! | 等待 | 页内 query 最后一次输入变化（`panel.rs` 变化检测 / 带词进页回填） | `get_items` 实际分派 | 去抖窗口 + 调度间隙 |
//! | 往返 | 分派 | 结果落地（非过期分支） | 后台线程 + IPC / 协议往返 |
//! | 渲染 | 落地 | 本帧 `draw_panel` 完成 | egui 即时模式：落地与绘制同帧 |
//!
//! 「首帧」= 落地所在帧的 `draw_panel` 完成时刻——**上界**：不含 egui 呈现 /
//! 垂直同步（swapchain present 无法在不挂钩渲染后端的前提下测得）。
//! 每次**成功落地**结算一条 debug 日志（`E2E 首屏:`），会话统计（p50/p95 与
//! 200 ms 门禁判定）由 `tools/gui_e2e_parse.py` 对捕获的 stderr 计算。
//!
//! 埋点为常驻 `log::debug!`，**不加** `#[cfg(debug_assertions)]`——感知指标属
//! release 真机口径（A3 教训：守卫必须连日志调用一起加；这里干脆不加守卫，
//! 常态成本 = 3 个 `Option<Instant>` 字段 + 每次落地一条日志，可忽略）。
//! 已知边界：样本结算若遇面板隐藏，会在隐藏帧（或下次可见帧）结算，总时长
//! 含隐藏期——判定时按 `items` / query 上下文甄别离群值。

use std::time::Instant;

use crate::app::PaletteApp;

/// 一次「输入 → 首屏」样本（成功落地时构建，本帧绘制完成时结算）。
pub(crate) struct E2eSample {
    pub(crate) page_id: String,
    /// 页内 query 字符数（落地时快照）。
    pub(crate) query_chars: usize,
    pub(crate) items: usize,
    pub(crate) input_at: Instant,
    pub(crate) dispatch_at: Instant,
    pub(crate) landed_at: Instant,
}

impl E2eSample {
    /// 三段耗时（毫秒）：`(去抖/进页等待, get_items 往返, 落地→绘制完成)`。
    /// 纯函数，单测锚点。
    pub(crate) fn breakdown_ms(&self, painted_at: Instant) -> (u128, u128, u128) {
        (
            self.dispatch_at
                .saturating_duration_since(self.input_at)
                .as_millis(),
            self.landed_at
                .saturating_duration_since(self.dispatch_at)
                .as_millis(),
            painted_at
                .saturating_duration_since(self.landed_at)
                .as_millis(),
        )
    }

    /// 总耗时（输入 → 绘制完成）。
    pub(crate) fn total_ms(&self, painted_at: Instant) -> u128 {
        painted_at
            .saturating_duration_since(self.input_at)
            .as_millis()
    }
}

impl PaletteApp {
    /// 帧尾结算：本帧 `draw_panel` 已把落地样本绘制完成 → 输出一条 E2E 日志。
    /// 无待结算样本时零成本早退。
    pub(crate) fn e2e_report(&mut self) {
        let Some(s) = self.e2e_pending.take() else {
            return;
        };
        let painted_at = Instant::now();
        let (wait, rtt, paint) = s.breakdown_ms(painted_at);
        let total = s.total_ms(painted_at);
        log::debug!(
            "[dd-gui] E2E 首屏: input→paint {total} ms（去抖/进页等待 {wait} + get_items 往返 {rtt} + 渲染 {paint}）；page={} query={} 字符 items={}",
            s.page_id,
            s.query_chars,
            s.items
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sample() -> E2eSample {
        let t0 = Instant::now();
        E2eSample {
            page_id: "files.results".to_string(),
            query_chars: 3,
            items: 30,
            input_at: t0,
            dispatch_at: t0 + Duration::from_millis(200),
            landed_at: t0 + Duration::from_millis(230),
        }
    }

    /// 三段分解：等待 = 分派 − 输入，往返 = 落地 − 分派，渲染 = 绘制 − 落地。
    #[test]
    fn breakdown_splits_wait_rtt_paint() {
        let s = sample();
        let painted = s.input_at + Duration::from_millis(233);
        assert_eq!(
            s.breakdown_ms(painted),
            (200, 30, 3),
            "等待 200 + 往返 30 + 渲染 3"
        );
        assert_eq!(s.total_ms(painted), 233);
    }

    /// painted_at 不晚于 landed_at（不该发生）→ 各段饱和为 0，不 panic。
    #[test]
    fn breakdown_saturates_instead_of_panicking() {
        let s = sample();
        let (wait, rtt, paint) = s.breakdown_ms(s.input_at);
        assert_eq!((wait, rtt, paint), (200, 30, 0));
        assert_eq!(s.total_ms(s.input_at), 0);
    }

    /// 系统发起的刷新（无输入起点 → input_at == dispatch_at）等待段为 0。
    #[test]
    fn wait_is_zero_when_input_equals_dispatch() {
        let t0 = Instant::now();
        let s = E2eSample {
            page_id: "files.results".to_string(),
            query_chars: 0,
            items: 10,
            input_at: t0,
            dispatch_at: t0,
            landed_at: t0 + Duration::from_millis(25),
        };
        let (wait, rtt, paint) = s.breakdown_ms(s.landed_at + Duration::from_millis(1));
        assert_eq!((wait, rtt, paint), (0, 25, 1));
    }
}
