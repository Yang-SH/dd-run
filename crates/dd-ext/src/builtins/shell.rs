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
//! 工作目录：**打开终端与无头执行都统一在用户主目录**（`USERPROFILE`）——
//! 避免继承宿主进程不确定的 CWD（真机反馈：从任意目录启动宿主时，终端起始目录
//! 与相对路径命令的结果都会随启动位置漂移）。
//!
//! 平台策略（P4 决策：Windows 优先）：Windows 实现 `cmd`；macOS / Linux
//! （`sh`/`$SHELL`）为**编译恒成立占位**，待对应平台轮实现。
//! 参考实现：[`docs/m4-record.md`](../../docs/m4-record.md) P4 决策。

use crate::{i18n::tr, ExtensionSpec};
use dd_protocol::messages::InvokeParams;
use dd_protocol::model::{CommandItem, CommandRef, CommandResult, Icon, IconKind};

/// 无头执行超时（毫秒）：超时 kill，避免长命令挂死扩展进程（宿主侧另有 10s
/// invoke 超时会杀整个扩展进程并触发崩溃计数，这里先自行兜底）。
const EXEC_TIMEOUT_MS: u64 = 3_000;
/// 结果摘要最大长度（超出截断 + 省略号）。
const MAX_SUMMARY: usize = 120;

