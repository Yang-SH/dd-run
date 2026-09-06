//! 设置页视图与设置卡片族（设计稿 08，v4.6 左右布局：左栏分组 + 右侧设置项）。

use crate::app::PaletteApp;
use crate::ui::widgets::draw_back_btn;
use crate::ui::widgets::text_width;
use dd_gui::theme;
use eframe::egui;

/// 设置页左栏栏目（设计稿 08 v4.6，D27）：**纯视图状态**——不落盘、不改协议；
/// 每次进入设置页经 `open_settings` 重置到首栏「外观」（B5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SettingsCategory {
    /// 外观：主题三选。
    #[default]
    Appearance,
    /// 常规：打开面板时显示 + 热键/自启占位。
    General,
    /// 搜索：搜索引擎配置。
    Search,
    /// 扩展：扩展管理占位（后续扩展配置的预留归组位）。
    Extensions,
}

/// 左栏栏目表（§08.1 v4.6「栏目与内容映射」）：顺序 = 栏目序（B5 验收），
/// 图标码位与对应卡片图标一致（外观 E790 / 搜索 E721 / 扩展 E74E）。
/// 第三元 = i18n key（v4.13 D38），绘制时经 `text::t` 按生效语言解析。
const NAV_CATS: [(SettingsCategory, char, &str); 4] = [
    (SettingsCategory::Appearance, '\u{E790}', "nav.appearance"),
    (SettingsCategory::General, '\u{E713}', "nav.general"),
    (SettingsCategory::Search, '\u{E721}', "nav.search"),
    (SettingsCategory::Extensions, '\u{E74E}', "nav.extensions"),
];

/// 左栏几何（§08.1 v4.6，B4 像素规格）：宽 168、项高 36、项间距 4、分栏间距 8。
const NAV_W: f32 = 168.0;
const NAV_ITEM_H: f32 = 36.0;
const NAV_GAP: f32 = 4.0;
const SPLIT_GAP: f32 = 8.0;

