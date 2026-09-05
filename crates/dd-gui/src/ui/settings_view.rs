//! 设置页视图与设置卡片族（设计稿 12）。

use crate::app::PaletteApp;
use crate::ui::widgets::draw_back_btn;
use crate::ui::widgets::text_width;
use crate::ui::widgets::PlaceholderSuffix;
use dd_gui::theme;
use eframe::egui;

impl PaletteApp {
    /// 设置页（§08 v4.2）：顶行三段式（返回 + 标题 + 版本徽标）+ 两张 settings-card
    /// （主题三选 + 占位项）。**键位提示由全局页脚统一渲染**（draw_status_footer
    /// 在 is_settings 时显示"修改主题立即生效并持久化" + Esc 返回）。
    ///
    /// 设计稿 §08.1 验收 B1：选中态 = 2px accent + accent_soft 底 + 实心圆点；
    /// 未选 = 1px border-strong + input_fill 底 + 空心圆点（1.5px stroke）。
    /// 选择即生效：点击 radio-card → 立即 `apply_theme_pref`（set_theme + save）。
    pub(crate) fn draw_settings(&mut self, ui: &mut egui::Ui) {
        let p = theme::Palette::of(ui.visuals().dark_mode);
        let dark = ui.visuals().dark_mode;

        // ── 顶行（D3 + §08.1）：40px 高，与 01 / 07 顶行同构 ──
        // 真机 2026-09-04 修复"标题上面空了很多"：旧实现先 allocate 40px 再另起
        // horizontal 行——40px 成了死空间。改为在 40px 行矩形内手动锚定中心线 cy
        // （返回按钮 28×28 子区、标题 painter 直绘、版本徽标精确子区，全部居中）。
        let total_w = ui.available_width();
        let (row_rect, _) = ui.allocate_exact_size(
            egui::vec2(total_w, theme::SEARCHBAR_H),
            egui::Sense::hover(),
        );
        let cy = row_rect.center().y;

        // 返回按钮：28×28 子区恰好填满（中心 = cy）
        let back_rect = egui::Rect::from_min_size(
            egui::pos2(row_rect.min.x, cy - 14.0),
            egui::vec2(28.0, 28.0),
        );
        let mut back_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(back_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Min)),
        );
        let back_clicked = draw_back_btn(&mut back_ui, &p);

        // 页标题：painter 直绘锚定 cy（body 16px；与返回按钮间距 = `.toprow`
        // gap 8px，与嵌套页 back→searchbar 间距同口径）
        ui.painter().text(
            egui::pos2(back_rect.right() + 8.0, cy),
            egui::Align2::LEFT_CENTER,
            "设置",
            egui::FontId::proportional(16.0),
            p.text,
        );

        // 版本徽标：精确尺寸子区贴右缘垂直居中（draw_version_chip 恰好填满）
        let ver_text = format!("v{}", env!("CARGO_PKG_VERSION"));
        // 宽度估算必须与 draw_ext_chip 的渲染字体一致（monospace）——曾错用
        // proportional 导致 chip_rect 偏窄 ~6px，allocate 溢出 max_rect 向右多画，
        // 胶囊右缘越过内容边线、距窗口边框只剩 ~5px（真机 2026-09-04 反馈）。
        let chip_w = text_width(ui, &ver_text, egui::FontId::monospace(10.0)) + 16.0;
        let chip_rect = egui::Rect::from_min_size(
            egui::pos2(row_rect.right() - chip_w, cy - 8.0),
            egui::vec2(chip_w, 16.0),
        );
        let mut chip_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(chip_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Min)),
        );
        draw_version_chip(&mut chip_ui, env!("CARGO_PKG_VERSION"), &p);

        if back_clicked {
            self.stack.go_back();
        }
        ui.add_space(8.0); // 顶行 padding-bottom

        // ── settings-card #1：主题外观（setting-row 头部 + radio-cards） ──
        let mut pick: Option<dd_gui::settings::ThemePref> = None;
        draw_settings_card_frame(ui, &p, |card| {
            // 主题 icon + 名称 + 描述（16px / 14/20 / 12/16 fg-2 / text / text-3）
            // item_spacing.x 清零：gap 严格 12px（§08 CSS setting-row gap），不受
            // egui 默认 8px item_spacing 叠加影响。
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    '\u{E790}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0); // gap 12（§08 CSS line 470 setting-row gap）
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(egui::RichText::new("主题外观").size(14.0).color(p.text));
                    ui.label(
                        egui::RichText::new("选择亮暗主题；「跟随系统」随 Windows 主题实时切换")
                            .size(12.0)
                            .color(p.text3),
                    );
                });
            });
            card.add_space(8.0);
            // radio-cards（三张等宽，gap 8px）
            // 真机 2026-09-04 修复"右边框线被盖/超出"：宽度必须在**卡片内**实测
            // （外层 total_w 未扣卡片 padding/描边，且 egui item_spacing 8px 会叠加
            // 在 add_space 之上，导致三卡总宽超卡内宽、盖住右边框）——
            // 本行 item_spacing.x 清零、间隙全部手动控制，宽度 = (内宽 - 2×gap)/3。
            let gap = 8.0;
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let avail = ui.available_width();
                let card_w = ((avail - 2.0 * gap) / 3.0).floor().max(64.0);
                let prefs = [
                    (
                        dd_gui::settings::ThemePref::System,
                        "跟随系统",
                        "System",
                        // swatch 系统卡显示亮 + 暗双本色
                        [
                            egui::Color32::WHITE,
                            egui::Color32::from_rgb(0x1b, 0x1b, 0x1b),
                        ],
                    ),
                    (
                        dd_gui::settings::ThemePref::Light,
                        "亮色",
                        "Light",
                        // 亮色主题卡：白 + 浅灰
                        [
                            egui::Color32::WHITE,
                            egui::Color32::from_rgb(0xf4, 0xf4, 0xf4),
                        ],
                    ),
                    (
                        dd_gui::settings::ThemePref::Dark,
                        "暗色",
                        "Dark",
                        // 暗色主题卡：灰[16] + 灰[12]
                        [
                            egui::Color32::from_rgb(0x29, 0x29, 0x29),
                            egui::Color32::from_rgb(0x1f, 0x1f, 0x1f),
                        ],
                    ),
                ];
                for (i, (pref, zh, en, sw)) in prefs.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(gap);
                    }
                    let selected = self.settings.theme == *pref;
                    if draw_radio_card(ui, card_w, zh, en, *sw, selected, &p, dark) {
                        pick = Some(*pref);
                    }
                }
            });
        });
        if let Some(pref) = pick {
            self.apply_theme_pref(ui.ctx(), pref);
        }
        ui.add_space(8.0); // 两卡间距 8px（§08.1 line 467 margin-top）

        // ── settings-card #1.5：打开面板时的首屏视图（设置项，真机反馈 2026-09-04）──
        let mut show_all = self.settings.open_view == dd_gui::settings::OpenView::All;
        let mut view_changed = false;
        draw_settings_card_frame(ui, &p, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    '\u{E8A9}', // Fluent "Home" 图标码位
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        egui::RichText::new("打开面板时显示")
                            .size(14.0)
                            .color(p.text),
                    );
                    ui.label(
                        egui::RichText::new(
                            "「默认功能」只显示计算、网页搜索等入口；输入查询时应用仍会参与匹配",
                        )
                        .size(12.0)
                        .color(p.text3),
                    );
                });
            });
            card.add_space(4.0);
            card.horizontal(|ui| {
                if ui.checkbox(&mut show_all, "显示所有应用").changed() {
                    view_changed = true;
                }
            });
        });
        if view_changed {
            self.apply_open_view(ui.ctx(), show_all);
        }
        ui.add_space(8.0);

        // ── settings-card #2：占位项（全局热键 / 开机自启 / 扩展管理，disabled） ──
        draw_settings_card_frame(ui, &p, |card| {
            draw_setting_row_disabled(
                card,
                '\u{E92E}',
                "全局热键",
                "自定义唤起快捷键（当前固定 Alt+Space）",
                PlaceholderSuffix::Soon,
                &p,
            );
            draw_setting_row_disabled(
                card,
                '\u{E7E8}',
                "开机自启",
                "登录 Windows 后自动后台运行",
                PlaceholderSuffix::DisabledSwitch,
                &p,
            );
            draw_setting_row_disabled(
                card,
                '\u{E74E}',
                "扩展管理",
                "启用/禁用扩展、查看扩展清单与版本",
                PlaceholderSuffix::Soon,
                &p,
            );
        });
    }
}

