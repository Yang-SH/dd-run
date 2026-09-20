//! M5 批次 4.0：宿主本地设置（纯逻辑，不依赖 egui，可单测）。
//!
//! 设计稿 §6.1 设置按钮打开的设置页内容（用户决策：**仅主题偏好**）。
//! `Settings` 持久化到 [`dd_host::manifest::config_file`]（数据根目录下
//! `config.json`）；文件缺失/损坏/字段未知时一律回落默认值（`System`），
//! 不让坏配置阻断启动。JSON 手工经 `serde_json::Value` 读写——仅一个字段，
//! 不为此引入 serde derive 依赖。
//!
//! 渲染层语义（在 bin 层接线）：选择变化 → 立即 `ctx.set_theme` 生效 +
//! [`Settings::save`] 落盘（best-effort，写失败仅记日志不阻断 UI）。

use dd_host::manifest::config_file;

/// 设置页在页面栈中的 `page_id` 标记（`PageState::page_id` 的保留值，
/// 不会与协议 `page_id` 冲突：协议 id 来自扩展，无此双下划线保留前缀）。
pub const SETTINGS_PAGE_ID: &str = "__settings__";

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

/// 搜索引擎配置（2026-09-05 新增设置项：可配置搜索引擎）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchEngine {
    /// 展示名（如 `Google`；也用于扩展侧命令 id 的 slug）。
    pub name: String,
    /// 搜索 URL 模板，含 `{q}` 占位符——dd-ext-websearch 将其替换为
    /// RFC 3986 编码后的关键词。
    pub template: String,
}

impl SearchEngine {
    /// 校验并构造：name 非空、template 含 `{q}` 且以 `http(s)://` 开头。
    pub fn new(name: &str, template: &str) -> Option<Self> {
        let name = name.trim();
        let template = template.trim();
        if name.is_empty()
            || !template.contains("{q}")
            || !(template.starts_with("http://") || template.starts_with("https://"))
        {
            return None;
        }
        Some(Self {
            name: name.to_string(),
            template: template.to_string(),
        })
    }
}

/// 常用预设引擎（设置页下拉可添加项；与 `dd-ext-websearch` 内置默认表保持一致——
/// 两侧各自定义，扩展侧为环境变量缺失时的回落值）。
pub fn preset_search_engines() -> Vec<SearchEngine> {
    [
        ("Google", "https://www.google.com/search?q={q}"),
        ("Bing", "https://www.bing.com/search?q={q}"),
        ("Baidu", "https://www.baidu.com/s?wd={q}"),
        ("DuckDuckGo", "https://duckduckgo.com/?q={q}"),
        ("GitHub", "https://github.com/search?q={q}"),
    ]
    .iter()
    .map(|(n, t)| SearchEngine::new(n, t).expect("预设引擎模板合法"))
    .collect()
}

/// 默认启用的引擎（2026-09-12 用户决策：默认**只开 Google**，其余预设仍可在
/// 设置页「搜索」栏手动添加）。与 [`preset_search_engines`]（可添加目录）解耦。
pub fn default_search_engines() -> Vec<SearchEngine> {
    let presets = preset_search_engines();
    vec![presets[0].clone()]
}

/// 宿主本地设置（主题偏好 + 首屏视图 + 搜索引擎 + 窗口材质 + 热键/自启/扩展；
/// 后续字段向后兼容追加）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub theme: ThemePref,
    pub open_view: OpenView,
    /// 启用的搜索引擎（面板「网络搜索」分组按此渲染；经
    /// `DD_WEBSEARCH_ENGINES` 环境变量传给 dd-ext-websearch）。
    pub search_engines: Vec<SearchEngine>,
    /// 窗口材质（v4.7 D30；默认云母，材质不可用场景由渲染层回退不透明）。
    pub backdrop: Backdrop,
    /// 材质不透明度百分比（窗口材质与边框方案 P2 v2，2026-09-13：0–100 直控
    /// egui 浓淡层，`alpha = cap × pct/100`，见 `theme::panel_tint_with_opacity`；
    /// 默认 40 = 精确复现 M1 真机调定四档锚点观感；0 = 纯材质，100 = 面板最实）。
    pub material_opacity: u8,
    /// 窗口圆角偏好（P3；默认圆角 = 现状）。
    pub corner_pref: CornerPref,
    /// 面板边框颜色模式（P4；默认中性 = 现状）。
    pub border_mode: BorderMode,
    /// Esc 键行为（B1，2026-09-20；默认「返回上一级」= 既有行为）。
    pub esc_behavior: EscBehavior,
    /// 退格键返回（B2，2026-09-20）：搜索框为空时按 Backspace 返回上一级；
    /// 默认 false = 既有行为（Backspace 仅用于编辑输入）。
    pub backspace_go_back: bool,
    /// 着色模式（T6，2026-09-20；默认系统强调色 = 既有行为）。
    pub colorization: ColorizationMode,
    /// 自定义浓淡色（RGB；默认 [`CUSTOM_TINT_DEFAULT`]）。
    pub custom_tint_color: [u8; 3],
    /// 自定义着色强度（0–100，默认 100 = 该主题档满混合比）。
    pub custom_tint_intensity: u8,
    /// 单击激活（T7，2026-09-20）：默认 **true** = 既有行为（单击行即执行）；
    /// 关闭 = 单击仅选中、双击才执行（对齐 CmdPal `SingleClickActivates`，
    /// 但默认取值与 CmdPal 相反——保持 dd-run 既有交互不变）。
    pub single_click_activation: bool,
    /// 界面动效（T8，2026-09-20）：默认 **true** = 既有行为（搜索框聚焦下划线
    /// 0.12s 过渡等装饰性过渡）；关闭 = 直出终态。**不涉及 DWM 过渡**
    /// （`DWMWA_TRANSITIONS_FORCEDISABLED` 恒禁，避免弹窗闪烁）。
    pub ui_animations: bool,
    /// 全局热键修饰键位掩码（M6 批次 6.3：MOD_ALT=1/CONTROL=2/SHIFT=4/WIN=8，
    /// 不含 NOREPEAT——注册时由热键线程统一补）。默认 Win+Alt。
    pub hotkey_mods: u32,
    /// 全局热键主键虚拟键码（默认 VK_SPACE = 0x20）。
    pub hotkey_vk: u32,
    /// 开机自启（M6 批次 6.3：HKCU Run 键；默认关）。
    pub autostart: bool,
    /// 已停用扩展的清单 id 列表（M6 批次 6.3：聚合时跳过；默认空 = 全启用）。
    pub disabled_extensions: Vec<String>,
    /// 面板记忆尺寸（v4.12 D37：手动拉伸后的逻辑宽高，取整存储；
    /// `None` = 从未手动拉伸，唤起用自适应基准 `APP_W/APP_H`）。
    /// 唤起时仍按光标所在屏工作区 clamp（小屏收缩），见 `app::root_panel_size`。
    pub panel_size: Option<(u32, u32)>,
    /// 界面语言偏好（v4.13 D38：默认 FollowSystem；只存用户偏好——
    /// 运行时解析后的生效语言存 `PaletteApp.lang_effective`）。
    pub lang: Lang,
    /// 列表密度（F2：默认标准 = D8 40px 行；缺失/未知值回落标准，旧配置零迁移）。
    pub density: ListDensity,
    /// 搜索应用（2026-09-12 新增）：关闭后根页结果不包含「应用」类项
    /// （空查询首屏与关键词匹配均排除）；默认开（保持既有行为）。
    /// （「优先搜索文件」开关同日加入、同日撤销：自动进页劫持常规搜索，
    /// 用户反馈后移除——当时保留 `f ` 前缀直达；该前缀亦已于 2026-09-19 移除，现由 `Ctrl+F` 一键直达承担。）
    pub search_apps: bool,
}

