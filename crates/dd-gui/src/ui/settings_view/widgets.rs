//! O12（2026-10-11）拆分自 `settings_view.rs`（O12 巨型文件拆分批次）。
//! 纪律（refactor-layering-plan 同款）：搬运单位 = 完整定义块（含文档注释），
//! 函数体一字不改；仅按编译器指示将跨子模块项 `pub(super)` 化。
//! 本文件承载：设置页自绘控件族（chip / 卡片容器 / radio-card / 密度 pill / 滑杆 / 按钮 / 键帽 / 下拉框）。

use super::*;

pub(crate) fn draw_ext_chip(ui: &mut egui::Ui, text: &str, p: &theme::Palette) -> egui::Rect {
    let text_w = text_width(ui, text, egui::FontId::monospace(10.0));
    let w = text_w + 16.0;
    let h = 16.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(8), p.chip_bg);
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(8),
        egui::Stroke::new(1.0, p.border),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::monospace(10.0),
        p.text3,
    );
    rect
}

/// 版本徽标 chip（设计稿 §08 mockup `.ext-chip`）：`v{version}` 文字。
pub(crate) fn draw_version_chip(
    ui: &mut egui::Ui,
    version: &str,
    p: &theme::Palette,
) -> egui::Rect {
    draw_ext_chip(ui, &format!("v{version}"), p)
}

/// 设置页卡片容器（Fluent 9 Card，§08.1 "卡片" 规格）：
/// card 底 + 1px `--border` 描边 + radius-xl(8) + padding 12px。
///
/// `body` 在卡内绘制内容；调用方需自己管理各 section 的间距。
pub(crate) fn draw_settings_card_frame(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    glass_card: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        // 玻璃卡（材质生效时，2026-09-13 真机反馈）：材质从卡面透出；
        // 回退路径 = 实色 card（theme::card_fill 内部裁决）。
        .fill(theme::card_fill(ui.visuals().dark_mode, glass_card))
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                // 卡片恒等宽（真机 2026-09-04 修复）：egui Frame 默认按内容最小宽
                // 收缩——内容只有短文本+checkbox 的卡片（「打开面板时显示」）实测窄
                // 84px（449 vs 533）。设计稿 §08.1 `.settings-card` 是 block 级全宽
                // 卡片，这里把内层 min_width 钉到可用宽，使所有卡片恒为全宽。
                ui.set_min_width(ui.available_width());
                body(ui);
            });
        });
}

/// 选中态 radio-card 的填充色（§08.1 line 486 `.radio-card.sel`）：
/// accent 软色叠加于 card 底之上。暗色 0.28 / 亮色 0.08 不透明度。
///
/// 绘制时由 `draw_radio_card` 直接 `rect_filled` 此色一次完成，不必先填
/// card 再叠 alpha——egui `rect_filled` 单色 + 边框即可等价视觉（卡片本身
/// 已 `draw_settings_card_frame` 在外层填过 card 底，此处填 alpha-多重 accent
/// 在视觉上与"accent_soft over card"等价）。
pub(crate) fn accent_soft(dark: bool, p: &theme::Palette) -> egui::Color32 {
    let alpha = if dark { 0.28 } else { 0.08 };
    p.accent.gamma_multiply(alpha)
}

/// 主题缩略图种类（B6③）：跟随系统 = 亮暗左右拼接、亮/暗 = 单色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThemeThumb {
    System,
    Light,
    Dark,
}

