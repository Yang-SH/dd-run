//! 内嵌扩展的**物化**（单文件 `dd-run.exe` 自包含分发）。
//!
//! 背景：M9 起内置扩展改为 **in-process**（宿主进程内直接调 `dd_ext::serve_line`，
//! 见 `docs/m9-inprocess-builtins.md`），**不再 spawn 子进程、不再内嵌内置 exe**，
//! 故本模块当前恒为「空物化」路径——`EMBEDDED` 为空 → `materialize()` 返回 `None`，
//! 宿主回退到「exe 同目录发现」/sidecar 路径。
//!
//! 保留本模块作为**将来内嵌 sidecar**（如文件搜索 `dd-ext-search`）的扩展点：
//! 届时往 `build.rs` 的 `EMBED_EXES` 填条目，`materialize()` 即可把 sidecar 字节
//! 物化到缓存目录；第三方/sidecar 的进程隔离（ADR-1）仍生效。
//!
//! - 内嵌字节来源：`build.rs` 生成的 `EMBEDDED` 表（`include_bytes!`，
//!   见 [`build.rs`](../build.rs)，M9 起恒为空）。
//! - 物化目标：`cache_dir()/embedded/`（`dd-host::manifest::cache_dir`）。
//! - 幂等 + 内容感知刷新：宿主以**内容指纹标记文件**判断是否需要重写——
//!   仅当标记 ≠ 内嵌内容指纹（FNV-1a）时才重写全部内嵌 exe；否则沿用已物化文件，
//!   避免每次冷启动重复写盘。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use dd_host::manifest;

// 内嵌扩展表（build.rs 生成；开发期可为空 → 回退 exe 同目录发现）。
// 表格类型 `&[(&str, &[u8])]`：`(可执行文件名, 字节)`。
include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

/// 物化标记：内嵌内容的 FNV-1a 64bit 指纹（over 文件名 + 字节）。
///
/// 此前按宿主版本号标记——开发期迭代扩展源码但不动版本号时，已物化的旧 exe
/// 不会刷新（真机 2026-09-05 反馈：删掉 demo 项后旧 `dd-ext-system.exe` 仍被
/// spawn，列表残留「UI 验收」行）。改为内容指纹后，内嵌字节任何变化都会触发
/// 重写，开发期无需手动清缓存；运行时在内存中算指纹，无额外磁盘读开销。
fn host_marker() -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = FNV_OFFSET;
    for (fname, bytes) in EMBEDDED {
        for b in fname.as_bytes().iter().chain(bytes.iter()) {
            h ^= u64::from(*b);
            h = h.wrapping_mul(FNV_PRIME);
        }
    }
    format!("dd-run-embedded-{h:016x}")
}

/// 内嵌扩展是否非空（经 package.sh 构建 → 有内容；直接 cargo build → 空）。
pub fn has_embedded() -> bool {
    !EMBEDDED.is_empty()
}

/// 物化内嵌扩展到目标目录（幂等 + 按宿主版本刷新）。
///
/// 返回物化目录路径；无内嵌（开发期）返回 `None`，由调用方回退 exe 同目录发现。
pub fn materialize() -> Option<PathBuf> {
    if EMBEDDED.is_empty() {
        return None;
    }
    let dir = manifest::cache_dir()?.join("embedded");
    match ensure_materialized(&dir) {
        Ok(()) => Some(dir),
        Err(e) => {
            // 物化失败（磁盘满/权限）不致命：宿主回退 exe 同目录发现；
            // 若同目录也无扩展则按既有逻辑给出空扩展提示。
            eprintln!("[dd-gui] 内嵌扩展物化失败（回退 exe 同目录发现）：{e}");
            None
        }
    }
}

/// 写盘逻辑：仅当版本标记缺失/不一致时重写。
fn ensure_materialized(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let marker_path = dir.join(".host-version");
    let up_to_date = fs::read_to_string(&marker_path)
        .map(|s| s.trim() == host_marker())
        .unwrap_or(false);
    if up_to_date {
        return Ok(());
    }
    for (fname, bytes) in EMBEDDED {
        fs::write(dir.join(fname), bytes)?;
    }
    fs::write(&marker_path, host_marker())?;
    Ok(())
}
