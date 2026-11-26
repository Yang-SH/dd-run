//! 热键捕获的低级键盘钩子（WH_KEYBOARD_LL，2026-09-29 B 方案 v3 异步化）。
//!
//! **为什么需要**：捕获模式原实现走 egui 应用层事件拦截——egui/winit 在
//! Windows 不暴露 Win 键 modifiers（keys.rs / settings_view.rs 两处注释自认），
//! 按下 Win 键被系统解释为「打开开始菜单」→ 前台焦点被抢 → 触发失焦自动
//! 隐藏，面板直接消失且组合无法录入（真机 bug 2026-09-29）。
//!
//! **v3 关键演进（真机「任何按键都录不到」根因修复）**：v1/v2 在 UI 线程
//! `recv_timeout(1s)` 同步等安装结果——本机内存高压下线程调度 + 安全软件
//! 介入可使 `SetWindowsHookExW` 超过 1s → UI 判超时 → 自卸钩子 → 静默回落。
//! **零依赖探针实证本机 LL 钩子模式完全可用**（同款 static channel + 独立
//! 线程 pump，8/8 事件全通）→ 分叉点即「UI 侧同步等待」。v3 改为：
//! - `start()` **立即返回**（零阻塞）：安装全部在后台线程；
//! - **自回声验证**：安装成功后线程自动注入一次 `F15`（F15 不在捕获
//!   白名单、无任何系统副作用），验证钩子真的收到本注入——收到才报
//!   [`CaptureEvent::Ready`]，否则 [`CaptureEvent::Failed`]；杜绝
//!   「SetWindowsHookExW 成功但钩子永不触发」（安全软件剥离）的假成功；
//! - **双路径接力**：安装期间/失败后 egui 基础路径持续可用（Ctrl/Alt 组合
//!   永远可录，M6 行为）；`Ready` 后钩子路径接管（Win 也可录）；
//! - **取消安全**：`Guard::drop` 清发送端 + `WM_QUIT`；泵循环每轮检测
//!   发送端已被清（被取消）即自行 unhook 退出——双保险杜绝钩子泄漏
//!   （钩子泄漏 = 模态吞键 = 全系统键盘失灵，绝不可发生）。
//!
//! **安全约束**（钩子回调拖慢会卡顿全系统）：
//! - 回调只做「读修饰位 + channel 投递 + 返回 1」，无 IO / 无锁竞争；
//! - 捕获生命周期由 `CaptureHookGuard` 的 Drop 严格兜底（hide / Esc / 完成 /
//!   30s 超时 / Failed 五条路径全部触发卸载，见 keys.rs）。

#[cfg(windows)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;

/// 捕获超时：钩子是全局模态（吞全键盘），超时自动取消防用户键盘「失灵」。
/// 裸 Esc 始终可取消，本值为双保险兜底。
pub const CAPTURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 自回声验证主键：VK_F15（0x7E）。不在捕获白名单（0x70..=0x7B）→ 即使
/// `GetAsyncKeyState` 读到 Alt 修饰也**不会**产生 `Combo` 事件，对 UI 零副作用；
/// 无任何系统绑定，注入完全无害。
#[cfg(windows)]
const SELFTEST_VK: u32 = 0x7E;

/// 钩子发送端（static 存放，供回调读取；同一时刻至多一个捕获钩子）。
static CAPTURE_TX: Mutex<Option<(u64, mpsc::Sender<CaptureEvent>)>> = Mutex::new(None);

/// 当前捕获会话；0 = 已取消。守卫与安装线程据此消除快速重入竞态。
static ACTIVE_SESSION: AtomicU64 = AtomicU64::new(0);
#[cfg(windows)]
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);
#[cfg(windows)]
static SESSION_LOCK: Mutex<()> = Mutex::new(());

/// 自回声命中旗标：hook_proc 看到 `SELFTEST_VK` keydown 即置位。
#[cfg(windows)]
static SELFTEST_HIT: AtomicBool = AtomicBool::new(false);

/// 钩子线程 id 与所属会话；守卫只向自己的会话投递 WM_QUIT。
#[cfg(windows)]
static THREAD_ID: Mutex<Option<(u64, u32)>> = Mutex::new(None);