/// 单个迷你面板缩略图区域（B6③）：搜索栏条 + 选中行条（accent_stroke）+
/// 普通行条，全部取目标主题 `pal` 的现有 token 值，零魔法色值。
fn draw_thumb_region(painter: &egui::Painter, pal: &theme::Palette, r: egui::Rect) {
    // 搜索栏条（input_fill 底 + 1px border 描边）
    let sb = egui::Rect::from_min_size(
        egui::pos2(r.left() + 4.0, r.top() + 5.0),
        egui::vec2(r.width() - 8.0, 6.0),
    );
    painter.rect_filled(sb, egui::CornerRadius::same(2), pal.input_fill);
    painter.rect_stroke(
        sb,
        egui::CornerRadius::same(2),
        egui::Stroke::new(1.0, pal.border),
        egui::StrokeKind::Inside,
    );
    // 选中行条（accent_stroke）
    let r1 = egui::Rect::from_min_size(
        egui::pos2(r.left() + 4.0, r.top() + 15.0),
        egui::vec2((r.width() - 8.0) * 0.62, 4.0),
    );
    painter.rect_filled(r1, egui::CornerRadius::same(2), pal.accent_stroke);
    // 普通行条
    let r2 = egui::Rect::from_min_size(
        egui::pos2(r.left() + 4.0, r.top() + 22.0),
        egui::vec2((r.width() - 8.0) * 0.45, 4.0),
    );
    painter.rect_filled(r2, egui::CornerRadius::same(2), pal.row_selected);
}

/// 主题缩略图（B6③）：34px 高圆角块，替换原双色块 swatch——直观呈现目标
/// 主题的面板观感。跟随系统 = 左半亮右半暗拼接。
fn draw_theme_thumb(ui: &mut egui::Ui, thumb: ThemeThumb, rect: egui::Rect, p: &theme::Palette) {
    let lp = theme::Palette::light();
    let dp = theme::Palette::dark();
    let radius4 = egui::CornerRadius::same(4);
    let painter = ui.painter();
    match thumb {
        ThemeThumb::Light => {
            painter.rect_filled(rect, radius4, lp.panel);
            draw_thumb_region(painter, &lp, rect);
        }
        ThemeThumb::Dark => {
            painter.rect_filled(rect, radius4, dp.panel);
            draw_thumb_region(painter, &dp, rect);
        }
        ThemeThumb::System => {
            // 左半亮右半暗拼接：整块先铺亮底，再叠右半暗底（radius 4 只在
            // 右缘起作用，中缝为直边拼接线）。
            let mid = rect.left() + rect.width() * 0.5;
            painter.rect_filled(rect, radius4, lp.panel);
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(mid, rect.top()), rect.max),
                radius4,
                dp.panel,
            );
            let left_half =
                egui::Rect::from_min_max(rect.min, egui::pos2(mid - 2.0, rect.bottom()));
            let right_half = egui::Rect::from_min_max(egui::pos2(mid + 2.0, rect.top()), rect.max);
            draw_thumb_region(painter, &lp, left_half);
            draw_thumb_region(painter, &dp, right_half);
        }
    }
    // 外描边用当前主题 border（缩略图属于当前 UI 的 chrome）
    painter.rect_stroke(
        rect,
        radius4,
        egui::Stroke::new(1.0, p.border),
        egui::StrokeKind::Inside,
    );
}

