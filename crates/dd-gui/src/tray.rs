//! Windows 系统托盘（设计稿 10C，v4.5；决策 D22–D25）。
//!
//! 职责（10C.1 规格表）：
//! - 常驻托盘图标（应用 `.ico`，D22：逻辑 16×16，物理按 DPI 取档；
//!   无 badge、无状态变化）；
//! - 左键单击 = 切换面板（D23，与全局热键 `Win+Alt+Space` 同一语义）；
//! - 右键 = 系统原生菜单（D24，`TrackPopupMenu`；固定 4 项见 [`menu_event`]），
//!   弹出前 `SetForegroundWindow`（官方要求，否则点击菜单外不关闭）；
//! - Tooltip 静态「dd-run — Win+Alt+Space 呼出」（D25）。
//!
//! 实现（对齐 [`hotkey`] 的线程模型）：独立线程创建隐藏窗口 → 注册托盘 →
//! `GetMessage` 消息循环；事件经 channel 发回主线程并 `request_repaint()`
//! （窗口隐藏时 egui 可能停止重绘）。**失败降级**：托盘创建失败只记日志、
//! 线程退出（热键仍可用）——区别于热键的 fail-fast，托盘缺失不致命。
//!
//! 图标来源：`assets/app.ico`（`include_bytes!`，由 `tools/gen_icon.py`
//! 按 10C.3 几何生成）**运行时物化**到 `cache_dir()/app.ico` 后
//! `LoadImageW(LR_LOADFROMFILE)` 加载——偏离记档：设计稿原写 winres 嵌入
//! exe 资源，windows-gnu 工具链无 windres，改为字节内嵌 + 物化（效果等价）。
//! 托盘生命周期 = 进程生命周期：退出直接结束进程，不主动 `NIM_DELETE`
//! （进程死亡时系统自动移除图标）。

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

/// 托盘菜单语言（v4.13 D38）：原子量跨线程共享生效语言——菜单**每次右键
/// 即席创建**（`show_menu`），读此值即可随设置页切换，无需重建托盘。
/// 编码：0 = ZhCn（默认），1 = EnUs（FollowSystem 不会传入，主线程已解析）。
static TRAY_LANG: AtomicU8 = AtomicU8::new(0);

/// 主线程在启动时与设置页切换语言时调用（`apply_lang`）。
pub fn set_tray_lang(lang: dd_gui::settings::Lang) {
    let code = match lang {
        dd_gui::settings::Lang::EnUs => 1,
        _ => 0,
    };
    TRAY_LANG.store(code, Ordering::Relaxed);
}

/// 托盘线程侧读取当前生效语言（`t()` 查表用）。
fn tray_lang() -> dd_gui::settings::Lang {
    if TRAY_LANG.load(Ordering::Relaxed) == 1 {
        dd_gui::settings::Lang::EnUs
    } else {
        dd_gui::settings::Lang::ZhCn
    }
}

/// 托盘菜单事件（10C.2 菜单项 → 行为映射）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// 「显示/隐藏面板」/ 左键单击：切换面板（= D23 左键 / 全局热键同款）。
    Toggle,
    /// 「设置」：显示面板并切到设置视图（D3）。
    OpenSettings,
    /// 「退出」：唯一显式退出入口（低危，不二次确认）。
    Exit,
}

/// 托盘 Tooltip（D25：静态字符串，热键可配置化后跟随拼接）。
/// v4.13 D38：改为**语言中立**文案（去掉中文动词）——Tooltip 在 NIM_ADD 时
/// 一次写入，随语言切换需跨线程 NIM_MODIFY，复杂度不成比例；菜单文案已随
/// 语言，Tooltip 中立化后整体无残留。
#[cfg(windows)]
const TOOLTIP: &str = "dd-run — Win+Alt+Space";

/// 托盘逻辑尺寸（D22：逻辑 16×16，物理随 DPI 取档）。
#[cfg(windows)]
const LOGICAL_ICON_PX: i32 = 16;

