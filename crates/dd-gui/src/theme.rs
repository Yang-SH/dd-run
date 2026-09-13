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

// ── F1 列表排印 token（icons-typography-plan.md F2/F1；初值 = 091c765
// 现状硬编码值，纯收拢零观感变化；F2 密度档经 ListMetrics 引用）──────────
/// 列表行名字号（B1 semibold 族，设计稿 `.name` 14）。
pub const LIST_TITLE_PT: f32 = 14.0;
/// 列表类型标签字号（caption1 12，设计稿 05.1）。
pub const LIST_CAT_PT: f32 = 12.0;
/// 图标列与标题间距（CSS `.row` gap 12px）。
pub const LIST_ICON_GAP: f32 = 12.0;
/// 列表图标格边长（设计稿 v2 20 → 真机反馈 2026-09-12 放大到 24）。
pub const LIST_ICON_CELL: f32 = 24.0;
/// glyph 图标字号（随图标格 24 的 20/24 比例）。
pub const LIST_GLYPH_PT: f32 = 20.0;

// ── F2 列表密度（icons-typography-plan.md，参考 DeskBox「图标/文字大小
// 可调」）────────────────────────────────────────────────────────────────
/// 列表密度档 → 行几何/排印指标。一档联动五值（不拆孤立设置项）；
/// **标准档硬性等于上方各常量**（parity 单测守卫），紧凑/宽松档按 ±1 字号、
/// ±4px 行高的同节奏派生。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListMetrics {
    /// 行高（标准档 = [`ROW_H`]）。
    pub row_h: f32,
    /// 行名字号（标准档 = [`LIST_TITLE_PT`]）。
    pub title_pt: f32,
    /// 类型标签字号（标准档 = [`LIST_CAT_PT`]）。
    pub cat_pt: f32,
    /// 图标格边长（标准档 = [`LIST_ICON_CELL`]）。
    pub icon_cell: f32,
    /// glyph 图标字号（标准档 = [`LIST_GLYPH_PT`]）。
    pub glyph_pt: f32,
}

impl ListMetrics {
    pub const fn of(density: crate::settings::ListDensity) -> Self {
        use crate::settings::ListDensity;
        match density {
            ListDensity::Compact => Self {
                row_h: 36.0,
                title_pt: 13.0,
                cat_pt: 11.0,
                icon_cell: 20.0,
                glyph_pt: 16.0,
            },
            ListDensity::Standard => Self {
                row_h: ROW_H,
                title_pt: LIST_TITLE_PT,
                cat_pt: LIST_CAT_PT,
                icon_cell: LIST_ICON_CELL,
                glyph_pt: LIST_GLYPH_PT,
            },
            ListDensity::Relaxed => Self {
                row_h: 44.0,
                title_pt: 15.0,
                cat_pt: 13.0,
                icon_cell: 28.0,
                glyph_pt: 24.0,
            },
        }
    }
}

// ── 页脚（`.panel-footer` / `.keys` / `.dot`）几何 ─────────────────────────
// v4：padding 8px 16px、字号 caption1 12/16、键帽 mini 10/14（页脚总高 32px）。
pub const FOOTER_PAD_X: f32 = 16.0; // `.panel-footer` padding: 8px 16px
pub const FOOTER_PAD_Y: f32 = 8.0;
pub const FOOTER_FONT: f32 = 12.0; // caption1（05.1 base200）
pub const FOOTER_GAP: f32 = 16.0; // `.panel-footer` gap（v4.10 D35：12→16，对齐截图组距）
pub const KEYCAP_FONT: f32 = 12.0; // v4.10 D35：proportional 12（原 mono mini 10）
pub const KEYCAP_H: f32 = 20.0; // 键帽盒高（12/18 文本行 + 上下描边 → 页脚 8+20+8=36）
pub const KEYCAP_PAD_X: f32 = 8.0; // v4.10 D35：6→8（对齐截图）
pub const KEYCAP_GAP: f32 = 4.0; // 同组键帽之间的间隙（v4.10 D35：2→4）
/// 键帽圆角（v4.10 D35：radius-md 4 → 5，对齐截图）。
pub const KEYCAP_RADIUS: u8 = 5;
/// 说明文本与键帽间距（v4.10 D35：4→6；组内顺序 = 说明在前、键帽在后）。
pub const KEYCAP_DESC_GAP: f32 = 6.0;
pub const DOT_SIZE: f32 = 6.0; // `.dot` 6×6
pub const DOT_GAP: f32 = 5.0; // `.dot` margin-right 5px