/// 单张 radio-card（§08.1 "主题单选"）：圆点 12×12 + 名称 (caption1 12/16)
/// + 副标题 (mini 10/14 fg-3) + 迷你面板缩略图（B6③，原双色块 swatch）。
///
/// 规格（§08.1 line 1236）：
/// - 未选：1px `--border-strong` + `--input-fill` 底 + 圆点 1.5px stroke 空心；
/// - 选中：2px accent 边 + accent_soft 底 + 圆点实心 (5.5px accent)；
/// - padding 10 12（line 482）；等宽由调用方按 `(avail - 2*gap) / 3` 计算。
///
/// 返回是否被点击。
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_radio_card(
    ui: &mut egui::Ui,
    w: f32,
    label: &str,
    sub: &str,
    thumb: ThemeThumb,
    selected: bool,
    p: &theme::Palette,
    dark: bool,
) -> bool {
    // 高度 = padding 10 top + name 20 + sub 14 + gap 8 + 缩略图 34 + padding 10 bot = 96
    //（B6③：缩略图 16→34，卡体随之增高）
    let h = 96.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
    let radius = egui::CornerRadius::same(8);

    // 底色：未选 = input_fill；选中 = accent 软色
    if selected {
        ui.painter().rect_filled(rect, radius, accent_soft(dark, p));
    } else {
        ui.painter().rect_filled(rect, radius, p.input_fill);
    }
    // hover（未选）：叠加 control_hover 暗示可点（B7 修订：几何判定 +
    // 亮色可辨灰阶）
    if !selected && ui.rect_contains_pointer(rect) {
        ui.painter()
            .rect_filled(rect.shrink(1.0), radius, p.control_hover);
    }
    // 边框（B2：2px 选中边框为线状小元素，走 accent_stroke）
    let stroke = if selected {
        egui::Stroke::new(2.0, p.accent_stroke)
    } else {
        egui::Stroke::new(1.0, p.border_strong)
    };
    ui.painter()
        .rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);

    // 圆点 12×12（CSS `.rc-dot`：1.5px 描边）：rect 内部 padding 12 →
    // 圆心 (rect.left+12+6, rect.top+12+6)。选中态（CSS
    // `.radio-card.sel .rc-dot`）= accent 环 + 3.5px accent 实心内点。
    let dot_cx = rect.left() + 18.0;
    let dot_cy = rect.top() + 18.0;
    if selected {
        ui.painter().circle_stroke(
            egui::pos2(dot_cx, dot_cy),
            6.0,
            egui::Stroke::new(1.5, p.accent_stroke),
        );
        ui.painter()
            .circle_filled(egui::pos2(dot_cx, dot_cy), 3.5, p.accent_stroke);
    } else {
        ui.painter().circle_stroke(
            egui::pos2(dot_cx, dot_cy),
            6.0,
            egui::Stroke::new(1.5, p.border_strong),
        );
    }
    // 名称（v4.9：12 → 14 body，text 色）
    let name_x = dot_cx + 12.0; // 圆点右 6 + 文字前内 padding 6
    let name_y = rect.top() + 12.0;
    ui.painter().text(
        egui::pos2(name_x, name_y),
        egui::Align2::LEFT_TOP,
        label,
        egui::FontId::proportional(14.0),
        p.text,
    );
    // 副标题（v4.9：10 → 11、text-3）
    ui.painter().text(
        egui::pos2(name_x, name_y + 20.0),
        egui::Align2::LEFT_TOP,
        sub,
        egui::FontId::proportional(11.0),
        p.text3,
    );
    // 迷你面板缩略图（B6③，34px 高 + gap 4）
    let thumb_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 12.0, rect.bottom() - 10.0 - 34.0),
        egui::vec2(rect.width() - 24.0, 34.0),
    );
    draw_theme_thumb(ui, thumb, thumb_rect, p);
    resp.clicked()
}

/// 密度三选 pill（F2）：radio-card 的无缩略图变体——选中 = accent_soft 底 +
/// 2px accent_stroke 边框；未选 = input_fill + 1px border_strong，hover 叠
/// control_hover（B7 几何判定口径）。返回是否被点击。
pub(crate) fn draw_density_pill(
    ui: &mut egui::Ui,
    w: f32,
    label: &str,
    selected: bool,
    p: &theme::Palette,
    dark: bool,
    dim: bool,
) -> bool {
    let h = 32.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
    let radius = egui::CornerRadius::same(6);
    if selected && !dim {
        ui.painter().rect_filled(rect, radius, accent_soft(dark, p));
    } else {
        ui.painter().rect_filled(rect, radius, p.input_fill);
        // 禁用态（dim，如边框行在材质关闭时）不做 hover 反馈
        if ui.rect_contains_pointer(rect) && ui.is_enabled() {
            ui.painter()
                .rect_filled(rect.shrink(1.0), radius, p.control_hover);
        }
    }
    // 边框同 radio-card：选中 2px 走 accent_stroke（B2 线状小元素口径）；
    // 禁用态去 accent（结构性提示改用 border_strong 描边 + text2 标签）
    let stroke = if dim {
        egui::Stroke::new(1.0, if selected { p.border_strong } else { p.border })
    } else if selected {
        egui::Stroke::new(2.0, p.accent_stroke)
    } else {
        egui::Stroke::new(1.0, p.border_strong)
    };
    ui.painter()
        .rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
    let label_color = if dim {
        if selected {
            p.text2
        } else {
            p.text3
        }
    } else if selected {
        p.text
    } else {
        p.text2
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(12.0),
        label_color,
    );
    resp.clicked()
}