/// 托盘回调消息（经典模式：`lParam` = 鼠标消息，`wParam` = 图标 id）。
#[cfg(windows)]
const TRAY_MSG: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;

/// 原生菜单命令 id（10C.2 固定 4 项：两项 + 分隔线 + 退出）。
#[cfg(windows)]
mod menu_id {
    pub const TOGGLE: u32 = 1;
    pub const SETTINGS: u32 = 2;
    pub const EXIT: u32 = 3;
}

/// 内嵌应用图标（`tools/gen_icon.py` 按 10C.3 几何生成；BMP 条目 16–48px，
/// `LoadImageW` 全版本兼容）。BMP 条目顺序即 D22 DPI 档：16/20/24/32（+48 富余）。
#[cfg(windows)]
const APP_ICO: &[u8] = include_bytes!("../assets/app.ico");

/// `str` → NUL 结尾 UTF-16（Win32 宽字符串入口）。纯函数。
#[cfg(windows)]
pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// DPI → 托盘物理边长（四舍五入；D22 取档：96→16 / 120→20 / 144→24 / 192→32）。纯函数。
#[cfg(windows)]
const fn icon_px_for_dpi(dpi: i32) -> i32 {
    (LOGICAL_ICON_PX * dpi + 48) / 96
}

/// 原生菜单命令 id → 事件（10C.2 映射；0 = 未点击任何项）。纯函数。
#[cfg(windows)]
const fn menu_event(id: u32) -> Option<TrayEvent> {
    match id {
        menu_id::TOGGLE => Some(TrayEvent::Toggle),
        menu_id::SETTINGS => Some(TrayEvent::OpenSettings),
        menu_id::EXIT => Some(TrayEvent::Exit),
        _ => None,
    }
}

/// 托盘线程句柄 + 事件接收端（结构对齐 [`hotkey::HotkeyThread`]）。
pub struct TrayThread {
    /// 供 eframe 线程消费的事件接收端。
    pub events: Receiver<TrayEvent>,
    /// 线程 join 句柄（进程退出即回收）。
    _handle: thread::JoinHandle<()>,
}

impl TrayThread {
    /// 启动托盘线程（Windows）。
    ///
    /// `click_flag`：左键 / 菜单「显示/隐藏面板」的**点击在途旗标**——发送
    /// `Toggle` 前置位，主线程消费该事件后复位。用途：面板可见时点击托盘，
    /// 任务栏会先夺走焦点触发失焦自动隐藏（`handle_focus_loss`），随后
    /// Toggle 到达时 `visible` 已是 false → 又 show → 「闪黑又展示」竞态
    /// （真机 2026-09-05 反馈）。失焦隐藏遇旗标跳过一次，让 Toggle 完成唯一
    /// 一次干净的 hide。旗标与 Toggle 事件严格成对（置位后必 send，消费即清），
    /// 无陈旧风险。
    #[cfg(windows)]
    pub fn spawn(ctx: eframe::egui::Context, click_flag: Arc<AtomicBool>) -> Self {
        let (tx, rx) = mpsc::channel::<TrayEvent>();
        let handle = thread::Builder::new()
            .name("dd-tray".into())
            .spawn(move || message_loop(tx, ctx, click_flag))
            .expect("failed to spawn tray thread");
        Self {
            events: rx,
            _handle: handle,
        }
    }

    /// 非 Windows 平台：无托盘，事件通道永远为空（跨平台编译占位，
    /// 策略同 `hotkey`——macOS `NSStatusItem` / Linux `AppIndicator` 留平台轮）。
    #[cfg(not(windows))]
    pub fn spawn(_ctx: eframe::egui::Context, _click_flag: Arc<AtomicBool>) -> Self {
        let (_tx, rx) = mpsc::channel::<TrayEvent>();
        Self {
            events: rx,
            _handle: thread::Builder::new()
                .name("dd-tray-dummy".into())
                .spawn(|| std::thread::sleep(std::time::Duration::MAX))
                .expect("failed to spawn dummy tray thread"),
        }
    }
}