// ── B1 语义字重：已撤销（2026-09-13 真机反馈「不用加粗字体」）────────────
/// 语义字重族名（**保留注册**：platform.rs 将其映射到 regular Proportional
/// 链，防历史 FontId 引用未知族；seguisb/msyhbd 加粗字体文件已停载）。
pub const SEMIBOLD_FAMILY: &str = "semibold";

/// 标题字重 `FontId`（原 B1 semibold）：分组标题 / 行名 / 空态标题 / 设置卡头
/// 等。2026-09-13 真机反馈「不用加粗字体」——**回落 regular 字形**；函数与
/// 全部调用点保留（语义层不撤），将来若恢复字重只改此一处。
pub fn semibold(size: f32) -> eframe::egui::FontId {
    eframe::egui::FontId::proportional(size)
}

/// 标题富文本：[`semibold`] + 0.05em 额外字距。字距为排印规格保留（引入时
/// 为缓解雅黑 Bold 小字号的「压缩」观感；字重撤销后 CJK 无内建 tracking，
/// 补呼吸感的作用不变）。painter 直绘不走此函数（painter.text 不支持
/// 字距，见 settings_view 页标题的 LayoutJob 用法）。
pub fn semibold_title(text: impl Into<String>, size: f32) -> eframe::egui::RichText {
    eframe::egui::RichText::new(text)
        .font(semibold(size))
        .extra_letter_spacing(size * 0.05)
}

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

/// 浓淡层 alpha 上限（P2 v2 直控式 → v5 两主题统一，2026-09-13）。语义 =
/// 「面板底不透明度直控」：`alpha = cap × pct/100`，滑杆覆盖 0→cap 全带。
/// 参考 DeskBox「不透明度/材质强度」能力的文档化落地：DWM SystemBackdrop
/// 本身无透明度控制，由 egui 侧浓淡层实现同观感。
///
/// **默认 40% 档**（cap × 0.4）= 真机对比 DeskBox 调定的推荐观感：
/// 云母 0.30 / 亚克力 0.40（v5 起两主题同档）。v5 统一理由：v3 把浓淡基色
/// 换成「面板色 × 系统强调色」后，M1「厚白浓淡 = 白漆」的前提已不存在——
/// 带色基色加厚只会增强材质色相、不会洗白，亮色原独立低档（0.55/0.70，
/// 默认 0.22/0.28）反而压低了材质存在感（真机对比 DeskBox 仍差一档），
/// 故按材质（而非主题）定档。亚克力拉满（1.0）= 面板全实色；云母拉满
/// （0.75）仍保留 25% 采色语义。
///
/// 修订沿革：初版 `基准 × (0.25 + 0.75×pct/100)` 在云母上有效带仅 0.075–0.30
/// （暗）/0.025–0.10（亮），叠在近乎不透明的 DWM 云母上感知极弱——v2 改直控
/// 全带（真机反馈「调透明度效果不明显」）；v3 浓淡基色带系统强调色（真机反馈
/// 「与 DeskBox 显示效果不一致」）；v4 亮色档增强（真机对比「还差一点」）；
/// v5 两主题统一 cap + 行 hover 材质适配（真机反馈「还需优化 / 悬浮样式按
/// 材质调」）。P2 未发版（同一 Unreleased 批次），历次重定义无迁移。
pub const TINT_CAP_MICA: f32 = 0.75;
pub const TINT_CAP_ACRYLIC: f32 = 1.0;

/// 材质激活时面板底的浓淡层 alpha **上限**（P2 v2 直控式，2026-09-13）；
/// `None` = 材质未生效（回退不透明面板底）。v5 起上限按材质定档、两主题一致
/// （`dark` 参数保留以稳定调用点签名）。
pub fn panel_tint_cap(dark: bool, backdrop: crate::settings::Backdrop) -> Option<f32> {
    use crate::settings::Backdrop;
    match backdrop {
        Backdrop::None => None,
        Backdrop::Mica => Some(TINT_CAP_MICA),
        Backdrop::Acrylic => Some(TINT_CAP_ACRYLIC),
    }
}