thread_local! {
    static OWN_HOOK: std::cell::Cell<*mut core::ffi::c_void> =
        const { std::cell::Cell::new(std::ptr::null_mut()) };
    static OWN_SESSION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// 捕获钩子 → 宿主 UI 的捕获事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureEvent {
    /// 钩子**安装并自回声验证通过**——此后 `Combo` / `Cancel` 才有意义。
    Ready,
    /// 安装 / 自回声验证失败（已含重试）。UI 应回落 egui 基础捕获路径。
    Failed(&'static str),
    /// 组合键成立（含 ≥1 个 Ctrl/Alt/Win 修饰）。`mods` = MOD_* 位
    /// （ALT=1 / CONTROL=2 / SHIFT=4 / WIN=8），`vk` = 原生虚拟键码。
    Combo { mods: u32, vk: u32 },
    /// Esc（无修饰）→ 取消捕获。
    Cancel,
}

#[cfg(any(windows, test))]
/// 修饰键组合 → settings/MOD_* 编码（纯函数，单测锚定）。
/// ALT=1 / CONTROL=2 / SHIFT=4 / WIN=8；与 `settings::HOTKEY_MODS_MASK`（0b1111）一致。
pub(crate) fn mods_from_keys(ctrl: bool, alt: bool, shift: bool, win: bool) -> u32 {
    (ctrl as u32) << 1 | alt as u32 | (shift as u32) << 2 | (win as u32) << 3
}

/// 捕获主键白名单（原生虚拟键码）：字母 / 数字 / F1–F12 / Space / Esc。
/// 与 egui 路径的 `capture_vk` 映射集完全一致（两路径同一键集约束）。
pub(crate) fn capture_vk_allowed(vk: u32) -> bool {
    (0x41..=0x5A).contains(&vk)        // A–Z
        || (0x30..=0x39).contains(&vk) // 0–9
        || (0x70..=0x7B).contains(&vk) // F1–F12
        || vk == 0x20 // Space
        || vk == 0x1B // Esc：裸 Esc 取消，带修饰时作为组合主键
}

/// 键是否为修饰键本身（修饰键按下只更新修饰状态，不作为组合主键）。
fn is_modifier_vk(vk: u32) -> bool {
    (0x10..=0x12).contains(&vk)      // VK_SHIFT / VK_CONTROL / VK_MENU
        || (0xA0..=0xA5).contains(&vk) // L/R Shift·Control·Menu
        || vk == 0x5B                  // VK_LWIN
        || vk == 0x5C // VK_RWIN
}

/// 一次按键按下在捕获模态中的分类；钩子与 egui 回落共用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureKeyAction {
    Cancel,
    Combo { mods: u32, vk: u32 },
    Ignore,
}

pub(crate) fn classify_capture_key(vk: u32, mods: u32) -> CaptureKeyAction {
    if vk == 0x1B && mods == 0 {
        return CaptureKeyAction::Cancel;
    }
    if is_modifier_vk(vk) {
        return CaptureKeyAction::Ignore;
    }
    if mods & 0b1011 == 0 || !capture_vk_allowed(vk) {
        return CaptureKeyAction::Ignore;
    }
    CaptureKeyAction::Combo { mods, vk }
}

// windows-sys 0.61.2 缺失 `UnhookWindowsHookExW`（SetWindowsHookExW 有、
// Unhook 无——上游遗漏）。**不能**手写 `#[link(name="user32")]` extern 补声明：
// Strawberry mingw 的 `libuser32.a` 导入库同样不含该符号（nm 计 0，2026-09-29
// release 链接实测 `undefined reference`）。故走运行时 `GetProcAddress`
// 动态解析（user32.dll 必有此导出，WinXP+）。

