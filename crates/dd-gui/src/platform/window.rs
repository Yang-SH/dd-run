//! 窗口定位：光标所在显示器工作区与唤起居中。

use crate::app::PaletteApp;
use eframe::egui;

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
            log::warn!("[dd-gui] 居中：GetCursorPos 失败，保持原位显示");
            return None;
        }
        let monitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
        if monitor.is_null() {
            log::debug!("[dd-gui] 居中：MonitorFromPoint 无结果，保持原位显示");
            return None;
        }
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            log::warn!("[dd-gui] 居中：GetMonitorInfoW 失败，保持原位显示");
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
            log::warn!("[dd-gui] 居中：GetCursorPos 失败，保持原位显示");
            return;
        }
        let monitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
        if monitor.is_null() {
            log::debug!("[dd-gui] 居中：MonitorFromPoint 无结果，保持原位显示");
            return;
        }
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            log::warn!("[dd-gui] 居中：GetMonitorInfoW 失败，保持原位显示");
            return;
        }
        let work = info.rcWork;
        let ppp = ctx.pixels_per_point().max(0.5);
        // 用**目标尺寸**而非 `inner_rect` 的遗留尺寸（见 `cursor_work_area` 注释）。
        let win_w = target.0 * ppp;
        let win_h = target.1 * ppp;
        let cx = work.left as f32 + ((work.right - work.left) as f32 - win_w) * 0.5;
        let cy = work.top as f32 + ((work.bottom - work.top) as f32 - win_h) * 0.5;
        log::debug!(
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
