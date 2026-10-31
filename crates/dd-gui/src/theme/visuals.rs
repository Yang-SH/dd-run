//! 批次 C（refactor-structure-2026-10-21.md §4.3）：着色混合与 egui `Visuals` 装配。
//! 自 `theme.rs` 原块（500–711 行）逐字节搬入、零逻辑改动；
//! 外部路径经 `theme.rs` 的 `pub use` 桥保持不变。

use eframe::egui::{Color32, Context, CornerRadius, Stroke, Theme, ThemePreference, Visuals};

use super::palette::Palette;

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
/// 着色配置（T6，2026-09-20）：从 `Settings` 投影出的三要素，即模式、自定义色
/// 与强度。theme 侧不直接依赖全量 `Settings`（渲染层只关心这三个值），用
/// [`Colorization::from_settings`] 单点投影。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colorization {
    pub mode: crate::settings::ColorizationMode,
    pub custom: [u8; 3],
    pub intensity: u8,
}

impl Default for Colorization {
    fn default() -> Self {
        Self {
            mode: crate::settings::ColorizationMode::default(),
            custom: crate::settings::CUSTOM_TINT_DEFAULT,
            intensity: 100,
        }
    }
}

impl Colorization {
    /// 设置 → 着色配置（唯一投影点）。
    pub fn from_settings(s: &crate::settings::Settings) -> Self {
        Self {
            mode: s.colorization,
            custom: s.custom_tint_color,
            intensity: s.custom_tint_intensity,
        }
    }
}

/// 浓淡混合比（纯函数，单测锚点）：暗 0.08 / 亮 0.30 为既有真机调定档
/// （见 [`tint_color`] 文档）。强度**只作用于自定义档**（系统强调色档保持
/// 既有固定比例，保证默认档与改造前逐像素一致）。
pub(super) fn tint_mix(dark: bool, cz: Colorization) -> f32 {
    let base = if dark { 0.08_f32 } else { 0.30_f32 };
    match cz.mode {
        crate::settings::ColorizationMode::Custom => {
            base * f32::from(cz.intensity.min(100)) / 100.0
        }
        _ => base,
    }
}

/// 面板色 × 着色色按 `mix` 线性混合（纯函数，单测锚点）。
pub(super) fn mix_panel(panel: Color32, tint: Color32, mix: f32) -> Color32 {
    let lerp = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * mix).round() as u8;
    Color32::from_rgb(
        lerp(panel.r(), tint.r()),
        lerp(panel.g(), tint.g()),
        lerp(panel.b(), tint.b()),
    )
}

/// 浓淡层基色（P2 v3 + v4，2026-09-13 显示对齐；T6 2026-09-20 扩展着色模式）：
/// **面板色 × 着色色混合**——既有口径 = 系统强调色（参考 DeskBox
/// `BuildContentTintColor`：暗基底 + 8% 强调色、亮基底 + 30%）。真机对比反馈：
/// 纯中性白浓淡把 DWM 云母「洗白」成无材质感的白平面——tint 带上色相后，浓淡层
/// 在任何 alpha 下都有材质色相。
///
/// 三档（T6）：
/// - `SystemAccent`（默认）= 既有行为；
/// - `None` = 纯面板色（不掺色）；
/// - `Custom` = 用户色 + 强度（强度按该主题档比例缩放，0 = 面板色）。
///
/// 取不到系统强调色（非 Windows / `DwmGetColorizationColor` 失败）→ 纯面板色
/// （中性回退，与既往一致）。每次 visuals 重建时取色（调用点均为事件驱动）。
pub fn tint_color(dark: bool, cz: Colorization) -> Color32 {
    use crate::settings::ColorizationMode;
    let p = Palette::of(dark);
    let tint = match cz.mode {
        ColorizationMode::None => return p.panel,
        ColorizationMode::SystemAccent => match crate::platform::system_accent_color() {
            Some(a) => a,
            None => return p.panel,
        },
        ColorizationMode::Custom => Color32::from_rgb(cz.custom[0], cz.custom[1], cz.custom[2]),
    };
    mix_panel(p.panel, tint, tint_mix(dark, cz))
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
pub fn visuals(dark: bool, panel_tint: Option<f32>, cz: Colorization) -> Visuals {
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
            let base = tint_color(dark, cz);
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
pub fn apply(ctx: &Context, pref: ThemePreference, panel_tint: Option<f32>, cz: Colorization) {
    ctx.set_visuals_of(Theme::Dark, visuals(true, panel_tint, cz));
    ctx.set_visuals_of(Theme::Light, visuals(false, panel_tint, cz));
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
pub fn apply_panel_tint(ctx: &Context, panel_tint: Option<f32>, cz: Colorization) {
    ctx.set_visuals_of(Theme::Dark, visuals(true, panel_tint, cz));
    ctx.set_visuals_of(Theme::Light, visuals(false, panel_tint, cz));
}

/// [`settings::ThemePref`] → egui [`ThemePreference`]（设置页选择立即生效用）。
pub fn theme_preference(pref: crate::settings::ThemePref) -> ThemePreference {
    match pref {
        crate::settings::ThemePref::System => ThemePreference::System,
        crate::settings::ThemePref::Light => ThemePreference::Light,
        crate::settings::ThemePref::Dark => ThemePreference::Dark,
    }
}

pub(super) fn rgb(v: u32) -> Color32 {
    Color32::from_rgb(
        ((v >> 16) & 0xff) as u8,
        ((v >> 8) & 0xff) as u8,
        (v & 0xff) as u8,
    )
}

/// 构造带 alpha 的 `Color32`（24 位 RGB + 自定义 alpha，未预乘）。
/// 用于「半透叠加」场景（如亚克力下的 hover 玻璃色、搜索框底色、下边框）。
pub(super) fn glass(v: u32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(
        ((v >> 16) & 0xff) as u8,
        ((v >> 8) & 0xff) as u8,
        (v & 0xff) as u8,
        a,
    )
}