impl PaletteApp {
    /// 设置页（§08 v4.6 左右布局，D27/D28）：顶行三段式（返回 + 标题 + 版本徽标，
    /// 不变）+ `[左栏 168px][间距 8][内容区 flex 1]` 水平两栏。顶行与左栏固定、
    /// 内容区独立滚动（B6）；左栏四类栏目（NavigationView pane 语义：选中 =
    /// row_selected 实色底 + 左缘 3×16 accent 指示条 + 图标/文字转 text，B4）。
    /// **键位提示由全局页脚统一渲染**（draw_status_footer 在 is_settings 时显示
    /// "修改主题立即生效并持久化" + Esc 返回）。
    pub(crate) fn draw_settings(&mut self, ui: &mut egui::Ui) {
        let p = theme::Palette::of(ui.visuals().dark_mode);

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

        // 页标题：painter 直绘锚定 cy（v4.9：16 → 18，真机反馈"整体文字偏小"；
        // Fluent subtitle 档，40px 顶行内仍居中）
        ui.painter().text(
            egui::pos2(back_rect.right() + 8.0, cy),
            egui::Align2::LEFT_CENTER,
            self.tr("page.settings"),
            egui::FontId::proportional(18.0),
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

        // ── 左右分栏（§08 v4.6 D27）：[左栏 168][间距 8][内容区 flex 1] ──
        // 顶行与左栏固定、内容区独立滚动（B6）；右/下留 12px（.settings-split
        // padding 口径）。ctx 句柄预克隆：引擎卡片的 |card| 闭包内不能再用外层
        // `ui`（已被 draw_settings_card_frame 的 &mut 借用），改用克隆的 Context
        // 句柄（廉价，egui::Context 内部 Arc），避免 ui 借用冲突。
        let ctx = ui.ctx().clone();
        let split = ui.available_rect_before_wrap();
        let split_rect = egui::Rect::from_min_max(
            split.min,
            egui::pos2(split.right() - 12.0, split.bottom() - 12.0),
        );
        let nav_rect = egui::Rect::from_min_size(split.min, egui::vec2(NAV_W, split_rect.height()));
        let content_rect = egui::Rect::from_min_max(
            egui::pos2(split.min.x + NAV_W + SPLIT_GAP, split.min.y),
            split_rect.max,
        );

        // ── 左栏：四类栏目（NavigationView pane 语义，§08.1 v4.6）──
        // 项高 36、圆角 4、图标 16 + 文字 14/20（B4）；点击 = 本期唯一栏目切换
        // 交互（↑↓ 焦点切换为可选增强，本期占位，见 8.1 键盘行）。
        let mut nav_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(nav_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        nav_ui.spacing_mut().item_spacing.y = NAV_GAP;
        for (cat, icon, label_key) in NAV_CATS {
            let selected = self.settings_category == cat;
            let label = crate::text::t(self.lang_effective, label_key);
            let (item_rect, resp) =
                nav_ui.allocate_exact_size(egui::vec2(NAV_W, NAV_ITEM_H), egui::Sense::click());
            let radius = egui::CornerRadius::same(4);
            if selected {
                nav_ui
                    .painter()
                    .rect_filled(item_rect, radius, p.row_selected);
            } else if resp.hovered() {
                nav_ui.painter().rect_filled(item_rect, radius, p.row_hover);
            }
            if selected {
                // 左缘 3×16 accent 指示条（radius 2，垂直居中）——与列表行选中
                // 语言（D9）同构，未引入新 token。
                let indicator = egui::Rect::from_min_size(
                    egui::pos2(item_rect.left(), item_rect.center().y - 8.0),
                    egui::vec2(3.0, 16.0),
                );
                nav_ui
                    .painter()
                    .rect_filled(indicator, egui::CornerRadius::same(2), p.accent);
            }
            // 图标 16px：内边距 12 + 槽位 16 居中；文字：图标右 12 起（左+40）。
            let fg = if selected { p.text } else { p.text2 };
            let cy = item_rect.center().y;
            nav_ui.painter().text(
                egui::pos2(item_rect.left() + 20.0, cy),
                egui::Align2::CENTER_CENTER,
                icon,
                egui::FontId::proportional(16.0),
                fg,
            );
            nav_ui.painter().text(
                egui::pos2(item_rect.left() + 40.0, cy),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(14.0),
                fg,
            );
            if resp.clicked() {
                self.settings_category = cat;
            }
        }

        // ── 内容区：按栏目分发（独立 ScrollArea；卡片族规格沿用 v4.2）──
        let mut content_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        // v4.9（真机反馈"整体文字偏小"）：设置页内**未显式指定字号**的控件文本
        // （checkbox / ComboBox / TextEdit / 弹出菜单项等 egui 默认 12.5）统一提到
        // 14（Fluent body 档）；显式 `.size()` 的标题/描述不受影响。
        {
            let styles = content_ui.style_mut();
            styles
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
            styles
                .text_styles
                .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        }
        egui::ScrollArea::vertical().show(&mut content_ui, |ui| match self.settings_category {
            SettingsCategory::Appearance => {
                self.draw_appearance_card(ui, &p);
                self.draw_material_card(ui, &p, &ctx);
            }
            SettingsCategory::General => self.draw_general_cards(ui, &p),
            SettingsCategory::Search => self.draw_search_engine_card(ui, &p, &ctx),
            SettingsCategory::Extensions => self.draw_extensions_card(ui, &p),
        });
    }

    /// 外观栏：主题外观卡（radio-card 三选，§08.1 沿用；验收 B1/B2）。
    ///
    /// 选中态 = 2px accent + accent_soft 底 + 实心圆点；未选 = 1px
    /// border-strong + input_fill 底 + 空心圆点（1.5px stroke）。
    /// 选择即生效：点击 radio-card 立即 `apply_theme_pref`（set_theme + save）。
    fn draw_appearance_card(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        let dark = ui.visuals().dark_mode;
        let lang = self.lang_effective;
        let mut pick: Option<dd_gui::settings::ThemePref> = None;
        draw_settings_card_frame(ui, p, |card| {
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
                ui.add_space(12.0); // gap 12（§08 CSS setting-row gap）
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        egui::RichText::new(crate::text::t(lang, "set.theme.name"))
                            .size(14.0)
                            .color(p.text),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(crate::text::t(lang, "set.theme.desc"))
                                .size(12.0)
                                .color(p.text3),
                        )
                        .wrap(),
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
                        crate::text::t(lang, "set.theme.follow"),
                        "System",
                        // swatch 系统卡显示亮 + 暗双本色
                        [
                            egui::Color32::WHITE,
                            egui::Color32::from_rgb(0x1b, 0x1b, 0x1b),
                        ],
                    ),
                    (
                        dd_gui::settings::ThemePref::Light,
                        crate::text::t(lang, "set.theme.light"),
                        "Light",
                        // 亮色主题卡：白 + 浅灰
                        [
                            egui::Color32::WHITE,
                            egui::Color32::from_rgb(0xf4, 0xf4, 0xf4),
                        ],
                    ),
                    (
                        dd_gui::settings::ThemePref::Dark,
                        crate::text::t(lang, "set.theme.dark"),
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
                    if draw_radio_card(ui, card_w, zh, en, *sw, selected, p, dark) {
                        pick = Some(*pref);
                    }
                }
            });
        });
        if let Some(pref) = pick {
            self.apply_theme_pref(ui.ctx(), pref);
        }
    }

    /// 外观栏：「窗口材质」卡（v4.7 D30/D31）——云母 / 亚克力两个 ToggleSwitch
    /// 行，互斥·后开优先：开关状态由单值 `backdrop` 派生（与该值比较），点击
    /// 已开项 → 关（None），点击未开项 → 开（该项）；变更经 `apply_backdrop`
    /// 即时生效 + 落盘（默认云母，D30）。
    fn draw_material_card(&mut self, ui: &mut egui::Ui, p: &theme::Palette, ctx: &egui::Context) {
        // 开关状态在闭包外读取、闭包内只收集点击结果（避免闭包内 &mut self 冲突）。
        let mica_on = self.settings.backdrop == dd_gui::settings::Backdrop::Mica;
        let acrylic_on = self.settings.backdrop == dd_gui::settings::Backdrop::Acrylic;
        let lang = self.lang_effective;
        let mut picked: Option<dd_gui::settings::Backdrop> = None;
        draw_settings_card_frame(ui, p, |card| {
            // 卡头：图标 + 名称 + 描述（行内 spacing.x 清零，同主题卡口径）
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    '\u{E771}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        egui::RichText::new(crate::text::t(lang, "set.backdrop.name"))
                            .size(14.0)
                            .color(p.text),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(crate::text::t(lang, "set.backdrop.desc"))
                                .size(12.0)
                                .color(p.text3),
                        )
                        .wrap(),
                    );
                });
            });
            // 开关行 ×2：名称 + 描述 + 贴右功能态开关（§08.1 v4.7「材质开关」行）
            // v4.15 真机反馈修复：①卡头 → 首个开关行 4px 间距；②每个开关行内
            // 标题/描述垂直垂直块 name ↔ desc 间 4px；③两个开关行之间 8px 间距
            //（避免「云母材质」与「亚克力材质」黏在一起）；④每行统一
            // `set_min_height(36)` 仍是底线高度，名+描 + 4px 自然撑开 ≥40，视觉
            // 与其他设置卡保持一致节奏。
            for (i, (backdrop, name, desc, on)) in [
                (
                    dd_gui::settings::Backdrop::Mica,
                    crate::text::t(lang, "set.backdrop.mica"),
                    crate::text::t(lang, "set.backdrop.mica.desc"),
                    mica_on,
                ),
                (
                    dd_gui::settings::Backdrop::Acrylic,
                    crate::text::t(lang, "set.backdrop.acrylic"),
                    crate::text::t(lang, "set.backdrop.acrylic.desc"),
                    acrylic_on,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                if i == 0 {
                    card.add_space(4.0); // 卡头 → 首开关行 4px
                } else {
                    card.add_space(8.0); // 开关行间 8px（真机反馈修复）
                }
                let mut clicked = false;
                card.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    // 与卡头图标对齐的 16px 空槽位（设计稿演示同构）
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.set_min_height(36.0);
                        ui.label(egui::RichText::new(name).size(14.0).color(p.text));
                        ui.add_space(4.0); // name ↔ desc 间距（修复"标题与描述黏"）
                        ui.label(egui::RichText::new(desc).size(12.0).color(p.text3));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        clicked = draw_switch_fn(ui, on, p);
                    });
                });
                if clicked {
                    // 互斥·后开优先（D30）：已开 → 关；未开 → 开（自动挤掉另一项）
                    picked = Some(if on {
                        dd_gui::settings::Backdrop::None
                    } else {
                        backdrop
                    });
                }
            }
        });
        if let Some(backdrop) = picked {
            self.apply_backdrop(ctx, backdrop);
        }
    }

    /// 常规栏（v4.8 功能态 + v4.9 Fluent 控件化）：「打开面板时显示」+
    /// 「全局热键」（可改：更改 = 捕获模式、恢复默认 = Win+Alt+Space 一键还原）+
    /// 「开机自启」（功能态开关）+「语言」（v4.13 D38，ComboBox 三选）。
    /// 动作入口在 `app/keys.rs`（M6 批次 6.3；语言切换 apply_lang）。
    fn draw_general_cards(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        let lang = self.lang_effective;
        // ── 卡 1：打开面板时显示 ──
        let mut show_all = self.settings.open_view == dd_gui::settings::OpenView::All;
        let mut view_changed = false;
        draw_settings_card_frame(ui, p, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    '\u{E8A9}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        egui::RichText::new(crate::text::t(lang, "set.openview.name"))
                            .size(14.0)
                            .color(p.text),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(crate::text::t(lang, "set.openview.desc"))
                                .size(12.0)
                                .color(p.text3),
                        )
                        .wrap(),
                    );
                });
            });
            card.add_space(8.0);
            card.horizontal(|ui| {
                ui.add_space(28.0);
                if ui
                    .checkbox(&mut show_all, crate::text::t(lang, "set.openview.all"))
                    .changed()
                {
                    view_changed = true;
                }
            });
        });
        if view_changed {
            self.apply_open_view(ui.ctx(), show_all);
        }
        ui.add_space(8.0); // 两卡间距 8px（§08.1 卡片间距）

        // ── 卡 2：全局热键 ──
        // 闭包外快照（闭包内只收集点击结果）；键帽标签拆分 modifiers 与主键。
        // v4.14 修复热键行溢出：原实现让 combo（标签+键帽）按自然宽度铺开，
        // 末尾再 `right_to_left` 放按钮——面板窄时（如 340px）"Space" 键帽会
        // 越过按钮区、视觉上压到 [Reset][Change] 上。修复方式：**预算按钮区
        // 固定宽度**，把 combo 区域用 `allocate_ui` 限制在剩余宽度内；超出
        // 键帽在 combo 边界被裁，不侵入按钮区。
        // 按钮宽度预算在闭包外计算（闭包内 `text_width(ui,…)` 借外层 ui，
        // 与闭包同时持 `card` 的可变借用冲突——E0502）。
        let capturing = self.hotkey_capturing;
        let mods_label = dd_gui::settings::hotkey_mods_label(self.settings.hotkey_mods);
        let vk_label = dd_gui::settings::hotkey_vk_label(self.settings.hotkey_vk);
        let mut caps: Vec<&str> = mods_label.split('+').collect();
        caps.push(vk_label.as_str());
        let mut capture_clicked = false;
        let mut default_clicked = false;
        let change_text_btn = if capturing {
            crate::text::t(lang, "set.hotkey.capturing_btn")
        } else {
            crate::text::t(lang, "set.hotkey.change")
        };
        let reset_text_btn = crate::text::t(lang, "set.hotkey.reset");
        let change_w = text_width(ui, change_text_btn, egui::FontId::proportional(14.0)) + 24.0;
        let reset_w = text_width(ui, reset_text_btn, egui::FontId::proportional(14.0)) + 24.0;
        // 8 = Change 与 Reset 之间的 gap，4 = 卡片内右边距
        let buttons_total_w = change_w + reset_w + 8.0 + 4.0;
        draw_settings_card_frame(ui, p, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    '\u{E92E}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        egui::RichText::new(crate::text::t(lang, "set.hotkey.name"))
                            .size(14.0)
                            .color(p.text),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(crate::text::t(lang, "set.hotkey.desc"))
                                .size(12.0)
                                .color(p.text3),
                        )
                        .wrap(),
                    );
                });
            });
            card.add_space(4.0);
            // 当前组合行：「当前组合」+ 大号键帽 …… 右侧 [恢复默认][更改]
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(28.0);
                ui.set_min_height(36.0);
                // combo 区：宽度 = available - buttons_total_w，下限 80px
                let combo_w = (ui.available_width() - buttons_total_w).max(80.0);
                ui.allocate_ui(egui::vec2(combo_w, 36.0), |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(
                            egui::RichText::new(crate::text::t(lang, "set.hotkey.current"))
                                .size(14.0)
                                .color(p.text2),
                        );
                        ui.add_space(12.0);
                        if capturing {
                            ui.label(
                                egui::RichText::new(crate::text::t(lang, "set.hotkey.capturing"))
                                    .size(14.0)
                                    .color(p.accent),
                            );
                        } else {
                            for (i, cap) in caps.iter().enumerate() {
                                if i > 0 {
                                    ui.add_space(4.0);
                                    ui.label(egui::RichText::new("+").size(12.0).color(p.text3));
                                    ui.add_space(4.0);
                                }
                                draw_keycap(ui, cap, p);
                            }
                        }
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(4.0);
                    // v4.9：Fluent 标准次级按钮（32px/文字 14）——旧 26px/12px
                    // 纯文字样式不符合 Fluent 控件规范（真机 2026-09-06 反馈
                    // "恢复默认/更改不像按钮"）。
                    if fluent_button(ui, change_text_btn, p) {
                        capture_clicked = true;
                    }
                    // v4.15 三轮反馈：两按钮贴在一起——外层 horizontal 已置
                    // item_spacing.x=0 且被本子 ui 继承，按钮间需显式 8px
                    // （buttons_total_w 预算里已含此 8）。
                    ui.add_space(8.0);
                    // Win 修饰不在可捕获集（egui Windows 不暴露 Win 键 modifiers），
                    // 默认组合 Win+Alt+Space 经此按钮一键还原。
                    if !capturing && fluent_button(ui, reset_text_btn, p) {
                        default_clicked = true;
                    }
                });
            });
        });
        if capture_clicked {
            self.start_hotkey_capture();
        }
        if default_clicked {
            self.apply_hotkey_default();
        }
        ui.add_space(8.0);

        // ── 卡 3：开机自启（功能态开关）──
        let autostart_on = self.settings.autostart;
        let mut autostart_toggled = false;
        draw_settings_card_frame(ui, p, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    '\u{E7E8}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        egui::RichText::new(crate::text::t(lang, "set.autostart.name"))
                            .size(14.0)
                            .color(p.text),
                    );
                    ui.add_space(2.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(crate::text::t(lang, "set.autostart.desc"))
                                .size(12.0)
                                .color(p.text3),
                        )
                        .wrap(),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    autostart_toggled = draw_switch_fn(ui, autostart_on, p);
                });
            });
        });
        if autostart_toggled {
            self.apply_autostart(!autostart_on);
        }

        // ── 卡 4：语言（v4.13 D38 + v4.15 真机反馈修）── ComboBox 三选
        //（跟随系统/简体中文/English，宽 180 高 32 与搜索引擎 ComboBox 同规格）；
        // 切换经 apply_lang 即时生效。语言卡自身文案也走 t()（当前生效语言）——
        // 切换后本卡文案随语言刷新，是 i18n 端到端的第一个验证点。
        //
        // v4.15 真机反馈修复：①左侧描述 Label 默认 WrapMode=Extend，长英文
        // 描述自然延伸覆盖右侧 ComboBox——加 `.wrap()` + 左列 `allocate_ui` 锁宽
        // = 描述在 `avail - combo_w - 16` 范围内换行、不再侵入下拉；②egui
        // `ComboBox` 视觉过于 native、控件高度自适应会盖住左侧标题——自绘
        // `draw_fluent_dropdown` 锁 32 高、统一 Fluent 2 控件库口径（圆角 4
        // / 1px border-strong / 右侧 ▼ ChevronDown / popup_below_widget 自动
        // 处理外部点击收起）。
        ui.add_space(8.0);
        let lang_pref = self.settings.lang;
        let lang_eff = self.lang_effective;
        let mut lang_picked: Option<dd_gui::settings::Lang> = None;
        let labels = [
            crate::text::t(lang_eff, "lang.follow_system"),
            crate::text::t(lang_eff, "lang.zh_cn"),
            crate::text::t(lang_eff, "lang.en_us"),
        ];
        // Lang 序：FollowSystem=0 / ZhCn=1 / EnUs=2，与 labels 严格对齐。
        let selected_idx = lang_pref as usize;
        let combo_w: f32 = 180.0;
        draw_settings_card_frame(ui, p, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                // 左：图标 + 标题描述（强制宽度 = available - combo_w - gap，
                // 确保描述 Label 在此范围内 wrap，不再延伸覆盖右侧下拉）
                let left_w = (ui.available_width() - combo_w - 16.0).max(160.0);
                ui.allocate_ui(egui::vec2(left_w, 36.0), |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let (icon_rect, _) =
                            ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                        ui.painter().text(
                            icon_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            '\u{E774}',
                            egui::FontId::proportional(16.0),
                            p.text2,
                        );
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            ui.set_min_height(36.0);
                            ui.label(
                                egui::RichText::new(crate::text::t(lang_eff, "settings.lang.name"))
                                    .size(14.0)
                                    .color(p.text),
                            );
                            ui.add_space(2.0);
                            // 关键修复：wrap() 让长描述在左列范围内换行
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(crate::text::t(
                                        lang_eff,
                                        "settings.lang.desc",
                                    ))
                                    .size(12.0)
                                    .color(p.text3),
                                )
                                .wrap(),
                            );
                        });
                    });
                });
                // 右：自绘 Fluent 下拉（替换 egui ComboBox）
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    use dd_gui::settings::Lang;
                    if let Some(idx) =
                        draw_fluent_dropdown(ui, selected_idx, &labels, combo_w, p, true)
                    {
                        lang_picked = Some(match idx {
                            0 => Lang::FollowSystem,
                            1 => Lang::ZhCn,
                            _ => Lang::EnUs,
                        });
                    }
                });
            });
        });
        if let Some(l) = lang_picked {
            self.apply_lang(l);
        }
    }

    /// 搜索栏（v4.8 交互改版，用户决策）：预设引擎改**下拉框添加**、自定义
    /// 引擎改**单 URL 输入框**（名称自动取域名）+ 已启用引擎列表带删除；
    /// 替换 v4.6 的「预设勾选列表 + 名称/URL 双输入表单」。变更落盘 +
    /// engines_dirty（离开设置页重聚合，协议 v1.0 冻结零新增）。
    fn draw_search_engine_card(
        &mut self,
        ui: &mut egui::Ui,
        p: &theme::Palette,
        ctx: &egui::Context,
    ) {
        // 闭包外快照（闭包内只改 settings + 收集待删项）
        let presets = dd_gui::settings::preset_search_engines();
        let addable: Vec<dd_gui::settings::SearchEngine> = presets
            .iter()
            .filter(|pr| {
                !self
                    .settings
                    .search_engines
                    .iter()
                    .any(|e| e.name == pr.name)
            })
            .cloned()
            .collect();
        let enabled: Vec<dd_gui::settings::SearchEngine> = self.settings.search_engines.clone();
        let mut remove_name: Option<String> = None;
        let mut preset_picked: Option<String> = None;
        let mut add_custom_url: Option<String> = None;

        draw_settings_card_frame(ui, p, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    '\u{E721}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        egui::RichText::new(self.tr("set.search.name"))
                            .size(14.0)
                            .color(p.text),
                    );
                    ui.add_space(2.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(self.tr("set.search.desc"))
                                .size(12.0)
                                .color(p.text3),
                        )
                        .wrap(),
                    );
                });
            });
            card.add_space(8.0);
            // ── 已启用引擎列表：名称 + 模板截断 + 删除（v4.9：名称 14 / 模板 12、
            // 行高 32——旧 12/10 过小；删除改 Fluent 小按钮）──
            for e in &enabled {
                card.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(28.0);
                    ui.set_min_height(32.0);
                    ui.label(egui::RichText::new(&e.name).size(14.0).color(p.text));
                    ui.add_space(12.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&e.template)
                                .size(12.0)
                                .color(p.text3)
                                .monospace(),
                        )
                        .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(4.0);
                        if fluent_button_small(ui, self.tr("set.search.delete"), p) {
                            remove_name = Some(e.name.clone());
                        }
                    });
                });
            }
            if enabled.is_empty() {
                card.horizontal(|ui| {
                    ui.add_space(28.0);
                    ui.label(
                        egui::RichText::new(self.tr("set.search.none"))
                            .size(12.0)
                            .color(p.text3),
                    );
                });
            }
            card.add_space(8.0);
            // ── 添加预设引擎：下拉框（v4.9 高 32 / 宽 260——旧 180×~18 过窄
            // 过矮；v4.15 起用自绘 draw_fluent_dropdown 与语言下拉同款）──
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(28.0);
                ui.set_min_height(32.0);
                ui.label(
                    egui::RichText::new(self.tr("set.search.add_engine"))
                        .size(14.0)
                        .color(p.text2),
                );
                ui.add_space(12.0);
                ui.spacing_mut().interact_size.y = 32.0; // Fluent 控件高 32
                                                         // v4.15 二轮反馈：搜索引擎下拉与语言下拉统一 Fluent 2 控件
                                                         // 口径（替换 egui ComboBox）。labels[0] = 占位文案（「选择预
                                                         // 设引擎...」/「已全部添加」），预设名从 idx=1 起，拾取偏移 -1。
                let (dd_labels, dd_enabled): (Vec<&str>, bool) = if addable.is_empty() {
                    (vec![self.tr("set.search.presets_done")], false)
                } else {
                    let mut v = vec![self.tr("set.search.pick")];
                    v.extend(addable.iter().map(|pr| pr.name.as_str()));
                    (v, true)
                };
                if let Some(idx) = draw_fluent_dropdown(ui, 0, &dd_labels, 260.0, p, dd_enabled) {
                    if idx > 0 {
                        preset_picked = Some(addable[idx - 1].name.clone());
                    }
                }
            });
            card.add_space(8.0);
            // ── 添加自定义引擎：单 URL 输入框（v4.15 三轮反馈：旧 egui 原生
            // TextEdit 样式与整体 Fluent 风格不一致，且控件实际高度与 32px
            // 「添加」按钮基线错位（不在一条线上））。改为自绘 Fluent TextBox：
            // card 底 / 1px border-strong / 圆角 4 / 高 32 + frameless TextEdit
            // 内嵌 + 文字垂直居中；聚焦 = 底边 2px accent 下划线（WinUI 文本框
            // 聚焦口径）。「添加」为标准 32px fluent_button，同线对齐。──
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.add_space(28.0);
                let add_label = self.tr("set.search.add");
                let url_hint = self.tr("set.search.url_hint");
                let btn_w = text_width(ui, add_label, egui::FontId::proportional(14.0)) + 24.0;
                let url_w = (ui.available_width() - btn_w - 8.0).max(160.0);
                // 外框几何完全自管（allocate 精确 32 高），与按钮同高同线
                let (box_rect, _) =
                    ui.allocate_exact_size(egui::vec2(url_w, 32.0), egui::Sense::hover());
                let url_edit_id = egui::Id::new("dd-engine-url");
                let url_focused = ui.ctx().memory(|m| m.has_focus(url_edit_id));
                let radius = egui::CornerRadius::same(4);
                // 背景与描边先画（TextEdit 文字绘制在其上层）
                ui.painter().rect_filled(box_rect, radius, p.card);
                ui.painter().rect_stroke(
                    box_rect,
                    radius,
                    egui::Stroke::new(1.0, p.border_strong),
                    egui::StrokeKind::Inside,
                );
                if url_focused {
                    // Fluent 聚焦态：底边 2px accent 下划线（内缩 1px 避让描边）
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(box_rect.left() + 1.0, box_rect.bottom() - 3.0),
                            egui::pos2(box_rect.right() - 1.0, box_rect.bottom() - 1.0),
                        ),
                        egui::CornerRadius::same(1),
                        p.accent,
                    );
                }
                // frameless TextEdit 内嵌于自绘外框（egui 0.36 `.frame()` 传入
                // 即完全接管样式，不再注入 visuals 边框；margin 烘进 Frame）
                ui.put(
                    box_rect,
                    egui::TextEdit::singleline(&mut self.engine_url_buf)
                        .id(url_edit_id)
                        .desired_width(url_w - 24.0)
                        .font(egui::FontId::proportional(14.0))
                        .text_color(p.text)
                        .vertical_align(egui::Align::Center)
                        .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 0)))
                        .hint_text(url_hint),
                );
                if fluent_button(ui, add_label, p) {
                    add_custom_url = Some(self.engine_url_buf.clone());
                }
            });
            if let Some(err) = &self.engine_add_err {
                card.horizontal(|ui| {
                    ui.add_space(28.0);
                    ui.label(
                        egui::RichText::new(err.clone())
                            .size(12.0)
                            .color(egui::Color32::from_rgb(0xC4, 0x2B, 0x1C)),
                    );
                });
            }
        });

        // ── 闭包外应用交互结果 ──
        if let Some(name) = remove_name {
            self.settings.search_engines.retain(|e| e.name != name);
            self.engine_add_err = None;
            self.apply_search_engines(ctx);
        }
        if let Some(name) = preset_picked {
            if let Some(pr) = presets.iter().find(|x| x.name == name) {
                self.settings.search_engines.push(pr.clone());
                self.engine_add_err = None;
                self.apply_search_engines(ctx);
            }
        }
        if let Some(url) = add_custom_url {
            let name = url
                .strip_prefix("https://")
                .or_else(|| url.strip_prefix("http://"))
                .map(|rest| rest.split(['/', '?', '#']).next().unwrap_or(rest))
                .unwrap_or("")
                .to_string();
            match dd_gui::settings::SearchEngine::new(&name, url.trim()) {
                Some(engine) => {
                    if self
                        .settings
                        .search_engines
                        .iter()
                        .any(|e| e.name == engine.name)
                    {
                        self.engine_add_err = Some(
                            self.tr("set.search.err_exists")
                                .replace("{name}", &engine.name),
                        );
                    } else {
                        self.settings.search_engines.push(engine);
                        self.engine_url_buf.clear();
                        self.engine_add_err = None;
                        self.apply_search_engines(ctx);
                    }
                }
                None => {
                    self.engine_add_err = Some(self.tr("set.search.err_url").to_string());
                }
            }
        }
    }
    /// 扩展栏：扩展管理（v4.8 排版优化：名称独占一行，版本并入第二行
    /// 「v0.1.0 · com.ddrun.apps」，不再挤在名称后）。
    fn draw_extensions_card(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        // 闭包外拷贝数据（闭包内只收集交互结果）。运行时状态（是否熔断 Failed）
        // 一并收集，驱动「重试」按钮（协议 §11 用户手动重试，M6.4 L2）。
        let rows: Vec<(String, String, String, bool, bool)> = self
            .exts
            .iter()
            .map(|e| {
                let enabled = !self
                    .settings
                    .disabled_extensions
                    .iter()
                    .any(|x| x == &e.manifest.id);
                let failed = self
                    .sources
                    .iter()
                    .any(|s| s.id == e.manifest.id && s.status.is_failed());
                (
                    e.manifest.id.clone(),
                    e.manifest.name.clone(),
                    e.manifest.version.clone(),
                    enabled,
                    failed,
                )
            })
            .collect();
        let mut changed: Option<(String, bool)> = None;
        let mut retry_id: Option<String> = None;
        let lang = self.lang_effective;

        draw_settings_card_frame(ui, p, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    '\u{E74E}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        egui::RichText::new(crate::text::t(lang, "set.ext.name"))
                            .size(14.0)
                            .color(p.text),
                    );
                    ui.add_space(2.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(crate::text::t(lang, "set.ext.desc"))
                                .size(12.0)
                                .color(p.text3),
                        )
                        .wrap(),
                    );
                });
            });
            card.add_space(4.0);
            if rows.is_empty() {
                card.label(
                    egui::RichText::new(crate::text::t(lang, "set.ext.empty"))
                        .size(12.0)
                        .color(p.text3),
                );
            }
            for (id, name, version, enabled, failed) in &rows {
                let mut clicked = false;
                let mut retry_clicked = false;
                card.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.set_min_height(36.0);
                        // 名称独占一行（v4.8：版本不再拼在名称后）
                        ui.label(egui::RichText::new(name).size(14.0).color(p.text));
                        ui.add_space(2.0);
                        // 第二行 = 版本 · id（monospace mini，text-3）
                        ui.label(
                            egui::RichText::new(format!("v{} · {}", version, id))
                                .size(10.0)
                                .color(p.text3)
                                .monospace(),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        clicked = draw_switch_fn(ui, *enabled, p);
                        // §11 用户手动重试：熔断（连续崩溃 Failed）的扩展显示「重试」
                        // 按钮（Fluent 小按钮，贴开关左侧）→ 解除熔断并重聚合（见循环外）。
                        if *failed {
                            ui.add_space(8.0);
                            if fluent_button_small(ui, crate::text::t(lang, "set.ext.retry"), p) {
                                retry_clicked = true;
                            }
                        }
                    });
                });
                if clicked {
                    changed = Some((id.clone(), !enabled));
                }
                if retry_clicked {
                    retry_id = Some(id.clone());
                }
            }
        });
        if let Some((id, enabled)) = changed {
            self.apply_extension_enabled(&id, enabled);
        }
        if let Some(id) = retry_id {
            // 解除熔断（清零连续崩溃计数）+ 全量重聚合拉起该扩展：reset 后重聚合
            // 会重新 spawn active 扩展（含刚解除熔断者）；成功则状态恢复 Warm、重试
            // 按钮消失，仍崩溃则再次熔断、按钮复现（语义正确）。取舍：全量重聚合而非
            // 单扩展复热——复用首屏聚合机制最稳，扩展管理页操作频率低可接受（记档）。
            self.reset_crash(&id);
            self.restart_aggregation();
            self.show_toast(self.tr("toast.ext_retry").replace("{id}", &id), Some(2_000));
        }
    }
}

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

