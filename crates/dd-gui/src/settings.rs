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

// O12（2026-10-11）：巨型文件拆分——设置枚举族（主题/材质/行为/语言/密度/背景图）
// 拆入 prefs.rs，搜索引擎表拆入 search.rs，单测拆入 tests.rs；本文件保留 Settings
// 结构体、Default、load/save 与备份/热键工具。纪律：搬运单位 = 完整定义块（含
// 文档注释），函数体一字不改；经 `pub use` 重导出保持 `crate::settings::*`
// 外部路径不变。

use std::collections::BTreeMap;

use dd_host::manifest::config_file;

/// 设置页在页面栈中的 `page_id` 标记（`PageState::page_id` 的保留值，
/// 不会与协议 `page_id` 冲突：协议 id 来自扩展，无此双下划线保留前缀）。
pub const SETTINGS_PAGE_ID: &str = "__settings__";

mod prefs;
mod search;

pub use prefs::*;
pub use search::*;

#[cfg(test)]
mod tests;

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
    /// 背景图路径（T9，2026-10-05）：`Some` = 该图即面板背景（**互斥语义**
    /// ——材质 / 着色 / 边框链路暂停生效，清除后恢复）；`None` = 无背景图
    /// （默认 = 既有行为）。trim 后空串在解析与构造处均归一为 `None`。
    pub background_image_path: Option<String>,
    /// 背景图不透明度百分比（T9；0–100，默认 20 = 对齐 CmdPal
    /// `BackgroundImageOpacity`，低不透明度保证行内容可读）。
    pub background_image_opacity: u8,
    /// 背景图适应方式（T9；默认 Fill = 等比裁剪铺满，对齐 CmdPal 档位）。
    pub background_image_fit: BgImageFit,
    /// 背景图着色强度（T9；0–100，默认 0 = 不叠色）：面板色按该比例叠加
    /// 在图上，用于压制高亮图片的干扰。
    pub background_image_tint_intensity: u8,
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
    /// 搜索 Steam 游戏（2026-10-07 新增）：关闭后根页结果不包含 Steam 游戏
    /// 条目（扩展侧 `steam` 机器标签判据；空查询首屏与关键词匹配均排除）。
    /// 默认关——Steam 游戏默认不参与常规搜索（用户需求：游戏不混入应用
    /// 搜索结果），开关打开后恢复参与。
    pub search_steam_games: bool,
    /// warm 进程池 LRU 保活容量（N4，2026-10-03；1–16，默认 8 = 原
    /// `LRU_WARM_CAPACITY` 编译期常量值）。超出容量时最久未用的扩展被
    /// close + 回落 stub；运行时调小经 `LruWarmSet::set_capacity` 立即驱逐。
    pub warm_capacity: u8,
    /// 自定义直达命令（N1，2026-10-04）：关键词 → URL / 本地路径。聚合期
    /// 转为宿主虚拟条目进首屏；默认空 = 无直达命令。
    pub custom_commands: Vec<CustomCommand>,
    /// 内置扩展用户可调配置（N2，2026-10-04）：`ext_id → (键 → 值)`。
    /// 聚合期经 `aggregator::inject_ext_settings` 注入扩展 `entry.env` 内存
    /// 副本（变量名 `DD_EXT_CFG_<KEY 大写>`）；in-process 内置另走内存通道
    /// 直接注入（websearch 引擎表先例）。值 v1 一律字符串（设计稿的
    /// `serde_json::Value` 草型收窄——消费者当前只需字符串，且 `Value` 无
    /// `Eq` 会破坏 `Settings` 的 derive）；BTreeMap 保证序列化确定性。
    pub ext_settings: BTreeMap<String, BTreeMap<String, String>>,
}

/// 全局热键默认修饰键：Win + Alt（MOD_* 值：ALT=1/CONTROL=2/SHIFT=4/WIN=8，
/// 与 hotkey.rs 的 windows-sys 常量一致；本 crate 纯逻辑不依赖 windows-sys，
/// 用字面量 + 单测锚定）。不含 NOREPEAT——注册时由热键线程统一补。
pub const HOTKEY_MODS_DEFAULT: u32 = 0b1000 | 0b0001;
/// 全局热键默认主键：VK_SPACE。
pub const HOTKEY_VK_DEFAULT: u32 = 0x20;
/// 修饰键合法位掩码（Ctrl/Alt/Shift/Win），解析时剔除其余位。
pub const HOTKEY_MODS_MASK: u32 = 0b1111;

