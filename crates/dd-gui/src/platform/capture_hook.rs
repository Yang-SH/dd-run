//! 热键捕获的低级键盘钩子（WH_KEYBOARD_LL，2026-09-29 B 方案）。
//!
//! **为什么需要**：捕获模式原实现走 egui 应用层事件拦截——egui/winit 在
//! Windows 不暴露 Win 键 modifiers（keys.rs / settings_view.rs 两处注释自认），
//! 按下 Win 键被系统解释为「打开开始菜单」→ 前台焦点被抢 → 触发失焦自动
//! 隐藏，面板直接消失且组合无法录入（真机 bug 2026-09-29）。
//!
//! **本模块的职责**：捕获模式期间安装系统级 LL 钩子——
//! - **吞掉全部键盘输入**（模态捕获语义，对齐 PowerToys Run 热键编辑器）；
//! - **Win 键恒吞**（keydown + keyup）→ 开始菜单不弹 → 焦点不丢 → 面板存活；
//! - 非修饰键按下时按 `GetAsyncKeyState` 现场读修饰状态，含 Ctrl/Alt/Win
//!   任一即回发 [`CaptureEvent::Combo`]（mods = MOD_* 位，含 WIN=0x8，与
//!   `settings.hotkey_mods` 编码一致）；
//! - Esc（无修饰）→ [`CaptureEvent::Cancel`]。
//!
//! **安全约束**（钩子回调拖慢会卡顿全系统）：
//! - 回调只做「读修饰位 + channel try 投递 + 返回 1」，无 IO / 无锁竞争
//!   （`CAPTURE_TX` 为无竞争单写 static，锁持纳亚秒）；
//! - 捕获生命周期由 `CaptureHookGuard` 的 Drop 严格兜底（hide / Esc / 完成 /
//!   30s 超时四条路径全部触发卸载，见 keys.rs）；
//! - 钩子线程独立 pump 消息（LL 钩子要求），Drop 经 `WM_QUIT` 结束后自行
//!   `UnhookWindowsHookExW`。

use std::sync::mpsc;
use std::sync::Mutex;

/// 捕获超时：钩子是全局模态（吞全键盘），超时自动取消防用户键盘「失灵」。
/// Esc 始终可取消，本值为双保险兜底。
pub const CAPTURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 捕获钩子 → 宿主 UI 的捕获事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureEvent {
    /// 组合键成立（含 ≥1 个 Ctrl/Alt/Win 修饰）。`mods` = MOD_* 位
    /// （ALT=1 / CONTROL=2 / SHIFT=4 / WIN=8），`vk` = 原生虚拟键码。
    Combo { mods: u32, vk: u32 },
    /// Esc（无修饰）→ 取消捕获。
    Cancel,
}

/// 钩子发送端（static 存放，供回调读取；同一时刻至多一个捕获钩子）。
static CAPTURE_TX: Mutex<Option<mpsc::Sender<CaptureEvent>>> = Mutex::new(None);

/// 修饰键组合 → settings/MOD_* 编码（纯函数，单测锚定）。
/// ALT=1 / CONTROL=2 / SHIFT=4 / WIN=8；与 `settings::HOTKEY_MODS_MASK`（0b1111）一致。
pub(crate) fn mods_from_keys(ctrl: bool, alt: bool, shift: bool, win: bool) -> u32 {
    (ctrl as u32) << 1 | alt as u32 | (shift as u32) << 2 | (win as u32) << 3
}

/// 捕获主键白名单（原生虚拟键码）：字母 / 数字 / F1–F12 / Space。
/// 与 egui 路径的 `capture_vk` 映射集完全一致（两路径同一键集约束）。
pub(crate) fn capture_vk_allowed(vk: u32) -> bool {
    (0x41..=0x5A).contains(&vk)        // A–Z
        || (0x30..=0x39).contains(&vk) // 0–9
        || (0x70..=0x7B).contains(&vk) // F1–F12
        || vk == 0x20 // Space
}

/// 键是否为修饰键本身（修饰键按下只更新修饰状态，不作为组合主键）。
fn is_modifier_vk(vk: u32) -> bool {
    (0x10..=0x12).contains(&vk)      // VK_SHIFT / VK_CONTROL / VK_MENU
        || (0xA0..=0xA5).contains(&vk) // L/R Shift·Control·Menu
        || vk == 0x5B                  // VK_LWIN
        || vk == 0x5C // VK_RWIN
}

// windows-sys 0.61.2 缺失 `UnhookWindowsHookExW`（SetWindowsHookExW 有、
// Unhook 无——上游遗漏）。**不能**手写 `#[link(name="user32")]` extern 补声明：
// Strawberry mingw 的 `libuser32.a` 导入库同样不含该符号（nm 计 0，2026-09-29
// release 链接实测 `undefined reference`）。故走运行时 `GetProcAddress`
// 动态解析（user32.dll 必有此导出，WinXP+）。

