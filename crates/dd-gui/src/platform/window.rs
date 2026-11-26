//! 窗口定位：光标所在显示器工作区与唤起居中。

use crate::app::PaletteApp;
use eframe::egui;

#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq)]
enum WindowPlacement {
    Clamped(f32, f32),
    OversizedCentered(f32, f32),
}

#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy)]
struct TargetWorkArea {
    work: (f32, f32, f32, f32),
    ppp: f32,
}

#[cfg(any(windows, test))]
fn logical_work_size(work: (f32, f32, f32, f32), ppp: f32) -> (f32, f32) {
    let ppp = ppp.max(0.5);
    ((work.2 - work.0) / ppp, (work.3 - work.1) / ppp)
}

#[cfg(any(windows, test))]
fn physical_window_size(target: (f32, f32), ppp: f32) -> (f32, f32) {
    let ppp = ppp.max(0.5);
    (target.0 * ppp, target.1 * ppp)
}

#[cfg(any(windows, test))]
fn centered_position(work: (f32, f32, f32, f32), win_w: f32, win_h: f32) -> (f32, f32) {
    let work_w = (work.2 - work.0).max(0.0);
    let work_h = (work.3 - work.1).max(0.0);
    (
        work.0 + (work_w - win_w) * 0.5,
        work.1 + (work_h - win_h) * 0.5,
    )
}

#[cfg(any(windows, test))]
fn viewport_position(physical: (f32, f32), window_ppp: f32) -> (f32, f32) {
    let window_ppp = window_ppp.max(0.5);
    (physical.0 / window_ppp, physical.1 / window_ppp)
}

#[cfg(any(windows, test))]
fn place_window(
    x: f32,
    y: f32,
    work: (f32, f32, f32, f32),
    win_w: f32,
    win_h: f32,
) -> WindowPlacement {
    let work_w = (work.2 - work.0).max(0.0);
    let work_h = (work.3 - work.1).max(0.0);
    if win_w > work_w || win_h > work_h {
        let (x, y) = centered_position(work, win_w, win_h);
        WindowPlacement::OversizedCentered(x, y)
    } else {
        let (x, y) = clamp_to_work(x, y, work, win_w, win_h);
        WindowPlacement::Clamped(x, y)
    }
}

#[cfg(any(windows, test))]
fn restore_physical_position(
    saved: (f32, f32),
    area: TargetWorkArea,
    target: (f32, f32),
    window_ppp: f32,
) -> (f32, f32) {
    let (win_w, win_h) = physical_window_size(target, area.ppp);
    let placement = place_window(saved.0, saved.1, area.work, win_w, win_h);
    let (physical_x, physical_y) = match placement {
        WindowPlacement::Clamped(x, y) | WindowPlacement::OversizedCentered(x, y) => (x, y),
    };
    viewport_position((physical_x, physical_y), window_ppp)
}

#[cfg(windows)]
fn target_work_area_at(point: (i32, i32), fallback_ppp: f32) -> Option<TargetWorkArea> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

    let monitor = unsafe {
        MonitorFromPoint(
            POINT {
                x: point.0,
                y: point.1,
            },
            MONITOR_DEFAULTTONEAREST,
        )
    };
    if monitor.is_null() {
        log::debug!("[dd-gui] 窗口定位：MonitorFromPoint 无结果，保持原位显示");
        return None;
    }
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        log::warn!("[dd-gui] 窗口定位：GetMonitorInfoW 失败，保持原位显示");
        return None;
    }
    let mut dpi_x = 0u32;
    let mut dpi_y = 0u32;
    let hr = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
    let ppp = if hr >= 0 && dpi_x > 0 && dpi_y > 0 {
        ((dpi_x + dpi_y) as f32 / 2.0) / 96.0
    } else {
        log::debug!("[dd-gui] 窗口定位：GetDpiForMonitor 失败，回落当前窗口 ppp");
        fallback_ppp.max(0.5)
    };
    Some(TargetWorkArea {
        work: (
            info.rcWork.left as f32,
            info.rcWork.top as f32,
            info.rcWork.right as f32,
            info.rcWork.bottom as f32,
        ),
        ppp,
    })
}

