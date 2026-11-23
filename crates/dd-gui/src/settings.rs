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
    /// 不含 NOREPEAT——注册时由热键线程统一补）。默认 Alt。
    pub hotkey_mods: u32,
    /// 全局热键主键虚拟键码（默认 VK_SPACE = 0x20）。
    pub hotkey_vk: u32,
    /// 开机自启（M6 批次 6.3：HKCU Run 键；默认关）。
    pub autostart: bool,
    /// §5.2：启动时自动检查更新（读 GitHub Releases；默认关——便携工具隐私
    /// 优先，手动「检查更新」按钮常驻不受此开关影响）。
    pub check_updates: bool,
    /// 已停用扩展的清单 id 列表（M6 批次 6.3：聚合时跳过；默认空 = 全启用）。
    pub disabled_extensions: Vec<String>,
    /// 面板记忆尺寸（v4.12 D37：手动拉伸后的逻辑宽高，取整存储；
    /// `None` = 从未手动拉伸，唤起用自适应基准 `APP_W/APP_H`）。
    /// 唤起时仍按光标所在屏工作区 clamp（小屏收缩），见 `app::root_panel_size`。
    pub panel_size: Option<(u32, u32)>,
    /// N21：面板唤起定位模式（opt-in；默认 `Cursor` = 鼠标所在屏居中——
    /// V-1 基线形态逐位不变）。
    pub panel_pos_mode: PanelPosMode,
    /// N21：记忆的面板位置（**逻辑点**整数对；仅 `Remember` 档在隐藏时落盘，
    /// 唤起按目标屏工作区 clamp 恢复）。**机器态**——N5 导出剥离（同
    /// `panel_size` 分辨率相关口径）。
    pub panel_last_pos: Option<(i32, i32)>,
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
    /// N11（2026-11-22）：空查询首屏置顶键表（上限 20，clamp）——两族键：
    /// 稳定 id 族 = `usage_key(ext_id, item_id)`、apps 族 = `app:{显示名}` 片段键
    /// （apps 条目 id 随枚举漂移，N2 注记）。业务态：随 N5 导出迁移（非机器态）。
    pub pinned_keys: Vec<String>,
    /// N20（2026-11-22）：记录搜索历史开关（默认开；关 = 停止记录 + 清空既有
    /// ——写设置即生效）。历史本体存独立 `history.json`（个人态，文件隔离
    /// 天然不随 N5 导出迁移）；本开关属用户偏好随导出。
    pub record_search_history: bool,
    /// N17（2026-11-22）：命令热键绑定表（≤3 槽，上限 clamp）——机器态随 N5
    /// 导入保留本机现值（导出物含字段供参考）。
    pub command_hotkeys: Vec<CommandHotkey>,
    /// N18（2026-11-22）：文本片段库（一档：面板内搜 → Enter 复制；上限
    /// [`SNIPPET_CAP`]）。业务态随 N5 导出迁移；`;keyword` 全局展开维持缓办。
    pub snippets: Vec<Snippet>,
    /// 内置扩展用户可调配置（N2，2026-10-04）：`ext_id → (键 → 值)`。
    /// 聚合期经 `aggregator::inject_ext_settings` 注入扩展 `entry.env` 内存
    /// 副本（变量名 `DD_EXT_CFG_<KEY 大写>`）；in-process 内置另走内存通道
    /// 直接注入（websearch 引擎表先例）。值 v1 一律字符串（设计稿的
    /// `serde_json::Value` 草型收窄——消费者当前只需字符串，且 `Value` 无
    /// `Eq` 会破坏 `Settings` 的 derive）；BTreeMap 保证序列化确定性。
    pub ext_settings: BTreeMap<String, BTreeMap<String, String>>,
    /// C4（dd-run-改进分析.md §4.8）：`parse_json` 读到的**未知字段**原样
    /// 保留——值存该字段的 **JSON 编码文本**（`Value::to_string()`；写盘时
    /// `from_str` 还原真实类型，对象/数组/数字/布尔均保真）。读→写一轮后
    /// 未知字段不再消失（新版本字段在旧版回退场景下保真）。BTreeMap 保证
    /// 序列化顺序确定。
    pub unknown: BTreeMap<String, String>,
    /// C4：读到**更高** schema 版本 → 只读模式（设置页保存被拒 + 顶部黄条，
    /// 由 UI 层按 `Settings::read_only` 提示）；防止旧版保存一次抹掉新字段。
    /// 本字段不落盘（每轮 `parse_json` 重算）。
    pub read_only: bool,
}

