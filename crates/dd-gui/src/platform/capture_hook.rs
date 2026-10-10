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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;

/// 捕获超时：钩子是全局模态（吞全键盘），超时自动取消防用户键盘「失灵」。
/// Esc 始终可取消，本值为双保险兜底。
pub const CAPTURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 自回声验证主键：VK_F15（0x7E）。不在捕获白名单（0x70..=0x7B）→ 即使
/// `GetAsyncKeyState` 读到 Alt 修饰也**不会**产生 `Combo` 事件，对 UI 零副作用；
/// 无任何系统绑定，注入完全无害。
const SELFTEST_VK: u32 = 0x7E;

/// 钩子发送端（static 存放，供回调读取；同一时刻至多一个捕获钩子）。
/// **被清空（None）= 已取消/已卸载**——泵循环每轮检测，据此自行退出。
static CAPTURE_TX: Mutex<Option<mpsc::Sender<CaptureEvent>>> = Mutex::new(None);

/// 自回声命中旗标：hook_proc 看到 `SELFTEST_VK` keydown 即置位。
static SELFTEST_HIT: AtomicBool = AtomicBool::new(false);

/// 钩子线程 id（安装线程回发后由 [`CaptureHookGuard`].drop 用于投递 WM_QUIT）。
/// 同一时刻至多一个捕获钩子，单槽 Mutex 足够。
static THREAD_ID: Mutex<u32> = Mutex::new(0);

// F10（2026-10-10）：钩子线程**自己的** HHOOK 句柄（裸指针以 `Cell` 存放，
// null = 未安装）。LL 钩子回调恒在安装线程执行 → TLS 可靠——`hook_proc`
// 入口的陈旧会话自检据此自卸（见 hook_proc 注释；句柄只在本线程内读写，
// 无跨线程共享）。普通注释：thread_local! 宏不消费 doc comment（-D warnings）。
thread_local! {
    static OWN_HOOK: std::cell::Cell<*mut core::ffi::c_void> =
        const { std::cell::Cell::new(std::ptr::null_mut()) };
}

