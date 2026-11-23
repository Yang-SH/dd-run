//! Windows 全局热键：唤起/隐藏面板（默认 `Alt+Space`，M6 批次 6.3 起可自定义）。
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

/// 内部热键 id（仅本进程内区分多个热键用）。
/// 热键消息循环的注册 id（RegisterHotKey / UnregisterHotKey 共用）。
#[cfg(windows)]
const HOTKEY_ID: i32 = 0xDD01;
/// N17：命令热键槽基址（slot 1..3 = 0xDD02..0xDD04；slot 0 = 面板 toggle，
/// id 不变——现行为逐位回归）。
#[cfg(windows)]
const HOTKEY_SLOT_BASE: i32 = 0xDD02;
/// 捕获期「系统组合键接管」专用热键 id（与 slot 0..3 分离，互不串扰）。
///
/// 回落捕获（LL 钩子被安全软件拦截）下按 Alt+Space，Windows 会把它解释成
/// 「弹出窗口系统菜单」——egui/winit 收不到 Space 键，组合永远录不进（真机
/// 2026-11-23）。临时注册该热键后，Windows 把整个组合从输入投递链摘除
///（`docs/hotkey-alt-space-default-2026-11-23.md` §2.1 实测），`WM_HOTKEY`
/// 改投本线程 → 回发 [`HotkeyEvent::CaptureCombo`] 供对话框录取。捕获结束即
/// 注销，系统菜单行为恢复（副作用窗口 ≤ 捕获超时 30s）。
// 沿用项目既有 0xDDxx 号段（HOTKEY_ID = 0xDD01、命令槽 = 0xDD02..0xDD04），
// 取下一个空位 0xDD05；与既有 id 同段，单测锚定不撞号。
// 注：RegisterHotKey 文档把 0xC000+ 划给共享 DLL（GlobalAddAtom），但本仓库
// 既有 id 已在该段且实测工作——此处保持一致，不扩大改动面。
#[cfg(windows)]
const HOTKEY_CAPTURE_ID: i32 = 0xDD05;
/// N17：命令槽批量重注册消息（wParam = 3×12 位打包[mods<<8|vk，vk 0 = 空槽]，
/// lParam = generation 配对）。
#[cfg(windows)]
const HOTKEY_SLOTS_MSG: u32 = 0x8003; // WM_APP + 3
/// 热键线程自定义命令消息（PostThreadMessageW 携带 seq + mods/vk）。
#[cfg(windows)]
const HOTKEY_CMD_MSG: u32 = 0x8002; // WM_APP + 2
/// 捕获接管热键的注册/注销命令（wParam = mods，lParam = vk；vk 0 = 注销）。
#[cfg(windows)]
const HOTKEY_CAPTURE_MSG: u32 = 0x8004; // WM_APP + 4

/// N17：命令热键槽打包（Windows 线程消息载荷）——每槽 12 位（mods 4bit << 8 |
/// vk 8bit），vk 0 = 空槽；3 槽 36 位装入 u64 低位。纯函数可单测（unpack 同径
/// 校验由线程侧测试锁定）。可捕获键集（capture_vk 域）vk ≤ 0x7B、mods ≤ 0xF，
/// 12 位/槽足够。
// 命令槽打包只服务 Windows 热键线程（Unix 走 id 反查）；单测仍需 → 双门控，
// 避免 check-linux/macos 因 dead_code 失败（CI `-D warnings`）。
#[cfg(any(windows, test))]
pub(crate) fn pack_command_slots(slots: &[(u32, u32)]) -> u64 {
    let mut packed = 0u64;
    for (i, (mods, vk)) in slots.iter().take(3).enumerate() {
        packed |= ((((*mods & 0xF) as u64) << 8) | (*vk & 0xFF) as u64) << (12 * i);
    }
    packed
}

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
    /// N17：命令热键槽触发（slot 1..3；宿主按绑定表分发执行自定义命令，
    /// 面板隐藏态亦生效）。
    Trigger { slot: u32 },
    /// N17：命令热键槽批量注册结果（`generation` 配对；`ok_mask` 位 i =
    /// 槽 i+1 注册成功——失败槽位由 UI 落 danger 行 + toast）。
    CommandSlots { generation: u32, ok_mask: u8 },
    /// 捕获期接管热键命中（`mods`/`vk` = 注册时的组合）。回落捕获下被系统
    /// 吞掉的组合（Alt+Space → 系统菜单）经此路落候选，不经 egui 事件。
    /// 非捕获期到达的同名事件由 UI 忽略（陈旧消息）。
    CaptureCombo { mods: u32, vk: u32 },
}

