//! O12（2026-10-11）拆分自 `app/keys.rs`（O12 巨型文件拆分批次）。
//! 纪律（refactor-layering-plan 同款）：搬运单位 = 完整定义块（含文档注释），
//! 函数体一字不改；仅按编译器指示将跨子模块项 `pub(super)` 化。
//! 本文件承载：设置页动作入口族（apply_* 39 项：外观/材质/背景图/搜索行为/扩展信任/备份导入导出/语言/密度/自启/warm 容量等）与 restart_aggregation。

use super::*;

impl PaletteApp {
    /// 设置页开机自启开关（M6 批次 6.3）：注册表先落，成功才持久化；
    /// 失败 Toast 错误并保持原状态。
    pub(crate) fn apply_autostart(&mut self, on: bool) {
        if self.settings.autostart == on {
            return;
        }
        match crate::platform::set_autostart(on) {
            Ok(()) => {
                self.settings.autostart = on;
                self.save_settings_with_feedback();
            }
            Err(e) => self.show_toast(self.tr("toast.autostart_fail").replace("{e}", &e), None),
        }
    }

    /// 设置页语言切换（v4.13 D38）：更新偏好 + 重算生效语言 + 落盘 + 同步托盘
    /// 菜单语言（菜单每次右键即席创建，读共享原子量，无需重建托盘）；
    /// egui immediate mode 下一帧全量重绘即生效。纯内存 + 落盘操作，无失败路径。
    pub(crate) fn apply_lang(&mut self, lang: dd_gui::settings::Lang) {
        if self.settings.lang == lang {
            return;
        }
        self.settings.lang = lang;
        self.lang_effective = Self::resolve_lang(&self.settings);
        self.save_settings_with_feedback();
        crate::tray::set_tray_lang(self.lang_effective);
        // 扩展进程须以新 DDRUN_LANG 重启才生效；离开设置页时重聚合消费。
        self.lang_dirty = true;
    }

    /// 设置页预热容量切换（N4，2026-10-03）：更新偏好 + 落盘 + 立即调整
    /// `LruWarmSet` 容量——缩容时按队尾（最久未用）驱逐受害者（close + 回落
    /// stub，走既有 `evict_warm` 路径；其内部 `lru.remove` 对已出队 id 为无操作）；
    /// 扩容不追补，后续触达照常入队。纯内存 + 落盘操作，无失败路径。
    pub(crate) fn apply_warm_capacity(&mut self, cap: u8) {
        if self.settings.warm_capacity == cap {
            return;
        }
        self.settings.warm_capacity = cap;
        self.save_settings_with_feedback();
        for victim in self.lru.set_capacity(cap as usize) {
            self.evict_warm(&victim);
        }
    }

    // ── N2（2026-10-04）：apps 用户屏蔽名单编辑（设置页扩展卡行内「设置」）──