pub fn spec() -> ExtensionSpec {
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
        pages: None, // Shell 为顶层命令，无子页（§6.3）
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
    use crate::Effect;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    /// 用户主目录（终端起始目录）：Windows 取 `USERPROFILE`，其余取 `HOME`。
    /// 与「双击/开始菜单打开终端」的默认目录一致，避免继承宿主 CWD。
    fn home_dir() -> Option<std::path::PathBuf> {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(std::path::PathBuf::from)
    }

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

    /// 命中即需二次确认的**危险命令名**（S-06，2026-09-23）。
    ///
    /// 判据刻意保守：只列「显然不可逆或可致数据/系统损坏」的少数命令，避免把日常操作
    /// 变成处处弹窗。比对的是**命令名**（去路径、去扩展名：`C:\Windows\System32\format.com`
    /// → `format`），大小写不敏感。放在 `sys`（Windows）内是因为只有本模块的
    /// `shell.run.query` 分支消费它——非 Windows 分支恒为占位，不引入未使用项。
    const DANGEROUS_COMMANDS: &[&str] = &[
        "format",   // 格式化卷
        "diskpart", // 分区/卷操作（clean 不可逆）
        "cipher",   // /w 擦除空闲空间，不可逆
        "del",      // 删除文件
        "erase",    // del 别名
        "rd",       // 删除目录
        "rmdir",    // rd 长名
        "takeown",  // 夺取所有权（常与删除/替换连用）
        "icacls",   // 改写 ACL（可致系统不可访问）
        "vssadmin", // 删除卷影副本（勒索软件常用）
        "bcdedit",  // 改写引导配置（可致无法启动）
        "shutdown", // 关机/重启/注销
    ];

    /// 需二次确认的 `(命令, 子命令)` 组合（S-06）：同名的查询类子命令不该被拦
    /// （`reg query` / `net view` 照常直接执行）。
    const DANGEROUS_SUBCOMMANDS: &[(&str, &str)] = &[
        ("reg", "delete"), // reg delete 删注册表项
        ("net", "user"),   // net user 改账号/口令
    ];

    /// 嵌套 shell / 等价破坏力启动器（O9，2026-10-11，严审报告 P4）：段首词命中
    /// 即整段过确认门——其参数中的危险命令（`cmd /C del …`、`call del …`、
    /// `powershell -Command "Remove-Item …"` 等）无法靠词表逐词覆盖，整段确认
    /// 是唯一可靠的保守面。含 `robocopy`（`/MIR` 可整目录镜像删除）与 `wmic`
    /// （`process call delete` 等价杀进程）非严格意义的 shell，但同属
    /// 「参数破坏力不进词表」一类。大小写不敏感、走既有扩展名剥离
    /// （`cmd.exe` 自动覆盖）。定位不变式：只扩大确认触发面，不新增拦截。
    const NESTED_SHELL_HEADS: &[&str] = &["cmd", "call", "powershell", "pwsh", "wmic", "robocopy"];

    /// 危险命令判定（纯函数，便于单测）：见 [`DANGEROUS_COMMANDS`] / [`DANGEROUS_SUBCOMMANDS`]。
    ///
    /// 取首个词作为命令名，按 `\` / `/` 去路径、按 `.` 去扩展名后比对；`reg delete`
    /// 一类需看第二个词。单段判定——多段命令由 [`is_dangerous_query`] 切段后
    /// 逐段调用本函数。
    fn is_dangerous_command(query: &str) -> bool {
        let lower = query.trim_start().to_ascii_lowercase();
        let mut words = lower.split_whitespace();
        let Some(head) = words.next() else {
            return false;
        };
        // 去路径（C:\Windows\System32\format.com）→ 去扩展名（format.com → format）
        let name = head.rsplit(['\\', '/']).next().unwrap_or(head);
        let name = name.split('.').next().unwrap_or(name);
        if DANGEROUS_COMMANDS.contains(&name) {
            return true;
        }
        // O9：嵌套 shell 启动器整段过门（见 [`NESTED_SHELL_HEADS`]）。
        if NESTED_SHELL_HEADS.contains(&name) {
            return true;
        }
        let sub = words.next().unwrap_or("");
        DANGEROUS_SUBCOMMANDS
            .iter()
            .any(|(cmd, subcmd)| *cmd == name && *subcmd == sub)
    }

    /// R-10（2026-09-29）：多段命令判定——S-06 确认门此前只查**首段**首词，
    /// `echo hi & rd /s /q C:\…` 首词无害即绕过（粘贴型社工恰恰最常以一行
    /// 多段命令出现）。按 `&`（含 `&&`）/ `|` / 换行切段后**每段**各自过
    /// [`is_dangerous_command`]，任一段命中即拦截。
    ///
    /// 切段刻意字符级（`&&` 会产生空段，空段判定恒 false）——不引入 shell
    /// 解析复杂度；引号内的 `&`/`|` 也会切段，属保守方向（宁可多问一次，
    /// 仍是「降低误触代价的护栏」而非沙箱边界的既定定位）。
    ///
    /// O9（2026-10-11）：含 `^`（cmd 转义符，执行期还原，如 `d^el`）的查询
    /// 整体过确认门——「含转义符」本身即可疑信号；不做全文还原
    /// （cmd 转义语义复杂，误报/漏报面不可控）。
    fn is_dangerous_query(query: &str) -> bool {
        query.contains('^')
            || query
                .split(['&', '|', '\r', '\n'])
                .any(is_dangerous_command)
    }

    pub fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
        match params.id.as_str() {
            "shell.open_terminal" => {
                // start 新窗口中的 cmd（CREATE_NO_WINDOW 隐藏 start 自身控制台）。
                //
                // ⚠️ 必须**显式指定起始目录**：否则 `start` 继承宿主进程的 CWD——
                // 从任意目录启动 dd-run 时终端会落在该目录（真机反馈：宿主在
                // `G:\AI\dd-test` 启动 → 终端也在那），与「双击/开始菜单打开终端」
                // （默认 `%USERPROFILE%`）不一致。统一为**用户主目录**。
                use std::os::windows::process::CommandExt;
                let mut command = Command::new("cmd.exe");
                command.args(["/C", "start", "", "cmd"]);
                if let Some(dir) = home_dir() {
                    command.current_dir(dir);
                }
                let spawned = command.creation_flags(0x0800_0000).spawn();
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
                // S-06（2026-09-23）：危险命令先确认。`system` 扩展的关机/重启/注销有
                // Confirm 门禁，而破坏力相当的 `format` / `del /s` / `rd /s` 此前无任何确认
                // ——防护强度与危险度不匹配。确认后宿主带 `context.confirmed = true` 重发
                // （§8.3 既有机制，零协议改动）。
                // R-10（2026-09-29）：判定改 [`is_dangerous_query`]——多段命令
                // 逐段过门，堵 `&`/`|`/换行绕过。
                let confirmed = params
                    .context
                    .as_ref()
                    .and_then(|c| c.confirmed)
                    .unwrap_or(false);
                if !confirmed && is_dangerous_query(query) {
                    return (
                        CommandResult::Confirm {
                            title: tr("确认执行危险命令？", "Run this dangerous command?")
                                .to_string(),
                            // UI-6/P4（2026-10-10）：明示「不拦截」语义——旧文案
                            // 「该操作可能不可撤销」易让用户高估防护、反而敢粘贴
                            // 不明命令。首行保留将执行的命令内容（确认前必须可见）。
                            description: tr(
                                "将执行：{query}\n\ndd-run 不会拦截或沙箱化任何命令：确认后将以你的用户权限直接执行，请核对命令内容再继续。",
                                "Will run: {query}\n\ndd-run does not block or sandbox commands: on confirm it runs with your user privileges as-is. Review the command before continuing.",
                            )
                            .replace("{query}", query),
                            confirm_label: tr("执行", "Run").to_string(),
                            is_critical: true,
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

    /// 无头执行并捕获输出：读线程**先**排空管道 → 轮询等待（超时 kill）→ join 收集 → 合并 stdout/stderr。
    ///
    /// 工作目录统一为**用户主目录**（与 `shell.open_terminal` 一致）——否则会继承
    /// 宿主进程不确定的 CWD，相对路径命令结果随启动位置漂移。
    ///
    /// 读管道必须**先于等待退出**（P1，2026-10-09）：子进程输出超过 OS 管道缓冲
    /// （约 4–64KB）时，若无人读取，子进程会阻塞在写端、永不退出，轮询只能等到
    /// 超时误杀。因此 spawn 后立即起读线程 `read_to_end` 排空，写端不再反压子
    /// 进程；正常退出后 `join` 读到 EOF 恒有限时完成。
    fn run_capture(program: &str, args: &[&str]) -> Result<String, String> {
        let mut command = Command::new(program);
        command
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = home_dir() {
            command.current_dir(dir);
        }
        let mut child = command.spawn().map_err(|e| format!("spawn 失败：{e}"))?;

        // ① 先取管道、起读线程排空（对端写满缓冲不再反压子进程——P1 死锁根因消除）
        let stdout_reader = child.stdout.take().map(spawn_pipe_reader);
        let stderr_reader = child.stderr.take().map(spawn_pipe_reader);

        // ② 轮询退出，超时则 kill。超时路径**不 join** 读线程（detached）：kill 只
        //    杀直接子进程，孙进程（如 `cmd /C ping …` 的 ping.exe）可能仍持有写端，
        //    join 会等到孙进程退出——把「超时误杀」劣化成「超时挂死」（既有测试
        //    `run_capture_timeouts_long_command` 的 5s 断言即依赖立即返回）。读线程
        //    在全部写端关闭后读到 EOF 自行结束，不泄漏。
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
        let _ = child.wait();

        // ③ 正常退出：join 收集（EOF 已到或即将到达，join 恒有限时完成；与原
        //    `wait_with_output` 的 read-to-EOF 口径一致）
        let stdout_bytes = stdout_reader
            .map(|h| h.join().unwrap_or_default())
            .unwrap_or_default();
        let stderr_bytes = stderr_reader
            .map(|h| h.join().unwrap_or_default())
            .unwrap_or_default();
        let mut combined = decode_output(&stdout_bytes);
        let stderr_text = decode_output(&stderr_bytes);
        if !stderr_text.trim().is_empty() {
            combined.push_str(&stderr_text);
        }
        Ok(combined)
    }

    /// 单管道读线程：`read_to_end` 直到 EOF（写端全部关闭——进程退出或被 kill——
    /// 后自然结束）。读失败按空处理，与原 `wait_with_output` 的宽容口径一致。
    fn spawn_pipe_reader<R: std::io::Read + Send + 'static>(
        mut r: R,
    ) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = r.read_to_end(&mut buf);
            buf
        })
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

        /// 终端起始目录：Windows 应能解析出 `USERPROFILE`（否则退回继承 CWD，
        /// 即真机反馈的「与手动打开不一致」）。
        #[test]
        #[cfg(windows)]
        fn home_dir_resolves_user_profile() {
            assert!(home_dir().is_some(), "Windows 应能解析 USERPROFILE");
        }

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

        /// 无头执行的工作目录 = 用户主目录（与「打开终端」统一，不继承宿主 CWD）。
        #[test]
        #[cfg(windows)]
        fn run_capture_runs_in_home_dir() {
            let out = run_capture("cmd.exe", &["/C", "cd"]).expect("cd 应成功");
            let home = home_dir().expect("应有主目录");
            assert!(
                out.trim()
                    .eq_ignore_ascii_case(home.to_string_lossy().trim()),
                "无头执行应在主目录，实得：{out:?}，期望：{home:?}"
            );
        }

        /// P1 回归锚：输出远超 OS 管道缓冲（~64KB）时不得被超时误杀——修复前
        /// 子进程阻塞在管道写端、永不退出，3s 被 kill 误报「执行超时」。
        #[test]
        fn run_capture_drains_output_larger_than_pipe_buffer() {
            let big = std::env::temp_dir().join("ddrun_run_capture_big.txt");
            std::fs::write(&big, "a".repeat(300 * 1024)).unwrap();
            // ⚠️ cmd 经 `Command::args` 传参走 MSVCRT 转义规则，路径**不能带内嵌引号**
            //（`\"` 会被 cmd 误解，BatBadBut 语义面——2026-10-09 首跑实锤）。
            // 故免引号直传；TEMP 路径含空格的环境跳过（本机与 CI 的 TEMP 均无空格）。
            let path = big.to_string_lossy().to_string();
            if path.contains(' ') {
                let _ = std::fs::remove_file(&big);
                eprintln!("跳过：TEMP 路径含空格，cmd /C 无法免引号传参：{path}");
                return;
            }
            let out = run_capture("cmd.exe", &["/C", &format!("type {path}")]);
            let _ = std::fs::remove_file(&big);
            let out = out.expect("大输出命令不得误判超时");
            assert!(
                out.len() >= 300 * 1024,
                "输出应完整排空，实得 {} 字节",
                out.len()
            );
        }

        /// UI-6/P4 回归锚：危险命令确认弹窗的 description 必须明示「不拦截」
        /// 语义——旧文案「该操作可能不可撤销」易让用户高估防护、反而敢粘贴
        /// 不明命令。进程级语言不能在单测翻转（与 i18n 集成测试同口径，见
        /// i18n.rs tests 尾注），英文文案由 tr 双字面量静态保证 + 真机腿覆盖。
        #[test]
        fn confirm_description_declares_no_blocking() {
            let params = InvokeParams {
                id: "shell.run.query".to_string(),
                sender: dd_protocol::model::Sender::TopLevel,
                context: Some(dd_protocol::messages::InvokeContext {
                    query: Some("format q:".to_string()),
                    selected_item_id: None,
                    form_data: None,
                    confirmed: None,
                }),
            };
            let (result, effects) = handle_invoke(&params);
            assert!(effects.is_empty(), "确认分支不产生 effect");
            match result {
                CommandResult::Confirm {
                    description,
                    is_critical,
                    ..
                } => {
                    assert!(is_critical, "critical 红底语义不变");
                    assert!(
                        description.contains("不会拦截"),
                        "中文文案须明示不拦截：{description}"
                    );
                    assert!(
                        description.contains("用户权限"),
                        "须声明以用户权限直接执行：{description}"
                    );
                    assert!(
                        description.contains("format q:"),
                        "必须显示将执行的命令：{description}"
                    );
                    assert!(
                        !description.contains("{query}"),
                        "占位符应已替换：{description}"
                    );
                }
                _ => panic!("危险命令未确认应返回 Confirm"),
            }
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

        // ── S-06（2026-09-23）：危险命令二次确认的判定 ─────────────────

        /// 危险命令命中（含去路径/扩展名、大小写、`(命令,子命令)` 组合）。
        #[test]
        fn dangerous_command_detection_hits() {
            for bad in [
                "format D: /q",
                "FORMAT C:",
                r"C:\Windows\System32\format.com C:",
                "del /s /q C:\\tmp",
                "rd /s /q D:\\data",
                "rmdir /s /q D:\\data",
                "cipher /w:C:",
                "diskpart",
                "vssadmin delete shadows /all",
                "bcdedit /set testsigning on",
                "shutdown /s /t 0",
                "  Takeown /f C:\\Windows\\System32",
                "reg delete HKLM\\SOFTWARE\\Foo /f",
                "net user administrator newpass",
            ] {
                assert!(is_dangerous_command(bad), "应判危险：{bad}");
            }
        }

        /// 日常命令与**同命令的查询类子命令**不得误拦（否则面板会处处弹窗）。
        #[test]
        fn dangerous_command_detection_does_not_over_block() {
            for ok in [
                "echo hello",
                "dir",
                "git status",
                "ipconfig /all",
                "ping -n 2 127.0.0.1",
                "reg query HKLM\\SOFTWARE",
                "net view",
                "where del",         // del 不是命令名
                "echo format",       // 首个词是 echo
                "deleted_files.bat", // 名字含 del 但命令名不同
                "",
            ] {
                assert!(!is_dangerous_command(ok), "不应判危险：{ok}");
            }
        }

        /// R-10：多段命令逐段判定——首段无害、次段危险的组合必须被确认门拦截。
        /// S-06 原实现只查首段首词，`echo hi & rd /s /q …` 可绕过。
        #[test]
        fn r10_dangerous_seg_amp() {
            assert!(
                is_dangerous_query("echo hi & rd /s /q C:\\Users\\me\\Documents"),
                "单 & 次段危险应拦截"
            );
        }

        #[test]
        fn r10_dangerous_seg_double_amp() {
            assert!(
                is_dangerous_query("echo hi && del /q C:\\tmp\\a.txt"),
                "双 && 次段危险应拦截"
            );
        }

        #[test]
        fn r10_dangerous_seg_pipe() {
            assert!(
                is_dangerous_query("echo x | format q: /fs:ntfs"),
                "管道次段危险应拦截"
            );
        }

        #[test]
        fn r10_dangerous_seg_newline() {
            assert!(
                is_dangerous_query("echo hi\ndel /q C:\\tmp\\a.txt"),
                "换行次段危险应拦截"
            );
        }

        /// R-10 负例：全无害的多段命令不得误报（否则日常组合处处弹窗）。
        #[test]
        fn r10_safe_multiseg_no_confirm() {
            assert!(
                !is_dangerous_query("echo hi & dir & ipconfig | findstr IPv4"),
                "全无害多段不应误拦"
            );
            assert!(!is_dangerous_query("echo a && echo b && echo c"));
            assert!(!is_dangerous_query("dir\r\ndel被拆在词中间\r\nipconfig"));
        }

        /// R-10 负例：普通单段命令照常直接执行（S-06 既有行为零变化）。
        #[test]
        fn r10_safe_single_no_confirm() {
            assert!(!is_dangerous_query("ipconfig /all"));
            assert!(!is_dangerous_query("git status && cargo test"));
        }

        /// O9（2026-10-11，严审报告 P4 + 补两类）：段首词为嵌套 shell 启动器时，
        /// 其参数中的危险命令此前不触发确认——7 条绕过样例逐条断言过确认门。
        #[test]
        fn o9_nested_shell_heads_confirm() {
            for bad in [
                "cmd /C del /s /q C:\\x",                    // P4 补：cmd /C 借壳
                "call del /s /q C:\\x",                      // P4：call 承载
                "powershell -Command \"Remove-Item C:\\x\"", // P4：powershell -Command
                "pwsh -Command \"Remove-Item C:\\x\"",       // 本文补：pwsh
                "wmic process call delete pid=123",          // P4：wmic
                "robocopy C:\\a C:\\b /MIR",                 // P4：/MIR 镜像删除
                "cmd.exe /C rd /s /q C:\\x",                 // 扩展名剥离后命中
            ] {
                assert!(is_dangerous_query(bad), "嵌套 shell 应过确认门：{bad}");
            }
        }

        /// O9：含 `^`（cmd 转义符，`d^el` 执行期还原为 `del`）的查询整体过确认门。
        #[test]
        fn o9_caret_escape_confirm() {
            assert!(is_dangerous_query("d^el /s /q C:\\x"), "^ 转义应过确认门");
        }

        /// O9 负例：既有非危险命令回归不弹确认。
        #[test]
        fn o9_safe_queries_no_confirm() {
            assert!(!is_dangerous_query("echo hello"));
            assert!(!is_dangerous_query("git status"));
            assert!(
                !is_dangerous_query("robust_check.exe --flag"),
                "前缀相近不误伤"
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
    use crate::Effect;

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
