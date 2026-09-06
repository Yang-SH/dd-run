//! Windows 全局热键：唤起/隐藏面板（默认 `Win+Alt+Space`，M6 批次 6.3 起可自定义）。
//!
//! 实现：独立线程调用 [`RegisterHotKey`]（线程级热键），随后进入 `GetMessage`
//! 消息循环；收到 `WM_HOTKEY` 后把 [`HotkeyEvent::Toggle`] 经 channel 发回主
//! 线程并触发 egui 重绘（窗口隐藏时也能被唤醒）。
//!
//! **重注册（M6 批次 6.3）**：设置页更改热键后，主线程经
//! [`HotkeyThread::re_register`]（`PostThreadMessageW` 自定义消息）命令热键
//! 线程解注册旧键并注册新键；结果经 [`HotkeyEvent::ReRegistered`] 回发——
//! 成功 → UI Toast 提示；失败（组合键被占用）→ 热键线程自动回滚旧键，
//! UI 提示失败并还原设置。**启动注册失败不再 panic**（旧实现的快速失败
//! 正式废除）：降级为「无全局热键」运行 + 一次事件通知，可在设置页换键修复。

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

/// 热键修饰键默认值：Win + Alt（M6 起可自定义，持久化于 settings.hotkey_mods）。
#[cfg(windows)]
pub const HOTKEY_MODIFIERS_DEFAULT: u32 = windows_sys::Win32::UI::Input::KeyboardAndMouse::MOD_WIN
    | windows_sys::Win32::UI::Input::KeyboardAndMouse::MOD_ALT;

/// 热键键码默认值：空格（VK_SPACE = 0x20）。
pub const HOTKEY_VK_DEFAULT: u32 = 0x20;

/// 内部热键 id（仅本进程内区分多个热键用）。
const HOTKEY_ID: i32 = 0xDD01;
/// 热键线程自定义命令消息（PostThreadMessageW 携带新 mods/vk）。
#[cfg(windows)]
const HOTKEY_CMD_MSG: u32 = 0x8002; // WM_APP + 2

/// 热键事件（热键线程 → 主线程）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// 按下唤起/隐藏组合键 → 切换面板可见性。
    Toggle,
    /// 重注册结果：true = 新热键已生效；false = 失败并已回滚旧键。
    ReRegistered(bool),
}

/// 热键重注册请求（UI → 热键线程，经 PostThreadMessageW）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyCommand {
    /// 修饰键掩码（MOD_* 位组合；注册时自动补 MOD_NOREPEAT）。
    pub mods: u32,
    /// 主键虚拟键码。
    pub vk: u32,
}

/// 热键线程句柄 + 事件接收端 + 重注册入口。
pub struct HotkeyThread {
    /// 供 eframe 线程消费的事件接收端。
    pub events: Receiver<HotkeyEvent>,
    /// join 句柄（drop 时不等待，进程退出即回收）。
    _handle: thread::JoinHandle<()>,
    /// 热键线程 id（PostThreadMessageW 目标；非 Windows = 0）。
    thread_id: u32,
}

impl HotkeyThread {
    /// 注册初始热键并启动消息循环线程。
    ///
    /// 持有 `egui::Context` 克隆：窗口隐藏时 egui 可能停止持续重绘，
    /// 热键线程每次发事件后主动 `request_repaint()` 唤醒它。
    ///
    /// **启动注册失败不 panic**（旧快速失败已废除）：降级为无热键运行并
    /// 发 [`HotkeyEvent::ReRegistered`](false)，可在设置页换键修复。
    #[cfg(windows)]
    pub fn spawn(ctx: eframe::egui::Context, mods: u32, vk: u32) -> Self {
        let (tx, rx) = mpsc::channel::<HotkeyEvent>();
        let (id_tx, id_rx) = mpsc::channel::<u32>();
        let handle = thread::Builder::new()
            .name("dd-hotkey".into())
            .spawn(move || message_loop(tx, ctx, id_tx, mods, vk))
            .expect("failed to spawn hotkey thread");
        // 线程启动即回发自身 id（阻塞等待，微秒级）
        let thread_id = id_rx.recv().unwrap_or(0);
        Self {
            events: rx,
            _handle: handle,
            thread_id,
        }
    }

