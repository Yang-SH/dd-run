//! 右键菜单浮层绘制（设计稿 10B）。

use crate::app::ctx_menu::CtxRow;
use crate::app::PaletteApp;
use crate::ui::widgets::text_width;
use dd_gui::theme;
use eframe::egui;

impl PaletteApp {
    /// 右键菜单绘制（10B.1）：
    /// - **透明点击捕获层**（Foreground）：点击面板其他区域即关闭（D19），
    ///   点击不穿透（原生菜单语义：第一次点击只关菜单）；
    /// - **菜单本体**（Tooltip 层）：`--panel` 底 + 1px border + 圆角 4 +
    ///   shadow8（theme::menu_shadow）；项高 32、圆角 4、hover/键盘焦点 =
    ///   bg1Hover；图标 16px（fg2）+ 名称 body1（fg1）+ 快捷键 caption1（fg3 右对齐）；
    ///   分隔线 1px、垂直 margin 4 / 内缩 8。
    /// - 定位（D20）：指针锚点 +2,2（键盘 = 选中行底边左缘）；下越界先向上翻转、
    ///   再在面板内 8px 边距内夹紧，绝不溢出。尺寸绘制前经 `ctx.fonts` 确定性
    ///   预量（复用 [`text_width`]），同帧完成 clamp（无首帧闪烁）。
    pub(crate) fn draw_context_menu(&mut self, ctx: &egui::Context, ui: &egui::Ui) {
        // D19：列表滚动/改选即关闭——滚动会使锚点与行的对应关系漂移。
        if self.ctx_menu.is_some() {
            let scrolled = ctx.input(|i| i.smooth_scroll_delta != egui::Vec2::ZERO);
            if scrolled {
                self.ctx_menu = None;
                return;
            }
        }
        let Some(state) = self.ctx_menu.as_ref() else {
            return;
        };
        let dark = ctx.theme() == egui::Theme::Dark;
        let p = theme::Palette::of(dark);
        let rows = state.rows.clone();
        let anchor = state.anchor;
        let focus = state.focus;

        // 预量尺寸：w = max(200, 内容宽 + 项 padding×2 + 容器 padding×2 + 描边×2)，
        // h = 容器 padding×2 + Σ(项 32 / 分隔线 9)。
        let font14 = egui::FontId::proportional(14.0);
        let font12 = egui::FontId::proportional(12.0);
        let measure = |text: &str, font: egui::FontId| -> f32 { text_width(ui, text, font) };
        let mut content_w = 0.0f32;
        for row in &rows {
            if let CtxRow::Entry(e) = row {
                let mut w =
                    theme::CTX_ICON + theme::CTX_ITEM_GAP + measure(&e.label, font14.clone());
                if !e.shortcut.is_empty() {
                    w += theme::CTX_ITEM_GAP + measure(e.shortcut, font12.clone());
                }
                content_w = content_w.max(w);
            }
        }
        let menu_w = (content_w + 2.0 * theme::CTX_ITEM_PAD_X + 2.0 * theme::CTX_MENU_PAD + 2.0)
            .max(theme::CTX_MENU_MIN_W);
        let menu_h = 2.0 * theme::CTX_MENU_PAD
            + rows
                .iter()
                .map(|r| match r {
                    CtxRow::Entry(_) => theme::CTX_ITEM_H,
                    CtxRow::Separator => theme::CTX_SEP_H,
                })
                .sum::<f32>();

        // D20 定位：先在锚点试放 → 下越界先向上翻转 → 8px 边距夹紧（水平不翻转、
        // 仅夹紧——菜单贴指针右侧展开与 Windows 惯例一致）。
        let screen = ctx.viewport_rect();
        let mut x = anchor.x;
        let mut y = anchor.y;
        if y + menu_h > screen.bottom() - theme::CTX_MENU_MARGIN {
            let flipped = anchor.y - menu_h - 2.0 * theme::CTX_ANCHOR_OFFSET;
            if flipped >= screen.top() + theme::CTX_MENU_MARGIN {
                y = flipped;
            }
        }
        x = x.clamp(
            screen.left() + theme::CTX_MENU_MARGIN,
            (screen.right() - theme::CTX_MENU_MARGIN - menu_w)
                .max(screen.left() + theme::CTX_MENU_MARGIN),
        );
        y = y.clamp(
            screen.top() + theme::CTX_MENU_MARGIN,
            (screen.bottom() - theme::CTX_MENU_MARGIN - menu_h)
                .max(screen.top() + theme::CTX_MENU_MARGIN),
        );

        let mut dismissed = false;
        // D19 修正：菜单开着时右键另一行 → 关闭并就地重开。捕获层吞掉了该行的
        // `secondary_clicked`，此处接管：记录右键点，收尾时按本帧行矩形命中。
        let mut secondary_at: Option<egui::Pos2> = None;
        // ── 透明点击捕获层（D19：点击面板其他区域即关闭，不穿透）──
        egui::Area::new(egui::Id::new("dd-gui-ctx-overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::LEFT_TOP, [0.0, 0.0])
            .show(ctx, |ui| {
                let (_, resp) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
                if resp.clicked() {
                    dismissed = true;
                }
                if resp.secondary_clicked() {
                    dismissed = true;
                    secondary_at = resp.interact_pointer_pos();
                }
            });

        // ── 菜单本体（Tooltip 层，高于捕获层）──
        let item_w = menu_w - 2.0 * theme::CTX_MENU_PAD - 2.0; // 扣描边 2×1px
        let mut activated: Option<usize> = None;
        let mut hover_focus: Option<usize> = None;
        egui::Area::new(egui::Id::new("dd-gui-ctx-menu"))
            .order(egui::Order::Tooltip)
            .fixed_pos(egui::pos2(x, y))
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(p.panel)
                    .stroke(egui::Stroke::new(1.0, p.border))
                    .corner_radius(4.0)
                    .shadow(theme::menu_shadow(dark))
                    .inner_margin(egui::Margin::same(theme::CTX_MENU_PAD as i8))
                    .show(ui, |ui| {
                        ui.set_min_size(egui::vec2(
                            item_w,
                            menu_h - 2.0 * theme::CTX_MENU_PAD - 2.0,
                        ));
                        let mut entry_i = 0usize;
                        for row in &rows {
                            match row {
                                CtxRow::Separator => {
                                    let (r, _) = ui.allocate_exact_size(
                                        egui::vec2(item_w, theme::CTX_SEP_H),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().hline(
                                        (r.min.x + theme::CTX_SEP_INSET)
                                            ..=(r.max.x - theme::CTX_SEP_INSET),
                                        r.center().y,
                                        egui::Stroke::new(1.0, p.border),
                                    );
                                }
                                CtxRow::Entry(e) => {
                                    let (rect, resp) = ui.allocate_exact_size(
                                        egui::vec2(item_w, theme::CTX_ITEM_H),
                                        egui::Sense::click(),
                                    );
                                    // hover 与键盘焦点同为 bg1Hover（10B.1）
                                    if resp.hovered() || entry_i == focus {
                                        ui.painter().rect_filled(
                                            rect,
                                            egui::CornerRadius::same(4),
                                            p.row_hover,
                                        );
                                    }
                                    if resp.hovered() {
                                        hover_focus = Some(entry_i);
                                    }
                                    if resp.clicked() {
                                        activated = Some(entry_i);
                                    }
                                    ui.painter().text(
                                        egui::pos2(
                                            rect.min.x + theme::CTX_ITEM_PAD_X,
                                            rect.center().y,
                                        ),
                                        egui::Align2::LEFT_CENTER,
                                        e.glyph,
                                        egui::FontId::proportional(theme::CTX_ICON),
                                        p.text2,
                                    );
                                    ui.painter().text(
                                        egui::pos2(
                                            rect.min.x
                                                + theme::CTX_ITEM_PAD_X
                                                + theme::CTX_ICON
                                                + theme::CTX_ITEM_GAP,
                                            rect.center().y,
                                        ),
                                        egui::Align2::LEFT_CENTER,
                                        &e.label,
                                        font14.clone(),
                                        p.text,
                                    );
                                    if !e.shortcut.is_empty() {
                                        ui.painter().text(
                                            egui::pos2(
                                                rect.max.x - theme::CTX_ITEM_PAD_X,
                                                rect.center().y,
                                            ),
                                            egui::Align2::RIGHT_CENTER,
                                            e.shortcut,
                                            font12.clone(),
                                            p.text3,
                                        );
                                    }
                                    entry_i += 1;
                                }
                            }
                        }
                    });
            });

        // 焦点/激活回写：悬停接管焦点（Windows 菜单惯例，Enter 激活悬停项）；
        // 激活 → 关闭菜单并执行动作；点击菜单外 → 仅关闭。
        if let Some(i) = hover_focus {
            if let Some(state) = self.ctx_menu.as_mut() {
                state.focus = i;
            }
        }
        if let Some(i) = activated {
            if let Some(state) = self.ctx_menu.as_mut() {
                state.focus = i;
            }
            self.activate_ctx_menu(ctx);
        } else if let Some(pos) = secondary_at {
            self.ctx_menu = None;
            self.reopen_ctx_menu_at(ctx, pos);
        } else if dismissed {
            self.ctx_menu = None;
        }
    }
}
