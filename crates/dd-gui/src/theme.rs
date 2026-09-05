//! UI 视觉主题层：设计稿 v4 token → egui `Style`/`Visuals`。
//!
//! 契约来源：`cmdpal-ui-mockups.html`（v4，Fluent 2 token 驱动）05「alias token
//! 映射表」+ 05.1 字号 ramp + CSS 组件派生值（searchbar / Tag / 键帽 / 页脚状态点）。
//! 亮暗两套视觉分别注册到 egui（`set_visuals_of(Theme::Dark|Light)`），主题偏好
//! `System` 跟随系统；无系统主题信息时 egui 回落暗色（设计稿 05 note「默认暗色」）。
//!
//! 语义：本模块是**唯一 token 源**——绘制层不写裸色值（`Visuals` 覆盖不了的
//! 场景：行 hover/选中填充、左侧 accent 指示条、Tag/键帽底、搜索框聚焦下划线、
//! 页脚状态点等，统一经 [`Palette`] 取色）。

// 本 crate 未直接依赖 `egui`（bin 层经 `eframe::egui` re-export 使用）；
// lib 层统一走同一路径，避免重复声明 egui 依赖。
use eframe::egui::{Color32, Context, CornerRadius, Stroke, Theme, ThemePreference, Visuals};

/// 面板逻辑尺寸外的行/搜索栏几何常量（与设计稿 v4 00.1/05 note 对齐：
/// D8 全 Fluent 控件高——搜索栏 40 / 行 40 / 页脚 32）。
pub const ROW_H: f32 = 40.0; // 行高（D8；44→40，一屏 9 行）
pub const ROW_RADIUS: u8 = 6; // 行圆角（CSS `.row` border-radius 6px = radius-l）
pub const ACCENT_BAR_W: f32 = 3.0; // 选中左侧指示条宽（CSS 3px）
pub const SEARCHBAR_H: f32 = 40.0; // 搜索栏高（Fluent Input large = 40，D17 filled-darker）
pub const SEARCHBAR_RADIUS: u8 = 4; // 搜索框圆角（radius-m，filled-darker 无边框）

// ── 页脚（`.panel-footer` / `.keys` / `.dot`）几何 ─────────────────────────
// v4：padding 8px 16px、字号 caption1 12/16、键帽 mini 10/14（页脚总高 32px）。
pub const FOOTER_PAD_X: f32 = 16.0; // `.panel-footer` padding: 8px 16px
pub const FOOTER_PAD_Y: f32 = 8.0;
pub const FOOTER_FONT: f32 = 12.0; // caption1（05.1 base200）
pub const FOOTER_GAP: f32 = 12.0; // `.panel-footer` gap（spacing ramp）
pub const KEYCAP_FONT: f32 = 10.0; // `.panel-footer b`（base100 mini）
pub const KEYCAP_H: f32 = 16.0; // 键帽盒高（mini 10/14 + 1px 描边 + 2px 下边 → 页脚 8+16+8=32）
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