/// 滑杆行（v4.20，2026-09-29）：自绘滑杆 + 同行右缘百分比。滑杆占满除
/// 百分比预留宽（最宽 "100%"）外的整行，百分比与滑钮垂直居中同排；取值在
/// 滑杆绘制之后——拖动当帧即显示新值，且预留宽固定、值变化不引起行宽抖动。
pub(super) fn draw_slider_row_with_pct(
    ui: &mut egui::Ui,
    enabled: bool,
    value: &mut u8,
    p: &theme::Palette,
) -> (bool, bool) {
    let mut out = (false, false);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let pct_w = text_width(ui, "100%", egui::FontId::proportional(12.0));
        let slider_w = (ui.available_width() - pct_w - 8.0).max(120.0);
        out = draw_opacity_slider(ui, enabled, value, slider_w, p);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("{}%", *value))
                    .size(12.0)
                    .color(if enabled { p.text2 } else { p.text3 }),
            );
        });
    });
    out
}

/// 不透明度滑杆（P2 v6，2026-09-13 设计风格对齐）：egui 默认 Slider 的细灰
/// 轨 + 行内百分比后缀与整套 Fluent 控件（pill/开关）风格脱节，按设计 token
/// 自绘——轨 4px 圆角 2（未选段 `--border` / 已选段 accent_stroke，与搜索框
/// 聚焦下划线同源的小面积强调口径）、钮 16px 白底 border-strong 描边（悬停
/// /拖动加粗 2px）；禁用态（材质关/未生效）置灰且不响应。返回 (changed,
/// released)：拖动中 changed 即时生效不落盘，released（松手/单击跳值）落盘。
/// `width`：轨占宽，由调用方预算（同行放百分比时扣除其预留宽）。
fn draw_opacity_slider(
    ui: &mut egui::Ui,
    enabled: bool,
    value: &mut u8,
    width: f32,
    p: &theme::Palette,
) -> (bool, bool) {
    let height = 20.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let id = ui.id().with("opacity_slider");
    let resp = ui.interact(rect, id, egui::Sense::click_and_drag());
    let mut changed = false;
    let mut released = false;
    if enabled {
        if let Some(pos) = resp.interact_pointer_pos() {
            if resp.dragged() || resp.clicked() {
                let v = slider_value(rect.left(), rect.width(), pos.x);
                if v != *value {
                    *value = v;
                    changed = true;
                }
            }
        }
        released = resp.drag_stopped() || resp.clicked();
    }
    // 轨：4px 圆角 2，垂直居中
    let rail_y = rect.center().y;
    let rail = egui::Rect::from_min_max(
        egui::pos2(rect.left(), rail_y - 2.0),
        egui::pos2(rect.right(), rail_y + 2.0),
    );
    let rail_radius = egui::CornerRadius::same(2);
    ui.painter().rect_filled(rail, rail_radius, p.border);
    // 已选段：accent_stroke（B2 小面积强调口径）
    let t = f32::from((*value).min(100)) / 100.0;
    let fill_right = rect.left() + rect.width() * t;
    if fill_right - rail.left() > 2.0 {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(rail.min, egui::pos2(fill_right, rail.max.y)),
            rail_radius,
            if enabled {
                p.accent_stroke
            } else {
                p.border_strong
            },
        );
    }
    // 钮：16px 白底 + border-strong 描边（悬停/拖动加粗）；禁用置灰
    let thumb_center = egui::pos2(
        fill_right.clamp(rail.left() + 8.0, rail.right() - 8.0),
        rail_y,
    );
    let active = enabled && (resp.hovered() || resp.dragged());
    let thumb_fill = if enabled {
        egui::Color32::WHITE
    } else {
        p.border
    };
    let thumb_stroke = egui::Stroke::new(
        if active { 2.0 } else { 1.0 },
        if enabled { p.border_strong } else { p.border },
    );
    ui.painter().circle_filled(thumb_center, 8.0, thumb_fill);
    ui.painter().circle_stroke(thumb_center, 8.0, thumb_stroke);
    (changed, released)
}