/// 徽标 chip（设计稿 CSS `.ext-chip` line 243 + §08 mockup）：文字 +
/// 1px border 描边 + 胶囊圆角（radius-circular）+ 1px/8px padding，
/// mini 10px monospace + fg3 + chip 底 + 高度 16px。返回实测矩形。
///
/// 用途：设置页顶行版本徽标（`v{version}`）、嵌套页页脚右端 `ext_id`
/// 徽标（§07.1，C 组批次 C1）。
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
    body: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(p.card)
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

/// 单张 radio-card（§08.1 "主题单选"）：圆点 12×12 + 名称 (caption1 12/16)
/// + 副标题 (mini 10/14 fg-3) + swatch (16px 高两色色板)。
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
    swatch_colors: [egui::Color32; 2],
    selected: bool,
    p: &theme::Palette,
    dark: bool,
) -> bool {
    // 高度 = padding 10 top + name 20 + sub 14 + gap 8 + swatch 16 + padding 10 bot = 78
    let h = 78.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
    let radius = egui::CornerRadius::same(8);

    // 底色：未选 = input_fill；选中 = accent 软色
    if selected {
        ui.painter().rect_filled(rect, radius, accent_soft(dark, p));
    } else {
        ui.painter().rect_filled(rect, radius, p.input_fill);
    }
    // hover（未选）：叠加 row_hover 暗示可点
    if !selected && resp.hovered() {
        ui.painter()
            .rect_filled(rect.shrink(1.0), radius, p.row_hover);
    }
    // 边框
    let stroke = if selected {
        egui::Stroke::new(2.0, p.accent)
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
            egui::Stroke::new(1.5, p.accent),
        );
        ui.painter()
            .circle_filled(egui::pos2(dot_cx, dot_cy), 3.5, p.accent);
    } else {
        ui.painter().circle_stroke(
            egui::pos2(dot_cx, dot_cy),
            6.0,
            egui::Stroke::new(1.5, p.border_strong),
        );
    }
    // 名称（caption1 12/16、text 色）
    let name_x = dot_cx + 12.0; // 圆点右 6 + 文字前内 padding 6
    let name_y = rect.top() + 12.0;
    ui.painter().text(
        egui::pos2(name_x, name_y),
        egui::Align2::LEFT_TOP,
        label,
        egui::FontId::proportional(12.0),
        p.text,
    );
    // 副标题（mini 10/14、text-3）
    ui.painter().text(
        egui::pos2(name_x, name_y + 16.0),
        egui::Align2::LEFT_TOP,
        sub,
        egui::FontId::proportional(10.0),
        p.text3,
    );
    // swatch 两色色板（16px 高 + gap 4 + 各自 1px stroke）
    let sw_y = rect.bottom() - 10.0 - 16.0;
    let sw_w = ((rect.width() - 24.0 - 4.0) / 2.0).max(8.0); // 横向 padding 12 两侧 + gap 4
    let sw_left = rect.left() + 12.0;
    for (i, c) in swatch_colors.iter().enumerate() {
        let sx = sw_left + i as f32 * (sw_w + 4.0);
        let srect = egui::Rect::from_min_size(egui::pos2(sx, sw_y), egui::vec2(sw_w, 16.0));
        ui.painter()
            .rect_filled(srect, egui::CornerRadius::same(4), *c);
        ui.painter().rect_stroke(
            srect,
            egui::CornerRadius::same(4),
            egui::Stroke::new(1.0, p.border),
            egui::StrokeKind::Inside,
        );
    }
    resp.clicked()
}

