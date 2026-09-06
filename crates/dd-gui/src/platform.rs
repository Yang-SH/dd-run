//! 系统副作用：字体加载、窗口居中定位、UAC 提权、资源管理器定位。
//!
//! 拆分自原 main.rs（docs/refactor-layering-plan.md 方案 1），方法体逐字未改。

use crate::app::PaletteApp;
use eframe::egui;

/// 加载本地字体栈：CJK 主字（msyh / SimHei / Deng）、Segoe UI Symbol 符号后援、
/// Segoe UI 拉丁/符号扩展后援、Segoe Fluent/MDL2 图标字体（§8.6 glyph 图标，M5 UI 批次 2）。
///
/// msyh.ttc 覆盖 CJK 与 ✓/✗（Dingbats 区），但**缺** Geometric Shapes 的 ◌ (U+25CC)
/// ——M3 桩态页脚会渲染成方框。seguisym.ttf（Win 7+ 必装）补 Geometric Shapes /
/// Misc Symbols，把 ◌/○/· 等符号路由到它去渲染。
///
/// **segoeui.ttf（v4.12 真机修复 2026-09-06）**：AMD Software 等应用的
/// AppsFolder 显示名含 U+A78B（MODIFIER LETTER COLON「꞉」，AMD 用它规避
/// Windows 文件名非法字符 `:`）——msyh/seguisym 的 cmap 均无此码位（已用
/// fonttools 核验），egui 渲染为缺字方块「AMD Software□ Adrenalin」。
/// Segoe UI 覆盖该码位（开始菜单即用 Segoe UI 渲染正常），补为拉丁后援；
/// 其 cmap **零 PUA 码位**（已核验），插在图标字体之前不会抢 §8.6 glyph 字形。
///
/// 图标字体按两代兼容顺序加载：Win11 的 `SegoeIcons.ttf`（Segoe Fluent Icons）优先，
/// Win10 无此文件时回落 `segmdl2.ttf`（Segoe MDL2 Assets）——码位（U+E700–U+E8FF
/// 一带）两代基本兼容；两个都不存在时 glyph 图标显示为方块（记录日志，不致命）。
/// 追加在字体族**末尾**：egui 字形回退按族内顺序查，CJK/符号字体缺的 PUA 码位
/// 自然落到图标字体（PUA 区 U+E000+ 两字体均无覆盖，无抢字形问题）。
///
/// **M6 批次 6.2（L10）：后台线程加载**——22MB 级字体（msyh.ttc ~19.7MB +
/// seguisym 2.5MB）的读盘 + 解析是冷启动 GUI 初始化的最大瓶颈（A2 实测
/// total ~2.8s，其中数据就绪仅 ~2ms）。本函数**立即返回**：首帧用 egui 默认
/// 字体渲染，字体在后台线程就绪后 `ctx.set_fonts` 原子热替换并请求重绘。
/// 已知取舍（记档）：若用户在字体就绪前（约 2.5s 内）唤起面板，CJK 文本
/// 短暂显示方块后自动恢复（字体热替换为原子操作，无半新半旧帧）。
pub fn setup_cjk_fonts(ctx: &egui::Context) {
    let ctx = ctx.clone();
    std::thread::Builder::new()
        .name("cjk-fonts".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            match load_cjk_font_definitions() {
                Some(fonts) => {
                    ctx.set_fonts(fonts);
                    ctx.request_repaint();
                    eprintln!(
                        "[dd-gui] CJK 字体后台加载完成（{} ms），已热替换；首帧为默认字体",
                        started.elapsed().as_millis()
                    );
                }
                None => eprintln!("[dd-gui] 未找到任何 CJK 字体，中文可能显示为方块"),
            }
        })
        .expect("spawn cjk-fonts thread");
}

