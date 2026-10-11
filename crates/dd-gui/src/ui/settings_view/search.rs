//! O12（2026-10-11）拆分自 `settings_view.rs`（O12 巨型文件拆分批次）。
//! 纪律（refactor-layering-plan 同款）：搬运单位 = 完整定义块（含文档注释），
//! 函数体一字不改；仅按编译器指示将跨子模块项 `pub(super)` 化。
//! 本文件承载：搜索卡族：搜索行为（search_behavior）/ 按键与交互（keys_behavior）/ 搜索引擎（search_engine）。

use super::*;

impl PaletteApp {
    /// 搜索栏：「搜索行为」卡（原「搜索应用」卡，2026-09-12）——两行同卡：
    /// ① 搜索应用（名称 + 描述 + 贴右功能态开关，排版同「窗口材质」卡的
    /// 开关行）；② Steam 游戏（2026-10-07，排版同上）。关闭后对应类别的项
    /// 不进首屏与搜索结果。变更经 apply_search_apps / apply_search_steam_games
    /// 即时生效 + 落盘。
    /// （「优先搜索文件」开关同日加入、同日撤销：自动进页劫持常规搜索，
    /// 用户反馈后移除——当时保留 `f ` 前缀直达；该前缀亦已于 2026-09-19 移除，现由 `Ctrl+F` 一键直达承担。）
    pub(super) fn draw_search_behavior_card(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        // 开关状态在闭包外读取、闭包内只收集点击结果（避免闭包内 &mut self 冲突）。
        let apps_on = self.settings.search_apps;
        let steam_on = self.settings.search_steam_games;
        let lang = self.lang_effective;
        let mut apps_toggled = false;
        let mut steam_toggled = false;
        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                // 与其他卡卡头图标对齐的 16px 空槽位
                ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.add_space(12.0);
                // 右侧开关 40 宽 + 16 间距预算，左列 allocate_ui 锁宽——长英文
                // 描述在左列范围内换行，不再占满整行把开关顶到右缘叠画
                // （同语言卡 v4.15 / 按键卡 D42 修法）。
                let left_w = (ui.available_width() - 40.0 - 16.0).max(160.0);
                ui.allocate_ui(egui::vec2(left_w, 40.0), |ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(40.0);
                        ui.label(
                            egui::RichText::new(crate::text::t(lang, "set.search.apps.name"))
                                .size(14.0)
                                .color(p.text),
                        );
                        // D42/K1：行名→描述统一 +2（原 +4，注释「材质卡口径」系错
                        // 标——材质卡实际为 +2，见方案 §1.2）。
                        ui.add_space(2.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(crate::text::t(lang, "set.search.apps.desc"))
                                    .size(12.0)
                                    .color(p.text3),
                            )
                            .wrap(),
                        );
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    apps_toggled = draw_switch_fn(ui, apps_on, p);
                });
            });
            // ── 行 2：Steam 游戏（2026-10-07，排版同行 1——同 16px 槽位 +
            // 12px 间距缩进保证两行文本与开关纵向对齐）──
            card.add_space(12.0);
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.add_space(12.0);
                let left_w = (ui.available_width() - 40.0 - 16.0).max(160.0);
                ui.allocate_ui(egui::vec2(left_w, 40.0), |ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(40.0);
                        ui.label(
                            egui::RichText::new(crate::text::t(lang, "set.search.steam.name"))
                                .size(14.0)
                                .color(p.text),
                        );
                        ui.add_space(2.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(crate::text::t(lang, "set.search.steam.desc"))
                                    .size(12.0)
                                    .color(p.text3),
                            )
                            .wrap(),
                        );
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    steam_toggled = draw_switch_fn(ui, steam_on, p);
                });
            });
        });
        if apps_toggled {
            self.apply_search_apps(ui.ctx(), !apps_on);
        }
        if steam_toggled {
            self.apply_search_steam_games(ui.ctx(), !steam_on);
        }
    }

    /// 常规栏：「按键行为」卡（B1/B2，2026-09-20）——两行同卡：
    /// ① Esc 键行为（左侧标题描述 + 右侧 Fluent 下拉，排版同语言卡）；
    /// ② 退格键返回（左侧标题描述 + 右侧功能开关，排版同搜索应用卡）。
    /// 两者都是纯设置项：变更即时生效（按键分支每次读设置）并落盘。
    pub(super) fn draw_keys_behavior_card(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        let lang = self.lang_effective;
        let esc_pref = self.settings.esc_behavior;
        let backspace_on = self.settings.backspace_go_back;
        let click_on = self.settings.single_click_activation; // T7
        let anim_on = self.settings.ui_animations; // T8
        let mut esc_picked: Option<dd_gui::settings::EscBehavior> = None;
        let mut backspace_toggled = false;
        let mut click_toggled = false; // T7
        let mut anim_toggled = false; // T8
        let labels: Vec<&str> = dd_gui::settings::EscBehavior::ALL
            .iter()
            .map(|b| crate::text::t(lang, b.name_key()))
            .collect();
        // D42/K3：宽度自适应（原硬编码 200——zh 落下限 180 与语言卡一致，
        // 长英文选项撑宽封顶 260，消除英文文案溢出盒外）。
        let combo_w = dropdown_width(ui, &labels);
        let selected_idx = dd_gui::settings::EscBehavior::ALL
            .iter()
            .position(|b| *b == esc_pref)
            .unwrap_or(0);
        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            // ── 卡头：图标 + 名称 + 描述 ──
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center() + egui::vec2(0.0, 1.0), // +1px 光学下移（Fluent glyph 重心偏上）
                    egui::Align2::CENTER_CENTER,
                    '\u{E765}', // KeyboardClassic
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        dd_gui::theme::semibold_title(crate::text::t(lang, "set.keys.name"), 14.0)
                            .color(p.text),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(crate::text::t(lang, "set.keys.desc"))
                                .size(12.0)
                                .color(p.text3),
                        )
                        .wrap(),
                    );
                });
            });
            // ── 行 1：Esc 键行为（左描述 + 右下拉）──
            card.add_space(12.0);
            let left_w = (card.available_width() - combo_w - 16.0).max(160.0);
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.allocate_ui(egui::vec2(left_w, 40.0), |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(crate::text::t(lang, "set.esc.name"))
                                .size(14.0)
                                .color(p.text),
                        );
                        // D42/K1：行名→描述 +2（与自启/语言卡统一口径）
                        ui.add_space(2.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(crate::text::t(lang, "set.esc.desc"))
                                    .size(12.0)
                                    .color(p.text3),
                            )
                            .wrap(),
                        );
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(idx) =
                        draw_fluent_dropdown(ui, selected_idx, &labels, combo_w, p, true)
                    {
                        esc_picked = dd_gui::settings::EscBehavior::ALL.get(idx).copied();
                    }
                });
            });
            // ── 行 2：退格键返回（左描述 + 右开关）──
            card.add_space(12.0);
            let left_w2 = (card.available_width() - 40.0 - 16.0).max(160.0);
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.allocate_ui(egui::vec2(left_w2, 40.0), |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(crate::text::t(lang, "set.backspace.name"))
                                .size(14.0)
                                .color(p.text),
                        );
                        // D42/K1：行名→描述 +2（与自启/语言卡统一口径）
                        ui.add_space(2.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(crate::text::t(lang, "set.backspace.desc"))
                                    .size(12.0)
                                    .color(p.text3),
                            )
                            .wrap(),
                        );
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    backspace_toggled = draw_switch_fn(ui, backspace_on, p);
                });
            });
            // ── 行 3：单击激活（T7；左描述 + 右开关）──
            card.add_space(12.0);
            let left_w3 = (card.available_width() - 40.0 - 16.0).max(160.0);
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.allocate_ui(egui::vec2(left_w3, 40.0), |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(crate::text::t(lang, "set.click.name"))
                                .size(14.0)
                                .color(p.text),
                        );
                        // D42/K1：行名→描述 +2（与自启/语言卡统一口径）
                        ui.add_space(2.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(crate::text::t(lang, "set.click.desc"))
                                    .size(12.0)
                                    .color(p.text3),
                            )
                            .wrap(),
                        );
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    click_toggled = draw_switch_fn(ui, click_on, p);
                });
            });
            // ── 行 4：界面动效（T8；左描述 + 右开关）──
            card.add_space(12.0);
            let left_w4 = (card.available_width() - 40.0 - 16.0).max(160.0);
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.allocate_ui(egui::vec2(left_w4, 40.0), |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(crate::text::t(lang, "set.anim.name"))
                                .size(14.0)
                                .color(p.text),
                        );
                        // D42/K1：行名→描述 +2（与自启/语言卡统一口径）
                        ui.add_space(2.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(crate::text::t(lang, "set.anim.desc"))
                                    .size(12.0)
                                    .color(p.text3),
                            )
                            .wrap(),
                        );
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    anim_toggled = draw_switch_fn(ui, anim_on, p);
                });
            });
        });
        if let Some(b) = esc_picked {
            self.apply_esc_behavior(b);
        }
        if backspace_toggled {
            self.apply_backspace_go_back(!backspace_on);
        }
        if click_toggled {
            self.apply_single_click_activation(!click_on);
        }
        if anim_toggled {
            self.apply_ui_animations(!anim_on);
        }
    }

    /// 搜索栏（v4.8 交互改版，用户决策）：预设引擎改**下拉框添加**、自定义
    /// 引擎改**单 URL 输入框**（名称自动取域名）+ 已启用引擎列表带删除；
    /// 替换 v4.6 的「预设勾选列表 + 名称/URL 双输入表单」。变更落盘 +
    /// engines_dirty（离开设置页重聚合，协议 v1.0 冻结零新增）。
    pub(super) fn draw_search_engine_card(
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

        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center() + egui::vec2(0.0, 1.0), // +1px 光学下移：Fluent glyph 墨迹重心偏上，对齐标题行
                    egui::Align2::CENTER_CENTER,
                    '\u{E721}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        dd_gui::theme::semibold_title(self.tr("set.search.name"), 14.0)
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
            // 行高 32；删除 = Fluent 小按钮）──
            // B6②：行级 hover 底 + 模板全文 tooltip（截断时）。行矩形先分配
            // （hover 底画在文字下层），内容经 child Ui 定位——与列表行
            // allocate-first 同一惯用法。
            for e in &enabled {
                let (row_rect, row_resp) = card.allocate_exact_size(
                    egui::vec2(card.available_width(), 32.0),
                    egui::Sense::hover(),
                );
                // B7 修订：几何判定 + control_hover（亮色 row_hover 与卡底不可辨）
                let hovered_now = card.rect_contains_pointer(row_rect);
                if hovered_now {
                    card.painter().rect_filled(
                        row_rect,
                        egui::CornerRadius::same(4),
                        p.control_hover,
                    );
                }
                let mut row_ui = card.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                row_ui.spacing_mut().item_spacing.x = 0.0;
                row_ui.add_space(28.0);
                row_ui.label(egui::RichText::new(&e.name).size(14.0).color(p.text));
                row_ui.add_space(12.0);
                // 删除按钮贴右：先按 fluent_button_small 同口径预算按钮宽
                // （文字 + 左右 8 padding，下限 = 高 24 + 8），模板占剩余宽截断。
                let del_label = self.tr("set.search.delete");
                let font12 = egui::FontId::proportional(12.0);
                let btn_w = (text_width(&row_ui, del_label, font12) + 16.0).max(32.0);
                let font_mono = egui::FontId::monospace(12.0);
                let tmpl_avail = (row_ui.available_width() - btn_w - 12.0).max(1.0);
                let template_truncated = text_width(&row_ui, &e.template, font_mono) > tmpl_avail;
                row_ui.add_sized(
                    egui::vec2(tmpl_avail, 16.0),
                    egui::Label::new(
                        egui::RichText::new(&e.template)
                            .size(12.0)
                            .color(p.text3)
                            .monospace(),
                    )
                    .truncate(),
                );
                row_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(4.0);
                    if fluent_button_small(ui, del_label, p) {
                        remove_name = Some(e.name.clone());
                    }
                });
                // tooltip 同样转几何判定（on_hover_text 依赖的 hovered() 链路失效）
                if template_truncated && hovered_now {
                    row_resp.show_tooltip_text(e.template.clone());
                }
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
            // ── 添加预设引擎：下拉框（v4.9 宽 260——旧 180×~18 过窄过矮；
            // v4.15 起用自绘 draw_fluent_dropdown 与语言下拉同款；D42 起高随
            // 控件 compact 档 28。宽维持 260 不接 K3 规则：预设名为短名（最长
            // DuckDuckGo），接规则会收窄到 180、背离 v4.9 加宽初衷；260 =
            // 规则上限，规格不冲突）──
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(28.0);
                ui.set_min_height(CONTROL_H);
                ui.label(
                    egui::RichText::new(self.tr("set.search.add_engine"))
                        .size(14.0)
                        .color(p.text2),
                );
                ui.add_space(12.0);
                ui.spacing_mut().interact_size.y = CONTROL_H; // Fluent 控件高（D42 compact 档）
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
                let btn_w =
                    text_width(ui, add_label, egui::FontId::proportional(CONTROL_FONT_PT)) + 24.0;
                let url_w = (ui.available_width() - btn_w - 8.0).max(160.0);
                // 外框几何完全自管（allocate 精确 32 高），与按钮同高同线
                let (box_rect, _) =
                    ui.allocate_exact_size(egui::vec2(url_w, CONTROL_H), egui::Sense::hover());
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
                    // Fluent 聚焦态：底边 2px accent 下划线（内缩 1px 避让描边）。
                    // B2：线状小元素走 accent_stroke。
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(box_rect.left() + 1.0, box_rect.bottom() - 3.0),
                            egui::pos2(box_rect.right() - 1.0, box_rect.bottom() - 1.0),
                        ),
                        egui::CornerRadius::same(1),
                        p.accent_stroke,
                    );
                }
                // frameless TextEdit 内嵌于自绘外框（egui 0.36 `.frame()` 传入
                // 即完全接管样式，不再注入 visuals 边框；margin 烘进 Frame）
                ui.put(
                    box_rect,
                    egui::TextEdit::singleline(&mut self.engine_url_buf)
                        .id(url_edit_id)
                        .desired_width(url_w - 24.0)
                        .font(egui::FontId::proportional(CONTROL_FONT_PT))
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
                    // B6①：报错文案走 Palette::danger（原裸色值 #C42B1C 违反
                    // 「绘制层不写裸色值」契约且不适配亮色主题）。
                    ui.label(egui::RichText::new(err.clone()).size(12.0).color(p.danger));
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
}
