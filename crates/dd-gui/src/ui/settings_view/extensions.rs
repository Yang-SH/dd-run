//! O12（2026-10-11）拆分自 `settings_view.rs`（O12 巨型文件拆分批次）。
//! 纪律（refactor-layering-plan 同款）：搬运单位 = 完整定义块（含文档注释），
//! 函数体一字不改；仅按编译器指示将跨子模块项 `pub(super)` 化。
//! 本文件承载：扩展管理：卡片（extensions_card）与行数据纯函数（ExtRow / failed_reason / extension_rows）。

use super::*;

impl PaletteApp {
    /// 扩展栏：扩展管理（v4.8 排版优化：名称独占一行，版本并入第二行
    /// 「v0.1.0 · com.ddrun.apps」，不再挤在名称后）。
    pub(super) fn draw_extensions_card(&mut self, ui: &mut egui::Ui, p: &theme::Palette) {
        // 闭包外拷贝数据（闭包内只收集交互结果）。运行时状态（失败原因 / 是否熔断）
        // 一并收集，驱动失败提示行与「重试」按钮（协议 §11 用户手动重试，M6.4 L2）。
        let rows = extension_rows(
            &self.exts,
            &self.settings.disabled_extensions,
            &self.sources,
            &self.trust,
        );
        let mut changed: Option<(String, bool)> = None;
        let mut retry_id: Option<String> = None;
        // S-05：行内「允许 / 阻止」的收集位（闭包内只收集，落盘在闭包外）。
        let mut trust_action: Option<(String, Decision)> = None;
        let lang = self.lang_effective;
        // 卡片头汇总：待批准数（页脚另有提示，但设置页是"处理现场"）与台账损坏提示。
        let pending = dd_gui::aggregator::pending_count(&self.trust);
        let ledger_corrupt = matches!(self.ledger_state, dd_host::trust::LedgerState::Corrupt);
        // R-20：解析失败清单（路径 + 原因），闭包外拷贝。
        let skipped = self.skipped_manifests.clone();
        // N2：apps 行内「设置」展开态 + 屏蔽名单快照与交互收集位（闭包外
        // 快照、闭包内只收集，落盘在闭包外——与引擎卡同纪律）。
        let apps_cfg_open = self.apps_cfg_open;
        let blocklist_frags = self.apps_blocklist_fragments();
        let mut apps_cfg_clicked = false;
        let mut blocklist_remove: Option<String> = None;
        let mut blocklist_add_clicked = false;

        draw_settings_card_frame(ui, p, self.backdrop_active, |card| {
            card.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    icon_rect.center() + egui::vec2(0.0, 1.0), // +1px 光学下移：Fluent glyph 墨迹重心偏上，对齐标题行
                    egui::Align2::CENTER_CENTER,
                    '\u{E74E}',
                    egui::FontId::proportional(16.0),
                    p.text2,
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_min_height(36.0);
                    ui.label(
                        dd_gui::theme::semibold_title(crate::text::t(lang, "set.ext.name"), 14.0)
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
            // S-05 提示区：待批准汇总 / 台账损坏（均只在需要时出现，不占常态版面）
            if ledger_corrupt {
                card.label(
                    egui::RichText::new(crate::text::t(lang, "set.ext.ledger_corrupt"))
                        .size(11.0)
                        .color(p.danger),
                );
                card.add_space(4.0);
            }
            if pending > 0 {
                card.label(
                    egui::RichText::new(
                        crate::text::t(lang, "set.ext.pending_summary")
                            .replace("{n}", &pending.to_string()),
                    )
                    .size(11.0)
                    .color(p.accent),
                );
                card.add_space(4.0);
            }
            // R-20：清单解析失败的扩展不再无声消失——扩展卡顶部警告行
            // （路径 + 原因；复用 Failed 行的 danger 样式）。只读、不可 Retry
            // （无扩展行可重试；修正清单后重聚合即恢复）。失败行样式（截断 +
            // 悬停全文）与 row.failed_reason 同款。
            for (path, reason) in &skipped {
                let line = crate::text::t(lang, "set.ext.skipped_warn")
                    .replace("{path}", path)
                    .replace("{reason}", reason);
                let resp = card.add(
                    egui::Label::new(egui::RichText::new(line).size(11.0).color(p.danger))
                        .truncate(),
                );
                if card.rect_contains_pointer(resp.rect) {
                    resp.show_tooltip_text(format!("{path}\n{reason}"));
                }
                card.add_space(4.0);
            }
            if rows.is_empty() {
                card.label(
                    egui::RichText::new(crate::text::t(lang, "set.ext.empty"))
                        .size(12.0)
                        .color(p.text3),
                );
            }
            for row in &rows {
                let mut clicked = false;
                let mut retry_clicked = false;
                // S-05：本行是否点了「允许 / 阻止」
                let mut allow_clicked = false;
                let mut block_clicked = false;
                card.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                    ui.add_space(12.0);
                    // 右侧控件宽预算（开关 40 + 按需的小按钮，小按钮宽 =
                    // 文字 + 16 padding、32 下限），左列 set_max_width 锁宽
                    // ——长名称 / version·id 在左列内换行，不再占满整行把
                    // 开关与按钮顶到右缘叠画（同语言卡 v4.15 修法）。
                    let font12 = egui::FontId::proportional(12.0);
                    let small_btn_w = |ui: &egui::Ui, t: &str| {
                        (text_width(ui, t, font12.clone()) + 16.0).max(32.0)
                    };
                    let mut right_w = 40.0; // 开关
                    if row.needs_approval() {
                        right_w += 8.0
                            + small_btn_w(ui, crate::text::t(lang, "set.ext.block"))
                            + 4.0
                            + small_btn_w(ui, crate::text::t(lang, "set.ext.allow"));
                    }
                    if row.failed_reason.is_some() {
                        right_w += 8.0 + small_btn_w(ui, crate::text::t(lang, "set.ext.retry"));
                    }
                    // N2：内置 apps 行「设置」按钮（用户可调项入口，仅内置扩展提供）。
                    if row.id == "com.ddrun.apps" {
                        right_w += 8.0 + small_btn_w(ui, crate::text::t(lang, "set.ext.configure"));
                    }
                    let left_w = (ui.available_width() - right_w).max(160.0);
                    ui.vertical(|ui| {
                        ui.set_min_height(36.0);
                        // 左列锁宽：须在加内容前设置，换行/截断均以此为界
                        ui.set_max_width(left_w);
                        // 名称独占一行（v4.8：版本不再拼在名称后）
                        ui.label(egui::RichText::new(&row.name).size(14.0).color(p.text));
                        ui.add_space(2.0);
                        // 第二行 = 版本 · id（monospace mini，text-3）
                        ui.label(
                            egui::RichText::new(format!("v{} · {}", row.version, row.id))
                                .size(10.0)
                                .color(p.text3)
                                .monospace(),
                        );
                        // S-05 第三行 = 来源标签 + 信任状态（仅未获信任时显色强调）。
                        // 这是用户判断「这是不是官方的」与「为什么它没生效」的唯一出口。
                        ui.add_space(2.0);
                        let origin = crate::text::t(lang, row.origin_key());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            let origin_resp = ui.add(
                                egui::Label::new(
                                    egui::RichText::new(origin).size(10.0).color(p.text3),
                                )
                                .truncate(),
                            );
                            // 来源 tooltip：两条路径（清单 + exe），支持复制定位
                            if ui.rect_contains_pointer(origin_resp.rect) {
                                let detail = if row.origin == ExtOrigin::Builtin {
                                    crate::text::t(lang, "set.ext.builtin_note").to_string()
                                } else {
                                    format!(
                                        "{}\n{}",
                                        row.manifest_path,
                                        crate::text::t(lang, "set.ext.exe_path")
                                            .replace("{p}", &row.exe_path)
                                    )
                                };
                                origin_resp.show_tooltip_text(detail);
                            }
                            if row.needs_approval() {
                                let is_pending = row.trust == Trust::Pending;
                                ui.label(
                                    egui::RichText::new(crate::text::t(lang, row.trust_key()))
                                        .size(10.0)
                                        .color(if is_pending { p.accent } else { p.danger }),
                                );
                            }
                        });
                        // D4 告警行：与随包扩展同 id 但来自用户目录
                        if row.shadow {
                            ui.add_space(2.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(crate::text::t(
                                        lang,
                                        "set.ext.shadow_warn",
                                    ))
                                    .size(11.0)
                                    .color(p.danger),
                                )
                                .truncate(),
                            );
                        }
                        // R-12 告警行：随包 sidecar 同版篡改嫌疑（已 fail-closed 拦下）
                        if row.sidecar_tampered {
                            ui.add_space(2.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(crate::text::t(
                                        lang,
                                        "set.ext.tamper_warn",
                                    ))
                                    .size(11.0)
                                    .color(p.danger),
                                )
                                .truncate(),
                            );
                        }

                        // O14 告警行：发行方签名校验失败（fail-closed 拦下）
                        if row.sig_invalid {
                            ui.add_space(2.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(crate::text::t(lang, "set.ext.sig_warn"))
                                        .size(11.0)
                                        .color(p.danger),
                                )
                                .truncate(),
                            );
                        } // 失败原因行（既有）：仅失败时
                        if let Some(reason) = &row.failed_reason {
                            ui.add_space(2.0);
                            let reason_resp = ui.add(
                                egui::Label::new(
                                    egui::RichText::new(reason).size(11.0).color(p.danger),
                                )
                                .truncate(),
                            );
                            // B7 修订：tooltip 几何判定（on_hover_text 链路失效）
                            if ui.rect_contains_pointer(reason_resp.rect) {
                                reason_resp.show_tooltip_text(reason.clone());
                            }
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        clicked = draw_switch_fn(ui, row.enabled, p);
                        // S-05：未获信任 → 行内「允许 / 阻止」（Fluent 小按钮，贴开关左侧）。
                        // 「已阻止」时按钮为「允许」（撤销路径），与「待批准」同文案，
                        // 语义由左侧状态标签区分。
                        if row.needs_approval() {
                            ui.add_space(8.0);
                            if fluent_button_small(ui, crate::text::t(lang, "set.ext.block"), p) {
                                block_clicked = true;
                            }
                            ui.add_space(4.0);
                            if fluent_button_small(ui, crate::text::t(lang, "set.ext.allow"), p) {
                                allow_clicked = true;
                            }
                        }
                        // §11 用户手动重试：熔断（连续崩溃 Failed）的扩展显示「重试」
                        // 按钮（Fluent 小按钮，贴开关左侧）→ 解除熔断并重聚合（见循环外）。
                        if row.failed_reason.is_some() {
                            ui.add_space(8.0);
                            if fluent_button_small(ui, crate::text::t(lang, "set.ext.retry"), p) {
                                retry_clicked = true;
                            }
                        }
                        // N2：内置 apps 行「设置」小按钮（展开/收起用户屏蔽名单编辑器）。
                        if row.id == "com.ddrun.apps" {
                            ui.add_space(8.0);
                            if fluent_button_small(ui, crate::text::t(lang, "set.ext.configure"), p)
                            {
                                apps_cfg_clicked = true;
                            }
                        }
                    });
                });
                if clicked {
                    changed = Some((row.id.clone(), !row.enabled));
                }
                if retry_clicked {
                    retry_id = Some(row.id.clone());
                }
                if allow_clicked {
                    trust_action = Some((row.id.clone(), Decision::Allow));
                }
                if block_clicked {
                    trust_action = Some((row.id.clone(), Decision::Deny));
                }
            }
            // ── N2：apps 行内「设置」展开区——用户屏蔽名单编辑器（搜索引擎卡
            // 同款列表范式：32px 行 + 删除小按钮 + 底部添加区）──
            if apps_cfg_open && rows.iter().any(|r| r.id == "com.ddrun.apps") {
                let font12 = egui::FontId::proportional(12.0);
                card.add_space(4.0);
                card.horizontal(|ui| {
                    ui.add_space(28.0);
                    ui.label(
                        egui::RichText::new(crate::text::t(lang, "set.apps.blocklist.name"))
                            .size(13.0)
                            .color(p.text),
                    );
                });
                card.horizontal(|ui| {
                    ui.add_space(28.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(crate::text::t(lang, "set.apps.blocklist.desc"))
                                .size(11.0)
                                .color(p.text3),
                        )
                        .wrap(),
                    );
                });
                card.add_space(4.0);
                for frag in &blocklist_frags {
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
                    row_ui.label(
                        egui::RichText::new(frag)
                            .size(12.0)
                            .color(p.text)
                            .monospace(),
                    );
                    let del_label = crate::text::t(lang, "set.custom.delete");
                    let btn_w = (text_width(&row_ui, del_label, font12.clone()) + 16.0).max(32.0);
                    let rest = (row_ui.available_width() - btn_w).max(1.0);
                    row_ui.add_sized(
                        egui::vec2(rest, 16.0),
                        egui::Label::new(egui::RichText::new("").size(12.0)),
                    );
                    row_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(4.0);
                        if fluent_button_small(ui, del_label, p) {
                            blocklist_remove = Some(frag.clone());
                        }
                    });
                    if hovered_now {
                        row_resp.show_tooltip_text(frag.clone());
                    }
                }
                card.add_space(4.0);
                card.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.add_space(28.0);
                    let add_label = crate::text::t(lang, "set.custom.add");
                    let btn_w =
                        text_width(ui, add_label, egui::FontId::proportional(CONTROL_FONT_PT))
                            + 24.0;
                    let w = (ui.available_width() - btn_w - 8.0).max(160.0);
                    draw_fluent_textbox(
                        ui,
                        w,
                        "dd-apps-blocklist",
                        &mut self.apps_blocklist_buf,
                        crate::text::t(lang, "set.apps.blocklist.hint"),
                        p,
                    );
                    if fluent_button(ui, add_label, p) {
                        blocklist_add_clicked = true;
                    }
                });
            }
        });
        if let Some((id, enabled)) = changed {
            self.apply_extension_enabled(&id, enabled);
        }
        // N2：apps「设置」展开开合 + 屏蔽名单增删落盘（apply 内含重聚合脏标记）。
        if apps_cfg_clicked {
            self.apps_cfg_open = !self.apps_cfg_open;
        }
        if let Some(frag) = blocklist_remove {
            self.apps_blocklist_remove(&frag);
        }
        if blocklist_add_clicked {
            self.apps_blocklist_add();
        }
        // S-05：信任决策（写台账 + 落盘 + 立即重聚合，见 `set_extension_trust`）。
        if let Some((id, decision)) = trust_action {
            self.set_extension_trust(&id, decision);
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

/// 扩展管理的一行视图数据（纯函数产出，便于单测）。
pub(crate) struct ExtRow {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    /// 开关是否开启（不在停用集内）。
    pub(crate) enabled: bool,
    /// 失败原因 = `SourceStatus::Failed.error`；`None` = 正常（warm / stub）。
    pub(crate) failed_reason: Option<String>,
    /// S-05：信任状态（待批准 / 已阻止 / 自动信任）。
    pub(crate) trust: Trust,
    /// S-05：来源（内置 / 随包 / 用户安装）——用户判断"这是不是官方的"的唯一依据。
    pub(crate) origin: ExtOrigin,
    /// S-05：与随包首方扩展同 id 但来自用户目录（D4 告警）。
    pub(crate) shadow: bool,
    /// R-12：随包 sidecar 同版篡改嫌疑（哈希与钉扎不符且宿主版本未变）。
    pub(crate) sidecar_tampered: bool,
    /// O14/F12：发行方签名校验失败（.sig 解析/验签/双哈希任一失败）。
    pub(crate) sig_invalid: bool,
    /// 清单路径（展示 + 溯源）。
    pub(crate) manifest_path: String,
    /// 可执行文件路径（内置为名义路径，渲染时改用文案替代）。
    pub(crate) exe_path: String,
}

impl ExtRow {
    /// 是否需要在行内显示「允许 / 阻止」按钮（仅未获信任者）。
    pub(crate) fn needs_approval(&self) -> bool {
        matches!(self.trust, Trust::Pending | Trust::Blocked)
    }

    /// 信任状态文案键。
    pub(crate) fn trust_key(&self) -> &'static str {
        match self.trust {
            Trust::Pending => "set.ext.trust.pending",
            Trust::Blocked => "set.ext.trust.blocked",
            Trust::AutoTrusted => "",
        }
    }

    /// 来源文案键。
    pub(crate) fn origin_key(&self) -> &'static str {
        match self.origin {
            ExtOrigin::Builtin => "set.ext.origin.builtin",
            ExtOrigin::Sidecar => "set.ext.origin.sidecar",
            ExtOrigin::UserDir => "set.ext.origin.user",
        }
    }
}

