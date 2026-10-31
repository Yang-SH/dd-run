//! 批次 C（refactor-structure-2026-10-21.md §4.3）：几何 / 排印 / 页脚键帽 / 设置按钮 token 与 `ListMetrics`。
//! 自 `theme.rs` 原块（16–130 行）逐字节搬入、零逻辑改动；
//! 外部路径经 `theme.rs` 的 `pub use` 桥保持不变。
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