/// N1（2026-10-04）：自定义直达命令的目标类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomCommandKind {
    /// URL（http/https 为主；执行时经 S-03 scheme 白名单校验）。
    Url,
    /// 本地路径（含 UNC；执行走 ShellExecute「双击等价」）。
    Path,
}

impl CustomCommandKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CustomCommandKind::Url => "url",
            CustomCommandKind::Path => "path",
        }
    }
    /// 未知字符串 → `None`（调用方回落/跳过，与 ThemePref 同口径）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "url" => Some(CustomCommandKind::Url),
            "path" => Some(CustomCommandKind::Path),
            _ => None,
        }
    }
}

/// N1（2026-10-04）：自定义直达命令（关键词 → URL / 本地路径）。
///
/// 聚合期转为宿主虚拟条目（`aggregator::custom_command_items`）：`keyword`
/// 并入条目 `tags`、`title` 生成拼音索引——输入即搜、选中即执行；无前缀
/// 语法、不带参数（`{q}` 带参直达与已移除的 `f ` 前缀同形，判缓办——见
/// future-features-plan §5）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomCommand {
    /// 显示名（首屏条目标题，生成拼音索引）。
    pub title: String,
    /// 匹配别名（并入条目 tags；首屏唯一——UI 添加时拒绝重复）。
    pub keyword: String,
    /// 目标类型。
    pub kind: CustomCommandKind,
    /// 目标（URL 或本地路径字符串）。
    pub target: String,
}

impl CustomCommand {
    /// 构造并规范化：三字段 trim、去空；keyword 转小写且**不得含空白**
    /// （关键词是单 token 别名）；任一为空 → `None`。
    pub fn new(title: &str, keyword: &str, kind: CustomCommandKind, target: &str) -> Option<Self> {
        let title = title.trim();
        let keyword = keyword.trim().to_lowercase();
        let target = target.trim();
        if title.is_empty() || keyword.is_empty() || target.is_empty() {
            return None;
        }
        if keyword.split_whitespace().count() != 1 {
            return None;
        }
        Some(Self {
            title: title.to_string(),
            keyword,
            kind,
            target: target.to_string(),
        })
    }
}

/// warm 保活容量默认值（N4：= 原 `pool.rs LRU_WARM_CAPACITY` 编译期常量，
/// M1–M4 内存基线在该默认下维持有效）。
pub const WARM_CAPACITY_DEFAULT: u8 = 8;
/// warm 保活容量下限（N4：LRU 容量至少为 1）。
pub const WARM_CAPACITY_MIN: u8 = 1;
/// warm 保活容量上限（N4：防误设超大值无谓常驻扩展进程）。
pub const WARM_CAPACITY_MAX: u8 = 16;

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
            background_image_path: None,
            background_image_opacity: BG_IMAGE_OPACITY_DEFAULT,
            background_image_fit: BgImageFit::Fill,
            background_image_tint_intensity: BG_IMAGE_TINT_DEFAULT,
            hotkey_mods: HOTKEY_MODS_DEFAULT,
            hotkey_vk: HOTKEY_VK_DEFAULT,
            autostart: false,
            disabled_extensions: Vec::new(),
            panel_size: None,
            lang: Lang::default(),
            density: ListDensity::default(),
            search_apps: true,
            search_steam_games: false,
            warm_capacity: WARM_CAPACITY_DEFAULT,
            custom_commands: Vec::new(),
            ext_settings: BTreeMap::new(),
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

