//! 批次 C（refactor-structure-2026-10-21.md §4.3）：材质档注册表与行 / 卡片填充。
//! 自 `theme.rs` 原块（331–498 行）逐字节搬入、零逻辑改动；
//! 外部路径经 `theme.rs` 的 `pub use` 桥保持不变。

use eframe::egui::Color32;

use super::palette::Palette;

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
/// （`dark` 参数保留以稳定调用点签名、不参与换算——以 `_dark` 标注）。
pub fn panel_tint_cap(dark: bool, backdrop: crate::settings::Backdrop) -> Option<f32> {
    let _ = dark; // v5 起上限按材质定档、两主题一致（签名保留以稳定调用点）
    backdrop_config(backdrop).tint_cap
}

/// 材质档的**能力与参数注册表**（M1，2026-09-20；参照 PowerToys CmdPal
/// `BackdropStyles`/`BackdropStyleConfig`）：新增档位只需在
/// `settings::Backdrop` 三件套加值 + [`backdrop_config`] 加一行——设置页
/// pill/描述、浓淡层 cap、平台 DWM 映射（`platform::SystemBackdrop::from`）
/// 全部由注册表派生，不再各处 `match`（消除加档漂移）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackdropStyleConfig {
    /// 浓淡层 alpha 上限；`None` = 该档不画浓淡层（不透明面板）。
    pub tint_cap: Option<f32>,
    /// 是否可调浓淡（dd-run 语义 = 浓淡强度而非窗口透明度，当前四档均为
    /// true；保留位，便于未来引入不可调档时置灰滑杆，对齐 CmdPal
    /// `SupportsOpacity`）。
    pub supports_opacity: bool,
    /// 结果行填充分档（材质适配）：见 [`RowFillProfile`]。
    pub row_fill: RowFillProfile,
}

/// 结果行 hover/selected 的材质适配档（P2 v5 真机反馈；M1 起由注册表派生）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowFillProfile {
    /// 云母系：**加权**——云母面板近白/近黑，行反馈需更重才可辨。
    Weighted,
    /// 亚克力：**减重**——动态模糊上过重会形成突兀亮块。
    Light,
}

/// 材质档 → 能力与参数（单一来源；`Backdrop::ALL` 每一档都必须有分支）。
pub fn backdrop_config(backdrop: crate::settings::Backdrop) -> BackdropStyleConfig {
    use crate::settings::Backdrop;
    match backdrop {
        Backdrop::None => BackdropStyleConfig {
            tint_cap: None,
            supports_opacity: false,
            // 无材质不走行填充分档（`row_fills` 提前返回实色）；占位值不可达。
            row_fill: RowFillProfile::Light,
        },
        // 云母 / 云母 Alt：同 cap（拉满仍保留 25% 采色语义）+ 同「加权」行填充
        Backdrop::Mica | Backdrop::MicaAlt => BackdropStyleConfig {
            tint_cap: Some(TINT_CAP_MICA),
            supports_opacity: true,
            row_fill: RowFillProfile::Weighted,
        },
        Backdrop::Acrylic => BackdropStyleConfig {
            tint_cap: Some(TINT_CAP_ACRYLIC),
            supports_opacity: true,
            row_fill: RowFillProfile::Light,
        },
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
///
/// 供 `row.rs` 结果行专用；卡片/设置项等其余 hover 维持实色不变。
#[derive(Debug, Clone, Copy)]
pub struct RowFills {
    /// 非选中行 hover 玻璃。
    pub hover: Color32,
    /// 选中行填充（材质档 = 玻璃；回退 = 实色 `row_selected`）。
    pub selected: Color32,
}

pub fn row_fills(
    dark: bool,
    backdrop: crate::settings::Backdrop,
    backdrop_active: bool,
) -> RowFills {
    use crate::settings::Backdrop;
    let p = Palette::of(dark);
    if !backdrop_active || backdrop == Backdrop::None {
        return RowFills {
            hover: p.row_hover,
            selected: p.row_selected,
        };
    }
    // M1：分档取自材质注册表（云母系加权 / 亚克力减重）——加档无需改本处。
    match (dark, backdrop_config(backdrop).row_fill) {
        (true, RowFillProfile::Weighted) => RowFills {
            hover: Color32::from_rgba_unmultiplied(255, 255, 255, 20),
            selected: Color32::from_rgba_unmultiplied(255, 255, 255, 26),
        },
        (true, RowFillProfile::Light) => RowFills {
            hover: Color32::from_rgba_unmultiplied(255, 255, 255, 10),
            selected: Color32::from_rgba_unmultiplied(255, 255, 255, 15),
        },
        (false, RowFillProfile::Weighted) => RowFills {
            hover: Color32::from_rgba_unmultiplied(0, 0, 0, 18),
            selected: Color32::from_rgba_unmultiplied(0, 0, 0, 26),
        },
        (false, RowFillProfile::Light) => RowFills {
            hover: Color32::from_rgba_unmultiplied(0, 0, 0, 9),
            selected: Color32::from_rgba_unmultiplied(0, 0, 0, 15),
        },
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
    let a: u8 = if dark {
        CARD_GLASS_ALPHA_DARK
    } else {
        CARD_GLASS_ALPHA_LIGHT
    };
    Color32::from_rgba_unmultiplied(p.card.r(), p.card.g(), p.card.b(), a)
}