/// 运行时解析并调用 `UnhookWindowsHookExW`（见上注释）。
#[cfg(windows)]
unsafe fn unhook_windows_hook(hook: *mut core::ffi::c_void) -> bool {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

    // SAFETY：模块名 / 过程名常量串；句柄来自本进程已加载的 user32。
    let user32 = unsafe { GetModuleHandleA(c"user32".as_ptr().cast()) };
    if user32.is_null() {
        return false;
    }
    let proc_addr = unsafe { GetProcAddress(user32, c"UnhookWindowsHookExW".as_ptr().cast()) };
    let Some(proc_addr) = proc_addr else {
        log::warn!("[dd-gui] GetProcAddress(UnhookWindowsHookExW) 失败——钩子延迟至进程退出回收");
        return false;
    };
    // SAFETY：签名与 user32 导出一致（单指针入参返回 BOOL）。
    let f: unsafe extern "system" fn(*mut core::ffi::c_void) -> i32 =
        unsafe { std::mem::transmute(proc_addr) };
    let removed = unsafe { f(hook) } != 0;
    if !removed {
        log::warn!("[dd-gui] UnhookWindowsHookExW 返回失败，等待线程退出回收");
    }
    removed
}

#[cfg(windows)]
fn should_unhook_own_hook(
    current: *mut core::ffi::c_void,
    expected: *mut core::ffi::c_void,
) -> bool {
    !current.is_null() && current == expected
}

#[cfg(windows)]
fn unhook_own_hook(expected: *mut core::ffi::c_void) -> bool {
    let current = OWN_HOOK.with(|cell| cell.get());
    if !should_unhook_own_hook(current, expected) {
        return false;
    }
    let removed = unsafe { unhook_windows_hook(expected) };
    if removed {
        OWN_HOOK.with(|cell| cell.set(std::ptr::null_mut()));
    }
    removed
}

/// `GetAsyncKeyState` 高位 = 物理按下（钩子回调内调用，µs 级）。
#[cfg(windows)]
fn async_down(vk: u16) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    // SAFETY：无副作用的全局键态查询。
    unsafe { GetAsyncKeyState(vk as i32) & 0x8000u16 as i16 != 0 }
}

/// LL 钩子回调（系统在安装线程泵消息期间同步调用——必须快）。
#[cfg(windows)]
unsafe extern "system" fn hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_SYSKEYDOWN,
    };

    // 链约定：code < 0 必须透传给下一钩子。
    if code < 0 {
        return CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
    }
    // F10 陈旧会话自检：本线程会话不再是 ACTIVE_SESSION 时立即自卸。
    // 不是最新会话（start() 被快速重启、旧线程收尾慢于新会话安装），立即自卸
    // 钩子并放行事件——旧钩子既不再吞键，也不再向新会话通道投递，根除
    // 「短暂双钩子并存 + 旧事件灌入新会话」竞态。自卸必须走
    // `unhook_windows_hook` 辅助（windows-sys 缺 UnhookWindowsHookExW 符号，
    // 见上方 91-95 行注记）；句柄读写仅在本线程（TLS），无锁竞争。
    // 卸载后本线程随泵循环退出（发送端被 Drop 清空 / WM_QUIT），TLS 随线程消亡。
    let session = OWN_SESSION.with(|cell| cell.get());
    if session == 0 || ACTIVE_SESSION.load(Ordering::Acquire) != session {
        let hook = OWN_HOOK.with(|cell| cell.get());
        if !hook.is_null() {
            unhook_own_hook(hook);
        }
        return CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
    }
    let down = wparam == WM_KEYDOWN as usize || wparam == WM_SYSKEYDOWN as usize;
    // SAFETY：lparam 指向系统提供的 KBDLLHOOKSTRUCT（回调期有效）。
    let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
    let vk = kb.vkCode;

    // 自回声主键（仅 keydown 置位）：无论修饰如何都不产生 Combo（F15 不在
    // 白名单），对 UI 零副作用——只作为「钩子真的在收事件」的实证。
    if vk == SELFTEST_VK && down {
        SELFTEST_HIT.store(true, Ordering::Relaxed);
    }

    // Win 键恒吞（keydown + keyup）：根因修复——开始菜单不弹、焦点不丢。
    if vk == VK_LWIN as u32 || vk == VK_RWIN as u32 {
        return 1;
    }
    if !down {
        return 1; // 模态捕获：吞掉全部 keyup
    }
    let ctrl = async_down(VK_CONTROL);
    let alt = async_down(VK_MENU);
    let shift = async_down(VK_SHIFT);
    let win = async_down(VK_LWIN) || async_down(VK_RWIN);

    let mods = mods_from_keys(ctrl, alt, shift, win);
    match classify_capture_key(vk, mods) {
        CaptureKeyAction::Cancel => send_capture_event(session, CaptureEvent::Cancel),
        CaptureKeyAction::Combo { mods, vk } => {
            send_capture_event(session, CaptureEvent::Combo { mods, vk })
        }
        CaptureKeyAction::Ignore => {}
    }
    1
}

