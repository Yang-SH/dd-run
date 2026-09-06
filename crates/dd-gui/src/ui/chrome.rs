//! 无边框窗口 chrome（设计稿 v4.10 D36）：全窗拖拽 + 8 方向边缘缩放。
//!
//! 根因：`with_decorations(false) + with_resizable(false)` 去掉原生标题栏后
//! 没有任何替代拖拽/缩放入口。本模块接管：
//!
//! - **拖拽**（v4.11 修正）：**不注册任何占屏交互 widget**——egui 0.36 的
//!   `interaction.rs` 中全屏 `Sense::drag()` widget 会在指针一移动即变成
//!   `dragged`，`is_decidedly_dragging()` 为真，释放时**抑制**其上方前台控件
//!   的 click（interaction.rs:174-182），表现为「能拖窗但按钮/列表/输入框
//!   全点不动」。现改为 `chrome_begin` 按帧手动检测：仅当主键按下落在
//!   「空白区」（不在缩放热区、且不在上一帧 `interactive_rects_last_pass()`
//!   任一 click/drag 控件矩形内）时记录候选起点，移动超 `DRAG_THRESHOLD` 再发
//!   `ViewportCommand::StartDrag`。前台控件 click 零干扰。
//! - **缩放**：`with_resizable(true)` 前置；8 分区纯函数 [`edge_zone`]
//!   （4 边 6px + 4 角 12px，角优先）hover 换光标、主键按下发
//!   `ViewportCommand::BeginResize(dir)`（egui 0.36 + winit Windows
//!   无边框窗口可用）。
//! - **守卫**：进入原生缩放模态循环后 egui 收不到输入事件，
//!   `native_resize` 旗标在 `primary_down()==false` 首帧清除，期间拖拽/
//!   缩放全部禁用（防模态循环排队重复命令）。
//! - **尺寸语义（D36 grill 决策）**：手动缩放仅本显示周期有效——
//!   `show()` 唤起重置栈顶页默认尺寸并仍居中光标屏（见 `lifecycle.rs`）。

use crate::app::PaletteApp;
use eframe::egui;

/// 边缘热区厚度（非角段，px）。
const EDGE: f32 = 6.0;
/// 角热区边长（px，角优先于边）。
const CORNER: f32 = 12.0;
/// 空白区拖拽起拖阈值（px）：主键按下后移动超过此距离才发起 `StartDrag`，
/// 避免把「点空白」误判为拖拽（也避免与任何潜在的空白点击手势冲突）。
const DRAG_THRESHOLD: f32 = 4.0;

/// 8 分区 hit-test（纯函数，可单测）：窗口矩形 + 指针位置 →
/// 缩放方向 + 对应光标。角 = 两轴都在 `CORNER` 内；边 = 单轴在 `EDGE` 内。
fn edge_zone(
    rect: egui::Rect,
    pos: egui::Pos2,
) -> Option<(egui::ResizeDirection, egui::CursorIcon)> {
    use egui::CursorIcon::{
        ResizeEast, ResizeNorth, ResizeNorthEast, ResizeNorthWest, ResizeSouth, ResizeSouthEast,
        ResizeSouthWest, ResizeWest,
    };
    use egui::ResizeDirection as D;
    if !rect.contains(pos) {
        return None;
    }
    let dl = pos.x - rect.left();
    let dr = rect.right() - pos.x;
    let dt = pos.y - rect.top();
    let db = rect.bottom() - pos.y;
    let near_w = dl < CORNER;
    let near_e = dr < CORNER;
    let near_n = dt < CORNER;
    let near_s = db < CORNER;
    // 角（两轴同时贴边，CORNER 范围，光标取对角斜向）
    if near_w && near_n {
        return Some((D::NorthWest, ResizeNorthWest));
    }
    if near_e && near_n {
        return Some((D::NorthEast, ResizeNorthEast));
    }
    if near_w && near_s {
        return Some((D::SouthWest, ResizeSouthWest));
    }
    if near_e && near_s {
        return Some((D::SouthEast, ResizeSouthEast));
    }
    // 边（单轴贴边，EDGE 范围——介于 EDGE 与 CORNER 之间是非热区）
    if dl < EDGE {
        Some((D::West, ResizeWest))
    } else if dr < EDGE {
        Some((D::East, ResizeEast))
    } else if dt < EDGE {
        Some((D::North, ResizeNorth))
    } else if db < EDGE {
        Some((D::South, ResizeSouth))
    } else {
        None
    }
}

