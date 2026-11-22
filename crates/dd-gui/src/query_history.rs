//! N20（2026-11-22）：搜索历史——`history.json`（数据根目录、与 config.json
//! 同目录的兄弟文件；usage.json 先例）。**独立文件 = 个人态天然不随设置迁移**
//! （N5 导出/备份只走 config 序列化，本文件不在其链路）——隐私三键承载：
//! 记录开关（`Settings.record_search_history`，默认开）/ 设置页「清空」按钮 /
//! 文件隔离。写入走 R-02 原子写（文件小、频次低，即时落盘）。
//!
//! 恒不失败姿态：缺失/损坏/路径不可得 → 空表（历史非关键数据，静默回落）；
//! 写失败仅日志。

use dd_host::manifest::config_file;
use std::path::{Path, PathBuf};

/// 历史上限（方案 §4.15：去重置顶、上限 50）。
pub const HISTORY_CAP: usize = 50;

/// 文件形态版本（usage.json 同惯例；字段演进用）。
const HISTORY_VERSION: u32 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
struct HistoryFile {
    version: u32,
    /// 最近查询在前（去重置顶后的列表）。
    queries: Vec<String>,
}

fn history_file() -> Option<PathBuf> {
    config_file().map(|p| p.with_file_name("history.json"))
}

/// 读取（从指定路径；测试注入用）。缺失/损坏/非法 → 空表。
pub(crate) fn load_from(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str::<HistoryFile>(&text)
        .map(|f| f.queries)
        .unwrap_or_default()
}

/// 原子写盘（从指定路径；测试注入用）。返回成败（调用方记日志）。
pub(crate) fn save_to(path: &Path, queries: &[String]) -> bool {
    let file = HistoryFile {
        version: HISTORY_VERSION,
        queries: queries.to_vec(),
    };
    let Ok(json) = serde_json::to_string(&file) else {
        return false;
    };
    crate::settings::atomic_write(path, json.as_bytes()).is_ok()
}

/// 记录一条查询（纯逻辑：trim、去空、去重置顶、上限截断）。
pub(crate) fn record(queries: &mut Vec<String>, query: &str) {
    let q = query.trim();
    if q.is_empty() {
        return;
    }
    queries.retain(|x| x != q);
    queries.insert(0, q.to_string());
    queries.truncate(HISTORY_CAP);
}

/// 读取全量历史（真实数据根路径）。
pub fn load_history() -> Vec<String> {
    history_file().map(|p| load_from(&p)).unwrap_or_default()
}

/// 原子保存全量历史（真实数据根路径；失败仅日志——历史非关键数据）。
pub fn save_history(queries: &[String]) -> bool {
    match history_file() {
        Some(p) => {
            let ok = save_to(&p, queries);
            if !ok {
                log::warn!("[dd-gui] 搜索历史写入失败（R-02 原子写）");
            }
            ok
        }
        None => {
            log::debug!("[dd-gui] 搜索历史路径不可得，跳过写入");
            false
        }
    }
}

/// 清空：删文件（内存表与回填游标由调用方清）。
pub fn clear_history() {
    if let Some(p) = history_file() {
        let _ = std::fs::remove_file(&p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n20_record_dedups_moves_top_and_caps() {
        let mut q = Vec::new();
        record(&mut q, "  hello ");
        assert_eq!(q, vec!["hello".to_string()], "trim 入表");
        record(&mut q, "world");
        assert_eq!(q, vec!["world", "hello"]);
        record(&mut q, "hello");
        assert_eq!(q, vec!["hello", "world"], "去重置顶");
        record(&mut q, "   ");
        assert_eq!(q.len(), 2, "空/纯空白跳过");
        for i in 0..60 {
            record(&mut q, &format!("q{i}"));
        }
        assert_eq!(q.len(), HISTORY_CAP, "上限 50");
        assert_eq!(q[0], "q59", "最新在前");
    }

    #[test]
    fn n20_save_load_roundtrip_and_corrupt_defaults_empty() {
        let dir = std::env::temp_dir().join(format!("ddrun-n20-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("history.json");
        let queries = vec!["α查询".to_string(), "beta".to_string()];
        assert!(save_to(&p, &queries));
        assert_eq!(load_from(&p), queries, "往返一致");
        std::fs::write(&p, "not json").unwrap();
        assert!(load_from(&p).is_empty(), "损坏 → 空表");
        assert!(
            load_from(&dir.join("missing.json")).is_empty(),
            "缺失 → 空表"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
