//! O12（2026-10-11）拆分自 `settings_view.rs`（O12 巨型文件拆分批次）。
//! 纪律（refactor-layering-plan 同款）：搬运单位 = 完整定义块（含文档注释），
//! 函数体一字不改；仅按编译器指示将跨子模块项 `pub(super)` 化。
//! 本文件承载：常规卡族：常规（general）/ 备份导入导出（backup）/ 自定义直达命令（custom_commands）。

use super::*;

impl PaletteApp {
    /// 常规栏（v4.8 功能态 + v4.9 Fluent 控件化）：「打开面板时显示」+
    /// 「全局热键」（可改：更改 = 捕获模式、恢复默认 = Win+Alt+Space 一键还原）+
    /// 「开机自启」（功能态开关）+「语言」（v4.13 D38，ComboBox 三选）。
    /// 动作入口在 `app/keys.rs`（M6 批次 6.3；语言切换 apply_lang）。
    pub(super) fn draw_general_cards(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        let lang = self.lang_effective;
        // ── 卡 1：打开面板时显示 ──
        let mut show_all = self.settings.open_view == dd_gui::settings::OpenView::All;
        let mut view_changed = false;
        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center() + egui::vec2(0.0, 1.0), // +1px 光学下移：Fluent glyph 墨迹重心偏上，对齐标题行
                    egui::Align2::CENTER_CENTER,
                    '\u{E8A9}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        dd_gui::theme::semibold_title(
                            crate::text::t(lang, "set.openview.name"),
                            14.0,
                        )
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
        // R-15：热键未注册（启动注册失败 / 热键线程死亡）→ 标题行「未注册」徽标。
        let hotkey_unregistered = self.hotkey_unregistered;
        // 捕获回落（钩子 Failed = egui 基础捕获）：提示文案切降级口径
        // （Win / Alt+Space 等系统组合不可录），2026-09-29 真机诊断洞补。
        let capture_fallback = self.hotkey_capturing && self.capture_failed;
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
        let change_w = text_width(
            ui,
            change_text_btn,
            egui::FontId::proportional(CONTROL_FONT_PT),
        ) + 24.0;
        let reset_w = text_width(
            ui,
            reset_text_btn,
            egui::FontId::proportional(CONTROL_FONT_PT),
        ) + 24.0;
        // 8 = Change 与 Reset 之间的 gap，4 = 卡片内右边距
        let buttons_total_w = change_w + reset_w + 8.0 + 4.0;
        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center() + egui::vec2(0.0, 1.0), // +1px 光学下移：Fluent glyph 墨迹重心偏上，对齐标题行
                    egui::Align2::CENTER_CENTER,
                    '\u{E92E}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            dd_gui::theme::semibold_title(
                                crate::text::t(lang, "set.hotkey.name"),
                                14.0,
                            )
                            .color(p.text),
                        );
                        // R-15：失败渲染口径（与扩展卡 shadow_warn 同款 danger 色），
                        // 提示当前组合键帽仅是「设置值」，实际并未注册生效。
                        if hotkey_unregistered {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(crate::text::t(
                                    lang,
                                    "set.hotkey.unregistered",
                                ))
                                .size(11.0)
                                .color(p.danger),
                            );
                        }
                    });
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
                            let prompt_key = if capture_fallback {
                                "set.hotkey.capturing_fallback"
                            } else {
                                "set.hotkey.capturing"
                            };
                            ui.label(
                                egui::RichText::new(crate::text::t(lang, prompt_key))
                                    .size(14.0)
                                    .color(if capture_fallback { p.danger } else { p.accent }),
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
                    // 2026-09-29 B 方案后 Win 修饰可经 LL 钩子捕获（Win+X 组合
                    // 可录）；本按钮仍保留——一键还原默认组合 Win+Alt+Space。
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
        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center() + egui::vec2(0.0, 1.0), // +1px 光学下移：Fluent glyph 墨迹重心偏上，对齐标题行
                    egui::Align2::CENTER_CENTER,
                    '\u{E7E8}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                // 右侧开关 40 宽 + 16 间距预算，左列 allocate_ui 锁宽——长英文
                // 描述在左列范围内换行，不再占满整行把开关顶到右缘叠画
                // （同语言卡 v4.15 / 按键卡 D42 修法）。
                let left_w = (ui.available_width() - 40.0 - 16.0).max(160.0);
                ui.allocate_ui(egui::vec2(left_w, 40.0), |ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(40.0);
                        ui.label(
                            dd_gui::theme::semibold_title(
                                crate::text::t(lang, "set.autostart.name"),
                                14.0,
                            )
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
        //（跟随系统/简体中文/English，宽 180 高 28 与搜索引擎 ComboBox 同规格）；
        // 切换经 apply_lang 即时生效。语言卡自身文案也走 t()（当前生效语言）——
        // 切换后本卡文案随语言刷新，是 i18n 端到端的第一个验证点。
        //
        // v4.15 真机反馈修复：①左侧描述 Label 默认 WrapMode=Extend，长英文
        // 描述自然延伸覆盖右侧 ComboBox——加 `.wrap()` + 左列 `allocate_ui` 锁宽
        // = 描述在 `avail - combo_w - 16` 范围内换行、不再侵入下拉；②egui
        // `ComboBox` 视觉过于 native、控件高度自适应会盖住左侧标题——自绘
        // `draw_fluent_dropdown` 锁 28 高（D42 compact 档）、统一 Fluent 2 控件
        // 库口径（圆角 4
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
        // D42/K3：宽度自适应——内容短恒落下限 180（= D38 规格，行为不变）。
        let combo_w = dropdown_width(ui, &labels);
        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                // 左：图标 + 标题描述（强制宽度 = available - combo_w - gap，
                // 确保描述 Label 在此范围内 wrap，不再延伸覆盖右侧下拉）
                let left_w = (ui.available_width() - combo_w - 16.0).max(160.0);
                ui.allocate_ui(egui::vec2(left_w, 40.0), |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let (icon_rect, _) =
                            ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                        ui.painter().text(
                            icon_rect.center() + egui::vec2(0.0, 1.0), // +1px 光学下移：Fluent glyph 墨迹重心偏上，对齐标题行
                            egui::Align2::CENTER_CENTER,
                            '\u{E774}',
                            egui::FontId::proportional(16.0),
                            p.text2,
                        );
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            ui.set_min_height(40.0);
                            ui.label(
                                dd_gui::theme::semibold_title(
                                    crate::text::t(lang_eff, "settings.lang.name"),
                                    14.0,
                                )
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

        // ── 卡 5：预热容量（N4，2026-10-03）── ComboBox 数字 1–16 选（排版
        // 与语言卡同规格：D42 行 40px + 左列锁宽 wrap + 自绘 Fluent 下拉）；
        // 切换经 apply_warm_capacity 即时生效（缩容立即驱逐 + 落盘）。
        ui.add_space(8.0);
        let warm_labels: Vec<String> = (1..=dd_gui::settings::WARM_CAPACITY_MAX as u32)
            .map(|n| n.to_string())
            .collect();
        let warm_labels: Vec<&str> = warm_labels.iter().map(String::as_str).collect();
        // 序：idx 0..15 ↔ 容量 1..16，严格对齐。
        let warm_selected = self.settings.warm_capacity as usize - 1;
        let mut warm_picked: Option<u8> = None;
        let warm_combo_w = dropdown_width(ui, &warm_labels);
        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let left_w = (ui.available_width() - warm_combo_w - 16.0).max(160.0);
                ui.allocate_ui(egui::vec2(left_w, 40.0), |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let (icon_rect, _) =
                            ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                        ui.painter().text(
                            icon_rect.center() + egui::vec2(0.0, 1.0), // +1px 光学下移：Fluent glyph 墨迹重心偏上，对齐标题行
                            egui::Align2::CENTER_CENTER,
                            '\u{E9F5}', // Processing：保活池容量语义
                            egui::FontId::proportional(16.0),
                            p.text2,
                        );
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            ui.set_min_height(40.0);
                            ui.label(
                                dd_gui::theme::semibold_title(
                                    crate::text::t(lang_eff, "set.warm_capacity.name"),
                                    14.0,
                                )
                                .color(p.text),
                            );
                            ui.add_space(2.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(crate::text::t(
                                        lang_eff,
                                        "set.warm_capacity.desc",
                                    ))
                                    .size(12.0)
                                    .color(p.text3),
                                )
                                .wrap(),
                            );
                        });
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(idx) =
                        draw_fluent_dropdown(ui, warm_selected, &warm_labels, warm_combo_w, p, true)
                    {
                        warm_picked = Some((idx + 1) as u8);
                    }
                });
            });
        });
        if let Some(cap) = warm_picked {
            self.apply_warm_capacity(cap);
        }

        // ── 卡 6：自定义命令（N1，2026-10-04）──管理范式复用搜索引擎卡
        //（列表行 + 删除小按钮 + 底部添加区）；添加/删除即时落盘 + 置聚合
        // 脏标记（下一次重聚合后首屏「直达」分组同步）。
        self.draw_custom_commands_card(ui, p);

        // ── 卡 7：导入 / 导出设置（N5，2026-10-04）──本机迁移（两步确认
        // 范式复用「恢复默认外观」卡；导出 = 数据目录备份文件，导入覆盖）。
        self.draw_backup_card(ui, p);
    }

    /// 常规栏「导入 / 导出设置」卡（N5，2026-10-04）：导出 = 数据目录下
    /// `dd-settings-backup.json`（原子写，机器态剔除，路径上屏便于拷贝）；
    /// 导入 = 同文件读回 → 容错解析 → 两步确认（点击变「确认导入」，5s
    /// 未确认自动撤销——`appearance_reset_armed` 同范式）→ 覆盖并应用。
    /// 失败一律 toast（复用既有 toast 组件；spec §4.5 零文件对话框依赖——
    /// 固定路径 + 展示路径）。
    pub(super) fn draw_backup_card(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        use std::time::{Duration, Instant};
        const ARM_WINDOW: Duration = Duration::from_secs(5);
        let lang = self.lang_effective;
        let armed = self
            .settings_import_armed
            .map(|t| t.elapsed() < ARM_WINDOW)
            .unwrap_or(false);
        if self.settings_import_armed.is_some() && !armed {
            self.settings_import_armed = None; // 超时自动撤销
        }
        let import_label = crate::text::t(
            lang,
            if armed {
                "set.backup.armed"
            } else {
                "set.backup.import"
            },
        );
        let mut export_clicked = false;
        let mut import_clicked = false;
        let backup_path = Self::settings_backup_path();

        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            // ── 卡头：图标 + 名称 + 描述 ──
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center() + egui::vec2(0.0, 1.0),
                    egui::Align2::CENTER_CENTER,
                    '\u{E8AB}', // Sync：迁移语义
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        dd_gui::theme::semibold_title(
                            crate::text::t(lang, "set.backup.name"),
                            14.0,
                        )
                        .color(p.text),
                    );
                    ui.add_space(2.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(crate::text::t(lang, "set.backup.desc"))
                                .size(12.0)
                                .color(p.text3),
                        )
                        .wrap(),
                    );
                });
            });
            card.add_space(8.0);
            // ── 备份路径上屏（mono 截断，便于用户整行拷贝）──
            if let Some(path) = &backup_path {
                let path_resp = card.add(
                    egui::Label::new(
                        egui::RichText::new(path.display().to_string())
                            .size(11.0)
                            .color(p.text3)
                            .monospace(),
                    )
                    .truncate(),
                );
                if card.rect_contains_pointer(path_resp.rect) {
                    path_resp.show_tooltip_text(path.display().to_string());
                }
            }
            card.add_space(4.0);
            // ── 动作行：导出 + 导入并覆盖（两步确认）──
            card.horizontal(|ui| {
                ui.add_space(28.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    import_clicked = fluent_button(ui, import_label, p);
                    ui.add_space(8.0);
                    export_clicked =
                        fluent_button(ui, crate::text::t(lang, "set.backup.export"), p);
                });
            });
        });

        if export_clicked {
            self.export_settings_backup();
        }
        if import_clicked {
            if armed {
                self.settings_import_armed = None;
                self.import_settings_backup(ui.ctx());
            } else {
                self.settings_import_armed = Some(Instant::now());
                ui.ctx().request_repaint_after(ARM_WINDOW); // 超时后重绘 → 按钮复原
            }
        }
    }

    /// 常规栏「自定义命令」卡（N1，2026-10-04）：关键词直达 URL / 本地路径。
    /// 交互范式复用搜索引擎卡——列表行（名称 + 关键词 + 目标截断 + 删除，
    /// D42 行规格 32px）+ 底部添加区（类型下拉 + 名称/关键词/目标输入 +
    /// 「添加」按钮）。校验经 [`dd_gui::settings::CustomCommand::new`]（trim /
    /// 小写化 / 空字段与空白关键词拒绝）+ 关键词唯一性，错误行 danger 提示。
    pub(super) fn draw_custom_commands_card(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        let lang = self.lang_effective;
        let cmds = self.settings.custom_commands.clone();
        let kind_labels = [
            crate::text::t(lang, "set.custom.kind_url"),
            crate::text::t(lang, "set.custom.kind_path"),
        ];
        let kind_idx = self.custom_kind_idx;
        let mut remove_keyword: Option<String> = None;
        let mut kind_picked: Option<usize> = None;
        let mut add_clicked = false;

        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            // ── 卡头：图标 + 名称 + 描述 ──
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center() + egui::vec2(0.0, 1.0), // +1px 光学下移：Fluent glyph 墨迹重心偏上
                    egui::Align2::CENTER_CENTER,
                    '\u{E71B}', // Link：直达语义
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        dd_gui::theme::semibold_title(
                            crate::text::t(lang, "set.custom.name"),
                            14.0,
                        )
                        .color(p.text),
                    );
                    ui.add_space(2.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(crate::text::t(lang, "set.custom.desc"))
                                .size(12.0)
                                .color(p.text3),
                        )
                        .wrap(),
                    );
                });
            });
            card.add_space(8.0);
            // ── 已有命令列表：名称 + 关键词 + 目标截断 + 删除（同引擎行 32px，
            // hover 底与 tooltip 惯用法一致）──
            for c in &cmds {
                let (row_rect, row_resp) = card.allocate_exact_size(
                    egui::vec2(card.available_width(), 32.0),
                    egui::Sense::hover(),
                );
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
                row_ui.label(egui::RichText::new(&c.title).size(14.0).color(p.text));
                row_ui.add_space(12.0);
                row_ui.label(
                    egui::RichText::new(format!("@{}", c.keyword))
                        .size(12.0)
                        .color(p.text3)
                        .monospace(),
                );
                row_ui.add_space(12.0);
                let del_label = crate::text::t(lang, "set.custom.delete");
                let font12 = egui::FontId::proportional(12.0);
                let btn_w = (text_width(&row_ui, del_label, font12) + 16.0).max(32.0);
                let font_mono = egui::FontId::monospace(12.0);
                let target_avail = (row_ui.available_width() - btn_w - 12.0).max(1.0);
                let target_truncated = text_width(&row_ui, &c.target, font_mono) > target_avail;
                row_ui.add_sized(
                    egui::vec2(target_avail, 16.0),
                    egui::Label::new(
                        egui::RichText::new(&c.target)
                            .size(12.0)
                            .color(p.text3)
                            .monospace(),
                    )
                    .truncate(),
                );
                row_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(4.0);
                    if fluent_button_small(ui, del_label, p) {
                        remove_keyword = Some(c.keyword.clone());
                    }
                });
                if target_truncated && hovered_now {
                    row_resp.show_tooltip_text(c.target.clone());
                }
            }
            if cmds.is_empty() {
                card.horizontal(|ui| {
                    ui.add_space(28.0);
                    ui.label(
                        egui::RichText::new(crate::text::t(lang, "set.custom.none"))
                            .size(12.0)
                            .color(p.text3),
                    );
                });
            }
            card.add_space(8.0);
            // ── 添加区 · 行 1：类型下拉 + 名称 + 关键词（均 CONTROL_H 高同线）──
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.add_space(28.0);
                if let Some(idx) = draw_fluent_dropdown(ui, kind_idx, &kind_labels, 110.0, p, true)
                {
                    kind_picked = Some(idx);
                }
                let pair_w = (ui.available_width() - 8.0) / 2.0;
                draw_fluent_textbox(
                    ui,
                    pair_w,
                    "dd-custom-title",
                    &mut self.custom_title_buf,
                    crate::text::t(lang, "set.custom.title_hint"),
                    p,
                );
                draw_fluent_textbox(
                    ui,
                    pair_w,
                    "dd-custom-keyword",
                    &mut self.custom_keyword_buf,
                    crate::text::t(lang, "set.custom.keyword_hint"),
                    p,
                );
            });
            card.add_space(8.0);
            // ── 添加区 · 行 2：目标输入（flex）+「添加」按钮 ──
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.add_space(28.0);
                let add_label = crate::text::t(lang, "set.custom.add");
                let btn_w =
                    text_width(ui, add_label, egui::FontId::proportional(CONTROL_FONT_PT)) + 24.0;
                let target_w = (ui.available_width() - btn_w - 8.0).max(160.0);
                draw_fluent_textbox(
                    ui,
                    target_w,
                    "dd-custom-target",
                    &mut self.custom_target_buf,
                    crate::text::t(lang, "set.custom.target_hint"),
                    p,
                );
                if fluent_button(ui, add_label, p) {
                    add_clicked = true;
                }
            });
            if let Some(err) = &self.custom_add_err {
                card.horizontal(|ui| {
                    ui.add_space(28.0);
                    ui.label(egui::RichText::new(err.clone()).size(12.0).color(p.danger));
                });
            }
        });

        // ── 闭包外应用交互结果 ──
        if let Some(idx) = kind_picked {
            self.custom_kind_idx = idx;
        }
        if let Some(keyword) = remove_keyword {
            self.delete_custom_command(&keyword);
        }
        if add_clicked {
            let kind = if self.custom_kind_idx == 0 {
                dd_gui::settings::CustomCommandKind::Url
            } else {
                dd_gui::settings::CustomCommandKind::Path
            };
            match dd_gui::settings::CustomCommand::new(
                &self.custom_title_buf,
                &self.custom_keyword_buf,
                kind,
                &self.custom_target_buf,
            ) {
                Some(c) => {
                    if self
                        .settings
                        .custom_commands
                        .iter()
                        .any(|x| x.keyword == c.keyword)
                    {
                        self.custom_add_err =
                            Some(crate::text::t(lang, "set.custom.dup_keyword").to_string());
                    } else {
                        self.settings.custom_commands.push(c);
                        self.custom_title_buf.clear();
                        self.custom_keyword_buf.clear();
                        self.custom_target_buf.clear();
                        self.custom_add_err = None;
                        // 与删除同一落盘 + 重聚合口径（复用 engines_dirty 消费点）。
                        self.engines_dirty = true;
                        self.save_settings_with_feedback();
                    }
                }
                None => {
                    self.custom_add_err =
                        Some(crate::text::t(lang, "set.custom.invalid").to_string());
                }
            }
        }
    }
}