/// 读盘并构建字体定义（纯函数，供 [`setup_cjk_fonts`] 的后台线程调用）；
/// `None` = 无任何 CJK 字体可用（维持 egui 默认字体）。
fn load_cjk_font_definitions() -> Option<egui::FontDefinitions> {
    let cjk_candidates = [
        // 优先 msyh.ttc（YaHei，Win7+ 必装且完整含 U+2713 ✓ 与 CJK）
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\Deng.ttf",
    ];
    let sym_candidate = r"C:\Windows\Fonts\seguisym.ttf";
    // M5 批次 2：glyph 图标字体（按 Win11→Win10 顺序尝试，两代码位兼容）
    let icon_candidates = [
        r"C:\Windows\Fonts\SegoeIcons.ttf", // Win11（Segoe Fluent Icons）
        r"C:\Windows\Fonts\segmdl2.ttf",    // Win10 回退（Segoe MDL2 Assets）
    ];

    let mut fonts = egui::FontDefinitions::default();
    let mut any_loaded = false;

    if let Some(path) = cjk_candidates
        .into_iter()
        .find(|p| std::path::Path::new(p).is_file())
    {
        match std::fs::read(path) {
            Ok(bytes) => {
                fonts.font_data.insert(
                    "cjk".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push("cjk".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("cjk".to_owned());
                any_loaded = true;
            }
            Err(e) => eprintln!("[dd-gui] 读 CJK 字体 {path} 失败：{e}"),
        }
    }
    if let Ok(bytes) = std::fs::read(sym_candidate) {
        // 符号后援：append 在 cjk 之后，egui 字形回退按字体族顺序查找，
        // cjk 缺的 Geometric Shapes/Misc Symbols 落到 seguisym。
        fonts.font_data.insert(
            "sym".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .push("sym".to_owned());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("sym".to_owned());
        any_loaded = true;
    } else {
        eprintln!("[dd-gui] 未找到 {sym_candidate}（符号字体）；M3 桩态 ◌ 等符号可能仍显示为方块");
    }
    // Segoe UI 拉丁/符号扩展后援（v4.12 真机修复）：U+A78B 等 msyh/seguisym
    // 缺失的拉丁修饰符码位落到这里（见函数 doc 注释的取证记录）。零 PUA
    // 码位，插在图标字体之前无抢字形风险；缺文件（< Vista）仅记日志。
    let latin_candidate = r"C:\Windows\Fonts\segoeui.ttf";
    if std::path::Path::new(latin_candidate).is_file() {
        let path = latin_candidate;
        match std::fs::read(path) {
            Ok(bytes) => {
                fonts.font_data.insert(
                    "segoe".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                // 插在 sym 之后、icons 之前：普通拉丁/符号优先用 Segoe UI，
                // PUA 图标码位继续落到后面的图标字体。
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push("segoe".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("segoe".to_owned());
                eprintln!("[dd-gui] 已加载拉丁后援字体：{path}");
            }
            Err(e) => eprintln!("[dd-gui] 读拉丁后援字体 {path} 失败：{e}"),
        }
    }
    if let Some(path) = icon_candidates
        .into_iter()
        .find(|p| std::path::Path::new(p).is_file())
    {
        match std::fs::read(path) {
            Ok(bytes) => {
                fonts.font_data.insert(
                    "icons".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                // 追加在族末（cjk/sym 之后）：PUA 码位（§8.6 glyph 值）落到图标字体。
                // 加入 Proportional + Monospace 两个族（列表副标题/键位提示同源显示）。
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push("icons".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("icons".to_owned());
                any_loaded = true;
                eprintln!("[dd-gui] 已加载图标字体：{path}");
            }
            Err(e) => eprintln!("[dd-gui] 读图标字体 {path} 失败：{e}"),
        }
    } else {
        eprintln!("[dd-gui] 未找到图标字体（SegoeIcons/segmdl2）；glyph 图标将显示为方块");
    }

    if !any_loaded {
        eprintln!("[dd-gui] 未找到任何 CJK 字体，中文可能显示为方块");
        return None;
    }
    Some(fonts)
}

impl PaletteApp {
    /// 取"光标所在显示器工作区"的**逻辑尺寸**（物理像素 ÷ `pixels_per_point`），
    /// 供 v4.12 D37 唤起尺寸自适应（`root_panel_size`/`settings_panel_size`）clamp
    /// 使用；返回 `None` = 取不到（指针/显示器信息失败），调用方按无 clamp 路径回落。
    ///
    /// 与 [`center_on_cursor`] 拆分的原因：原 `send_center_on_cursor` 在发
    /// `InnerSize` **之前**读取 `inner_rect` 作为居中窗口尺寸——但该尺寸是上一次
    /// 可见态的遗留值（例如设置页 650×640），而本次 `show()` 实际要显示的是
    /// 复位后的根页 650×440。用陈旧尺寸居中 + 实际小尺寸渲染 → 面板相对屏心
    /// 偏移 `(大-小)/2`，表现为"切换设置项后关闭重开面板位置变化"。拆分后
    /// 先取工作区算目标尺寸，再用目标尺寸精确居中，消除该跳动。
    #[cfg(windows)]
    pub(crate) fn cursor_work_area(&self, ctx: &egui::Context) -> Option<(f32, f32)> {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let mut pt = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut pt) } == 0 {
            eprintln!("[dd-gui] 居中：GetCursorPos 失败，保持原位显示");
            return None;
        }
        let monitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
        if monitor.is_null() {
            eprintln!("[dd-gui] 居中：MonitorFromPoint 无结果，保持原位显示");
            return None;
        }
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            eprintln!("[dd-gui] 居中：GetMonitorInfoW 失败，保持原位显示");
            return None;
        }
        let work = info.rcWork; // 工作区（物理像素，不含任务栏）
        let ppp = ctx.pixels_per_point().max(0.5);
        Some((
            (work.right - work.left) as f32 / ppp,
            (work.bottom - work.top) as f32 / ppp,
        ))
    }

    /// 把 `target`（逻辑点，即本次 `show()` 将要显示的窗口尺寸）居中到
    /// 光标所在显示器工作区，发送 `OuterPosition`。
    ///
    /// 坐标换算：显示器（`rcWork`）与 winit 窗口位置都是**物理像素**，而
    /// egui `OuterPosition` 期望**逻辑点**（egui-winit 内部按窗口 scale factor
    /// 再换算回物理）。这里用 `ctx.pixels_per_point()` 作换算率（即当前窗口缩放）。
    /// 多 DPI 混合屏上目标屏缩放与窗口当前缩放不同时会有几像素偏差——验收标准
    /// （grills A1："每次唤起居中、无位置跳动"）不要求像素级精确，单屏/同 DPI
    /// 场景完全居中。
    ///
    /// 任一步失败直接返回（指针/显示器信息取不到）：窗口仍正常显示在上一次
    /// 位置，仅不居中——不让定位失败阻断唤起。
    #[cfg(windows)]
    pub(crate) fn center_on_cursor(&self, ctx: &egui::Context, target: (f32, f32)) {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let mut pt = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut pt) } == 0 {
            eprintln!("[dd-gui] 居中：GetCursorPos 失败，保持原位显示");
            return;
        }
        let monitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
        if monitor.is_null() {
            eprintln!("[dd-gui] 居中：MonitorFromPoint 无结果，保持原位显示");
            return;
        }
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            eprintln!("[dd-gui] 居中：GetMonitorInfoW 失败，保持原位显示");
            return;
        }
        let work = info.rcWork;
        let ppp = ctx.pixels_per_point().max(0.5);
        // 用**目标尺寸**而非 `inner_rect` 的遗留尺寸（见 `cursor_work_area` 注释）。
        let win_w = target.0 * ppp;
        let win_h = target.1 * ppp;
        let cx = work.left as f32 + ((work.right - work.left) as f32 - win_w) * 0.5;
        let cy = work.top as f32 + ((work.bottom - work.top) as f32 - win_h) * 0.5;
        eprintln!(
            "[dd-gui] 唤起居中：光标屏工作区=({},{} {}x{}) 目标={}x{} → ({}, {})",
            work.left,
            work.top,
            work.right - work.left,
            work.bottom - work.top,
            target.0,
            target.1,
            cx,
            cy
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
            cx / ppp,
            cy / ppp,
        )));
    }

    /// 非 Windows 占位：取不到工作区，返回 `None`（唤起尺寸不做 clamp）。
    #[cfg(not(windows))]
    pub(crate) fn cursor_work_area(&self, _ctx: &egui::Context) -> Option<(f32, f32)> {
        None
    }

    /// 非 Windows 占位：退化为 egui 自带"按窗口当前所在屏居中"（dd-run 当前
    /// Windows 宿主不走此分支；多屏语义在此平台未定义）。
    #[cfg(not(windows))]
    pub(crate) fn center_on_cursor(&self, ctx: &egui::Context, _target: (f32, f32)) {
        if let Some(cmd) = egui::ViewportCommand::center_on_screen(ctx) {
            ctx.send_viewport_cmd(cmd);
        }
    }
}