/// 材质不透明度滑杆（P2 v2 直控式，2026-09-13）→ 实际浓淡 alpha：
/// `cap(材质) × pct/100`——0% = 纯材质（无浓淡层），100% = 面板最实（亚克力
/// 拉满即全实色）。默认 40%（`Settings::default().material_opacity`）= 真机
/// 对比 DeskBox 调定的推荐观感档。pct 超界按 100 处理（u8 入参，序列化层已
/// clamp，此处兜底）；`None` = 材质未生效（回退不透明面板底）。
pub fn panel_tint_with_opacity(
    dark: bool,
    backdrop: crate::settings::Backdrop,
    opacity_pct: u8,
) -> Option<f32> {
    panel_tint_cap(dark, backdrop).map(|cap| cap * f32::from(opacity_pct.min(100)) / 100.0)
}

/// 结果行填充对（P2 v5 材质适配，2026-09-13 真机反馈）：
/// - **hover 材质分档**：同一块 `row_hover_glass`（#f5f5f5/#3d3d3d @ 31%）在
///   云母下面板近白对比不足（「不明显」）、在亚克力动态模糊上又形成亮块
///   （「太突兀」）——云母**加权**（亮 = 黑 7% / 暗 = 白 7.8% 玻璃）、亚克力
///   **减重**（亮 = 黑 3.5% / 暗 = 白 3.9%）；
/// - **selected 同走玻璃**（`selected` 比 `hover` 重一档）：材质激活时若保持
///   不透明 `row_selected` 实色，鼠标扫动时「玻璃 → 不透明块 → 消失」的两段
///   跳变在半透明面板上即「应用栏目闪烁」——selected 玻璃化后扫动为半透明层
///   平滑过渡（accent 竖条仍由 `row.rs` 绘制，选中辨识度不变）；
/// - **回退**（无材质 / 未生效）：`row_hover` / `row_selected` 实色——既有观
///   感不变。
/// 供 `row.rs` 结果行专用；卡片/设置项等其余 hover 维持实色不变。
#[derive(Debug, Clone, Copy)]
pub struct RowFills {
    /// 非选中行 hover 玻璃。
    pub hover: Color32,
    /// 选中行填充（材质档 = 玻璃；回退 = 实色 `row_selected`）。
    pub selected: Color32,
}

pub fn row_fills(dark: bool, backdrop: crate::settings::Backdrop, backdrop_active: bool) -> RowFills {
    use crate::settings::Backdrop;
    let p = Palette::of(dark);
    if !backdrop_active || backdrop == Backdrop::None {
        return RowFills { hover: p.row_hover, selected: p.row_selected };
    }
    match (dark, backdrop) {
        (true, Backdrop::Mica) => RowFills {
            hover: Color32::from_rgba_unmultiplied(255, 255, 255, 20),
            selected: Color32::from_rgba_unmultiplied(255, 255, 255, 26),
        },
        (true, Backdrop::Acrylic) => RowFills {
            hover: Color32::from_rgba_unmultiplied(255, 255, 255, 10),
            selected: Color32::from_rgba_unmultiplied(255, 255, 255, 15),
        },
        (false, Backdrop::Mica) => RowFills {
            hover: Color32::from_rgba_unmultiplied(0, 0, 0, 18),
            selected: Color32::from_rgba_unmultiplied(0, 0, 0, 26),
        },
        (false, Backdrop::Acrylic) => RowFills {
            hover: Color32::from_rgba_unmultiplied(0, 0, 0, 9),
            selected: Color32::from_rgba_unmultiplied(0, 0, 0, 15),
        },
        (_, Backdrop::None) => RowFills { hover: p.row_hover, selected: p.row_selected },
    }
}

/// 设置卡片填充（2026-09-13 真机反馈「材质覆盖设置页」）：材质生效时卡面
/// **玻璃化**（card 色降 alpha，材质从卡片本身透出——WinUI Card 半透明层
/// 语义）；不透明回退路径仍用实色 `Palette::card`。亮色卡比暗色厚一档
/// （亮材质上太薄会与背景糊在一起，靠 1px 描边区分层级）。注意 egui
/// `Color32` 为预乘存储：`from_rgba_unmultiplied` 会按 alpha 缩 RGB。
pub const CARD_GLASS_ALPHA_DARK: u8 = 190;
pub const CARD_GLASS_ALPHA_LIGHT: u8 = 210;