/// 热键线程句柄 + 事件接收端 + 重注册入口。
pub struct HotkeyThread {
    /// 供 eframe 线程消费的事件接收端。
    pub events: Receiver<HotkeyEvent>,
    /// join 句柄（drop 时不等待，进程退出即回收）；线程创建失败时为
    /// `None`（R-05：降级为无热键运行，不 panic）。
    _handle: Option<thread::JoinHandle<()>>,
    /// 热键线程 id（PostThreadMessageW 目标；非 Windows = 0，仅写不读）。
    #[cfg_attr(not(windows), allow(dead_code))]
    thread_id: u32,
    /// 非 Windows：重注册命令发送端（Windows 走 PostThreadMessageW，无此字段；
    /// 线程创建失败时发送端对端已死，send 静默失败由 UI 确认超时兜底）。
    #[cfg(not(windows))]
    cmd_tx: Option<Sender<UnixHotkeyCmd>>,
}

/// N17：非 Windows 命令（mpsc 无打包限制——结构体直传）。
#[cfg(not(windows))]
pub(crate) enum UnixHotkeyCmd {
    /// slot 0 重绑定（seq 配对，与 Windows 侧 `HotkeyCommand` 同语义）。
    Rebind(HotkeyCommand),
    /// N17：命令热键槽批量重注册（generation 配对；vk 0 = 空槽）。
    Slots {
        generation: u32,
        slots: [(u32, u32); 3],
    },
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

    /// N17：命令热键槽批量重注册（异步；结果经 [`HotkeyEvent::CommandSlots`]
    /// 以 generation 配对回发）。slot 0（面板 toggle）不受影响。
    #[cfg(windows)]
    pub fn re_register_slots(&self, packed: u64, generation: u32) {
        use windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
        unsafe {
            let _ = PostThreadMessageW(
                self.thread_id,
                HOTKEY_SLOTS_MSG,
                packed as usize,
                generation as isize,
            );
        }
    }

