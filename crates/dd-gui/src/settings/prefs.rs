//! O12（2026-10-11）拆分自 `settings.rs`：设置枚举族——主题（ThemePref）/ 首屏
//! （OpenView）/ 材质（Backdrop）/ 圆角（CornerPref）/ 边框（BorderMode）/ Esc
//! 行为（EscBehavior/EscAction）/ 着色模式（ColorizationMode）/ 语言（Lang）/
//! 列表密度（ListDensity）/ 背景图适应（BgImageFit）。搬运单位 = 完整定义块，
//! 函数体一字不改。

/// 主题偏好（设计稿 §6.1 用户决策范围：跟随系统 / 亮色 / 暗色三选）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemePref {
    /// 跟随系统亮暗（默认）。
    #[default]
    System,
    /// 强制亮色。
    Light,
    /// 强制暗色。
    Dark,
}

impl ThemePref {
    /// 设置页显示标签。
    pub fn label(self) -> &'static str {
        match self {
            ThemePref::System => "跟随系统",
            ThemePref::Light => "亮色",
            ThemePref::Dark => "暗色",
        }
    }

    /// JSON 序列化值（稳定标识，与显示标签解耦）。
    pub fn as_str(self) -> &'static str {
        match self {
            ThemePref::System => "system",
            ThemePref::Light => "light",
            ThemePref::Dark => "dark",
        }
    }

    /// JSON 值反解；未知值返回 `None`（调用方回落默认）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "system" => Some(ThemePref::System),
            "light" => Some(ThemePref::Light),
            "dark" => Some(ThemePref::Dark),
            _ => None,
        }
    }
}

/// 打开面板（空查询）时的首屏显示范围（真机反馈 2026-09-04：
/// 默认只显示默认功能，不铺全部应用；输入查询时应用仍参与匹配）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpenView {
    /// 默认功能：隐藏「应用」列表（`result_category == "应用"`），其余分组照常。
    #[default]
    Default,
    /// 显示全部（含所有应用，旧行为）。
    All,
}

impl OpenView {
    /// 设置页显示标签。
    pub fn label(self) -> &'static str {
        match self {
            OpenView::Default => "默认功能",
            OpenView::All => "所有应用与功能",
        }
    }

    /// JSON 序列化值（稳定标识，与显示标签解耦）。
    pub fn as_str(self) -> &'static str {
        match self {
            OpenView::Default => "default",
            OpenView::All => "all",
        }
    }

    /// JSON 值反解；未知值返回 `None`（调用方回落默认）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "default" => Some(OpenView::Default),
            "all" => Some(OpenView::All),
            _ => None,
        }
    }
}

/// 窗口材质（v4.7 D30：Win11 DWM 系统背景材质，单值属性；2026-09-20 M2 加
/// 云母 Alt 档——参照 PowerToys CmdPal `BackdropStyle`）。默认云母（含旧配置
/// 升级：字段缺失 → 云母）。档位清单见 [`Backdrop::ALL`]（设置页 pill 顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backdrop {
    /// 无材质（不透明面板）。
    None,
    /// 云母（默认）。
    #[default]
    Mica,
    /// 云母 Alt（`DWMSBT_TABBEDWINDOW`；云母变体，色更浓、对比更强，
    /// Win11 22H2+；应用失败走既有回退链）。
    MicaAlt,
    /// 亚克力。
    Acrylic,
}

impl Backdrop {
    /// 全部档位（M1，2026-09-20）：设置页 pill 顺序 = 本数组顺序；
    /// 新增档位只改此处 + `label/as_str/parse` 三件套 + i18n 两键。
    pub const ALL: [Backdrop; 4] = [
        Backdrop::None,
        Backdrop::Mica,
        Backdrop::MicaAlt,
        Backdrop::Acrylic,
    ];

