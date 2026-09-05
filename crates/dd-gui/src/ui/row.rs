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
) -> egui::Response {
    let p = theme::Palette::of(ui.visuals().dark_mode);

    // 预算行矩形（本帧指针测试 → 同帧 hover 填充，无帧延迟）
    let row_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), theme::ROW_H),
    );
    let hovered_now = ui.rect_contains_pointer(row_rect);
    let fill = if selected {
        p.row_selected
    } else if hovered_now {
        p.row_hover
    } else {
        egui::Color32::TRANSPARENT
    };

    let frame_resp = egui::Frame::default()
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
            ui.horizontal(|ui| {
                // 图标列（20px；无图标/url 也占位，各行对齐——设计稿 04）
                draw_icon_cell(ui, icon);
                ui.add_space(12.0); // CSS `.row` gap 12px
                                    // 名称：14px（设计稿 `.name` font-weight 500——egui FontId 不携带
                                    // 字重、`.strong()` 只变色不改字重，平台限制无法等价实现，留档）
                                    // ── 右列宽度预留（真机反馈修复：长名时标题/副标题把右侧「应用」
                                    //    类型标签挤跑/重叠）——先测类型标签 + tag chips 的宽度并从左侧
                                    //    可用宽度中扣除，右列因此贴右且逐行垂直对齐。
                let font14 = egui::FontId::proportional(14.0);
                let font12 = egui::FontId::proportional(12.0);
                let mut right_reserve = 0.0;
                if let Some(cat) = &item.result_category {
                    right_reserve += text_width(ui, cat, font12.clone()).min(90.0) + 8.0;
                }
                // v4.7 修订（用户决策 2026-09-05，D13 废止）：行内 Tag chips 整体
                // 移除——默认视图中 tags 多余且挤占标题/副标题空间；宽度预留只保留
                // 类型标签。`PanelItem.tags` 数据字段保留（协议层不改），仅不渲染。
                // 4px 余量吸收测量舍入（chip 描边/字重差异）
                let avail = (ui.available_width() - right_reserve - 4.0).max(0.0);

                // 标题：占宽不超过左侧可用空间，过长截断（不再无限伸展）；
                // +2px 吸收布局舍入，避免整除边界误截断
                let title_w = (text_width(ui, &item.title, font14) + 2.0)
                    .min(avail)
                    .max(1.0);
                ui.add_sized(
                    egui::vec2(title_w, 16.0),
                    egui::Label::new(egui::RichText::new(&item.title).size(14.0)).truncate(),
                );
                // 描述：纯文本次级文本（D12——Fluent Badge 只用于状态/计数，
                // caption1 12/fg3、无底无框、最大 240px 截断）。仅在标题截断后的
                // 剩余空间 ≥48px 时显示，不足则整段省略（避免挤压右列）。
                if !item.subtitle.is_empty() {
                    ui.add_space(12.0);
                    let remain = (avail - title_w - 12.0).clamp(0.0, 240.0);
                    if remain >= 48.0 {
                        ui.add_sized(
                            egui::vec2(remain, 16.0),
                            egui::Label::new(
                                egui::RichText::new(&item.subtitle)
                                    .size(12.0)
                                    .color(p.text3),
                            )
                            .truncate(),
                        );
                    }
                }
                // 类型标签：贴右最右（caption1 12px text-3，最长 90px 截断）。
                // v4.7 修订（D13 废止）：原其左的 Tag chips 渲染已整体移除。
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(cat) = &item.result_category {
                        let cat_w = text_width(ui, cat, egui::FontId::proportional(12.0)).min(90.0);
                        if cat_w > 0.0 {
                            ui.add_sized(
                                egui::vec2(cat_w, 16.0),
                                egui::Label::new(
                                    egui::RichText::new(cat).size(12.0).color(p.text3),
                                )
                                .truncate(),
                            );
                        }
                    }
                });
            });
        })
        .response;

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
            p.accent,
        );
    }

    ui.interact(
        frame_resp.rect,
        ui.id().with(("hit", &item.id)),
        egui::Sense::click().union(egui::Sense::hover()),
    )
}
