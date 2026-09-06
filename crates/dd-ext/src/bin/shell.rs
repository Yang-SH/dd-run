//! dd-ext-shell —— 内置「Shell 命令」扩展（`com.ddrun.shell`，⚙️ 平台相关）。
//!
//! 功能（M4 A10 核对点：Shell 执行）：
//! - **顶层**：`shell.open_terminal` —— 打开新终端窗口（Windows `cmd`）；
//! - **兜底**（§6.2）：模板命令 `shell.run.query`，`title = "运行 {query}"`——
//!   宿主渲染替换 `{query}`；invoke 时对 `context.query` 做**无头执行**
//!   （Windows `cmd /C`，捕获 stdout+stderr），3 秒超时终止，结果摘要以
//!   `ShowToast` 回显（长输出截断）。
//!
//! 说明：面板内执行任意 shell 命令与在 CmdPal Shell 中一致——命令以当前用户
//! 权限运行、无沙箱；危险命令（如 `shutdown`）由用户自行负责，扩展不设拦。
//!
//! 平台策略（P4 决策：Windows 优先）：Windows 实现 `cmd`；macOS / Linux
//! （`sh`/`$SHELL`）为**编译恒成立占位**，待对应平台轮实现。
//! 参考实现：[`docs/m4-record.md`](../../docs/m4-record.md) P4 决策。

use dd_ext::{i18n::tr, run, ExtensionSpec};
use dd_protocol::messages::InvokeParams;
use dd_protocol::model::{CommandItem, CommandRef, CommandResult, Icon, IconKind};

/// 无头执行超时（毫秒）：超时 kill，避免长命令挂死扩展进程（宿主侧另有 10s
/// invoke 超时会杀整个扩展进程并触发崩溃计数，这里先自行兜底）。
const EXEC_TIMEOUT_MS: u64 = 3_000;
/// 结果摘要最大长度（超出截断 + 省略号）。
const MAX_SUMMARY: usize = 120;

fn main() {
    run(&spec());
}

fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: "com.ddrun.shell",
        display_name: tr("Shell", "Shell"),
        description: tr(
            "打开终端 / 在面板中执行 shell 命令（Windows cmd）",
            "Open a terminal / run shell commands in the panel (Windows cmd)",
        ),
        frozen: true,
        has_fallback: true,
        capabilities: &[],
        log_tag: "dd-ext-shell",
        top_level: sys::top_level_commands,
        fallback: Some(fallback_commands),
        invoke: sys::handle_invoke,
    }
}

/// 兜底模板：`title` 的 `{query}` 由宿主渲染替换（§6.2）。
fn fallback_commands() -> Vec<CommandItem> {
    vec![CommandItem {
        id: "shell.run.query".to_string(),
        title: tr("运行 {query}", "Run {query}").to_string(),
        subtitle: Some(
            tr(
                "在面板中执行（cmd，无头捕获输出，3s 超时）",
                "Run in the panel (cmd, headless output capture, 3s timeout)",
            )
            .to_string(),
        ),
        icon: Some(Icon {
            kind: IconKind::Glyph,
            value: "\u{E756}".to_string(), // CommandPrompt
        }),
        section: Some(tr("Shell", "Shell").to_string()),
        tags: None,
        details: None,
        text_to_suggest: None,
        more_commands: None,
        command: CommandRef::Invoke,
    }]
}

/// Windows 实现（cmd）。
#[cfg(windows)]
mod sys {
    use super::*;
    use dd_ext::Effect;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    pub fn top_level_commands() -> Vec<CommandItem> {
        vec![CommandItem {
            id: "shell.open_terminal".to_string(),
            title: tr("打开终端", "Open Terminal").to_string(),
            subtitle: Some(tr("打开新的 cmd 窗口", "Open a new cmd window").to_string()),
            icon: Some(Icon {
                kind: IconKind::Glyph,
                value: "\u{E756}".to_string(),
            }),
            section: Some(tr("Shell", "Shell").to_string()),
            tags: Some(vec!["shell".to_string(), "terminal".to_string()]),
            details: None,
            text_to_suggest: None,
            more_commands: None,
            command: CommandRef::Invoke,
        }]
    }

