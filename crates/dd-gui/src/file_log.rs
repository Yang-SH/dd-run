//! P2-5.3①（dd-run-改进分析.md §5.3）：日志落盘——双后端（stderr + 文件）组合。
//!
//! 背景：release 产物是 `windows_subsystem = "windows"`（无控制台），旧链路
//! 恒写 stderr → 用户双击启动时日志**既看不见也不留存**（只有 panic.log 落盘）。
//! 本模块把全局日志后端从「单 stderr」升级为 **Tee**：stderr 口径逐字不变
//! （终端排障/重定向场景与接入前一致），文件侧默认落
//! `%APPDATA%\dd-run\logs\dd-run.log`（级别默认 **Info**——Debug 直接落盘涨得
//! 很快，§5.3 口径），滚动保留 5×1 MB。
//!
//! 设计约束：
//! - `log` 全局只允许一个后端 → 组合后端在本模块组装（`log::set_boxed_logger`），
//!   **dd-protocol::logging 零改动**（扩展子进程入口仍用其 `init()` 单 stderr）；
//! - 级别过滤沿用 `DDRUN_LOG` 规格解析（`dd_protocol::logging::Filters`）：
//!   stderr 用原 spec（空 = Debug），文件用 `spec` 非空时同 spec、为空时按
//!   `"info"`——显式设置时双后端跟随（显式意图优先）；
//! - 全部失败降级：目录不可建/文件打不开 → 文件腿静默退场（stderr-only，与
//!   R-05 剪贴板工作线程同口径），写失败静默忽略（与 StderrLogger 同约定）。

use dd_protocol::logging::Filters;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 单文件滚动阈值（1 MiB，§5.3 口径）。
const ROTATE_THRESHOLD_BYTES: u64 = 1024 * 1024;
/// 滚动保留文件数（含当前文件，§5.3「5×1 MB」）。
const ROTATE_KEEP: usize = 5;

/// 文件腿的常驻句柄与定位。
struct FileLogSink {
    file: Mutex<File>,
    path: PathBuf,
    dir: PathBuf,
    base: String,
}

impl FileLogSink {
    /// 打开（或创建）日志文件；失败返回 `None`（降级 stderr-only）。
    fn open(dir: &Path, base: &str) -> Option<Self> {
        std::fs::create_dir_all(dir).ok()?;
        let path = dir.join(base);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()?;
        Some(Self {
            file: Mutex::new(file),
            path,
            dir: dir.to_path_buf(),
            base: base.to_string(),
        })
    }

    /// 追加一行；写入前检查滚动。失败静默（与 stderr 腿同约定）。
    fn write_line(&self, line: &str) {
        let Ok(mut f) = self.file.lock() else { return };
        if self.should_rotate(&f) {
            // 滚动后重开；重开失败则继续写旧句柄（只丢滚动，不丢日志）。
            rotate_logs(&self.dir, &self.base, ROTATE_KEEP);
            if let Ok(rotated) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
            {
                *f = rotated;
            }
        }
        let _ = writeln!(f, "{line}");
    }

    fn should_rotate(&self, f: &File) -> bool {
        f.metadata()
            .map(|m| m.len() >= ROTATE_THRESHOLD_BYTES)
            .unwrap_or(false)
    }
}

/// 滚动日志文件（纯文件系统操作，单测锚点）：`base` → `base.1` → … →
/// `base.{keep-2}`（最旧的直接删除；keep = 保留文件总数含当前）。
/// Windows `rename` 不覆盖既有目标 → 先移除目标位再逐级 shift。
fn rotate_logs(dir: &Path, base: &str, keep: usize) {
    let numbered = |n: usize| dir.join(format!("{base}.{n}"));
    for n in (1..keep.saturating_sub(1)).rev() {
        let _ = std::fs::remove_file(numbered(n + 1));
        let _ = std::fs::rename(numbered(n), numbered(n + 1));
    }
    let _ = std::fs::remove_file(numbered(1));
    let _ = std::fs::rename(dir.join(base), numbered(1));
}

/// 双后端组合（stderr 口径逐字不变 + 文件落盘）。
struct TeeLogger {
    stderr: dd_protocol::logging::StderrLogger,
    file: Option<FileLogSink>,
    /// 文件腿过滤器（默认 Info；`DDRUN_LOG` 显式设置时跟随）。
    file_filters: Filters,
}

