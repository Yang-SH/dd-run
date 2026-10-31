//! 批次 C（refactor-structure-2026-10-21.md §4.3）：亮/暗语义色板与阴影 / 遮罩。
//! 自 `theme.rs` 原块（132–329 行）逐字节搬入、零逻辑改动；
//! 外部路径经 `theme.rs` 的 `pub use` 桥保持不变。

use eframe::egui::Color32;

use super::visuals::{glass, rgb};

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
    /// 卡片表面：设置卡片/Toast/按钮底（05 `colorNeutralCardBackground`；
    /// 暗 grey[20] #333333 / 亮 grey[98] #fafafa，Fluent 2 卡面比画布
    /// 亮一档（暗）或灰一档（亮）以与 `--panel` 区分）
    pub card: Color32,
    /// 卡片 hover（05 `colorNeutralCardBackgroundHover`：卡面同族内偏移一档、
    /// 方向同 hover 语义——暗比卡面更亮 grey[24]、亮比卡面更灰 grey[96]，
    /// 非纯白/纯黑跳变；当前为预留 token，绘制层按需引用）
    pub card_hover: Color32,
    /// 强调色：选中指示条/聚焦下划线/主按钮（05 `colorBrandBackground`）
    pub accent: Color32,
    /// 强调色 hover：功能态开关开启态悬停底（B7，v4 设计稿 CSS `--accent-hover`；
    /// 暗 brand[80] #2886de / 亮 brand[70] #115ea3——与 colorBrandBackgroundHover
    /// 同阶，亮暗取值互换同 v4.13 口径）
    pub accent_hover: Color32,
    /// 小控件悬停底（B7 真机修订 2026-09-12）：齿轮/返回键/导航项/设置行等
    /// 小面积控件的 hover 填充。亮色主题 row_hover(#f5f5f5) 与面板/卡片底
    /// 同灰阶不可辨（真机反馈"没有悬浮效果"的根因），取 grey[94] #f0f0f0
    /// ——介于 bg1Hover(#f5f5f5) 与 bg1Selected(#ebebeb) 之间，与选中态可区分；
    /// 暗色沿用 row_hover #3d3d3d（对 #292929 明显）。
    pub control_hover: Color32,
    /// 小面积强调色（B2，v5.2 方案）：3px 选中指示条 / 2px 聚焦下划线 /
    /// Spinner 弧 / 导航指示条 / radio-card 选中边框等**线状小元素**专用。
    /// Fluent colorBrandStroke1 语义：暗 = brand[100] #479ef5（对 #292929
    /// 对比度 5.2:1；brand[70] 仅 2.2:1 几乎不可辨）、亮 = brand[80] #0f6cbd
    /// （= 原值不变）。大面积 accent 底色（主按钮等）仍用 [`Self::accent`]。
    pub accent_stroke: Color32,
    /// 成功语义色（05 `colorStatusSuccessForeground1`；暗色待核，暂用派生值）
    pub success: Color32,
    /// 危险语义色（05 `colorStatusDangerForeground1`；暗色待核，暂用派生值）
    pub danger: Color32,
    /// 行 hover 填充（05 `colorNeutralBackground1Hover`；v4 改实色）
    pub row_hover: Color32,
    /// 行 hover 玻璃填充：`row_hover` alpha=80，专供结果列表（亚克力下通透）。
    /// 其他用 `row_hover` 的场景（卡片 hover/设置项/icon 按钮/确认按钮/菜单）维持实色不变。
    pub row_hover_glass: Color32,
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
            accent_hover: rgb(0x28_86_de), // brand[80] · colorBrandBackgroundHover（暗，v4 CSS --accent-hover）
            accent_stroke: rgb(0x47_9e_f5), // brand[100] · colorBrandStroke1（暗，2026-09 按 @fluentui/tokens 口径）
            control_hover: rgb(0x3d_3d_3d), // = row_hover（暗色对 #292929 足够可辨）
            success: rgb(0x54_b0_54), // green[tint30] · colorStatusSuccessForeground1（暗，2026-09 按 @fluentui/tokens 核实）
            danger: rgb(0xdc_62_6d),  // cranberry[tint30] · colorStatusDangerForeground1（暗）
            row_hover: rgb(0x3d_3d_3d),
            row_hover_glass: glass(0x3d_3d_3d, 80), // 玻璃：等色 alpha=80
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
            card_hover: rgb(0xf5_f5_f5), // 亮卡面 grey[98] → hover grey[96]（Fluent 卡面 hover 同族偏移）
            accent: rgb(0x0f_6c_bd),
            accent_hover: rgb(0x11_5e_a3), // brand[70] · colorBrandBackgroundHover（亮，v4 CSS --accent-hover）
            accent_stroke: rgb(0x0f_6c_bd), // brand[80] · colorBrandStroke1（亮 = 原 --accent 值不变）
            control_hover: rgb(0xf0_f0_f0), // grey[94]（亮：介于 bg1Hover #f5f5f5 与 bg1Selected #ebebeb 之间）
            success: rgb(0x0e_70_0e), // green[shade10] · colorStatusSuccessForeground1（亮；#107c10 实为 primary）
            danger: rgb(0xb1_0e_1c), // cranberry[shade10] · colorStatusDangerForeground1（亮；#c50f1f 实为 primary）
            row_hover: rgb(0xf5_f5_f5),
            row_hover_glass: glass(0xf5_f5_f5, 80), // 玻璃：等色 alpha=80
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
