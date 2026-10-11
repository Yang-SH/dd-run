//! Windows 全局热键：唤起/隐藏面板（默认 `Win+Alt+Space`，M6 批次 6.3 起可自定义）。
//!
//! 实现：独立线程调用 [`RegisterHotKey`]（线程级热键），随后进入 `GetMessage`
//! 消息循环；收到 `WM_HOTKEY` 后把 [`HotkeyEvent::Toggle`] 经 channel 发回主
//! 线程并触发 egui 重绘（窗口隐藏时也能被唤醒）。
//!
//! **重注册（M6 批次 6.3；2026-09-30 seq 协议化）**：设置页更改热键后，主线程
//! 经 [`HotkeyThread::re_register`]（`PostThreadMessageW` 自定义消息）命令热键
//! 线程解注册旧键并注册新键；结果经 [`HotkeyEvent::ReRegistered`] 回发——
//! 成功 → UI 写设置 + Toast 提示；失败（组合键被占用）→ 热键线程自动回滚
//! 旧键，UI 行内提示（设置不变形）。**事件带 seq 序号**（v0.1.1 真机 bug：
//! 无配对的事件 + 单槽回滚快照在重试场景错位——「冲突只拦截一次，第二次
//! 假成功」，见 lifecycle.rs）——UI 按 seq 匹配，过期/未知事件一律忽略。
//! **启动注册失败不再 panic**（旧实现的快速失败正式废除）：降级为「无全局
//! 热键」运行 + 一次事件通知（seq=0），可在设置页换键修复。
//!
//! **非 Windows（O6 批次三，2026-10-11）**：`global-hotkey` crate 真实现替换
//! 早期跨平台编译占位——专用线程 + std mpsc 命令通道（Windows 的
//! `PostThreadMessageW` 对应物），事件/回发/seq 协议/回滚语义与 Windows
//! 逐位一致；Linux 仅 X11（Wayland 不支持 → manager 创建 Err 走既有降级
//! 路径）。位掩码口径不变（settings 约定），经纯函数映射为 crate 类型。

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
/// 热键线程自定义命令消息（PostThreadMessageW 携带 seq + mods/vk）。
#[cfg(windows)]
const HOTKEY_CMD_MSG: u32 = 0x8002; // WM_APP + 2

/// 启动注册结果专用 seq（UI 据此区分「启动注册」与「改绑确认」两类事件；
/// 改绑请求的 seq 从 1 起由 UI 递增，永不与 0 撞号）。
pub const HOTKEY_SEQ_STARTUP: u32 = 0;

/// 热键重注册请求（UI → 热键线程，经 PostThreadMessageW）。
/// seq：请求序号，原样随 [`HotkeyEvent::ReRegistered`] 回发——UI 事件配对的
/// 唯一依据（2026-09-30 seq 协议化：废除「按到达顺序盲配 + 单槽快照回滚」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyCommand {
    /// 请求序号（HOTKEY_SEQ_STARTUP = 启动注册，非 0 = 改绑确认）。
    pub seq: u32,
    /// 修饰键掩码（MOD_* 位组合；注册时自动补 MOD_NOREPEAT）。
    pub mods: u32,
    /// 主键虚拟键码。
    pub vk: u32,
}

impl HotkeyCommand {
    /// 打包进 `PostThreadMessageW` 的 (wParam, lParam)：wParam = seq，
    /// lParam = mods<<32 | vk（mods 仅 HOTKEY_MODS_MASK 4 位 + MOD_NOREPEAT
    /// 在注册处补，恒不溢出 32 位；lParam 为 isize，x64 下 64 位足够）。
    #[cfg(windows)]
    pub(crate) fn pack(&self) -> (usize, isize) {
        (
            self.seq as usize,
            ((self.mods as isize) << 32) | (self.vk as isize),
        )
    }

    /// [`Self::pack`] 的逆变换（roundtrip 单测锚定）。
    #[cfg(windows)]
    pub(crate) fn unpack(wparam: usize, lparam: isize) -> Self {
        Self {
            seq: wparam as u32,
            mods: ((lparam >> 32) as usize) as u32,
            vk: (lparam as usize) as u32, // 截取低 32 位
        }
    }
}