pub fn card_fill(dark: bool, glass_card: bool) -> Color32 {
    let p = Palette::of(dark);
    if !glass_card {
        return p.card;
    }
    let a: u8 = if dark { CARD_GLASS_ALPHA_DARK } else { CARD_GLASS_ALPHA_LIGHT };
    Color32::from_rgba_unmultiplied(p.card.r(), p.card.g(), p.card.b(), a)
}

/// 浓淡层基色（P2 v3 + v4，2026-09-13 显示对齐）：**面板色 × 系统强调色
/// 混合**——参考 DeskBox `BuildContentTintColor` 配方（暗基底 + 8% 强调色、
/// 亮基底 + 强调色；基底用本项目 Fluent token #292929/#FFFFFF）。真机对比
/// 反馈：纯中性白浓淡把 DWM 云母「洗白」成无材质感的白平面（DeskBox 同桌面
/// 下呈灰蓝材质面）——tint 带上系统强调色后，浓淡层在任何 alpha 下都有材质
/// 色相。v4 亮色混合比 0.16 → 0.30：DeskBox 另有亮度层压深表面（DWM
/// SystemBackdrop 无对应旋钮），用更强的色相混合补偿，混合比按真机对比调定。
/// 取不到系统强调色（非 Windows / DwmGetColorizationColor 失败）→ 纯面板色
/// （中性回退，与既往一致）。每次 visuals 重建时取色（调用点均为事件驱动，
/// DwmGetColorizationColor 为轻量 API，滑杆拖动期双调用/帧可忽略）。
pub fn tint_color(dark: bool) -> Color32 {
    let p = Palette::of(dark);
    let Some(accent) = crate::platform::system_accent_color() else {
        return p.panel;
    };
    let mix = if dark { 0.08_f32 } else { 0.30_f32 };
    let lerp = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * mix).round() as u8;
    Color32::from_rgb(
        lerp(p.panel.r(), accent.r()),
        lerp(p.panel.g(), accent.g()),
        lerp(p.panel.b(), accent.b()),
    )
}