/// 运行时解析并调用 `UnhookWindowsHookExW`（见上注释）。
#[cfg(windows)]
unsafe fn unhook_windows_hook(hook: *mut core::ffi::c_void) {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

    // SAFETY：模块名 / 过程名常量串；句柄来自本进程已加载的 user32。
    let user32 = unsafe { GetModuleHandleA(b"user32\0".as_ptr().cast()) };
    if user32.is_null() {
        return;
    }
    let proc_addr = unsafe { GetProcAddress(user32, b"UnhookWindowsHookExW\0".as_ptr().cast()) };
    let Some(proc_addr) = proc_addr else {
        // 理论不可达（user32.dll 自 XP 起导出该函数）：捕获钩子残留至进程
        // 退出由系统回收，进程退出本身会卸载全部钩子，无长期泄漏。
        log::warn!("[dd-gui] GetProcAddress(UnhookWindowsHookExW) 失败——钩子延迟至进程退出回收");
        return;
    };
    // SAFETY：签名与 user32 导出一致（单指针入参返回 BOOL）。
    let f: unsafe extern "system" fn(*mut core::ffi::c_void) -> i32 =
        unsafe { std::mem::transmute(proc_addr) };
    unsafe { f(hook) };
}

/// 捕获钩子守卫：Drop = 卸载（清发送端 + 令钩子线程退出并自行 unhook）。
pub struct CaptureHookGuard {
    thread_id: u32,
    _handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for CaptureHookGuard {
    fn drop(&mut self) {
        // 先清发送端再令线程退出：竞态窗口内的按键会穿透而非误吞（安全向）。
        if let Ok(mut tx) = CAPTURE_TX.lock() {
            *tx = None;
        }
        #[cfg(windows)]
        if self.thread_id != 0 {
            // SAFETY：线程 id 由安装线程回发，WM_QUIT 结束其消息循环。
            use windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
            use windows_sys::Win32::UI::WindowsAndMessaging::WM_QUIT;
            unsafe {
                PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
            }
        }
        // 不 join：卸载路径在 UI 线程（Esc / hide / 超时），钩子线程 unhook
        // 是纯系统调用级收尾，残留线程毫秒级消亡，无需阻塞 UI 等待。
    }
}

/// `GetAsyncKeyState` 高位 = 物理按下（钩子回调内调用，µs 级）。
#[cfg(windows)]
fn async_down(vk: u16) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    // SAFETY：无副作用的全局键态查询。
    unsafe { GetAsyncKeyState(vk as i32) & 0x8000u16 as i16 != 0 }
}

/// LL 钩子回调（系统在安装线程 pump 消息期间同步调用——必须快）。
#[cfg(windows)]
unsafe extern "system" fn hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_SYSKEYDOWN,
    };

    // 链约定：code < 0 必须透传给下一钩子。
    if code < 0 {
        return CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
    }
    let Ok(tx) = CAPTURE_TX.lock() else {
        return 1; // Mutex 中毒（理论上不可能）：宁可吞键不可穿透
    };
    let Some(tx) = tx.as_ref() else {
        return CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
    };

    let down = wparam == WM_KEYDOWN as usize || wparam == WM_SYSKEYDOWN as usize;
    // SAFETY：lparam 指向系统提供的 KBDLLHOOKSTRUCT（回调期有效）。
    let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
    let vk = kb.vkCode;

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

    // Esc（无修饰）= 取消。
    if vk == VK_ESCAPE as u32 && !(ctrl || alt || shift || win) {
        let _ = tx.send(CaptureEvent::Cancel);
        return 1;
    }
    // 修饰键自身按下：只更新状态（上方 GetAsyncKeyState 已覆盖），吞掉等待主键。
    if is_modifier_vk(vk) {
        return 1;
    }
    // 组合判定：需含 Ctrl/Alt/Win 至少一个（纯 Shift 不成组合，对齐 egui 路径）。
    let mods = mods_from_keys(ctrl, alt, shift, win);
    if mods & 0b1011 == 0 {
        return 1;
    }
    // 主键白名单与 egui 路径一致（字母/数字/F1–F12/Space）。
    if !capture_vk_allowed(vk) {
        return 1;
    }
    let _ = tx.send(CaptureEvent::Combo { mods, vk });
    1
}