/// 热键事件（热键线程 → 主线程）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// 按下唤起/隐藏组合键 → 切换面板可见性。
    Toggle,
    /// 重注册结果：`seq` 对应请求序号（0 = 启动注册结果）；`ok` = 新热键是否
    /// 已生效（false 时设置保持不变）；`rolled_back` = ok=false 时旧键是否
    /// 回滚成功——false 表示**当前无任何生效热键**（旧键回滚也失败，UI 须
    /// 置 R-15「未注册」位，否则设置页声称旧键生效而实际为空）。
    ReRegistered {
        seq: u32,
        ok: bool,
        rolled_back: bool,
    },
    /// R-15：热键线程异常死亡（`GetMessageW` 返回 -1 等）——全局热键失效，
    /// 宿主置「未注册」状态位并提示（面板打开时可见）。
    Died,
}

/// 热键线程句柄 + 事件接收端 + 重注册入口。
pub struct HotkeyThread {
    /// 供 eframe 线程消费的事件接收端。
    pub events: Receiver<HotkeyEvent>,
    /// join 句柄（drop 时不等待，进程退出即回收）；线程创建失败时为
    /// `None`（R-05：降级为无热键运行，不 panic）。
    _handle: Option<thread::JoinHandle<()>>,
    /// 热键线程 id（PostThreadMessageW 目标；非 Windows = 0）。
    thread_id: u32,
    /// 非 Windows：重注册命令发送端（Windows 走 PostThreadMessageW，无此字段；
    /// 线程创建失败时发送端对端已死，send 静默失败由 UI 确认超时兜底）。
    #[cfg(not(windows))]
    cmd_tx: Option<Sender<HotkeyCommand>>,
}

impl HotkeyThread {
    /// 注册初始热键并启动消息循环线程。
    ///
    /// 持有 `egui::Context` 克隆：窗口隐藏时 egui 可能停止持续重绘，
    /// 热键线程每次发事件后主动 `request_repaint()` 唤醒它。
    ///
    /// **启动注册失败不 panic**（旧快速失败已废除）：降级为无热键运行并
    /// 发 [`HotkeyEvent::ReRegistered`](false)，可在设置页换键修复。
    /// 线程创建失败同口径（R-05）：`log::error!` + 回发失败事件后继续。
    #[cfg(windows)]
    pub fn spawn(ctx: eframe::egui::Context, mods: u32, vk: u32) -> Self {
        let (tx, rx) = mpsc::channel::<HotkeyEvent>();
        let (id_tx, id_rx) = mpsc::channel::<u32>();
        // 失败通知用克隆（tx 随闭包移交线程，创建失败时闭包被丢弃）
        let fail_tx = tx.clone();
        match thread::Builder::new()
            .name("dd-hotkey".into())
            .spawn(move || message_loop(tx, ctx, id_tx, mods, vk))
        {
            Ok(handle) => {
                // 线程启动即回发自身 id（阻塞等待，微秒级）
                let thread_id = id_rx.recv().unwrap_or(0);
                Self {
                    events: rx,
                    _handle: Some(handle),
                    thread_id,
                }
            }
            Err(e) => {
                log::error!("[dd-gui] 热键线程创建失败：{e} —— 降级为无全局热键运行");
                let _ = fail_tx.send(HotkeyEvent::ReRegistered {
                    seq: HOTKEY_SEQ_STARTUP,
                    ok: false,
                    rolled_back: false, // 无旧键可回滚
                });
                Self {
                    events: rx,
                    _handle: None,
                    thread_id: 0,
                }
            }
        }
    }

    /// 命令热键线程解注册旧键并注册新键（异步；结果带原 seq 经
    /// [`HotkeyEvent::ReRegistered`] 回发）。
    #[cfg(windows)]
    pub fn re_register(&self, cmd: HotkeyCommand) {
        use windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
        let (w, l) = cmd.pack();
        unsafe {
            // 返回 0（线程消息循环未就绪/已退出）仅忽略——结果由事件回发兜底
            //（事件不到时 UI 侧确认超时兜底，见 lifecycle.rs）。
            let _ = PostThreadMessageW(self.thread_id, HOTKEY_CMD_MSG, w, l);
        }
    }