/// 以管理员身份运行（UAC 提权）：`ShellExecuteW` verb=runas（10B.2）。
/// 用户在 UAC 弹窗取消时返回 Err（返回值 ≤ 32）。
#[cfg(windows)]
pub(crate) fn run_as_admin(path: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
    let verb = wide("runas");
    let file = wide(path);
    let h = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if h as isize > 32 {
        Ok(())
    } else {
        Err(format!("ShellExecuteW = {}", h as isize))
    }
}

#[cfg(not(windows))]
pub(crate) fn run_as_admin(_path: &str) -> Result<(), String> {
    Err("仅 Windows 支持提权运行".to_string())
}

/// 在资源管理器中定位文件（`explorer /select,<path>`，10B.2）。
///
/// 若目标路径不存在，fallback 到其父目录；父目录也不存在则返回错误，避免
/// Windows 在 `/select` 失效时随机打开「文档」等默认位置。
#[cfg(windows)]
pub(crate) fn reveal_in_folder(path: &str) -> Result<(), String> {
    use std::path::Path;

    let p = Path::new(path);
    let arg = if p.exists() {
        format!("/select,{path}")
    } else if let Some(parent) = p.parent().filter(|d| d.exists()) {
        // 目标已不存在/被卸载：至少打开其原本所在目录（定位到目录本身）。
        format!("/open,{}", parent.display())
    } else {
        return Err(format!("路径不存在：{path}"));
    };

    std::process::Command::new("explorer")
        .arg(arg)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
pub(crate) fn reveal_in_folder(_path: &str) -> Result<(), String> {
    Err("仅 Windows 支持资源管理器定位".to_string())
}

/// 窗口系统背景材质（v4.7 D31：Win11 22H2+ `DWMWA_SYSTEMBACKDROP_TYPE`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemBackdrop {
    /// 无材质（`DWMSBT_NONE`，不透明面板）。
    None,
    /// 云母（`DWMSBT_MAINWINDOW`）。
    Mica,
    /// 亚克力（`DWMSBT_TRANSIENTWINDOW`）。
    Acrylic,
}