/// 安装捕获钩子：返回守卫（Drop 卸载）；事件接收端由调用方持有。
/// `Err` = 线程创建 / 钩子安装失败 / **1s 超时**（安全软件可能延迟钩子安装——
/// 绝不允许阻塞 UI 线程），调用方回落 egui 应用层捕获。
#[cfg(windows)]
pub fn start(tx: mpsc::Sender<CaptureEvent>) -> Result<CaptureHookGuard, String> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMessageW, PostThreadMessageW, SetWindowsHookExW, MSG, WH_KEYBOARD_LL, WM_QUIT,
    };

    let (id_tx, id_rx) = mpsc::channel::<u32>();
    let handle = std::thread::Builder::new()
        .name("dd-capture-hook".into())
        .spawn(move || {
            // ① **先报线程 id 再装钩子**（立即）：调用方由此始终能对慢安装
            //    的线程发 WM_QUIT 兜底（v2 协议：0=安装失败，1=安装成功）。
            let _ = id_tx.send(unsafe { GetCurrentThreadId() });
            // 先注册发送端再装钩子（回调随时可能被触发）。
            if let Ok(mut g) = CAPTURE_TX.lock() {
                *g = Some(tx);
            }
            // SAFETY：hmod = 主模块句柄（LL 钩子仅要求可定位回调所在模块）。
            let mut hook = unsafe {
                SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(hook_proc),
                    GetModuleHandleW(std::ptr::null()),
                    0,
                )
            };
            // 安装失败重试一次（2026-09-29 真机：Alt+Space 系统菜单弹开证明
            // 钩子未生效——安全软件拦截 LL 钩子安装常为瞬时，第二次放行）。
            if hook.is_null() {
                std::thread::sleep(std::time::Duration::from_millis(200));
                // SAFETY：同上。
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
                if let Ok(mut g) = CAPTURE_TX.lock() {
                    *g = None;
                }
                let _ = id_tx.send(0);
                return;
            }
            let _ = id_tx.send(1);
            // LL 钩子要求安装线程 pump 消息；WM_QUIT（Drop）→ GetMessageW = 0。
            loop {
                let mut msg: MSG = unsafe { std::mem::zeroed() };
                let r = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
                if r == 0 || r == -1 {
                    break;
                }
            }
            // 运行时动态解析（导入库缺符号，见 unhook_windows_hook 注释）。
            unsafe { unhook_windows_hook(hook) };
            if let Ok(mut g) = CAPTURE_TX.lock() {
                *g = None;
            }
        })
        .map_err(|e| format!("捕获钩子线程创建失败：{e}"))?;

    const INSTALL_WAIT: std::time::Duration = std::time::Duration::from_secs(1);
    let quit = |thread_id: u32| {
        if thread_id != 0 {
            unsafe {
                PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
            }
        }
        if let Ok(mut g) = CAPTURE_TX.lock() {
            *g = None;
        }
    };
    // 线程 id（①，应微秒级到达；超时 = 线程调度异常，兜底退出）。
    let thread_id = match id_rx.recv_timeout(INSTALL_WAIT) {
        Ok(id) => id,
        Err(_) => {
            return Err("捕获钩子线程未在 1s 内报出线程 id——兜底退出".to_string());
        }
    };
    // 安装结果（②）；超时同样兜底（防安全软件延迟 SetWindowsHookExW 冻结 UI）。
    let status = match id_rx.recv_timeout(INSTALL_WAIT) {
        Ok(s) => s,
        Err(_) => {
            quit(thread_id);
            return Err("捕获钩子安装超时（1s，可能被安全软件延迟）——回落 egui 捕获".to_string());
        }
    };
    if status == 0 {
        quit(thread_id);
        return Err("SetWindowsHookExW(WH_KEYBOARD_LL) 失败".to_string());
    }
    Ok(CaptureHookGuard {
        thread_id,
        _handle: Some(handle),
    })
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
    /// 与 `settings::HOTKEY_MODS_MASK` 一致（默认 Win+Alt = 0b1001）。
    #[test]
    fn mods_encoding_matches_mod_star_layout() {
        assert_eq!(mods_from_keys(false, true, false, true), 0b1001, "Win+Alt");
        assert_eq!(mods_from_keys(true, false, false, false), 0b0010, "Ctrl");
        assert_eq!(mods_from_keys(false, false, true, false), 0b0100, "Shift");
        assert_eq!(mods_from_keys(false, false, false, false), 0b0000);
        // 纯 Shift 不含 Ctrl/Alt/Win → 组合判定掩码 0b1011 下为 0（不成立）。
        assert_eq!(mods_from_keys(false, false, true, false) & 0b1011, 0);
    }

    /// 主键白名单：与 egui 路径 `capture_vk` 的映射集一致。
    #[test]
    fn capture_vk_allowed_set_matches_egui_path() {
        for vk in [0x20, 0x30, 0x39, 0x41, 0x5A, 0x70, 0x7B] {
            assert!(capture_vk_allowed(vk), "白名单内被误拒：{vk:#x}");
        }
        for vk in [0x11, 0x12, 0x5B, 0x5C, 0xA0, 0x25, 0x2D, 0x14, 0x6A] {
            assert!(!capture_vk_allowed(vk), "白名单外被误放：{vk:#x}");
        }
    }
}