/// 亮/暗语义色板（05 表 Fluent 2 alias token；数值逐一有 parity 单测守卫）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// 面板背景（05 `colorNeutralBackground1`）
    pub panel: Color32,
    /// 下沉层：页脚背景（05 `colorNeutralBackground2`；暗色比面板更暗）
    pub panel_2: Color32,
    /// 强边框：面板/按钮描边（05 `colorNeutralStroke1`）
    pub border_strong: Color32,
    /// 弱边框：页脚顶边、键帽/卡片/Toast 描边（05 `colorNeutralStroke2`）
    pub border: Color32,
    /// 主文本（05 `colorNeutralForeground1`）
    pub text: Color32,
    /// 次级文本：图标/Tag 文本/页脚动作（05 `colorNeutralForeground2`）
    pub text2: Color32,
    /// 三级文本：描述/分组标题/类型标签/placeholder（05 `colorNeutralForeground3`）
    pub text3: Color32,
    /// 四级文本：stub 状态点等更弱文本（05 `colorNeutralForeground4`）
    pub text4: Color32,
    /// 禁用态文本：设置页占位项（05 `colorNeutralForegroundDisabled`）
    pub text_disabled: Color32,
    /// 卡片表面：设置卡片/Toast/按钮底（05 `colorNeutralCardBackground`）
    pub card: Color32,
    /// 卡片 hover（05 `colorNeutralCardBackgroundHover`）
    pub card_hover: Color32,
    /// 强调色：选中指示条/聚焦下划线/主按钮（05 `colorBrandBackground`）
    pub accent: Color32,
    /// 成功语义色（05 `colorStatusSuccessForeground1`；暗色待核，暂用派生值）
    pub success: Color32,
    /// 危险语义色（05 `colorStatusDangerForeground1`；暗色待核，暂用派生值）
    pub danger: Color32,
    /// 行 hover 填充（05 `colorNeutralBackground1Hover`；v4 改实色）
    pub row_hover: Color32,
    /// 行按下填充（05 `colorNeutralBackground1Pressed`）
    pub row_pressed: Color32,
    /// 选中行填充（05 `colorNeutralBackground1Selected`；v4 改实色）
    pub row_selected: Color32,
    /// 搜索框填充（05 `colorNeutralBackground3`；filled-darker 外观）
    pub input_fill: Color32,
    /// Tag/键帽底（暗 = cardBackground，亮 = bg3；05 `--chip-bg`）
    pub chip_bg: Color32,
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            panel: rgb(0x29_29_29),
            panel_2: rgb(0x1f_1f_1f),
            border_strong: rgb(0x66_66_66),
            border: rgb(0x52_52_52),
            text: rgb(0xff_ff_ff),
            text2: rgb(0xd6_d6_d6),
            text3: rgb(0xad_ad_ad),
            text4: rgb(0x99_99_99),
            text_disabled: rgb(0x5c_5c_5c),
            card: rgb(0x33_33_33),
            card_hover: rgb(0x3d_3d_3d),
            accent: rgb(0x11_5e_a3), // brand[70] · colorBrandBackground（暗；#479ef5=brand[100] 属 Foreground/Compound 系）
            success: rgb(0x54_b0_54), // green[tint30] · colorStatusSuccessForeground1（暗，2026-09 按 @fluentui/tokens 核实）
            danger: rgb(0xdc_62_6d),  // cranberry[tint30] · colorStatusDangerForeground1（暗）
            row_hover: rgb(0x3d_3d_3d),
            row_pressed: rgb(0x1f_1f_1f),
            row_selected: rgb(0x38_38_38),
            input_fill: rgb(0x14_14_14),
            chip_bg: rgb(0x33_33_33),
        }
    }

    pub fn light() -> Self {
        Self {
            panel: rgb(0xff_ff_ff),
            panel_2: rgb(0xfa_fa_fa),
            border_strong: rgb(0xd1_d1_d1),
            border: rgb(0xe0_e0_e0),
            text: rgb(0x24_24_24),
            text2: rgb(0x42_42_42),
            text3: rgb(0x61_61_61),
            text4: rgb(0x70_70_70),
            text_disabled: rgb(0xbd_bd_bd),
            card: rgb(0xfa_fa_fa),
            card_hover: rgb(0xff_ff_ff),
            accent: rgb(0x0f_6c_bd),
            success: rgb(0x0e_70_0e), // green[shade10] · colorStatusSuccessForeground1（亮；#107c10 实为 primary）
            danger: rgb(0xb1_0e_1c), // cranberry[shade10] · colorStatusDangerForeground1（亮；#c50f1f 实为 primary）
            row_hover: rgb(0xf5_f5_f5),
            row_pressed: rgb(0xe0_e0_e0),
            row_selected: rgb(0xeb_eb_eb),
            input_fill: rgb(0xf5_f5_f5),
            chip_bg: rgb(0xf5_f5_f5),
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

/// Toast 阴影（05 `shadow16`：D10 分级——面板/对话框 shadow64、Toast shadow16）。
/// Fluent：key 层不透明度暗 28% / 亮 14%，offset (0,8) blur 16。
pub fn toast_shadow(dark: bool) -> eframe::egui::Shadow {
    let a: f32 = if dark { 0.28 } else { 0.14 };
    eframe::egui::Shadow {
        offset: [0, 8],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha((a * 255.0).round() as u8),
    }
}

/// 对话框阴影（05 `--shadow` = shadow64，D10：面板/对话框级）。
/// Fluent：dark `0 32px 64px rgba(0,0,0,.28)` / light `0 32px 64px rgba(0,0,0,.24)`；
/// ambient 层（0 0 2px）由面板 1px 描边替代（Windows 约定，egui Shadow 单层）。
pub fn dialog_shadow(dark: bool) -> eframe::egui::Shadow {
    let a: f32 = if dark { 0.28 } else { 0.24 };
    eframe::egui::Shadow {
        offset: [0, 32],
        blur: 64,
        spread: 0,
        color: Color32::from_black_alpha((a * 255.0).round() as u8),
    }
}

// ── 右键菜单（设计稿 10B，v4.4）────────────────────────────────────────
/// 容器 min-width（10B.1：200px）。
pub const CTX_MENU_MIN_W: f32 = 200.0;
/// 菜单项高（10B.1：32px）。
pub const CTX_ITEM_H: f32 = 32.0;
/// 菜单容器内边距（10B.1：padding 4px）。
pub const CTX_MENU_PAD: f32 = 4.0;
/// 菜单项水平内边距（10B.1：padding 0 10px）。
pub const CTX_ITEM_PAD_X: f32 = 10.0;
/// 菜单项内部间距（图标↔名称↔快捷键，CSS `.ctx-item` gap 10px）。
pub const CTX_ITEM_GAP: f32 = 10.0;
/// 菜单项图标尺寸（10B.1：glyph 16px）。
pub const CTX_ICON: f32 = 16.0;
/// 分隔线总占高（1px 线 + 上下各 4px margin）。
pub const CTX_SEP_H: f32 = 9.0;
/// 分隔线水平内缩（10B.1：左右内缩 8px）。
pub const CTX_SEP_INSET: f32 = 8.0;
/// 面板内夹紧边距（D20：菜单绝不溢出面板，越界先翻转后夹紧）。
pub const CTX_MENU_MARGIN: f32 = 8.0;
/// 指针锚点偏移（D20：右键点即菜单左上角偏移 2,2）。
pub const CTX_ANCHOR_OFFSET: f32 = 2.0;

/// 右键菜单阴影（05 `--shadow-8` = shadow8，v4.4 新增 token；官方 elevation
/// 低层 ramp：暗 28% / 亮 14%，offset (0,4) blur 8。菜单归此档——暗色 shadow8
/// 用途 = command bars / command dropdowns / tooltips）。
pub fn menu_shadow(dark: bool) -> eframe::egui::Shadow {
    let a: f32 = if dark { 0.28 } else { 0.14 };
    eframe::egui::Shadow {
        offset: [0, 4],
        blur: 8,
        spread: 0,
        color: Color32::from_black_alpha((a * 255.0).round() as u8),
    }
}

/// Dialog 遮罩（§10.1 `colorBackgroundOverlay`）：
/// 暗 blackAlpha[50]、亮 blackAlpha[40]。
pub fn overlay(dark: bool) -> Color32 {
    Color32::from_black_alpha(if dark { 128 } else { 102 })
}

/// 05 表 → egui `Visuals`：以 egui 默认视觉为基底，覆盖 token 可映射字段。
/// 组件类色板（Tag/行态）不进 `Visuals`（无对应字段），由绘制层经
/// [`Palette`] 直接取用。
///
/// `panel_transparent`（v4.7 D31）：窗口材质生效时面板背景透明——DWM 系统材质
/// 画在窗口表面之后，egui 面板必须不涂底色才可见。只透明 `panel_fill`
/// （CentralPanel 底），行/卡片/页脚等表面保持不透明层级。
pub fn visuals(dark: bool, panel_transparent: bool) -> Visuals {
    let p = Palette::of(dark);
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.dark_mode = dark;
    v.panel_fill = if panel_transparent {
        Color32::TRANSPARENT
    } else {
        p.panel
    };
    v.window_fill = p.panel;
    v.extreme_bg_color = p.input_fill; // TextEdit/滚动条底（bg3，filled-darker 同源）
    v.faint_bg_color = p.row_hover;
    v.code_bg_color = p.input_fill; // 行内 code 底（CSS 用 --panel-3 = bg3 同值）
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
    v.error_fg_color = p.danger;
    v.window_corner_radius = CornerRadius::same(8);
    v.window_stroke = Stroke::new(1.0, p.border_strong);
    v
}

/// 注册双主题 + 按用户偏好设定主题（M5 批次 4.0 起偏好来自持久化设置，
/// 不再写死跟随系统）。程序启动时调用一次；此后用户在设置页改选时由
/// `Context::set_theme` 运行时切换（无需重启）。
///
/// `panel_transparent`（v4.7 D31）：材质生效时为 true——亮暗两套 Style **都**
/// 带透明 panel_fill 注册，保证「跟随系统」在系统亮暗切换 re-resolve 后
/// 透明性不丢失。
pub fn apply(ctx: &Context, pref: ThemePreference, panel_transparent: bool) {
    ctx.set_visuals_of(Theme::Dark, visuals(true, panel_transparent));
    ctx.set_visuals_of(Theme::Light, visuals(false, panel_transparent));
    ctx.set_theme(pref);
}

/// 仅切换面板背景透明性（v4.7 D31：材质开/关与回退时调用），不动主题偏好。
/// 亮暗两套 Style 同步重注册（同 [`apply`] 的透明性口径）。
pub fn apply_panel_transparency(ctx: &Context, panel_transparent: bool) {
    ctx.set_visuals_of(Theme::Dark, visuals(true, panel_transparent));
    ctx.set_visuals_of(Theme::Light, visuals(false, panel_transparent));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 05 表 parity 守卫（v4 Fluent 2 alias token）：十六进制值必须与设计稿
    /// 一致（防手滑改色）。
    #[test]
    fn dark_palette_matches_design_tokens() {
        let p = Palette::dark();
        assert_eq!(p.panel, rgb(0x29_29_29), "--panel 暗 = bg1 grey[16]");
        assert_eq!(p.panel_2, rgb(0x1f_1f_1f), "--panel-2 暗 = bg2 grey[12]");
        assert_eq!(
            p.border_strong,
            rgb(0x66_66_66),
            "--border-strong 暗 = stroke1 grey[40]"
        );
        assert_eq!(p.border, rgb(0x52_52_52), "--border 暗 = stroke2 grey[32]");
        assert_eq!(p.text, rgb(0xff_ff_ff), "--text 暗 = fg1");
        assert_eq!(p.text2, rgb(0xd6_d6_d6), "--text-2 暗 = fg2 grey[84]");
        assert_eq!(p.text3, rgb(0xad_ad_ad), "--text-3 暗 = fg3 grey[68]");
        assert_eq!(p.text4, rgb(0x99_99_99), "--text-4 暗 = fg4 grey[60]");
        assert_eq!(
            p.text_disabled,
            rgb(0x5c_5c_5c),
            "--text-disabled 暗 = fgDisabled grey[36]"
        );
        assert_eq!(
            p.card,
            rgb(0x33_33_33),
            "--card 暗 = cardBackground grey[20]"
        );
        assert_eq!(
            p.accent,
            rgb(0x11_5e_a3),
            "--accent 暗 = brand[70]（colorBrandBackground）"
        );
        assert_eq!(p.success, rgb(0x54_b0_54), "--success 暗 = green[tint30]");
        assert_eq!(p.danger, rgb(0xdc_62_6d), "--danger 暗 = cranberry[tint30]");
        assert_eq!(
            p.row_hover,
            rgb(0x3d_3d_3d),
            "--row-hover 暗 = bg1Hover grey[24]"
        );
        assert_eq!(
            p.row_selected,
            rgb(0x38_38_38),
            "--row-selected 暗 = bg1Selected grey[22]"
        );
        assert_eq!(
            p.row_pressed,
            rgb(0x1f_1f_1f),
            "--row-pressed 暗 = bg1Pressed grey[12]"
        );
        assert_eq!(
            p.input_fill,
            rgb(0x14_14_14),
            "--input-fill 暗 = bg3 grey[8]"
        );
        assert_eq!(p.chip_bg, rgb(0x33_33_33), "--chip-bg 暗 = card grey[20]");
    }

    #[test]
    fn light_palette_matches_design_tokens() {
        let p = Palette::light();
        assert_eq!(p.panel, rgb(0xff_ff_ff), "--panel 亮 = bg1 white");
        assert_eq!(p.panel_2, rgb(0xfa_fa_fa), "--panel-2 亮 = bg2 grey[98]");
        assert_eq!(
            p.border_strong,
            rgb(0xd1_d1_d1),
            "--border-strong 亮 = stroke1 grey[82]"
        );
        assert_eq!(p.border, rgb(0xe0_e0_e0), "--border 亮 = stroke2 grey[88]");
        assert_eq!(p.text, rgb(0x24_24_24), "--text 亮 = fg1 grey[14]");
        assert_eq!(p.text2, rgb(0x42_42_42), "--text-2 亮 = fg2 grey[26]");
        assert_eq!(p.text3, rgb(0x61_61_61), "--text-3 亮 = fg3 grey[38]");
        assert_eq!(p.text4, rgb(0x70_70_70), "--text-4 亮 = fg4 grey[44]");
        assert_eq!(
            p.text_disabled,
            rgb(0xbd_bd_bd),
            "--text-disabled 亮 = fgDisabled grey[74]"
        );
        assert_eq!(
            p.card,
            rgb(0xfa_fa_fa),
            "--card 亮 = cardBackground grey[98]"
        );
        assert_eq!(p.accent, rgb(0x0f_6c_bd), "--accent 亮 = brand[80]");
        assert_eq!(p.success, rgb(0x0e_70_0e), "--success 亮 = green[shade10]");
        assert_eq!(
            p.danger,
            rgb(0xb1_0e_1c),
            "--danger 亮 = cranberry[shade10]"
        );
        assert_eq!(
            p.row_hover,
            rgb(0xf5_f5_f5),
            "--row-hover 亮 = bg1Hover grey[96]"
        );
        assert_eq!(
            p.row_selected,
            rgb(0xeb_eb_eb),
            "--row-selected 亮 = bg1Selected grey[92]"
        );
        assert_eq!(
            p.row_pressed,
            rgb(0xe0_e0_e0),
            "--row-pressed 亮 = bg1Pressed grey[88]"
        );
        assert_eq!(
            p.input_fill,
            rgb(0xf5_f5_f5),
            "--input-fill 亮 = bg3 grey[96]"
        );
        assert_eq!(p.chip_bg, rgb(0xf5_f5_f5), "--chip-bg 亮 = bg3 grey[96]");
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

    /// v4 D11：暗色行态改为 Fluent 实色（bg1Hover/bg1Selected），
    /// 不再是 v2/v3 的半透明白叠加（实色在滚动内容上不会透出下层文字）。
    #[test]
    fn dark_row_states_are_solid() {
        let p = Palette::dark();
        assert_eq!(p.row_hover.a(), 255, "暗色 hover 为实色");
        assert_eq!(p.row_selected.a(), 255, "暗色 selected 为实色");
        assert_ne!(p.row_hover, p.row_selected, "hover 与 selected 可区分");
    }

    /// 卡片表面（Toast/设置卡/按钮底）与面板底可区分（暗色 card grey[20]
    /// 比面板 grey[16] 亮一档；亮色 card grey[98] 比面板 white 灰一档）。
    #[test]
    fn card_differs_from_panel_in_both_themes() {
        for dark in [true, false] {
            let p = Palette::of(dark);
            assert_ne!(
                p.card,
                p.panel,
                "{} 主题：card 应区别于面板底",
                if dark { "暗" } else { "亮" }
            );
        }
    }

    #[test]
    fn visuals_reflect_palette_of_theme() {
        for dark in [true, false] {
            let v = visuals(dark, false);
            let p = Palette::of(dark);
            assert_eq!(v.dark_mode, dark, "dark_mode 标志随主题");
            assert_eq!(v.panel_fill, p.panel, "panel_fill = --panel");
            assert_eq!(v.override_text_color, Some(p.text), "主文本 = --text");
            assert_eq!(v.weak_text_color, Some(p.text2), "weak 文本 = --text-2");
            assert_eq!(v.hyperlink_color, p.accent, "链接 = --accent");
            assert_eq!(v.error_fg_color, p.danger, "错误色 = --danger");
            assert_eq!(
                v.window_corner_radius,
                CornerRadius::same(8),
                "窗口圆角 8px（几何 note）"
            );
        }
    }

    /// v4.7 D31：材质生效时 panel_fill 透明（其余字段不变），亮暗两套一致。
    #[test]
    fn panel_transparent_visuals_only_affect_panel_fill() {
        for dark in [true, false] {
            let opaque = visuals(dark, false);
            let transparent = visuals(dark, true);
            assert_eq!(transparent.panel_fill, Color32::TRANSPARENT);
            assert_ne!(opaque.panel_fill, Color32::TRANSPARENT);
            // 其余表面不透明层级不变（行/卡片经 Palette 取用，不在 Visuals 内）
            assert_eq!(transparent.extreme_bg_color, opaque.extreme_bg_color);
            assert_eq!(transparent.window_fill, opaque.window_fill);
            assert_eq!(transparent.dark_mode, opaque.dark_mode);
        }
    }

    #[test]
    fn light_and_dark_palettes_differ_in_accent() {
        // 两主题 accent 不同（Fluent 品牌色阶）；同值说明写错表了
        assert_ne!(Palette::dark().accent, Palette::light().accent);
    }

    /// D10 阴影分级：Toast shadow16 的 key 层不透明度暗 28% / 亮 14%。
    #[test]
    fn toast_shadow_follows_elevation_opacities() {
        assert_eq!(
            toast_shadow(true).color.a(),
            (0.28f32 * 255.0).round() as u8
        );
        assert_eq!(
            toast_shadow(false).color.a(),
            (0.14f32 * 255.0).round() as u8
        );
    }

    /// C 组批次 C3（§10.1）：遮罩 blackAlpha[50]（暗）/ [40]（亮）。
    #[test]
    fn overlay_matches_fluent_black_alpha() {
        assert_eq!(
            overlay(true),
            Color32::from_black_alpha(128),
            "暗 = blackAlpha[50]"
        );
        assert_eq!(
            overlay(false),
            Color32::from_black_alpha(102),
            "亮 = blackAlpha[40]"
        );
    }

    /// C 组批次 C3（§10.1）：对话框 shadow64——offset (0,32) blur 64，
    /// key 层不透明度暗 28% / 亮 24%。
    #[test]
    fn dialog_shadow_follows_elevation_opacities() {
        let s = dialog_shadow(true);
        assert_eq!(s.offset, [0, 32]);
        assert_eq!(s.blur, 64);
        assert_eq!(s.color.a(), (0.28f32 * 255.0).round() as u8);
        assert_eq!(
            dialog_shadow(false).color.a(),
            (0.24f32 * 255.0).round() as u8
        );
    }

    /// v4.4（10B.1）：右键菜单 shadow8——offset (0,4) blur 8，key 层
    /// 不透明度暗 28% / 亮 14%（与官方 elevation 低层 ramp 一致）。
    #[test]
    fn menu_shadow_follows_elevation_opacities() {
        let s = menu_shadow(true);
        assert_eq!(s.offset, [0, 4]);
        assert_eq!(s.blur, 8);
        assert_eq!(s.color.a(), (0.28f32 * 255.0).round() as u8);
        assert_eq!(
            menu_shadow(false).color.a(),
            (0.14f32 * 255.0).round() as u8
        );
    }

    /// v4.4（10B.1）几何常量：min-width 200 / 项高 32 / 容器 padding 4 /
    /// 分隔线 1+4+4=9 / 面板内夹紧边距 8（D20）。
    #[test]
    fn context_menu_geometry_matches_design_10b() {
        assert_eq!(CTX_MENU_MIN_W, 200.0, "容器 min-width 200");
        assert_eq!(CTX_ITEM_H, 32.0, "菜单项高 32");
        assert_eq!(CTX_MENU_PAD, 4.0, "容器 padding 4");
        assert_eq!(CTX_ITEM_PAD_X, 10.0, "菜单项 padding 0 10");
        assert_eq!(CTX_ICON, 16.0, "图标 16px");
        assert_eq!(CTX_SEP_H, 9.0, "分隔线 = 1px + 上下 4px margin");
        assert_eq!(CTX_SEP_INSET, 8.0, "分隔线水平内缩 8");
        assert_eq!(CTX_MENU_MARGIN, 8.0, "面板内夹紧边距 8（D20）");
        assert_eq!(CTX_ANCHOR_OFFSET, 2.0, "指针锚点偏移 2,2（D20）");
    }

    /// 几何常量与设计稿 v4 一致（D8：搜索栏 40 / 行 40 / 页脚 32）。
    #[test]
    fn geometry_matches_design_v4() {
        assert_eq!(ROW_H, 40.0, "行高 40（D8）");
        assert_eq!(SEARCHBAR_H, 40.0, "搜索栏高 40（Fluent Input large）");
        assert_eq!(
            FOOTER_PAD_Y * 2.0 + KEYCAP_H,
            32.0,
            "页脚总高 = 8 + 16 + 8 = 32（D8）"
        );
        assert_eq!(FOOTER_FONT, 12.0, "页脚字号 = caption1 base200");
        assert_eq!(KEYCAP_FONT, 10.0, "键帽字号 = base100 mini");
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