/// 全局热键默认修饰键：Win + Alt（MOD_* 值：ALT=1/CONTROL=2/SHIFT=4/WIN=8，
/// 与 hotkey.rs 的 windows-sys 常量一致；本 crate 纯逻辑不依赖 windows-sys，
/// 用字面量 + 单测锚定）。不含 NOREPEAT——注册时由热键线程统一补。
pub const HOTKEY_MODS_DEFAULT: u32 = 0b1000 | 0b0001;
/// 全局热键默认主键：VK_SPACE。
pub const HOTKEY_VK_DEFAULT: u32 = 0x20;
/// 修饰键合法位掩码（Ctrl/Alt/Shift/Win），解析时剔除其余位。
pub const HOTKEY_MODS_MASK: u32 = 0b1111;

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemePref::default(),
            open_view: OpenView::default(),
            search_engines: default_search_engines(),
            backdrop: Backdrop::default(),
            material_opacity: 40,
            corner_pref: CornerPref::default(),
            border_mode: BorderMode::default(),
            esc_behavior: EscBehavior::default(),
            backspace_go_back: false,
            colorization: ColorizationMode::default(),
            custom_tint_color: CUSTOM_TINT_DEFAULT,
            custom_tint_intensity: 100,
            single_click_activation: true,
            ui_animations: true,
            hotkey_mods: HOTKEY_MODS_DEFAULT,
            hotkey_vk: HOTKEY_VK_DEFAULT,
            autostart: false,
            disabled_extensions: Vec::new(),
            panel_size: None,
            lang: Lang::default(),
            density: ListDensity::default(),
            search_apps: true,
        }
    }
}

/// 修饰键掩码 → 显示标签（固定顺序 Ctrl+Alt+Shift+Win，Windows 惯例）。
pub fn hotkey_mods_label(mods: u32) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if mods & 0b0010 != 0 {
        parts.push("Ctrl");
    }
    if mods & 0b0001 != 0 {
        parts.push("Alt");
    }
    if mods & 0b0100 != 0 {
        parts.push("Shift");
    }
    if mods & 0b1000 != 0 {
        parts.push("Win");
    }
    parts.join("+")
}

/// 虚拟键码 → 显示名（覆盖设置页可捕获的键集，M6 批次 6.3）。
pub fn hotkey_vk_label(vk: u32) -> String {
    match vk {
        0x20 => "Space".to_string(),
        0x21 => "PgUp".to_string(),
        0x22 => "PgDn".to_string(),
        0x2D => "Insert".to_string(),
        0x70..=0x7B => format!("F{}", vk - 0x6F),
        0x30..=0x39 | 0x41..=0x5A => (vk as u8 as char).to_string(),
        0xBA => ";".into(),
        0xBB => "=".into(),
        0xBC => ",".into(),
        0xBD => "-".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0xC0 => "`".into(),
        0xDB => "[".into(),
        0xDC => "\\".into(),
        0xDD => "]".into(),
        0xDE => "'".into(),
        _ => format!("VK_{vk:02X}"),
    }
}

