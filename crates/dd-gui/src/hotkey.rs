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

    /// 非 Windows 平台：不注册热键，事件通道永远为空（跨平台编译占位）。
    #[cfg(not(windows))]
    pub fn spawn(_ctx: eframe::egui::Context, _mods: u32, _vk: u32) -> Self {
        let (_tx, rx) = mpsc::channel::<HotkeyEvent>();
        Self {
            events: rx,
            _handle: dummy_handle("dd-hotkey-dummy"),
            thread_id: 0,
        }
    }

    /// 非 Windows 平台：重注册为空操作。
    #[cfg(not(windows))]
    pub fn re_register(&self, _cmd: HotkeyCommand) {}

    /// 测试用桩：不注册热键、无事件（`make_app` 注入用）。
    pub fn dummy() -> Self {
        let (_tx, rx) = mpsc::channel::<HotkeyEvent>();
        Self {
            events: rx,
            _handle: dummy_handle("dd-hotkey-dummy"),
            thread_id: 0,
        }
    }

    /// R-15 测试注入：外部持有发送端的事件通道（`make_app_with` 用）。
    #[cfg(test)]
    pub(crate) fn for_events(rx: mpsc::Receiver<HotkeyEvent>) -> Self {
        Self {
            events: rx,
            _handle: dummy_handle("dd-hotkey-test"),
            thread_id: 0,
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
}