/// 指针横坐标 → 0–100 档位（纯函数，供单测；宽度非正防除零）。
pub(super) fn slider_value(left: f32, width: f32, x: f32) -> u8 {
    if width <= 0.0 {
        return 0;
    }
    ((x - left) / width)
        .clamp(0.0, 1.0)
        .mul_add(100.0, 0.5)
        .floor() as u8
}

/// 功能态 ToggleSwitch（§08.1 v4.7「材质开关」行；v4.9 放大到 Fluent 规格
/// 40×20、滑块 16）：开 = accent 填充底 + 白滑块居右；关 = input_fill 底 +
/// 1px border-strong 描边 + text-2 滑块居左。B7 悬停：开 = accent_hover 底、
/// 关 = row_hover 底（命令语义控件，光标保持默认箭头）。返回是否被点击。
pub(crate) fn draw_switch_fn(ui: &mut egui::Ui, on: bool, p: &theme::Palette) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(40.0, 20.0), egui::Sense::click());
    // B7 真机修订（2026-09-12）：hovered() 不生效，几何判定同列表行。
    let hovered_now = ui.rect_contains_pointer(rect);
    let radius = egui::CornerRadius::same(10);
    let knob_cx = if on {
        rect.right() - 2.0 - 8.0
    } else {
        rect.left() + 2.0 + 8.0
    };
    if on {
        let fill = if hovered_now {
            p.accent_hover
        } else {
            p.accent
        };
        ui.painter().rect_filled(rect, radius, fill);
        ui.painter().circle_filled(
            egui::pos2(knob_cx, rect.center().y),
            8.0,
            egui::Color32::WHITE,
        );
    } else {
        let fill = if hovered_now {
            p.control_hover
        } else {
            p.input_fill
        };
        ui.painter().rect_filled(rect, radius, fill);
        // 关态悬停：描边转 accent_stroke（input_fill 与 control_hover 同为
        // 浅灰、仅靠填充不可辨，描边变化提供明确反馈）
        ui.painter().rect_stroke(
            rect,
            radius,
            egui::Stroke::new(
                1.0,
                if hovered_now {
                    p.accent_stroke
                } else {
                    p.border_strong
                },
            ),
            egui::StrokeKind::Inside,
        );
        ui.painter()
            .circle_filled(egui::pos2(knob_cx, rect.center().y), 8.0, p.text2);
    }
    resp.clicked()
}

/// Fluent 2 标准次级按钮（v4.9，真机 2026-09-06 反馈"恢复默认/更改不像按钮"）：
/// 高 28（D42 compact 档，原 32）/ 文字 12（D42 控件档，原 14 body）/ 左右
/// padding 12 / 圆角 4（radius-m）/ 1px
/// `--border-strong` 描边 / card 底；hover = bg1Hover、按下 = bg1Pressed
/// （Fluent 控件态三段）。返回是否被点击。
pub(crate) fn fluent_button(ui: &mut egui::Ui, text: &str, p: &theme::Palette) -> bool {
    fluent_button_sized(ui, text, p, CONTROL_H, CONTROL_FONT_PT, 12.0)
}