    /// 命令热键线程解注册旧键并注册新键（异步；结果经 ReRegistered 事件回发）。
    #[cfg(windows)]
    pub fn re_register(&self, mods: u32, vk: u32) {
        use windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
        unsafe {
            // 返回 0（线程消息循环未就绪/已退出）仅忽略——结果由事件回发兜底
            let _ = PostThreadMessageW(self.thread_id, HOTKEY_CMD_MSG, mods as usize, vk as isize);
        }
    }

    /// 非 Windows 平台：不注册热键，事件通道永远为空（跨平台编译占位）。
    #[cfg(not(windows))]
    pub fn spawn(_ctx: eframe::egui::Context, _mods: u32, _vk: u32) -> Self {
        let (_tx, rx) = mpsc::channel::<HotkeyEvent>();
        Self {
            events: rx,
            _handle: thread::Builder::new()
                .name("dd-hotkey-dummy".into())
                .spawn(|| std::thread::sleep(std::time::Duration::MAX))
                .expect("failed to spawn dummy hotkey thread"),
            thread_id: 0,
        }
    }

    /// 非 Windows 平台：重注册为空操作。
    #[cfg(not(windows))]
    pub fn re_register(&self, _mods: u32, _vk: u32) {}

    /// 测试用桩：不注册热键、无事件（`make_app` 注入用）。
    pub fn dummy() -> Self {
        let (_tx, rx) = mpsc::channel::<HotkeyEvent>();
        Self {
            events: rx,
            _handle: thread::Builder::new()
                .name("dd-hotkey-dummy".into())
                .spawn(|| std::thread::sleep(std::time::Duration::MAX))
                .expect("failed to spawn dummy hotkey thread"),
            thread_id: 0,
        }
    }
}

/// 热键线程主体：注册 → 消息循环 → 发事件（含重注册命令处理）。
#[cfg(windows)]
fn message_loop(
    tx: Sender<HotkeyEvent>,
    ctx: eframe::egui::Context,
    id_tx: Sender<u32>,
    mods: u32,
    vk: u32,
) {
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, TranslateMessage, MSG, WM_HOTKEY,
    };

    let _ = id_tx.send(unsafe { GetCurrentThreadId() });

    // 当前生效组合（重注册失败回滚用）；NOREPEAT 统一在注册处补
    let mut current = (mods, vk);
    unsafe {
        let ok = RegisterHotKey(
            std::ptr::null_mut(), // 线程级热键（HWND = *mut c_void）
            HOTKEY_ID,
            current.0 | MOD_NOREPEAT,
            current.1,
        );
        if ok == 0 {
            // 启动失败降级（不 panic）：可能被其他启动器占用；设置页换键可修复
            eprintln!(
                "[dd-gui] 全局热键注册失败（{}+{}），降级为无热键运行——可在设置页更换",
                current.0, current.1
            );
            let _ = tx.send(HotkeyEvent::ReRegistered(false));
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
            } else if msg.message == HOTKEY_CMD_MSG {
                // 重注册命令：解旧注册新；失败自动回滚旧键并回发失败事件
                let new = (msg.wParam as u32, msg.lParam as i32 as u32);
                let _ = UnregisterHotKey(std::ptr::null_mut(), HOTKEY_ID);
                if RegisterHotKey(std::ptr::null_mut(), HOTKEY_ID, new.0 | MOD_NOREPEAT, new.1) != 0
                {
                    current = new;
                    let _ = tx.send(HotkeyEvent::ReRegistered(true));
                } else {
                    eprintln!("[dd-gui] 新热键注册失败（{}+{}），回滚旧键", new.0, new.1);
                    let re = RegisterHotKey(
                        std::ptr::null_mut(),
                        HOTKEY_ID,
                        current.0 | MOD_NOREPEAT,
                        current.1,
                    );
                    if re == 0 {
                        eprintln!("[dd-gui] 旧键回滚注册也失败——降级为无热键");
                    }
                    let _ = tx.send(HotkeyEvent::ReRegistered(false));
                }
                ctx.request_repaint();
            }
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
    // 注：进程退出时线程随之终止；不主动 UnregisterHotKey（生命周期同进程）。
}