/// 注入窗口过程的共享状态（`GWLP_USERDATA` 裸指针持有）。
#[cfg(windows)]
struct TrayState {
    tx: Sender<TrayEvent>,
    ctx: eframe::egui::Context,
    /// Toggle 点击在途旗标（见 [`TrayThread::spawn`]）。
    click_flag: Arc<AtomicBool>,
}

/// 物化内嵌 `.ico` 到 `cache_dir()/app.ico`（内容变化即重写，幂等）。
/// 返回文件路径；无缓存目录 / 写盘失败 → `None`（调用方降级不加载托盘）。
#[cfg(windows)]
fn ensure_icon_file() -> Option<std::path::PathBuf> {
    use std::fs;
    let path = dd_host::manifest::cache_dir()?.join("app.ico");
    let stale = match fs::read(&path) {
        Ok(bytes) => bytes != APP_ICO,
        Err(_) => true,
    };
    if stale {
        fs::write(&path, APP_ICO)
            .map_err(|e| eprintln!("[dd-gui] app.ico 物化失败（托盘降级）：{e}"))
            .ok()?;
    }
    Some(path)
}

/// 托盘线程主体：物化图标 → 建隐藏窗口 → 注册托盘 → 消息循环。
///
/// 任何一步失败 → 记一次日志并返回（降级：面板仍可经热键使用；
/// 「退出」入口缺失可接受，托盘不可用的场景由用户经任务管理器结束）。
#[cfg(windows)]
fn message_loop(tx: Sender<TrayEvent>, ctx: eframe::egui::Context, click_flag: Arc<AtomicBool>) {
    use windows_sys::Win32::Graphics::Gdi::{GetDC, GetDeviceCaps, ReleaseDC, LOGPIXELSX};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DispatchMessageW, GetMessageW, LoadImageW, RegisterClassW,
        SetWindowLongPtrW, TranslateMessage, IMAGE_ICON, LR_LOADFROMFILE, MSG, WNDCLASSW,
    };

    // 1) 物化 + 加载图标（按 DPI 取档，D22）。
    let Some(ico_path) = ensure_icon_file() else {
        return;
    };
    let dpi = unsafe {
        let hdc = GetDC(std::ptr::null_mut());
        if hdc.is_null() {
            96 // 取不到屏幕 DC → 按 100% 档
        } else {
            let dpi = GetDeviceCaps(hdc, LOGPIXELSX as i32);
            ReleaseDC(std::ptr::null_mut(), hdc);
            if dpi > 0 {
                dpi
            } else {
                96
            }
        }
    };
    let px = icon_px_for_dpi(dpi);
    let path_wide = to_wide(&ico_path.to_string_lossy());
    let hicon = unsafe {
        LoadImageW(
            std::ptr::null_mut(), // LR_LOADFROMFILE：忽略 hinst
            path_wide.as_ptr(),
            IMAGE_ICON,
            px,
            px,
            LR_LOADFROMFILE,
        )
    };
    if hicon.is_null() {
        eprintln!(
            "[dd-gui] 托盘图标加载失败（{ico_path:?} @ {px}px，托盘降级）：{}",
            std::io::Error::last_os_error()
        );
        return;
    }

    // 2) 隐藏窗口（托盘回调的宿主；不 ShowWindow，不参与任务栏）。
    let (hwnd, _class_name) = unsafe {
        let hinstance = GetModuleHandleW(std::ptr::null());
        let class_name = to_wide("dd-run-tray");
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(tray_wndproc);
        wc.hInstance = hinstance;
        wc.lpszClassName = class_name.as_ptr();
        if RegisterClassW(&wc) == 0 {
            eprintln!(
                "[dd-gui] 托盘窗口类注册失败（托盘降级）：{}",
                std::io::Error::last_os_error()
            );
            return;
        }
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            0, // 无样式：隐藏的顶层窗口（不调用 ShowWindow）
            0,
            0,
            0,
            0,
            std::ptr::null_mut(), // 无父窗口
            std::ptr::null_mut(), // 无菜单
            hinstance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            eprintln!(
                "[dd-gui] 托盘窗口创建失败（托盘降级）：{}",
                std::io::Error::last_os_error()
            );
            return;
        }
        let state = Box::into_raw(Box::new(TrayState {
            tx,
            ctx,
            click_flag,
        }));
        SetWindowLongPtrW(
            hwnd,
            windows_sys::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            state as isize,
        );
        (hwnd, class_name)
    };

    // 3) 注册托盘（D22/D24/D25：ICON + 回调消息 + 静态 Tooltip）。
    unsafe {
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = TRAY_MSG;
        nid.hIcon = hicon;
        let tip = to_wide(TOOLTIP);
        nid.szTip[..tip.len()].copy_from_slice(&tip); // 长度 ≤128 由测试守卫
        if Shell_NotifyIconW(NIM_ADD, &nid) == 0 {
            eprintln!(
                "[dd-gui] Shell_NotifyIconW(NIM_ADD) 失败（托盘降级）：{}",
                std::io::Error::last_os_error()
            );
            return;
        }
    }
    eprintln!("[dd-gui] 托盘已注册（{px}px 图标，dpi={dpi}）");

    // 4) 消息循环（结构同 hotkey：GetMessage 阻塞，进程退出即终止线程）。
    loop {
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        unsafe {
            let r = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if r == 0 || r == -1 {
                break; // WM_QUIT / 错误 → 线程结束
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// 托盘窗口过程：处理回调消息（左键 / 右键）与销毁。
#[cfg(windows)]
unsafe extern "system" fn tray_wndproc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, GetWindowLongPtrW, PostQuitMessage, GWLP_USERDATA, WM_DESTROY,
        WM_LBUTTONUP, WM_RBUTTONUP,
    };
    let _ = wparam;
    unsafe {
        // CreateWindowExW 期间（WM_CREATE 等）userdata 尚未注入 → 走默认过程。
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayState;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let state = &*ptr;
        if msg == TRAY_MSG {
            let mouse = lparam as u32; // 经典模式：lParam 低字 = 鼠标消息
            match mouse {
                WM_LBUTTONUP => {
                    // D23：左键单击 = 切换面板（与热键同语义）。先置「点击在途」
                    // 旗标再发事件（见 spawn 文档：避免失焦隐藏与 Toggle 竞态）。
                    state.click_flag.store(true, Ordering::Relaxed);
                    let _ = state.tx.send(TrayEvent::Toggle);
                    state.ctx.request_repaint();
                }
                WM_RBUTTONUP => {
                    show_menu(hwnd, state);
                }
                _ => {}
            }
            0
        } else if msg == WM_DESTROY {
            PostQuitMessage(0);
            0
        } else {
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

/// 右键弹出系统原生菜单（D24：固定 4 项 + 分隔线；TrackPopupMenu 前置
/// `SetForegroundWindow`，关闭后投 `WM_NULL`——官方要求，缺一则外点不关闭）。
#[cfg(windows)]
unsafe fn show_menu(hwnd: windows_sys::Win32::Foundation::HWND, state: &TrayState) {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, PostMessageW, SetForegroundWindow,
        TrackPopupMenu, MF_SEPARATOR, MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD,
        WM_NULL,
    };
    unsafe {
        SetForegroundWindow(hwnd);
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        // accel 列经 `\t` 呈现（原生菜单惯例）；文案按当前生效语言（D38）。
        let lang = tray_lang();
        let toggle = to_wide(crate::text::t(lang, "tray.toggle"));
        let settings = to_wide(crate::text::t(lang, "tray.settings"));
        let exit = to_wide(crate::text::t(lang, "tray.exit"));
        AppendMenuW(menu, MF_STRING, menu_id::TOGGLE as usize, toggle.as_ptr());
        AppendMenuW(
            menu,
            MF_STRING,
            menu_id::SETTINGS as usize,
            settings.as_ptr(),
        );
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, menu_id::EXIT as usize, exit.as_ptr());
        let mut pt = POINT::default();
        GetCursorPos(&mut pt);
        // TPM_RETURNCMD：返回值 = 所选命令 id（0 = 未选 / Esc）。
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_LEFTALIGN | TPM_BOTTOMALIGN,
            pt.x,
            pt.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        DestroyMenu(menu);
        PostMessageW(hwnd, WM_NULL, 0, 0);
        if let Some(ev) = menu_event(cmd as u32) {
            // 菜单「显示/隐藏面板」同样走 Toggle → 前置点击在途旗标（同左键）。
            if ev == TrayEvent::Toggle {
                state.click_flag.store(true, Ordering::Relaxed);
            }
            let _ = state.tx.send(ev);
            state.ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn to_wide_appends_nul_terminator() {
        assert_eq!(to_wide(""), vec![0]);
        assert_eq!(to_wide("dd"), vec![100, 100, 0]);
        assert_eq!(to_wide("—"), vec![0x2014, 0]); // U+2014（BMP 内单单元）
    }

    #[cfg(windows)]
    #[test]
    fn tooltip_fits_sztip_buffer() {
        // NOTIFYICONDATAW.szTip = [u16; 128]（含 NUL）。
        let units = TOOLTIP.encode_utf16().count();
        assert!(units < 128, "tooltip 过长：{units}");
        assert!(TOOLTIP.contains("Win+Alt+Space"));
    }

    #[cfg(windows)]
    #[test]
    fn dpi_takes_design_doc_tiers() {
        // D22：100%→16 / 125%→20 / 150%→24 / 200%→32（四舍五入）。
        assert_eq!(icon_px_for_dpi(96), 16);
        assert_eq!(icon_px_for_dpi(120), 20);
        assert_eq!(icon_px_for_dpi(144), 24);
        assert_eq!(icon_px_for_dpi(192), 32);
        // 非整档：165% → (16*264+48)/96 = 44（整数像素向下取整）。
        assert_eq!(icon_px_for_dpi(264), 44);
    }

    #[cfg(windows)]
    #[test]
    fn menu_maps_fixed_four_items() {
        assert_eq!(menu_event(menu_id::TOGGLE), Some(TrayEvent::Toggle));
        assert_eq!(menu_event(menu_id::SETTINGS), Some(TrayEvent::OpenSettings));
        assert_eq!(menu_event(menu_id::EXIT), Some(TrayEvent::Exit));
        // 分隔线 / 未点击（TrackPopupMenu 未选返回 0）→ None。
        assert_eq!(menu_event(0), None);
        assert_eq!(menu_event(999), None);
    }

    #[cfg(windows)]
    #[test]
    fn embedded_ico_has_dpi_tier_entries() {
        // ICO 容器：ICONDIR(6) + ICONDIRENTRY(16)×n。宽在偏移 6+16i（0 = 256）。
        assert!(APP_ICO.len() > 22, "ico 容器不完整");
        let count = u16::from_le_bytes([APP_ICO[4], APP_ICO[5]]) as usize;
        assert_eq!(count, 5, "应有 16/20/24/32/48 五档条目");
        let widths: Vec<usize> = (0..count)
            .map(|i| {
                let base = 6 + 16 * i;
                if APP_ICO[base] == 0 {
                    256
                } else {
                    APP_ICO[base] as usize
                }
            })
            .collect();
        assert_eq!(widths, vec![16, 20, 24, 32, 48]);
    }
}