/// Fluent 2 小按钮（列表行内动作，如引擎「删除」）：高 24 / 文字 12 / padding 8。
pub(crate) fn fluent_button_small(ui: &mut egui::Ui, text: &str, p: &theme::Palette) -> bool {
    fluent_button_sized(ui, text, p, 24.0, 12.0, 8.0)
}

/// Fluent 2 单行文本框（N1 自定义命令卡；自绘口径同搜索引擎卡「添加自定义
/// 引擎」：card 底 / 1px border-strong / 圆角 4 / 高 CONTROL_H + frameless
/// TextEdit 内嵌垂直居中；聚焦 = 底边 2px accent 下划线）。`width` 为外框
/// 总宽，`id` 须全页唯一（focus 判定键）。
pub(crate) fn draw_fluent_textbox(
    ui: &mut egui::Ui,
    width: f32,
    id: &str,
    buf: &mut String,
    hint: &str,
    p: &theme::Palette,
) {
    let (box_rect, _) = ui.allocate_exact_size(egui::vec2(width, CONTROL_H), egui::Sense::hover());
    let edit_id = egui::Id::new(id);
    let focused = ui.ctx().memory(|m| m.has_focus(edit_id));
    let radius = egui::CornerRadius::same(4);
    // 背景与描边先画（TextEdit 文字绘制在其上层）
    ui.painter().rect_filled(box_rect, radius, p.card);
    ui.painter().rect_stroke(
        box_rect,
        radius,
        egui::Stroke::new(1.0, p.border_strong),
        egui::StrokeKind::Inside,
    );
    if focused {
        // Fluent 聚焦态：底边 2px accent 下划线（内缩 1px 避让描边）。
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(box_rect.left() + 1.0, box_rect.bottom() - 3.0),
                egui::pos2(box_rect.right() - 1.0, box_rect.bottom() - 1.0),
            ),
            egui::CornerRadius::same(1),
            p.accent_stroke,
        );
    }
    ui.put(
        box_rect,
        egui::TextEdit::singleline(buf)
            .id(edit_id)
            .desired_width(width - 24.0)
            .font(egui::FontId::proportional(CONTROL_FONT_PT))
            .text_color(p.text)
            .vertical_align(egui::Align::Center)
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 0)))
            .hint_text(hint),
    );
}

/// [`fluent_button`] / [`fluent_button_small`] 共用绘制核心。
fn fluent_button_sized(
    ui: &mut egui::Ui,
    text: &str,
    p: &theme::Palette,
    h: f32,
    font_size: f32,
    pad_x: f32,
) -> bool {
    let font = egui::FontId::proportional(font_size);
    let w = (text_width(ui, text, font.clone()) + 2.0 * pad_x).max(h + 8.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
    let radius = egui::CornerRadius::same(4);
    // B7 修订：几何判定 + control_hover（亮色 row_hover 与 card 底不可辨）
    let hovered_now = ui.rect_contains_pointer(rect);
    let fill = if hovered_now && resp.is_pointer_button_down_on() {
        p.row_pressed
    } else if hovered_now {
        p.control_hover
    } else {
        p.card
    };
    ui.painter().rect_filled(rect, radius, fill);
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0, p.border_strong),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        font,
        p.text,
    );
    resp.clicked()
}