impl Settings {
    /// C4：当前写入方支持的配置 schema 版本（`to_json_string` 落
    /// `"schema_version"`；读到更高值进入只读模式）。
    pub const SCHEMA_VERSION: u32 = 1;
}

/// 全局热键默认修饰键：Alt（MOD_* 值：ALT=1/CONTROL=2/SHIFT=4/WIN=8；
/// 本 crate 纯逻辑不依赖 windows-sys，用字面量 + 单测锚定）。
/// 不含 NOREPEAT——注册时由热键线程统一补。
pub const HOTKEY_MODS_DEFAULT: u32 = 0b0001;
/// 全局热键默认主键：VK_SPACE。
pub const HOTKEY_VK_DEFAULT: u32 = 0x20;
/// 修饰键合法位掩码（Ctrl/Alt/Shift/Win），解析时剔除其余位。
pub const HOTKEY_MODS_MASK: u32 = 0b1111;

/// N21（2026-11-21）：面板唤起定位模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelPosMode {
    /// 鼠标所在屏居中（默认——V-1 基线形态逐位不变）。
    #[default]
    Cursor,
    /// 记忆上次位置（隐藏时落盘 `panel_last_pos`；唤起按目标屏工作区
    /// clamp 恢复；恢复失败回落居中）。
    Remember,
}

