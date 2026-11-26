//! N16（2026-11-22）：窗口切换器内置扩展（`com.ddrun.windows`；ALTRun 对照
//! 方案 §4.11 / 实施计划批次 D-2）——`EnumWindows` 枚举可见顶层窗口（过滤
//! 工具窗 / 幽灵窗口 / 自身），模糊 / 拼音搜索（宿主管线对 title 自动生成），
//! invoke 经经典 `AttachThreadInput → SetForegroundWindow` 前台锁规避激活
//! （最小化先 `SW_RESTORE`）；UIPI 提权窗口失败 → toast 降级（方案边界：
//! 强制切换不做）。
//!
//! 平台：仅 Windows 实实现；非 Windows 构建 → 空表（窗口切换为 Windows
//! 固有语义，非目标平台记档）。图标 v1 glyph 占位（图标抽取属后续档，方案
//! 边界）；窗口内容预览不做。

use crate::Effect;
use crate::ExtensionSpec;
use dd_protocol::messages::InvokeParams;
use dd_protocol::model::{CommandItem, CommandRef, CommandResult, Icon, IconKind};

pub fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: "com.ddrun.windows",
        display_name: tr("窗口", "Windows"),
        description: tr(
            "窗口切换：列出可见顶层窗口（模糊 / 拼音搜索），Enter 一键切换",
            "Window switcher: list visible top-level windows and activate with Enter",
        ),
        // 窗口列表随时变 → fresh（不落磁盘桩，方案 §4.11 口径）
        frozen: false,
        has_fallback: false,
        capabilities: &[],
        log_tag: "dd-ext-windows",
        top_level: top_level_commands,
        fallback: None,
        pages: None,
        invoke: handle_invoke,
    }
}

use crate::i18n::tr;

/// 窗口条目（枚举产出；过滤谓词纯函数可单测——Win32 调用集中在
/// [`enumerate_windows`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowEntry {
    /// 窗口句柄（激活 id 内嵌：`windows.activate.<hwnd>`）。
    pub hwnd: isize,
    /// 窗口标题（条目标题；宿主管线生成拼音索引——中文标题天然可拼音搜）。
    pub title: String,
    /// 进程名（去扩展名，如 `notepad`；副标题展示 + 自身排除判定）。
    pub process: String,
}

/// 过滤谓词（纯函数，HWND/字段注入 fixture 可单测）——`visible` /
/// `toolwindow` / `cloaked` 由 enumerate 的 Win32 调用产出。规则 =
/// 方案 §4.11：可见 + 标题非空 + 非工具窗 + 非 DWM cloak（UWP 幽灵）+ 非
/// 自身（**实现细化记档**：进程名前缀 `dd-run` 排除替代 HWND 精确比对——
/// in-process 拿宿主 HWND 需平台句柄透传，v1 从简；顺带排除其它 dd-run
/// 实例，切换同类启动器无意义）。
// 非 Windows 无 `enumerate_windows` 调用方（`imp` 为空表）→ `cfg(any(windows, test))`
// 兼顾单测；不加会在 check-linux/macos 触发 dead_code（CI `-D warnings`）。
#[cfg(any(windows, test))]
fn keep(title: &str, process: &str, visible: bool, toolwindow: bool, cloaked: bool) -> bool {
    visible
        && !toolwindow
        && !cloaked
        && !title.trim().is_empty()
        && !process.to_lowercase().starts_with("dd-run")
}

/// 条目上限（z-order 前 50，方案 §4.11 口径）。
const WINDOW_CAP: usize = 50;

/// 条目组装（纯函数可单测）：id = `windows.activate.<hwnd>`（句柄内嵌，无
/// 注册表竞态面）；副标题 = 进程名；图标 = glyph 占位（图标抽取属后续档）。
fn window_item(w: &WindowEntry) -> CommandItem {
    CommandItem {
        id: format!("windows.activate.{}", w.hwnd),
        title: w.title.clone(),
        subtitle: Some(w.process.clone()),
        icon: Some(Icon {
            kind: IconKind::Glyph,
            value: "\u{E7F4}".to_string(), // Windows 徽标 glyph（覆盖以机检为准）
        }),
        section: Some(tr("窗口", "Windows").to_string()),
        tags: None,
        details: None,
        text_to_suggest: None,
        more_commands: None,
        command: CommandRef::Invoke,
    }
}