    /// 设置页 pill 文案 i18n 键（显示文案唯一来源；`label()` 仅供日志短标签）。
    pub fn name_key(self) -> &'static str {
        match self {
            Backdrop::None => "set.material.none",
            Backdrop::Mica => "set.material.mica",
            Backdrop::MicaAlt => "set.material.mica_alt",
            Backdrop::Acrylic => "set.material.acrylic",
        }
    }

    /// 卡片描述 i18n 键（跟随当前材质的说明行）。
    pub fn desc_key(self) -> &'static str {
        match self {
            Backdrop::None => "set.material.none.desc",
            Backdrop::Mica => "set.material.mica.desc",
            Backdrop::MicaAlt => "set.material.mica_alt.desc",
            Backdrop::Acrylic => "set.material.acrylic.desc",
        }
    }

    /// 设置页显示标签。
    pub fn label(self) -> &'static str {
        match self {
            Backdrop::None => "无材质",
            Backdrop::Mica => "云母",
            Backdrop::MicaAlt => "云母 Alt",
            Backdrop::Acrylic => "亚克力",
        }
    }

    /// JSON 序列化值（稳定标识，与显示标签解耦）。
    pub fn as_str(self) -> &'static str {
        match self {
            Backdrop::None => "none",
            Backdrop::Mica => "mica",
            Backdrop::MicaAlt => "mica_alt",
            Backdrop::Acrylic => "acrylic",
        }
    }

    /// JSON 值反解；未知值返回 `None`（调用方回落默认云母）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Backdrop::None),
            "mica" => Some(Backdrop::Mica),
            "mica_alt" => Some(Backdrop::MicaAlt),
            "acrylic" => Some(Backdrop::Acrylic),
            _ => None,
        }
    }
}

/// 窗口圆角偏好（窗口材质与边框方案 P3，2026-09-13；参考 DeskBox
/// `WidgetCornerPreference`）。映射 `DWMWA_WINDOW_CORNER_PREFERENCE` 三档
/// （`platform::apply_window_chrome`）；Win11 以下该属性不可用，失败仅记日志
/// 跳过。默认圆角 = 既有现状（v4.7 起恒 `DWMWCP_ROUND`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CornerPref {
    /// 圆角（`DWMWCP_ROUND`，≈8px，对齐设计 token `window_corner_radius = 8`）。
    #[default]
    Round,
    /// 小圆角（`DWMWCP_ROUNDSMALL`，≈4px）。
    Small,
    /// 方角（`DWMWCP_DONOTROUND`）。
    Square,
}

impl CornerPref {
    /// 设置页显示标签。
    pub fn label(self) -> &'static str {
        match self {
            CornerPref::Round => "圆角",
            CornerPref::Small => "小圆角",
            CornerPref::Square => "方角",
        }
    }

    /// JSON 序列化值（稳定标识，与显示标签解耦）。
    pub fn as_str(self) -> &'static str {
        match self {
            CornerPref::Round => "round",
            CornerPref::Small => "small",
            CornerPref::Square => "square",
        }
    }

    /// JSON 值反解；未知值返回 `None`（调用方回落默认圆角）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "round" => Some(CornerPref::Round),
            "small" => Some(CornerPref::Small),
            "square" => Some(CornerPref::Square),
            _ => None,
        }
    }
}

/// 面板边框颜色模式（窗口材质与边框方案 P4，2026-09-13；参考 DeskBox
/// `WidgetBorderColorMode`）。映射 `DWMWA_BORDER_COLOR`——1px 实色，宽度
/// 固定，粗细不在能力面内；仅材质生效时绘制（既有语义）。默认中性 = 现状。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderMode {
    /// 中性灰（`Palette::border_strong`，随主题明暗；默认）。
    #[default]
    Neutral,
    /// 系统强调色（`DwmGetColorizationColor`，取不到回落 `Palette::accent`；
    /// 跨主题恒色）。
    Accent,
    /// 关（`DWMWA_BORDER_COLOR_NONE` 停画）。
    None,
}

impl BorderMode {
    /// 设置页显示标签。
    pub fn label(self) -> &'static str {
        match self {
            BorderMode::Neutral => "中性",
            BorderMode::Accent => "强调色",
            BorderMode::None => "关",
        }
    }

    /// JSON 序列化值（稳定标识，与显示标签解耦）。
    pub fn as_str(self) -> &'static str {
        match self {
            BorderMode::Neutral => "neutral",
            BorderMode::Accent => "accent",
            BorderMode::None => "none",
        }
    }

    /// JSON 值反解；未知值返回 `None`（调用方回落默认中性）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "neutral" => Some(BorderMode::Neutral),
            "accent" => Some(BorderMode::Accent),
            "none" => Some(BorderMode::None),
            _ => None,
        }
    }
}

/// Esc 键行为（B1，2026-09-20；参照 PowerToys CmdPal `EscapeKeyBehavior`，
/// 按 dd-run 语义收敛为三档）。默认 `GoBack` = **既有行为**（非 Root 返回上一级、
/// Root 隐藏），旧配置零迁移。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EscBehavior {
    /// 返回上一级（Root 时隐藏）——既有行为，默认。
    #[default]
    GoBack,
    /// 先清除搜索内容，然后返回（对齐 CmdPal 默认档）。
    ClearThenGoBack,
    /// 始终隐藏面板（不返回）。
    AlwaysHide,
}