/// 05 表 → egui `Visuals`：以 egui 默认视觉为基底，覆盖 token 可映射字段。
/// 组件类色板（Tag/行态）不进 `Visuals`（无对应字段），由绘制层经
/// [`Palette`] 直接取用。
///
/// `panel_tint`（v4.7 D31 + M1 2026-09-13 + P2 v3）：窗口材质生效时面板底为
/// **半透明浓淡层**（[`tint_color`] 基色 × alpha——面板色 × 系统强调色混合，
/// DWM 系统材质从底下透出），同时保证面板轮廓与内容对比度。`None` = 材质未
/// 生效（回退实色面板底）。只调 `panel_fill`，行/卡片/页脚等表面保持不透明
/// 层级（行 hover 的 `row_hover_glass` 同构先例）。
pub fn visuals(dark: bool, panel_tint: Option<f32>) -> Visuals {
    let p = Palette::of(dark);
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.dark_mode = dark;
    v.panel_fill = match panel_tint {
        None => p.panel,
        // 只缩 alpha、不动 RGB（from_rgba_unmultiplied；不用 gamma_multiply——
        // 它会连带压暗 RGB，浓淡层的语义是「半透明面板色」而非「调暗面板色」）。
        Some(a) => {
            let base = tint_color(dark);
            Color32::from_rgba_unmultiplied(
                base.r(),
                base.g(),
                base.b(),
                (a.clamp(0.0, 1.0) * 255.0).round() as u8,
            )
        }
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
/// `panel_tint`（v4.7 D31 + M1）：材质生效时传 `Some(alpha)`——亮暗两套
/// Style **都**带浓淡层注册，保证「跟随系统」在系统亮暗切换 re-resolve 后
/// 一致性不丢失；`None` = 不透明面板（启动时材质成败未知，先按实色注册）。
///
/// B3（v5.2 方案）：floating 滚动条 Fluent 化调参（两主题一致）——静默时
/// 细条 4px 低透明，悬停展开到 8px 便于拖拽。字段口径按 egui 0.36.1
/// `ScrollStyle`（style.rs:494）：把手颜色随 `foreground_color`（floating
/// 默认高对比），不做任意取色。
pub fn apply(ctx: &Context, pref: ThemePreference, panel_tint: Option<f32>) {
    ctx.set_visuals_of(Theme::Dark, visuals(true, panel_tint));
    ctx.set_visuals_of(Theme::Light, visuals(false, panel_tint));
    ctx.all_styles_mut(|style| {
        let scroll = &mut style.spacing.scroll;
        // egui floating 默认：bar_width 10 / floating_width 2 / dormant handle 0.0
        // ——静默完全隐形、悬停展开偏宽。Fluent 口径：静默可见的 4px 细条
        // （handle 0.35），hover 展开到 8px（bar_width）并提亮到 0.6。
        scroll.bar_width = 8.0;
        scroll.floating_width = 4.0;
        scroll.dormant_handle_opacity = 0.35;
        scroll.active_handle_opacity = 0.6;
        scroll.interact_handle_opacity = 1.0;
    });
    ctx.set_theme(pref);
}

/// 仅切换面板底浓淡（v4.7 D31 + M1 2026-09-13：材质开/关、云母↔亚克力互切
/// 与回退时调用），不动主题偏好。亮暗两套 Style 同步重注册（同 [`apply`]
/// 的口径）。
pub fn apply_panel_tint(ctx: &Context, panel_tint: Option<f32>) {
    ctx.set_visuals_of(Theme::Dark, visuals(true, panel_tint));
    ctx.set_visuals_of(Theme::Light, visuals(false, panel_tint));
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

/// 构造带 alpha 的 `Color32`（24 位 RGB + 自定义 alpha，未预乘）。
/// 用于「半透叠加」场景（如亚克力下的 hover 玻璃色、搜索框底色、下边框）。
fn glass(v: u32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(
        ((v >> 16) & 0xff) as u8,
        ((v >> 8) & 0xff) as u8,
        (v & 0xff) as u8,
        a,
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
            p.card_hover,
            rgb(0x3d_3d_3d),
            "--card-hover 暗 = cardHover grey[24]（卡面 grey[20] 同族更亮一档）"
        );
        assert_eq!(
            p.accent,
            rgb(0x11_5e_a3),
            "--accent 暗 = brand[70]（colorBrandBackground）"
        );
        assert_eq!(
            p.accent_hover,
            rgb(0x28_86_de),
            "--accent-hover 暗 = brand[80]（colorBrandBackgroundHover）"
        );
        assert_eq!(
            p.accent_stroke,
            rgb(0x47_9e_f5),
            "--accent-stroke 暗 = brand[100]（colorBrandStroke1）"
        );
        assert_eq!(
            p.control_hover,
            rgb(0x3d_3d_3d),
            "--control-hover 暗 = row_hover（bg1Hover）"
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
        assert_eq!(
            p.card_hover,
            rgb(0xf5_f5_f5),
            "--card-hover 亮 = cardHover grey[96]（卡面 grey[98] 同族更灰一档）"
        );
        assert_eq!(p.accent, rgb(0x0f_6c_bd), "--accent 亮 = brand[80]");
        assert_eq!(
            p.accent_hover,
            rgb(0x11_5e_a3),
            "--accent-hover 亮 = brand[70]（colorBrandBackgroundHover）"
        );
        assert_eq!(
            p.accent_stroke,
            rgb(0x0f_6c_bd),
            "--accent-stroke 亮 = brand[80]（colorBrandStroke1，= 原 --accent 值）"
        );
        assert_eq!(
            p.control_hover,
            rgb(0xf0_f0_f0),
            "--control-hover 亮 = grey[94]（可辨且区别于 selected #ebebeb）"
        );
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

    /// 行 hover 玻璃色（亚克力下结果列表用）：保留 row_hover 色相，仅 alpha=80，
    /// RGB 与 row_hover 一致仅 alpha 不同（其他用 row_hover 的场景不受牵连）。
    #[test]
    fn row_hover_glass_is_translucent_and_keeps_hue() {
        for dark in [true, false] {
            let p = Palette::of(dark);
            // 期望值直接用 from_rgba_unmultiplied 构造（与 glass helper 同实现路径），
            // 全字段相等比较避免 Color32 内部预乘语义差异
            let (v, name) = if dark {
                (0x3d_3d_3d, "暗")
            } else {
                (0xf5_f5_f5, "亮")
            };
            let expected = Color32::from_rgba_unmultiplied(
                ((v >> 16) & 0xff) as u8,
                ((v >> 8) & 0xff) as u8,
                (v & 0xff) as u8,
                80,
            );
            assert_eq!(
                p.row_hover_glass,
                expected,
                "{} 主题：glass = rgb({:02x},{:02x},{:02x},80)",
                name,
                (v >> 16) & 0xff,
                (v >> 8) & 0xff,
                v & 0xff
            );
            assert_eq!(
                p.row_hover,
                rgb(v),
                "{} 主题：row_hover = rgb({:02x},{:02x},{:02x}) 实色",
                name,
                (v >> 16) & 0xff,
                (v >> 8) & 0xff,
                v & 0xff
            );
            assert_eq!(
                p.row_hover_glass.a(),
                80,
                "{} 主题：glass alpha=80（亚克力下通透）",
                name
            );
        }
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
            let v = visuals(dark, None);
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

    /// v4.7 D31 + M1（2026-09-13）+ P2 v3/v5：`None` = 不透明面板底；材质档 =
    /// 浓淡基色半透明层（只调 alpha，RGB 不变；cap 按材质两主题同档），其余
    /// 字段两路径一致，亮暗两套相同。
    #[test]
    fn panel_tint_visuals_only_affect_panel_fill() {
        use crate::settings::Backdrop;
        for dark in [true, false] {
            let p = Palette::of(dark);
            let opaque = visuals(dark, None);
            assert_eq!(opaque.panel_fill, p.panel, "回退路径（无材质）= 实色面板底");
            for backdrop in [Backdrop::Mica, Backdrop::Acrylic] {
                let a = panel_tint_cap(dark, backdrop).expect("材质档必有浓淡层");
                let tinted = visuals(dark, Some(a));
                // P2 v3：浓淡基色 = tint_color（面板色 × 系统强调色混合，DeskBox
                // 配方）；非 Windows/取色失败时 = p.panel（本断言经 tint_color
                // 取期望值，两平台口径一致）。
                let base = tint_color(dark);
                assert_eq!(
                    tinted.panel_fill,
                    Color32::from_rgba_unmultiplied(
                        base.r(),
                        base.g(),
                        base.b(),
                        (a * 255.0).round() as u8
                    ),
                    "材质档 panel_fill = 浓淡基色 × alpha（RGB 不变）"
                );
                // 其余表面不透明层级不变（行/卡片经 Palette 取用，不在 Visuals 内）
                assert_eq!(tinted.extreme_bg_color, opaque.extreme_bg_color);
                assert_eq!(tinted.window_fill, opaque.window_fill);
                assert_eq!(tinted.dark_mode, opaque.dark_mode);
            }
        }
        // v5：cap 按材质两主题同档（亮色低档已废——带色基色无白漆问题）
        assert_eq!(
            panel_tint_cap(false, Backdrop::Mica),
            panel_tint_cap(true, Backdrop::Mica),
            "云母 cap 两主题一致"
        );
        assert_eq!(
            panel_tint_cap(false, Backdrop::Acrylic),
            panel_tint_cap(true, Backdrop::Acrylic),
            "亚克力 cap 两主题一致"
        );
    }

    /// P2 v2/v5（2026-09-13 真机反馈迭代）：不透明度直控式换算——默认 40% 档
    /// = 真机对比调定推荐观感（云母 0.30 / 亚克力 0.40，两主题同档）；0% =
    /// 纯材质；100% = cap；随 pct 单调不减（越界 clamp）；无材质恒 None。
    #[test]
    fn panel_tint_with_opacity_direct_control() {
        use crate::settings::Backdrop;
        // 默认 40% = 推荐观感锚点（±1e-6：1.0×40/100 存在浮点舍入）
        let anchors = [
            (true, Backdrop::Mica, 0.30_f32),
            (true, Backdrop::Acrylic, 0.40_f32),
            (false, Backdrop::Mica, 0.30_f32),
            (false, Backdrop::Acrylic, 0.40_f32),
        ];
        for (dark, backdrop, anchor) in anchors {
            let a = panel_tint_with_opacity(dark, backdrop, 40).expect("材质档必有浓淡层");
            assert!(
                (a - anchor).abs() < 1e-6,
                "默认 40% 必须复现推荐观感锚点 {anchor}（实际 {a}，dark={dark} {backdrop:?}）"
            );
        }
        // 0% = 纯材质（浓淡层全透）；100% = cap 上限（拉满即面板最实）
        assert_eq!(panel_tint_with_opacity(true, Backdrop::Mica, 0), Some(0.0));
        assert_eq!(
            panel_tint_with_opacity(true, Backdrop::Acrylic, 100),
            panel_tint_cap(true, Backdrop::Acrylic)
        );
        assert_eq!(
            panel_tint_with_opacity(false, Backdrop::Mica, 100),
            panel_tint_cap(false, Backdrop::Mica)
        );
        // 单调不减；超界 u8（200 > 100）视同 100
        let mut prev = -1.0_f32;
        for pct in [0u8, 10, 25, 40, 60, 80, 100, 200] {
            let a = panel_tint_with_opacity(true, Backdrop::Mica, pct).unwrap();
            assert!(a >= prev, "alpha 随 pct 单调不减（pct={pct}）");
            prev = a;
        }
        assert_eq!(
            panel_tint_with_opacity(true, Backdrop::Mica, 200),
            panel_tint_cap(true, Backdrop::Mica),
            "pct > 100 兜底视同 100"
        );
        // cap 排序（材质语义：亚克力拉满可全实、云母拉满保留采色语义）——本条
        // 即真机反馈「调透明度不明显」的修复锚点
        assert!(TINT_CAP_MICA >= 0.75 && TINT_CAP_ACRYLIC > TINT_CAP_MICA);
        // 无材质恒 None（滑杆在该档置灰，换算层同口径）
        assert_eq!(panel_tint_with_opacity(true, Backdrop::None, 50), None);
    }

    /// P2 v5（2026-09-13 真机反馈）：行填充材质适配——hover 分档（云母加权、
    /// 亚克力减重）；selected 同走玻璃（比 hover 重一档，消除扫动时「不透明块
    /// 跳变」的闪烁）；回退 = row_hover/row_selected 实色不变。
    #[test]
    fn row_fills_are_material_aware() {
        use crate::settings::Backdrop;
        let p_light = Palette::of(false);
        let p_dark = Palette::of(true);
        // 回退路径 = row_hover / row_selected 实色（既有观感不变）
        for backdrop in [Backdrop::None, Backdrop::Mica, Backdrop::Acrylic] {
            let f = row_fills(true, backdrop, false);
            assert_eq!(f.hover, p_dark.row_hover);
            assert_eq!(f.selected, p_dark.row_selected);
            let f = row_fills(false, backdrop, false);
            assert_eq!(f.hover, p_light.row_hover);
            assert_eq!(f.selected, p_light.row_selected);
        }
        let f = row_fills(true, Backdrop::None, true);
        assert_eq!(f.hover, p_dark.row_hover);
        assert_eq!(f.selected, p_dark.row_selected);
        // 材质档 = 半透明玻璃（alpha < 255），且 selected 比 hover 重一档
        let lm = row_fills(false, Backdrop::Mica, true);
        let la = row_fills(false, Backdrop::Acrylic, true);
        let dm = row_fills(true, Backdrop::Mica, true);
        let da = row_fills(true, Backdrop::Acrylic, true);
        for f in [&lm, &la, &dm, &da] {
            assert!(f.hover.a() < 255 && f.selected.a() < 255, "材质档必须全玻璃");
            assert!(f.selected.a() > f.hover.a(), "selected 必须重于 hover（平滑过渡）");
        }
        // 云母档强于亚克力档
        assert!(lm.hover.a() > la.hover.a(), "亮色：云母 hover > 亚克力 hover");
        assert!(dm.hover.a() > da.hover.a(), "暗色：云母 hover > 亚克力 hover");
        // 定值锚点（亮·云母 黑 7%/10%、暗·云母 白 7.8%/10.2%、亚克力减半）
        assert_eq!(lm.hover, Color32::from_rgba_unmultiplied(0, 0, 0, 18));
        assert_eq!(lm.selected, Color32::from_rgba_unmultiplied(0, 0, 0, 26));
        assert_eq!(dm.hover, Color32::from_rgba_unmultiplied(255, 255, 255, 20));
        assert_eq!(dm.selected, Color32::from_rgba_unmultiplied(255, 255, 255, 26));
        assert_eq!(la.hover, Color32::from_rgba_unmultiplied(0, 0, 0, 9));
        assert_eq!(la.selected, Color32::from_rgba_unmultiplied(0, 0, 0, 15));
        assert_eq!(da.hover, Color32::from_rgba_unmultiplied(255, 255, 255, 10));
        assert_eq!(da.selected, Color32::from_rgba_unmultiplied(255, 255, 255, 15));
    }

    /// 设置卡填充（2026-09-13）：非玻璃 = 实色 card；玻璃 = card 色 × alpha
    /// （预乘存储，材质从卡面透出），亮暗两主题一致。
    #[test]
    fn card_fill_glass_is_translucent_only_when_asked() {
        for dark in [true, false] {
            let p = Palette::of(dark);
            assert_eq!(card_fill(dark, false), p.card, "回退路径 = 实色卡");
            let expected_a = if dark { CARD_GLASS_ALPHA_DARK } else { CARD_GLASS_ALPHA_LIGHT };
            assert_eq!(
                card_fill(dark, true),
                Color32::from_rgba_unmultiplied(p.card.r(), p.card.g(), p.card.b(), expected_a),
                "玻璃卡 = card 色 × alpha"
            );
            assert!(expected_a < 255, "玻璃卡必须降 alpha（材质透出）");
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

    /// 几何常量与设计稿一致（D8：搜索栏 40 / 行 40；v4.10 D35：页脚 36）。
    #[test]
    fn geometry_matches_design_v4() {
        assert_eq!(ROW_H, 40.0, "行高 40（D8）");
        assert_eq!(SEARCHBAR_H, 40.0, "搜索栏高 40（Fluent Input large）");
        assert_eq!(
            FOOTER_PAD_Y * 2.0 + KEYCAP_H,
            36.0,
            "页脚总高 = 8 + 20 + 8 = 36（v4.10 D35）"
        );
        assert_eq!(FOOTER_FONT, 12.0, "页脚字号 = caption1 base200");
        assert_eq!(KEYCAP_FONT, 12.0, "键帽字号 = caption1（v4.10 D35）");
        assert_eq!(KEYCAP_RADIUS, 5, "键帽圆角 5（v4.10 D35）");
        assert_eq!(KEYCAP_DESC_GAP, 6.0, "帽-文距 6（v4.10 D35）");
        assert_eq!(KEYCAP_PAD_X, 8.0, "键帽左右 padding 8（v4.10 D35）");
    }

    #[test]
    fn theme_preference_maps_all_three_choices() {
        use crate::settings::ThemePref;
        // 批次 4.0：设置页三选 → egui 偏好一一对应
        assert_eq!(theme_preference(ThemePref::System), ThemePreference::System);
        assert_eq!(theme_preference(ThemePref::Light), ThemePreference::Light);
        assert_eq!(theme_preference(ThemePref::Dark), ThemePreference::Dark);
    }

    /// F2 parity：密度标准档硬性等于排印/几何常量锚点（紧凑/宽松按
    /// ±1 字号、±4px 行高同节奏派生），改常量漏改映射在此被拦下。
    #[test]
    fn list_metrics_standard_matches_constants() {
        use crate::settings::ListDensity;
        let m = ListMetrics::of(ListDensity::Standard);
        assert_eq!(m.row_h, ROW_H, "标准档行高 = ROW_H");
        assert_eq!(m.title_pt, LIST_TITLE_PT, "标准档行名字号");
        assert_eq!(m.cat_pt, LIST_CAT_PT, "标准档标签字号");
        assert_eq!(m.icon_cell, LIST_ICON_CELL, "标准档图标格");
        assert_eq!(m.glyph_pt, LIST_GLYPH_PT, "标准档 glyph 字号");
        // 非标准档为独立取值，且节奏与标准档对齐（行高 ±4）
        assert_eq!(
            ListMetrics::of(ListDensity::Compact).row_h,
            ROW_H - 4.0,
            "紧凑 = 标准 − 4"
        );
        assert_eq!(
            ListMetrics::of(ListDensity::Relaxed).row_h,
            ROW_H + 4.0,
            "宽松 = 标准 + 4"
        );
        assert_eq!(ListMetrics::of(ListDensity::Compact).icon_cell, 20.0);
        assert_eq!(ListMetrics::of(ListDensity::Relaxed).icon_cell, 28.0);
    }
}
