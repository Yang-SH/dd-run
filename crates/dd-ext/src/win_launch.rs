//! Shell 启动原语（安全加固 S-01）：**`ShellExecuteW(verb="open")`，不经过 cmd.exe**。
//!
//! ## 为什么需要这个模块
//!
//! 加固前 `builtins/apps.rs` 用 `Command::new("cmd.exe").args(["/C", "start", "", <不受信>])`
//! 启动 `.lnk` / 协议 URL。该写法存在**跨层引号错配**（见
//! [`docs/security-audit-2026-09-23.md`](../../../docs/security-audit-2026-09-23.md) §3.1）：
//! `std::process::Command::args` 按 MSVC 规则给含空格参数加引号、并把参数内的 `"` 转义成
//! `\"`，而 **cmd.exe 不把 `\` 当转义符** —— 那个 `"` 会翻转 cmd 的引号状态，紧随其后的
//! `&` 即成为命令分隔符；参数不含空格时 Rust 干脆不加引号，`&` 直接就是分隔符。
//! 实测（含 PoC）：`.url` 里一行 `URL=https://a/"&calc&"` 即可让宿主执行任意命令。
//!
//! ## 修法：换掉汇点，而不是加转义
//!
//! `ShellExecuteW` 的 `lpFile` 是**文件/URL 参数，不是命令行**——Windows 不会再对它做一次
//! 分隔符/引号解析，`start` 本身也只是转调 `ShellExecute`，故这是**语义等价替换**：
//! 目录 → Explorer、文件 → 关联程序、协议 URL → 对应处理器，全部不变。副作用是
//! 攻击面从"命令行注入"直接消失（没有解释器可注入）。
//!
//! ## 为什么校验只拒「不可能合法」的输入
//!
//! 刻意**不**拒绝 `&` `%` `|` 等 cmd 元字符：它们在 Windows 文件名与 URL 中**是合法字符**
//! （`Start Menu\Programs\Foo & Bar\app.lnk` 真实存在），拒绝它们会造成既有应用无法启动的
//! 功能回归，而对注入零收益（本模块根本没有解释器）。只拒绝两类**在任何合法路径/URL 中
//! 都不可能出现**的输入：控制字符（含 `\r`/`\n`/`\0`，Windows 文件名非法）与裸双引号
//! （文件名非法、URL 中必须百分号编码），外加空串。
//!
//! 平台：Windows 实现真实；其余平台为编译恒成立的占位（P4「Windows 优先」）。

/// 启动目标是否合法（纯函数，跨平台可测）。
///
/// 判据（仅拒"不可能合法"的输入，见模块文档）：
/// - 非空（trim 后）；
/// - 不含控制字符（`\0`、`\r`、`\n`、`\t` 等；Windows 文件名非法，URL 亦非法）；
/// - 不含裸双引号（Windows 文件名非法，URL 中须 `%22`）。
pub fn target_is_safe(target: &str) -> bool {
    !target.trim().is_empty() && !target.chars().any(|c| c.is_control() || c == '"')
}

/// 用系统默认方式打开 `target`（文件 / 目录 / 协议 URL），**不经过任何 shell**。
///
/// 语义与「双击」一致：目录 → Explorer 窗口；文件 → 关联程序；`http(s)://` 等
/// 协议 URL → 注册的处理器。与加固前的 `cmd /C start "" <target>` 行为等价，
/// 但无命令行解析层。
///
/// 返回 `Ok(())` = `ShellExecuteW` 返回值 > 32（Windows 的失败约定是 ≤ 32）。
#[cfg(windows)]
pub fn shell_open(target: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    if !target_is_safe(target) {
        return Err(format!("非法的启动目标（S-01 校验拒绝）：{target:?}"));
    }

    /// UTF-16 NUL 结尾宽字符串（`windows-sys` 无 `wide_string!` 宏）。
    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    let verb = wide("open");
    let file = wide(target);
    // SAFETY：三个宽字符串均为 NUL 结尾且在本调用期间存活；其余参数按 API 约定为
    // null（父窗口、参数、工作目录），返回值按 ShellExecuteW 约定 > 32 视为成功。
    let h = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if h as isize > 32 {
        Ok(())
    } else {
        Err(format!("ShellExecuteW = {}", h as isize))
    }
}

/// 非 Windows 占位（P4 Windows 优先）：编译恒成立，调用即报「未实现」。
#[cfg(not(windows))]
pub fn shell_open(_target: &str) -> Result<(), String> {
    Err("Shell 启动：当前平台尚未实现（P4 Windows 优先）".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合法输入必须放行——**这是防功能回归的主判据**：Windows 文件名/URL 中合法出现
    /// `&` `%` `|` `^` `'` `#` `?` 等字符，收紧校验会误伤真实应用。
    #[test]
    fn accepts_legitimate_targets() {
        for ok in [
            r"C:\Program Files\7-Zip\7zFM.exe",
            // 含 cmd 元字符但**合法**的真实路径形态（加固前会被 cmd 误解析）
            r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Foo & Bar\app.lnk",
            r"C:\tools\100%\app.exe",
            r"C:\tools\a|b\app.exe",
            r"\\server\share\app.exe",
            "https://example.com/search?q=a%20b&x=1",
            "steam://rungameid/12345",
            "shell:AppsFolder\\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App",
        ] {
            assert!(target_is_safe(ok), "合法目标被误拒：{ok}");
        }
    }

    /// 非法输入必须拒绝：空串、控制字符、裸双引号（三者都不可能出现在合法路径/URL 中）。
    #[test]
    fn rejects_impossible_targets() {
        for bad in [
            "",
            "   ",
            "\n",
            "C:\\a\r\nb\\app.exe",
            "C:\\a\0b\\app.exe",
            "https://example.com/?q=a\ttab",
            // S-01 PoC 的两种注入形态（含裸引号）必须被拒
            "https://example.com/\"&calc&\"",
            "C:\\a b\\x\"&cd.>C:\\marker&\"z",
        ] {
            assert!(!target_is_safe(bad), "非法目标被放行：{bad:?}");
        }
    }

    /// 回归护栏：启动路径**不得再引入 `cmd.exe`**（S-01 是一类缺陷，不是单点 bug——
    /// 任何人日后用 `cmd /C start` 加回"便捷启动"都会把注入面带回来）。
    /// 读源码断言：注入点只能经 [`shell_open`]。
    #[test]
    fn launch_path_does_not_use_cmd() {
        let src = include_str!("builtins/apps.rs");
        assert!(
            !src.contains("Command::new(\"cmd.exe\")"),
            "builtins/apps.rs 不得再用 cmd.exe 承载启动参数（S-01）"
        );
        assert!(
            !src.contains("\"start\""),
            "builtins/apps.rs 不得再用 cmd 的 start 内建（S-01）"
        );
        // 正向：启动两臂应经 shell_open
        assert!(
            src.contains("win_launch::shell_open"),
            "启动路径应经 win_launch::shell_open"
        );
    }
}
