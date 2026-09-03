//! M5 UI 批次 3 视觉主题层：设计稿 token → egui `Style`/`Visuals`。
//!
//! 契约来源：`cmdpal-ui-mockups.html` 05「设计 token → egui 映射」表 + CSS 派生
//! 组件值（searchbar / chip / badge / 页脚状态点）。亮暗两套视觉分别注册到
//! egui（`set_visuals_of(Theme::Dark|Light)`），主题偏好 `System` 跟随系统；
//! 无系统主题信息时 egui 回落暗色（设计稿 05 note「默认暗色」）。
//!
//! 语义：本模块是**唯一 token 源**——绘制层不写裸色值（`Visuals` 覆盖不了的
//! 场景：行 hover/选中填充、左侧 accent 指示条、chip/badge 底、搜索框聚焦下划线、
//! 页脚状态点等，统一经 [`Palette`] 取色）。

// 本 crate 未直接依赖 `egui`（bin 层经 `eframe::egui` re-export 使用）；
// lib 层统一走同一路径，避免重复声明 egui 依赖。
use eframe::egui::{Color32, Context, CornerRadius, Stroke, Theme, ThemePreference, Visuals};

/// 面板逻辑尺寸外的行/搜索栏几何常量（与设计稿 05 note 对齐）。
pub const ROW_H: f32 = 44.0; // 行高（设计稿 05 note）
pub const ROW_RADIUS: u8 = 6; // 行圆角（CSS `.row` border-radius 6px）
pub const ACCENT_BAR_W: f32 = 3.0; // 选中左侧指示条宽（CSS 3px）
pub const SEARCHBAR_H: f32 = 46.0; // 搜索栏高（设计稿 05 note：large 46px）

// ── 页脚（`.panel-footer` / `.keys` / `.dot`）几何 ─────────────────────────
// 05 token 表只列了 8 个语义色，页脚的底色/描边/字号来自 CSS 派生，集中放这里，
// 避免在绘制层散落裸值。
pub const FOOTER_PAD_X: f32 = 16.0; // `.panel-footer` padding: 9px 16px
pub const FOOTER_PAD_Y: f32 = 9.0;
pub const FOOTER_FONT: f32 = 11.5; // `.panel-footer` font-size
pub const FOOTER_GAP: f32 = 14.0; // `.panel-footer` gap
pub const KEYCAP_FONT: f32 = 10.5; // `.panel-footer b`
pub const KEYCAP_H: f32 = 17.0; // 键帽盒高（10.5px 行高 + 上下描边留白）
pub const KEYCAP_PAD_X: f32 = 6.0; // `.panel-footer b` padding: 0 6px
pub const KEYCAP_GAP: f32 = 2.0; // 同组键帽之间的间隙（`↑` `↓`）
pub const DOT_SIZE: f32 = 6.0; // `.dot` 6×6
pub const DOT_GAP: f32 = 5.0; // `.dot` margin-right 5px

// ── 设置按钮（设计稿 §6.1，批次 4.0）─────────────────────────────────────
/// 齿轮热区边长：24×24 px（视觉 16px + 8px 可点击扩展）。
pub const GEAR_SIZE: f32 = 24.0;
/// 齿轮视觉字号 16px（§6.1 规格）。
pub const GEAR_FONT: f32 = 16.0;
/// 齿轮 glyph（Segoe Fluent/MDL2 "Settings" U+E713）。
pub const GEAR_GLYPH: char = '\u{E713}';

