//! 窗口材质与 chrome：DWM 系统背景、明暗染色、圆角、描边、强调色。

use eframe::egui;

/// 窗口系统背景材质（v4.7 D31：Win11 22H2+ `DWMWA_SYSTEMBACKDROP_TYPE`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemBackdrop {
    /// 无材质（`DWMSBT_NONE`，不透明面板）。
    None,
    /// 云母（`DWMSBT_MAINWINDOW`）。
    Mica,
    /// 云母 Alt（`DWMSBT_TABBEDWINDOW`，M2 2026-09-20）：云母变体，色更浓、
    /// 对比更强；Win11 22H2+ 支持，旧系统应用失败走既有回退链。
    MicaAlt,
    /// 亚克力（`DWMSBT_TRANSIENTWINDOW`）。
    Acrylic,
}

/// M1（2026-09-20）：设置枚举 → DWM 类型的**唯一映射点**（参照 CmdPal
/// `BackdropStyles` 注册表思想）。调用方不再各自 `match`——新增材质档
/// 只需在 [`SystemBackdrop`] 与 [`dd_gui::settings::Backdrop`] 各加一行。
impl From<dd_gui::settings::Backdrop> for SystemBackdrop {
    fn from(b: dd_gui::settings::Backdrop) -> Self {
        use dd_gui::settings::Backdrop;
        match b {
            Backdrop::None => SystemBackdrop::None,
            Backdrop::Mica => SystemBackdrop::Mica,
            Backdrop::MicaAlt => SystemBackdrop::MicaAlt,
            Backdrop::Acrylic => SystemBackdrop::Acrylic,
        }
    }
}

/// 应用系统背景材质到主面板窗口（v4.7 D31，设计稿 8.1「材质效果」行）。
///
/// 返回 `false` = API 不支持或调用失败（Win10 / Win11 22621 以下返回错误码），
/// 调用方必须回退不透明面板——降级不阻断（区别热键 fail-fast）。
#[cfg(windows)]
pub fn apply_system_backdrop(hwnd: isize, backdrop: SystemBackdrop) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMSBT_TABBEDWINDOW,
        DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
    };
    let value: i32 = match backdrop {
        SystemBackdrop::None => DWMSBT_NONE,
        SystemBackdrop::Mica => DWMSBT_MAINWINDOW,
        SystemBackdrop::MicaAlt => DWMSBT_TABBEDWINDOW,
        SystemBackdrop::Acrylic => DWMSBT_TRANSIENTWINDOW,
    };
    let hr = unsafe {
        DwmSetWindowAttribute(
            hwnd as HWND,
            DWMWA_SYSTEMBACKDROP_TYPE as u32,
            (&value) as *const i32 as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        )
    };
    if hr != 0 {
        log::debug!(
            "[dd-gui] DWMWA_SYSTEMBACKDROP_TYPE 应用失败（hr=0x{:x}）→ 回退不透明面板",
            hr
        );
        return false;
    }
    true
}

#[cfg(not(windows))]
pub fn apply_system_backdrop(_hwnd: isize, _backdrop: SystemBackdrop) -> bool {
    false
}

/// 材质/窗口明暗染色跟随主题（`DWMWA_USE_IMMERSIVE_DARK_MODE` = 20，D31）。
/// best-effort：失败仅记日志（材质染色回退系统默认，不影响功能）。
#[cfg(windows)]
pub fn set_immersive_dark(hwnd: isize, dark: bool) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
    let value: i32 = dark as i32;
    let hr = unsafe {
        DwmSetWindowAttribute(
            hwnd as HWND,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            (&value) as *const i32 as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        )
    };
    if hr != 0 {
        log::debug!(
            "[dd-gui] DWMWA_USE_IMMERSIVE_DARK_MODE 应用失败（hr=0x{:x}）",
            hr
        );
        return false;
    }
    true
}

#[cfg(not(windows))]
pub fn set_immersive_dark(_hwnd: isize, _dark: bool) -> bool {
    false
}

/// 窗口 chrome 应用（M3/M4，2026-09-13，参考 DeskBox 材质能力清单）：
/// - **圆角**：`DWMWA_WINDOW_CORNER_PREFERENCE` 按设置三档（P3，2026-09-13：
///   圆角 `DWMWCP_ROUND` ≈8px 对齐设计 token `window_corner_radius = 8` /
///   小圆角 `DWMWCP_ROUNDSMALL` ≈4px / 方角 `DWMWCP_DONOTROUND`）——无边框
///   工具窗在 Win11 上是否默认圆角取决于系统对窗口样式的判定，行为不定；
///   显式设置后跨机型确定。设置页改选时由 `apply_corner_pref` 重调（DWM
///   属性幂等，与 egui 帧内容无关，无防闪面）；
/// - **禁过渡**：`DWMWA_TRANSITIONS_FORCEDISABLED`——唤起/隐藏瞬时（A1 验收
///   语义），不随系统「窗口动画」设置漂移（DeskBox 有动画开关；启动器场景
///   无「要动画」的需求面，直接禁用并记档，不做设置项）。
///
/// 两属性均为 Win11 文档化 DWM 属性；失败（Win10 / 22621 以下）仅记日志
/// 跳过，行为与既往一致。首次调用由 `refresh_backdrop` 以 `chrome_applied`
/// 旗标执行，与材质选择无关（无材质/不透明路径同样圆角）。
#[cfg(windows)]
pub fn apply_window_chrome(hwnd: isize, corner: crate::settings::CornerPref) {
    use crate::settings::CornerPref;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED, DWMWA_WINDOW_CORNER_PREFERENCE,
        DWMWCP_DONOTROUND, DWMWCP_ROUND, DWMWCP_ROUNDSMALL,
    };
    let corner: i32 = match corner {
        CornerPref::Round => DWMWCP_ROUND,
        CornerPref::Small => DWMWCP_ROUNDSMALL,
        CornerPref::Square => DWMWCP_DONOTROUND,
    };
    unsafe {
        let hr = DwmSetWindowAttribute(
            hwnd as HWND,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&corner) as *const i32 as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );
        if hr != 0 {
            log::debug!(
                "[dd-gui] DWMWA_WINDOW_CORNER_PREFERENCE 应用失败（hr=0x{:x}），保持系统默认圆角",
                hr
            );
        }
        let disable: i32 = 1;
        let hr = DwmSetWindowAttribute(
            hwnd as HWND,
            DWMWA_TRANSITIONS_FORCEDISABLED as u32,
            (&disable) as *const i32 as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );
        if hr != 0 {
            log::debug!(
                "[dd-gui] DWMWA_TRANSITIONS_FORCEDISABLED 应用失败（hr=0x{:x}）",
                hr
            );
        }
    }
}