impl PanelPosMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PanelPosMode::Cursor => "cursor",
            PanelPosMode::Remember => "remember",
        }
    }
    /// 未知字符串 → `None`（调用方回落默认，与 CustomCommandKind::parse 同口径）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cursor" => Some(PanelPosMode::Cursor),
            "remember" => Some(PanelPosMode::Remember),
            _ => None,
        }
    }
}

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
    /// 构造并规范化：三字段 trim、去空；keyword 转小写且**不得含空白**。
    ///
    /// N19：keyword 支持**逗号分隔多关键词**——每段独立 trim / 小写 / 拒空白 /
    /// 拒空段，段重复拒绝；规范化存储 = 规范段以 `,` 连接（**主关键词 = 第一段**，
    /// 聚合条目 id 与右键删除语义锚定于此）。单关键词形态与 N1 行为逐字节一致。
    /// 任一字段为空（无有效段）→ `None`。
    pub fn new(title: &str, keyword: &str, kind: CustomCommandKind, target: &str) -> Option<Self> {
        let title = title.trim();
        let target = target.trim();
        if title.is_empty() || target.is_empty() {
            return None;
        }
        let segs: Vec<String> = keyword
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        if segs.is_empty() {
            return None;
        }
        if segs.iter().any(|s| s.split_whitespace().count() != 1) {
            return None;
        }
        // 段重复拒绝（大小写已归一；含同段重复与跨段重复）
        for (i, s) in segs.iter().enumerate() {
            if segs[i + 1..].iter().any(|t| t == s) {
                return None;
            }
        }
        Some(Self {
            title: title.to_string(),
            keyword: segs.join(","),
            kind,
            target: target.to_string(),
        })
    }

    /// N19：主关键词（第一段；单关键词形态 = 全串）。
    pub fn primary_keyword(&self) -> &str {
        self.keyword.split(',').next().unwrap_or_default()
    }

    /// N19：全部关键词段（已规范化的存储串切分）。
    pub fn keyword_segments(&self) -> impl Iterator<Item = &str> {
        self.keyword.split(',').filter(|s| !s.is_empty())
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
            check_updates: false,
            disabled_extensions: Vec::new(),
            panel_size: None,
            panel_pos_mode: PanelPosMode::default(),
            panel_last_pos: None,
            lang: Lang::default(),
            density: ListDensity::default(),
            search_apps: true,
            search_steam_games: false,
            warm_capacity: WARM_CAPACITY_DEFAULT,
            custom_commands: Vec::new(),
            pinned_keys: Vec::new(),
            record_search_history: true,
            command_hotkeys: Vec::new(),
            snippets: Vec::new(),
            ext_settings: BTreeMap::new(),
            unknown: BTreeMap::new(),
            read_only: false,
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
    /// 写盘时已知键的集合（C4 未知字段保真：其余顶层键进 `unknown`）。
    fn known_keys() -> &'static [&'static str] {
        &[
            "schema_version",
            "theme",
            "open_view",
            "search_engines",
            "backdrop",
            "material_opacity",
            "corner_pref",
            "border_mode",
            "esc_behavior",
            "backspace_go_back",
            "colorization",
            "custom_tint_color",
            "custom_tint_intensity",
            "single_click_activation",
            "ui_animations",
            "background_image_path",
            "background_image_opacity",
            "background_image_fit",
            "background_image_tint_intensity",
            "hotkey_mods",
            "hotkey_vk",
            "autostart",
            "check_updates",
            "disabled_extensions",
            "panel_size",
            // N21：唤起定位模式 + 记忆位置（后者为机器态，导出剥离——但
            // config.json 本身要消费，归已知键）
            "panel_pos_mode",
            "panel_last_pos",
            "lang",
            "density",
            "search_apps",
            "search_steam_games",
            "warm_capacity",
            "custom_commands",
            "pinned_keys",
            "record_search_history",
            "command_hotkeys",
            "snippets",
            "ext_settings",
            // N5 导出物版本标记：随导出物写入、导入时由 parse_json 读走后
            // 不再回写（写盘方是 config.json，不是备份文件）——归为已知键，
            // 不进 unknown 保真集。
            "exported_from",
        ]
    }

    /// 从 JSON 文本解析；空/损坏/字段未知 → 默认（防御性，永不失败）。
    ///
    /// C4（§4.8）：① 未知顶层字段原样保留进 `unknown`（JSON 编码文本，
    /// `to_json_string` 回写）；② `schema_version` **高于**当前支持值 →
    /// `read_only = true`（防旧版保存抹掉新字段）；缺失/同值/更低 → 正常
    ///（更低 = 旧版本配置文件，逐字段读已知键即迁移）。
    pub fn parse_json(text: &str) -> Self {
        let mut s = Self::default();
        let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
            return s;
        };
        // C4①：未知顶层字段原样保留（已知键 / 元数据键除外）。
        if let Some(obj) = v.as_object() {
            for (k, val) in obj {
                if !Self::known_keys().contains(&k.as_str()) {
                    s.unknown.insert(k.clone(), val.to_string());
                }
            }
        }
        // C4②：schema 版本门禁（严格大于才只读——同值/缺失/更低都放行）。
        if let Some(n) = v.get("schema_version").and_then(|x| x.as_u64()) {
            if n > Self::SCHEMA_VERSION as u64 {
                s.read_only = true;
            }
        }
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
        // 缺失/类型损坏 → 回落默认 Alt + Space。
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
        // §5.2：启动时检查更新（字段缺失/类型损坏 → 默认关，零迁移）
        if let Some(b) = v.get("check_updates").and_then(|b| b.as_bool()) {
            s.check_updates = b;
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
        // N21：唤起定位模式（旧版本缺失 / 未知值 → Cursor 默认）。
        s.panel_pos_mode = v
            .get("panel_pos_mode")
            .and_then(|x| x.as_str())
            .and_then(PanelPosMode::parse)
            .unwrap_or_default();
        // N21：记忆位置（旧版本缺失 / 损坏 / 非两元素整数数组 → None）。
        s.panel_last_pos = v.get("panel_last_pos").and_then(|x| {
            let a = x.as_array()?;
            if a.len() != 2 {
                return None;
            }
            let x = a[0].as_i64()?;
            let y = a[1].as_i64()?;
            Some((x as i32, y as i32))
        });
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
        // N11（2026-11-22）：置顶键表——字段缺失（旧版本配置）→ 默认空；
        // 逐条收字符串、去空串，上限 20 clamp（保留前 20，防导入膨胀）。
        if let Some(arr) = v.get("pinned_keys").and_then(|a| a.as_array()) {
            s.pinned_keys = arr
                .iter()
                .filter_map(|x| x.as_str())
                .map(str::to_string)
                .filter(|k| !k.is_empty())
                .take(PINNED_KEYS_CAP)
                .collect();
        }
        // N20（2026-11-22）：搜索历史开关（默认开——字段缺失回落 true）。
        if let Some(b) = v.get("record_search_history").and_then(|b| b.as_bool()) {
            s.record_search_history = b;
        }
        // N18（2026-11-22）：文本片段——字段缺失（旧版本配置）→ 默认空；
        // 逐条经 `Snippet::new` 校验规范化，非法条目跳过；上限 200 clamp。
        if let Some(arr) = v.get("snippets").and_then(|a| a.as_array()) {
            s.snippets = arr
                .iter()
                .filter_map(|e| {
                    Snippet::new(
                        e.get("name").and_then(|x| x.as_str())?,
                        e.get("keyword").and_then(|x| x.as_str())?,
                        e.get("text").and_then(|x| x.as_str())?,
                    )
                })
                .take(SNIPPET_CAP)
                .collect();
        }
        // N17（2026-11-22）：命令热键绑定——字段缺失（旧版本配置）→ 默认空；
        // 逐条校验（mods/vk 非零、keyword 非空），非法条目跳过；上限 3 clamp。
        if let Some(arr) = v.get("command_hotkeys").and_then(|a| a.as_array()) {
            s.command_hotkeys = arr
                .iter()
                .filter_map(|e| {
                    let mods = e.get("mods").and_then(|x| x.as_u64())? as u32;
                    let vk = e.get("vk").and_then(|x| x.as_u64())? as u32;
                    let keyword = e.get("keyword").and_then(|x| x.as_str())?.to_string();
                    (mods != 0 && vk != 0 && !keyword.is_empty()).then_some(CommandHotkey {
                        mods,
                        vk,
                        keyword,
                    })
                })
                .take(COMMAND_HOTKEY_CAP)
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
    ///
    /// C4（§4.8）：先落 `schema_version` 与**未知字段**（JSON 编码文本还原
    /// 为真实类型），再由下方已知键覆盖——未知字段在「读→写」一轮后保真，
    /// 高版本配置回退旧版保存不再抹掉新字段。
    pub fn to_json_string(&self) -> String {
        let engines: Vec<serde_json::Value> = self
            .search_engines
            .iter()
            .map(|e| serde_json::json!({ "name": e.name, "template": e.template }))
            .collect();
        // C4：未知字段还原（编码文本 → Value；损坏即丢弃——不该发生）。
        let mut out = serde_json::Map::new();
        out.insert(
            "schema_version".to_string(),
            serde_json::json!(Self::SCHEMA_VERSION),
        );
        for (k, encoded) in &self.unknown {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(encoded) {
                out.insert(k.clone(), val);
            }
        }
        let known = serde_json::json!({
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
            "check_updates": self.check_updates,
            "disabled_extensions": self.disabled_extensions,
            "panel_size": match self.panel_size {
                Some((w, h)) => serde_json::json!([w, h]),
                None => serde_json::Value::Null,
            },
            "panel_pos_mode": self.panel_pos_mode.as_str(),
            "panel_last_pos": match self.panel_last_pos {
                Some((x, y)) => serde_json::json!([x, y]),
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
            "pinned_keys": self.pinned_keys,
            "record_search_history": self.record_search_history,
            "command_hotkeys": self
                .command_hotkeys
                .iter()
                .map(|h| {
                    serde_json::json!({
                        "mods": h.mods,
                        "vk": h.vk,
                        "keyword": h.keyword,
                    })
                })
                .collect::<Vec<_>>(),
            "snippets": self
                .snippets
                .iter()
                .map(|sn| {
                    serde_json::json!({
                        "name": sn.name,
                        "keyword": sn.keyword,
                        "text": sn.text,
                    })
                })
                .collect::<Vec<_>>(),
            "ext_settings": self.ext_settings,
        });
        // 已知键覆盖同名未知键（正常不相交；防御键集漂移）
        if let Some(known_obj) = known.as_object() {
            for (k, v) in known_obj {
                out.insert(k.clone(), v.clone());
            }
        }
        serde_json::Value::Object(out).to_string()
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
    /// 加固①（2026-11-22）：**durable 写**（tmp sync_all 后 rename——显式保存
    /// 低频，崩溃窗口压到零；高频路径 `persist_panel_size`/`persist_panel_position`
    /// 保持 [`atomic_write`] 快速分层）+ **滚动备份环**：custom_commands 与盘上
    /// 版本不同才轮转旧 config 进 `config.json.bak.1..3`（无变更保存不触发，
    /// 控制写放大；读旧文件失败 = 首存不轮转）。
    fn save_to(&self, path: &std::path::Path) -> bool {
        if let Ok(old) = std::fs::read_to_string(path) {
            let (b1, b2, b3) = backup_ring_paths(path);
            rotate_settings_backup(&b1, &b2, &b3, &old, &self.custom_commands);
        }
        // 加固②：显式保存审计（字节量 / 命令条目数 / 耗时 → dd-run.log；
        // B-2 异常可从此 grep 出「保存 → 为空」窗口的时间线）。
        let payload = self.to_json_string();
        let started = std::time::Instant::now();
        let ok = match atomic_write_durable(path, payload.as_bytes()) {
            Ok(()) => {
                log::info!(
                    "[dd-gui] 设置已保存：{}（{} B / custom_commands {} 条 / {:?}）",
                    path.display(),
                    payload.len(),
                    self.custom_commands.len(),
                    started.elapsed()
                );
                true
            }
            Err(e) => {
                log::warn!("[dd-gui] 配置写入失败（{}）：{e}", path.display());
                false
            }
        };
        ok
    }

    /// [`save_to`] 的可注入形态（A4，仅测试编译）：`writer` 注入写盘失败，
    /// 单测不再依赖 OS 权限语义（root 容器无视 `chmod`、`share_mode(0)` 仅
    /// Windows）。生产路径恒 [`save_to`] → [`atomic_write`]。
    #[cfg(test)]
    fn save_to_with(
        &self,
        path: &std::path::Path,
        writer: impl Fn(&std::path::Path, &[u8]) -> std::io::Result<()>,
    ) -> bool {
        match atomic_write_with(path, self.to_json_string().as_bytes(), writer) {
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
        // N21：panel_last_pos 亦属机器/分辨率相关态——剥离同 panel_size 口径
        obj.remove("panel_last_pos");
        obj.insert(
            BACKUP_VERSION_KEY.to_string(),
            serde_json::json!(crate::aggregator::HOST_VERSION),
        );
        v.to_string()
    }

    /// N5：备份落盘（原子写，同 [`Self::save`] 的 R-02 机制；返回成败供
    /// 调用方 toast——失败静默会「看似成功」）。
    pub fn save_backup_to(&self, path: &std::path::Path) -> bool {
        match atomic_write_durable(path, self.export_backup().as_bytes()) {
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
    /// 导入迁移备份（N5）：解析 + 覆盖导入（自述机器侧字段保留当前值）。
    ///
    /// C3（dd-run-改进分析.md §4.6）：错误类型从 `String` 收拢为
    /// [`SettingsImportError`]（三边界之一的设置边界）——Display 与原拼接
    /// 文案逐字一致（toast 键 `toast.settings_import_fail` 的 `{e}` 占位不变），
    /// 调用方可按变体分流。
    pub fn import_backup(text: &str, current: &Settings) -> Result<Settings, SettingsImportError> {
        let v: serde_json::Value = serde_json::from_str(text)?;
        if !v.is_object() {
            return Err(SettingsImportError::NotAnObject);
        }
        let mut s = Settings::parse_json(text);
        s.autostart = current.autostart;
        s.panel_size = current.panel_size;
        // N21：panel_last_pos 属机器态——同 panel_size 保留本机现值
        s.panel_last_pos = current.panel_last_pos;
        s.hotkey_mods = current.hotkey_mods;
        s.hotkey_vk = current.hotkey_vk;
        // N17：命令热键绑定属机器态（组合键占用属机器环境，与 hotkey_mods/vk
        // 同族——导入期自动注册有冲突风险；导出物仍含该字段供参考）
        s.command_hotkeys = current.command_hotkeys.clone();
        Ok(s)
    }
}

/// N17（2026-11-22）：命令热键绑定（第二热键基建，≤3 槽）——全局组合键直接
/// 触发对应**自定义命令**（复用 N1 执行体 `run_custom_command`，S-03 白名单
/// 天然生效；面板隐藏态亦生效）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandHotkey {
    /// 修饰键掩码（settings 位约定：ALT=1/CONTROL=2/SHIFT=4/WIN=8）。
    pub mods: u32,
    /// 主键虚拟键码。
    pub vk: u32,
    /// 触发的自定义命令**主关键词**（必须命中既有 `CustomCommand`）。
    pub keyword: String,
}

/// N17：命令热键槽位上限（方案 §4.12：≤3 槽；slot 0 = 面板 toggle 不占）。
pub const COMMAND_HOTKEY_CAP: usize = 3;

/// N18（2026-11-22）：文本片段（一档：面板内搜片段 → Enter 复制）。
/// `;keyword` 全局展开**不做**（缓办边界 §3.2——输入伪造面）；占位符 / 批量
/// 导入不做（方案 §4.13 边界）。业务态随 N5 导出迁移。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    /// 显示名（首屏条目标题，生成拼音索引）。
    pub name: String,
    /// 匹配关键词（规范化同 CustomCommand：trim / 小写 / 拒空白；首屏唯一）。
    pub keyword: String,
    /// 片段正文（Enter 复制到剪贴板；单条 ≤ [`SNIPPET_TEXT_CAP`] 字符）。
    pub text: String,
}

/// N18：片段条目上限。
pub const SNIPPET_CAP: usize = 200;
/// N18：单条片段正文字符上限（clamp；方案 §4.13 口径）。
pub const SNIPPET_TEXT_CAP: usize = 5000;

impl Snippet {
    /// 构造并规范化：name/text trim、keyword 按 CustomCommand 同规则（trim /
    /// 小写 / 拒空白）；text 超 [`SNIPPET_TEXT_CAP`] 截断；任一必填为空 → None。
    pub fn new(name: &str, keyword: &str, text: &str) -> Option<Self> {
        let name = name.trim();
        let text = text.trim();
        let keyword = keyword.trim().to_lowercase();
        if name.is_empty() || keyword.is_empty() || text.is_empty() {
            return None;
        }
        if keyword.split_whitespace().count() != 1 {
            return None;
        }
        let mut text = text.to_string();
        if text.chars().count() > SNIPPET_TEXT_CAP {
            text = text.chars().take(SNIPPET_TEXT_CAP).collect();
        }
        Some(Self {
            name: name.to_string(),
            keyword,
            text,
        })
    }
}

/// C3：[`Settings::import_backup`] 的错误（Display 与原 String 文案逐字一致）。
#[derive(Debug, thiserror::Error)]
pub enum SettingsImportError {
    #[error("不是合法 JSON：{0}")]
    NotJson(#[from] serde_json::Error),
    #[error("备份内容不是 JSON 对象")]
    NotAnObject,
}

/// N5：设置迁移备份文件名（数据根目录下，与 `config.json` 同目录——
/// 用户拷贝该单文件即可迁移）。
pub const BACKUP_FILE_NAME: &str = "dd-settings-backup.json";
/// N5：导出物版本标记字段（`exported_from`：导出时宿主版本，便于排查
/// 「新版本导出 → 旧版本导入」的字段差异）。
pub const BACKUP_VERSION_KEY: &str = "exported_from";

/// N11：置顶键表上限（方案 §4.6 口径；parse clamp 与新增淘汰共用）。
pub(crate) const PINNED_KEYS_CAP: usize = 20;

/// N11：置顶键幂等切换——存在即移除（返回 false = 取消置顶）；不存在则追加
/// （达上限先淘汰最旧[队首]，返回 true = 已置顶）。纯函数，调用方负责落盘 +
/// 重聚合脏标记。
pub(crate) fn toggle_pinned_key(keys: &mut Vec<String>, key: &str) -> bool {
    if let Some(idx) = keys.iter().position(|k| k == key) {
        keys.remove(idx);
        return false;
    }
    while keys.len() >= PINNED_KEYS_CAP {
        keys.remove(0);
    }
    keys.push(key.to_string());
    true
}

impl Settings {
    /// N17：命令热键绑定校验（设置卡「保存」用；错误给可读原因）——
    /// ① 组合键完整且含修饰（Ctrl/Alt/Win，与主热键捕获口径一致）；
    /// ② 主关键词必须命中既有自定义命令；③ 组合键不得与主热键 / 既有槽位
    /// 重复；④ 关键词不得与既有槽位重复；⑤ 槽位未满。
    pub fn validate_command_hotkey(&self, hk: &CommandHotkey) -> Result<(), String> {
        if hk.mods == 0 || hk.vk == 0 {
            return Err("组合键不完整（须含修饰键与主键）".into());
        }
        if hk.mods & HOTKEY_MODS_MASK == 0 {
            return Err("须含 Ctrl / Alt / Win 修饰键".into());
        }
        if !self
            .custom_commands
            .iter()
            .any(|c| c.primary_keyword() == hk.keyword)
        {
            return Err(format!("关键词「{}」未命中任何自定义命令", hk.keyword));
        }
        if self.hotkey_mods == hk.mods && self.hotkey_vk == hk.vk {
            return Err("与面板唤起热键重复".into());
        }
        if let Some(dup) = self
            .command_hotkeys
            .iter()
            .find(|h| h.mods == hk.mods && h.vk == hk.vk)
        {
            return Err(format!("组合键已被绑定（关键词「{}」）", dup.keyword));
        }
        if self.command_hotkeys.iter().any(|h| h.keyword == hk.keyword) {
            return Err(format!("关键词「{}」已被绑定", hk.keyword));
        }
        if self.command_hotkeys.len() >= COMMAND_HOTKEY_CAP {
            return Err("命令热键槽位已满（上限 3）".into());
        }
        Ok(())
    }

    /// N18：新增片段（设置卡「添加」用）——规范化 + 校验（必填 / 关键词
    /// 规范化 / 关键词唯一 / 槽位上限），成功即入表（调用方落盘 + 重聚合）。
    pub fn add_snippet(&mut self, name: &str, keyword: &str, text: &str) -> Result<(), String> {
        if self.snippets.len() >= SNIPPET_CAP {
            return Err(format!("片段数已达上限（{}）", SNIPPET_CAP));
        }
        let sn = Snippet::new(name, keyword, text)
            .ok_or_else(|| "名称 / 关键词 / 内容不能为空，关键词不得含空白".to_string())?;
        if self.snippets.iter().any(|x| x.keyword == sn.keyword) {
            return Err(format!("关键词「{}」已被占用", sn.keyword));
        }
        self.snippets.push(sn);
        Ok(())
    }

    /// N18：删除片段（按关键词；不存在返回 false）。
    pub fn remove_snippet(&mut self, keyword: &str) -> bool {
        let before = self.snippets.len();
        self.snippets.retain(|x| x.keyword != keyword);
        self.snippets.len() != before
    }
}

/// 原子写盘（R-02）：同目录写 `.tmp` 临时文件后 `rename` 覆盖目标。
///
/// 写盘中途崩溃/断电只丢 `.tmp`，目标文件要么保持完整旧内容、要么已整体
/// 换成完整新内容。`std::fs::rename` 在 Windows 走
/// `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`，可直接覆盖既有目标。失败时尽力
/// 删除 `.tmp` 残留。刻意不做 fsync：调用方（`persist_panel_size` 每次隐藏
/// 面板都写盘）优先低延迟，崩溃窗口从「整个写入时长」缩到「一次 rename」。
/// 与 `dd-host` 的同款助手各落一份（方案 R-02 口径：两 crate 各一）。
/// `pub(crate)`（§5.1）：`usage::Usage::save` 复用同一原子写语义。
pub(crate) fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    atomic_write_with(path, bytes, atomic_writer)
}

/// 加固①（2026-11-22）：durable 变体——tmp 落盘后 `sync_all` 再 rename
///（FlushFileBuffers 语义由 [`std::fs::File::sync_all`] 承载）。仅显式保存链
/// 使用（save / save_to / save_backup_to——低频且恰是 B-2 异常最可疑窗口）；
/// 高频路径保持 [`atomic_write`] 快速分层，与上方「刻意不做 fsync」注释的
/// 设计意图不冲突（按频率分层）。
pub(crate) fn atomic_write_durable(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    atomic_write_with(path, bytes, durable_writer)
}

/// 生产 durable 写入器：显式句柄写入 + `sync_all`（确保落盘后再 rename）。
fn durable_writer(tmp: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut f = std::fs::File::create(tmp)?;
    f.write_all(bytes)?;
    f.sync_all()
}

/// 加固①：滚动备份环文件名（config.json 同目录；上限 3）。
const BACKUP_RING: [&str; 3] = [
    "config.json.bak.1",
    "config.json.bak.2",
    "config.json.bak.3",
];

/// 加固②：启动骤降检测纯函数——当前命令数**严格小于**备份解析数的一半且
/// 备份非空 → `Some(备份条数)`（调用方 toast 指引恢复）；备份缺失/损坏（parse
/// 回落空表）/空表 → `None`（无基线可比，不制造虚警）。
pub(crate) fn detect_custom_commands_drop(
    current: &[CustomCommand],
    backup: Option<&str>,
) -> Option<usize> {
    let bak_cc = Settings::parse_json(backup?).custom_commands;
    if bak_cc.is_empty() {
        return None;
    }
    (current.len() * 2 < bak_cc.len()).then_some(bak_cc.len())
}

/// 加固①：环内三路径（config.json 同目录；无父目录回落当前目录——防御）。
pub(crate) fn backup_ring_paths(
    config: &std::path::Path,
) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let dir = config.parent().unwrap_or_else(|| std::path::Path::new("."));
    (
        dir.join(BACKUP_RING[0]),
        dir.join(BACKUP_RING[1]),
        dir.join(BACKUP_RING[2]),
    )
}

/// 加固①：备份环轮转——旧 config 的 custom_commands 与当前不同才轮转：
/// bak2→bak3、bak1→bak2、旧全文→bak1（[`atomic_write`] 快速路径——环写入
/// 本身低频备份，主 config 写盘才是 durable 层）。无变更返回 false 不动环。
/// B-2 异常形态（保存→发现为空的窗口内丢命令）的最小兜底：任一时刻环内
/// 至少保留最近三次含命令变更的旧配置。
pub(crate) fn rotate_settings_backup(
    bak1: &std::path::Path,
    bak2: &std::path::Path,
    bak3: &std::path::Path,
    old_config_text: &str,
    current: &[CustomCommand],
) -> bool {
    if Settings::parse_json(old_config_text).custom_commands == *current {
        return false;
    }
    // 环轮转（rename 目标不存在时忽略——环未满属常态）
    let _ = std::fs::rename(bak2, bak3);
    let _ = std::fs::rename(bak1, bak2);
    atomic_write(bak1, old_config_text.as_bytes()).is_ok()
}

/// 生产写入器：整文件写入（`std::fs::write`）。
fn atomic_writer(tmp: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(tmp, bytes)
}

/// [`atomic_write`] 的可注入形态（A4）：`writer` 负责把 `bytes` 落到传入的
/// `.tmp` 路径。测试传恒 `Err` 的桩注入写盘失败——不再依赖 OS 权限语义，
/// 注入分支（`share_mode` / `chmod`）已删，root/非 root 行为一致。
fn atomic_write_with(
    path: &std::path::Path,
    bytes: &[u8],
    writer: impl Fn(&std::path::Path, &[u8]) -> std::io::Result<()>,
) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp_name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    match writer(&tmp, bytes) {
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