    pub fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
        match params.id.as_str() {
            "shell.open_terminal" => {
                // start 新窗口中的 cmd（CREATE_NO_WINDOW 隐藏 start 自身控制台）
                use std::os::windows::process::CommandExt;
                let spawned = Command::new("cmd.exe")
                    .args(["/C", "start", "", "cmd"])
                    .creation_flags(0x0800_0000)
                    .spawn();
                match spawned {
                    Ok(_) => (CommandResult::Dismiss, Vec::new()),
                    Err(e) => (
                        CommandResult::ShowToast {
                            message: tr("打开终端失败：{e}", "Failed to open terminal: {e}")
                                .replace("{e}", &e.to_string()),
                            duration_ms: Some(3_000),
                        },
                        Vec::new(),
                    ),
                }
            }
            "shell.run.query" => {
                let query = params
                    .context
                    .as_ref()
                    .and_then(|c| c.query.as_deref())
                    .unwrap_or("")
                    .trim();
                if query.is_empty() {
                    return (
                        CommandResult::ShowToast {
                            message: tr(
                                "输入要执行的命令后选择「运行 …」项，例如 echo hello",
                                "Type a command to run, then pick the “Run …” item, e.g. echo hello",
                            )
                            .to_string(),
                            duration_ms: Some(2_500),
                        },
                        Vec::new(),
                    );
                }
                match run_capture("cmd.exe", &["/C", query]) {
                    Ok(output) => {
                        let summary = summarize(&output);
                        (
                            CommandResult::ShowToast {
                                message: summary,
                                duration_ms: Some(3_000),
                            },
                            Vec::new(),
                        )
                    }
                    Err(e) => (
                        CommandResult::ShowToast {
                            message: tr("执行失败：{e}", "Execution failed: {e}")
                                .replace("{e}", &e.to_string()),
                            duration_ms: Some(3_000),
                        },
                        Vec::new(),
                    ),
                }
            }
            other => (
                CommandResult::ShowToast {
                    message: tr("未知 shell 命令：{other}", "Unknown shell command: {other}")
                        .replace("{other}", other),
                    duration_ms: Some(2_500),
                },
                Vec::new(),
            ),
        }
    }

    /// 无头执行并捕获输出：轮询等待（超时 kill）→ 读管道 → 合并 stdout/stderr。
    fn run_capture(program: &str, args: &[&str]) -> Result<String, String> {
        let mut child = Command::new(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("spawn 失败：{e}"))?;

        // 轮询退出，超时则 kill（进程退出后管道写端关闭，wait_with_output 不会死锁）
        let deadline = Instant::now() + Duration::from_millis(EXEC_TIMEOUT_MS);
        loop {
            match child.try_wait().map_err(|e| e.to_string())? {
                Some(_) => break,
                None => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(format!("执行超时（>{EXEC_TIMEOUT_MS}ms），已终止"));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }

        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        let mut combined = decode_output(&output.stdout);
        let stderr = decode_output(&output.stderr);
        if !stderr.trim().is_empty() {
            combined.push_str(&stderr);
        }
        Ok(combined)
    }

    /// 解码子进程输出为 UTF-8 字符串。
    ///
    /// 中文 Windows 的 cmd 输出是 OEM 代码页（简中 936/GBK），`from_utf8_lossy`
    /// 会产生乱码（真机反馈：`'1+1' 不是内部或外部命令` → 乱码 toast）。策略：
    /// ① 合法 UTF-8（多数现代 CLI 如 git/pwsh 输出 UTF-8）直接采用；
    /// ② 否则按 `GetConsoleOutputCP()` 转码；③ 转码失败再回落 lossy。
    #[cfg(windows)]
    fn decode_output(bytes: &[u8]) -> String {
        if let Ok(s) = std::str::from_utf8(bytes) {
            return s.to_string();
        }
        let cp = unsafe { windows_sys::Win32::System::Console::GetConsoleOutputCP() };
        decode_with_codepage(bytes, cp)
            .unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned())
    }

    /// 按指定代码页把多字节字符串转为 UTF-16 再到 String（`MultiByteToWideChar`）。
    #[cfg(windows)]
    fn decode_with_codepage(bytes: &[u8], codepage: u32) -> Option<String> {
        if bytes.is_empty() {
            return Some(String::new());
        }
        let len = unsafe {
            windows_sys::Win32::Globalization::MultiByteToWideChar(
                codepage,
                0,
                bytes.as_ptr(),
                bytes.len() as i32,
                std::ptr::null_mut(),
                0,
            )
        };
        if len <= 0 {
            return None;
        }
        let mut utf16 = vec![0u16; len as usize];
        let written = unsafe {
            windows_sys::Win32::Globalization::MultiByteToWideChar(
                codepage,
                0,
                bytes.as_ptr(),
                bytes.len() as i32,
                utf16.as_mut_ptr(),
                len,
            )
        };
        if written != len {
            return None;
        }
        Some(String::from_utf16_lossy(&utf16))
    }

    /// 结果摘要：trim、压缩多余空行、截断加省略号。
    fn summarize(output: &str) -> String {
        let trimmed = output.trim();
        if trimmed.is_empty() {
            return "执行完成（无输出）".to_string();
        }
        // 压缩连续空行为单个换行，控制 toast 高度
        let compact = trimmed
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if compact.chars().count() > MAX_SUMMARY {
            let head: String = compact.chars().take(MAX_SUMMARY).collect();
            format!("{head}…")
        } else {
            compact
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn summarize_handles_empty_long_and_multiline() {
            assert_eq!(summarize("   \n\t "), "执行完成（无输出）");
            let long = "x".repeat(300);
            let s = summarize(&long);
            assert!(s.ends_with('…'));
            assert!(s.chars().count() <= MAX_SUMMARY + 1);
            let multi = "hello\r\nworld\n\n\n!";
            assert_eq!(summarize(multi), "hello world !");
        }

        #[test]
        fn run_capture_executes_echo() {
            // cmd 必存在于 Windows；echo 输出应包含 hello
            let out = run_capture("cmd.exe", &["/C", "echo hello"]).expect("echo 应成功");
            assert!(out.contains("hello"), "got {out}");
        }

        #[test]
        fn run_capture_timeouts_long_command() {
            let started = Instant::now();
            let r = run_capture("cmd.exe", &["/C", "ping -n 10 127.0.0.1"]);
            assert!(r.is_err(), "超过 3s 的命令应被终止");
            let err = r.unwrap_err();
            assert!(err.contains("超时"), "got {err}");
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "超时终止不应拖过长"
            );
        }

        #[test]
        #[cfg(windows)]
        fn decode_with_codepage_converts_gbk() {
            // "不是内部或外部命令" 的 GBK(CP936) 编码字节
            // （python: '...'.encode('gbk') → B2BB CAC7 C4DA B2BF BBF2 CDE2 B2BF C3FC C1EE）
            let gbk = [
                0xB2u8, 0xBB, 0xCA, 0xC7, 0xC4, 0xDA, 0xB2, 0xBF, 0xBB, 0xF2, 0xCD, 0xE2, 0xB2,
                0xBF, 0xC3, 0xFC, 0xC1, 0xEE,
            ];
            assert_eq!(
                decode_with_codepage(&gbk, 936).as_deref(),
                Some("不是内部或外部命令"),
                "GBK 字节应按 CP936 正确转码"
            );
            // 合法 UTF-8 直接走快路径
            assert_eq!(decode_output("你好 utf8".as_bytes()), "你好 utf8");
            // 无效字节回落 lossy 不 panic
            assert!(!decode_output(&[0xFF, 0xFE]).is_empty());
        }
    }
}

/// 非 Windows 占位（P4 Windows 优先）：编译恒成立，功能待对应平台轮实现。
#[cfg(not(windows))]
mod sys {
    use super::*;
    use dd_ext::Effect;

    pub fn top_level_commands() -> Vec<CommandItem> {
        // macOS / Linux：$SHELL 打开终端与执行，TODO 对应平台轮
        Vec::new()
    }

    pub fn handle_invoke(_params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
        (
            CommandResult::ShowToast {
                message: tr(
                    "Shell 命令：当前平台尚未实现（P4 Windows 优先）",
                    "Shell commands: not implemented on this platform yet (P4: Windows first)",
                )
                .to_string(),
                duration_ms: Some(2_500),
            },
            Vec::new(),
        )
    }
}
