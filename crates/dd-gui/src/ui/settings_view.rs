//! 设置页视图与设置卡片族（设计稿 08，v4.6 左右布局：左栏分组 + 右侧设置项）。

// O12（2026-10-11）：巨型文件拆分——卡片族按设置栏目拆入子模块（appearance/
// general/search/extensions），热键捕获对话框独立（hotkey_dialog），自绘控件族
// 独立（widgets），本文件保留页面入口（draw_settings + 左栏导航）、共享常量与
// 单测。纪律：搬运单位 = 完整定义块（含文档注释），函数体一字不改；仅将跨
// 子模块项 `pub(super)` 化（编译器指示的最小可见性改动）。

pub(crate) use crate::app::PaletteApp;
pub(crate) use crate::ui::widgets::draw_back_btn;
pub(crate) use crate::ui::widgets::text_width;
pub(crate) use dd_gui::aggregator::{SourceStatus, SourceSummary};
pub(crate) use dd_gui::theme;
pub(crate) use dd_host::manifest::LoadedExtension;
pub(crate) use dd_host::trust::{Assessment, Decision, ExtOrigin, Trust};
pub(crate) use eframe::egui;
pub(crate) use std::collections::HashMap;

mod appearance;
mod extensions;
mod general;
mod hotkey_dialog;
mod search;
mod widgets;

// 卡片族方法经 inherent impl 直呼，无需路径重导出；widgets 供兄弟模块
// （states.rs 取 accent_soft、panel.rs 取 draw_ext_chip）与单测取用
pub(crate) use widgets::*;