/// 顶层命令：枚举 → 过滤 → cap 50 → 条目（z-order 顺序 = EnumWindows 遍历
/// 序，方案口径）。
fn top_level_commands() -> Vec<CommandItem> {
    enumerate_windows()
        .into_iter()
        .take(WINDOW_CAP)
        .map(|w| window_item(&w))
        .collect()
}

/// invoke：`windows.activate.<hwnd>` → 激活（成功 Dismiss 关面板；失败 toast
/// ——UIPI 边界内降级，方案验收腿）；其他 id 忽略（KeepOpen）。
fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
    let Some(hwnd_str) = params.id.strip_prefix("windows.activate.") else {
        return (CommandResult::KeepOpen, vec![]);
    };
    let Ok(hwnd) = hwnd_str.parse::<isize>() else {
        return (CommandResult::KeepOpen, vec![]);
    };
    match activate_window(hwnd) {
        Ok(()) => (CommandResult::Dismiss, vec![]),
        Err(e) => (
            CommandResult::ShowToast {
                message: format!("{}：{e}", tr("窗口切换失败", "Window switch failed")),
                duration_ms: None,
            },
            vec![],
        ),
    }
}

#[cfg(any(windows, test))]
const SW_RESTORE_CMD: i32 = 1;
#[cfg(any(windows, test))]
const SW_SHOWMINNOACTIVE_CMD: i32 = 7;

#[cfg(any(windows, test))]
trait WindowActivationOps {
    fn is_iconic(&mut self, hwnd: isize) -> bool;
    fn is_window_visible(&mut self, hwnd: isize) -> bool;
    fn show_window(&mut self, hwnd: isize, command: i32) -> Result<(), String>;
    fn foreground_window(&mut self) -> isize;
    fn window_thread_id(&self, hwnd: isize) -> u32;
    fn attach_thread_input(&mut self, from: u32, to: u32) -> bool;
    fn detach_thread_input(&mut self, from: u32, to: u32) -> bool;
    fn set_foreground_window(&mut self, hwnd: isize) -> Result<(), String>;
}

