//! O12（2026-10-11）拆分自 `settings_view.rs`（O12 巨型文件拆分批次）。
//! 纪律（refactor-layering-plan 同款）：搬运单位 = 完整定义块（含文档注释），
//! 函数体一字不改；仅按编译器指示将跨子模块项 `pub(super)` 化。
//! 本文件承载：外观卡族：主题三选（appearance）/ 材质（material）/ 背景图（background_image）/ 密度（density）/ 恢复默认外观（reset_appearance）。

use super::*;

impl PaletteApp {
    /// 外观栏：主题外观卡（radio-card 三选，§08.1 沿用；验收 B1/B2）。
    ///
    /// 选中态 = 2px accent + accent_soft 底 + 实心圆点；未选 = 1px
    /// border-strong + input_fill 底 + 空心圆点（1.5px stroke）。
    /// 选择即生效：点击 radio-card 立即 `apply_theme_pref`（set_theme + save）。
    pub(super) fn draw_appearance_card(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        let dark = ui.visuals().dark_mode;
        let lang = self.lang_effective;
        let mut pick: Option<dd_gui::settings::ThemePref> = None;
        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            // 主题 icon + 名称 + 描述（16px / 14/20 / 12/16 fg-2 / text / text-3）
            // item_spacing.x 清零：gap 严格 12px（§08 CSS setting-row gap），不受
            // egui 默认 8px item_spacing 叠加影响。
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center() + egui::vec2(0.0, 1.0), // +1px 光学下移：Fluent glyph 墨迹重心偏上，对齐标题行
                    egui::Align2::CENTER_CENTER,
                    '\u{E790}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0); // gap 12（§08 CSS setting-row gap）
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        dd_gui::theme::semibold_title(crate::text::t(lang, "set.theme.name"), 14.0)
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
                // B6③：缩略图种类跟随目标主题（跟随系统 = 亮暗拼接）
                let prefs = [
                    (
                        dd_gui::settings::ThemePref::System,
                        crate::text::t(lang, "set.theme.follow"),
                        "System",
                        ThemeThumb::System,
                    ),
                    (
                        dd_gui::settings::ThemePref::Light,
                        crate::text::t(lang, "set.theme.light"),
                        "Light",
                        ThemeThumb::Light,
                    ),
                    (
                        dd_gui::settings::ThemePref::Dark,
                        crate::text::t(lang, "set.theme.dark"),
                        "Dark",
                        ThemeThumb::Dark,
                    ),
                ];
                for (i, (pref, zh, en, thumb)) in prefs.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(gap);
                    }
                    let selected = self.settings.theme == *pref;
                    if draw_radio_card(ui, card_w, zh, en, *thumb, selected, p, dark) {
                        pick = Some(*pref);
                    }
                }
            });
        });
        if let Some(pref) = pick {
            self.apply_theme_pref(ui.ctx(), pref);
        }
    }

    /// 外观栏：「窗口材质与边框」卡（窗口材质与边框方案 P1–P4，2026-09-13，
    /// 参考 DeskBox「外观 → 窗口材质与边框」）：卡头描述跟随当前材质；四行 =
    /// 材质三选 pill（P1，互斥单值 `backdrop`）+ 不透明度滑杆（P2）+ 窗口圆角
    /// 三选（P3）+ 面板边框三选（P4）。pill 复用 `draw_density_pill` 口径
    /// （F2 同构，三选等宽）；材质未生效（回退 / Win10）时滑杆与边框行置灰
    /// （`add_enabled_ui`，与 S6 降级语义一致——圆角行与材质无关保持可用）。
    /// P2 滑杆即时生效不落盘，松手（`drag_released`）时统一 `settings.save()`。
    pub(super) fn draw_material_card(
        &mut self,
        ui: &mut egui::Ui,
        p: &theme::Palette,
        ctx: &egui::Context,
    ) {
        let dark = ui.visuals().dark_mode;
        let lang = self.lang_effective;
        let material = self.settings.backdrop;
        let material_active = self.backdrop_active;
        // T9（2026-10-05）：背景图生效 → 材质行整体置灰（互斥语义——材质 /
        // 着色 / 边框链路暂停生效），卡头下提示恢复条件。
        let bg_set = self.settings.background_image_path.is_some();
        // 开关状态在闭包外读取、闭包内只收集点击结果（避免闭包内 &mut self 冲突）。
        let mut picked_material: Option<dd_gui::settings::Backdrop> = None;
        let mut picked_corner: Option<dd_gui::settings::CornerPref> = None;
        let mut picked_border: Option<dd_gui::settings::BorderMode> = None;
        let mut opacity_tmp = self.settings.material_opacity;
        let mut opacity_changed = false;
        let mut opacity_released = false;
        // T6（2026-09-20）：着色行状态（模式 pill + 自定义色 + 强度）
        let colorization = self.settings.colorization;
        let mut picked_colorization: Option<dd_gui::settings::ColorizationMode> = None;
        let mut color_tmp = self.settings.custom_tint_color;
        let mut color_changed = false;
        let mut intensity_tmp = self.settings.custom_tint_intensity;
        let mut intensity_changed = false;
        let mut intensity_released = false;
        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            // 卡头：图标 + 名称 + 描述（描述跟随当前材质，同 DeskBox 行描述语义）
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center() + egui::vec2(0.0, 1.0), // +1px 光学下移：Fluent glyph 墨迹重心偏上，对齐标题行
                    egui::Align2::CENTER_CENTER,
                    '\u{E771}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        dd_gui::theme::semibold_title(
                            crate::text::t(lang, "set.backdrop.name"),
                            14.0,
                        )
                        .color(p.text),
                    );
                    ui.add(
                        egui::Label::new(
                            // M1：描述键由注册表派生（`Backdrop::desc_key`）
                            egui::RichText::new(crate::text::t(lang, material.desc_key()))
                                .size(12.0)
                                .color(p.text3),
                        )
                        .wrap(),
                    );
                });
            });
            // T9：背景图生效提示（材质行置灰的原因与恢复条件）
            if bg_set {
                card.label(
                    egui::RichText::new(crate::text::t(lang, "set.bgimg.active_hint"))
                        .size(12.0)
                        .color(p.text3),
                );
                card.add_space(2.0);
            }
            // ── 行 1：材质三选 pill（无材质 / 云母 / 亚克力，P1）──
            card.add_space(8.0);
            card.label(
                egui::RichText::new(crate::text::t(lang, "set.material.name"))
                    .size(14.0)
                    .color(p.text),
            );
            card.add_space(2.0);
            card.label(
                egui::RichText::new(crate::text::t(lang, "set.material.desc"))
                    .size(12.0)
                    .color(p.text3),
            );
            card.add_space(6.0);
            // M1（2026-09-20）：pill 列表与文案键由注册表派生（`Backdrop::ALL`
            // + `name_key()`），宽度按档数均分——加档无需改本处。
            let gap = 8.0;
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let avail = ui.available_width();
                let n = dd_gui::settings::Backdrop::ALL.len() as f32;
                let pill_w = (avail - (n - 1.0) * gap) / n;
                for (i, backdrop) in dd_gui::settings::Backdrop::ALL.into_iter().enumerate() {
                    if i > 0 {
                        ui.add_space(gap);
                    }
                    if draw_density_pill(
                        ui,
                        pill_w,
                        crate::text::t(lang, backdrop.name_key()),
                        material == backdrop,
                        p,
                        dark,
                        bg_set,
                    ) {
                        picked_material = Some(backdrop);
                    }
                }
            });
            // ── 行 2：着色（T6；三选 pill + 自定义档条件显隐颜色/强度）──
            // 只在材质生效时有视觉意义 → 材质未生效整行置灰（dim）。
            card.add_space(8.0);
            card.label(
                egui::RichText::new(crate::text::t(lang, "set.color.name"))
                    .size(14.0)
                    .color(p.text),
            );
            card.add_space(2.0);
            card.label(
                egui::RichText::new(crate::text::t(lang, "set.color.desc"))
                    .size(12.0)
                    .color(p.text3),
            );
            card.add_space(6.0);
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let avail = ui.available_width();
                let n = dd_gui::settings::ColorizationMode::ALL.len() as f32;
                let pill_w = (avail - (n - 1.0) * gap) / n;
                for (i, mode) in dd_gui::settings::ColorizationMode::ALL
                    .into_iter()
                    .enumerate()
                {
                    if i > 0 {
                        ui.add_space(gap);
                    }
                    if draw_density_pill(
                        ui,
                        pill_w,
                        crate::text::t(lang, mode.name_key()),
                        colorization == mode,
                        p,
                        dark,
                        !material_active || bg_set,
                    ) {
                        picked_colorization = Some(mode);
                    }
                }
            });
            if colorization == dd_gui::settings::ColorizationMode::Custom {
                // 自定义档：色块 + 强度滑杆（材质未生效同样置灰）
                card.add_space(6.0);
                card.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(crate::text::t(lang, "set.color.custom_color"))
                            .size(12.0)
                            .color(p.text3),
                    );
                    ui.add_space(8.0);
                    ui.add_enabled_ui(material_active && !bg_set, |ui| {
                        if ui.color_edit_button_srgb(&mut color_tmp).changed() {
                            color_changed = true;
                        }
                    });
                });
                card.add_space(6.0);
                card.label(
                    egui::RichText::new(crate::text::t(lang, "set.color.intensity"))
                        .size(12.0)
                        .color(p.text3),
                );
                card.add_space(6.0);
                // 独立 id 作用域：`draw_opacity_slider` 内部按名取 id，
                // 与下方材质不透明度滑杆同卡相邻，必须隔开避免 id 冲突。
                let (changed, released) = card
                    .push_id("tint_intensity", |ui| {
                        draw_slider_row_with_pct(
                            ui,
                            material_active && !bg_set,
                            &mut intensity_tmp,
                            p,
                        )
                    })
                    .inner;
                if changed {
                    intensity_changed = true;
                }
                if released {
                    intensity_released = true;
                }
            }
            // ── 行 3：不透明度滑杆（P2；材质关/未生效 → 置灰）──
            // v6（2026-09-13 设计风格对齐）：egui 默认 Slider（细灰轨 + 行内
            // 百分比后缀）与整套 Fluent 控件（pill/开关）脱节，改自绘规格：
            // 轨 4px 圆角 2（未选 `--border` / 已选 accent_stroke）+ 16px 白钮
            // border-strong 描边（悬停/拖动加粗到 2px）。
            // v4.20（2026-09-29）：百分比从行头描述行移到滑杆同一行右缘——
            // 描述行恢复占满整行，拖动时读值不再跨行找。
            card.add_space(8.0);
            card.label(
                egui::RichText::new(crate::text::t(lang, "set.opacity.name"))
                    .size(14.0)
                    .color(p.text),
            );
            card.add_space(2.0);
            card.add(
                egui::Label::new(
                    egui::RichText::new(crate::text::t(lang, "set.opacity.desc"))
                        .size(12.0)
                        .color(p.text3),
                )
                .wrap(),
            );
            card.add_space(6.0);
            let (slider_changed, slider_released) =
                draw_slider_row_with_pct(card, material_active && !bg_set, &mut opacity_tmp, p);
            if slider_changed {
                opacity_changed = true;
            }
            if slider_released {
                opacity_released = true;
            }
            // ── 行 3：窗口圆角三选 pill（P3；与材质无关恒可用）──
            card.add_space(8.0);
            card.label(
                egui::RichText::new(crate::text::t(lang, "set.corner.name"))
                    .size(14.0)
                    .color(p.text),
            );
            card.add_space(2.0);
            card.label(
                egui::RichText::new(crate::text::t(lang, "set.corner.desc"))
                    .size(12.0)
                    .color(p.text3),
            );
            card.add_space(6.0);
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let avail = ui.available_width();
                let pill_w = (avail - 2.0 * gap) / 3.0;
                for (i, (pref, key)) in [
                    (dd_gui::settings::CornerPref::Round, "set.corner.round"),
                    (dd_gui::settings::CornerPref::Small, "set.corner.small"),
                    (dd_gui::settings::CornerPref::Square, "set.corner.square"),
                ]
                .into_iter()
                .enumerate()
                {
                    if i > 0 {
                        ui.add_space(gap);
                    }
                    if draw_density_pill(
                        ui,
                        pill_w,
                        crate::text::t(lang, key),
                        self.settings.corner_pref == pref,
                        p,
                        dark,
                        false,
                    ) {
                        picked_corner = Some(pref);
                    }
                }
            });
            // ── 行 4：面板边框三选 pill（P4；仅材质生效时绘制 → 未生效置灰）──
            card.add_space(8.0);
            card.label(
                egui::RichText::new(crate::text::t(lang, "set.border.name"))
                    .size(14.0)
                    .color(p.text),
            );
            card.add_space(2.0);
            card.label(
                egui::RichText::new(crate::text::t(lang, "set.border.desc"))
                    .size(12.0)
                    .color(p.text3),
            );
            card.add_space(6.0);
            // v6 修复：三个 pill 必须包在 horizontal 里（此前直接落在垂直
            // 闭包中被逐行堆叠——真机截图「面板边框排版」问题根因）。
            card.add_enabled_ui(material_active && !bg_set, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let avail = ui.available_width();
                    let pill_w = (avail - 2.0 * gap) / 3.0;
                    for (i, (mode, key)) in [
                        (dd_gui::settings::BorderMode::Neutral, "set.border.neutral"),
                        (dd_gui::settings::BorderMode::Accent, "set.border.accent"),
                        (dd_gui::settings::BorderMode::None, "set.border.off"),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        if i > 0 {
                            ui.add_space(gap);
                        }
                        if draw_density_pill(
                            ui,
                            pill_w,
                            crate::text::t(lang, key),
                            self.settings.border_mode == mode,
                            p,
                            dark,
                            !material_active || bg_set,
                        ) {
                            picked_border = Some(mode);
                        }
                    }
                });
            });
        });
        if let Some(backdrop) = picked_material {
            self.apply_backdrop(ctx, backdrop);
        }
        if opacity_changed {
            self.apply_material_opacity(ctx, opacity_tmp);
        }
        if opacity_released {
            self.save_settings_with_feedback();
        }
        // T6（2026-09-20）：着色变更即时生效（强度松手落盘、自定义色在指针
        // 松开时落盘——见 `apply_custom_tint_color`）
        if let Some(m) = picked_colorization {
            self.apply_colorization(ctx, m);
        }
        if color_changed {
            self.apply_custom_tint_color(ctx, color_tmp);
        }
        if intensity_changed {
            self.apply_custom_tint_intensity(ctx, intensity_tmp);
        }
        if intensity_released {
            self.save_settings_with_feedback();
        }
        if let Some(pref) = picked_corner {
            self.apply_corner_pref(pref);
        }
        if let Some(mode) = picked_border {
            self.apply_border_mode(ctx, mode);
        }
    }

    /// 外观栏：「背景图」卡（T9，2026-10-05；材质卡之后）——路径输入 +
    /// 设为背景 / 清除 + 适应方式 pill + 不透明度 / 着色强度滑杆。一期零文件
    /// 对话框依赖（`rfd` 未引入，方案 §3.2 判定）——路径文本输入 + 提示文案；
    /// 校验失败（文件不存在）与运行期解码失败（负缓存命中）走 danger 错误行
    /// （搜索引擎 / 自定义命令卡同款范式）。滑杆即时生效、松手落盘（材质
    /// 不透明度同口径）。
    pub(super) fn draw_background_image_card(
        &mut self,
        ui: &mut egui::Ui,
        p: &theme::Palette,
        ctx: &egui::Context,
    ) {
        let dark = ui.visuals().dark_mode;
        let lang = self.lang_effective;
        let bg_set = self.settings.background_image_path.is_some();
        // 运行期解码失败（负缓存命中且路径与当前设置一致）→ 错误行提示
        let decode_failed = self
            .backdrop_cache
            .load_failed(self.settings.background_image_path.as_deref());
        let mut apply_clicked = false;
        let mut clear_clicked = false;
        let mut picked_fit: Option<dd_gui::settings::BgImageFit> = None;
        let mut opacity_tmp = self.settings.background_image_opacity;
        let mut opacity_changed = false;
        let mut opacity_released = false;
        let mut tint_tmp = self.settings.background_image_tint_intensity;
        let mut tint_changed = false;
        let mut tint_released = false;
        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            // 卡头：图标 + 名称 + 描述（同主题 / 材质卡口径；Photo glyph E91B）
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center() + egui::vec2(0.0, 1.0), // +1px 光学下移：Fluent glyph 墨迹重心偏上
                    egui::Align2::CENTER_CENTER,
                    '\u{E91B}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        dd_gui::theme::semibold_title(crate::text::t(lang, "set.bgimg.name"), 14.0)
                            .color(p.text),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(crate::text::t(lang, "set.bgimg.desc"))
                                .size(12.0)
                                .color(p.text3),
                        )
                        .wrap(),
                    );
                });
            });
            // ── 行 1：图片路径输入 + 应用 / 清除按钮 ──
            card.add_space(8.0);
            card.label(
                egui::RichText::new(crate::text::t(lang, "set.bgimg.path"))
                    .size(14.0)
                    .color(p.text),
            );
            card.add_space(6.0);
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let apply_label = crate::text::t(lang, "set.bgimg.apply");
                let clear_label = crate::text::t(lang, "set.bgimg.clear");
                let btns_w =
                    text_width(ui, apply_label, egui::FontId::proportional(CONTROL_FONT_PT))
                        + text_width(ui, clear_label, egui::FontId::proportional(CONTROL_FONT_PT))
                        + 48.0;
                let path_w = (ui.available_width() - btns_w).max(160.0);
                draw_fluent_textbox(
                    ui,
                    path_w,
                    "dd-bgimg-path",
                    &mut self.bg_path_buf,
                    crate::text::t(lang, "set.bgimg.path_hint"),
                    p,
                );
                if fluent_button(ui, apply_label, p) {
                    apply_clicked = true;
                }
                ui.add_enabled_ui(bg_set, |ui| {
                    if fluent_button(ui, clear_label, p) {
                        clear_clicked = true;
                    }
                });
            });
            if let Some(err) = &self.bg_err {
                card.horizontal(|ui| {
                    ui.label(egui::RichText::new(err.clone()).size(12.0).color(p.danger));
                });
            }
            if decode_failed {
                card.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(crate::text::t(lang, "set.bgimg.err_decode"))
                            .size(12.0)
                            .color(p.danger),
                    );
                });
            }
            // ── 行 2：适应方式 pill（填满=等比裁剪铺满 / 拉伸）──
            card.add_space(8.0);
            card.label(
                egui::RichText::new(crate::text::t(lang, "set.bgimg.fit"))
                    .size(14.0)
                    .color(p.text),
            );
            card.add_space(6.0);
            card.add_enabled_ui(bg_set, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let avail = ui.available_width();
                    let n = dd_gui::settings::BgImageFit::ALL.len() as f32;
                    let gap = 8.0;
                    let pill_w = (avail - (n - 1.0) * gap) / n;
                    for (i, fit) in dd_gui::settings::BgImageFit::ALL.into_iter().enumerate() {
                        if i > 0 {
                            ui.add_space(gap);
                        }
                        if draw_density_pill(
                            ui,
                            pill_w,
                            crate::text::t(lang, fit.name_key()),
                            self.settings.background_image_fit == fit,
                            p,
                            dark,
                            !bg_set,
                        ) {
                            picked_fit = Some(fit);
                        }
                    }
                });
            });
            // ── 行 3：图片不透明度滑杆（无图 → 置灰）──
            card.add_space(8.0);
            card.label(
                egui::RichText::new(crate::text::t(lang, "set.bgimg.opacity"))
                    .size(14.0)
                    .color(p.text),
            );
            card.add_space(2.0);
            card.add(
                egui::Label::new(
                    egui::RichText::new(crate::text::t(lang, "set.bgimg.opacity.desc"))
                        .size(12.0)
                        .color(p.text3),
                )
                .wrap(),
            );
            card.add_space(6.0);
            let (changed, released) = card
                .push_id("bgimg_opacity", |ui| {
                    draw_slider_row_with_pct(ui, bg_set, &mut opacity_tmp, p)
                })
                .inner;
            opacity_changed = changed;
            opacity_released = released;
            // ── 行 4：着色强度滑杆（无图 → 置灰；默认 0 = 不叠色）──
            card.add_space(8.0);
            card.label(
                egui::RichText::new(crate::text::t(lang, "set.bgimg.tint"))
                    .size(14.0)
                    .color(p.text),
            );
            card.add_space(2.0);
            card.add(
                egui::Label::new(
                    egui::RichText::new(crate::text::t(lang, "set.bgimg.tint.desc"))
                        .size(12.0)
                        .color(p.text3),
                )
                .wrap(),
            );
            card.add_space(6.0);
            let (changed, released) = card
                .push_id("bgimg_tint", |ui| {
                    draw_slider_row_with_pct(ui, bg_set, &mut tint_tmp, p)
                })
                .inner;
            tint_changed = changed;
            tint_released = released;
        });
        // ── 闭包外应用交互结果 ──
        if apply_clicked {
            let path = self.bg_path_buf.trim();
            if path.is_empty() {
                // 空输入 + 「设为背景」= 清除（提供无「清除」按钮依赖的出路）
                self.bg_err = None;
                self.apply_background_path(ctx, None);
            } else if !std::path::Path::new(path).is_file() {
                self.bg_err = Some(crate::text::t(lang, "set.bgimg.err_not_found").to_string());
            } else {
                self.bg_err = None;
                self.apply_background_path(ctx, Some(path.to_string()));
            }
        }
        if clear_clicked {
            self.bg_path_buf.clear();
            self.bg_err = None;
            self.apply_background_path(ctx, None);
        }
        if let Some(fit) = picked_fit {
            self.apply_background_fit(fit);
        }
        if opacity_changed {
            self.apply_background_opacity(opacity_tmp);
        }
        if opacity_released {
            self.save_settings_with_feedback();
        }
        if tint_changed {
            self.apply_background_tint(tint_tmp);
        }
        if tint_released {
            self.save_settings_with_feedback();
        }
    }

    /// 外观栏：「列表密度」卡（F2，icons-typography-plan.md；参考 DeskBox
    /// 「图标/文字大小可调」）——紧凑/标准/宽松三选 pill，一档联动行高、
    /// 字号与图标格（`theme::ListMetrics`）。点击即落盘；面板默认高度在
    /// 下次唤起时按新档推导（`base_height_for_workarea`）。
    pub(super) fn draw_density_card(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        let dark = ui.visuals().dark_mode;
        let lang = self.lang_effective;
        let current = self.settings.density;
        let mut pick: Option<dd_gui::settings::ListDensity> = None;
        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            // 卡头：图标 + 名称 + 描述（同主题/材质卡口径）
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center() + egui::vec2(0.0, 1.0), // +1px 光学下移：Fluent glyph 墨迹重心偏上，对齐标题行
                    egui::Align2::CENTER_CENTER,
                    '\u{E8FD}', // BulletedList（Segoe MDL2/Fluent，列表语义）
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        dd_gui::theme::semibold_title(
                            crate::text::t(lang, "set.density.name"),
                            14.0,
                        )
                        .color(p.text),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(crate::text::t(lang, "set.density.desc"))
                                .size(12.0)
                                .color(p.text3),
                        )
                        .wrap(),
                    );
                });
            });
            card.add_space(8.0);
            // 三选 pill 行（同 radio-card 行的宽度口径：item_spacing.x 清零，
            // 间隙 8px 手动控制，宽度 = (内宽 − 2×gap)/3）
            let gap = 8.0;
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let avail = ui.available_width();
                let pill_w = (avail - 2.0 * gap) / 3.0;
                for (i, (density, key)) in [
                    (
                        dd_gui::settings::ListDensity::Compact,
                        "set.density.compact",
                    ),
                    (
                        dd_gui::settings::ListDensity::Standard,
                        "set.density.standard",
                    ),
                    (
                        dd_gui::settings::ListDensity::Relaxed,
                        "set.density.relaxed",
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    if i > 0 {
                        ui.add_space(gap);
                    }
                    let selected = current == density;
                    if draw_density_pill(
                        ui,
                        pill_w,
                        crate::text::t(lang, key),
                        selected,
                        p,
                        dark,
                        false,
                    ) {
                        pick = Some(density);
                    }
                }
            });
        });
        if let Some(density) = pick {
            self.settings.density = density;
            self.save_settings_with_feedback();
        }
    }
    /// 外观栏：「恢复默认外观」卡（T5，2026-09-20）——两步确认（点击 → 按钮变
    /// 「确认重置」，5s 未再点自动撤销），避免误触一次性重置外观。
    ///
    /// 刻意**不复用** `ConfirmDialog`：该组件语义绑定「扩展 invoke 的二次确认」
    /// （携带 ext_id + pending 参数），几何与生命周期也不同；此处为宿主本地操作。
    pub(super) fn draw_reset_appearance_card(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        use std::time::{Duration, Instant};
        const ARM_WINDOW: Duration = Duration::from_secs(5);
        let lang = self.lang_effective;
        let armed = self
            .appearance_reset_armed
            .map(|t| t.elapsed() < ARM_WINDOW)
            .unwrap_or(false);
        if self.appearance_reset_armed.is_some() && !armed {
            self.appearance_reset_armed = None; // 超时自动撤销（按钮回「恢复默认」）
        }
        let btn_label = crate::text::t(
            lang,
            if armed {
                "set.reset.armed"
            } else {
                "set.reset.action"
            },
        );
        let mut clicked = false;
        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center() + egui::vec2(0.0, 1.0),
                    egui::Align2::CENTER_CENTER,
                    '\u{E777}', // Refresh
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                // 右侧按钮按 fluent_button 同口径预算宽（文字 + 2×12 padding、
                // 36 下限），左列 allocate_ui 锁宽——长描述在左列范围内换行，
                // 不再占满整行把按钮顶到右缘叠画在首行行尾（同语言卡 v4.15
                // 修法；armed 文案「确认重置」与「恢复默认」同宽，预算恒成立）。
                let btn_w =
                    (text_width(ui, btn_label, egui::FontId::proportional(CONTROL_FONT_PT)) + 24.0)
                        .max(36.0);
                let left_w = (ui.available_width() - btn_w - 16.0).max(160.0);
                ui.allocate_ui(egui::vec2(left_w, 36.0), |ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(36.0);
                        ui.label(
                            dd_gui::theme::semibold_title(
                                crate::text::t(lang, "set.reset.name"),
                                14.0,
                            )
                            .color(p.text),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(crate::text::t(lang, "set.reset.desc"))
                                    .size(12.0)
                                    .color(p.text3),
                            )
                            .wrap(),
                        );
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    clicked = fluent_button(ui, btn_label, p);
                });
            });
        });
        if clicked {
            if armed {
                self.appearance_reset_armed = None;
                self.apply_reset_appearance(ui.ctx());
            } else {
                self.appearance_reset_armed = Some(Instant::now());
                ui.ctx().request_repaint_after(ARM_WINDOW); // 超时后重绘 → 按钮复原
            }
        }
    }
}
