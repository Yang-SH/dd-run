//! 宿主逻辑层。
//!
//! 契约来源：
//! - [`docs/manifest-schema.md`](../../docs/manifest-schema.md)（清单扫描，见 [`manifest`]）
//! - [`docs/protocol.md`](../../docs/protocol.md)（子进程通信，见 [`process`]）
//!
//! S-05 扩展信任分级（宿主私有台账，**非**协议/清单契约）见 [`trust`]。

pub mod builtin;
pub mod cache;
/// C3：三边界错误类型（清单/信任/spawn → `LoadError`）。
pub mod error;
pub mod manifest;
pub mod process;
pub mod signing;
pub mod trust;

/// 生产写入器：整文件写入（`std::fs::write`）。
fn atomic_writer(tmp: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(tmp, bytes)
}

/// 原子写盘（R-02）：同目录写 `.tmp` 临时文件后 `rename` 覆盖目标。
///
/// 写盘中途崩溃/断电只丢 `.tmp`，目标文件要么保持完整旧内容、要么已整体
/// 换成完整新内容，不会出现半截文件（trust.json 半截 → `LedgerState::Corrupt`
/// fail-closed，全部已批准扩展回 Pending）。`std::fs::rename` 在 Windows 走
/// `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`，可直接覆盖既有目标。失败时尽力
/// 删除 `.tmp` 残留。刻意不做 fsync：调用方（UI 线程高频落盘）优先低延迟，
/// 崩溃窗口从「整个写入时长」缩到「一次 rename」。
pub(crate) fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    atomic_write_with(path, bytes, atomic_writer)
}

/// [`atomic_write`] 的可注入形态（A4）：`writer` 负责把 `bytes` 落到传入的
/// `.tmp` 路径。测试传恒 `Err` 的桩注入写盘失败——不再依赖 OS 权限语义
/// （root 容器无视 `chmod`、`share_mode(0)` 仅 Windows 可用），注入分支
/// （`share_mode` / `chmod`）已删，root/非 root 行为一致。
pub(crate) fn atomic_write_with(
    path: &std::path::Path,
    bytes: &[u8],
    writer: impl Fn(&std::path::Path, &[u8]) -> std::io::Result<()>,
) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp_name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    match writer(&tmp, bytes) {
        Ok(()) => match std::fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        },
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::atomic_write;
    use std::path::PathBuf;

    /// 与仓库既有测试同口径的临时目录（进程级唯一，测试负责清理）。
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dd-host-r02-{tag}-{}-{}",
            std::process::id(),
            std::time::Instant::now().elapsed().as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// R-02：目标已存在时整体替换，不留旧内容片段。
    #[test]
    fn r02_atomic_write_replaces_existing() {
        let dir = temp_dir("replace");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("data.json");
        std::fs::write(&target, b"{\"old\": 1}").unwrap();
        atomic_write(&target, b"{\"new\": 2}").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"{\"new\": 2}");
        assert!(!dir.join("data.json.tmp").exists(), "无 .tmp 残留");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// R-02：目标目录缺失时先创建再写入（对齐 `save` 原有 create_dir_all 口径）。
    #[test]
    fn r02_atomic_write_missing_dir() {
        let dir = temp_dir("missing");
        let target = dir.join("a/b/data.json");
        atomic_write(&target, b"ok").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"ok");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// R-02：写入失败 → 返回 `Err`、原文件保持完整旧内容、无 `.tmp` 残留。
    ///
    /// 失败注入（A4）：writer 桩恒 `Err`——不再依赖 OS 权限语义（root 容器
    /// 无视 `chmod`、`share_mode(0)` 仅 Windows 可用），root/非 root 一致。
    #[test]
    fn r02_atomic_write_failure_keeps_old() {
        use super::atomic_write_with;

        let dir = temp_dir("failure");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("data.json");
        std::fs::write(&target, b"original").unwrap();

        let seen_tmp = std::cell::RefCell::new(None);
        let err = atomic_write_with(&target, b"new", |tmp, _| {
            *seen_tmp.borrow_mut() = Some(tmp.to_path_buf());
            Err(std::io::Error::other("injected"))
        })
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::Other);
        assert!(
            seen_tmp
                .borrow()
                .as_deref()
                .is_some_and(|p| p.to_string_lossy().ends_with(".tmp")),
            "writer 收到的必须是 .tmp 路径（目标本体不可被触碰）"
        );

        assert_eq!(std::fs::read(&target).unwrap(), b"original");
        assert!(!dir.join("data.json.tmp").exists(), "无 .tmp 残留");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