/// Esc 按键的**决策结果**（纯函数 [`EscBehavior::decide`] 的返回值，可单测；
/// 由 `app::keys` 执行副作用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscAction {
    /// 清空当前页搜索内容（不返回）。
    ClearSearch,
    /// 返回上一级（并聚焦回落后页面的搜索框）。
    GoBack,
    /// 隐藏面板。
    Hide,
}

impl EscBehavior {
    /// 全部档位（设置页下拉顺序 = 本数组顺序）。
    pub const ALL: [EscBehavior; 3] = [
        EscBehavior::GoBack,
        EscBehavior::ClearThenGoBack,
        EscBehavior::AlwaysHide,
    ];

    /// 下拉文案 i18n 键。
    pub fn name_key(self) -> &'static str {
        match self {
            EscBehavior::GoBack => "set.esc.go_back",
            EscBehavior::ClearThenGoBack => "set.esc.clear_then_back",
            EscBehavior::AlwaysHide => "set.esc.always_hide",
        }
    }

    /// 设置页显示标签（日志用短标签）。
    pub fn label(self) -> &'static str {
        match self {
            EscBehavior::GoBack => "返回上一级",
            EscBehavior::ClearThenGoBack => "先清搜索再返回",
            EscBehavior::AlwaysHide => "始终隐藏",
        }
    }

    /// JSON 序列化值（稳定标识，与显示标签解耦）。
    pub fn as_str(self) -> &'static str {
        match self {
            EscBehavior::GoBack => "go_back",
            EscBehavior::ClearThenGoBack => "clear_then_go_back",
            EscBehavior::AlwaysHide => "always_hide",
        }
    }

    /// JSON 值反解；未知值返回 `None`（调用方回落默认「返回上一级」）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "go_back" => Some(EscBehavior::GoBack),
            "clear_then_go_back" => Some(EscBehavior::ClearThenGoBack),
            "always_hide" => Some(EscBehavior::AlwaysHide),
            _ => None,
        }
    }

    /// 纯决策：给定「是否 Root」与「搜索框是否为空」→ 该做什么。
    ///
    /// - `GoBack`：Root → 隐藏；否则返回上一级；
    /// - `ClearThenGoBack`：搜索非空 → 清空；搜索为空 → 同 `GoBack`；
    /// - `AlwaysHide`：恒隐藏（无论层级与查询）。
    pub fn decide(self, is_root: bool, query_empty: bool) -> EscAction {
        match self {
            EscBehavior::AlwaysHide => EscAction::Hide,
            EscBehavior::GoBack => {
                if is_root {
                    EscAction::Hide
                } else {
                    EscAction::GoBack
                }
            }
            EscBehavior::ClearThenGoBack => {
                if !query_empty {
                    EscAction::ClearSearch
                } else if is_root {
                    EscAction::Hide
                } else {
                    EscAction::GoBack
                }
            }
        }
    }
}

/// 着色模式（T6，2026-09-20；参照 PowerToys CmdPal `ColorizationMode`，按 dd-run
/// 语义收敛为三档）——决定**材质浓淡层基色**的来源。**刻意只影响浓淡层**：
/// Fluent 主题 token（选中/链接等 accent）与边框强调色不受影响，避免主题系统级冲突。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorizationMode {
    /// 系统强调色（默认 = 既有行为：面板色 × 系统强调色，暗 8% / 亮 30%）。
    #[default]
    SystemAccent,
    /// 无着色（浓淡层 = 纯面板色）。
    None,
    /// 自定义色 + 强度（1–100；100 = 该主题档的满混合比）。
    Custom,
}

/// 自定义浓淡色默认值（Windows 默认强调蓝；仅「切到自定义且未取色」时可见）。
pub const CUSTOM_TINT_DEFAULT: [u8; 3] = [0, 120, 212];

impl ColorizationMode {
    /// 全部档位（设置页 pill 顺序 = 本数组顺序）。
    pub const ALL: [ColorizationMode; 3] = [
        ColorizationMode::SystemAccent,
        ColorizationMode::None,
        ColorizationMode::Custom,
    ];