/// 设置页大号键帽（v4.9：热键组合展示专用）——monospace 12 / 24px 高 / 左右
/// 8px padding / chip 底 + 1px border-strong 描边 + 圆角 4。页脚键帽
/// （10px/16px 高）在设置页正文中过小（真机反馈"文字偏小"），放大一档。
/// 捕获对话框实时修饰键（PowerToys 徽章预览）：每帧轮询 `GetAsyncKeyState`
/// 高位（µs 级查询，仅对话框打开时每帧 5 次，无性能顾虑）。Win 键 L/R 归一。
#[cfg(windows)]
pub(super) fn held_modifier_labels() -> Vec<&'static str> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    let down = |vk: u16| unsafe { GetAsyncKeyState(vk as i32) & 0x8000u16 as i16 != 0 };
    let mut v = Vec::with_capacity(4);
    if down(VK_LWIN) || down(VK_RWIN) {
        v.push("Win");
    }
    if down(VK_CONTROL) {
        v.push("Ctrl");
    }
    if down(VK_MENU) {
        v.push("Alt");
    }
    if down(VK_SHIFT) {
        v.push("Shift");
    }
    v
}

pub(super) fn draw_keycap(ui: &mut egui::Ui, cap: &str, p: &theme::Palette) {
    let font = egui::FontId::monospace(12.0);
    let w = text_width(ui, cap, font.clone()) + 16.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 24.0), egui::Sense::hover());
    let radius = egui::CornerRadius::same(4);
    ui.painter().rect_filled(rect, radius, p.chip_bg);
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0, p.border_strong),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        cap,
        font,
        p.text2,
    );
}

/// 下拉宽度自适应（D42/K3）：`clamp(最长选项宽 + 左右 pad 24 + 箭头区 16,
/// 180, 260)`。zh 选项集常态落下限 180（与语言卡 D38 规格一致）；长英文
/// 选项撑宽、至上限 260 封顶（消除硬编码宽下英文文案溢出盒外）。极端超
/// 上限文案不做字符截断——现有选项集实测均 ≤260，上限即兜底。
pub(super) fn dropdown_width(ui: &egui::Ui, labels: &[&str]) -> f32 {
    let font = egui::FontId::proportional(CONTROL_FONT_PT);
    let longest = labels
        .iter()
        .map(|l| text_width(ui, l, font.clone()))
        .fold(0.0_f32, f32::max);
    (longest + 24.0 + 16.0).clamp(180.0, 260.0)
}

