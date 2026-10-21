//! 系统 UI 语言探测。

/// 系统 UI 语言探测（v4.13 D38）：`GetUserDefaultUILanguage` 的主语言 ID
/// （LANGID 低 10 位）为中文（0x04）→ [`Lang::ZhCn`]，否则 → [`Lang::EnUs`]。
/// 供 `settings.lang == FollowSystem` 时解析生效语言（`PaletteApp::resolve_lang`）。
#[cfg(windows)]
pub fn system_ui_lang() -> dd_gui::settings::Lang {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
    // SAFETY：无参数、仅读进程默认 UI 语言，线程安全。
    let langid = unsafe { GetUserDefaultUILanguage() };
    if langid & 0x03FF == 0x0004 {
        dd_gui::settings::Lang::ZhCn
    } else {
        dd_gui::settings::Lang::EnUs
    }
}

/// 非 Windows 开发兜底：文案表以中文为主开发语言，跟随系统 → ZhCn。
#[cfg(not(windows))]
pub fn system_ui_lang() -> dd_gui::settings::Lang {
    dd_gui::settings::Lang::ZhCn
}
