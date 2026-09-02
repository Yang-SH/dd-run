//! Windows 全局热键：`Win+Alt+Space` 唤起/隐藏面板。
//!
//! 实现：独立线程调用 [`RegisterHotKey`]，随后进入 `GetMessage` 消息循环；
//! 收到 `WM_HOTKEY` 后把 [`HotkeyEvent::Toggle`] 经 channel 发回主线程，
//! 并触发 egui 重绘（保证窗口处于隐藏态时也能被唤醒）。
//!
//! `RegisterHotKey` 必须在执行消息循环的同一线程上调用，因此这里
//! 用专用线程持有热键注册与消息循环，主线程只消费 channel。

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

/// 热键修饰键：`Win` + `Alt`（设计文档 §4.3：`Win+Alt+Space` 唤起/隐藏）。
#[cfg(windows)]
pub const HOTKEY_MODIFIERS: u32 = windows_sys::Win32::UI::Input::KeyboardAndMouse::MOD_WIN
    | windows_sys::Win32::UI::Input::KeyboardAndMouse::MOD_ALT
    | windows_sys::Win32::UI::Input::KeyboardAndMouse::MOD_NOREPEAT;

/// 热键键码：空格（`VK_SPACE` = 0x20）。
pub const HOTKEY_VK: u32 = 0x20;

/// 内部热键 id（仅本进程内区分多个热键用）。
const HOTKEY_ID: i32 = 0xDD01;

/// 热键事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// 按下唤起/隐藏组合键 → 切换面板可见性。
    Toggle,
}

/// 热键线程句柄 + 事件接收端。
pub struct HotkeyThread {
    /// 供 eframe 线程消费的事件接收端。
    pub events: Receiver<HotkeyEvent>,
    /// 线程 join 句柄（drop 时不等待，进程退出即回收）。
    _handle: thread::JoinHandle<()>,
}

impl HotkeyThread {
    /// 注册 `Win+Alt+Space` 并启动消息循环线程。
    ///
    /// 持有 `egui::Context` 克隆：窗口隐藏时 egui 可能停止持续重绘，
    /// 热键线程每次发事件后主动 `request_repaint()` 唤醒它。
    ///
    /// # Panics
    /// 注册失败（热键被占用等）时 panic——R1 尖峰阶段采用快速失败，
    /// 正式版会改为错误传播 + 降级提示。
    #[cfg(windows)]
    pub fn spawn(ctx: eframe::egui::Context) -> Self {
        let (tx, rx) = mpsc::channel::<HotkeyEvent>();
        let handle = thread::Builder::new()
            .name("dd-hotkey".into())
            .spawn(move || message_loop(tx, ctx))
            .expect("failed to spawn hotkey thread");
        Self {
            events: rx,
            _handle: handle,
        }
    }

    /// 非 Windows 平台：不注册热键，事件通道永远为空（跨平台编译占位）。
    #[cfg(not(windows))]
    pub fn spawn(_ctx: eframe::egui::Context) -> Self {
        let (_tx, rx) = mpsc::channel::<HotkeyEvent>();
        Self {
            events: rx,
            _handle: thread::Builder::new()
                .name("dd-hotkey-dummy".into())
                .spawn(|| std::thread::sleep(std::time::Duration::MAX))
                .expect("failed to spawn dummy hotkey thread"),
        }
    }
}

/// 热键线程主体：注册 → 消息循环 → 发事件。
#[cfg(windows)]
fn message_loop(tx: Sender<HotkeyEvent>, ctx: eframe::egui::Context) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::RegisterHotKey;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, TranslateMessage, MSG, WM_HOTKEY,
    };

    unsafe {
        let ok = RegisterHotKey(
            std::ptr::null_mut(), // 线程级热键（不需要窗口；HWND = *mut c_void）
            HOTKEY_ID,
            HOTKEY_MODIFIERS,
            HOTKEY_VK,
        );
        if ok == 0 {
            panic!(
                "RegisterHotKey failed (Win+Alt+Space): {}",
                std::io::Error::last_os_error()
            );
        }
    }

    loop {
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        unsafe {
            // GetMessageW 返回 0 表示收到 WM_QUIT，-1 表示错误。
            let r = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if r == 0 {
                break;
            }
            if r == -1 {
                eprintln!("GetMessage failed");
                break;
            }
            if msg.message == WM_HOTKEY {
                let _ = tx.send(HotkeyEvent::Toggle);
                ctx.request_repaint();
            }
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
    // 注：进程退出时线程随之终止；不主动 UnregisterHotKey（生命周期同进程）。
}
