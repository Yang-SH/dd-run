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

/// 宿主本地设置（当前仅主题偏好 + 首屏视图；后续字段向后兼容追加）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    pub theme: ThemePref,
    pub open_view: OpenView,
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
        s
    }

    /// 序列化为 JSON 文本（单行，便于人查）。
    pub fn to_json_string(&self) -> String {
        serde_json::json!({
            "theme": self.theme.as_str(),
            "open_view": self.open_view.as_str(),
        })
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
}