/// 功能态 ToggleSwitch（§08.1 v4.7「材质开关」行；v4.9 放大到 Fluent 规格
/// 40×20、滑块 16）：开 = accent 填充底 + 白滑块居右；关 = input_fill 底 +
/// 1px border-strong 描边 + text-2 滑块居左。返回是否被点击。
pub(crate) fn draw_switch_fn(ui: &mut egui::Ui, on: bool, p: &theme::Palette) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(40.0, 20.0), egui::Sense::click());
    let radius = egui::CornerRadius::same(10);
    let knob_cx = if on {
        rect.right() - 2.0 - 8.0
    } else {
        rect.left() + 2.0 + 8.0
    };
    if on {
        ui.painter().rect_filled(rect, radius, p.accent);
        ui.painter().circle_filled(
            egui::pos2(knob_cx, rect.center().y),
            8.0,
            egui::Color32::WHITE,
        );
    } else {
        ui.painter().rect_filled(rect, radius, p.input_fill);
        ui.painter().rect_stroke(
            rect,
            radius,
            egui::Stroke::new(1.0, p.border_strong),
            egui::StrokeKind::Inside,
        );
        ui.painter()
            .circle_filled(egui::pos2(knob_cx, rect.center().y), 8.0, p.text2);
    }
    resp.clicked()
}