#[cfg(windows)]
fn cursor_target_work_area(fallback_ppp: f32) -> Option<TargetWorkArea> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point) } == 0 {
        log::warn!("[dd-gui] 窗口定位：GetCursorPos 失败，保持原位显示");
        return None;
    }
    target_work_area_at((point.x, point.y), fallback_ppp)
}

impl PaletteApp {
    /// 光标所在显示器工作区的逻辑尺寸；换算使用目标屏 DPI。
    #[cfg(windows)]
    pub(crate) fn cursor_work_area(&self, ctx: &egui::Context) -> Option<(f32, f32)> {
        let target = cursor_target_work_area(ctx.pixels_per_point())?;
        Some(logical_work_size(target.work, target.ppp))
    }

    /// 非 Windows 占位：取不到工作区，返回 `None`。
    #[cfg(not(windows))]
    pub(crate) fn cursor_work_area(&self, _ctx: &egui::Context) -> Option<(f32, f32)> {
        None
    }

    /// 以目标屏 DPI 换算窗口物理尺寸，再用当前窗口 ppp 转换 egui 逻辑位置。
    #[cfg(windows)]
    pub(crate) fn center_on_cursor(&self, ctx: &egui::Context, target: (f32, f32)) {
        let window_ppp = ctx.pixels_per_point().max(0.5);
        let Some(area) = cursor_target_work_area(window_ppp) else {
            return;
        };
        let (win_w, win_h) = physical_window_size(target, area.ppp);
        let (cx, cy) = centered_position(area.work, win_w, win_h);
        let (x, y) = viewport_position((cx, cy), window_ppp);
        log::debug!(
            "[dd-gui] 唤起居中：目标屏 ppp={:.2} 窗口 ppp={:.2} 工作区={:?} 目标={}x{} → ({}, {})",
            area.ppp,
            window_ppp,
            area.work,
            target.0,
            target.1,
            x,
            y
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
    }

    /// 非 Windows 占位：退化为 egui 自带当前屏居中。
    #[cfg(not(windows))]
    pub(crate) fn center_on_cursor(&self, ctx: &egui::Context, _target: (f32, f32)) {
        if let Some(cmd) = egui::ViewportCommand::center_on_screen(ctx) {
            ctx.send_viewport_cmd(cmd);
        }
    }

    /// 恢复记忆位置；目标屏 DPI 用于换算，超小屏明确回落居中。
    #[cfg(windows)]
    pub(crate) fn restore_remembered_position(
        &self,
        ctx: &egui::Context,
        saved: egui::Pos2,
        target: (f32, f32),
    ) -> bool {
        let window_ppp = ctx.pixels_per_point().max(0.5);
        let bootstrap = ((saved.x * window_ppp) as i32, (saved.y * window_ppp) as i32);
        let Some(area) = target_work_area_at(bootstrap, window_ppp) else {
            return false;
        };
        let saved_physical = (saved.x * area.ppp, saved.y * area.ppp);
        let (win_w, win_h) = physical_window_size(target, area.ppp);
        let placement = place_window(saved_physical.0, saved_physical.1, area.work, win_w, win_h);
        if matches!(placement, WindowPlacement::OversizedCentered(_, _)) {
            log::debug!("[dd-gui] N21 窗口大于目标屏工作区，回落居中");
        }
        let (physical_x, physical_y) = match placement {
            WindowPlacement::Clamped(x, y) | WindowPlacement::OversizedCentered(x, y) => (x, y),
        };
        let (x, y) = viewport_position((physical_x, physical_y), window_ppp);
        log::debug!(
            "[dd-gui] N21 位置恢复：目标屏 ppp={:.2} saved=({:.0},{:.0}) → ({:.0},{:.0})",
            area.ppp,
            saved.x,
            saved.y,
            x,
            y
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
        true
    }

    /// 恢复物理像素位置；选屏不再依赖当前窗口 ppp。
    #[cfg(windows)]
    pub(crate) fn restore_remembered_physical_position(
        &self,
        ctx: &egui::Context,
        saved: (i32, i32),
        target: (f32, f32),
    ) -> bool {
        let window_ppp = ctx.pixels_per_point().max(0.5);
        let Some(area) = target_work_area_at(saved, window_ppp) else {
            return false;
        };
        let (x, y) =
            restore_physical_position((saved.0 as f32, saved.1 as f32), area, target, window_ppp);
        log::debug!(
            "[dd-gui] N21 物理位置恢复：目标屏 ppp={:.2} saved=({}, {}) → ({:.0},{:.0})",
            area.ppp,
            saved.0,
            saved.1,
            x,
            y
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
        true
    }

    /// 非 Windows 占位：不恢复（回落居中）。
    #[cfg(not(windows))]
    pub(crate) fn restore_remembered_physical_position(
        &self,
        _ctx: &egui::Context,
        _saved: (i32, i32),
        _target: (f32, f32),
    ) -> bool {
        false
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
}

/// N21 纯 clamp：把点 `(x, y)` 约束进工作区矩形内、窗口 `(win_w, win_h)`
/// 完整可见。工作区比窗口还窄时钉在左/上缘（单测锚定）。
/// 纯函数：把窗口左上角 clamp 进工作区（Windows 专用于记忆位置恢复；非 Windows
/// 无调用方——`cfg(any(windows, test))` 避免 check-linux/macos 的 dead_code 阻断）。
/// 单测锚定见 [`n21_clamp_to_work`]。
#[cfg(any(windows, test))]
fn clamp_to_work(x: f32, y: f32, work: (f32, f32, f32, f32), win_w: f32, win_h: f32) -> (f32, f32) {
    let (l, t, r, b) = work;
    let max_x = (r - win_w).max(l);
    let max_y = (b - win_h).max(t);
    (x.clamp(l, max_x), y.clamp(t, max_y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p1_17_mixed_dpi_geometry_uses_target_and_window_scales_separately() {
        let work = (-1920.0, 0.0, 0.0, 1080.0);
        assert_eq!(logical_work_size(work, 2.0), (960.0, 540.0));
        assert_eq!(physical_window_size((650.0, 440.0), 2.0), (1300.0, 880.0));
        let centered = centered_position(work, 1300.0, 880.0);
        assert_eq!(centered, (-1610.0, 100.0));
        assert_eq!(viewport_position(centered, 1.0), (-1610.0, 100.0));
        assert_eq!(viewport_position(centered, 1.25), (-1288.0, 80.0));
    }

    #[test]
    fn p1_17_oversized_window_falls_back_to_centered() {
        assert_eq!(
            place_window(300.0, 300.0, (100.0, 100.0, 900.0, 700.0), 900.0, 700.0),
            WindowPlacement::OversizedCentered(50.0, 50.0)
        );
        assert_eq!(
            place_window(-1500.0, 0.0, (-1920.0, 0.0, 0.0, 1080.0), 650.0, 532.0),
            WindowPlacement::Clamped(-1500.0, 0.0)
        );
    }

    #[test]
    fn p1_17_restart_uses_saved_physical_position_not_current_window_dpi() {
        let area = TargetWorkArea {
            work: (1920.0, 0.0, 3840.0, 1080.0),
            ppp: 2.0,
        };
        assert_eq!(
            restore_physical_position((2000.0, 100.0), area, (650.0, 440.0), 1.0),
            (2000.0, 100.0)
        );
    }

    #[test]
    fn p1_17_default_center_baseline_is_unchanged() {
        let work = (0.0, 0.0, 2880.0, 1800.0);
        let (win_w, win_h) = physical_window_size((650.0, 440.0), 1.5);
        let centered = centered_position(work, win_w, win_h);
        assert_eq!(centered, (952.5, 570.0));
        assert_eq!(viewport_position(centered, 1.5), (635.0, 380.0));
    }

    #[cfg(windows)]
    #[test]
    fn p1_17_primary_monitor_dpi_query_returns_usable_work_area() {
        let target = target_work_area_at((0, 0), 1.0).expect("主屏工作区可查询");
        assert!(target.ppp >= 1.0);
        assert!(logical_work_size(target.work, target.ppp).0 > 0.0);
        assert!(logical_work_size(target.work, target.ppp).1 > 0.0);
    }

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
