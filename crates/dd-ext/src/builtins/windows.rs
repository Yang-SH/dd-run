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

// ── Windows 实实现 ─────────────────────────────────────────────────────────────

#[cfg(windows)]
mod imp {
    use super::WindowEntry;
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows_sys::Win32::System::Threading::{
        AttachThreadInput, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow,
        GWL_EXSTYLE, SW_RESTORE, WS_EX_TOOLWINDOW,
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

    /// 激活（经典前台锁规避）：最小化先 `SW_RESTORE`；`AttachThreadInput`
    /// （当前前台线程 ↔ 目标线程）后 `SetForegroundWindow` 再分离；失败 =
    /// UIPI 拒绝（提权窗口）→ 调用方 toast 降级（方案边界）。
    pub(crate) fn activate_window(hwnd: isize) -> Result<(), String> {
        let h = hwnd as HWND;
        unsafe {
            if IsIconic(h) != 0 {
                ShowWindow(h, SW_RESTORE);
            }
            let fg = GetForegroundWindow();
            let fg_tid = GetWindowThreadProcessId(fg, std::ptr::null_mut());
            let target_tid = GetWindowThreadProcessId(h, std::ptr::null_mut());
            let attached = fg_tid != 0
                && target_tid != 0
                && fg_tid != target_tid
                && AttachThreadInput(fg_tid, target_tid, 1) != 0;
            let ok = SetForegroundWindow(h) != 0;
            if attached {
                AttachThreadInput(fg_tid, target_tid, 0);
            }
            if ok {
                Ok(())
            } else {
                Err("SetForegroundWindow 失败（可能为提权窗口，UIPI 边界）".to_string())
            }
        }
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
}