/// 亮/暗语义色板（05 表 + CSS 派生；数值逐一有 parity 单测守卫）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// 面板背景（05 `--panel`）
    pub panel: Color32,
    /// 强边框：面板/搜索框描边（05 `--border-strong`）
    pub border_strong: Color32,
    /// 弱边框：页脚顶边、键帽描边（CSS `--border`；05 表未列）
    pub border: Color32,
    /// 次级面板底：页脚背景（CSS `--panel-2`；05 表未列）
    pub panel_2: Color32,
    /// 主文本（05 `--text`）
    pub text: Color32,
    /// 次级文本：描述徽标/页脚/图标（05 `--text-2`）
    pub text2: Color32,
    /// 三级文本：分组标题/placeholder（05 `--text-3`）
    pub text3: Color32,
    /// 强调色：选中指示条/聚焦下划线/链接（05 `--accent`）
    pub accent: Color32,
    /// 行 hover 填充（05 `--row-hover`）
    pub row_hover: Color32,
    /// 选中行填充（05 `--row-selected`）
    pub row_selected: Color32,
    /// 搜索框填充（CSS `--input-fill`）
    pub input_fill: Color32,
    /// tag chip 底（CSS `--chip-bg`）
    pub chip_bg: Color32,
    /// 描述徽标底（CSS `--badge-bg`）
    pub badge_bg: Color32,
    /// 页脚来源状态点：正常（CSS `.dot.ok`，亮暗不同值）
    pub dot_ok: Color32,
    /// 页脚来源状态点：异常（CSS `.dot.err`，亮暗不同值）
    pub dot_err: Color32,
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            panel: rgb(0x29_29_29),
            border_strong: rgb(0x4d_4d_4d),
            border: rgb(0x3d_3d_3d),
            panel_2: rgb(0x23_23_23),
            text: rgb(0xff_ff_ff),
            text2: rgb(0xd1_d1_d1),
            text3: rgb(0x8d_8d_8d),
            accent: rgb(0x47_9e_f5),
            row_hover: rgba(255, 255, 255, 15), // 0.0605 × 255 ≈ 15
            row_selected: rgba(255, 255, 255, 21), // 0.0837 × 255 ≈ 21
            input_fill: rgb(0x1f_1f_1f),
            chip_bg: rgb(0x33_33_33),
            badge_bg: rgb(0x3d_3d_3d),
            dot_ok: rgb(0x3e_cf_8e),
            dot_err: rgb(0xf2_55_5a),
        }
    }

    pub fn light() -> Self {
        Self {
            panel: rgb(0xff_ff_ff),
            border_strong: rgb(0xd1_d1_d1),
            border: rgb(0xe0_e0_e0),
            panel_2: rgb(0xfa_fa_fa),
            text: rgb(0x24_24_24),
            text2: rgb(0x61_61_61),
            text3: rgb(0x97_97_97),
            accent: rgb(0x0f_6c_bd),
            row_hover: rgb(0xf5_f5_f5),
            row_selected: rgb(0xf0_f0_f0),
            input_fill: rgb(0xff_ff_ff),
            chip_bg: rgb(0xf0_f0_f0),
            badge_bg: rgb(0xf0_f0_f0),
            dot_ok: rgb(0x10_7c_41),
            dot_err: rgb(0xc5_0f_1f),
        }
    }

    /// 按当前主题取色板（egui `Visuals::dark_mode` 为唯一真源）。
    pub fn of(dark_mode: bool) -> Self {
        if dark_mode {
            Self::dark()
        } else {
            Self::light()
        }
    }
}

/// 05 表 → egui `Visuals`：以 egui 默认视觉为基底，覆盖 token 可映射字段。
/// 组件类色板（chip/badge/行态）不进 `Visuals`（无对应字段），由绘制层经
/// [`Palette`] 直接取用。
pub fn visuals(dark: bool) -> Visuals {
    let p = Palette::of(dark);
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.dark_mode = dark;
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.badge_bg; // TextEdit/滚动条底
    v.faint_bg_color = p.row_hover;
    v.code_bg_color = p.badge_bg;
    // 选中态（egui `paint_text_selection`：bg_fill 画选中底，**选中字形会被
    // 重绘为 stroke.color**——若两者同色文字即不可见。真机 2026-09-03：搜索框
    // 全选后整段变纯蓝块）。修复 = Fluent/Windows 原生风格：半透明 accent 底
    // （30%，文本保持可读）+ 主文本色字形；暗色主题下白字 + 半透明蓝底同样可读。
    v.selection.bg_fill = p.accent.gamma_multiply(0.30);
    v.selection.stroke = Stroke::new(1.0, p.text);
    // IME 组合文本用新版渲染（下划线 + 组合内光标），禁用 legacy（=选区式蓝底）。
    // 根因：egui 0.36 在 Windows 上 legacy_visuals 默认 true（因 winit 韩文光标
    // bug，见 style.rs `ImeComposition` 注释），组合中文会被涂成整块 selection
    // 底色（= accent 蓝块）。新 visuals 专为中日韩输入设计；该 winit bug 仅影响
    // 韩文，中文（Microsoft Pinyin）正常。真机 2026-09-03 反馈：输入中文变蓝块。
    v.ime_composition.legacy_visuals = false;
    v.hyperlink_color = p.accent;
    v.override_text_color = Some(p.text);
    v.weak_text_alpha = 1.0; // weak 色由 weak_text_color 显式接管，不叠加透明
    v.weak_text_color = Some(p.text2);
    v.error_fg_color = p.dot_err;
    v.window_corner_radius = CornerRadius::same(8);
    v.window_stroke = Stroke::new(1.0, p.border_strong);
    v
}

/// 注册双主题 + 按用户偏好设定主题（M5 批次 4.0 起偏好来自持久化设置，
/// 不再写死跟随系统）。程序启动时调用一次；此后用户在设置页改选时由
/// `Context::set_theme` 运行时切换（无需重启）。
pub fn apply(ctx: &Context, pref: ThemePreference) {
    ctx.set_visuals_of(Theme::Dark, visuals(true));
    ctx.set_visuals_of(Theme::Light, visuals(false));
    ctx.set_theme(pref);
}

/// [`settings::ThemePref`] → egui [`ThemePreference`]（设置页选择立即生效用）。
pub fn theme_preference(pref: crate::settings::ThemePref) -> ThemePreference {
    match pref {
        crate::settings::ThemePref::System => ThemePreference::System,
        crate::settings::ThemePref::Light => ThemePreference::Light,
        crate::settings::ThemePref::Dark => ThemePreference::Dark,
    }
}