    /// 当前屏蔽名单片段（`ext_settings["com.ddrun.apps"]["blocklist"]` 逗号
    /// 分隔解析；trim、去空）。
    pub(crate) fn apps_blocklist_fragments(&self) -> Vec<String> {
        self.settings
            .ext_settings
            .get("com.ddrun.apps")
            .and_then(|m| m.get("blocklist"))
            .map(|s| {
                s.split(',')
                    .map(|f| f.trim().to_string())
                    .filter(|f| !f.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 名单变更统一落盘：写 `ext_settings` + 持久化 + 重聚合脏标记
    ///（离开设置页重聚合 → 内存通道重注入 + 首屏重拉，扩展侧即生效）。
    pub(crate) fn apply_apps_blocklist(&mut self, joined: String) {
        self.settings
            .ext_settings
            .entry("com.ddrun.apps".to_string())
            .or_default()
            .insert("blocklist".to_string(), joined);
        self.save_settings_with_feedback();
        self.engines_dirty = true;
    }

    /// 追加片段（输入缓冲 trim；空忽略；重复忽略）。
    pub(crate) fn apps_blocklist_add(&mut self) {
        let frag = self.apps_blocklist_buf.trim().to_string();
        if frag.is_empty() {
            return;
        }
        let mut list = self.apps_blocklist_fragments();
        if list.iter().any(|x| x.eq_ignore_ascii_case(&frag)) {
            self.apps_blocklist_buf.clear();
            return;
        }
        list.push(frag);
        self.apps_blocklist_buf.clear();
        self.apply_apps_blocklist(list.join(", "));
    }

    /// 移除片段（精确匹配）。
    pub(crate) fn apps_blocklist_remove(&mut self, frag: &str) {
        let mut list = self.apps_blocklist_fragments();
        list.retain(|x| x != frag);
        self.apply_apps_blocklist(list.join(", "));
    }

    // ── N5（2026-10-04）：设置导入 / 导出（本机迁移）──

    /// 备份文件路径（数据根目录下 [`dd_gui::settings::BACKUP_FILE_NAME`]，
    /// 与 config.json 同目录——`config_file()` 不可定位 → `None`）。
    pub(crate) fn settings_backup_path() -> Option<std::path::PathBuf> {
        dd_host::manifest::config_file()
            .map(|p| p.with_file_name(dd_gui::settings::BACKUP_FILE_NAME))
    }

    /// 导出设置备份（原子写；成功 toast 带路径便于拷贝，失败 toast 不静默）。
    pub(crate) fn export_settings_backup(&mut self) {
        let Some(path) = Self::settings_backup_path() else {
            self.show_error_toast(
                self.tr("toast.settings_export_fail")
                    .replace("{e}", "数据目录不可定位"),
            );
            return;
        };
        if self.settings.save_backup_to(&path) {
            self.show_toast(
                self.tr("toast.settings_export_ok")
                    .replace("{p}", &path.display().to_string()),
                Some(4_000),
            );
        } else {
            self.show_error_toast(
                self.tr("toast.settings_export_fail")
                    .replace("{e}", "写盘失败"),
            );
        }
    }

    /// 导入并覆盖（已在 UI 层两步确认）：读备份文件 → 容错解析（机器态保留
    /// 本机现值）→ 逐组应用（主题/材质/圆角/边框走既有 apply 链即时生效；
    /// 语言经 apply_lang 同步托盘 + 聚合脏标记；warm 容量即时调整含缩容驱逐；
    /// 聚合类配置统一 `engines_dirty`——离开设置页重聚合消费）→ 落盘 + toast。
    pub(crate) fn import_settings_backup(&mut self, ctx: &eframe::egui::Context) {
        let Some(path) = Self::settings_backup_path() else {
            self.show_error_toast(
                self.tr("toast.settings_import_fail")
                    .replace("{e}", "数据目录不可定位"),
            );
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("[dd-gui] 备份读取失败（{}）：{e}", path.display());
                self.show_error_toast(
                    self.tr("toast.settings_import_fail")
                        .replace("{e}", &e.to_string()),
                );
                return;
            }
        };
        let imported = match dd_gui::settings::Settings::import_backup(&text, &self.settings) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("[dd-gui] 备份导入校验失败：{e}");
                self.show_error_toast(self.tr("toast.settings_import_fail").replace("{e}", &e));
                return;
            }
        };
        // UI 即时项先经既有 apply 链（各自内部检查差异 + 落盘 + ctx 生效；
        // 须在整体赋值前调用——赋值后差值归零会跳过 set_theme 等副作用）。
        self.apply_theme_pref(ctx, imported.theme);
        self.apply_backdrop(ctx, imported.backdrop);
        self.apply_material_opacity(ctx, imported.material_opacity);
        // T9：背景图路径变化走 apply_background_path（材质链互斥切换）；
        // 导入后 UI 输入缓冲同步为导入值（自增字段随整体赋值生效）。
        self.apply_background_path(ctx, imported.background_image_path.clone());
        self.apply_background_fit(imported.background_image_fit);
        self.apply_background_opacity(imported.background_image_opacity);
        self.apply_background_tint(imported.background_image_tint_intensity);
        self.bg_path_buf = imported.background_image_path.clone().unwrap_or_default();
        self.bg_err = None;
        self.apply_corner_pref(imported.corner_pref);
        self.apply_border_mode(ctx, imported.border_mode);
        if self.settings.lang != imported.lang {
            self.apply_lang(imported.lang);
        }
        self.settings = imported;
        // warm 容量：走 apply_warm_capacity 的驱逐口径（缩容立即回落 stub）。
        for victim in self.lru.set_capacity(self.settings.warm_capacity as usize) {
            self.evict_warm(&victim);
        }
        // 聚合类配置（搜索引擎 / 停用集 / 自定义命令 / ext_settings / 语言）
        // 统一脏标记——离开设置页重聚合消费（与逐项开关同路径）。
        self.engines_dirty = true;
        self.save_settings_with_feedback();
        ctx.request_repaint();
        log::info!("[dd-gui] 设置备份已导入并应用（{}）", path.display());
        self.show_toast(self.tr("toast.settings_import_ok"), Some(4_000));
    }

    /// 设置页扩展**信任决策**（S-05，2026-09-24）：写台账 → 落盘 → 立即重聚合。
    ///
    /// 与 [`Self::apply_extension_enabled`] 的分工（两者都影响"是否被拉起"，但语义不同）：
    /// - **停用集**：用户主动关掉一个**已获信任**的扩展；只改内存/配置，**不写信任台账**；
    /// - **信任决策**：决定一个扩展**是否有资格**被拉起；必须写台账（跨启动持久），
    ///   且哈希随内容绑定（内容一变即回到待批准）。
    ///
    /// 失败一律 toast（不静默）：用户点了「允许」却什么都没发生是最坏体验。
    /// 成功则**立即重聚合**——批准后首屏即出现其命令，阻止后立即停掉。
    pub(crate) fn set_extension_trust(&mut self, id: &str, decision: Decision) {
        let Some(ext) = self.exts.iter().find(|e| e.manifest.id == id).cloned() else {
            self.show_toast(self.tr("toast.ext_missing").to_string(), Some(3_000));
            return;
        };
        let mut ledger = TrustLedger::load();
        if let Err(e) = ledger.record(&ext, decision) {
            log::warn!("[dd-gui] 信任台账记录失败（{id}）：{e}");
            let msg = self.tr("toast.ext_trust_fail").replace("{e}", &e);
            self.show_toast(msg, Some(4_000));
            return;
        }
        if let Err(e) = ledger.save() {
            log::warn!("[dd-gui] 信任台账写盘失败（{id}）：{e}");
            let msg = self
                .tr("toast.ext_trust_fail")
                .replace("{e}", &e.to_string());
            self.show_toast(msg, Some(4_000));
            return;
        }
        log::info!("[dd-gui] 扩展 {id} 信任决策已记录：{decision:?}");
        self.restart_aggregation();
        let key = match decision {
            Decision::Allow => "toast.ext_allowed",
            Decision::Deny => "toast.ext_blocked",
        };
        let msg = self.tr(key).replace("{id}", id);
        self.show_toast(msg, Some(2_000));
    }

    /// 设置页扩展启停（M6 批次 6.3）：更新停用表 + 落盘 + 置脏标记
    /// （离开设置页时重聚合，见 mod.rs 收口点）；幂等调用无副作用。
    pub(crate) fn apply_extension_enabled(&mut self, id: &str, enabled: bool) {
        let mut next = self.settings.disabled_extensions.clone();
        if enabled {
            next.retain(|x| x != id);
        } else if !next.iter().any(|x| x == id) {
            next.push(id.to_string());
        }
        if next == self.settings.disabled_extensions {
            return; // 幂等
        }
        self.settings.disabled_extensions = next;
        self.save_settings_with_feedback();
        self.exts_dirty = true;
    }

    /// 打开设置页（批次 4.0）：推入页面栈（复用嵌套页语义，Esc 返回）。
    /// 已在设置页时幂等（不重复推栈）。每次进入重置左栏栏目到首栏「外观」
    /// （§08 v4.6 B5：栏目为纯视图状态，与 go_home 复位语义一致）。
    pub(crate) fn open_settings(&mut self) {
        if self.stack.current().is_settings {
            return;
        }
        log::debug!("[dd-gui] 打开设置页（PageStack 推页）");
        self.settings_category = SettingsCategory::default();
        self.stack.push(PageState::settings());
    }

    /// 设置页改选主题（批次 4.0）：立即生效 + 持久化（best-effort）。
    pub(crate) fn apply_theme_pref(
        &mut self,
        ctx: &egui::Context,
        pref: dd_gui::settings::ThemePref,
    ) {
        if self.settings.theme == pref {
            return;
        }
        log::debug!("[dd-gui] 主题偏好变更：{} → 立即生效并保存", pref.label());
        self.settings.theme = pref;
        ctx.set_theme(theme::theme_preference(pref));
        // v4.7 D31：材质生效时同步 DWM 明暗染色（跟随新主题；best-effort）
        if self.backdrop_active {
            if let Some(hwnd) = self.hwnd {
                let dark = ctx.theme() == egui::Theme::Dark;
                crate::platform::set_immersive_dark(hwnd, dark);
                // M2 + P4：描边色随主题切换——与 refresh_backdrop 同一单点收口
                // `border_color`（中性档随明暗、强调色档跨主题恒色）。
                crate::platform::set_window_border(hwnd, self.border_color(dark));
            }
        }
        self.save_settings_with_feedback();
    }

    /// 设置页材质开关（v4.7 D30/D31）：更新设置 → 落盘 → 立即应用。
    /// 两开关互斥·后开优先由单值 `backdrop` 派生（开关状态 = 与该值比较），
    /// 开关行点击语义：已开 → 关（None）；未开 → 开（该项）。
    pub(crate) fn apply_backdrop(
        &mut self,
        ctx: &egui::Context,
        backdrop: dd_gui::settings::Backdrop,
    ) {
        if self.settings.backdrop == backdrop {
            return;
        }
        log::debug!("[dd-gui] 窗口材质：{} → 立即生效并保存", backdrop.label());
        self.settings.backdrop = backdrop;
        self.save_settings_with_feedback();
        self.refresh_backdrop(ctx);
    }

    /// T9（2026-10-05）：背景图路径（互斥语义开关）——落盘 + 重走材质链
    /// （设图 → effective = None 回退路径；清图 → 恢复 `settings.backdrop`）。
    /// 纹理缓存按 (路径, mtime) 门控，路径变更自动重载，无需显式失效。
    pub(crate) fn apply_background_path(&mut self, ctx: &egui::Context, path: Option<String>) {
        let path = path.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        if self.settings.background_image_path == path {
            return;
        }
        log::info!(
            "[dd-gui] 背景图：{} → 立即生效并保存",
            match &path {
                Some(p) => p.as_str(),
                None => "清除",
            }
        );
        self.settings.background_image_path = path;
        self.save_settings_with_feedback();
        self.refresh_backdrop(ctx);
    }

    /// T9：背景图不透明度（0–100）——拖动即时生效（纯绘制层读值）、松手落盘
    /// （UI 层 `drag_stopped` 统一 `save_settings_with_feedback`，同材质
    /// 不透明度口径）。
    pub(crate) fn apply_background_opacity(&mut self, pct: u8) {
        let pct = pct.clamp(0, 100);
        self.settings.background_image_opacity = pct;
    }

    /// T9：背景图适应方式（Fill / Stretch）——纯设置项，绘制层每帧读值。
    pub(crate) fn apply_background_fit(&mut self, fit: dd_gui::settings::BgImageFit) {
        if self.settings.background_image_fit == fit {
            return;
        }
        self.settings.background_image_fit = fit;
        self.save_settings_with_feedback();
    }

    /// T9：背景图着色强度（0–100）——拖动即时生效、松手落盘（同上）。
    pub(crate) fn apply_background_tint(&mut self, pct: u8) {
        self.settings.background_image_tint_intensity = pct.clamp(0, 100);
    }

    /// 设置页「不透明度」滑杆（P2 v2，2026-09-13）：即时重算 egui 浓淡层
    /// （`alpha = cap × pct/100`，0 = 纯材质、100 = 面板最实），**不落盘**——
    /// 拖动中每帧触发，写盘由 UI 层在松手（`drag_stopped`）时调
    /// `save_settings_with_feedback()`，避免拖动期间逐帧写盘。材质未生效时无
    /// 视觉可调，仅更新内存值（UI 层此时置灰，正常路径不会进来）。
    pub(crate) fn apply_material_opacity(&mut self, ctx: &egui::Context, pct: u8) {
        let pct = pct.clamp(0, 100);
        if self.settings.material_opacity == pct {
            return;
        }
        self.settings.material_opacity = pct;
        if self.backdrop_active {
            let dark = ctx.theme() == egui::Theme::Dark;
            theme::apply_panel_tint(
                ctx,
                theme::panel_tint_with_opacity(dark, self.settings.backdrop, pct),
                self.colorization(),
            );
        }
    }

    /// 设置页「窗口圆角」（P3，2026-09-13）：落盘 + 即时重设 DWM 圆角偏好
    /// （属性幂等，与 egui 帧内容无关，无防闪面）。HWND 未捕获（首帧前）时
    /// 仅落盘——首帧 `refresh_backdrop` 会按设置应用。
    pub(crate) fn apply_corner_pref(&mut self, pref: dd_gui::settings::CornerPref) {
        if self.settings.corner_pref == pref {
            return;
        }
        log::debug!("[dd-gui] 窗口圆角：{} → 立即生效并保存", pref.label());
        self.settings.corner_pref = pref;
        self.save_settings_with_feedback();
        if let Some(hwnd) = self.hwnd {
            crate::platform::apply_window_chrome(hwnd, pref);
        }
    }

    /// 设置页「面板边框」（P4，2026-09-13）：落盘 + 即时重设描边色；描边仅
    /// 材质生效时绘制（既有语义），未生效时只落盘（UI 层该行置灰）。
    pub(crate) fn apply_border_mode(
        &mut self,
        ctx: &egui::Context,
        mode: dd_gui::settings::BorderMode,
    ) {
        if self.settings.border_mode == mode {
            return;
        }
        log::info!("[dd-gui] 面板边框：{} → 立即生效并保存", mode.label());
        self.settings.border_mode = mode;
        self.save_settings_with_feedback();
        if self.backdrop_active {
            if let Some(hwnd) = self.hwnd {
                let dark = ctx.theme() == egui::Theme::Dark;
                crate::platform::set_window_border(hwnd, self.border_color(dark));
            }
        }
    }

    /// P4 描边色单点收口（`refresh_backdrop` / `apply_theme_pref` /
    /// `apply_border_mode` 同源）：中性 = `border_strong`（随主题明暗）；
    /// 强调色 = 系统强调色（`DwmGetColorizationColor`，取不到回落
    /// `Palette::accent`——跨主题恒色）；关 = `None` 停画。材质未生效时调用方
    /// 本就不画描边，本函数不判 `backdrop_active`。
    fn border_color(&self, dark: bool) -> Option<egui::Color32> {
        use dd_gui::settings::BorderMode;
        match self.settings.border_mode {
            BorderMode::Neutral => Some(theme::Palette::of(dark).border_strong),
            BorderMode::Accent => Some(
                crate::platform::system_accent_color()
                    .unwrap_or_else(|| theme::Palette::of(dark).accent),
            ),
            BorderMode::None => None,
        }
    }

    /// 按当前设置应用 DWM 材质（v4.7 D31 + M1–M4 2026-09-13）。成功 → 面板底
    /// 切为浓淡层（`apply_panel_tint`，亮暗两套 Style 同步注册）+ 明暗染色与
    /// 1px 描边跟随主题；失败（Win10 / 22621 以下）→ 保持
    /// 不透明（platform 层已记日志，回退不阻断）。HWND 未捕获（首帧前）时
    /// 跳过——`ui()` 捕获后会再调用一次。
    ///
    /// **切换防闪（v4.7 真机反馈）**：透明化方向 DWM 先行——材质先在当前
    /// （尚不透明）帧后面就位，下一帧透明面板呈现时即有材质可透出；不透明化
    /// 方向（切到「无材质」）**不能立即清 DWM**——DWM 属性即时生效而 egui 要
    /// 下一帧才画出不透明面板，间隙内桌面穿透一闪。改为置
    /// `backdrop_clear_countdown`，由 `ui()` 末尾在不透明帧呈现之后倒计时清材质。
    pub(crate) fn refresh_backdrop(&mut self, ctx: &egui::Context) {
        let Some(hwnd) = self.hwnd else {
            return;
        };
        // T9（2026-10-05）：背景图生效 → 材质链路整体按「无材质」走（互斥语义
        // ——设置值保持不动，清图后经 apply_background_path 重调本函数即恢复
        // 原材质），本函数内一律以 effective 判定。
        let effective = if self.settings.background_image_path.is_some() {
            dd_gui::settings::Backdrop::None
        } else {
            self.settings.backdrop
        };
        // M3/M4（2026-09-13）：窗口 chrome（圆角 + 禁过渡动画）一次性应用，
        // 与材质选择无关（无材质路径同样圆角）；P3 起圆角档来自设置，改选时
        // 经 `apply_corner_pref` 重调；Win10 无对应属性 → platform 层失败跳过。
        if !self.chrome_applied {
            self.chrome_applied = true;
            crate::platform::apply_window_chrome(hwnd, self.settings.corner_pref);
        }
        // ── 不透明化方向（backdrop = None）：先绘制不透明，后清材质 ──
        if effective == dd_gui::settings::Backdrop::None {
            if self.backdrop_active {
                self.backdrop_active = false;
                // M1：面板底回实色；M2：材质场景结束 → 停画描边。
                theme::apply_panel_tint(ctx, None, self.colorization());
                crate::platform::set_window_border(hwnd, None);
                // 倒计时 3 帧：点击帧（旧透明视觉）→ 第 1 个不透明帧绘制并呈现
                // → 第 2 个不透明帧呈现后清 DWM 材质。全程无透明帧暴露窗口。
                self.backdrop_clear_countdown = 3;
            }
            return;
        }
        // ── 透明化方向（云母 / 亚克力）：DWM 先行，再切透明视觉 ──
        // M1：材质 → DWM 类型收敛为 `From<Backdrop>` 单一来源（platform.rs），
        // 新增档位无需改本处。
        let kind = crate::platform::SystemBackdrop::from(effective);
        let ok = crate::platform::apply_system_backdrop(hwnd, kind);
        let active = ok;
        if active {
            let dark = ctx.theme() == egui::Theme::Dark;
            crate::platform::set_immersive_dark(hwnd, dark);
            // M2 + P4：材质生效 → 1px 描边，颜色按边框模式单点收口（中性随
            // 主题 / 强调色恒色 / 关 = 停画）；主题切换的同步点在 apply_theme_pref
            // ——材质切换不经该路径。
            crate::platform::set_window_border(hwnd, self.border_color(dark));
            // M1 + P2：面板底浓淡层随主题、材质与不透明度设置（云母/亚克力互
            // 切、不透明度拖动都要刷新面板底——原实现两档同为全透明无需刷新，
            // M1 起语义不同）。
            theme::apply_panel_tint(
                ctx,
                theme::panel_tint_with_opacity(dark, effective, self.settings.material_opacity),
                self.colorization(),
            );
        }
        if active != self.backdrop_active {
            self.backdrop_active = active;
            if !active {
                // apply_system_backdrop 失败（Win10 / 22621 以下）→ 回退不透明
                // 面板底 + 停描边，视觉与 v4.6 一致。
                theme::apply_panel_tint(ctx, None, self.colorization());
                crate::platform::set_window_border(hwnd, None);
            }
        }
    }

    /// B1（2026-09-20）：Esc 键行为。纯设置项（无附加副作用），照既有范式
    /// 「落盘 + 日志」即可——按键分支每次读取生效值。
    pub(crate) fn apply_esc_behavior(&mut self, b: dd_gui::settings::EscBehavior) {
        if self.settings.esc_behavior == b {
            return;
        }
        log::debug!("[dd-gui] Esc 键行为：{} → 立即生效并保存", b.label());
        self.settings.esc_behavior = b;
        self.save_settings_with_feedback();
    }

    /// T7（2026-09-20）：单击激活开关（默认开 = 既有行为）。纯设置项：
    /// 行点击分支每次读取生效值。
    pub(crate) fn apply_single_click_activation(&mut self, on: bool) {
        if self.settings.single_click_activation == on {
            return;
        }
        log::debug!(
            "[dd-gui] 单击激活：{} → 立即生效并保存",
            if on {
                "开"
            } else {
                "关（单击选中、双击执行）"
            }
        );
        self.settings.single_click_activation = on;
        self.save_settings_with_feedback();
    }

    /// T8（2026-09-20）：界面动效开关（默认开 = 既有行为）。纯设置项：
    /// 过渡调用点每次读取生效值（关闭 = 直出终态）。
    pub(crate) fn apply_ui_animations(&mut self, on: bool) {
        if self.settings.ui_animations == on {
            return;
        }
        log::debug!(
            "[dd-gui] 界面动效：{} → 立即生效并保存",
            if on { "开" } else { "关" }
        );
        self.settings.ui_animations = on;
        self.save_settings_with_feedback();
    }

    /// T6（2026-09-20）：着色配置投影（设置 → `theme::Colorization`）——
    /// 所有 `apply_panel_tint` 调用点统一经此取值，避免各处重复拼装。
    pub(crate) fn colorization(&self) -> theme::Colorization {
        theme::Colorization::from_settings(&self.settings)
    }

    /// T6：着色模式（系统强调色 / 无 / 自定义）——即时重注册浓淡层 + 落盘。
    pub(crate) fn apply_colorization(
        &mut self,
        ctx: &egui::Context,
        mode: dd_gui::settings::ColorizationMode,
    ) {
        if self.settings.colorization == mode {
            return;
        }
        log::debug!("[dd-gui] 着色模式：{} → 立即生效并保存", mode.label());
        self.settings.colorization = mode;
        self.save_settings_with_feedback();
        self.repaint_panel_tint(ctx);
    }

    /// T6：自定义浓淡色——即时生效；**指针未按下时**才落盘（色盘拖动期逐帧
    /// 触发，避免拖动期间频繁写盘）。
    pub(crate) fn apply_custom_tint_color(&mut self, ctx: &egui::Context, rgb: [u8; 3]) {
        if self.settings.custom_tint_color == rgb {
            return;
        }
        self.settings.custom_tint_color = rgb;
        self.repaint_panel_tint(ctx);
        // 指钟未按下才落盘（色盘拖动期逐帧触发，避免频繁写盘）
        if !ctx.input(|i| i.pointer.any_down()) {
            self.save_settings_with_feedback();
        }
    }

    /// T6：自定义着色强度（0–100）——拖动即时生效、松手落盘（同材质不透明度口径）。
    pub(crate) fn apply_custom_tint_intensity(&mut self, ctx: &egui::Context, pct: u8) {
        let pct = pct.min(100);
        if self.settings.custom_tint_intensity == pct {
            return;
        }
        self.settings.custom_tint_intensity = pct;
        self.repaint_panel_tint(ctx);
    }

    /// T6：按当前设置重注册面板浓淡层（着色变更的统一出口；材质未生效时
    /// 无视觉可调，仅更新内存值）。
    pub(crate) fn repaint_panel_tint(&mut self, ctx: &egui::Context) {
        if !self.backdrop_active {
            return;
        }
        let dark = ctx.theme() == egui::Theme::Dark;
        let tint = theme::panel_tint_with_opacity(
            dark,
            self.settings.backdrop,
            self.settings.material_opacity,
        );
        theme::apply_panel_tint(ctx, tint, self.colorization());
    }

    /// B2（2026-09-20）：退格键返回开关（同上，纯设置项）。
    pub(crate) fn apply_backspace_go_back(&mut self, on: bool) {
        if self.settings.backspace_go_back == on {
            return;
        }
        log::debug!(
            "[dd-gui] 退格键返回：{} → 立即生效并保存",
            if on { "开" } else { "关" }
        );
        self.settings.backspace_go_back = on;
        self.save_settings_with_feedback();
    }

    /// T5（2026-09-20）：「恢复默认外观」——复用既有 `apply_*` 即时生效链路
    /// （各自幂等、各自落盘），默认值唯一来源 = `Settings::default()`；
    /// 范围**刻意不含**热键 / 语言 / 搜索引擎 / 扩展启停 / 自启 / 面板尺寸
    /// （那些属功能配置，重置外观不应改动）。
    pub(crate) fn apply_reset_appearance(&mut self, ctx: &egui::Context) {
        let d = dd_gui::settings::Settings::default();
        self.apply_theme_pref(ctx, d.theme);
        self.apply_backdrop(ctx, d.backdrop);
        self.apply_material_opacity(ctx, d.material_opacity);
        self.apply_background_path(ctx, d.background_image_path);
        self.apply_background_fit(d.background_image_fit);
        self.apply_background_opacity(d.background_image_opacity);
        self.apply_background_tint(d.background_image_tint_intensity);
        self.bg_path_buf.clear();
        self.bg_err = None;
        self.apply_corner_pref(d.corner_pref);
        self.apply_border_mode(ctx, d.border_mode);
        if self.settings.density != d.density {
            self.settings.density = d.density;
        }
        self.save_settings_with_feedback();
        log::info!("[dd-gui] 外观已恢复默认（主题/材质/浓淡/圆角/边框/密度）");
        self.show_toast(self.tr("set.reset.toast"), Some(1_500));
    }

    /// 设置页改选「打开面板时显示」：立即生效（重算 root 首屏可见表）+ 持久化。
    pub(crate) fn apply_open_view(&mut self, ctx: &egui::Context, show_all: bool) {
        let view = if show_all {
            dd_gui::settings::OpenView::All
        } else {
            dd_gui::settings::OpenView::Default
        };
        if self.settings.open_view == view {
            return;
        }
        log::debug!("[dd-gui] 首屏视图变更：{}", view.label());
        self.settings.open_view = view;
        self.stack.root_mut().list.set_empty_view(if show_all {
            dd_gui::state::EmptyQueryView::All
        } else {
            dd_gui::state::EmptyQueryView::WithoutApps
        });
        self.save_settings_with_feedback();
        ctx.request_repaint();
    }

    /// 设置页改「搜索应用」（2026-09-12）：立即重算根页可见表（空查询与
    /// 关键词匹配两条分支都在 `set_apps_hidden` 内重算）+ 持久化。
    pub(crate) fn apply_search_apps(&mut self, ctx: &egui::Context, on: bool) {
        if self.settings.search_apps == on {
            return;
        }
        log::debug!("[dd-gui] 搜索应用：{on}");
        self.settings.search_apps = on;
        self.stack.root_mut().list.set_apps_hidden(!on);
        self.save_settings_with_feedback();
        ctx.request_repaint();
    }

    /// 设置页改「搜索 Steam 游戏」（2026-10-07）：立即重算根页可见表（空查询
    /// 与关键词匹配两条分支都在 `set_steam_hidden` 内重算）+ 持久化。
    pub(crate) fn apply_search_steam_games(&mut self, ctx: &egui::Context, on: bool) {
        if self.settings.search_steam_games == on {
            return;
        }
        log::debug!("[dd-gui] 搜索 Steam 游戏：{on}");
        self.settings.search_steam_games = on;
        self.stack.root_mut().list.set_steam_hidden(!on);
        self.save_settings_with_feedback();
        ctx.request_repaint();
    }

    /// 设置页搜索引擎变更（勾选预设/添加/删除自定义，2026-09-05）：
    /// 立即持久化 + 置脏标记；**离开设置页时**由 `ui()` 的 size-diff 收口点
    /// 消费并全量重聚合（websearch 进程须以新环境变量重启才能生效）。
    pub(crate) fn apply_search_engines(&mut self, ctx: &egui::Context) {
        log::debug!(
            "[dd-gui] 搜索引擎配置变更：{} 个引擎已保存（离开设置页后重新聚合生效）",
            self.settings.search_engines.len()
        );
        self.engines_dirty = true;
        self.save_settings_with_feedback();
        ctx.request_repaint();
    }

    /// 搜索引擎配置变更后的全量重聚合：重走 scan → 注入引擎环境 → collect
    /// → 替换 Root 列表（复用首屏聚合的全部既有机制，含进程替换与 LRU）。
    pub(crate) fn restart_aggregation(&mut self) {
        log::debug!("[dd-gui] 搜索引擎配置变更 → 重新聚合首屏");
        // 进程将以新环境变量（搜索引擎/语言）重启：作废已缓存的兜底模板与
        // 在途拉取，否则会残留旧引擎/旧语言的兜底项（如仅启用 Bing 仍显示
        // 全部搜索引擎的「在 X 搜索 …」）。下一帧由新进程重新拉取正确模板。
        self.fallback_store.clear();
        self.inflight.clear();
        self.fallback_rx = None; // 丢弃旧在途拉取（其进程属旧环境）
        let (tx, rx) = mpsc::channel();
        crate::app::spawn_aggregation(
            tx,
            self.cache.clone(),
            self.settings.search_engines_env(),
            self.settings.disabled_extensions.clone(),
            self.lang_effective,
            self.settings.custom_commands.clone(),
            self.settings.ext_settings.clone(),
        );
        self.aggregate_rx = Some(rx);
        self.aggregating = true;
    }
}