    /// 非 Windows 平台：经 `global-hotkey` 注册热键并启动轮询线程（O6 批次三
    /// 真实现，语义与 Windows 侧逐位对齐——启动失败降级发
    /// [`HotkeyEvent::ReRegistered`](false)，线程创建失败同口径 R-05）。
    #[cfg(not(windows))]
    pub fn spawn(ctx: eframe::egui::Context, mods: u32, vk: u32) -> Self {
        let (tx, rx) = mpsc::channel::<HotkeyEvent>();
        let (cmd_tx, cmd_rx) = mpsc::channel::<HotkeyCommand>();
        // 失败通知用克隆（tx 随闭包移交线程，创建失败时闭包被丢弃）
        let fail_tx = tx.clone();
        match thread::Builder::new()
            .name("dd-hotkey".into())
            .spawn(move || message_loop_unix(tx, ctx, cmd_rx, mods, vk))
        {
            Ok(handle) => Self {
                events: rx,
                _handle: Some(handle),
                thread_id: 0,
                cmd_tx: Some(cmd_tx),
            },
            Err(e) => {
                log::error!("[dd-gui] 热键线程创建失败：{e} —— 降级为无全局热键运行");
                let _ = fail_tx.send(HotkeyEvent::ReRegistered {
                    seq: HOTKEY_SEQ_STARTUP,
                    ok: false,
                    rolled_back: false, // 无旧键可回滚
                });
                Self {
                    events: rx,
                    _handle: None,
                    thread_id: 0,
                    // 对端闭包已随 spawn 失败丢弃，re_register 静默失败（UI
                    // 确认超时兜底），字段留 Some 保接口一致。
                    cmd_tx: Some(cmd_tx),
                }
            }
        }
    }

    /// 非 Windows 平台：重注册命令经 std mpsc 发往热键线程（Windows 的
    /// `PostThreadMessageW` 对应物）；结果带原 seq 经
    /// [`HotkeyEvent::ReRegistered`] 回发，seq 协议一致。
    #[cfg(not(windows))]
    pub fn re_register(&self, cmd: HotkeyCommand) {
        // 对端已关闭（线程退出）时 send 返回 Err——静默放弃，UI 侧确认
        // 超时兜底（与 Windows 侧 PostThreadMessageW 返回 0 的忽略同口径）。
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(cmd);
        }
    }

    /// 测试用桩：不注册热键、无事件（`make_app` 注入用）。
    pub fn dummy() -> Self {
        let (_tx, rx) = mpsc::channel::<HotkeyEvent>();
        Self {
            events: rx,
            _handle: dummy_handle("dd-hotkey-dummy"),
            thread_id: 0,
            #[cfg(not(windows))]
            cmd_tx: None,
        }
    }

    /// R-15 测试注入：外部持有发送端的事件通道（`make_app_with` 用）。
    #[cfg(test)]
    pub(crate) fn for_events(rx: mpsc::Receiver<HotkeyEvent>) -> Self {
        Self {
            events: rx,
            _handle: dummy_handle("dd-hotkey-test"),
            thread_id: 0,
            #[cfg(not(windows))]
            cmd_tx: None,
        }
    }
}