/// R-19（2026-09-30）：按当前热键设置拼装**完整组合名**（如 `Ctrl+Shift+P`）
/// ——托盘 tooltip 与菜单尾缀动态化共用（D38 静态口径的后续）。纯函数。
pub fn hotkey_combo_label(mods: u32, vk: u32) -> String {
    format!("{}+{}", hotkey_mods_label(mods), hotkey_vk_label(vk))
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
        // 背景图（T9，2026-10-05）：字段缺失（旧版本配置）→ None（零迁移）；
        // 非字符串 / trim 后空串 → None；不透明度与着色强度越界 clamp 0–100、
        // 类型损坏回落默认；适应方式未知字符串回落 Fill。
        s.background_image_path = v
            .get("background_image_path")
            .and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(String::from);
        s.background_image_opacity = v
            .get("background_image_opacity")
            .and_then(|x| x.as_u64())
            .map(|x| x.clamp(0, 100) as u8)
            .unwrap_or(BG_IMAGE_OPACITY_DEFAULT);
        if let Some(t) = v.get("background_image_fit").and_then(|t| t.as_str()) {
            if let Some(fit) = BgImageFit::parse(t) {
                s.background_image_fit = fit;
            }
        }
        s.background_image_tint_intensity = v
            .get("background_image_tint_intensity")
            .and_then(|x| x.as_u64())
            .map(|x| x.clamp(0, 100) as u8)
            .unwrap_or(BG_IMAGE_TINT_DEFAULT);
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
        // 搜索 Steam 游戏（2026-10-07）：字段缺失（旧版本配置）/ 类型损坏 →
        // 默认关（Steam 游戏默认不参与搜索）。
        if let Some(b) = v.get("search_steam_games").and_then(|b| b.as_bool()) {
            s.search_steam_games = b;
        }
        // warm 保活容量（N4，2026-10-03）：字段缺失（旧版本配置）/ 类型损坏 →
        // 默认 8；数值越界 clamp 到 1–16（与 material_opacity 同口径）。
        s.warm_capacity = v
            .get("warm_capacity")
            .and_then(|x| x.as_u64())
            .map(|x| x.clamp(WARM_CAPACITY_MIN as u64, WARM_CAPACITY_MAX as u64) as u8)
            .unwrap_or(WARM_CAPACITY_DEFAULT);
        // 自定义直达命令（N1，2026-10-04）：字段缺失（旧版本配置）→ 默认空；
        // 逐条经 `CustomCommand::new` 校验规范化，非法条目跳过（与搜索引擎
        // 同口径）。
        if let Some(arr) = v.get("custom_commands").and_then(|a| a.as_array()) {
            s.custom_commands = arr
                .iter()
                .filter_map(|e| {
                    CustomCommand::new(
                        e.get("title").and_then(|x| x.as_str())?,
                        e.get("keyword").and_then(|x| x.as_str())?,
                        e.get("kind")
                            .and_then(|x| x.as_str())
                            .and_then(CustomCommandKind::parse)?,
                        e.get("target").and_then(|x| x.as_str())?,
                    )
                })
                .collect();
        }
        // 内置扩展用户可调配置（N2，2026-10-04）：字段缺失（旧版本配置）→
        // 默认空；逐条校验（ext_id 非空、值仅收字符串），非字符串值跳过。
        if let Some(obj) = v.get("ext_settings").and_then(|x| x.as_object()) {
            for (ext_id, kv) in obj {
                if ext_id.is_empty() {
                    continue;
                }
                let Some(kv_obj) = kv.as_object() else {
                    continue;
                };
                let entry: BTreeMap<String, String> = kv_obj
                    .iter()
                    .filter_map(|(k, val)| val.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect();
                if !entry.is_empty() {
                    s.ext_settings.insert(ext_id.clone(), entry);
                }
            }
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
            "background_image_path": self.background_image_path,
            "background_image_opacity": self.background_image_opacity,
            "background_image_fit": self.background_image_fit.as_str(),
            "background_image_tint_intensity": self.background_image_tint_intensity,
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
            "search_steam_games": self.search_steam_games,
            "warm_capacity": self.warm_capacity,
            "custom_commands": self
                .custom_commands
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "title": c.title,
                        "keyword": c.keyword,
                        "kind": c.kind.as_str(),
                        "target": c.target,
                    })
                })
                .collect::<Vec<_>>(),
            "ext_settings": self.ext_settings,
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
        match config_file() {
            Some(path) => Self::load_from(&path),
            None => Self::default(),
        }
    }

    /// R-14（2026-09-29）：config.json 读盘体积上限。校验此前发生在**读入
    /// 之后**——超限 JSON 会先整读再解析失败 → 启动期 OOM/长挂。超限视为
    /// 损坏：记日志后回落默认（`load_from` 参数化便于单测注入路径）。
    const CONFIG_MAX_BYTES: u64 = 1024 * 1024;

    fn load_from(path: &std::path::Path) -> Self {
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() > Self::CONFIG_MAX_BYTES {
                log::warn!(
                    "[dd-gui] config.json {} 字节超过 {} 字节上限，视为损坏回落默认（R-14 限幅）",
                    meta.len(),
                    Self::CONFIG_MAX_BYTES
                );
                return Self::default();
            }
        }
        match std::fs::read_to_string(path) {
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
    /// R-02：经 [`atomic_write`] 原子落盘，写盘中途崩溃不再丢全部设置。
    /// R-17：返回是否成功持久化（false = 目录不可定位或写盘失败），供调用方
    /// 做「每会话首次失败」的用户可见反馈（静默会「看似成功」、重启回滚）。
    pub fn save(&self) -> bool {
        match config_file() {
            Some(path) => self.save_to(&path),
            None => {
                log::debug!("[dd-gui] 配置目录不可定位，设置未持久化");
                false
            }
        }
    }

    /// [`save`] 的可注入核心（单测不触真实 config.json）。
    fn save_to(&self, path: &std::path::Path) -> bool {
        match atomic_write(path, self.to_json_string().as_bytes()) {
            Ok(()) => {
                log::info!("[dd-gui] 设置已保存：{}", path.display());
                true
            }
            Err(e) => {
                log::warn!("[dd-gui] 配置写入失败（{}）：{e}", path.display());
                false
            }
        }
    }

    /// N5（2026-10-04）：导出为**迁移备份** JSON 文本——全量字段减去机器态
    ///（`autostart`：注册表状态随机器；`panel_size`：分辨率相关——跨机无
    /// 意义），并写 [`BACKUP_VERSION_KEY`] = 宿主版本号。`trust.json` 永不在
    /// 此（S-05 fail-closed：信任判定绑定本机清单/exe 哈希，导出只会制造
    /// 虚假迁移预期，spec §4.5）。
    pub fn export_backup(&self) -> String {
        let mut v: serde_json::Value =
            serde_json::from_str(&self.to_json_string()).expect("to_json_string 恒产出合法 JSON");
        let obj = v.as_object_mut().expect("同上，恒为对象");
        obj.remove("autostart");
        obj.remove("panel_size");
        obj.insert(
            BACKUP_VERSION_KEY.to_string(),
            serde_json::json!(crate::aggregator::HOST_VERSION),
        );
        v.to_string()
    }

    /// N5：备份落盘（原子写，同 [`Self::save`] 的 R-02 机制；返回成败供
    /// 调用方 toast——失败静默会「看似成功」）。
    pub fn save_backup_to(&self, path: &std::path::Path) -> bool {
        match atomic_write(path, self.export_backup().as_bytes()) {
            Ok(()) => {
                log::info!("[dd-gui] 设置备份已导出：{}", path.display());
                true
            }
            Err(e) => {
                log::warn!("[dd-gui] 备份写入失败（{}）：{e}", path.display());
                false
            }
        }
    }

    /// N5：导入迁移备份——先**整体**验证（垃圾文件 → `Err`，不能走
    /// `parse_json` 的静默回落默认——那等于把用户设置清空）；字段级容错
    /// 沿用 `parse_json`（未知字段忽略、越界/损坏字段回落，惯例 ②）。
    ///
    /// **机器态一律保留本机现值**（不随导入）：`autostart`（注册表）、
    /// `panel_size`（分辨率）、`hotkey_mods`/`hotkey_vk`（改绑须走捕获流程，
    /// 导入期自动注册有冲突风险——v1 刻意不随导入，导出物仍含该字段供参考）。
    pub fn import_backup(text: &str, current: &Settings) -> Result<Settings, String> {
        let v: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("不是合法 JSON：{e}"))?;
        if !v.is_object() {
            return Err("备份内容不是 JSON 对象".to_string());
        }
        let mut s = Settings::parse_json(text);
        s.autostart = current.autostart;
        s.panel_size = current.panel_size;
        s.hotkey_mods = current.hotkey_mods;
        s.hotkey_vk = current.hotkey_vk;
        Ok(s)
    }
}

/// N5：设置迁移备份文件名（数据根目录下，与 `config.json` 同目录——
/// 用户拷贝该单文件即可迁移）。
pub const BACKUP_FILE_NAME: &str = "dd-settings-backup.json";
/// N5：导出物版本标记字段（`exported_from`：导出时宿主版本，便于排查
/// 「新版本导出 → 旧版本导入」的字段差异）。
pub const BACKUP_VERSION_KEY: &str = "exported_from";

/// 原子写盘（R-02）：同目录写 `.tmp` 临时文件后 `rename` 覆盖目标。
///
/// 写盘中途崩溃/断电只丢 `.tmp`，目标文件要么保持完整旧内容、要么已整体
/// 换成完整新内容。`std::fs::rename` 在 Windows 走
/// `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`，可直接覆盖既有目标。失败时尽力
/// 删除 `.tmp` 残留。刻意不做 fsync：调用方（`persist_panel_size` 每次隐藏
/// 面板都写盘）优先低延迟，崩溃窗口从「整个写入时长」缩到「一次 rename」。
/// 与 `dd-host` 的同款助手各落一份（方案 R-02 口径：两 crate 各一）。
fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp_name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    match std::fs::write(&tmp, bytes) {
        Ok(()) => match std::fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        },
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}
