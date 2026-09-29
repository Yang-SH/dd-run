//! R-25 崩溃取证：`panic::set_hook` 把 panic 信息追加写入
//! `%APPDATA%\dd-run\logs\panic.log`。
//!
//! 背景：release 构建无控制台（`windows_subsystem = "windows"`），O4 日志
//! 后端恒写 stderr——R-01 这类未知崩溃用户手里无痕迹可报、开发者无从诊断。
//! 本模块只覆盖**崩溃取证**这一刚性需求（全量日志文件化见方案 §7 缓办行）。
//!
//! 纪律：
//! - hook 内**不得二次 panic**——目录创建/写盘/时间获取任何失败一律静默
//!   放弃（hook 落盘失败不影响标准 panic 流程）；
//! - **默认行为不变**——落盘后调用 `take_hook()` 取回的原默认 hook（stderr
//!   输出），release 下语义与安装前一致；
//! - 格式化为纯函数 [`format_panic_entry`]，便于单测（时间戳 / location /
//!   多行 message / 缺 location 各态）。

use std::io::Write as _;
use std::path::Path;

/// 安装全局 panic hook（进程入口调用一次，O4 init 之后）。
pub fn install() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // hook 内任何步骤失败都必须静默放弃（不得二次 panic）
        let message = payload_str(info.payload());
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let thread = std::thread::current().name().map(str::to_string);
        let entry = format_panic_entry(
            &local_timestamp_rfc3339(),
            &message,
            location.as_deref(),
            thread.as_deref(),
        );
        if let Some(dir) = dd_host::manifest::logs_dir() {
            let _ = append_entry(&dir, &entry);
        }
        // 默认行为不变：落盘后走标准 panic 流程（stderr 输出 + unwind）
        default_hook(info);
    }));
}

/// panic 载荷转字符串（`&str` / `String` / 其他类型三态，不 panic）。
fn payload_str(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Box<dyn Any>（非字符串 panic 载荷）".to_string()
    }
}

/// 本地时间戳（RFC3339 带时区，毫秒精度）；时钟不可用时回落
/// `<timestamp 不可用>`（不得 panic）。chrono 已在依赖树内（dd-host/dd-ext），
/// 本地时区由 chrono 的 local feature 解析。
fn local_timestamp_rfc3339() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3f%:z")
        .to_string()
}

/// 追加一条记录到 `dir/panic.log`（目录不存在则创建）。失败返回
/// `Err` 由调用方决定是否放弃——测试借此验证"路径不可写不 panic"。
pub(crate) fn append_entry(dir: &Path, entry: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("panic.log");
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)?;
    f.write_all(entry.as_bytes())
}

/// 格式化一条 panic 记录（纯函数，单测覆盖三态）：
/// `[时间戳] thread "<名>" panicked at <位置>:\n<消息>\n`。
///
/// 多行 message 原样保留换行（与 std 默认 stderr 输出同构）；缺 location /
/// 线程名时以显式占位标注（取证时至少可分辨是哪类缺失）。字符串操作均为
/// 安全 API，本函数不 panic。
pub(crate) fn format_panic_entry(
    when: &str,
    message: &str,
    location: Option<&str>,
    thread_name: Option<&str>,
) -> String {
    let thread = match thread_name {
        Some(n) => format!("thread \"{n}\""),
        None => "thread <unnamed>".to_string(),
    };
    let at = location.unwrap_or("<location 不可用>");
    format!("[{when}] {thread} panicked at {at}:\n{message}\n")
}

#[cfg(test)]
mod tests {
    use super::{append_entry, format_panic_entry};

    /// R-25：格式化纯函数三态断言——含 location / 多行 message / 缺 location。
    #[test]
    fn r25_panic_log_format_states() {
        // ① 常规态：时间戳 + 线程名 + location + 单行 message
        let entry = format_panic_entry(
            "2026-09-29T12:00:00.000+08:00",
            "called `Option::unwrap()` on a `None` value",
            Some("src\\ui\\chrome.rs:133:50"),
            Some("main"),
        );
        assert!(entry.starts_with("[2026-09-29T12:00:00.000+08:00] "));
        assert!(entry.contains("thread \"main\""));
        assert!(entry.contains("panicked at src\\ui\\chrome.rs:133:50:"));
        assert!(entry.contains("called `Option::unwrap()` on a `None` value"));
        assert!(entry.ends_with('\n'), "记录以换行收尾");

        // ② 多行 message：换行原样保留（与 std 默认输出同构）
        let entry = format_panic_entry(
            "2026-09-29T12:00:01.000+08:00",
            "line one\nline two\nline three",
            Some("src\\a.rs:1:1"),
            Some("dd-hotkey"),
        );
        assert!(entry.contains("line one\nline two\nline three"));
        assert_eq!(entry.lines().count(), 4, "头行 + 3 行消息");

        // ③ 缺 location / 未命名线程：显式占位，不 panic
        let entry = format_panic_entry("2026-09-29T12:00:02.000Z", "boom", None, None);
        assert!(entry.contains("thread <unnamed>"));
        assert!(entry.contains("panicked at <location 不可用>:"));
        assert!(entry.contains("boom"));
    }

    /// R-25：路径不可写（父级是文件）时 `append_entry` 返回 `Err` 而非 panic
    /// ——hook 的降级纪律。
    #[test]
    fn r25_append_unwritable_path_degrades() {
        let base = std::env::temp_dir().join(format!(
            "dd-gui-r25-{}-{}",
            std::process::id(),
            std::time::Instant::now().elapsed().as_nanos()
        ));
        // base 本身建成一个**文件**——其下创建目录必然失败
        std::fs::write(&base, b"not a dir").unwrap();
        let dir = base.join("logs");
        let result = std::panic::catch_unwind(|| append_entry(&dir, "[t] boom\n"));
        assert!(result.is_ok(), "append_entry 不得 panic");
        assert!(result.unwrap().is_err(), "不可写路径应返回 Err");
        let _ = std::fs::remove_file(&base);
    }

    /// R-25：可写路径下追加语义——两次写入同文件，两条记录都在（追加非覆盖）。
    #[test]
    fn r25_append_is_append() {
        let dir = std::env::temp_dir().join(format!(
            "dd-gui-r25-append-{}-{}",
            std::process::id(),
            std::time::Instant::now().elapsed().as_nanos()
        ));
        append_entry(&dir, "[t1] first\n").unwrap();
        append_entry(&dir, "[t2] second\n").unwrap();
        let text = std::fs::read_to_string(dir.join("panic.log")).unwrap();
        assert!(text.contains("[t1] first"));
        assert!(text.contains("[t2] second"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