/// Fluent 2 标准次级按钮（v4.9，真机 2026-09-06 反馈"恢复默认/更改不像按钮"）：
/// 高 32 / 文字 14（body）/ 左右 padding 12 / 圆角 4（radius-m）/ 1px
/// `--border-strong` 描边 / card 底；hover = bg1Hover、按下 = bg1Pressed
/// （Fluent 控件态三段）。返回是否被点击。
pub(crate) fn fluent_button(ui: &mut egui::Ui, text: &str, p: &theme::Palette) -> bool {
    fluent_button_sized(ui, text, p, 32.0, 14.0, 12.0)
}

/// Fluent 2 小按钮（列表行内动作，如引擎「删除」）：高 24 / 文字 12 / padding 8。
pub(crate) fn fluent_button_small(ui: &mut egui::Ui, text: &str, p: &theme::Palette) -> bool {
    fluent_button_sized(ui, text, p, 24.0, 12.0, 8.0)
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
    let fill = if resp.is_pointer_button_down_on() {
        p.row_pressed
    } else if resp.hovered() {
        p.row_hover
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
fn draw_keycap(ui: &mut egui::Ui, cap: &str, p: &theme::Palette) {
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

/// Fluent 2 标准下拉控件（v4.15）：高 32、宽 `width`、圆角 4、1px
/// `--border-strong` 描边、`card` 底；左侧文字 + 右侧 ▼ 字符（Segoe Fluent
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
fn draw_fluent_dropdown(
    ui: &mut egui::Ui,
    selected: usize,
    labels: &[&str],
    width: f32,
    p: &theme::Palette,
    enabled: bool,
) -> Option<usize> {
    use egui::containers::{Popup, PopupCloseBehavior};
    let h: f32 = 32.0;
    let font = egui::FontId::proportional(14.0);

    // ── 按钮：rect_filled + 描边 + 文字 + ▼ ──
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::click());
    let radius = egui::CornerRadius::same(4);
    let (fill, text_color, arrow_color) = if !enabled {
        (p.card, p.text3, p.text3)
    } else if resp.is_pointer_button_down_on() {
        (p.row_pressed, p.text, p.text2)
    } else if resp.hovered() {
        (p.row_hover, p.text, p.text2)
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
                    ui.allocate_exact_size(egui::vec2(item_w, 28.0), egui::Sense::click());
                let item_fill = if is_sel {
                    p.row_selected
                } else if item_resp.hovered() {
                    p.row_hover
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_category_default_is_appearance() {
        // §08 v4.6（B5）：默认选中首栏「外观」；open_settings 每次进入重置到
        // default（与 go_home 复位语义一致），栏目不落盘。
        assert_eq!(SettingsCategory::default(), SettingsCategory::Appearance);
    }

    #[test]
    fn nav_cats_order_labels_unique() {
        // B5：左栏四栏与 8.1「栏目与内容映射」逐项一致（顺序 = 枚举声明序），
        // 标签互不重复（渲染按 NAV_CATS 顺序逐项绘制）。
        // v4.13：NAV_CATS 存 i18n key——按 zh 解析后断言（D38）。
        let labels: Vec<&str> = NAV_CATS
            .iter()
            .map(|(_, _, k)| crate::text::t(dd_gui::settings::Lang::ZhCn, k))
            .collect();
        assert_eq!(labels, ["外观", "常规", "搜索", "扩展"]);
        let cats: Vec<SettingsCategory> = NAV_CATS.iter().map(|(c, _, _)| *c).collect();
        assert_eq!(
            cats,
            [
                SettingsCategory::Appearance,
                SettingsCategory::General,
                SettingsCategory::Search,
                SettingsCategory::Extensions,
            ],
            "栏目不得重复或缺漏"
        );
    }
}