/// 占位/测试桩的保活线程（R-05：创建失败仅记日志降级，不 panic）。
fn dummy_handle(name: &str) -> Option<thread::JoinHandle<()>> {
    match thread::Builder::new()
        .name(name.into())
        .spawn(|| std::thread::sleep(std::time::Duration::MAX))
    {
        Ok(h) => Some(h),
        Err(e) => {
            log::error!("[dd-gui] {name} 占位线程创建失败：{e}（占位桩降级继续）");
            None
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
            log::debug!(
                "[dd-gui] 全局热键注册失败（{}+{}），降级为无热键运行——可在设置页更换",
                current.0,
                current.1
            );
            let _ = tx.send(HotkeyEvent::ReRegistered {
                seq: HOTKEY_SEQ_STARTUP,
                ok: false,
                rolled_back: false, // 无旧键可回滚
            });
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
                // R-15：线程即将静默死亡 → 通知宿主置「未注册」位（原先仅
                // log::debug，全局热键失效用户无从知晓）。
                log::error!("[dd-gui] GetMessageW 失败——热键线程退出，全局热键失效");
                let _ = tx.send(HotkeyEvent::Died);
                break;
            }
            if msg.message == WM_HOTKEY {
                let _ = tx.send(HotkeyEvent::Toggle);
                ctx.request_repaint();
            } else if msg.message == HOTKEY_CMD_MSG {
                // 重注册命令（seq 配对协议）：解旧注册新；失败自动回滚旧键并
                // 回发失败事件。**UI 侧不再补发回滚命令**（旧实现的多余回滚
                // 会产生一次「假成功」事件，与下一次尝试错位配对）。
                let cmd = HotkeyCommand::unpack(msg.wParam, msg.lParam);
                let _ = UnregisterHotKey(std::ptr::null_mut(), HOTKEY_ID);
                if RegisterHotKey(
                    std::ptr::null_mut(),
                    HOTKEY_ID,
                    cmd.mods | MOD_NOREPEAT,
                    cmd.vk,
                ) != 0
                {
                    current = (cmd.mods, cmd.vk);
                    // 诊断日志（发行版 stderr 无控制台，开发期可见）：区分
                    // 「注册成功但 IME 层抢占按键」与「注册失败回滚」两类
                    // 「改绑不生效」真机反馈；seq 供与 UI 侧日志对账。
                    log::info!(
                        "[dd-gui] 全局热键注册成功（seq={}）：{}+{}",
                        cmd.seq,
                        dd_gui::settings::hotkey_mods_label(cmd.mods),
                        dd_gui::settings::hotkey_vk_label(cmd.vk)
                    );
                    let _ = tx.send(HotkeyEvent::ReRegistered {
                        seq: cmd.seq,
                        ok: true,
                        rolled_back: true, // 成功时无意义，恒 true
                    });
                } else {
                    log::warn!(
                        "[dd-gui] 新热键注册失败（seq={}，{}+{}），回滚旧键",
                        cmd.seq,
                        cmd.mods,
                        cmd.vk
                    );
                    let re = RegisterHotKey(
                        std::ptr::null_mut(),
                        HOTKEY_ID,
                        current.0 | MOD_NOREPEAT,
                        current.1,
                    );
                    if re == 0 {
                        log::warn!(
                            "[dd-gui] 旧键回滚注册也失败——降级为无热键（rolled_back=false）"
                        );
                    }
                    let _ = tx.send(HotkeyEvent::ReRegistered {
                        seq: cmd.seq,
                        ok: false,
                        rolled_back: re != 0,
                    });
                }
                ctx.request_repaint();
            }
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
    // 注：进程退出时线程随之终止；不主动 UnregisterHotKey（生命周期同进程）。
}

/// 设置位掩码 → `keyboard_types::Modifiers`（非 Windows；位值与 settings.rs
/// 约定一致：ALT=1/CONTROL=2/SHIFT=4/WIN=8）。WIN 位映射 SUPER——Linux 上
/// 为 Super 键、macOS 上为 Cmd。
#[cfg(not(windows))]
fn mods_mask_to_modifiers(mask: u32) -> global_hotkey::hotkey::Modifiers {
    use global_hotkey::hotkey::Modifiers;
    let mut m = Modifiers::empty();
    if mask & 0b0001 != 0 {
        m |= Modifiers::ALT;
    }
    if mask & 0b0010 != 0 {
        m |= Modifiers::CONTROL;
    }
    if mask & 0b0100 != 0 {
        m |= Modifiers::SHIFT;
    }
    if mask & 0b1000 != 0 {
        m |= Modifiers::SUPER;
    }
    m
}

/// F1–F12 的 Code 表（VK 0x70..=0x7B 与 F1..=F12 顺序一一对应）。
#[cfg(not(windows))]
const F_KEY_CODES: [global_hotkey::hotkey::Code; 12] = [
    global_hotkey::hotkey::Code::F1,
    global_hotkey::hotkey::Code::F2,
    global_hotkey::hotkey::Code::F3,
    global_hotkey::hotkey::Code::F4,
    global_hotkey::hotkey::Code::F5,
    global_hotkey::hotkey::Code::F6,
    global_hotkey::hotkey::Code::F7,
    global_hotkey::hotkey::Code::F8,
    global_hotkey::hotkey::Code::F9,
    global_hotkey::hotkey::Code::F10,
    global_hotkey::hotkey::Code::F11,
    global_hotkey::hotkey::Code::F12,
];

/// A–Z 的 Code 表（VK 0x41..=0x5A 与 KeyA..=KeyZ 顺序一一对应）。
#[cfg(not(windows))]
const LETTER_KEY_CODES: [global_hotkey::hotkey::Code; 26] = [
    global_hotkey::hotkey::Code::KeyA,
    global_hotkey::hotkey::Code::KeyB,
    global_hotkey::hotkey::Code::KeyC,
    global_hotkey::hotkey::Code::KeyD,
    global_hotkey::hotkey::Code::KeyE,
    global_hotkey::hotkey::Code::KeyF,
    global_hotkey::hotkey::Code::KeyG,
    global_hotkey::hotkey::Code::KeyH,
    global_hotkey::hotkey::Code::KeyI,
    global_hotkey::hotkey::Code::KeyJ,
    global_hotkey::hotkey::Code::KeyK,
    global_hotkey::hotkey::Code::KeyL,
    global_hotkey::hotkey::Code::KeyM,
    global_hotkey::hotkey::Code::KeyN,
    global_hotkey::hotkey::Code::KeyO,
    global_hotkey::hotkey::Code::KeyP,
    global_hotkey::hotkey::Code::KeyQ,
    global_hotkey::hotkey::Code::KeyR,
    global_hotkey::hotkey::Code::KeyS,
    global_hotkey::hotkey::Code::KeyT,
    global_hotkey::hotkey::Code::KeyU,
    global_hotkey::hotkey::Code::KeyV,
    global_hotkey::hotkey::Code::KeyW,
    global_hotkey::hotkey::Code::KeyX,
    global_hotkey::hotkey::Code::KeyY,
    global_hotkey::hotkey::Code::KeyZ,
];

/// 虚拟键码 → `keyboard_types::Code`（非 Windows）。覆盖设置页可捕获的全键集
/// （与 `settings::hotkey_vk_label` 的 match 域一致）；未知码 → `None`
/// （调用侧按注册失败处置，不做静默猜测）。
#[cfg(not(windows))]
fn vk_to_code(vk: u32) -> Option<global_hotkey::hotkey::Code> {
    use global_hotkey::hotkey::Code;
    Some(match vk {
        0x20 => Code::Space,
        0x21 => Code::PageUp,
        0x22 => Code::PageDown,
        0x2D => Code::Insert,
        0x30..=0x39 => match vk {
            0x30 => Code::Digit0,
            0x31 => Code::Digit1,
            0x32 => Code::Digit2,
            0x33 => Code::Digit3,
            0x34 => Code::Digit4,
            0x35 => Code::Digit5,
            0x36 => Code::Digit6,
            0x37 => Code::Digit7,
            0x38 => Code::Digit8,
            _ => Code::Digit9,
        },
        0x41..=0x5A => LETTER_KEY_CODES[(vk - 0x41) as usize],
        0x70..=0x7B => F_KEY_CODES[(vk - 0x70) as usize],
        0xBA => Code::Semicolon,
        0xBB => Code::Equal,
        0xBC => Code::Comma,
        0xBD => Code::Minus,
        0xBE => Code::Period,
        0xBF => Code::Slash,
        0xC0 => Code::Backquote,
        0xDB => Code::BracketLeft,
        0xDC => Code::Backslash,
        0xDD => Code::BracketRight,
        0xDE => Code::Quote,
        _ => return None,
    })
}

/// (mods 位掩码, vk) → crate [`HotKey`]；主键不可映射 → `None`。
#[cfg(not(windows))]
fn to_hotkey(mods: u32, vk: u32) -> Option<global_hotkey::hotkey::HotKey> {
    use global_hotkey::hotkey::{HotKey, Modifiers};
    let code = vk_to_code(vk)?;
    let m = mods_mask_to_modifiers(mods);
    Some(if m.is_empty() {
        HotKey::new(None, code)
    } else {
        HotKey::new(Some(m), code)
    })
}

/// 热键线程主体（非 Windows，O6 批次三）：manager 创建 → 初始注册 → 轮询
/// 循环（重注册命令 + 全局热键事件）。降级语义与 Windows 侧逐位一致。
#[cfg(not(windows))]
fn message_loop_unix(
    tx: Sender<HotkeyEvent>,
    ctx: eframe::egui::Context,
    cmd_rx: Receiver<HotkeyCommand>,
    mods: u32,
    vk: u32,
) {
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
    use std::time::Duration;

    // manager 创建：Linux 仅 X11（Wayland / 无显示 → Err）；macOS 实现可能
    // 主线程断言 panic——catch_unwind 双护栏，两路均降级为「无 manager」。
    let manager = match std::panic::catch_unwind(GlobalHotKeyManager::new) {
        Ok(Ok(m)) => Some(m),
        Ok(Err(e)) => {
            log::warn!("[dd-gui] 全局热键 manager 创建失败：{e} —— 降级为无热键运行");
            None
        }
        Err(_) => {
            log::warn!("[dd-gui] 全局热键 manager 创建 panic —— 降级为无热键运行");
            None
        }
    };

    // 当前生效组合（重注册失败回滚源；None = 当前无生效热键）
    let mut current: Option<(u32, u32)> = None;
    if let Some(m) = &manager {
        if let Some(hk) = to_hotkey(mods, vk) {
            match m.register(hk) {
                Ok(()) => {
                    current = Some((mods, vk));
                    log::info!(
                        "[dd-gui] 全局热键注册成功：{}",
                        dd_gui::settings::hotkey_combo_label(mods, vk)
                    );
                }
                Err(e) => log::debug!(
                    "[dd-gui] 全局热键注册失败（{e}），降级为无热键运行——可在设置页更换"
                ),
            }
        }
    }
    if current.is_none() {
        // 启动失败降级（不 panic）：与 Windows 侧启动失败事件逐位一致
        let _ = tx.send(HotkeyEvent::ReRegistered {
            seq: HOTKEY_SEQ_STARTUP,
            ok: false,
            rolled_back: false, // 无旧键可回滚
        });
    }

    loop {
        // 1) 重注册命令（非阻塞清空）：解旧键注册新键；失败自动回滚旧键并
        //    回发失败事件——与 Windows 侧 message_loop 的 seq 协议一致。
        while let Ok(cmd) = cmd_rx.try_recv() {
            let old = current;
            let mut result = (false, false); // (ok, rolled_back)
            match (&manager, to_hotkey(cmd.mods, cmd.vk)) {
                (Some(m), Some(hk)) => {
                    if let Some(cur) = old {
                        if let Some(old_hk) = to_hotkey(cur.0, cur.1) {
                            // 解注册失败仅忽略（键可能已失效）；回滚成败由
                            // 后续 register 判定
                            let _ = m.unregister(old_hk);
                        }
                    }
                    match m.register(hk) {
                        Ok(()) => {
                            current = Some((cmd.mods, cmd.vk));
                            // 诊断日志：与 Windows 侧同口径（seq 供 UI 对账）
                            log::info!(
                                "[dd-gui] 全局热键注册成功（seq={}）：{}",
                                cmd.seq,
                                dd_gui::settings::hotkey_combo_label(cmd.mods, cmd.vk)
                            );
                            result = (true, true); // 成功时 rolled_back 无意义，恒 true
                        }
                        Err(e) => {
                            log::warn!(
                                "[dd-gui] 新热键注册失败（seq={}，{}+{}，{e}），回滚旧键",
                                cmd.seq,
                                cmd.mods,
                                cmd.vk
                            );
                            let mut rolled_back = false;
                            if let Some(cur) = old {
                                if let Some(old_hk) = to_hotkey(cur.0, cur.1) {
                                    rolled_back = m.register(old_hk).is_ok();
                                    if !rolled_back {
                                        current = None; // 旧键回滚也失败：无任何生效热键
                                    }
                                } else {
                                    current = None; // 旧键不可再映射：视同回滚失败
                                }
                            }
                            result = (false, rolled_back);
                        }
                    }
                }
                // manager 缺席（Wayland/无显示降级中）或新键不可映射：请求不
                // 执行。与 (Some, Some) 分支不同，此处**未解注册旧键**——旧键
                // 仍生效，rolled_back 依事件契约如实回发（旧键缺席 = 本就无
                // 生效热键；不变式：current 为 Some 蕴含 manager 为 Some）。
                _ => {
                    log::warn!(
                        "[dd-gui] 重注册请求无法执行（seq={}，manager 缺席或键不可映射）",
                        cmd.seq
                    );
                    result = (false, old.is_some());
                }
            }
            let _ = tx.send(HotkeyEvent::ReRegistered {
                seq: cmd.seq,
                ok: result.0,
                rolled_back: result.1,
            });
            ctx.request_repaint();
        }

        // 2) 全局热键事件：crate 按按下/释放各发一次，仅取按下（Toggle 语义
        //    与 Windows WM_HOTKEY 一致）；窗口隐藏时 request_repaint 唤醒 egui。
        while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
            if ev.state() == HotKeyState::Pressed {
                let _ = tx.send(HotkeyEvent::Toggle);
                ctx.request_repaint();
            }
        }

        // 3) 节拍兜底：命令通道 25ms 超时等待——命令/热键事件延迟上限 25ms、
        //    唤醒频率 40/s 量级（Windows 侧为事件驱动，此处为跨平台 crate 的
        //    轮询型消费面的对价）。通道断开 = HotkeyThread 已 drop（进程退出）。
        match cmd_rx.recv_timeout(Duration::from_millis(25)) {
            Ok(_) => {} // 留待下轮步骤 1) 非阻塞清空统一处理
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// seq 协议命令打包 roundtrip（PostThreadMessageW 只有 2 个整型槽，
    /// mods<<32|vk 装包不可失真——含高位 vk 与全部 MOD 位组合）。
    #[cfg(windows)]
    #[test]
    fn hotkey_command_pack_roundtrip() {
        for cmd in [
            HotkeyCommand {
                seq: 0,
                mods: 0b1001,
                vk: 0x20,
            }, // 启动 Win+Alt+Space
            HotkeyCommand {
                seq: 1,
                mods: 0b0010,
                vk: 0x20,
            }, // 改绑 Ctrl+Space
            HotkeyCommand {
                seq: u32::MAX,
                mods: 0b1111,
                vk: 0x7B,
            }, // 边界：全 MOD + F12
            HotkeyCommand {
                seq: 7,
                mods: 0,
                vk: 0x41,
            }, // 无修饰 + A
        ] {
            let (w, l) = cmd.pack();
            assert_eq!(HotkeyCommand::unpack(w, l), cmd);
        }
    }

    /// vk → Code 映射：设置页可捕获全键集（与 `settings::hotkey_vk_label`
    /// 的 match 域同域）逐键 Some；域外代表键码（含 VK_LWIN 等 Windows 专属
    /// 不可捕获键）一律 None——映射域收窄在测试里钉死。
    #[cfg(not(windows))]
    #[test]
    fn vk_to_code_covers_label_key_set() {
        let mut labeled: Vec<u32> = vec![0x20, 0x21, 0x22, 0x2D];
        labeled.extend(0x70..=0x7B);
        labeled.extend(0x30..=0x39);
        labeled.extend(0x41..=0x5A);
        labeled.extend([
            0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0, 0xDB, 0xDC, 0xDD, 0xDE,
        ]);
        for vk in labeled {
            assert!(vk_to_code(vk).is_some(), "VK_{vk:02X} 应可映射");
        }
        for vk in [0x00, 0x03, 0x14, 0x5B, 0x6F, 0x7C, 0xE2, 0xFF] {
            assert!(vk_to_code(vk).is_none(), "VK_{vk:02X} 不应可映射");
        }
    }

    /// 修饰键位掩码 → Modifiers 逐位映射（含空掩码与全掩码边界）。
    #[cfg(not(windows))]
    #[test]
    fn mods_mask_to_modifiers_maps_all_bits() {
        use global_hotkey::hotkey::Modifiers;
        assert!(mods_mask_to_modifiers(0).is_empty());
        assert!(mods_mask_to_modifiers(0b0001).contains(Modifiers::ALT));
        assert!(mods_mask_to_modifiers(0b0010).contains(Modifiers::CONTROL));
        assert!(mods_mask_to_modifiers(0b0100).contains(Modifiers::SHIFT));
        assert!(mods_mask_to_modifiers(0b1000).contains(Modifiers::SUPER));
        let all = mods_mask_to_modifiers(0b1111);
        assert!(all.contains(Modifiers::ALT));
        assert!(all.contains(Modifiers::CONTROL));
        assert!(all.contains(Modifiers::SHIFT));
        assert!(all.contains(Modifiers::SUPER));
    }

    /// to_hotkey 组合：默认 Win+Alt+Space 可构造；空掩码走 None modifiers
    /// 分支；不可映射主键整体返回 None。
    #[cfg(not(windows))]
    #[test]
    fn to_hotkey_combines_mask_and_key() {
        use global_hotkey::hotkey::{Code, Modifiers};
        let hk = to_hotkey(0b1001, 0x20).expect("Win+Alt+Space 应可构造");
        assert_eq!(hk.mods, mods_mask_to_modifiers(0b1001));
        assert_eq!(hk.key, Code::Space);
        // 空掩码：HotKey::new(None, _) 分支
        let bare = to_hotkey(0, 0x41).expect("裸键应可构造");
        assert_eq!(bare.mods, Modifiers::empty());
        assert_eq!(bare.key, Code::KeyA);
        // 主键不可映射 → None
        assert!(to_hotkey(0b1001, 0x5B).is_none());
    }
}
