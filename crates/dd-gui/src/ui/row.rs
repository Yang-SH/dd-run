//! 列表行绘制。

use crate::ui::icons::draw_icon_cell;
use crate::ui::icons::IconView;
use crate::ui::widgets::text_width;
use dd_gui::state::PanelItem;
use dd_gui::theme;
use eframe::egui;

pub(crate) fn draw_item_row(
    ui: &mut egui::Ui,
    item: &PanelItem,
    selected: bool,
    icon: Option<&IconView>,
    hover_enabled: bool,
) -> egui::Response {
    let p = theme::Palette::of(ui.visuals().dark_mode);

    // 预算行矩形（本帧指针测试 → 同帧 hover 填充，无帧延迟）
    let row_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), theme::ROW_H),
    );
    // v4.17a：`hover_enabled=false` = 鼠标自面板唤起后尚未动过（指针停在唤起
    // 前的残留位置）→ 不画 hover，避免与"第一项 keyboard 选中"抢视觉。
    let hovered_now = hover_enabled && ui.rect_contains_pointer(row_rect);
    let fill = if selected {
        // 选中行：实色填充 + 左侧 accent 竖条（与 hover 玻璃色视觉可区分）。
        p.row_selected
    } else if hovered_now {
        // 非选中行 hover：玻璃色（row_hover alpha=80），亚克力下通透。
        // selected 行即使被 hover 也不画 hover（fill 分支已优先 selected）——
        // 避免键盘选中被鼠标覆盖（设计稿 §6.1 视觉一致性）。
        p.row_hover_glass
    } else {
        egui::Color32::TRANSPARENT
    };

    let framed = egui::Frame::default()
        .fill(fill)
        .corner_radius(theme::ROW_RADIUS)
        // CSS `.row` padding（v4）：8px 10px 8px 8px（4px ramp，上下 8 → 行高 40）。
        // 注意 egui 0.36 `Margin` 字段为 i8。
        .inner_margin(egui::Margin {
            left: 8,
            right: 10,
            top: 8,
            bottom: 8,
        })
        .show(ui, |ui| {
            // 内容区固定 40 - 16 = 24px 高（垂直居中图标与文字）
            ui.set_min_height(theme::ROW_H - 16.0);
            // B5：行内两段式——**标题占满中列 + 类型标签贴右**；副标题（应用 =
            // exe/lnk 路径）不再渲染在行内，转整行 hover tooltip（信息不丢，
            // 用户决策 2026-09-12）。item_spacing.x 清零，间距全部显式控制，
            // 类型标签的贴右位置才能精确预算。
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.horizontal(|ui| {
                // 图标列（20px；无图标/url 也占位，各行对齐——设计稿 04）
                draw_icon_cell(ui, icon);
                ui.add_space(12.0); // CSS `.row` gap 12px
                let font14 = egui::FontId::proportional(14.0);
                let font12 = egui::FontId::proportional(12.0);
                // 类型标签宽（caption1 12px，最长 90px 截断）先测出，从标题
                // 可用宽中扣除；标签与标题间留 12px 间隙。
                let cat_w = item
                    .result_category
                    .as_deref()
                    .map(|cat| text_width(ui, cat, font12).min(90.0))
                    .unwrap_or(0.0);
                let title_avail = (ui.available_width()
                    - if cat_w > 0.0 { cat_w + 12.0 } else { 0.0 })
                .max(1.0);
                // 标题：占满中列，过长截断。
                ui.add_sized(
                    egui::vec2(title_avail, 16.0),
                    egui::Label::new(egui::RichText::new(&item.title).size(14.0)).truncate(),
                );
                // 类型标签：贴右最右。
                if let Some(cat) = &item.result_category {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_sized(
                            egui::vec2(cat_w, 16.0),
                            egui::Label::new(egui::RichText::new(cat).size(12.0).color(p.text3)),
                        );
                    });
                }
                // 标题自然宽度超出可用宽 = 被截断（供行 tooltip 判定；
                // egui 0.36 无 Response::truncated()，用量宽比较）
                text_width(ui, &item.title, font14) > title_avail
            })
            .inner
        });
    let frame_resp = framed.response;
    let title_truncated = framed.inner;

    // 选中指示条：行左缘 3px accent（`.row.selected::before`：left 0 / top-bottom 8 / 圆角 2）。
    // 画在 Frame 背景之后（x 0..3 区域无内容，不与文字重叠）。
    if selected {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(frame_resp.rect.left(), frame_resp.rect.top() + 8.0),
                egui::pos2(
                    frame_resp.rect.left() + theme::ACCENT_BAR_W,
                    frame_resp.rect.bottom() - 8.0,
                ),
            ),
            egui::CornerRadius::same(2),
            p.accent_stroke, // B2：线状小元素用 accent_stroke（暗色 brand[100]）
        );
    }

    let hit_resp = ui.interact(
        frame_resp.rect,
        ui.id().with(("hit", &item.id)),
        egui::Sense::click().union(egui::Sense::hover()),
    );
    // B5：信息不丢——整行 hover tooltip。标题被截断时给全文；副标题
    // （应用 = exe/lnk 路径）行内已不渲染，在此展示。两者都没有则不挂。
    let tip = match (title_truncated, item.subtitle.is_empty()) {
        (true, false) => format!("{}\n{}", item.title, item.subtitle),
        (true, true) => item.title.clone(),
        (false, false) => item.subtitle.clone(),
        (false, true) => String::new(),
    };
    if tip.is_empty() {
        hit_resp
    } else {
        hit_resp.on_hover_text(tip)
    }
}