    /// 捕获期临时接管热键：`vk != 0` 注册（自动替换旧接管键），`vk == 0` 注销。
    ///
    /// 仅 Windows 有意义——回落捕获下 `Alt+Space` 被系统吞成「弹出系统菜单」，
    /// 通过 `RegisterHotKey` 把该组合从投递链摘除后，按键改以 `WM_HOTKEY`
    /// 到达热键线程并回发 [`HotkeyEvent::CaptureCombo`]（见
    /// [`HOTKEY_CAPTURE_ID`]）。非 Windows 无此系统行为，恒 no-op。
    /// 线程已死亡 / 桩线程（id=0）时 `PostThreadMessageW` 静默失败，不打断 UI。
    pub fn capture_sink(&self, mods: u32, vk: u32) {
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
            unsafe {
                let _ = PostThreadMessageW(
                    self.thread_id,
                    HOTKEY_CAPTURE_MSG,
                    mods as usize,
                    vk as isize,
                );
            }
        }
        #[cfg(not(windows))]
        let _ = (mods, vk);
    }

    /// 非 Windows 平台：经 `global-hotkey` 注册热键并启动轮询线程（O6 批次三
    /// 真实现，语义与 Windows 侧逐位对齐——启动失败降级发
    /// [`HotkeyEvent::ReRegistered`](false)，线程创建失败同口径 R-05）。
    #[cfg(not(windows))]
    pub fn spawn(ctx: eframe::egui::Context, mods: u32, vk: u32) -> Self {
        let (tx, rx) = mpsc::channel::<HotkeyEvent>();
        let (cmd_tx, cmd_rx) = mpsc::channel::<UnixHotkeyCmd>();
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
            let _ = tx.send(UnixHotkeyCmd::Rebind(cmd));
        }
    }

    /// N17：命令热键槽批量重注册（异步；结果经 [`HotkeyEvent::CommandSlots`]
    /// 以 generation 配对回发）。slot 0（面板 toggle）不受影响。
    #[cfg(not(windows))]
    pub fn re_register_slots(&self, slots: [(u32, u32); 3], generation: u32) {
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(UnixHotkeyCmd::Slots { generation, slots });
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
    // 捕获期临时接管热键（slot 独立；None = 未注册）。命令 HOTKEY_CAPTURE_MSG
    // 负责注册/注销；命中时经 CaptureCombo 回发 UI（不参与 slot 分派）。
    let mut capture_sink: Option<(u32, u32)> = None;
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
                if msg.wParam as i32 == HOTKEY_CAPTURE_ID {
                    // 捕获期接管热键命中：不参与 slot 分派，直接回发候选
                    // （回落捕获下 Alt+Space 不再弹系统菜单，改走这条）。
                    if let Some((mods, vk)) = capture_sink {
                        log::info!("[dd-gui] 捕获接管热键命中：{mods}+{vk}");
                        let _ = tx.send(HotkeyEvent::CaptureCombo { mods, vk });
                    }
                } else {
                    // N17：槽位分派——slot 0 = 面板 toggle（逐位不变）；1..3 = 命令
                    // 热键槽（宿主按绑定表分发执行自定义命令，面板隐藏态亦生效）。
                    let slot = (msg.wParam as i32 - HOTKEY_ID) as u32;
                    if slot == 0 {
                        let _ = tx.send(HotkeyEvent::Toggle);
                    } else {
                        let _ = tx.send(HotkeyEvent::Trigger { slot });
                    }
                }
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
                        crate::settings::hotkey_mods_label(cmd.mods),
                        crate::settings::hotkey_vk_label(cmd.vk)
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
            } else if msg.message == HOTKEY_SLOTS_MSG {
                // N17：命令热键槽批量重注册（generation 配对；slot 0 不动）。
                // 逐槽：解旧注册新（vk 0 = 空槽仅解注册）；失败槽位落日志，
                // 结果 ok_mask 经事件回发（UI 按位落 danger 行）。
                let packed = msg.wParam as u64;
                let generation = msg.lParam as u32;
                let mut ok_mask = 0u8;
                for slot in 1..=3u32 {
                    let v = (packed >> (12 * (slot as usize - 1))) & 0xFFF;
                    let mods = (v >> 8) as u32;
                    let vk = (v & 0xFF) as u32;
                    let id = HOTKEY_SLOT_BASE + slot as i32 - 1;
                    let _ = UnregisterHotKey(std::ptr::null_mut(), id);
                    if vk != 0 {
                        if RegisterHotKey(std::ptr::null_mut(), id, mods | MOD_NOREPEAT, vk) != 0 {
                            ok_mask |= 1 << (slot - 1);
                        } else {
                            log::warn!(
                                "[dd-gui] 命令热键槽 {slot} 注册失败（{}+{}）",
                                crate::settings::hotkey_mods_label(mods),
                                crate::settings::hotkey_vk_label(vk)
                            );
                        }
                    }
                }
                let _ = tx.send(HotkeyEvent::CommandSlots {
                    generation,
                    ok_mask,
                });
                ctx.request_repaint();
            } else if msg.message == HOTKEY_CAPTURE_MSG {
                // 捕获期接管热键注册/注销（vk 0 = 注销）：先注销旧键，再按需
                // 注册新键。失败仅记日志——回落 egui 路径仍可录普通组合键，
                // 不应因此打断捕获对话框；slot 0..3 全程不受影响。
                if capture_sink.take().is_some() {
                    let _ = UnregisterHotKey(std::ptr::null_mut(), HOTKEY_CAPTURE_ID);
                }
                let mods = (msg.wParam as u32) & crate::settings::HOTKEY_MODS_MASK;
                let vk = msg.lParam as u32;
                if vk != 0
                    && RegisterHotKey(
                        std::ptr::null_mut(),
                        HOTKEY_CAPTURE_ID,
                        mods | MOD_NOREPEAT,
                        vk,
                    ) != 0
                {
                    capture_sink = Some((mods, vk));
                    log::info!("[dd-gui] 捕获接管热键已注册：{mods}+{vk}");
                } else if vk != 0 {
                    log::warn!("[dd-gui] 捕获接管热键注册失败（{mods}+{vk}）——该组合可能无法录入");
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
    use global_hotkey::hotkey::HotKey;
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
    cmd_rx: Receiver<UnixHotkeyCmd>,
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
    // N17：slot 0 的 global-hotkey 注册 id + 命令槽现值簿记（crate 无逐槽
    // 反查——线程侧自持；id 0 = 未注册）。
    let mut slot0_id: u32 = 0;
    let mut slot_current: [(u32, u32, u32); 3] = [(0, 0, 0); 3];
    if let Some(m) = &manager {
        if let Some(hk) = to_hotkey(mods, vk) {
            let id0 = hk.id();
            match m.register(hk) {
                Ok(()) => {
                    current = Some((mods, vk));
                    slot0_id = id0;
                    log::info!(
                        "[dd-gui] 全局热键注册成功：{}",
                        crate::settings::hotkey_combo_label(mods, vk)
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
        // 1) 重注册命令（非阻塞清空）：slot 0 重绑定走 seq 协议（失败自动
        //    回滚旧键并回发失败事件，与 Windows 侧一致）；N17 命令槽批量
        //    重注册走 generation 配对。
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                UnixHotkeyCmd::Rebind(cmd) => {
                    let old = current;
                    // (ok, rolled_back)——match 全分支都会赋值，无需初始化
                    //（linux 构建曾报 unused_assignments：初始值从未被读）。
                    let result;
                    match (&manager, to_hotkey(cmd.mods, cmd.vk)) {
                        (Some(m), Some(hk)) => {
                            let new_id = hk.id();
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
                                    slot0_id = new_id;
                                    // 诊断日志：与 Windows 侧同口径（seq 供 UI 对账）
                                    log::info!(
                                        "[dd-gui] 全局热键注册成功（seq={}）：{}",
                                        cmd.seq,
                                        crate::settings::hotkey_combo_label(cmd.mods, cmd.vk)
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
                UnixHotkeyCmd::Slots { generation, slots } => {
                    // N17：命令热键槽批量重注册（generation 配对；vk 0 = 空槽
                    // 仅解注册）。逐槽：解旧（失败忽略——键可能已失效）→ 注册
                    // 新；失败槽位落日志，ok_mask 经事件回发（UI 按位落 danger
                    // 行）。
                    let mut ok_mask = 0u8;
                    for (i, (mods, vk)) in slots.iter().enumerate() {
                        let slot = i as u32 + 1;
                        let (old_id, old_mods, old_vk) = slot_current[i];
                        if old_id != 0 {
                            if let (Some(m), Some(old_hk)) =
                                (manager.as_ref(), to_hotkey(old_mods, old_vk))
                            {
                                let _ = m.unregister(old_hk);
                            }
                        }
                        slot_current[i] = (0, 0, 0);
                        if *vk != 0 {
                            match (manager.as_ref(), to_hotkey(*mods, *vk)) {
                                (Some(m), Some(hk)) => match m.register(hk) {
                                    Ok(()) => {
                                        ok_mask |= 1 << i;
                                        slot_current[i] = (hk.id(), *mods, *vk);
                                    }
                                    Err(e) => {
                                        log::warn!("[dd-gui] 命令热键槽 {slot} 注册失败（{e}）")
                                    }
                                },
                                _ => log::warn!(
                                    "[dd-gui] 命令热键槽 {slot} 键不可映射（{mods}+{vk}）"
                                ),
                            }
                        }
                    }
                    let _ = tx.send(HotkeyEvent::CommandSlots {
                        generation,
                        ok_mask,
                    });
                    ctx.request_repaint();
                }
            }
        }

        // 2) 全局热键事件：crate 按按下/释放各发一次，仅取按下；窗口隐藏时
        //    request_repaint 唤醒 egui。N17：按注册 id 分派——slot 0 = Toggle；
        //    1..3 = 命令槽触发（宿主按绑定表分发执行）。
        while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
            if ev.state() == HotKeyState::Pressed {
                let id = ev.id();
                if id != 0 && id == slot0_id {
                    let _ = tx.send(HotkeyEvent::Toggle);
                } else if let Some(pos) = slot_current
                    .iter()
                    .position(|(g, _, _)| *g != 0 && *g == id)
                {
                    let _ = tx.send(HotkeyEvent::Trigger {
                        slot: pos as u32 + 1,
                    });
                }
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

    /// 捕获接管热键的 id / 命令消息必须与 slot 0..3 及既有命令消息不撞号——
    /// 撞号会把「录 Alt+Space」误判成面板 Toggle 或命令槽触发。
    #[cfg(windows)]
    #[test]
    fn capture_sink_ids_are_distinct() {
        assert_ne!(HOTKEY_CAPTURE_ID, HOTKEY_ID, "不得与面板热键撞号");
        for slot in 1..=3 {
            assert_ne!(
                HOTKEY_CAPTURE_ID,
                HOTKEY_SLOT_BASE + slot - 1,
                "不得与命令槽 {slot} 撞号"
            );
        }
        assert_ne!(HOTKEY_CAPTURE_MSG, HOTKEY_CMD_MSG);
        assert_ne!(HOTKEY_CAPTURE_MSG, HOTKEY_SLOTS_MSG);
    }

    /// 捕获接管命令发送端在桩线程（id=0）/ 非 Windows 上恒不 panic——
    /// UI 侧 begin/end 无条件调用，不能因热键线程缺席而中断捕获对话框。
    #[test]
    fn capture_sink_on_dummy_thread_is_noop() {
        let thread = HotkeyThread::dummy();
        thread.capture_sink(
            crate::settings::HOTKEY_MODS_DEFAULT,
            crate::settings::HOTKEY_VK_DEFAULT,
        );
        thread.capture_sink(0, 0);
    }

    /// seq 协议命令打包 roundtrip（PostThreadMessageW 只有 2 个整型槽，
    /// mods<<32|vk 装包不可失真——含高位 vk 与全部 MOD 位组合）。
    #[cfg(windows)]
    #[test]
    fn hotkey_command_pack_roundtrip() {
        for cmd in [
            HotkeyCommand {
                seq: 0,
                mods: 0b0001,
                vk: 0x20,
            }, // 启动 Alt+Space
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

    /// to_hotkey 组合：默认 Alt+Space 可构造；空掩码走 None modifiers
    /// 分支；不可映射主键整体返回 None。
    #[cfg(not(windows))]
    #[test]
    fn to_hotkey_combines_mask_and_key() {
        use global_hotkey::hotkey::{Code, Modifiers};
        let hk = to_hotkey(0b0001, 0x20).expect("Alt+Space 应可构造");
        assert_eq!(hk.mods, mods_mask_to_modifiers(0b0001));
        assert_eq!(hk.key, Code::Space);
        // 空掩码：HotKey::new(None, _) 分支
        let bare = to_hotkey(0, 0x41).expect("裸键应可构造");
        assert_eq!(bare.mods, Modifiers::empty());
        assert_eq!(bare.key, Code::KeyA);
        // 主键不可映射 → None
        assert!(to_hotkey(0b1001, 0x5B).is_none());
    }

    /// N17：命令热键槽打包 roundtrip（12 位/槽：mods 4bit<<8 | vk 8bit；
    /// 空槽 vk 0；3 槽互不串扰）。
    #[test]
    fn n17_pack_command_slots_roundtrip() {
        let slots = [(3u32, 0x47u32), (0, 0), (0b1001, 0x20)];
        let packed = pack_command_slots(&slots);
        for (i, (mods, vk)) in slots.iter().enumerate() {
            let v = (packed >> (12 * i)) & 0xFFF;
            assert_eq!((v >> 8) as u32, mods & 0xF, "槽 {} mods", i + 1);
            assert_eq!((v & 0xFF) as u32, vk & 0xFF, "槽 {} vk", i + 1);
        }
        // 超出 3 槽的输入截断（take(3)）
        assert_eq!(
            pack_command_slots(&[(1, 0x41), (2, 0x42), (4, 0x43), (8, 0x44)]),
            pack_command_slots(&[(1, 0x41), (2, 0x42), (4, 0x43)]),
            "仅前 3 槽参与打包"
        );
    }
}