/// 占位设置行（§08.1 line 1238 "占位项"）：图标 16px (fg-disabled) +
/// 名称 (body1 14/20 fg-disabled) + 描述 (caption1 12/16 fg-disabled) +
/// 尾缀（Soon chip 或 Disabled Toggle）。
///
/// **不注册任何点击处理**——占位项不绑定行为，仅做视觉占位。
pub(crate) fn draw_setting_row_disabled(
    ui: &mut egui::Ui,
    icon: char,
    name: &str,
    desc: &str,
    suffix: PlaceholderSuffix,
    p: &theme::Palette,
) {
    let h = 48.0;
    // 注意：必须用 `allocate_exact_size` 返回的 rect 作为行矩形。
    // 之前误用 `ui.min_rect()`——它是 Ui 创建以来的**累计**包围盒，随每行
    // 递增膨胀，导致三行文字全部绘制到第一行位置（真机 2026-09-04 重叠）。
    let (row, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::hover());

    // 图标 16px（fg-disabled）
    ui.painter().text(
        egui::pos2(row.left() + 4.0, row.center().y),
        egui::Align2::LEFT_CENTER,
        icon,
        egui::FontId::proportional(16.0),
        p.text_disabled,
    );
    // 名称 + 描述
    let main_x = row.left() + 28.0; // 4 left pad + 16 icon + 8 gap
    let main_y = row.top() + 8.0;
    ui.painter().text(
        egui::pos2(main_x, main_y),
        egui::Align2::LEFT_TOP,
        name,
        egui::FontId::proportional(14.0),
        p.text_disabled,
    );
    ui.painter().text(
        egui::pos2(main_x, main_y + 20.0),
        egui::Align2::LEFT_TOP,
        desc,
        egui::FontId::proportional(12.0),
        p.text_disabled,
    );

    match suffix {
        PlaceholderSuffix::Soon => draw_soon_chip_at(ui, row, p),
        PlaceholderSuffix::DisabledSwitch => draw_disabled_switch_at(ui, row, p),
    }
}