    /// 设置页文案 i18n 键。
    pub fn name_key(self) -> &'static str {
        match self {
            ColorizationMode::SystemAccent => "set.color.system_accent",
            ColorizationMode::None => "set.color.none",
            ColorizationMode::Custom => "set.color.custom",
        }
    }

    /// 设置页显示标签（日志用短标签）。
    pub fn label(self) -> &'static str {
        match self {
            ColorizationMode::SystemAccent => "系统强调色",
            ColorizationMode::None => "无着色",
            ColorizationMode::Custom => "自定义",
        }
    }

    /// JSON 序列化值（稳定标识，与显示标签解耦）。
    pub fn as_str(self) -> &'static str {
        match self {
            ColorizationMode::SystemAccent => "system_accent",
            ColorizationMode::None => "none",
            ColorizationMode::Custom => "custom",
        }
    }

    /// JSON 值反解；未知值返回 `None`（调用方回落默认系统强调色）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "system_accent" => Some(ColorizationMode::SystemAccent),
            "none" => Some(ColorizationMode::None),
            "custom" => Some(ColorizationMode::Custom),
            _ => None,
        }
    }
}
/// 界面语言（v4.13 D38）。`FollowSystem`（默认）在运行时经平台探测解析为
/// 具体语言（`crate::platform::system_ui_lang`：zh 系 → ZhCn，其余 → EnUs）。
/// 显示文案不走固定中文 `label()`——语言卡经 `text::t()` 按**当前生效语言**
/// 取文案；「简体中文」/「English」两选项恒为自称原文（语言选择惯例）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// 跟随系统 UI 语言（默认）。
    #[default]
    FollowSystem,
    /// 简体中文。
    ZhCn,
    /// English。
    EnUs,
}

impl Lang {
    /// JSON 序列化值（稳定标识，与显示文案解耦）。
    pub fn as_str(self) -> &'static str {
        match self {
            Lang::FollowSystem => "follow_system",
            Lang::ZhCn => "zh_cn",
            Lang::EnUs => "en_us",
        }
    }

    /// JSON 值反解；未知值返回 `None`（调用方回落默认 FollowSystem）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "follow_system" => Some(Lang::FollowSystem),
            "zh_cn" => Some(Lang::ZhCn),
            "en_us" => Some(Lang::EnUs),
            _ => None,
        }
    }
}

/// 列表密度档（icons-typography-plan.md F2，参考 DeskBox「图标/文字大小可调」）。
/// 一档联动行高/标题字号/标签字号/图标格/glyph 字号五值——具体数值见
/// `dd_gui::theme::ListMetrics`（标准档 = 既有常量，parity 单测守卫）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListDensity {
    /// 紧凑（行高 36，一屏更多结果）。
    Compact,
    /// 标准（默认；行高 40，D8 几何契约）。
    #[default]
    Standard,
    /// 宽松（行高 44，触控友好）。
    Relaxed,
}

impl ListDensity {
    /// 设置页显示标签。
    pub fn label(self) -> &'static str {
        match self {
            ListDensity::Compact => "紧凑",
            ListDensity::Standard => "标准",
            ListDensity::Relaxed => "宽松",
        }
    }

    /// JSON 序列化值（稳定标识，与显示标签解耦）。
    pub fn as_str(self) -> &'static str {
        match self {
            ListDensity::Compact => "compact",
            ListDensity::Standard => "standard",
            ListDensity::Relaxed => "relaxed",
        }
    }

    /// JSON 值反解；未知值返回 `None`（调用方回落默认）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "compact" => Some(ListDensity::Compact),
            "standard" => Some(ListDensity::Standard),
            "relaxed" => Some(ListDensity::Relaxed),
            _ => None,
        }
    }
}

/// T9（2026-10-05）：背景图适应方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgImageFit {
    /// 等比裁剪铺满（cover）——保持宽高比，裁掉超出部分（对齐 CmdPal Fill）。
    Fill,
    /// 拉伸至面板尺寸（忽略宽高比）。
    Stretch,
}

impl BgImageFit {
    pub const ALL: [BgImageFit; 2] = [BgImageFit::Fill, BgImageFit::Stretch];

    pub fn as_str(&self) -> &'static str {
        match self {
            BgImageFit::Fill => "fill",
            BgImageFit::Stretch => "stretch",
        }
    }
    /// 未知字符串 → `None`（调用方回落 Fill，与 ThemePref 同口径）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fill" => Some(BgImageFit::Fill),
            "stretch" => Some(BgImageFit::Stretch),
            _ => None,
        }
    }
    pub fn name_key(&self) -> &'static str {
        match self {
            BgImageFit::Fill => "set.bgimg.fit_fill",
            BgImageFit::Stretch => "set.bgimg.fit_stretch",
        }
    }
}

/// 背景图不透明度默认值（T9：对齐 CmdPal `BackgroundImageOpacity` = 20）。
pub const BG_IMAGE_OPACITY_DEFAULT: u8 = 20;
/// 背景图着色强度默认值（T9：默认不叠色）。
pub const BG_IMAGE_TINT_DEFAULT: u8 = 0;