/// 捕获钩子守卫：Drop = 取消会话、清发送端并投递 WM_QUIT。
pub struct CaptureHookGuard {
    session: u64,
}

impl Drop for CaptureHookGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        let _session_lock = SESSION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if ACTIVE_SESSION
            .compare_exchange(self.session, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        if let Ok(mut tx) = CAPTURE_TX.lock() {
            if tx.as_ref().map(|(session, _)| *session) == Some(self.session) {
                *tx = None;
            }
        }
        #[cfg(windows)]
        let thread_id = THREAD_ID
            .lock()
            .ok()
            .and_then(|g| *g)
            .filter(|(session, _)| *session == self.session)
            .map(|(_, tid)| tid)
            .unwrap_or(0);
        #[cfg(windows)]
        if thread_id != 0 {
            use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
            unsafe {
                PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
            }
        }
    }
}

#[cfg(windows)]
fn register_capture_sender(session: u64, tid: u32, tx: mpsc::Sender<CaptureEvent>) -> bool {
    let _session_lock = SESSION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut slot = CAPTURE_TX.lock().unwrap_or_else(|e| e.into_inner());
    if ACTIVE_SESSION.load(Ordering::Acquire) != session {
        return false;
    }
    *THREAD_ID.lock().unwrap_or_else(|e| e.into_inner()) = Some((session, tid));
    *slot = Some((session, tx));
    true
}

#[cfg(windows)]
fn capture_session_active(session: u64) -> bool {
    ACTIVE_SESSION.load(Ordering::Acquire) == session
        && CAPTURE_TX
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|(id, _)| *id))
            == Some(session)
}

#[cfg(windows)]
fn send_capture_event(session: u64, event: CaptureEvent) {
    if ACTIVE_SESSION.load(Ordering::Acquire) != session {
        return;
    }
    if let Ok(slot) = CAPTURE_TX.lock() {
        if let Some((id, tx)) = slot.as_ref() {
            if *id == session {
                let _ = tx.send(event);
            }
        }
    }
}

#[cfg(windows)]
fn clear_capture_sender(session: u64) {
    if let Ok(mut slot) = CAPTURE_TX.lock() {
        if slot.as_ref().map(|(id, _)| *id) == Some(session) {
            *slot = None;
        }
    }
}

#[cfg(windows)]
fn finish_capture_session(session: u64) {
    clear_capture_sender(session);
    if let Ok(mut thread_id) = THREAD_ID.lock() {
        if thread_id.map(|(id, _)| id) == Some(session) {
            *thread_id = None;
        }
    }
    let _ = ACTIVE_SESSION.compare_exchange(session, 0, Ordering::AcqRel, Ordering::Acquire);
}