/// 「即将支持」chip（mini 10/14 + text-3 + 1px border + 胶囊 + padding 1px 8px），
/// 贴给定 row 的右内边缘绘制。
pub(crate) fn draw_soon_chip_at(ui: &mut egui::Ui, row: egui::Rect, p: &theme::Palette) {
    let text = "即将支持";
    let text_w = text_width(ui, text, egui::FontId::proportional(10.0));
    let w = text_w + 16.0;
    let h = 16.0;
    let rect = egui::Rect::from_min_size(
        egui::pos2(row.right() - 4.0 - w, row.center().y - h / 2.0),
        egui::vec2(w, h),
    );
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(8), p.card);
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
        egui::FontId::proportional(10.0),
        p.text3,
    );
}

/// 禁用 Toggle（36×18 + 1px border-strong + 12×12 fg-disabled 内圆），
/// 贴右绘制。
pub(crate) fn draw_disabled_switch_at(ui: &mut egui::Ui, row: egui::Rect, p: &theme::Palette) {
    let w = 36.0;
    let h = 18.0;
    let rect = egui::Rect::from_min_size(
        egui::pos2(row.right() - 4.0 - w, row.center().y - h / 2.0),
        egui::vec2(w, h),
    );
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(9), p.card);
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(9),
        egui::Stroke::new(1.0, p.border_strong),
        egui::StrokeKind::Inside,
    );
    ui.painter().circle_filled(
        egui::pos2(rect.left() + 8.0, rect.center().y),
        6.0,
        p.text_disabled,
    );
}