/// 取某扩展的失败原因（2026-09-10 增补）。
///
/// 此前 UI 只读 `is_failed()` 布尔——`Failed.error`（含 spawn 失败的命令路径、
/// 熔断时的 stderr 诊断）从未被渲染，用户只能看到「暂时不可用」而无从判断原因。
fn failed_reason(sources: &[SourceSummary], ext_id: &str) -> Option<String> {
    sources
        .iter()
        .find(|s| s.id == ext_id)
        .and_then(|s| match &s.status {
            SourceStatus::Failed { error } => Some(error.clone()),
            _ => None,
        })
}

/// 汇总扩展管理行：清单 × 停用集 × 运行态 × **信任判定**（S-05）。
///
/// 判定表缺项时按 **fail-closed** 渲染（`Pending` / `UserDir`）：宁可显示"待批准"
/// 也不显示成"可信"，与门禁本身同口径。
pub(super) fn extension_rows(
    exts: &[LoadedExtension],
    disabled: &[String],
    sources: &[SourceSummary],
    trust: &HashMap<String, Assessment>,
) -> Vec<ExtRow> {
    exts.iter()
        .map(|e| {
            let a = trust.get(&e.manifest.id);
            ExtRow {
                id: e.manifest.id.clone(),
                name: e.manifest.name.clone(),
                version: e.manifest.version.clone(),
                enabled: !disabled.iter().any(|x| x == &e.manifest.id),
                failed_reason: failed_reason(sources, &e.manifest.id),
                trust: a.map(|x| x.trust).unwrap_or(Trust::Pending),
                origin: a.map(|x| x.origin).unwrap_or(ExtOrigin::UserDir),
                shadow: a.map(|x| x.shadows_first_party()).unwrap_or(false),
                sidecar_tampered: a.map(|x| x.sidecar_tampered).unwrap_or(false),
                sig_invalid: a.map(|x| x.sig_invalid).unwrap_or(false),
                manifest_path: e.path.display().to_string(),
                exe_path: e.command.display().to_string(),
            }
        })
        .collect()
}