/// 非 Windows 平台无对应语义，空实现（调用点恒安全）。
#[cfg(not(windows))]
pub fn apply_window_chrome(_hwnd: isize, _corner: crate::settings::CornerPref) {}

/// 面板外描边（M2，2026-09-13）：`DWMWA_BORDER_COLOR`（= 34，Win11 文档化）。
/// 材质生效时 1px 实色描边压在窗口边缘——Mica 染色极淡，面板边界与桌面易融，
/// 描边给出可辨轮廓，且跟随 [`apply_window_chrome`] 的圆角形状（DWM 自绘、
/// 贴窗口形状，无 egui 自绘描边的圆角缺口问题）。取色复用 `Palette::
/// border_strong` token（暗 0x666666 / 亮 0xd1d1d1），不造新色。`None` →
/// `DWMWA_BORDER_COLOR_NONE`（0xFFFFFFFE）停画。注意 COLORREF 无 alpha
/// （0x00BBGGRR，传入色的 alpha 被忽略）；Win10 无此属性失败跳过。
/// 幂等：材质/主题切换时由 `refresh_backdrop`/`apply_theme_pref` 重设。
#[cfg(windows)]
pub fn set_window_border(hwnd: isize, color: Option<egui::Color32>) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR};
    const BORDER_COLOR_NONE: u32 = 0xFFFF_FFFE;
    let value: u32 = match color {
        Some(c) => (c.b() as u32) << 16 | (c.g() as u32) << 8 | c.r() as u32,
        None => BORDER_COLOR_NONE,
    };
    let hr = unsafe {
        DwmSetWindowAttribute(
            hwnd as HWND,
            DWMWA_BORDER_COLOR as u32,
            (&value) as *const u32 as *const core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        )
    };
    if hr != 0 {
        log::warn!("[dd-gui] DWMWA_BORDER_COLOR 应用失败（hr=0x{:x}）", hr);
    }
}

/// 非 Windows 平台无对应语义，空实现（调用点恒安全）。
#[cfg(not(windows))]
pub fn set_window_border(_hwnd: isize, _color: Option<egui::Color32>) {}

/// 系统强调色（P4，2026-09-13）：`DwmGetColorizationColor`（dwmapi，
/// `Win32_Graphics_Dwm` 特性已启用）——DWM 色彩化色 ≈ 用户在系统设置里选的
/// 强调色（DeskBox 用 WinRT `UISettings`；此处取等价近似，不引入 WinRT 依赖）。
/// 返回 `None` = 调用失败，调用方回落 `Palette::accent`。
///
/// COLORREF 为 `0x00BBGGRR`（无 alpha，与 [`set_window_border`] 同约定）；
/// `pfOpaqueBlend = FALSE` 表示 DWM 对该色做透明混合——色相不变，1px 描边
/// 小面积上混合差量不可辨，忽略该标志直接取色。
#[cfg(windows)]
pub fn system_accent_color() -> Option<egui::Color32> {
    use windows_sys::Win32::Graphics::Dwm::DwmGetColorizationColor;
    let mut colorref: u32 = 0;
    let mut opaque: i32 = 0;
    // SAFETY：两个出参指针均为本地变量，HRESULT 判定后使用。
    let hr = unsafe { DwmGetColorizationColor(&mut colorref, &mut opaque) };
    if hr != 0 {
        log::debug!(
            "[dd-gui] DwmGetColorizationColor 失败（hr=0x{hr:x}），强调色描边回落 Palette::accent"
        );
        return None;
    }
    Some(egui::Color32::from_rgb(
        (colorref & 0xFF) as u8,
        ((colorref >> 8) & 0xFF) as u8,
        ((colorref >> 16) & 0xFF) as u8,
    ))
}

/// 非 Windows 平台无对应语义：恒 `None`（调用方恒走 `Palette::accent` 回落）。
#[cfg(not(windows))]
pub fn system_accent_color() -> Option<egui::Color32> {
    None
}

/// 窗口是否 OS 层可见（`IsWindowVisible`）。
///
/// v4.16 真机修复（拖拽后面板空白）判据：应用态 `visible=false` 但 OS 可见
/// ⇒ `Visible(false)` 曾在原生拖拽/缩放模态循环内被 Windows 静默忽略
/// （SC_MOVE 循环内 ShowWindow 无效），应用态与 OS 态脱钩——宿主据此自愈。
#[cfg(windows)]
pub fn is_window_visible(hwnd: isize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
    unsafe { IsWindowVisible(hwnd as _) != 0 }
}

#[cfg(not(windows))]
pub fn is_window_visible(_hwnd: isize) -> bool {
    true
}

/// UTF-16 NUL 结尾宽字符串（windows-sys 0.61 无 wide_string! 宏，本地辅助）。
#[cfg(windows)]
pub(super) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
