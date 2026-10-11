//! O12（2026-10-11）拆分自 `settings_view.rs`（O12 巨型文件拆分批次）。
//! 纪律（refactor-layering-plan 同款）：搬运单位 = 完整定义块（含文档注释），
//! 函数体一字不改；仅按编译器指示将跨子模块项 `pub(super)` 化。
//! 本文件承载：热键捕获对话框（draw_hotkey_capture_dialog）。

use super::*;

impl PaletteApp {
    /// 热键捕获模态对话框（PowerToys CmdPal「激活快捷键」同构 + **本项目
    /// Fluent Dialog 规格**——与 `ui/confirm.rs` 二次确认框完全同构：Tooltip
    /// 层面板（panel 底 + 1px border + 圆角 8 + dialog_shadow + padding
    /// 20/20/16）+ Foreground 层 `theme::overlay` 遮罩（点击遮罩 = 取消）+
    /// 高 32 按钮（保存 = accent 主按钮，重置/取消 = secondary）。v1 用裸
    /// `egui::Window` 不符设计风格（真机 2026-09-30 反馈），本版重写。
    ///
    /// 内容：实时修饰键徽章 + 候选键帽 + 提示/占用/警示行 + [保存][重置]
    /// [取消]。捕获**只记候选**（`hotkey_pending`），点「保存」走 seq 确认流
    ///（`apply_captured_hotkey`），成功回发后 `end_hotkey_capture` 关闭本窗
    /// ——设置只在确认成功后写入，失败行内报错、候选保留可重试。
    pub(super) fn draw_hotkey_capture_dialog(&mut self, ctx: &egui::Context, p: &theme::Palette) {
        if !self.hotkey_capturing {
            return;
        }
        let lang = self.lang_effective;
        let dark = ctx.theme() == egui::Theme::Dark;
        let mut cancelled = false;
        let mut save_clicked = false;
        let mut reset_clicked = false;
        let mut dialog_rect = egui::Rect::NOTHING;

        // ── 面板（Tooltip 层：高于遮罩 Foreground，按内容定高）──
        egui::Area::new(egui::Id::new("hotkey_capture_dialog"))
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
                        // 内容宽 380（窄屏 clamp 到可用宽度，同 confirm.rs）。
                        ui.set_min_width(380.0f32.min(ui.available_width()));
                        // 标题 subtitle1 16·600 + 副题 caption 12 text3。
                        ui.label(
                            dd_gui::theme::semibold_title(
                                crate::text::t(lang, "set.hotkey.dialog_title"),
                                16.0,
                            )
                            .color(p.text),
                        );
                        ui.label(
                            egui::RichText::new(crate::text::t(lang, "set.hotkey.dialog_sub"))
                                .size(12.0)
                                .color(p.text3),
                        );
                        ui.add_space(8.0);
                        // 键帽预览区：固定高 48 水平居中（PowerToys：按住的
                        // 修饰键实时亮起；候选成立后显示完整组合键帽）。
                        let area_w = ui.available_width();
                        ui.allocate_ui(egui::vec2(area_w, 48.0), |ui| {
                            ui.set_min_height(48.0);
                            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                                ui.horizontal(|ui| match self.hotkey_pending {
                                    Some((mods, vk)) => {
                                        let vk_label = dd_gui::settings::hotkey_vk_label(vk);
                                        let mut caps: Vec<String> =
                                            dd_gui::settings::hotkey_mods_label(mods)
                                                .split('+')
                                                .map(str::to_string)
                                                .collect();
                                        caps.push(vk_label);
                                        for (i, cap) in caps.iter().enumerate() {
                                            if i > 0 {
                                                ui.add_space(4.0);
                                                ui.label(
                                                    egui::RichText::new("+")
                                                        .size(12.0)
                                                        .color(p.text3),
                                                );
                                                ui.add_space(4.0);
                                            }
                                            draw_keycap(ui, cap, p);
                                        }
                                    }
                                    None => {
                                        #[cfg(windows)]
                                        let held = held_modifier_labels();
                                        #[cfg(not(windows))]
                                        let held: Vec<
                                            &'static str,
                                        > = Vec::new();
                                        if held.is_empty() {
                                            let prompt_key = if self.capture_failed {
                                                "set.hotkey.capturing_fallback"
                                            } else {
                                                "set.hotkey.capturing"
                                            };
                                            ui.label(
                                                egui::RichText::new(crate::text::t(
                                                    lang, prompt_key,
                                                ))
                                                .size(13.0)
                                                .color(if self.capture_failed {
                                                    p.danger
                                                } else {
                                                    p.accent
                                                }),
                                            );
                                        } else {
                                            for (i, cap) in held.iter().enumerate() {
                                                if i > 0 {
                                                    ui.add_space(4.0);
                                                    ui.label(
                                                        egui::RichText::new("+")
                                                            .size(12.0)
                                                            .color(p.text3),
                                                    );
                                                    ui.add_space(4.0);
                                                }
                                                draw_keycap(ui, cap, p);
                                            }
                                        }
                                    }
                                });
                            });
                        });
                        // 提示行 caption 11 text3 + 占用红行 + R-21 警示行。
                        ui.label(
                            egui::RichText::new(crate::text::t(lang, "set.hotkey.hint"))
                                .size(11.0)
                                .color(p.text3),
                        );
                        if self.hotkey_apply_failed {
                            ui.label(
                                egui::RichText::new(crate::text::t(lang, "set.hotkey.occupied"))
                                    .size(12.0)
                                    .color(p.danger),
                            );
                        }
                        if let Some((mods, vk)) = self.hotkey_pending {
                            if Self::is_system_reserved_combo(mods, vk) {
                                ui.label(
                                    egui::RichText::new(crate::text::t(
                                        lang,
                                        "set.hotkey.warn_reserved",
                                    ))
                                    .size(12.0)
                                    .color(p.accent),
                                );
                            }
                        }
                        ui.add_space(16.0);
                        // ── 按钮行（高 32、间距 8、右对齐；视觉 [保存][重置][取消]）──
                        let row_w = ui.available_width();
                        let (row, _) =
                            ui.allocate_exact_size(egui::vec2(row_w, 32.0), egui::Sense::hover());
                        let btn_font = egui::FontId::proportional(14.0);
                        let applying = self.hotkey_confirm.is_some();
                        let save_text = if applying {
                            crate::text::t(lang, "set.hotkey.applying")
                        } else {
                            crate::text::t(lang, "set.hotkey.save")
                        };
                        let reset_text = crate::text::t(lang, "set.hotkey.dialog_reset");
                        let cancel_text = crate::text::t(lang, "dialog.cancel");
                        let save_w = text_width(ui, save_text, btn_font.clone()) + 24.0;
                        let reset_w = text_width(ui, reset_text, btn_font.clone()) + 24.0;
                        let cancel_w = text_width(ui, cancel_text, btn_font.clone()) + 24.0;
                        // 自右向左排布：取消最右 → 重置居中 → 保存最左。
                        let cancel_rect = egui::Rect::from_min_size(
                            egui::pos2(row.right() - cancel_w, row.min.y),
                            egui::vec2(cancel_w, 32.0),
                        );
                        let reset_rect = egui::Rect::from_min_size(
                            egui::pos2(cancel_rect.left() - 8.0 - reset_w, row.min.y),
                            egui::vec2(reset_w, 32.0),
                        );
                        let save_rect = egui::Rect::from_min_size(
                            egui::pos2(reset_rect.left() - 8.0 - save_w, row.min.y),
                            egui::vec2(save_w, 32.0),
                        );
                        let unchanged = self.hotkey_pending
                            == Some((self.settings.hotkey_mods, self.settings.hotkey_vk));
                        let save_enabled = !applying && !unchanged && self.hotkey_pending.is_some();
                        // 保存 = accent 主按钮（禁用 = card 底 + text_disabled，
                        // 仍绘制但点击不生效——无候选 / 未改动 / 应用中）。
                        let (save_fill, save_tcol) = if save_enabled {
                            (p.accent, egui::Color32::WHITE)
                        } else {
                            (p.card, p.text_disabled)
                        };
                        if crate::ui::confirm::draw_dialog_button(
                            ui,
                            save_rect,
                            save_text,
                            save_fill,
                            save_tcol,
                            save_enabled,
                        ) && save_enabled
                        {
                            save_clicked = true;
                        }
                        if crate::ui::confirm::draw_dialog_button(
                            ui, reset_rect, reset_text, p.card, p.text, true,
                        ) {
                            reset_clicked = true;
                        }
                        if crate::ui::confirm::draw_dialog_button(
                            ui,
                            cancel_rect,
                            cancel_text,
                            p.card,
                            p.text,
                            true,
                        ) {
                            cancelled = true;
                        }
                    });
                dialog_rect = frame.response.rect;
            });

        // ── 全屏遮罩（Foreground 层：盖住设置页，Tooltip 面板在其上）──
        // 点击遮罩 = 取消（落在面板内的点击不算，同 confirm.rs §10.1）。
        let screen = ctx.input(|i| i.raw.screen_rect.unwrap_or_else(|| i.viewport_rect()));
        egui::Area::new(egui::Id::new("hotkey_capture_scrim"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::LEFT_TOP, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.painter().rect_filled(screen, 0.0, theme::overlay(dark));
                let (_, resp) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
                if resp.clicked() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        if !dialog_rect.contains(pos) {
                            cancelled = true;
                        }
                    }
                }
            });

        if cancelled {
            self.end_hotkey_capture();
        } else if reset_clicked {
            self.hotkey_pending = Some((self.settings.hotkey_mods, self.settings.hotkey_vk));
            self.hotkey_apply_failed = false;
        } else if save_clicked {
            if let Some((mods, vk)) = self.hotkey_pending {
                // 确认流：结果在 poll_hotkey 按 seq 裁决——成功关窗写设置；
                // 失败清 confirm + 行内报错（窗保持开、候选保留可重试）。
                self.apply_captured_hotkey(mods, vk);
            }
        }
    }
}