/// Fluent 2 标准下拉控件（v4.15；D42 改 compact 档）：高 28（原 32）、宽
/// `width`、圆角 4、1px
/// `--border-strong` 描边、`card` 底；左侧文字（12）+ 右侧 ▼ 字符（Segoe Fluent
/// Icons `E70D` ChevronDown）；hover/press 三态（card → row_hover →
/// row_pressed，与 fluent_button 同链路）；点击展开 popup
///（egui `Popup::from_toggle_button_response` 自管 toggle + 内置点击外部收起
/// `CloseOnClickOutside`），popup 内各选项 hover = row_hover、选中 =
/// row_selected 底。返回被点击的索引；None = 关闭未选。
///
/// `enabled = false`（禁用态）：文字/箭头降为 `text3`、无 hover 反馈、不弹
/// popup（搜索引擎「预设全部已添加」占位用）。
///
/// 真机 2026-09-06 反馈：egui `ComboBox` 视觉过于 native、且和左侧描述文字
/// 重叠时无法用宽度约束解决（控件自身高度变化盖住标题）。改自绘后宽度/
/// 高度/边框/箭头字符全可断言，Fluent 2 控件库口径一致（与 fluent_button/
/// draw_switch_fn 同画法）。
///
/// 真机 2026-09-06 二轮反馈：popup 被「套在一个框里」——根因 egui `Popup`
/// 默认自带 `Frame::popup(ui.style())`（自带底色/描边/阴影），内层再画一个
/// Frame = 双层框。改用 `Popup::frame(...)` 覆盖为 Fluent 配方（card 底 +
/// 1px border-strong + 圆角 4 + shadow8 `theme::menu_shadow`），选项行直接
/// 平铺不再嵌 Frame。
///
/// 实现注：egui 0.36 `Memory::toggle_popup`/`is_popup_open`/`close_popup` 全
/// 是 `pub(crate)`——外部不可调；公开 API 走 `egui::containers::Popup`
/// + 静态助手 `Popup::close_id(ctx, id)`。
///
/// 本控件用 `from_toggle_button_response` 派生 popup id 并把 open 状态落
/// `Memory`，关闭时显式调 `close_id`。
pub(super) fn draw_fluent_dropdown(
    ui: &mut egui::Ui,
    selected: usize,
    labels: &[&str],
    width: f32,
    p: &theme::Palette,
    enabled: bool,
) -> Option<usize> {
    use egui::containers::{Popup, PopupCloseBehavior};
    let h: f32 = CONTROL_H;
    let font = egui::FontId::proportional(CONTROL_FONT_PT);

    // ── 按钮：rect_filled + 描边 + 文字 + ▼ ──
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::click());
    let radius = egui::CornerRadius::same(4);
    // B7 修订：几何判定 + control_hover
    let hovered_now = ui.rect_contains_pointer(rect);
    let (fill, text_color, arrow_color) = if !enabled {
        (p.card, p.text3, p.text3)
    } else if hovered_now && resp.is_pointer_button_down_on() {
        (p.row_pressed, p.text, p.text2)
    } else if hovered_now {
        (p.control_hover, p.text, p.text2)
    } else {
        (p.card, p.text, p.text2)
    };
    ui.painter().rect_filled(rect, radius, fill);
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0, p.border_strong),
        egui::StrokeKind::Inside,
    );
    let sel_text = labels.get(selected).copied().unwrap_or("");
    ui.painter().text(
        egui::pos2(rect.left() + 12.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        sel_text,
        font.clone(),
        text_color,
    );
    // ▼ 字符：右侧 12px padding，Segoe Fluent Icons E70D ChevronDown。
    ui.painter().text(
        egui::pos2(rect.right() - 12.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        '\u{E70D}',
        egui::FontId::proportional(10.0),
        arrow_color,
    );

    if !enabled {
        return None;
    }

    // ── popup：从 button response 派生 id，自管 toggle（用 Memory 存开闭态），
    // ── CloseOnClickOutside 让外部点击（除按钮外）自动收起 ──
    let popup_id = Popup::default_response_id(&resp);
    let ctx = ui.ctx().clone();
    let mut picked = None;
    Popup::from_toggle_button_response(&resp)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .gap(4.0)
        .width(width)
        .frame(
            // Fluent 菜单层配方（与右键菜单同源）：card 底 + 1px border-strong
            // + 圆角 4 + shadow8。覆盖 egui 默认 Frame::popup，避免双层框。
            egui::Frame::new()
                .fill(p.card)
                .stroke(egui::Stroke::new(1.0, p.border_strong))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::same(4))
                .shadow(theme::menu_shadow(ui.visuals().dark_mode)),
        )
        .show(|ui| {
            // 选项行平铺（无嵌套 Frame/无行间距）：内容宽 = width − 左右
            // inner_margin 各 4。
            let item_w = width - 8.0;
            for (i, label) in labels.iter().enumerate() {
                let is_sel = i == selected;
                let (item_rect, item_resp) =
                    ui.allocate_exact_size(egui::vec2(item_w, POPUP_ITEM_H), egui::Sense::click());
                let item_fill = if is_sel {
                    p.row_selected
                } else if ui.rect_contains_pointer(item_rect) {
                    p.control_hover
                } else {
                    egui::Color32::TRANSPARENT
                };
                ui.painter()
                    .rect_filled(item_rect, egui::CornerRadius::same(2), item_fill);
                ui.painter().text(
                    egui::pos2(item_rect.left() + 10.0, item_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    *label,
                    font.clone(),
                    p.text,
                );
                if item_resp.clicked() {
                    picked = Some(i);
                    // CloseOnClickOutside 不响应选项内点击 → 显式 close
                    Popup::close_id(&ctx, popup_id);
                }
            }
        });
    picked
}