fn rgb(v: u32) -> Color32 {
    Color32::from_rgb(
        ((v >> 16) & 0xff) as u8,
        ((v >> 8) & 0xff) as u8,
        (v & 0xff) as u8,
    )
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 05 表 parity 守卫：token 十六进制值必须与设计稿一致（防手滑改色）。
    #[test]
    fn dark_palette_matches_design_tokens() {
        let p = Palette::dark();
        assert_eq!(p.panel, rgb(0x29_29_29), "--panel 暗");
        assert_eq!(p.border_strong, rgb(0x4d_4d_4d), "--border-strong 暗");
        assert_eq!(p.text, rgb(0xff_ff_ff), "--text 暗");
        assert_eq!(p.text2, rgb(0xd1_d1_d1), "--text-2 暗");
        assert_eq!(p.text3, rgb(0x8d_8d_8d), "--text-3 暗");
        assert_eq!(p.accent, rgb(0x47_9e_f5), "--accent 暗");
        // CSS 派生（05 表未列）：页脚底 `--panel-2` / 弱边框 `--border`
        assert_eq!(p.panel_2, rgb(0x23_23_23), "--panel-2 暗");
        assert_eq!(p.border, rgb(0x3d_3d_3d), "--border 暗");
    }

    #[test]
    fn light_palette_matches_design_tokens() {
        let p = Palette::light();
        assert_eq!(p.panel, rgb(0xff_ff_ff), "--panel 亮");
        assert_eq!(p.border_strong, rgb(0xd1_d1_d1), "--border-strong 亮");
        assert_eq!(p.text, rgb(0x24_24_24), "--text 亮");
        assert_eq!(p.text2, rgb(0x61_61_61), "--text-2 亮");
        assert_eq!(p.text3, rgb(0x97_97_97), "--text-3 亮");
        assert_eq!(p.accent, rgb(0x0f_6c_bd), "--accent 亮");
        assert_eq!(p.row_hover, rgb(0xf5_f5_f5), "--row-hover 亮");
        assert_eq!(p.row_selected, rgb(0xf0_f0_f0), "--row-selected 亮");
        assert_eq!(p.panel_2, rgb(0xfa_fa_fa), "--panel-2 亮");
        assert_eq!(p.border, rgb(0xe0_e0_e0), "--border 亮");
    }

    /// 页脚底必须与面板底不同（设计稿 `--panel-2` ≠ `--panel`），
    /// 否则页脚与列表区糊成一片、失去底部区隔。
    #[test]
    fn footer_fill_differs_from_panel_in_both_themes() {
        for dark in [true, false] {
            let p = Palette::of(dark);
            assert_ne!(
                p.panel_2,
                p.panel,
                "{} 主题：页脚底 --panel-2 应区别于面板底 --panel",
                if dark { "暗" } else { "亮" }
            );
        }
    }

    #[test]
    fn dark_row_states_are_translucent_white() {
        // 设计稿暗色 row 态为半透明白（非纯白），避免在深底上"发灰"。
        // 注意 egui Color32 以**预乘**存储：from_rgba_unmultiplied(255,255,255,a)
        // 后 r()==a()（原色 255 时预乘分量恰等于 alpha），据此断言"白底透明"。
        let p = Palette::dark();
        assert!(p.row_hover.a() < 255 && p.row_selected.a() < 255);
        assert_eq!(p.row_hover.r(), p.row_hover.a(), "暗色 hover 为白底透明");
        assert_eq!(p.row_hover.g(), p.row_hover.a());
        assert_eq!(p.row_selected.r(), p.row_selected.a());
        assert_eq!(p.row_selected.g(), p.row_selected.a());
    }

    #[test]
    fn visuals_reflect_palette_of_theme() {
        for dark in [true, false] {
            let v = visuals(dark);
            let p = Palette::of(dark);
            assert_eq!(v.dark_mode, dark, "dark_mode 标志随主题");
            assert_eq!(v.panel_fill, p.panel, "panel_fill = --panel");
            assert_eq!(v.override_text_color, Some(p.text), "主文本 = --text");
            assert_eq!(v.weak_text_color, Some(p.text2), "weak 文本 = --text-2");
            assert_eq!(v.hyperlink_color, p.accent, "链接 = --accent");
            assert_eq!(v.error_fg_color, p.dot_err, "错误色 = --dot-err");
            assert_eq!(
                v.window_corner_radius,
                CornerRadius::same(8),
                "窗口圆角 8px（几何 note）"
            );
        }
    }

    #[test]
    fn light_and_dark_palettes_differ_in_accent() {
        // 两主题 accent 不同（Fluent 品牌色阶）；同值说明写错表了
        assert_ne!(Palette::dark().accent, Palette::light().accent);
    }

    #[test]
    fn theme_preference_maps_all_three_choices() {
        use crate::settings::ThemePref;
        // 批次 4.0：设置页三选 → egui 偏好一一对应
        assert_eq!(theme_preference(ThemePref::System), ThemePreference::System);
        assert_eq!(theme_preference(ThemePref::Light), ThemePreference::Light);
        assert_eq!(theme_preference(ThemePref::Dark), ThemePreference::Dark);
    }
}