// 设置页自绘控件排印（D42：Fluent compact 档，文字 = ramp base200）。
// K2：v4.9/D34「整体字号放大」把控件与正文一并提到 14，控件失去次级定位——
// 此处仅回调控件档（正文/行名 14 semibold / 描述 12 不变），并配套把盒高
// 32→28（Fluent compact 成对比例，12pt 文字 + 32px 盒会显空旷）。
pub(crate) const CONTROL_H: f32 = 28.0; // 下拉框/标准按钮/输入框盒高（原 32）
pub(crate) const CONTROL_FONT_PT: f32 = 12.0; // 控件文字（原 14；ramp base200）
const POPUP_ITEM_H: f32 = 24.0; // 下拉 popup 选项行高（原 28）

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
        // Fluent subtitle 档，40px 顶行内仍居中）。字重 regular（2026-09-13
        // 真机反馈「不用加粗字体」，semibold() 已回落 regular 字形）。
        // 0.05em 字距为排印规格保留（painter.text 不支持，故经
        // LayoutJob 排版后 painter.galley 直绘：左对齐 + 垂直居中手工对位）。
        let mut title_job = egui::text::LayoutJob::single_section(
            self.tr("page.settings").to_owned(),
            egui::TextFormat::simple(dd_gui::theme::semibold(18.0), p.text),
        );
        title_job.sections[0].format.extra_letter_spacing = 18.0 * 0.05;
        let title_galley = ui.ctx().fonts_mut(|f| f.layout_job(title_job));
        ui.painter().galley(
            egui::pos2(back_rect.right() + 8.0, cy - title_galley.size().y / 2.0),
            title_galley,
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
            self.go_back_focused(); // 返回 + 聚焦回落后页面的搜索框
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
            // B7 真机修订（2026-09-12）：`Response::hovered()` 在本应用不生效，
            // 悬停/光标改用与列表行同款的几何判定（rect_contains_pointer）。
            let hovered_now = nav_ui.rect_contains_pointer(item_rect);
            if hovered_now && !selected {
                nav_ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let pressed_now = hovered_now && resp.is_pointer_button_down_on();
            let radius = egui::CornerRadius::same(4);
            // B7 悬停语言：选中项不铺 hover（避免盖过选中语义，沿用原分支）；
            // 未选项 hover = row_hover、按下 = row_pressed。
            if selected {
                nav_ui
                    .painter()
                    .rect_filled(item_rect, radius, p.row_selected);
            } else if pressed_now {
                nav_ui
                    .painter()
                    .rect_filled(item_rect, radius, p.row_pressed);
            } else if hovered_now {
                nav_ui
                    .painter()
                    .rect_filled(item_rect, radius, p.control_hover);
            }
            if selected {
                // 左缘 3×16 accent 指示条（radius 2，垂直居中）——与列表行选中
                // 语言（D9）同构，未引入新 token。
                let indicator = egui::Rect::from_min_size(
                    egui::pos2(item_rect.left(), item_rect.center().y - 8.0),
                    egui::vec2(3.0, 16.0),
                );
                nav_ui.painter().rect_filled(
                    indicator,
                    egui::CornerRadius::same(2),
                    p.accent_stroke,
                );
            }
            // 图标 16px：内边距 12 + 槽位 16 居中；文字：图标右 12 起（左+40）。
            // B7：未选项 hover 时图标/文字 text2→text 提亮。
            let fg = if selected || hovered_now {
                p.text
            } else {
                p.text2
            };
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
                self.draw_background_image_card(ui, &p, &ctx);
                self.draw_density_card(ui, &p);
                ui.add_space(8.0); // 卡片间距 8px（§08.1）
                self.draw_reset_appearance_card(ui, &p);
            }
            SettingsCategory::General => {
                self.draw_general_cards(ui, &p);
                ui.add_space(8.0); // 卡片间距 8px（§08.1）
                self.draw_keys_behavior_card(ui, &p);
            }
            SettingsCategory::Search => {
                self.draw_search_behavior_card(ui, &p);
                ui.add_space(8.0); // 卡片间距 8px（§08.1）
                self.draw_search_engine_card(ui, &p, &ctx);
            }
            SettingsCategory::Extensions => self.draw_extensions_card(ui, &p),
        });

        // ── PowerToys 式热键捕获对话框（2026-09-30 seq 确认流配套）：ctx 层
        // Area/Window，滚动内容之后绘制保证置顶；遮罩吞下层交互（模态语义）。
        self.draw_hotkey_capture_dialog(ui.ctx(), &p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // O12：extension_rows 随扩展管理拆入 extensions 子模块（pub(super)）
    use extensions::extension_rows;

    #[test]
    fn settings_category_default_is_appearance() {
        // §08 v4.6（B5）：默认选中首栏「外观」；open_settings 每次进入重置到
        // default（与 go_home 复位语义一致），栏目不落盘。
        assert_eq!(SettingsCategory::default(), SettingsCategory::Appearance);
    }

    /// D42（K2）：控件排印 compact 档常量锚定，防回归漂移。
    /// 盒高 28 / 控件文字 12（ramp base200）/ popup 选项行高 24。
    #[test]
    fn control_typography_constants_d42() {
        assert_eq!(CONTROL_H, 28.0);
        assert_eq!(CONTROL_FONT_PT, 12.0);
        assert_eq!(POPUP_ITEM_H, 24.0);
    }

    /// D42/K3：下拉宽度自适应——①下限/上限两分支精确断言（短选项 = 180、
    /// 超长选项 = 260）；②真实 zh/en 选项集落在 (180, 260] 区间内且不溢出。
    /// 注：方案 §3.3「zh 常态落 180」估宽前提不成立——Esc 卡最长选项实为
    /// 「先清除搜索内容，然后返回」（13 全角字 ≈188+），非「返回上一级
    /// （默认）」；zh 按 fallback 字形计宽。
    /// O5-a（2026-10-07）：`default_fonts` 移除后 headless 上下文字体集为空、
    /// 测宽恒 0——装入系统 segoeui.ttf 复现「有拉丁字形」的测宽前提（测试仅
    /// 在 windows CI / 真机跑，该文件必有）。
    #[test]
    fn dropdown_width_clamps_zh_and_en() {
        let ctx = egui::Context::default();
        let segoeui = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf").expect(
            "测试前置失败：C:\\Windows\\Fonts\\segoeui.ttf 不可读（本测试仅在 Windows 跑）",
        );
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "segoeui".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(segoeui)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .push("segoeui".to_owned());
        ctx.set_fonts(fonts);
        let mut out = ctx.run_ui(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                // ① 分支行为：短选项 → 下限 180；超长选项 → 上限 260。
                assert_eq!(dropdown_width(ui, &["Go"]), 180.0);
                assert_eq!(
                    dropdown_width(
                        ui,
                        &["Clear the search first, then go back and then go back again"],
                    ),
                    260.0
                );
                // ② 真实选项集：zh/en 均在区间内（en 按最长英文选项撑宽）。
                let zh_labels = [
                    "返回上一级（默认）",
                    "先清除搜索内容，然后返回",
                    "始终隐藏面板",
                ];
                let en_labels = [
                    "Go back (default)",
                    "Clear the search first, then go back",
                    "Always hide the panel",
                ];
                let zh = dropdown_width(ui, &zh_labels);
                let en = dropdown_width(ui, &en_labels);
                assert!((180.0..=260.0).contains(&zh), "zh width = {zh}");
                assert!(en > 180.0 && en <= 260.0, "en width = {en}");
            });
        });
        // headless 无渲染后端：丢弃字体纹理增量（不消费会在 Drop 时 panic）。
        out.textures_delta.clear();
    }

    /// P2 v6：自绘滑杆的指针→档位纯函数——映射、越界钳制与零宽防除零。
    #[test]
    fn opacity_slider_value_maps_and_clamps() {
        assert_eq!(super::slider_value(0.0, 100.0, 0.0), 0);
        assert_eq!(super::slider_value(0.0, 100.0, 40.0), 40);
        assert_eq!(super::slider_value(0.0, 100.0, 100.0), 100);
        // 越界指针钳制到 0–100
        assert_eq!(super::slider_value(0.0, 100.0, -20.0), 0);
        assert_eq!(super::slider_value(0.0, 100.0, 150.0), 100);
        // 零宽防除零
        assert_eq!(super::slider_value(10.0, 0.0, 10.0), 0);
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

    /// 扩展管理行：失败原因只在 `SourceStatus::Failed` 时给出（warm / stub → `None`），
    /// 且停用集决定开关态。2026-09-10 增补——此前 `Failed.error` 从未被渲染，
    /// 用户只能看到「暂时不可用」而无从得知原因。
    #[test]
    fn extension_rows_surface_failure_reason() {
        let exts = vec![
            dd_host::manifest::from_executable("a.exe".into(), "com.example.a", "Ext A"),
            dd_host::manifest::from_executable("b.exe".into(), "com.example.b", "Ext B"),
            dd_host::manifest::from_executable("c.exe".into(), "com.example.c", "Ext C"),
        ];
        let sources = vec![
            SourceSummary {
                id: "com.example.a".into(),
                name: "Ext A".into(),
                status: SourceStatus::Failed {
                    error: "spawn 失败：os error 2（命令：a.exe）".into(),
                },
            },
            SourceSummary {
                id: "com.example.b".into(),
                name: "Ext B".into(),
                status: SourceStatus::Warm { commands: 2 },
            },
        ];
        let disabled = vec!["com.example.c".to_string()];
        // S-05：传空判定表 → 全部按 fail-closed 渲染（Pending / UserDir），
        // 本用例只关心失败原因与停用集，故同时锁定 fail-closed 口径。
        let trust: HashMap<String, Assessment> = HashMap::new();

        let rows = extension_rows(&exts, &disabled, &sources, &trust);
        assert_eq!(rows.len(), 3);
        assert!(rows[0].enabled, "a 未停用 → 开关开");
        assert_eq!(
            rows[0].failed_reason.as_deref(),
            Some("spawn 失败：os error 2（命令：a.exe）"),
            "失败原因应透出（含被尝试的命令路径）"
        );
        assert!(rows[1].enabled, "b 未停用 → 开关开");
        assert_eq!(rows[1].failed_reason, None, "warm 态无失败原因");
        assert!(!rows[2].enabled, "c 在停用集内 → 开关关");
        assert_eq!(
            rows[2].failed_reason, None,
            "无运行态记录（未扫描到）→ 无失败原因"
        );
        // fail-closed：判定表缺项不得被渲染成「可信」
        assert_eq!(rows[0].trust, Trust::Pending, "缺项 → 待批准");
        assert_eq!(rows[0].origin, ExtOrigin::UserDir, "缺项 → 按用户安装显示");
        assert!(rows[0].needs_approval());
        // `from_executable("a.exe")` → dir = "" → 清单路径 = "<id>.json"
        assert_eq!(rows[0].manifest_path, "com.example.a.json");
        assert_eq!(rows[0].exe_path, "a.exe");
    }

    /// S-05：信任状态与来源驱动行内按钮与标签（纯视图逻辑，不依赖 egui）。
    #[test]
    fn extension_rows_expose_trust_and_origin() {
        let exts = vec![
            dd_host::manifest::from_executable("x.exe".into(), "com.example.p", "P"),
            dd_host::manifest::from_executable("y.exe".into(), "com.ddrun.filesearch", "F"),
        ];
        let mut trust: HashMap<String, Assessment> = HashMap::new();
        trust.insert(
            "com.example.p".to_string(),
            Assessment {
                id: "com.example.p".to_string(),
                trust: Trust::Blocked,
                origin: ExtOrigin::UserDir,
                sidecar_tampered: false,
            },
        );
        trust.insert(
            "com.ddrun.filesearch".to_string(),
            Assessment {
                id: "com.ddrun.filesearch".to_string(),
                trust: Trust::AutoTrusted,
                origin: ExtOrigin::Builtin,
                sidecar_tampered: false,
            },
        );

        let rows = extension_rows(&exts, &[], &[], &trust);
        // 已阻止：需要审批按钮（撤销路径），标签键为 blocked
        assert!(rows[0].needs_approval());
        assert_eq!(rows[0].trust_key(), "set.ext.trust.blocked");
        assert_eq!(rows[0].origin_key(), "set.ext.origin.user");
        // 自动信任：无按钮、无状态标签（常态不占版面）
        assert!(!rows[1].needs_approval());
        assert_eq!(rows[1].trust_key(), "");
        assert_eq!(rows[1].origin_key(), "set.ext.origin.builtin");
    }
}
