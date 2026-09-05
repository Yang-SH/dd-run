//! 系统副作用：字体加载、窗口居中定位、UAC 提权、资源管理器定位。
//!
//! 拆分自原 main.rs（docs/refactor-layering-plan.md 方案 1），方法体逐字未改。

use crate::app::PaletteApp;
use crate::app::APP_H;
use crate::app::APP_W;
use eframe::egui;

/// 加载本地字体栈：CJK 主字（msyh / SimHei / Deng）+ Segoe UI Symbol 符号后援
/// + Segoe Fluent/MDL2 图标字体（§8.6 glyph 图标，M5 UI 批次 2）。
///
/// msyh.ttc 覆盖 CJK 与 ✓/✗（Dingbats 区），但**缺** Geometric Shapes 的 ◌ (U+25CC)
/// ——M3 桩态页脚会渲染成方框。seguisym.ttf（Win 7+ 必装）补 Geometric Shapes /
/// Misc Symbols，把 ◌/○/· 等符号路由到它去渲染。
///
/// 图标字体按两代兼容顺序加载：Win11 的 `SegoeIcons.ttf`（Segoe Fluent Icons）优先，
/// Win10 无此文件时回落 `segmdl2.ttf`（Segoe MDL2 Assets）——码位（U+E700–U+E8FF
/// 一带）两代基本兼容；两个都不存在时 glyph 图标显示为方块（记录日志，不致命）。
/// 追加在字体族**末尾**：egui 字形回退按族内顺序查，CJK/符号字体缺的 PUA 码位
/// 自然落到图标字体（PUA 区 U+E000+ 两字体均无覆盖，无抢字形问题）。
pub fn setup_cjk_fonts(ctx: &egui::Context) {
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
        return;
    }
    ctx.set_fonts(fonts);
}

impl PaletteApp {
    /// 计算"光标所在显示器工作区"内使面板居中的 `OuterPosition` 并发送。
    ///
    /// 坐标换算：显示器（`rcWork`）与 winit 窗口位置都是**物理像素**，
    /// 而 egui `OuterPosition` 期望**逻辑点**（egui-winit 内部按窗口 scale
    /// factor 再换算回物理）。这里用 `ctx.pixels_per_point()` 作换算率
    /// （即当前窗口缩放）。多 DPI 混合屏上目标屏缩放与窗口当前缩放不同时
    /// 会有几像素偏差——验收标准（grills A1："每次唤起居中、无位置跳动"）
    /// 不要求像素级精确，单屏/同 DPI 场景完全居中。
    ///
    /// 失败时静默返回（指针/显示器信息取不到）：窗口仍会正常显示在
    /// 上一次的位置，仅不居中——不让定位失败阻断唤起。
    #[cfg(windows)]
    pub(crate) fn send_center_on_cursor(&self, ctx: &egui::Context) {
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
        let work = info.rcWork; // 工作区（物理像素，不含任务栏）
        let ppp = ctx.pixels_per_point().max(0.5);
        let win_w = APP_W * ppp;
        let win_h = APP_H * ppp;
        let cx = work.left as f32 + ((work.right - work.left) as f32 - win_w) * 0.5;
        let cy = work.top as f32 + ((work.bottom - work.top) as f32 - win_h) * 0.5;
        eprintln!(
            "[dd-gui] 唤起居中：光标屏工作区=({},{} {}x{}) → ({}, {})",
            work.left,
            work.top,
            work.right - work.left,
            work.bottom - work.top,
            cx,
            cy
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
            cx / ppp,
            cy / ppp,
        )));
    }

    /// 非 Windows 占位：退化为 egui 自带"按窗口当前所在屏居中"（dd-run 当前
    /// Windows 宿主不走此分支；多屏语义在此平台未定义）。
    #[cfg(not(windows))]
    pub(crate) fn send_center_on_cursor(&self, ctx: &egui::Context) {
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
#[cfg(windows)]
pub(crate) fn reveal_in_folder(path: &str) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(format!("/select,{path}"))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
pub(crate) fn reveal_in_folder(_path: &str) -> Result<(), String> {
    Err("仅 Windows 支持资源管理器定位".to_string())
}