/// 安装捕获钩子（**异步，立即返回**）：
/// - 线程创建失败（spawn Err）→ Err（极罕见）；其余结果经 `tx` 异步回发：
///   `Ready`（安装 + 自回声通过）/ `Failed(摘要)`；
/// - 宿主侧在 `Ready` 前保持 egui 基础捕获（Ctrl/Alt 组合永远可录）；
/// - 宿主 drop 返回的 [`CaptureHookGuard`] 即取消安装/卸载钩子（任何时候）。
#[cfg(windows)]
pub fn start(tx: mpsc::Sender<CaptureEvent>) -> Result<CaptureHookGuard, String> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::keybd_event;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        PeekMessageW, SetWindowsHookExW, MSG, PM_REMOVE, WH_KEYBOARD_LL, WM_QUIT,
    };

    unsafe fn pump_messages() -> bool {
        loop {
            let mut msg: MSG = unsafe { std::mem::zeroed() };
            let has = unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) };
            if has == 0 {
                return false;
            }
            if msg.message == WM_QUIT {
                return true;
            }
        }
    }

    let session = {
        let _session_lock = SESSION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let session = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
        ACTIVE_SESSION.store(session, Ordering::Release);
        session
    };
    let guard = CaptureHookGuard { session };
    std::thread::Builder::new()
        .name("dd-capture-hook".into())
        .spawn(move || {
            let tid = unsafe { GetCurrentThreadId() };
            if !register_capture_sender(session, tid, tx.clone())
                || !capture_session_active(session)
            {
                return;
            }

            let mut hook = unsafe {
                SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(hook_proc),
                    GetModuleHandleW(std::ptr::null()),
                    0,
                )
            };
            if hook.is_null() {
                std::thread::sleep(std::time::Duration::from_millis(200));
                hook = unsafe {
                    SetWindowsHookExW(
                        WH_KEYBOARD_LL,
                        Some(hook_proc),
                        GetModuleHandleW(std::ptr::null()),
                        0,
                    )
                };
            }
            if hook.is_null() {
                let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
                send_capture_event(session, CaptureEvent::Failed("SetWindowsHookExW 两连失败"));
                finish_capture_session(session);
                log::error!("[dd-gui] 捕获钩子安装失败（GetLastError={err}）——回落 egui 捕获");
                return;
            }
            if !capture_session_active(session) {
                unsafe { unhook_windows_hook(hook) };
                finish_capture_session(session);
                return;
            }
            OWN_HOOK.with(|cell| cell.set(hook));
            OWN_SESSION.with(|cell| cell.set(session));

            SELFTEST_HIT.store(false, Ordering::Relaxed);
            unsafe {
                keybd_event(SELFTEST_VK as u8, 0, 0, 0);
                keybd_event(SELFTEST_VK as u8, 0, 2, 0);
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1200);
            let mut quit = false;
            let mut hit = false;
            while std::time::Instant::now() < deadline {
                quit = unsafe { pump_messages() };
                if quit || !capture_session_active(session) {
                    break;
                }
                if SELFTEST_HIT.load(Ordering::Relaxed) {
                    hit = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if quit || !capture_session_active(session) {
                unhook_own_hook(hook);
                finish_capture_session(session);
                return;
            }
            if !hit {
                send_capture_event(
                    session,
                    CaptureEvent::Failed("钩子自回声验证失败（事件未达）"),
                );
                unhook_own_hook(hook);
                finish_capture_session(session);
                log::error!("[dd-gui] 捕获钩子自回声验证失败——回落 egui 捕获");
                return;
            }

            send_capture_event(session, CaptureEvent::Ready);
            log::info!("[dd-gui] 热键捕获：LL 钩子已安装并自验证通过（系统级，Win 键可录）");
            loop {
                if unsafe { pump_messages() } || !capture_session_active(session) {
                    unhook_own_hook(hook);
                    finish_capture_session(session);
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        })
        .map(|_| guard)
        .map_err(|e| format!("捕获钩子线程创建失败：{e}"))
}

/// 非 Windows 平台：无 LL 钩子，恒 Err（调用方回落 egui 应用层捕获）。
#[cfg(not(windows))]
pub fn start(_tx: mpsc::Sender<CaptureEvent>) -> Result<CaptureHookGuard, String> {
    Err("仅 Windows 支持低级键盘钩子捕获".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// mods 编码锚定：位布局 = ALT=1 / CONTROL=2 / SHIFT=4 / WIN=8，
    /// 与 `settings::HOTKEY_MODS_MASK` 一致（Win+Alt 编码锚 = 0b1001，非当前默认值）。
    #[cfg(windows)]
    #[test]
    fn p1_16_own_hook_guard_prevents_stale_double_unhook() {
        let hook = 0x1234 as *mut core::ffi::c_void;
        assert!(should_unhook_own_hook(hook, hook));
        assert!(!should_unhook_own_hook(std::ptr::null_mut(), hook));
        assert!(!should_unhook_own_hook(
            0x5678 as *mut core::ffi::c_void,
            hook
        ));
    }

    #[test]
    fn mods_encoding_matches_mod_star_layout() {
        assert_eq!(mods_from_keys(false, true, false, true), 0b1001, "Win+Alt");
        assert_eq!(mods_from_keys(true, false, false, false), 0b0010, "Ctrl");
        assert_eq!(mods_from_keys(false, false, true, false), 0b0100, "Shift");
        assert_eq!(mods_from_keys(false, false, false, false), 0b0000);
        // 纯 Shift 不含 Ctrl/Alt/Win → 组合判定掩码 0b1011 下为 0（不成立）。
        assert_eq!(mods_from_keys(false, false, true, false) & 0b1011, 0);
    }

    #[test]
    fn p1_16_capture_classification_modified_escape() {
        use crate::platform::capture_hook::CaptureKeyAction;

        assert_eq!(classify_capture_key(0x1B, 0), CaptureKeyAction::Cancel);
        assert_eq!(
            classify_capture_key(0x1B, 0b0010),
            CaptureKeyAction::Combo {
                mods: 0b0010,
                vk: 0x1B
            }
        );
        assert_eq!(
            classify_capture_key(0x1B, 0b0110),
            CaptureKeyAction::Combo {
                mods: 0b0110,
                vk: 0x1B
            }
        );
        assert_eq!(classify_capture_key(0x1B, 0b0100), CaptureKeyAction::Ignore);
        assert_eq!(classify_capture_key(0x41, 0), CaptureKeyAction::Ignore);
        assert_eq!(
            classify_capture_key(0x41, 0b0010),
            CaptureKeyAction::Combo {
                mods: 0b0010,
                vk: 0x41
            }
        );
        assert_eq!(classify_capture_key(0x7E, 0b0010), CaptureKeyAction::Ignore);
        assert_eq!(classify_capture_key(0x11, 0b0010), CaptureKeyAction::Ignore);
    }

    #[cfg(windows)]
    #[test]
    fn p1_16_cancel_before_register_and_stale_guard_are_safe() {
        let session = 9_000_001;
        ACTIVE_SESSION.store(0, Ordering::Relaxed);
        CAPTURE_TX.lock().unwrap().take();
        *THREAD_ID.lock().unwrap() = None;

        let (tx, rx) = mpsc::channel();
        assert!(!register_capture_sender(session, 0, tx.clone()));
        assert!(rx.try_recv().is_err(), "取消后不得注册发送端");

        ACTIVE_SESSION.store(session, Ordering::Relaxed);
        assert!(register_capture_sender(session, 0, tx));
        assert_eq!(*THREAD_ID.lock().unwrap(), Some((session, 0)));
        drop(CaptureHookGuard { session });
        assert_eq!(ACTIVE_SESSION.load(Ordering::Relaxed), 0);
        assert!(CAPTURE_TX.lock().unwrap().is_none());

        let active = session + 1;
        let (new_tx, _new_rx) = mpsc::channel();
        ACTIVE_SESSION.store(active, Ordering::Relaxed);
        *CAPTURE_TX.lock().unwrap() = Some((active, new_tx));
        *THREAD_ID.lock().unwrap() = Some((active, 0));
        drop(CaptureHookGuard { session });
        assert_eq!(ACTIVE_SESSION.load(Ordering::Relaxed), active);
        assert_eq!(CAPTURE_TX.lock().unwrap().as_ref().unwrap().0, active);

        ACTIVE_SESSION.store(0, Ordering::Relaxed);
        CAPTURE_TX.lock().unwrap().take();
        *THREAD_ID.lock().unwrap() = None;
    }

    /// 主键白名单：与 egui 路径 `capture_vk` 的映射集一致。
    #[test]
    fn capture_vk_allowed_set_matches_egui_path() {
        for vk in [0x1B, 0x20, 0x30, 0x39, 0x41, 0x5A, 0x70, 0x7B] {
            assert!(capture_vk_allowed(vk), "白名单内被误拒：{vk:#x}");
        }
        // F15（自回声键）必须在白名单**外**——保证自回声注入对 UI 零副作用。
        for vk in [0x11, 0x12, 0x5B, 0x5C, 0xA0, 0x25, 0x2D, 0x14, 0x6A, 0x7E] {
            assert!(!capture_vk_allowed(vk), "白名单外被误放：{vk:#x}");
        }
    }
}
