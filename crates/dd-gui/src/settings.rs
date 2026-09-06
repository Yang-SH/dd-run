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

/// 窗口材质（v4.7 D30：Win11 DWM 系统背景材质，单值属性——云母/亚克力互斥，
/// 两开关状态由该值派生）。默认云母（含旧配置升级：字段缺失 → 云母）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backdrop {
    /// 无材质（不透明面板）。
    None,
    /// 云母（默认）。
    #[default]
    Mica,
    /// 亚克力。
    Acrylic,
}

impl Backdrop {
    /// 设置页显示标签。
    pub fn label(self) -> &'static str {
        match self {
            Backdrop::None => "无材质",
            Backdrop::Mica => "云母",
            Backdrop::Acrylic => "亚克力",
        }
    }

    /// JSON 序列化值（稳定标识，与显示标签解耦）。
    pub fn as_str(self) -> &'static str {
        match self {
            Backdrop::None => "none",
            Backdrop::Mica => "mica",
            Backdrop::Acrylic => "acrylic",
        }
    }

    /// JSON 值反解；未知值返回 `None`（调用方回落默认云母）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Backdrop::None),
            "mica" => Some(Backdrop::Mica),
            "acrylic" => Some(Backdrop::Acrylic),
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

/// 常用预设引擎（设置页勾选项；与 `dd-ext-websearch` 内置默认表保持一致——
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
    /// 全局热键修饰键位掩码（M6 批次 6.3：MOD_ALT=1/CONTROL=2/SHIFT=4/WIN=8，
    /// 不含 NOREPEAT——注册时由热键线程统一补）。默认 Win+Alt。
    pub hotkey_mods: u32,
    /// 全局热键主键虚拟键码（默认 VK_SPACE = 0x20）。
    pub hotkey_vk: u32,
    /// 开机自启（M6 批次 6.3：HKCU Run 键；默认关）。
    pub autostart: bool,
    /// 已停用扩展的清单 id 列表（M6 批次 6.3：聚合时跳过；默认空 = 全启用）。
    pub disabled_extensions: Vec<String>,
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
            search_engines: preset_search_engines(),
            backdrop: Backdrop::default(),
            hotkey_mods: HOTKEY_MODS_DEFAULT,
            hotkey_vk: HOTKEY_VK_DEFAULT,
            autostart: false,
            disabled_extensions: Vec::new(),
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
        // 搜索引擎：字段缺失（旧版本配置）→ 预设 5 引擎；字段存在 → 逐条
        // 校验，非法条目跳过（空数组 = 用户全部关闭，尊重其意图）。
        match v.get("search_engines") {
            None => s.search_engines = preset_search_engines(),
            Some(val) => {
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
            "hotkey_mods": self.hotkey_mods,
            "hotkey_vk": self.hotkey_vk,
            "autostart": self.autostart,
            "disabled_extensions": self.disabled_extensions,
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
                    eprintln!(
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
            eprintln!("[dd-gui] 配置目录不可定位，设置未持久化");
            return;
        };
        let dir = path.parent().map(std::path::Path::to_path_buf);
        if let Some(dir) = dir {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("[dd-gui] 配置目录创建失败（{}）：{e}", dir.display());
                return;
            }
        }
        match std::fs::write(&path, self.to_json_string()) {
            Ok(()) => eprintln!("[dd-gui] 设置已保存：{}", path.display()),
            Err(e) => eprintln!("[dd-gui] 配置写入失败（{}）：{e}", path.display()),
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
    fn search_engines_default_is_preset_five() {
        let presets = preset_search_engines();
        assert_eq!(Settings::default().search_engines, presets);
        assert_eq!(presets.len(), 5);
        assert_eq!(Settings::parse_json("{}").search_engines, presets);
        // 旧版本配置（无 search_engines 字段）→ 回落预设
        assert_eq!(
            Settings::parse_json(r#"{"theme":"dark"}"#).search_engines,
            presets
        );
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
        // 字段类型损坏（非数组）→ 保持默认
        assert_eq!(
            Settings::parse_json(r#"{"search_engines":42}"#).search_engines,
            preset_search_engines()
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
        for b in [Backdrop::None, Backdrop::Mica, Backdrop::Acrylic] {
            let s = Settings {
                backdrop: b,
                ..Settings::default()
            };
            let parsed = Settings::parse_json(&s.to_json_string());
            assert_eq!(parsed.backdrop, b, "{} 往返一致", b.label());
        }
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
}