#[cfg(any(windows, test))]
fn activate_window_with(hwnd: isize, ops: &mut impl WindowActivationOps) -> Result<(), String> {
    let original_foreground = ops.foreground_window();
    let was_iconic = ops.is_iconic(hwnd);
    if was_iconic {
        ops.show_window(hwnd, SW_RESTORE_CMD)
            .map_err(|e| format!("窗口恢复失败：{e}"))?;
    }
    if ops.is_iconic(hwnd) || !ops.is_window_visible(hwnd) {
        return Err("窗口恢复后不可见或仍处于最小化态".to_string());
    }

    let original_tid = ops.window_thread_id(original_foreground);
    let target_tid = ops.window_thread_id(hwnd);
    let attached = original_tid != 0
        && target_tid != 0
        && original_tid != target_tid
        && ops.attach_thread_input(original_tid, target_tid);
    let result = ops.set_foreground_window(hwnd);
    let failure = match result {
        Err(e) => Some(e),
        Ok(()) if ops.foreground_window() != hwnd => {
            Some("SetForegroundWindow 返回成功但前台 HWND 未切换".to_string())
        }
        Ok(()) => None,
    };
    if let Some(mut failure) = failure {
        if let Err(rollback_error) = rollback_activation(ops, original_foreground, hwnd, was_iconic)
        {
            failure.push_str(&format!("；回滚失败：{rollback_error}"));
        }
        let detach_failed = attached && !ops.detach_thread_input(original_tid, target_tid);
        if detach_failed {
            failure.push_str("；解附着失败");
        }
        return Err(failure);
    }
    if attached && !ops.detach_thread_input(original_tid, target_tid) {
        let mut failure = "解附着失败".to_string();
        if let Err(rollback_error) = rollback_activation(ops, original_foreground, hwnd, was_iconic)
        {
            failure.push_str(&format!("；回滚失败：{rollback_error}"));
        }
        if !ops.detach_thread_input(original_tid, target_tid) {
            failure.push_str("；重试解附着失败");
        }
        return Err(failure);
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn rollback_activation(
    ops: &mut impl WindowActivationOps,
    original_foreground: isize,
    hwnd: isize,
    was_iconic: bool,
) -> Result<(), String> {
    let mut errors = Vec::new();
    if was_iconic {
        if let Err(e) = ops.show_window(hwnd, SW_SHOWMINNOACTIVE_CMD) {
            errors.push(format!("最小化恢复失败：{e}"));
        }
    }
    if original_foreground != 0 {
        if let Err(e) = ops.set_foreground_window(original_foreground) {
            errors.push(format!("焦点恢复失败：{e}"));
        }
    }
    if original_foreground != 0 && ops.foreground_window() != original_foreground {
        errors.push("焦点恢复后 HWND 不一致".to_string());
    }
    if ops.is_iconic(hwnd) != was_iconic {
        errors.push("最小化态恢复后不一致".to_string());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("；"))
    }
}

// ── Windows 实实现 ─────────────────────────────────────────────────────────────

#[cfg(windows)]
mod imp {
    use super::{activate_window_with, WindowActivationOps, WindowEntry};
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows_sys::Win32::System::Threading::{
        AttachThreadInput, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow,
        GWL_EXSTYLE, WS_EX_TOOLWINDOW,
    };

    /// 枚举原始候选（过滤前置字段——`keep` 纯谓词消费，可注入单测）。
    pub(crate) struct RawWindow {
        pub hwnd: isize,
        pub title: String,
        pub process: String,
        pub visible: bool,
        pub toolwindow: bool,
        pub cloaked: bool,
    }

    /// `EnumWindows` 回调：逐窗读取标题 / 样式 / cloak / 进程名，收集候选
    /// （过滤延后到 `keep` 纯谓词——宿主侧单测口径）。
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
        let out = unsafe { &mut *(lparam as *mut Vec<RawWindow>) };
        let visible = unsafe { IsWindowVisible(hwnd) != 0 };
        let toolwindow =
            unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } & WS_EX_TOOLWINDOW as i32 != 0;
        let mut cloaked_flag: u32 = 0;
        let cloaked = unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED as u32,
                &mut cloaked_flag as *mut u32 as *mut core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            ) == 0
                && cloaked_flag != 0
        };
        let len = unsafe { GetWindowTextLengthW(hwnd) };
        let title = if len <= 0 {
            String::new()
        } else {
            let mut buf = vec![0u16; len as usize + 1];
            // 用**实际写入长度**切片，而非预查询长度：两次调用之间标题可能变短
            // （或被销毁 → 返回 0），按 len 切片会带出 NUL 填充，`trim()` 又不
            // 视 NUL 为空白 → 该窗口会以「空白行」进入列表（2026-11-22 复核轮）。
            let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), len + 1) };
            if n <= 0 {
                String::new()
            } else {
                String::from_utf16_lossy(&buf[..n as usize])
            }
        };
        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        let process = if pid == 0 {
            String::new()
        } else {
            unsafe {
                match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) {
                    h if !h.is_null() => {
                        let mut buf = [0u16; 1024];
                        let mut l = buf.len() as u32;
                        let ok = QueryFullProcessImageNameW(
                            h,
                            PROCESS_NAME_WIN32,
                            buf.as_mut_ptr(),
                            &mut l,
                        );
                        CloseHandle(h);
                        if ok != 0 && l > 0 {
                            std::path::Path::new(&String::from_utf16_lossy(&buf[..l as usize]))
                                .file_stem()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_default()
                        } else {
                            String::new()
                        }
                    }
                    _ => String::new(),
                }
            }
        };
        out.push(RawWindow {
            hwnd: hwnd as isize,
            title,
            process,
            visible,
            toolwindow,
            cloaked,
        });
        1 // 继续枚举
    }

    /// 枚举可见顶层窗口（z-order 序；cap 由调用方 [`super::top_level_commands`]
    /// 施加）。
    pub(crate) fn enumerate_windows() -> Vec<super::WindowEntry> {
        let mut raw: Vec<RawWindow> = Vec::new();
        unsafe {
            EnumWindows(Some(enum_proc), &mut raw as *mut _ as LPARAM);
        }
        raw.into_iter()
            .filter(|r| super::keep(&r.title, &r.process, r.visible, r.toolwindow, r.cloaked))
            .map(|r| WindowEntry {
                hwnd: r.hwnd,
                title: r.title,
                process: r.process,
            })
            .collect()
    }

    struct Win32WindowOps;

    impl WindowActivationOps for Win32WindowOps {
        fn is_iconic(&mut self, hwnd: isize) -> bool {
            unsafe { IsIconic(hwnd as HWND) != 0 }
        }

        fn is_window_visible(&mut self, hwnd: isize) -> bool {
            unsafe { IsWindowVisible(hwnd as HWND) != 0 }
        }

        fn show_window(&mut self, hwnd: isize, command: i32) -> Result<(), String> {
            unsafe { ShowWindow(hwnd as HWND, command) };
            Ok(())
        }

        fn foreground_window(&mut self) -> isize {
            unsafe { GetForegroundWindow() as isize }
        }

        fn window_thread_id(&self, hwnd: isize) -> u32 {
            unsafe { GetWindowThreadProcessId(hwnd as HWND, std::ptr::null_mut()) }
        }

        fn attach_thread_input(&mut self, from: u32, to: u32) -> bool {
            unsafe { AttachThreadInput(from, to, 1) != 0 }
        }

        fn detach_thread_input(&mut self, from: u32, to: u32) -> bool {
            unsafe { AttachThreadInput(from, to, 0) != 0 }
        }

        fn set_foreground_window(&mut self, hwnd: isize) -> Result<(), String> {
            if unsafe { SetForegroundWindow(hwnd as HWND) } == 0 {
                Err("SetForegroundWindow 失败（可能为提权窗口，UIPI 边界）".to_string())
            } else {
                Ok(())
            }
        }
    }

    /// 激活后复核前台 HWND；失败恢复原焦点与最小化态。
    pub(crate) fn activate_window(hwnd: isize) -> Result<(), String> {
        activate_window_with(hwnd, &mut Win32WindowOps)
    }
}