/// 捕获钩子 → 宿主 UI 的捕获事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureEvent {
    /// 钩子**安装并自回声验证通过**——此后 `Combo` / `Cancel` 才有意义。
    /// 载荷 = 钩子线程 id（UI 已持 [`CaptureHookGuard`]，此 id 仅日志用）。
    Ready,
    /// 安装 / 自回声验证失败（已含重试）。UI 应回落 egui 基础捕获路径。
    Failed(&'static str),
    /// 组合键成立（含 ≥1 个 Ctrl/Alt/Win 修饰）。`mods` = MOD_* 位
    /// （ALT=1 / CONTROL=2 / SHIFT=4 / WIN=8），`vk` = 原生虚拟键码。
    Combo { mods: u32, vk: u32 },
    /// Esc（无修饰）→ 取消捕获。
    Cancel,
}

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
    let user32 = unsafe { GetModuleHandleA(c"user32".as_ptr().cast()) };
    if user32.is_null() {
        return;
    }
    let proc_addr = unsafe { GetProcAddress(user32, c"UnhookWindowsHookExW".as_ptr().cast()) };
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
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
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
    // F10 陈旧会话自检：THREAD_ID 恒存「最新安装会话」的线程 id。本线程若已
    // 不是最新会话（start() 被快速重启、旧线程收尾慢于新会话安装），立即自卸
    // 钩子并放行事件——旧钩子既不再吞键，也不再向新会话通道投递，根除
    // 「短暂双钩子并存 + 旧事件灌入新会话」竞态。自卸必须走
    // `unhook_windows_hook` 辅助（windows-sys 缺 UnhookWindowsHookExW 符号，
    // 见上方 91-95 行注记）；句柄读写仅在本线程（TLS），无锁竞争。
    // 卸载后本线程随泵循环退出（发送端被 Drop 清空 / WM_QUIT），TLS 随线程消亡。
    if *THREAD_ID.lock().unwrap_or_else(|e| e.into_inner()) != unsafe { GetCurrentThreadId() } {
        OWN_HOOK.with(|c| {
            let h = c.get();
            if !h.is_null() {
                // SAFETY：句柄来自本线程 SetWindowsHookExW 的成功返回，且 LL
                // 钩子回调恒在安装线程执行（卸载线程 = 安装线程）。
                unsafe { unhook_windows_hook(h) };
                c.set(std::ptr::null_mut());
            }
        });
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

    // Esc（无修饰）= 取消。
    if vk == VK_ESCAPE as u32 && !(ctrl || alt || shift || win) {
        if let Ok(g) = CAPTURE_TX.lock() {
            if let Some(tx) = g.as_ref() {
                let _ = tx.send(CaptureEvent::Cancel);
            }
        }
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
    if let Ok(g) = CAPTURE_TX.lock() {
        if let Some(tx) = g.as_ref() {
            let _ = tx.send(CaptureEvent::Combo { mods, vk });
        }
    }
    1
}

/// 捕获钩子守卫：Drop = 卸载（清发送端 + WM_QUIT；泵循环检测到发送端已清
/// 也会自行 unhook 退出——双保险）。`start()` 立即返回守卫（安装异步进行）。
pub struct CaptureHookGuard;

impl Drop for CaptureHookGuard {
    fn drop(&mut self) {
        // 先清发送端再令线程退出：竞态窗口内的按键会穿透而非误吞（安全向）；
        // 泵循环每轮检测「发送端已清」自行 unhook（不依赖 WM_QUIT 必达）。
        if let Ok(mut tx) = CAPTURE_TX.lock() {
            *tx = None;
        }
        let thread_id = THREAD_ID.lock().map(|g| *g).unwrap_or(0);
        #[cfg(windows)]
        if thread_id != 0 {
            // SAFETY：线程 id 由安装线程回发，WM_QUIT 结束其消息循环。
            use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
            unsafe {
                PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
            }
        }
        // 不 join：卸载路径在 UI 线程（Esc / hide / 超时），钩子线程 unhook
        // 是纯系统调用级收尾，残留线程毫秒级消亡，无需阻塞 UI 等待。
    }
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

    std::thread::Builder::new()
        .name("dd-capture-hook".into())
        .spawn(move || {
            // ① 记录线程 id（CaptureHookGuard::drop 投递 WM_QUIT 用）。
            let tid = unsafe { GetCurrentThreadId() };
            if let Ok(mut g) = THREAD_ID.lock() {
                *g = tid;
            }
            // ② 先注册发送端再装钩子（回调随时可能被触发）。
            if let Ok(mut g) = CAPTURE_TX.lock() {
                *g = Some(tx);
            }
            // ③ 安装（失败重试一次：安全软件拦截常为瞬时）。
            // SAFETY：hmod = 主模块句柄（LL 钩子仅要求可定位回调所在模块）。
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
                let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
                // 先发 Failed **再**清发送端（顺序反了会静默丢失）。
                if let Ok(g) = CAPTURE_TX.lock() {
                    if let Some(tx) = g.as_ref() {
                        let _ = tx.send(CaptureEvent::Failed("SetWindowsHookExW 两连失败"));
                    }
                }
                if let Ok(mut g) = CAPTURE_TX.lock() {
                    *g = None;
                }
                log::error!("[dd-gui] 捕获钩子安装失败（GetLastError={err}）——回落 egui 捕获");
                return;
            }
            // F10：登记本线程钩子句柄（TLS）——hook_proc 陈旧自检的卸载依据。
            OWN_HOOK.with(|c| c.set(hook));
            // ④ **自回声验证**：注入一次 F15，验证钩子真的收到注入
            //    （SetWindowsHookExW 成功 ≠ 钩子真的在收事件——安全软件可能
            //    剥离）。F15 不在捕获白名单 → 对 UI 零副作用；**不带 Alt 等
            //    修饰注入**——验证旗标只看 F15 keydown，修饰注入是纯风险
            //    （这几毫秒内用户按任意键都会带上幽灵 Alt 修饰，可能触发
            //    前台应用菜单模式）。
            SELFTEST_HIT.store(false, Ordering::Relaxed);
            // SAFETY：注入合成按键（dwFlags 0=down / 2=KEYEVENTF_KEYUP）。
            unsafe {
                keybd_event(SELFTEST_VK as u8, 0, 0, 0); // F15 down
                keybd_event(SELFTEST_VK as u8, 0, 2, 0); // F15 up
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1200);
            // ⚠️ 等待窗口**必须持续泵消息**：LL 钩子回调仅在安装线程泵消息
            // 期间被系统调用——v3 初版在此窗口只 sleep 不泵 → hook_proc 永不
            // 触发 → 自回声必败 → 必然回落（真机「Alt+Space 弹系统菜单 +
            // Ctrl+Space 无反应」的直接根因，2026-09-30 探针对照实锤：
            // 探针等待期在阻塞泵里所以能过）。
            let mut hit = false;
            let mut quit = false;
            loop {
                // 单轮泵：排空队列；WM_QUIT（Drop/取消）→ 退出。
                loop {
                    let mut msg: MSG = unsafe { std::mem::zeroed() };
                    // SAFETY：peek 移除本线程队列消息。
                    let has =
                        unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) };
                    if has == 0 {
                        break;
                    }
                    if msg.message == WM_QUIT {
                        quit = true;
                        break;
                    }
                }
                if quit {
                    break;
                }
                if SELFTEST_HIT.load(Ordering::Relaxed) {
                    hit = true;
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            // 先发 Failed **再**清发送端（v3 初版先清后发 → Failed 静默丢失，
            // UI 提示永远停留在安装中状态——连带 bug，同日修复）。
            if !hit || quit {
                if let Ok(g) = CAPTURE_TX.lock() {
                    if let Some(tx) = g.as_ref() {
                        let _ = tx.send(CaptureEvent::Failed("钩子自回声验证失败（事件未达）"));
                    }
                }
                unsafe { unhook_windows_hook(hook) };
                if let Ok(mut g) = CAPTURE_TX.lock() {
                    *g = None;
                }
                log::error!("[dd-gui] 捕获钩子自回声验证失败——回落 egui 捕获");
                return;
            }
            if let Ok(g) = CAPTURE_TX.lock() {
                if let Some(tx) = g.as_ref() {
                    let _ = tx.send(CaptureEvent::Ready);
                }
            }
            log::info!("[dd-gui] 热键捕获：LL 钩子已安装并自验证通过（系统级，Win 键可录）");
            // ⑤ 常驻泵循环：PeekMessage 轮询式泵（5ms）——每轮检测发送端已清
            //    （被取消）即自行 unhook 退出，与 Guard::drop 的 WM_QUIT 双保险。
            loop {
                loop {
                    let mut msg: MSG = unsafe { std::mem::zeroed() };
                    // SAFETY：peek 移除本线程队列消息；WM_QUIT → 退出。
                    let has =
                        unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) };
                    if has == 0 {
                        break;
                    }
                    if msg.message == WM_QUIT {
                        unsafe { unhook_windows_hook(hook) };
                        if let Ok(mut g) = CAPTURE_TX.lock() {
                            *g = None;
                        }
                        return;
                    }
                }
                let cancelled = CAPTURE_TX.lock().map(|g| g.is_none()).unwrap_or(false);
                if cancelled {
                    unsafe { unhook_windows_hook(hook) };
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        })
        .map(|_| CaptureHookGuard)
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
        // F15（自回声键）必须在白名单**外**——保证自回声注入对 UI 零副作用。
        for vk in [0x11, 0x12, 0x5B, 0x5C, 0xA0, 0x25, 0x2D, 0x14, 0x6A, 0x7E] {
            assert!(!capture_vk_allowed(vk), "白名单外被误放：{vk:#x}");
        }
    }
}
