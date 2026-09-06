//! 二次确认对话框绘制。

use crate::app::PaletteApp;
use crate::ui::widgets::keycap_width;
use crate::ui::widgets::paint_keycap;
use crate::ui::widgets::text_width;
use dd_gui::theme;
use eframe::egui;

impl PaletteApp {
    /// 二次确认对话框（设计稿 §10 Fluent Dialog 语义，C 组批次 C3）：
    /// 全屏遮罩（blackAlpha .50 暗 / .40 亮，点击遮罩 = 取消）+ 420px 面板
    /// （panel 底 + 1px border + 圆角 8 + shadow64、padding 20/20/16）+
    /// 右对齐按钮区（键位提示最左 + 取消 secondary + 确认 accent/critical
    /// danger 底，高 32 圆角 4）。
    ///
    /// 键盘语义不变（`handle_keys`：对话框活跃时 Enter=确认、Esc=取消、
    /// 其余键不穿透 → 遮罩期间列表键盘选择冻结）。鼠标点击确认同样触发。
    /// 渲染只读借用 `self.confirm`，点击后再取走并真正发起请求。
    pub(crate) fn draw_confirm(&mut self, ctx: &egui::Context) {
        let dialog = match self.confirm.as_ref() {
            Some(d) => d,
            None => return,
        };
        let title = dialog.title.clone();
        let description = dialog.description.clone();
        let confirm_label = if dialog.confirm_label.is_empty() {
            self.tr("dialog.confirm").to_string()
        } else {
            dialog.confirm_label.clone()
        };
        let is_critical = dialog.is_critical;
        let dark = ctx.theme() == egui::Theme::Dark;
        let p = theme::Palette::of(dark);

        let mut confirmed = false;
        let mut cancelled = false;
        let mut dialog_rect = egui::Rect::NOTHING;

        // ── 对话框面板（Tooltip 层：高于遮罩 Foreground，面板自动按内容定高） ──
        egui::Area::new(egui::Id::new("dd-gui-dialog"))
            .order(egui::Order::Tooltip)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let frame = egui::Frame::default()
                    .fill(p.panel)
                    .stroke(egui::Stroke::new(1.0, p.border))
                    .corner_radius(8.0)
                    .shadow(theme::dialog_shadow(dark))
                    .inner_margin(egui::Margin {
                        left: 20,
                        right: 20,
                        top: 20,
                        bottom: 16,
                    })
                    .show(ui, |ui| {
                        // §10.1 面板宽 420px（含 padding 20×2 → 内容 380），
                        // 窄屏时 clamp 到可用宽度（max-width:100%）。
                        ui.set_min_width(380.0f32.min(ui.available_width()));
                        // 标题 subtitle1 16/22·600 text
                        ui.label(
                            egui::RichText::new(&title)
                                .size(16.0)
                                .strong()
                                .color(p.text),
                        );
                        ui.add_space(8.0);
                        // 正文 body1 14/20 text-2（可换行）
                        ui.label(egui::RichText::new(&description).size(14.0).color(p.text2));
                        ui.add_space(20.0);
                        // ── 按钮区（右对齐、间距 8、高 32 圆角 4；提示最左） ──
                        let row_w = ui.available_width();
                        let (row, _) =
                            ui.allocate_exact_size(egui::vec2(row_w, 32.0), egui::Sense::hover());
                        let confirm_w =
                            text_width(ui, &confirm_label, egui::FontId::proportional(14.0)) + 24.0;
                        let cancel_label = self.tr("dialog.cancel");
                        let cancel_w =
                            text_width(ui, cancel_label, egui::FontId::proportional(14.0)) + 24.0;
                        let confirm_rect = egui::Rect::from_min_size(
                            egui::pos2(row.right() - confirm_w, row.min.y),
                            egui::vec2(confirm_w, 32.0),
                        );
                        let cancel_rect = egui::Rect::from_min_size(
                            egui::pos2(confirm_rect.left() - 8.0 - cancel_w, row.min.y),
                            egui::vec2(cancel_w, 32.0),
                        );
                        // 键位提示（最左，hint margin-right auto）：Enter 键帽 +
                        // 确认 + Esc 键帽 + 取消（caption1 12 text-3）。
                        let esc_kw = keycap_width(ui, "Esc");
                        // §10.1 键帽行「↵ Enter 确认 / Esc 取消」：↵ 入键帽
                        let enter_cap = "↵ Enter";
                        let enter_kw = keycap_width(ui, enter_cap);
                        let que_w = text_width(
                            ui,
                            self.tr("dialog.confirm"),
                            egui::FontId::proportional(12.0),
                        );
                        let qux_w = text_width(ui, cancel_label, egui::FontId::proportional(12.0));
                        let hint_w = enter_kw + 4.0 + que_w + 12.0 + esc_kw + 4.0 + qux_w;
                        let cy = row.center().y;
                        if cancel_rect.left() - row.min.x > hint_w + 12.0 {
                            let mut x = row.min.x;
                            paint_keycap(
                                ui,
                                egui::Rect::from_min_size(
                                    egui::pos2(x, cy - theme::KEYCAP_H / 2.0),
                                    egui::vec2(enter_kw, theme::KEYCAP_H),
                                ),
                                enter_cap,
                                &p,
                            );
                            x += enter_kw + 4.0;
                            ui.painter().text(
                                egui::pos2(x, cy),
                                egui::Align2::LEFT_CENTER,
                                self.tr("dialog.confirm"),
                                egui::FontId::proportional(12.0),
                                p.text3,
                            );
                            x += que_w + 12.0;
                            paint_keycap(
                                ui,
                                egui::Rect::from_min_size(
                                    egui::pos2(x, cy - theme::KEYCAP_H / 2.0),
                                    egui::vec2(esc_kw, theme::KEYCAP_H),
                                ),
                                "Esc",
                                &p,
                            );
                            x += esc_kw + 4.0;
                            ui.painter().text(
                                egui::pos2(x, cy),
                                egui::Align2::LEFT_CENTER,
                                cancel_label,
                                egui::FontId::proportional(12.0),
                                p.text3,
                            );
                        }
                        // 确认按钮：默认 accent 底白字；critical → danger 底白字
                        let (fill, tcol) = if is_critical {
                            (p.danger, egui::Color32::WHITE)
                        } else {
                            (p.accent, egui::Color32::WHITE)
                        };
                        if draw_dialog_button(ui, confirm_rect, &confirm_label, fill, tcol, false) {
                            confirmed = true;
                        }
                        // 取消按钮：secondary 形态（card 底 + border-strong 描边）
                        if draw_dialog_button(ui, cancel_rect, cancel_label, p.card, p.text, true) {
                            cancelled = true;
                        }
                    });
                dialog_rect = frame.response.rect;
            });

        // ── 全屏遮罩（Foreground 层：盖住列表/页脚，Tooltip 层对话框在其上）──
        let screen = ctx.viewport_rect();
        egui::Area::new(egui::Id::new("dd-gui-confirm-overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::LEFT_TOP, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.painter().rect_filled(screen, 0.0, theme::overlay(dark));
                let resp = ui.allocate_exact_size(screen.size(), egui::Sense::click());
                if resp.1.clicked() {
                    // 点击遮罩 = 取消（§10.1）；落在对话框面板内的点击不算
                    if let Some(pos) = resp.1.interact_pointer_pos() {
                        if !dialog_rect.contains(pos) {
                            cancelled = true;
                        }
                    }
                }
            });

        if confirmed {
            // 取走对话框并真正重发 invoke（带 context.confirmed = true）
            let taken = self.confirm.take().expect("对话框仍应在位");
            let params = taken.pending.confirmed_params();
            self.dispatch_invoke(&taken.ext_id, params);
        } else if cancelled {
            self.confirm = None;
        }
    }
}

// ── 设置页（§08 v4.2）─────────────────────────────────────────────────

/// Dialog 按钮（§10.1，C 组批次 C3）：高 32、圆角 4、padding 0 12、
/// body1 14px。`stroked` = secondary 形态（card 底 + border-strong 描边，
/// hover → row-hover）；主按钮 accent/danger 底白字无描边。
/// 用 `ui.interact` 在给定绝对矩形上注册点击（矩形已由调用方排定）。
pub(crate) fn draw_dialog_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    label: &str,
    fill: egui::Color32,
    text_color: egui::Color32,
    stroked: bool,
) -> bool {
    let resp = ui.interact(
        rect,
        egui::Id::new(("dd-dialog-btn", label)),
        egui::Sense::click(),
    );
    let hover_fill = theme::Palette::of(ui.visuals().dark_mode).row_hover;
    let bg = if resp.hovered() && stroked {
        hover_fill
    } else {
        fill
    };
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(4), bg);
    if stroked {
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(4),
            egui::Stroke::new(
                1.0,
                theme::Palette::of(ui.visuals().dark_mode).border_strong,
            ),
            egui::StrokeKind::Inside,
        );
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(14.0),
        text_color,
    );
    resp.clicked()
}