#[cfg(windows)]
use imp::{activate_window, enumerate_windows};

/// 非 Windows：窗口切换为 Windows 固有语义——空表（非目标平台记档；
/// CI macos check 仅编译路径）。
#[cfg(not(windows))]
mod imp {
    pub(crate) fn enumerate_windows() -> Vec<super::WindowEntry> {
        Vec::new()
    }

    pub(crate) fn activate_window(_hwnd: isize) -> Result<(), String> {
        Err("窗口切换仅支持 Windows".to_string())
    }
}
#[cfg(not(windows))]
use imp::{activate_window, enumerate_windows};

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str, process: &str) -> WindowEntry {
        WindowEntry {
            hwnd: 42,
            title: title.to_string(),
            process: process.to_string(),
        }
    }

    /// 过滤谓词：可见 + 标题非空 + 非工具窗 + 非 cloak + 非自身（dd-run
    /// 前缀进程名排除——实现细化记档）。
    #[test]
    fn n16_keep_filter_predicates() {
        assert!(keep("记事本", "notepad", true, false, false));
        assert!(!keep("", "notepad", true, false, false), "空标题排除");
        assert!(!keep("  ", "notepad", true, false, false), "纯空白排除");
        assert!(!keep("X", "notepad", false, false, false), "不可见排除");
        assert!(!keep("X", "notepad", true, true, false), "工具窗排除");
        assert!(
            !keep("X", "notepad", true, false, true),
            "cloak 排除（UWP 幽灵）"
        );
        assert!(
            !keep("dd-run", "dd-run-0.3.0", true, false, false),
            "自身进程排除（dd-run 前缀，实现细化记档）"
        );
        assert!(keep("其它实例", "explorer", true, false, false));
    }

    /// 条目形态：id = `windows.activate.<hwnd>`（句柄内嵌无注册表）、副标题 =
    /// 进程名、分组「窗口」、Invoke；cap 50（61 条 fabricated 截 50）。
    #[test]
    fn n16_window_item_shape_and_cap() {
        let w = entry("记事本 - 无标题", "notepad");
        let item = window_item(&w);
        assert_eq!(item.id, "windows.activate.42");
        assert_eq!(item.title, "记事本 - 无标题");
        assert_eq!(item.subtitle.as_deref(), Some("notepad"));
        assert_eq!(item.section.as_deref(), Some(tr("窗口", "Windows")));
        assert!(matches!(item.command, CommandRef::Invoke));

        let many: Vec<WindowEntry> = (0..61)
            .map(|i| WindowEntry {
                hwnd: i,
                title: format!("w{i}"),
                process: "app".into(),
            })
            .collect();
        let items: Vec<CommandItem> = many
            .into_iter()
            .take(WINDOW_CAP)
            .map(|w| window_item(&w))
            .collect();
        assert_eq!(items.len(), 50, "cap 50");
    }

    struct FakeWindowOps {
        foreground: isize,
        foreground_after_set: Option<isize>,
        minimized: bool,
        visible: bool,
        restore_error: bool,
        restore_noop: bool,
        set_error: bool,
        rollback_focus_error: bool,
        rollback_minimize_error: bool,
        attach_ok: bool,
        detach_ok: bool,
        actions: Vec<(&'static str, isize, i32)>,
    }

    impl FakeWindowOps {
        fn new() -> Self {
            Self {
                foreground: 1,
                foreground_after_set: None,
                minimized: false,
                visible: true,
                restore_error: false,
                restore_noop: false,
                set_error: false,
                rollback_focus_error: false,
                rollback_minimize_error: false,
                attach_ok: true,
                detach_ok: true,
                actions: Vec::new(),
            }
        }
    }

    impl WindowActivationOps for FakeWindowOps {
        fn is_window_visible(&mut self, _hwnd: isize) -> bool {
            self.visible
        }

        fn is_iconic(&mut self, hwnd: isize) -> bool {
            self.actions.push(("iconic", hwnd, 0));
            self.minimized
        }

        fn show_window(&mut self, hwnd: isize, command: i32) -> Result<(), String> {
            self.actions.push(("show", hwnd, command));
            if command == SW_RESTORE_CMD {
                if self.restore_error {
                    return Err("restore failed".to_string());
                }
                if !self.restore_noop {
                    self.minimized = false;
                    self.visible = true;
                }
            } else if command == SW_SHOWMINNOACTIVE_CMD {
                if self.rollback_minimize_error {
                    return Err("minimize rollback failed".to_string());
                }
                self.minimized = true;
            }
            Ok(())
        }

        fn foreground_window(&mut self) -> isize {
            self.actions.push(("foreground", 0, 0));
            self.foreground
        }

        fn window_thread_id(&self, hwnd: isize) -> u32 {
            match hwnd {
                1 => 10,
                10 => 20,
                _ => 0,
            }
        }

        fn attach_thread_input(&mut self, _from: u32, _to: u32) -> bool {
            self.actions.push(("attach", 0, 0));
            self.attach_ok
        }

        fn detach_thread_input(&mut self, _from: u32, _to: u32) -> bool {
            self.actions.push(("detach", 0, 0));
            self.detach_ok
        }

        fn set_foreground_window(&mut self, hwnd: isize) -> Result<(), String> {
            self.actions.push(("set", hwnd, 0));
            if hwnd == 10 && self.set_error {
                return Err("SetForegroundWindow denied".to_string());
            }
            if hwnd == 1 && self.rollback_focus_error {
                return Err("focus rollback failed".to_string());
            }
            self.foreground = if hwnd == 10 {
                self.foreground_after_set.unwrap_or(hwnd)
            } else {
                hwnd
            };
            Ok(())
        }
    }

    #[test]
    fn p1_14_activation_verifies_foreground_and_restores_minimized_target() {
        let mut ops = FakeWindowOps::new();
        ops.minimized = true;
        ops.foreground_after_set = Some(99);
        let err = activate_window_with(10, &mut ops).expect_err("焦点未落到目标必须失败");
        assert!(err.contains("前台"), "{err}");
        assert_eq!(ops.foreground, 1, "失败后恢复原焦点");
        assert!(ops.minimized, "失败后恢复原最小化态");
        assert_eq!(ops.actions[0], ("foreground", 0, 0));
        assert_eq!(ops.actions[1], ("iconic", 10, 0));
        assert!(ops.actions.contains(&("set", 1, 0)));
        assert!(ops.actions.contains(&("show", 10, SW_SHOWMINNOACTIVE_CMD)));
    }

    #[test]
    fn p1_14_activation_success_keeps_target_restored() {
        let mut ops = FakeWindowOps::new();
        ops.minimized = true;
        activate_window_with(10, &mut ops).expect("目标成为前台即成功");
        assert_eq!(ops.foreground, 10);
        assert!(!ops.minimized);
        assert!(!ops.actions.contains(&("set", 1, 0)));
        assert!(!ops.actions.contains(&("show", 10, SW_SHOWMINNOACTIVE_CMD)));
        assert!(ops.actions.contains(&("detach", 0, 0)));
    }

    #[test]
    fn p1_14_activation_failure_restores_focus_without_minimizing_normal_target() {
        let mut ops = FakeWindowOps::new();
        ops.set_error = true;
        let err = activate_window_with(10, &mut ops).expect_err("提权拒绝必须失败");
        assert!(err.contains("SetForegroundWindow"), "{err}");
        assert_eq!(ops.foreground, 1);
        assert!(!ops.minimized);
        assert!(!ops.actions.contains(&("show", 10, SW_SHOWMINNOACTIVE_CMD)));
        assert!(ops.actions.contains(&("set", 1, 0)));
    }

    #[test]
    fn p1_14_restore_must_leave_target_visible() {
        let mut ops = FakeWindowOps::new();
        ops.visible = false;
        ops.restore_noop = true;
        assert!(activate_window_with(10, &mut ops).is_err());
        assert!(!ops.actions.iter().any(|action| action.0 == "set"));
    }

    #[test]
    fn p1_14_detach_failure_rolls_back_and_retries() {
        let mut ops = FakeWindowOps::new();
        ops.minimized = true;
        ops.detach_ok = false;
        let err = activate_window_with(10, &mut ops).expect_err("解附着失败必须上报");
        assert!(err.contains("解附着失败"), "{err}");
        assert_eq!(ops.foreground, 1, "解附着失败也必须恢复原焦点");
        assert!(ops.minimized, "解附着失败也必须恢复原最小化态");
        let first_detach = ops.actions.iter().position(|a| a.0 == "detach").unwrap();
        let restore_focus = ops
            .actions
            .iter()
            .position(|a| a.0 == "set" && a.1 == 1)
            .unwrap();
        let second_detach = ops
            .actions
            .iter()
            .skip(first_detach + 1)
            .position(|a| a.0 == "detach");
        assert!(restore_focus > first_detach, "首次解附着失败后才回滚");
        assert!(second_detach.is_some(), "回滚后应重试解附着");
    }

    #[test]
    fn p1_14_rollback_failure_is_reported_after_detach() {
        let mut ops = FakeWindowOps::new();
        ops.minimized = true;
        ops.set_error = true;
        ops.rollback_focus_error = true;
        let err = activate_window_with(10, &mut ops).expect_err("回滚失败必须显式上报");
        assert!(err.contains("回滚失败"), "{err}");
        let detach = ops.actions.iter().position(|a| a.0 == "detach").unwrap();
        let restore_focus = ops
            .actions
            .iter()
            .position(|a| a.0 == "set" && a.1 == 1)
            .unwrap();
        assert!(detach > restore_focus, "附着必须保持到回滚完成之后");
    }

    #[test]
    fn p1_14_activation_tolerates_attach_failure_when_target_gets_focus() {
        let mut ops = FakeWindowOps::new();
        ops.attach_ok = false;
        activate_window_with(10, &mut ops).expect("attach 失败不否决实际焦点结果");
        assert!(!ops.actions.contains(&("detach", 0, 0)));
    }

    #[test]
    fn p1_14_restore_failure_stops_before_activation() {
        let mut ops = FakeWindowOps::new();
        ops.minimized = true;
        ops.restore_error = true;
        assert!(activate_window_with(10, &mut ops).is_err());
        assert!(!ops.actions.iter().any(|action| action.0 == "set"));
    }
}