/// 帧首调用（本帧任何控件注册之前）：清原生缩放旗标 + 判定空白区拖拽候选。
///
/// v4.11 修正：此前用 `Order::Background` 全屏 `allocate_rect(screen,
/// Sense::drag())` 注册拖拽 widget——egui 0.36 的 `interaction.rs` 中，
/// 全屏 drag widget 在指针一移动即变成 `dragged`，`is_decidedly_dragging()`
/// 为真，释放时**抑制**落在它之上前台控件的 click（interaction.rs:174-182）。
/// 表现为「能拖窗但按钮/列表/输入框全部点不动」。
///
/// 现改为**按帧手动检测**：不注册任何占屏交互 widget，仅当指针按下且落点
/// ① 不在缩放热区、② 不在任何前台交互控件矩形内（用上一帧
/// `interactive_rects_last_pass()` 判定，该集合只含 click/drag 控件、不含
/// hover-only 背景）时记录候选起点；指针移动超过阈值再发 `StartDrag`。
/// 这样前台控件 click 完全不受干扰。
pub(crate) fn chrome_begin(app: &mut PaletteApp, ctx: &egui::Context) {
    if app.native_resize {
        // 原生缩放模态循环期间 egui 收不到输入；循环结束后的首个
        // 「主键已抬起」帧在这里清除旗标，恢复 chrome。
        app.drag_candidate = None;
        if !ctx.input(|i| i.pointer.primary_down()) {
            app.native_resize = false;
        }
        return;
    }
    // 窗口屏幕矩形：egui 0.36 无 `Context::screen_rect()`，取本帧视口矩形
    // （与视口命令同一坐标系）。`raw.screen_rect` 为 `Option<Rect>`。
    let screen = ctx.input(|i| i.raw.screen_rect).unwrap();
    let pointer = ctx.input(|i| i.pointer.clone());

    // 上一帧所有 click/drag 交互控件矩形（空白拖拽判定基准）。
    let interactive = ctx.interactive_rects_last_pass();
    // 给定屏幕坐标是否落在某个前台交互控件内。
    let over_interactive = |p: egui::Pos2| -> bool { interactive.iter().any(|r| r.contains(p)) };

    // 主键刚按下：仅在「空白区」记录拖拽候选（缩放热区优先给 chrome_end，
    // 交互控件上则交给该控件处理，不抢 press）。
    if pointer.primary_pressed() {
        app.drag_candidate = pointer
            .press_origin()
            .filter(|&o| edge_zone(screen, o).is_none() && !over_interactive(o));
    }

    // 待定拖拽：主键仍按住且移动超过阈值 → 发起原生拖拽（之后由 OS 接管，
    // 本帧清候选避免重复发令）；已抬起则作废。
    if let Some(origin) = app.drag_candidate {
        if pointer.primary_down() {
            if let Some(pos) = pointer.latest_pos() {
                if pos.distance(origin) > DRAG_THRESHOLD {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    app.drag_candidate = None;
                }
            }
        } else {
            app.drag_candidate = None;
        }
    }
}

/// 帧尾调用（全部控件绘制之后）：缩放热区光标覆盖 + 按下发起原生缩放。
/// 放在帧尾是为了让光标图标覆盖任何控件 hover 光标（如输入框 Text）。
pub(crate) fn chrome_end(app: &mut PaletteApp, ctx: &egui::Context) {
    if app.native_resize {
        return;
    }
    let screen = ctx.input(|i| i.raw.screen_rect).unwrap();
    let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) else {
        return;
    };
    if let Some((dir, icon)) = edge_zone(screen, pos) {
        ctx.set_cursor_icon(icon);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            app.native_resize = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::edge_zone;
    use eframe::egui::{pos2, vec2, CursorIcon, Rect, ResizeDirection as D};

    fn screen() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(560.0, 460.0))
    }

    #[test]
    fn edge_segments_only_within_edge_band() {
        let r = screen();
        let cases = [
            (pos2(3.0, 230.0), D::West),
            (pos2(557.0, 230.0), D::East),
            (pos2(280.0, 3.0), D::North),
            (pos2(280.0, 457.0), D::South),
        ];
        for (pos, dir) in cases {
            assert_eq!(edge_zone(r, pos).unwrap().0, dir, "{pos:?}");
        }
        // EDGE(6) 与 CORNER(12) 之间的单轴带 = 非热区（边只认 6px）
        assert_eq!(edge_zone(r, pos2(8.0, 230.0)), None);
        assert_eq!(edge_zone(r, pos2(280.0, 8.0)), None);
    }

    #[test]
    fn corner_zones_win_over_edges() {
        let r = screen();
        let cases = [
            (pos2(3.0, 3.0), D::NorthWest),
            (pos2(557.0, 3.0), D::NorthEast),
            (pos2(3.0, 457.0), D::SouthWest),
            (pos2(557.0, 457.0), D::SouthEast),
            // 距角 <12 但距边 >6（如 (9,9)）：仍在 CORNER 角带内 → 角生效
            (pos2(9.0, 9.0), D::NorthWest),
        ];
        for (pos, dir) in cases {
            assert_eq!(edge_zone(r, pos).unwrap().0, dir, "{pos:?}");
        }
    }

    #[test]
    fn interior_and_outside_are_none() {
        let r = screen();
        assert_eq!(edge_zone(r, pos2(280.0, 230.0)), None, "内部非热区");
        assert_eq!(edge_zone(r, pos2(-1.0, 230.0)), None, "窗外左侧");
        assert_eq!(edge_zone(r, pos2(280.0, 461.0)), None, "窗外下方");
    }

    #[test]
    fn cursor_icons_match_directions() {
        let r = screen();
        assert_eq!(
            edge_zone(r, pos2(3.0, 230.0)).unwrap().1,
            CursorIcon::ResizeWest
        );
        assert_eq!(
            edge_zone(r, pos2(3.0, 3.0)).unwrap().1,
            CursorIcon::ResizeNorthWest
        );
        assert_eq!(
            edge_zone(r, pos2(557.0, 3.0)).unwrap().1,
            CursorIcon::ResizeNorthEast
        );
    }
}
