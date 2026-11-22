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

    /// N21：把记忆位置（**逻辑点**）clamp 进其所在显示器工作区并发送
    /// `OuterPosition`。目标窗口尺寸 = 本次 `show()` 的逻辑尺寸（与
    /// [`Self::center_on_cursor`] 同口径：物理 = 逻辑 × 当前 ppp——多 DPI
    /// 混屏下有少量偏差，验收标准（记忆位置恢复 + 不出屏）不要求像素级精确）。
    /// 任一步失败 → `false`（调用方回落居中路径，不阻断唤起）。
    ///
    /// clamp 纯函数见 [`clamp_to_work`]（单测锚定）。
    #[cfg(windows)]
    pub(crate) fn restore_remembered_position(
        &self,
        ctx: &egui::Context,
        saved: egui::Pos2,
        target: (f32, f32),
    ) -> bool {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        };

        let ppp = ctx.pixels_per_point().max(0.5);
        let pt = POINT {
            x: (saved.x * ppp) as i32,
            y: (saved.y * ppp) as i32,
        };
        let monitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
        if monitor.is_null() {
            return false;
        }
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return false;
        }
        let work = info.rcWork;
        let win_w = target.0 * ppp;
        let win_h = target.1 * ppp;
        let (x, y) = clamp_to_work(
            saved.x * ppp,
            saved.y * ppp,
            (
                work.left as f32,
                work.top as f32,
                work.right as f32,
                work.bottom as f32,
            ),
            win_w,
            win_h,
        );
        log::debug!(
            "[dd-gui] N21 位置恢复：saved=({:.0},{:.0}) → 屏 clamp 后 =({:.0},{:.0})",
            saved.x,
            saved.y,
            x / ppp,
            y / ppp
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
            x / ppp,
            y / ppp,
        )));
        true
    }

    /// 非 Windows 占位：不恢复（回落居中）。
    #[cfg(not(windows))]
    pub(crate) fn restore_remembered_position(
        &self,
        _ctx: &egui::Context,
        _saved: egui::Pos2,
        _target: (f32, f32),
    ) -> bool {
        false
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

/// N21 纯 clamp：把点 `(x, y)` 约束进工作区矩形内、窗口 `(win_w, win_h)`
/// 完整可见。工作区比窗口还窄时钉在左/上缘（单测锚定）。
fn clamp_to_work(x: f32, y: f32, work: (f32, f32, f32, f32), win_w: f32, win_h: f32) -> (f32, f32) {
    let (l, t, r, b) = work;
    let max_x = (r - win_w).max(l);
    let max_y = (b - win_h).max(t);
    (x.clamp(l, max_x), y.clamp(t, max_y))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// N21：clamp——界内不动；越界拉回；窗口大于工作区钉左/上缘。
    #[test]
    fn n21_clamp_to_work() {
        // 工作区 (0,0)-(1920,1040)，窗口 650×532
        assert_eq!(
            clamp_to_work(100.0, 200.0, (0.0, 0.0, 1920.0, 1040.0), 650.0, 532.0),
            (100.0, 200.0)
        );
        assert_eq!(
            clamp_to_work(5000.0, 200.0, (0.0, 0.0, 1920.0, 1040.0), 650.0, 532.0),
            (1270.0, 200.0),
            "右越界拉回"
        );
        assert_eq!(
            clamp_to_work(-50.0, 2000.0, (0.0, 0.0, 1920.0, 1040.0), 650.0, 532.0),
            (0.0, 508.0),
            "左/下越界拉回"
        );
        // 工作区比窗口窄（800 宽 < 窗口 900）：钉左缘
        assert_eq!(
            clamp_to_work(50.0, 0.0, (0.0, 0.0, 800.0, 1040.0), 900.0, 532.0).0,
            0.0
        );
        // 副屏负象限工作区
        assert_eq!(
            clamp_to_work(-1500.0, 0.0, (-1920.0, 0.0, 0.0, 1040.0), 650.0, 532.0),
            (-1500.0, 0.0),
            "副屏界内不动"
        );
    }
}
