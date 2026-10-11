//! O12（2026-10-11）拆分自 `settings.rs`：单测（原 1325–2299 行）。本文件即
//! `#[cfg(test)] mod tests`（由 mod.rs 声明），仅去除原文件的内层 `mod tests { }`
//! 包装与整体缩进之外的包装行，测试项零改动。

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
fn search_steam_games_roundtrip() {
    // 2026-10-07：默认关（Steam 游戏默认不参与搜索）；字段缺失（旧版本
    // 配置）/ 类型损坏 → 默认关；显式值往返一致。
    assert!(!Settings::default().search_steam_games);
    assert!(!Settings::parse_json("{}").search_steam_games);
    assert!(!Settings::parse_json(r#"{"theme":"dark"}"#).search_steam_games);
    let s = Settings {
        search_steam_games: true,
        ..Settings::default()
    };
    let parsed = Settings::parse_json(&s.to_json_string());
    assert_eq!(parsed, s, "search_steam_games 往返一致");
    // 类型损坏（非布尔）→ 回落默认关
    assert!(!Settings::parse_json(r#"{"search_steam_games":1}"#).search_steam_games);
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

/// 与仓库既有测试同口径的临时目录（进程级唯一，测试负责清理）。
fn r02_temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "dd-gui-r02-{tag}-{}-{}",
        std::process::id(),
        std::time::Instant::now().elapsed().as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// R-02：目标已存在时整体替换，不留旧内容片段。
#[test]
fn r02_atomic_write_replaces_existing() {
    let dir = r02_temp_dir("replace");
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("config.json");
    std::fs::write(&target, b"{\"old\": 1}").unwrap();
    atomic_write(&target, b"{\"new\": 2}").unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"{\"new\": 2}");
    assert!(!dir.join("config.json.tmp").exists(), "无 .tmp 残留");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// R-02：目标目录缺失时先创建再写入（对齐 `save` 原有 create_dir_all 口径）。
#[test]
fn r02_atomic_write_missing_dir() {
    let dir = r02_temp_dir("missing");
    let target = dir.join("a/b/config.json");
    atomic_write(&target, b"ok").unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"ok");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// R-14：>1 MiB config.json → 视为损坏回落默认设置，不 panic、不整读。
#[test]
fn r14_config_over_limit_defaults() {
    let dir = r02_temp_dir("r14");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    // 外形是 JSON 开头 + 2 MiB 填充（校验在读入前，内容不参与）
    std::fs::write(
        &path,
        format!(
            "{{\"theme\":0,{}\"}}",
            "\"x\":\"".to_string() + &"y".repeat(2 * 1024 * 1024)
        ),
    )
    .unwrap();
    assert!(std::fs::metadata(&path).unwrap().len() > Settings::CONFIG_MAX_BYTES);
    assert_eq!(Settings::load_from(&path), Settings::default());
    // 限内正常文件照常解析（theme 为字符串枚举）
    let ok = dir.join("ok.json");
    std::fs::write(&ok, "{\"theme\":\"light\"}").unwrap();
    assert_eq!(Settings::load_from(&ok).theme, ThemePref::Light);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// R-02：写入失败 → 返回 `Err`、原文件保持完整旧内容、无 `.tmp` 残留。
///
/// 失败注入：Windows 下以 `share_mode(0)` 独占打开目标文件，使
/// `MoveFileExW(REPLACE_EXISTING)` 因共享冲突失败；Unix 下把目录置只读。
#[test]
fn r02_atomic_write_failure_keeps_old() {
    let dir = r02_temp_dir("failure");
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("config.json");
    std::fs::write(&target, b"original").unwrap();

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let _lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&target)
            .unwrap();
        assert!(atomic_write(&target, b"new").is_err());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        assert!(atomic_write(&target, b"new").is_err());
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    assert_eq!(std::fs::read(&target).unwrap(), b"original");
    assert!(!dir.join("config.json.tmp").exists(), "无 .tmp 残留");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// R-17：`save` 返回**是否持久化成功**——可写目录 `true`（成功路径零变化，
/// 回归锚定）；写盘失败 `false`（Windows 用 `share_mode(0)` 独占锁注入，
/// 与 r02 同款）。调用方据此做「每会话首次失败」一次性反馈。
#[test]
fn r17_save_to_reports_persisted_and_failure() {
    let dir = r02_temp_dir("r17");
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("config.json");
    assert!(Settings::default().save_to(&target), "可写目录应返回 true");
    assert!(target.exists(), "写盘真实发生");

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let _lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&target)
            .unwrap();
        assert!(
            !Settings::default().save_to(&target),
            "写盘失败应返回 false（而非静默「看似成功」）"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        assert!(!Settings::default().save_to(&target));
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// N4：`warm_capacity` 默认 / 钳位 / 往返三态——字段缺失（旧版本配置）
/// 或类型损坏 → 默认 8；0 与 99 等越界值 clamp 到 1–16；合法值经
/// `to_json_string` → `parse_json` 往返保真（`Settings::save` 覆盖依据）。
#[test]
fn n4_warm_capacity_defaults_clamps_and_roundtrips() {
    // 默认值 = 原 LRU_WARM_CAPACITY 常量值（M1–M4 基线口径不变）
    assert_eq!(Settings::default().warm_capacity, WARM_CAPACITY_DEFAULT);
    assert_eq!(WARM_CAPACITY_DEFAULT, 8);
    // 字段缺失（旧版本配置）→ 默认
    assert_eq!(Settings::parse_json("{}").warm_capacity, 8);
    // 类型损坏 → 默认
    assert_eq!(
        Settings::parse_json(r#"{"warm_capacity":"x"}"#).warm_capacity,
        8
    );
    // 越界 clamp 到 1–16
    assert_eq!(
        Settings::parse_json(r#"{"warm_capacity":0}"#).warm_capacity,
        1
    );
    assert_eq!(
        Settings::parse_json(r#"{"warm_capacity":99}"#).warm_capacity,
        16
    );
    // 合法值往返保真
    let s = Settings {
        warm_capacity: 3,
        ..Settings::default()
    };
    assert_eq!(Settings::parse_json(&s.to_json_string()), s);
    assert_eq!(Settings::parse_json(&s.to_json_string()).warm_capacity, 3);
}

/// N1：`CustomCommand` 构造规范化 + `custom_commands` 序列化往返与
/// 兼容——① 构造 trim/小写化/空白关键词拒绝/空字段拒绝；② 配置解析逐条
/// 校验、非法条目跳过、字段缺失（旧版本配置）默认空；③ 合法值往返保真。
#[test]
fn n1_custom_commands_validate_roundtrip_and_compat() {
    use CustomCommandKind::{Path, Url};
    // ① 构造规范化
    let c = CustomCommand::new(" GitHub ", " GH ", Url, " https://github.com ").unwrap();
    assert_eq!(c.keyword, "gh");
    assert_eq!(c.title, "GitHub");
    assert!(CustomCommand::new("", "gh", Url, "https://x").is_none());
    assert!(CustomCommand::new("t", "", Url, "https://x").is_none());
    assert!(CustomCommand::new("t", "gh", Url, " ").is_none());
    assert!(CustomCommand::new("t", "两个 词", Url, "https://x").is_none());
    assert_eq!(CustomCommandKind::parse("url"), Some(Url));
    assert_eq!(CustomCommandKind::parse("path"), Some(Path));
    assert_eq!(CustomCommandKind::parse("exe"), None);
    // ② 配置解析：非法条目跳过、kind 未知跳过、字段缺失默认空
    let s = Settings::parse_json(
        r#"{"custom_commands":[
                {"title":"GitHub","keyword":"gh","kind":"url","target":"https://github.com"},
                {"title":"坏条目","keyword":"","kind":"url","target":"https://x"},
                {"title":"报告","keyword":"rep","kind":"folder","target":"C:\\doc"}
            ]}"#,
    );
    assert_eq!(s.custom_commands.len(), 1, "非法条目跳过");
    assert_eq!(s.custom_commands[0].keyword, "gh");
    assert_eq!(
        Settings::parse_json("{}").custom_commands,
        Vec::<CustomCommand>::new(),
        "旧版本配置默认空"
    );
    // ③ 往返保真
    let s = Settings {
        custom_commands: vec![
            CustomCommand::new("GitHub", "gh", Url, "https://github.com").unwrap(),
            CustomCommand::new("报告", "rep", Path, r"C:\Users\doc").unwrap(),
        ],
        ..Settings::default()
    };
    let back = Settings::parse_json(&s.to_json_string());
    assert_eq!(back, s);
}

/// N2：`ext_settings` 通道——字段缺失（旧版本配置）默认空；逐条校验
/// （ext_id 非空、值仅收字符串，非字符串值跳过）；合法值往返保真。
#[test]
fn n2_ext_settings_roundtrip_and_compat() {
    // 默认空 + 旧版本配置兼容
    assert!(Settings::default().ext_settings.is_empty());
    assert!(Settings::parse_json("{}").ext_settings.is_empty());
    // 解析：非字符串值 / 空 ext_id / 非对象值跳过；合法条目保留
    let s = Settings::parse_json(
        r#"{"ext_settings":{
                "com.ddrun.apps":{"blocklist":"卸载, update"},
                "com.ddrun.calc":{"answer":42},
                "":{"k":"v"},
                "com.ddrun.shell":"not-an-object"
            }}"#,
    );
    assert_eq!(s.ext_settings.len(), 1);
    assert_eq!(
        s.ext_settings
            .get("com.ddrun.apps")
            .unwrap()
            .get("blocklist"),
        Some(&"卸载, update".to_string())
    );
    // 往返保真
    let back = Settings::parse_json(&s.to_json_string());
    assert_eq!(back, s);
}

/// N5：导出/导入迁移备份——① 导出剔除机器态（无 autostart / panel_size
/// 键）且写 `exported_from`，业务字段保留；② 垃圾文件 → `Err`（不得静默
/// 回落默认——那等于清空用户设置）；③ 导入保留本机机器态（autostart /
/// panel_size / 热键不随导入）；④ 旧版本导出物（exported_from 任意值）
/// 与未知字段兼容；⑤ 导出→导入往返还原业务字段。
#[test]
fn n5_backup_export_strips_and_import_tolerates() {
    use CustomCommandKind::Url;
    // ① 导出裁剪
    let s = Settings {
        custom_commands: vec![
            CustomCommand::new("GitHub", "gh", Url, "https://github.com").unwrap(),
        ],
        ..Settings::default()
    };
    let backup = s.export_backup();
    let v: serde_json::Value = serde_json::from_str(&backup).unwrap();
    assert!(v.get("autostart").is_none(), "机器态剔除：autostart");
    assert!(v.get("panel_size").is_none(), "机器态剔除：panel_size");
    assert_eq!(
        v.get(BACKUP_VERSION_KEY).and_then(|x| x.as_str()),
        Some(crate::aggregator::HOST_VERSION),
        "导出物带宿主版本标记"
    );
    assert_eq!(
        v.get("theme").and_then(|x| x.as_str()),
        Some(s.theme.as_str()),
        "业务字段保留"
    );
    // ② 垃圾文件 → Err（非 JSON / 非对象）
    assert!(Settings::import_backup("not json", &Settings::default()).is_err());
    assert!(Settings::import_backup("[1,2]", &Settings::default()).is_err());
    // ③ 机器态保留本机现值（备份里即使带了也覆盖回去）
    let current = Settings {
        autostart: true,
        panel_size: Some((800, 600)),
        hotkey_mods: 0b0110,
        hotkey_vk: 0x50,
        ..Settings::default()
    };
    let backup_with_machine = r#"{
            "theme": "light", "autostart": false, "panel_size": [100, 100],
            "hotkey_mods": 1, "hotkey_vk": 32,
            "exported_from": "0.0.9", "future_field": true
        }"#;
    let imported = Settings::import_backup(backup_with_machine, &current).unwrap();
    assert_eq!(imported.theme.as_str(), "light", "业务字段随导入");
    assert!(imported.autostart, "autostart 保留本机现值");
    assert_eq!(
        imported.panel_size,
        Some((800, 600)),
        "panel_size 保留本机现值"
    );
    assert_eq!(
        imported.hotkey_mods, 0b0110,
        "热键不随导入（改绑须走捕获流程）"
    );
    assert_eq!(imported.hotkey_vk, 0x50);
    // ④ 未知字段容忍（parse_json 惯例 ②）+ 旧版本 exported_from 不设门槛
    // ⑤ 导出 → 导入往返还原业务字段（机器态按 current 保留）
    let other = Settings {
        warm_capacity: 3,
        search_apps: false,
        ..Settings::default()
    };
    let restored = Settings::import_backup(&other.export_backup(), &current).unwrap();
    assert_eq!(restored.warm_capacity, 3);
    assert!(!restored.search_apps);
    assert!(restored.autostart, "往返后机器态仍为本机现值");
    assert_eq!(restored, {
        let mut expect = other.clone();
        expect.autostart = current.autostart;
        expect.panel_size = current.panel_size;
        expect.hotkey_mods = current.hotkey_mods;
        expect.hotkey_vk = current.hotkey_vk;
        expect
    });
}

/// T9（2026-10-05）：背景图字段——默认值 / 类型损坏回落 / 越界 clamp /
/// 旧版本兼容（字段缺失 → None）/ trim 归一 / 往返保真。
#[test]
fn t9_background_image_defaults_clamp_and_roundtrip() {
    // ① 默认：无图 / 不透明度 20（对齐 CmdPal）/ Fill / 着色 0
    let d = Settings::default();
    assert_eq!(d.background_image_path, None, "默认无背景图");
    assert_eq!(d.background_image_opacity, BG_IMAGE_OPACITY_DEFAULT);
    assert_eq!(d.background_image_fit, BgImageFit::Fill);
    assert_eq!(d.background_image_tint_intensity, BG_IMAGE_TINT_DEFAULT);
    // ② 完整字段解析：路径保留、越界 clamp、合法 fit
    let parsed = Settings::parse_json(
        r#"{"background_image_path":"C:\\pic\\bg.png",
                "background_image_opacity":250,
                "background_image_fit":"stretch",
                "background_image_tint_intensity":33}"#,
    );
    assert_eq!(
        parsed.background_image_path.as_deref(),
        Some("C:\\pic\\bg.png")
    );
    assert_eq!(parsed.background_image_opacity, 100, "越界 clamp 0–100");
    assert_eq!(parsed.background_image_fit, BgImageFit::Stretch);
    assert_eq!(parsed.background_image_tint_intensity, 33);
    // ③ 类型损坏 / 未知 fit / 非字符串路径 → 回落默认（None / Fill / 20 / 0）
    let bad = Settings::parse_json(
        r#"{"background_image_path":42,"background_image_opacity":"x",
                "background_image_fit":"cover","background_image_tint_intensity":-1}"#,
    );
    assert_eq!(bad.background_image_path, None, "非字符串路径 → None");
    assert_eq!(bad.background_image_opacity, BG_IMAGE_OPACITY_DEFAULT);
    assert_eq!(
        bad.background_image_fit,
        BgImageFit::Fill,
        "未知 fit → Fill"
    );
    assert_eq!(bad.background_image_tint_intensity, BG_IMAGE_TINT_DEFAULT);
    // ④ 旧版本配置（字段缺失）→ 默认，零迁移
    let old = Settings::parse_json(r#"{"theme":"dark"}"#);
    assert_eq!(old.background_image_path, None);
    assert_eq!(old.background_image_opacity, BG_IMAGE_OPACITY_DEFAULT);
    // ⑤ trim 归一：空白路径 → None
    let blank = Settings::parse_json(r#"{"background_image_path":"   "}"#);
    assert_eq!(blank.background_image_path, None, "空白路径归一为 None");
    // ⑥ 往返保真（None 与 Some 两态）
    let roundtrip_none = Settings::parse_json(&Settings::default().to_json_string());
    assert_eq!(roundtrip_none.background_image_path, None);
    let with_bg = Settings {
        background_image_path: Some("D:/wall.jpg".into()),
        background_image_opacity: 55,
        background_image_fit: BgImageFit::Stretch,
        background_image_tint_intensity: 10,
        ..Settings::default()
    };
    let roundtrip = Settings::parse_json(&with_bg.to_json_string());
    assert_eq!(roundtrip.background_image_path, Some("D:/wall.jpg".into()));
    assert_eq!(roundtrip.background_image_opacity, 55);
    assert_eq!(roundtrip.background_image_fit, BgImageFit::Stretch);
    assert_eq!(roundtrip.background_image_tint_intensity, 10);
}