impl Settings {
    /// 从 JSON 文本解析；空/损坏/字段未知 → 默认（防御性，永不失败）。
    pub fn parse_json(text: &str) -> Self {
        let mut s = Self::default();
        let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
            return s;
        };
        if let Some(t) = v.get("theme").and_then(|t| t.as_str()) {
            if let Some(pref) = ThemePref::parse(t) {
                s.theme = pref;
            }
        }
        if let Some(t) = v.get("open_view").and_then(|t| t.as_str()) {
            if let Some(view) = OpenView::parse(t) {
                s.open_view = view;
            }
        }
        // 窗口材质（v4.7）：字段缺失（旧版本配置）→ 默认云母（D30）；未知值回落云母。
        if let Some(t) = v.get("backdrop").and_then(|t| t.as_str()) {
            if let Some(backdrop) = Backdrop::parse(t) {
                s.backdrop = backdrop;
            }
        }
        // 材质不透明度 / 窗口圆角 / 面板边框（P2–P4，2026-09-13）：字段缺失
        // （旧版本配置）→ 默认（40 / 圆角 / 中性——40% = M1 真机调定锚点观感）；
        // 数值越界 clamp 到 0–100；类型损坏或未知字符串回落默认（与 backdrop
        // 同口径）。
        s.material_opacity = v
            .get("material_opacity")
            .and_then(|x| x.as_u64())
            .map(|x| x.clamp(0, 100) as u8)
            .unwrap_or(40);
        if let Some(t) = v.get("corner_pref").and_then(|t| t.as_str()) {
            if let Some(pref) = CornerPref::parse(t) {
                s.corner_pref = pref;
            }
        }
        if let Some(t) = v.get("border_mode").and_then(|t| t.as_str()) {
            if let Some(mode) = BorderMode::parse(t) {
                s.border_mode = mode;
            }
        }
        // Esc 键行为 / 退格键返回（B1/B2，2026-09-20）：字段缺失（旧版本配置）、
        // 未知字符串或类型损坏 → 默认（返回上一级 / 关），零迁移。
        if let Some(t) = v.get("esc_behavior").and_then(|t| t.as_str()) {
            if let Some(b) = EscBehavior::parse(t) {
                s.esc_behavior = b;
            }
        }
        s.backspace_go_back = v
            .get("backspace_go_back")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        // 着色模式 / 自定义色 / 强度（T6，2026-09-20）：字段缺失、未知字符串、
        // 类型损坏或颜色数组非法 → 默认（系统强调色 / 默认蓝 / 100）。
        if let Some(t) = v.get("colorization").and_then(|t| t.as_str()) {
            if let Some(m) = ColorizationMode::parse(t) {
                s.colorization = m;
            }
        }
        if let Some(arr) = v.get("custom_tint_color").and_then(|x| x.as_array()) {
            let rgb: Option<Vec<u8>> = arr
                .iter()
                .map(|c| c.as_u64().filter(|n| *n <= 255).map(|n| n as u8))
                .collect();
            if let Some(rgb) = rgb {
                if rgb.len() == 3 {
                    s.custom_tint_color = [rgb[0], rgb[1], rgb[2]];
                }
            }
        }
        s.custom_tint_intensity = v
            .get("custom_tint_intensity")
            .and_then(|x| x.as_u64())
            .map(|x| x.clamp(0, 100) as u8)
            .unwrap_or(100);
        // 单击激活 / 界面动效（T7/T8，2026-09-20）：缺失、类型损坏 → 默认 true
        // （= 既有行为），零迁移。
        s.single_click_activation = v
            .get("single_click_activation")
            .and_then(|x| x.as_bool())
            .unwrap_or(true);
        s.ui_animations = v
            .get("ui_animations")
            .and_then(|x| x.as_bool())
            .unwrap_or(true);
        // 全局热键（M6 批次 6.3）：掩码先剔除非法位；剔除后无任何修饰键或字段
        // 缺失/类型损坏 → 回落默认 Win+Alt + Space。
        if let Some(m) = v.get("hotkey_mods").and_then(|m| m.as_u64()) {
            let masked = (m as u32) & HOTKEY_MODS_MASK;
            if masked & 0b1011 != 0 {
                // 至少含 Ctrl/Alt/Win 之一（纯 Shift 不作为热键修饰）
                s.hotkey_mods = masked;
            }
        }
        if let Some(k) = v.get("hotkey_vk").and_then(|k| k.as_u64()) {
            s.hotkey_vk = k as u32;
        }
        // 开机自启 / 停用扩展（M6 批次 6.3）
        if let Some(b) = v.get("autostart").and_then(|b| b.as_bool()) {
            s.autostart = b;
        }
        if let Some(arr) = v.get("disabled_extensions").and_then(|a| a.as_array()) {
            s.disabled_extensions = arr
                .iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect();
        }
        // 面板记忆尺寸（v4.12 D37）：[宽, 高] 逻辑点取整；字段缺失（旧版本
        // 配置）/ 类型损坏 / 数值不合法 → None（= 从未拉伸，用自适应基准）。
        s.panel_size = v
            .get("panel_size")
            .and_then(|p| p.as_array())
            .and_then(|a| {
                let w = a.first()?.as_u64()?;
                let h = a.get(1)?.as_u64()?;
                if (1..=100_000).contains(&w) && (1..=100_000).contains(&h) {
                    Some((w as u32, h as u32))
                } else {
                    None
                }
            });
        // 界面语言（v4.13 D38）：字段缺失（旧版本配置）/ 未知值 → 默认跟随系统。
        if let Some(t) = v.get("lang").and_then(|t| t.as_str()) {
            if let Some(lang) = Lang::parse(t) {
                s.lang = lang;
            }
        }
        // 列表密度（F2）：字段缺失（旧版本配置）/ 未知值 → 默认标准档。
        if let Some(t) = v.get("density").and_then(|t| t.as_str()) {
            if let Some(d) = ListDensity::parse(t) {
                s.density = d;
            }
        }
        // 搜索引擎：字段缺失（旧版本配置）→ 默认（仅 Google，2026-09-12 用户
        // 决策）；字段存在 → 逐条校验，非法条目跳过（空数组 = 用户全部关闭，
        // 尊重其意图）。
        if let Some(val) = v.get("search_engines") {
            if let Some(arr) = val.as_array() {
                s.search_engines = arr
                    .iter()
                    .filter_map(|e| {
                        SearchEngine::new(
                            e.get("name").and_then(|x| x.as_str())?,
                            e.get("template").and_then(|x| x.as_str())?,
                        )
                    })
                    .collect();
            }
        }
        // 搜索应用（2026-09-12）：字段缺失（旧版本配置）/ 类型损坏 → 默认开。
        // （「优先搜索文件」已撤销：配置中的历史字段按未知字段忽略。）
        if let Some(b) = v.get("search_apps").and_then(|b| b.as_bool()) {
            s.search_apps = b;
        }
        s
    }

    /// 序列化为 JSON 文本（单行，便于人查）。
    pub fn to_json_string(&self) -> String {
        let engines: Vec<serde_json::Value> = self
            .search_engines
            .iter()
            .map(|e| serde_json::json!({ "name": e.name, "template": e.template }))
            .collect();
        serde_json::json!({
            "theme": self.theme.as_str(),
            "open_view": self.open_view.as_str(),
            "search_engines": engines,
            "backdrop": self.backdrop.as_str(),
            "material_opacity": self.material_opacity,
            "corner_pref": self.corner_pref.as_str(),
            "border_mode": self.border_mode.as_str(),
            "esc_behavior": self.esc_behavior.as_str(),
            "backspace_go_back": self.backspace_go_back,
            "colorization": self.colorization.as_str(),
            "custom_tint_color": self.custom_tint_color,
            "custom_tint_intensity": self.custom_tint_intensity,
            "single_click_activation": self.single_click_activation,
            "ui_animations": self.ui_animations,
            "hotkey_mods": self.hotkey_mods,
            "hotkey_vk": self.hotkey_vk,
            "autostart": self.autostart,
            "disabled_extensions": self.disabled_extensions,
            "panel_size": match self.panel_size {
                Some((w, h)) => serde_json::json!([w, h]),
                None => serde_json::Value::Null,
            },
            "lang": self.lang.as_str(),
            "density": self.density.as_str(),
            "search_apps": self.search_apps,
        })
        .to_string()
    }

    /// 引擎表 → `DD_WEBSEARCH_ENGINES` 环境变量值（紧凑 JSON 数组）。
    ///
    /// 配置通道 = 进程环境（manifest `entry.env` 既有机制）——协议 v1.0 冻结，
    /// 零协议字段新增；扩展侧未注入/非法时回落其内置默认表。
    pub fn search_engines_env(&self) -> String {
        serde_json::Value::Array(
            self.search_engines
                .iter()
                .map(|e| serde_json::json!({ "name": e.name, "template": e.template }))
                .collect(),
        )
        .to_string()
    }

    /// 从 [`config_file`] 读配置；文件缺失/读盘失败/解析失败 → 默认。
    pub fn load() -> Self {
        let Some(path) = config_file() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse_json(&text),
            Err(e) => {
                // 不存在属首次运行的常态，不算错误；其他读盘失败记日志后回落默认。
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::debug!(
                        "[dd-gui] 配置读取失败（{}）：{e}，回落默认设置",
                        path.display()
                    );
                }
                Self::default()
            }
        }
    }

    /// 写回 [`config_file`]（best-effort：目录不存在则创建；失败仅记日志，
    /// 不阻断 UI——下次启动回落上次成功落盘的值或默认）。
    pub fn save(&self) {
        let Some(path) = config_file() else {
            log::debug!("[dd-gui] 配置目录不可定位，设置未持久化");
            return;
        };
        let dir = path.parent().map(std::path::Path::to_path_buf);
        if let Some(dir) = dir {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                log::warn!("[dd-gui] 配置目录创建失败（{}）：{e}", dir.display());
                return;
            }
        }
        match std::fs::write(&path, self.to_json_string()) {
            Ok(()) => log::info!("[dd-gui] 设置已保存：{}", path.display()),
            Err(e) => log::warn!("[dd-gui] 配置写入失败（{}）：{e}", path.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_pref_json_roundtrip() {
        for pref in [ThemePref::System, ThemePref::Light, ThemePref::Dark] {
            let s = Settings {
                theme: pref,
                ..Settings::default()
            };
            let parsed = Settings::parse_json(&s.to_json_string());
            assert_eq!(parsed, s, "{} 往返一致", pref.label());
        }
    }

    #[test]
    fn parse_json_defaults_on_garbage() {
        // 损坏/空/字段未知/未知值 → 一律回落默认 System，永不失败
        assert_eq!(Settings::parse_json(""), Settings::default());
        assert_eq!(Settings::parse_json("not json"), Settings::default());
        assert_eq!(Settings::parse_json("{}"), Settings::default());
        assert_eq!(
            Settings::parse_json(r#"{"theme": 42}"#),
            Settings::default()
        );
        assert_eq!(
            Settings::parse_json(r#"{"theme": "neon"}"#),
            Settings::default(),
            "未知主题值回落默认"
        );
        // 亮/暗可正确解析
        assert_eq!(
            Settings::parse_json(r#"{"theme": "dark"}"#).theme,
            ThemePref::Dark
        );
        assert_eq!(
            Settings::parse_json(r#"{"theme": "light"}"#).theme,
            ThemePref::Light
        );
    }

    #[test]
    fn parse_json_tolerates_unknown_fields() {
        // 向后兼容：多出的字段忽略不报错
        let s = Settings::parse_json(r#"{"theme":"dark","future":"x"}"#);
        assert_eq!(s.theme, ThemePref::Dark);
    }

    #[test]
    fn open_view_json_roundtrip_and_default() {
        // 默认 = Default（首屏默认功能，不铺全部应用——真机反馈 2026-09-04）
        assert_eq!(Settings::default().open_view, OpenView::Default);
        assert_eq!(Settings::parse_json("{}").open_view, OpenView::Default);
        for view in [OpenView::Default, OpenView::All] {
            let s = Settings {
                open_view: view,
                ..Settings::default()
            };
            assert_eq!(
                Settings::parse_json(&s.to_json_string()).open_view,
                view,
                "{} 往返一致",
                view.label()
            );
        }
        // 未知值回落默认
        assert_eq!(
            Settings::parse_json(r#"{"open_view":"neon"}"#).open_view,
            OpenView::Default
        );
    }

    #[test]
    fn settings_page_id_has_reserved_prefix() {
        // 设置页 id 属 GUI 保留值，不得与协议 page_id 命名空间混淆：
        // 协议 id 由扩展提供（§6.3），约定不含双下划线保留前缀。
        assert!(SETTINGS_PAGE_ID.starts_with("__"));
        assert_eq!(SETTINGS_PAGE_ID, "__settings__");
    }

    #[test]
    fn search_engines_default_is_google_only() {
        // 2026-09-12 用户决策：默认只启用 Google；其余预设可在设置页手动添加。
        let defaults = Settings::default().search_engines;
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].name, "Google");
        // 配置缺字段（含 "{}" 与旧版本配置）→ 同样回落仅 Google
        assert_eq!(Settings::parse_json("{}").search_engines, defaults);
        assert_eq!(
            Settings::parse_json(r#"{"theme":"dark"}"#).search_engines,
            defaults
        );
        // 可添加目录仍是完整 5 预设（设置页下拉用）
        assert_eq!(preset_search_engines().len(), 5);
        assert_eq!(default_search_engines(), defaults);
    }

    #[test]
    fn search_engines_json_roundtrip_with_custom() {
        let mut s = Settings::default();
        s.search_engines.retain(|e| e.name == "Baidu");
        s.search_engines.push(
            SearchEngine::new("Stack Overflow", "https://stackoverflow.com/search?q={q}").unwrap(),
        );
        let parsed = Settings::parse_json(&s.to_json_string());
        assert_eq!(parsed, s);
    }

    #[test]
    fn search_engines_invalid_entries_skipped_and_empty_respected() {
        // 非法条目（缺字段 / 缺 {q} / 非 http）逐条跳过
        let parsed = Settings::parse_json(
            r#"{"search_engines":[
                {"name":"Good","template":"https://a.com/?q={q}"},
                {"name":"NoQ","template":"https://b.com/"},
                {"template":"https://c.com/?q={q}"},
                {"name":"Ftp","template":"ftp://d.com/?q={q}"}
            ]}"#,
        );
        assert_eq!(parsed.search_engines.len(), 1);
        assert_eq!(parsed.search_engines[0].name, "Good");
        // 空数组 = 用户全部关闭（尊重意图，不回落预设）
        assert!(Settings::parse_json(r#"{"search_engines":[]}"#)
            .search_engines
            .is_empty());
        // 字段类型损坏（非数组）→ 保持默认（仅 Google）
        assert_eq!(
            Settings::parse_json(r#"{"search_engines":42}"#).search_engines,
            default_search_engines()
        );
    }

    #[test]
    fn search_engines_env_is_compact_json_array() {
        let s = Settings::parse_json(
            r#"{"search_engines":[{"name":"Bing","template":"https://www.bing.com/search?q={q}"}]}"#,
        );
        let env = s.search_engines_env();
        assert_eq!(
            env,
            r#"[{"name":"Bing","template":"https://www.bing.com/search?q={q}"}]"#
        );
        assert!(Settings::default().search_engines_env().starts_with('['));
    }

    #[test]
    fn search_engine_new_validates() {
        assert!(SearchEngine::new("", "https://a.com/?q={q}").is_none());
        assert!(
            SearchEngine::new("X", "https://a.com/").is_none(),
            "缺 {{q}}"
        );
        assert!(SearchEngine::new("X", "ftp://a.com/?q={q}").is_none());
        let e = SearchEngine::new("  Bing  ", " https://a.com/?q={q} ").unwrap();
        assert_eq!(e.name, "Bing");
        assert_eq!(e.template, "https://a.com/?q={q}");
    }

    #[test]
    fn backdrop_default_is_mica_and_roundtrips() {
        // v4.7 D30：默认云母；三值往返一致；未知值回落云母
        assert_eq!(Settings::default().backdrop, Backdrop::Mica);
        assert_eq!(Settings::parse_json("{}").backdrop, Backdrop::Mica);
        // 旧版本配置（无 backdrop 字段）→ 云母，其余字段正常解析
        let old = Settings::parse_json(r#"{"theme":"dark","open_view":"all"}"#);
        assert_eq!(old.backdrop, Backdrop::Mica);
        assert_eq!(old.theme, ThemePref::Dark);
        assert_eq!(old.open_view, OpenView::All);
        for b in Backdrop::ALL {
            let s = Settings {
                backdrop: b,
                ..Settings::default()
            };
            let parsed = Settings::parse_json(&s.to_json_string());
            assert_eq!(parsed.backdrop, b, "{} 往返一致", b.label());
        }
        // M2（2026-09-20）：云母 Alt 档往返 + 稳定标识
        assert_eq!(Backdrop::MicaAlt.as_str(), "mica_alt");
        assert_eq!(Backdrop::parse("mica_alt"), Some(Backdrop::MicaAlt));
        assert_eq!(
            Settings::parse_json(r#"{"backdrop":"mica_alt"}"#).backdrop,
            Backdrop::MicaAlt
        );
        // 未知值 / 类型损坏 → 回落默认云母
        assert_eq!(
            Settings::parse_json(r#"{"backdrop":"frosted"}"#).backdrop,
            Backdrop::Mica
        );
        assert_eq!(
            Settings::parse_json(r#"{"backdrop":42}"#).backdrop,
            Backdrop::Mica
        );
    }

    /// B1/B2（2026-09-20）：Esc 行为与退格键返回——默认、往返、未知回落，
    /// 以及 `decide` 的完整决策矩阵。
    #[test]
    fn esc_behavior_and_backspace_defaults_roundtrip_and_decide() {
        let d = Settings::default();
        assert_eq!(d.esc_behavior, EscBehavior::GoBack, "默认 = 既有行为");
        assert!(!d.backspace_go_back, "默认关 = 既有行为");
        for b in EscBehavior::ALL {
            let s = Settings {
                esc_behavior: b,
                backspace_go_back: true,
                ..Settings::default()
            };
            let parsed = Settings::parse_json(&s.to_json_string());
            assert_eq!(parsed.esc_behavior, b, "{} 往返一致", b.label());
            assert!(parsed.backspace_go_back, "退格开关往返一致");
        }
        assert_eq!(
            Settings::parse_json(r#"{"esc_behavior":"nope"}"#).esc_behavior,
            EscBehavior::GoBack
        );
        assert_eq!(
            Settings::parse_json(r#"{"esc_behavior":7}"#).esc_behavior,
            EscBehavior::GoBack
        );
        assert!(
            !Settings::parse_json(r#"{"backspace_go_back":"yes"}"#).backspace_go_back,
            "类型损坏 → 默认关"
        );
        // 决策矩阵：三档 × {Root, 非 Root} × {空, 非空}
        use EscAction::{ClearSearch, GoBack as GoBackAct, Hide};
        let m = |b: EscBehavior, root: bool, empty: bool| b.decide(root, empty);
        assert_eq!(m(EscBehavior::GoBack, true, false), Hide, "Root → 隐藏");
        assert_eq!(m(EscBehavior::GoBack, false, false), GoBackAct);
        assert_eq!(m(EscBehavior::GoBack, false, true), GoBackAct);
        assert_eq!(
            m(EscBehavior::ClearThenGoBack, false, false),
            ClearSearch,
            "非空 → 先清搜索"
        );
        assert_eq!(
            m(EscBehavior::ClearThenGoBack, true, false),
            ClearSearch,
            "非空优先于层级"
        );
        assert_eq!(m(EscBehavior::ClearThenGoBack, false, true), GoBackAct);
        assert_eq!(m(EscBehavior::ClearThenGoBack, true, true), Hide);
        assert_eq!(m(EscBehavior::AlwaysHide, false, false), Hide);
        assert_eq!(m(EscBehavior::AlwaysHide, false, true), Hide);
        assert_eq!(m(EscBehavior::AlwaysHide, true, false), Hide);
    }

    /// T6（2026-09-20）：着色三档——默认、往返、未知/非法回落、强度 clamp。
    #[test]
    fn colorization_defaults_roundtrip_and_sanitize() {
        let d = Settings::default();
        assert_eq!(
            d.colorization,
            ColorizationMode::SystemAccent,
            "默认 = 既有行为"
        );
        assert_eq!(d.custom_tint_color, CUSTOM_TINT_DEFAULT);
        assert_eq!(d.custom_tint_intensity, 100);
        for m in ColorizationMode::ALL {
            let s = Settings {
                colorization: m,
                custom_tint_color: [12, 34, 56],
                custom_tint_intensity: 42,
                ..Settings::default()
            };
            let parsed = Settings::parse_json(&s.to_json_string());
            assert_eq!(parsed.colorization, m, "{} 往返一致", m.label());
            assert_eq!(parsed.custom_tint_color, [12, 34, 56]);
            assert_eq!(parsed.custom_tint_intensity, 42);
        }
        let bad = Settings::parse_json(
            r#"{"colorization":"rainbow","custom_tint_color":[1,2],"custom_tint_intensity":300}"#,
        );
        assert_eq!(
            bad.colorization,
            ColorizationMode::SystemAccent,
            "未知模式 → 默认"
        );
        assert_eq!(
            bad.custom_tint_color, CUSTOM_TINT_DEFAULT,
            "长度不符 → 默认色"
        );
        assert_eq!(bad.custom_tint_intensity, 100, "越界 clamp 到 100");
        let bad2 =
            Settings::parse_json(r#"{"custom_tint_color":[1,2,"x"],"custom_tint_intensity":-5}"#);
        assert_eq!(
            bad2.custom_tint_color, CUSTOM_TINT_DEFAULT,
            "元素非法 → 默认色"
        );
        // 负数不是 u64 → 走「类型损坏 → 默认 100」口径（与 material_opacity 一致），
        // 而非 clamp 到 0；越界**正数**才 clamp（上方 300 → 100 已覆盖）
        assert_eq!(
            bad2.custom_tint_intensity, 100,
            "负数按类型损坏口径回落默认"
        );
    }

    /// T7/T8（2026-09-20）：单击激活与界面动效——默认 true（= 既有行为）、
    /// 往返一致、类型损坏回落 true。
    #[test]
    fn click_and_animation_defaults_roundtrip() {
        let d = Settings::default();
        assert!(d.single_click_activation, "默认单击激活（既有行为）");
        assert!(d.ui_animations, "默认开动效（既有行为）");
        let s = Settings {
            single_click_activation: false,
            ui_animations: false,
            ..Settings::default()
        };
        let parsed = Settings::parse_json(&s.to_json_string());
        assert!(!parsed.single_click_activation, "关档往返一致");
        assert!(!parsed.ui_animations, "关档往返一致");
        let bad = Settings::parse_json(r#"{"single_click_activation":"yes","ui_animations":3}"#);
        assert!(bad.single_click_activation, "类型损坏 → 默认开");
        assert!(bad.ui_animations, "类型损坏 → 默认开");
        // 旧配置（两字段缺失）→ 默认开，其余字段不受影响
        let old = Settings::parse_json(r#"{"theme":"dark"}"#);
        assert!(old.single_click_activation && old.ui_animations);
    }

    // ── P2–P4（窗口材质与边框方案，2026-09-13）────────────────────────

    #[test]
    fn material_window_defaults_and_roundtrip() {
        // 默认值：40 / 圆角 / 中性（40% = M1 真机调定锚点观感）
        assert_eq!(Settings::default().material_opacity, 40);
        assert_eq!(Settings::default().corner_pref, CornerPref::Round);
        assert_eq!(Settings::default().border_mode, BorderMode::Neutral);
        // 旧版本配置（三个字段均缺失）→ 默认值
        let old = Settings::parse_json(r#"{"backdrop":"acrylic"}"#);
        assert_eq!(old.material_opacity, 40);
        assert_eq!(old.corner_pref, CornerPref::Round);
        assert_eq!(old.border_mode, BorderMode::Neutral);
        // 合法值往返
        let s = Settings {
            material_opacity: 40,
            corner_pref: CornerPref::Small,
            border_mode: BorderMode::Accent,
            ..Settings::default()
        };
        let parsed = Settings::parse_json(&s.to_json_string());
        assert_eq!(parsed.material_opacity, 40);
        assert_eq!(
            parsed.corner_pref,
            CornerPref::Small,
            "{} 往返一致",
            CornerPref::Small.label()
        );
        assert_eq!(
            parsed.border_mode,
            BorderMode::Accent,
            "{} 往返一致",
            BorderMode::Accent.label()
        );
    }

    #[test]
    fn material_opacity_out_of_range_and_type_corruption() {
        // 越界 clamp 到 0–100（滑杆口径）；负数/类型损坏 → 默认 40
        assert_eq!(
            Settings::parse_json(r#"{"material_opacity":57}"#).material_opacity,
            57
        );
        assert_eq!(
            Settings::parse_json(r#"{"material_opacity":0}"#).material_opacity,
            0
        );
        assert_eq!(
            Settings::parse_json(r#"{"material_opacity":100}"#).material_opacity,
            100
        );
        assert_eq!(
            Settings::parse_json(r#"{"material_opacity":255}"#).material_opacity,
            100
        );
        assert_eq!(
            Settings::parse_json(r#"{"material_opacity":-3}"#).material_opacity,
            40
        );
        assert_eq!(
            Settings::parse_json(r#"{"material_opacity":"half"}"#).material_opacity,
            40
        );
    }

    #[test]
    fn corner_and_border_unknown_values_fall_back() {
        // 未知字符串 → 默认（与 backdrop 未知回落口径一致）；合法值单独解析
        assert_eq!(
            Settings::parse_json(r#"{"corner_pref":"huge"}"#).corner_pref,
            CornerPref::Round
        );
        assert_eq!(
            Settings::parse_json(r#"{"border_mode":"thick"}"#).border_mode,
            BorderMode::Neutral
        );
        assert_eq!(
            Settings::parse_json(r#"{"corner_pref":"square"}"#).corner_pref,
            CornerPref::Square
        );
        assert_eq!(
            Settings::parse_json(r#"{"border_mode":"none"}"#).border_mode,
            BorderMode::None
        );
        // 类型损坏 → 默认
        assert_eq!(
            Settings::parse_json(r#"{"corner_pref":3}"#).corner_pref,
            CornerPref::Round
        );
        assert_eq!(
            Settings::parse_json(r#"{"border_mode":true}"#).border_mode,
            BorderMode::Neutral
        );
    }

    #[test]
    fn hotkey_fields_default_sanitize_and_roundtrip() {
        // M6 批次 6.3：默认 Win+Alt + Space；掩码剔除非法位；纯 Shift 无效回落
        let s = Settings::parse_json(
            r#"{"hotkey_mods":10,"hotkey_vk":80}"#, // Ctrl(2)+Win(8) + 'P'
        );
        assert_eq!(s.hotkey_mods, 0b1010);
        assert_eq!(s.hotkey_vk, 80);
        assert_eq!(hotkey_mods_label(s.hotkey_mods), "Ctrl+Win");
        assert_eq!(hotkey_vk_label(80), "P");
        // 纯 Shift（4）→ 无 Ctrl/Alt/Win → 回落默认；字段缺失 → 默认
        assert_eq!(
            Settings::parse_json(r#"{"hotkey_mods":4}"#).hotkey_mods,
            HOTKEY_MODS_DEFAULT
        );
        assert_eq!(Settings::parse_json("{}").hotkey_mods, HOTKEY_MODS_DEFAULT);
        assert_eq!(Settings::parse_json("{}").hotkey_vk, HOTKEY_VK_DEFAULT);
        // 往返 + 非法位剔除
        let s2 = Settings {
            hotkey_mods: 0b1010 | 0b0100_0000, // 含非法位 64
            ..Settings::default()
        };
        let parsed = Settings::parse_json(&s2.to_json_string());
        assert_eq!(parsed.hotkey_mods, 0b1010, "非法位被剔除");
        // Space 标签 + F 键标签
        assert_eq!(hotkey_vk_label(0x20), "Space");
        assert_eq!(hotkey_vk_label(0x70), "F1");
    }

    #[test]
    fn autostart_and_disabled_extensions_roundtrip() {
        // M6 批次 6.3：开机自启默认关、停用扩展默认空；往返一致；类型损坏回落
        assert!(!Settings::default().autostart);
        assert!(Settings::default().disabled_extensions.is_empty());
        let s = Settings {
            autostart: true,
            disabled_extensions: vec!["com.ddrun.calc".into()],
            ..Settings::default()
        };
        let parsed = Settings::parse_json(&s.to_json_string());
        assert!(parsed.autostart);
        assert_eq!(parsed.disabled_extensions, vec!["com.ddrun.calc"]);
        // 字段缺失 → 默认；类型损坏 → 默认
        let old = Settings::parse_json(r#"{"theme":"dark"}"#);
        assert!(!old.autostart);
        assert!(old.disabled_extensions.is_empty());
        assert!(!Settings::parse_json(r#"{"autostart":"yes"}"#).autostart);
        assert!(Settings::parse_json(r#"{"disabled_extensions":42}"#)
            .disabled_extensions
            .is_empty());
    }

    #[test]
    fn lang_roundtrip_and_unknown_fallback() {
        // v4.13 D38：默认跟随系统；往返一致；未知值/旧配置缺字段 → 默认
        assert_eq!(Settings::default().lang, Lang::FollowSystem);
        let s = Settings {
            lang: Lang::EnUs,
            ..Settings::default()
        };
        let parsed = Settings::parse_json(&s.to_json_string());
        assert_eq!(parsed.lang, Lang::EnUs, "lang 往返保留");
        assert_eq!(
            Settings::parse_json(r#"{"lang":"fr_fr"}"#).lang,
            Lang::FollowSystem,
            "未知值回落跟随系统"
        );
        assert_eq!(Settings::parse_json("{}").lang, Lang::FollowSystem);
        assert_eq!(Lang::parse("zh_cn"), Some(Lang::ZhCn));
        assert_eq!(Lang::parse("follow_system"), Some(Lang::FollowSystem));
        assert_eq!(Lang::parse("nope"), None);
        assert_eq!(Lang::ZhCn.as_str(), "zh_cn");
        assert_eq!(Lang::EnUs.as_str(), "en_us");
    }

    #[test]
    fn density_default_missing_and_roundtrip() {
        // F2：默认标准档；字段缺失（旧版本配置）/ 未知值 / 类型损坏 → 标准档
        assert_eq!(Settings::default().density, ListDensity::Standard);
        assert_eq!(Settings::parse_json("{}").density, ListDensity::Standard);
        let old = Settings::parse_json(r#"{"lang":"en_us"}"#);
        assert_eq!(old.density, ListDensity::Standard, "旧配置无该字段");
        assert_eq!(old.lang, Lang::EnUs, "其余字段解析不受影响");
        assert_eq!(
            Settings::parse_json(r#"{"density":"roomy"}"#).density,
            ListDensity::Standard
        );
        assert_eq!(
            Settings::parse_json(r#"{"density":3}"#).density,
            ListDensity::Standard
        );
        // 三档往返一致
        for d in [
            ListDensity::Compact,
            ListDensity::Standard,
            ListDensity::Relaxed,
        ] {
            let s = Settings {
                density: d,
                ..Settings::default()
            };
            let parsed = Settings::parse_json(&s.to_json_string());
            assert_eq!(parsed, s, "{} 往返一致", d.label());
        }
        assert_eq!(ListDensity::parse("compact"), Some(ListDensity::Compact));
        assert_eq!(ListDensity::parse("relaxed"), Some(ListDensity::Relaxed));
        assert_eq!(ListDensity::parse("nope"), None);
    }

    #[test]
    fn search_apps_roundtrip() {
        // 2026-09-12：默认开；字段缺失（旧版本配置）/ 类型损坏 → 默认开；
        // 显式值往返一致。（「优先搜索文件」已撤销——配置中残留的该字段按
        // 未知字段忽略，不报错。）
        assert!(Settings::default().search_apps);
        assert!(Settings::parse_json("{}").search_apps);
        assert!(Settings::parse_json(r#"{"prioritize_files":true}"#).search_apps);
        let s = Settings {
            search_apps: false,
            ..Settings::default()
        };
        let parsed = Settings::parse_json(&s.to_json_string());
        assert_eq!(parsed, s, "search_apps 往返一致");
        // 类型损坏（非布尔）→ 回落默认开
        assert!(Settings::parse_json(r#"{"search_apps":1}"#).search_apps);
    }

    #[test]
    fn panel_size_default_missing_and_roundtrip() {
        // v4.12 D37：默认 / 字段缺失（旧版本配置）/ 类型损坏 / 越界值 → None
        assert_eq!(Settings::default().panel_size, None);
        assert_eq!(Settings::parse_json("{}").panel_size, None);
        assert_eq!(
            Settings::parse_json(r#"{"theme":"dark"}"#).panel_size,
            None,
            "旧版本配置无该字段"
        );
        assert_eq!(
            Settings::parse_json(r#"{"panel_size":42}"#).panel_size,
            None
        );
        assert_eq!(
            Settings::parse_json(r#"{"panel_size":[1]}"#).panel_size,
            None,
            "缺高度"
        );
        assert_eq!(
            Settings::parse_json(r#"{"panel_size":[0,400]}"#).panel_size,
            None,
            "非法宽度"
        );
        assert_eq!(
            Settings::parse_json(r#"{"panel_size":[999999,400]}"#).panel_size,
            None,
            "越界尺寸"
        );
        // 往返一致（含 None 与 Some 两态）
        let s = Settings {
            panel_size: Some((900, 700)),
            ..Settings::default()
        };
        let parsed = Settings::parse_json(&s.to_json_string());
        assert_eq!(parsed.panel_size, Some((900, 700)), "Some 往返一致");
        assert!(
            Settings::default()
                .to_json_string()
                .contains("\"panel_size\":null"),
            "None 序列化为 null"
        );
    }
}