impl TeeLogger {
    fn file_enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        self.file.is_some()
            && self
                .file_filters
                .allows(metadata.target(), metadata.level())
    }
}

impl log::Log for TeeLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        // 取两腿较宽者放行（max_level 已按宽者设定），过滤交给两腿各自判定。
        self.stderr.enabled(metadata) || self.file_enabled(metadata)
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.stderr.enabled(record.metadata()) {
            self.stderr.log(record);
        }
        if self.file_enabled(record.metadata()) {
            let line = format!(
                "{} {} {}: {}",
                crate::crashlog::local_timestamp_rfc3339(),
                record.level(),
                record.target(),
                record.args()
            );
            if let Some(sink) = &self.file {
                sink.write_line(&line);
            }
        }
    }

    fn flush(&self) {
        if let Some(sink) = &self.file {
            if let Ok(mut f) = sink.file.lock() {
                let _ = f.flush();
            }
        }
    }
}

/// 装配全局日志（main 入口专用；扩展子进程仍走 `dd_protocol::logging::init`）。
///
/// 级别上界 = 两腿较宽者（stderr spec 默认 Debug；文件默认 Info）。
/// `max_level` 先设（与 `dd_protocol::logging::install` 同顺序——即使后端安装
/// 失败，级别上界也已生效）。
pub fn init() {
    let spec = std::env::var(dd_protocol::logging::ENV_VAR).unwrap_or_default();
    let stderr_filters = Filters::parse(&spec);
    // 文件腿：显式 spec 双后端跟随；空 spec → Info（§5.3：Debug 落盘涨得快）。
    let file_filters = if spec.is_empty() {
        Filters::parse("info")
    } else {
        Filters::parse(&spec)
    };
    let file = dd_host::manifest::logs_dir().and_then(|dir| FileLogSink::open(&dir, "dd-run.log"));
    if file.is_none() {
        log::warn!("[dd-gui] 日志目录不可用，文件落盘降级关闭（仅 stderr）");
    }
    let max = stderr_filters
        .default_level()
        .max(stderr_filters.max_target_level())
        .max(file_filters.default_level())
        .max(file_filters.max_target_level());
    let _ = log::set_boxed_logger(Box::new(TeeLogger {
        stderr: dd_protocol::logging::StderrLogger::new(stderr_filters),
        file,
        file_filters,
    }));
    log::set_max_level(max);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 滚动语义：current → .1 → .2 → …（最旧删除）；keep=5 时最多 5 个文件。
    #[test]
    fn rotate_shifts_and_drops_oldest() {
        let dir = std::env::temp_dir().join(format!(
            "dd-gui-filelog-rotate-{}-{}",
            std::process::id(),
            std::time::Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("dd-run.log"), "current").unwrap();
        for n in 1..=4 {
            std::fs::write(dir.join(format!("dd-run.log.{n}")), format!("old{n}")).unwrap();
        }

        rotate_logs(&dir, "dd-run.log", 5);

        assert_eq!(
            std::fs::read_to_string(dir.join("dd-run.log.1")).unwrap(),
            "current"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("dd-run.log.2")).unwrap(),
            "old1"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("dd-run.log.4")).unwrap(),
            "old3"
        );
        assert!(!dir.join("dd-run.log.5").exists(), "keep=5 → 编号止于 4");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 空目录滚动不 panic（首次写满前 rotate 只把 current 挪位）。
    #[test]
    fn rotate_on_empty_dir_is_safe() {
        let dir = std::env::temp_dir().join(format!(
            "dd-gui-filelog-empty-{}-{}",
            std::process::id(),
            std::time::Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        rotate_logs(&dir, "dd-run.log", 5);
        assert!(!dir.join("dd-run.log").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 超限触发滚动：写满阈值后下一行落到新文件，旧内容在 .1。
    #[test]
    fn write_rotates_at_threshold() {
        let dir = std::env::temp_dir().join(format!(
            "dd-gui-filelog-thresh-{}-{}",
            std::process::id(),
            std::time::Instant::now().elapsed().as_nanos()
        ));
        let sink = FileLogSink::open(&dir, "dd-run.log").expect("可写目录");
        let big = "x".repeat(64);
        for _ in 0..(ROTATE_THRESHOLD_BYTES / 64 + 10) {
            sink.write_line(&big);
        }
        assert!(
            dir.join("dd-run.log.1").exists(),
            "超限后应产生 .1 滚动文件"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