/// 应用系统背景材质到主面板窗口（v4.7 D31，设计稿 8.1「材质效果」行）。
///
/// 返回 `false` = API 不支持或调用失败（Win10 / Win11 22621 以下返回错误码），
/// 调用方必须回退不透明面板——降级不阻断（区别热键 fail-fast）。
#[cfg(windows)]
pub fn apply_system_backdrop(hwnd: isize, backdrop: SystemBackdrop) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW,
        DWMWA_SYSTEMBACKDROP_TYPE,
    };
    let value: i32 = match backdrop {
        SystemBackdrop::None => DWMSBT_NONE,
        SystemBackdrop::Mica => DWMSBT_MAINWINDOW,
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
        eprintln!(
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
        eprintln!(
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

/// UTF-16 NUL 结尾宽字符串（windows-sys 0.61 无 wide_string! 宏，本地辅助）。
#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 开机自启（M6 批次 6.3）：HKCU `...\CurrentVersion\Run` 写/删 `dd-run` 值。
/// 值 = 带引号的当前 exe 路径（含空格安全）。返回 Err = 注册表操作失败。
#[cfg(windows)]
pub fn set_autostart(enable: bool) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_SZ,
    };
    let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let value_name = wide("dd-run");
    let exe = std::env::current_exe()
        .map_err(|e| format!("current_exe 失败：{e}"))?
        .to_string_lossy()
        .to_string();
    let data: Vec<u16> = wide(&format!("\"{exe}\""));

    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        let open = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        );
        if open != 0 {
            return Err(format!("RegOpenKeyExW = {open}"));
        }
        let result = if enable {
            let hr = RegSetValueExW(
                hkey,
                value_name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                (data.len() * 2) as u32,
            );
            if hr == 0 {
                Ok(())
            } else {
                Err(format!("RegSetValueExW = {hr}"))
            }
        } else {
            // 删除不存在的值（ERROR_FILE_NOT_FOUND = 2）属幂等成功
            let hr = RegDeleteValueW(hkey, value_name.as_ptr());
            if hr == 0 || hr == 2 {
                Ok(())
            } else {
                Err(format!("RegDeleteValueW = {hr}"))
            }
        };
        RegCloseKey(hkey);
        result
    }
}

#[cfg(not(windows))]
pub fn set_autostart(_enable: bool) -> Result<(), String> {
    Err("仅 Windows 支持开机自启".to_string())
}

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
