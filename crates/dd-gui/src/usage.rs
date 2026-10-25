//! §5.1（dd-run-改进分析.md）：使用频次（frecency）——启动器第一体验排序依据。
//!
//! 数据形态（分析文档口径）：`usage.json` 存 `{id: {count, last_ts}}`；
//! - **空查询**：可见列表按 `count` 降序稳定排序（未使用项保持聚合原序）；
//! - **有查询**：模糊同分时以 `count` 降序 tiebreak（得分仍恒优先）。
//! 排序判据只用 `count`（`last_ts` 留作后续 recency 衰减的观测数据，暂不参与）。
//!
//! 记录点 = [`crate::app::invoke` 的 `confirm_selected`]（Root/嵌套页、键盘/鼠标
//! 共用的激活中枢）；每次记录即原子落盘（激活是低频人工动作，写放大可忽略）。
//!
//! 键 = `扩展 id \u{1f} 命令 id`（US 单元分隔符不出现于协议 id；与
//! `ListState::select_by_identity` 的 `(ext_id, id)` 身份对齐）。文件损坏 →
//! 空表 + warn（R-05 同口径：本地统计缺失可容忍，不阻塞面板）。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

/// 单条使用记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageEntry {
    /// 累计激活次数。
    pub count: u32,
    /// 最近一次激活的 Unix 秒（仅观测记录，暂不参与排序）。
    pub last_ts: u64,
}

/// 使用频次表（进程内权威副本；`PanelState` 只持有不可变快照 [`Usage::scores`]）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Usage {
    entries: BTreeMap<String, UsageEntry>,
}

/// 注入 [`crate::state::PanelState`] 的排序快照（键 → count）。
/// `Arc` 使「应用侧更新 + 面板侧只读」零拷贝共享。
pub type UsageScores = Arc<BTreeMap<String, u32>>;

/// 使用记录键（`ext_id \u{1f} item_id`）。
pub fn usage_key(ext_id: &str, item_id: &str) -> String {
    format!("{ext_id}\u{1f}{item_id}")
}

/// `usage.json` 路径 = 数据根目录下、与 `config.json` 同目录的兄弟文件。
fn usage_file() -> Option<PathBuf> {
    dd_host::manifest::config_file().map(|p| p.with_file_name("usage.json"))
}

#[derive(Serialize, Deserialize)]
struct UsageFile {
    version: u32,
    entries: BTreeMap<String, UsageEntry>,
}

impl Usage {
    /// 从 `usage.json` 读；文件缺失 / 读盘失败 / 解析失败 → 空表（失败仅 warn）。
    pub fn load() -> Self {
        let Some(path) = usage_file() else {
            return Self::default();
        };
        Self::load_from(&path)
    }

    fn load_from(path: &std::path::Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<UsageFile>(&bytes) {
                Ok(file) if file.version == 1 => Self {
                    entries: file.entries,
                },
                Ok(file) => {
                    log::warn!(
                        "[dd-gui] usage.json 版本 {} 不识别（当前 1），忽略使用数据",
                        file.version
                    );
                    Self::default()
                }
                Err(e) => {
                    log::warn!("[dd-gui] usage.json 解析失败，忽略使用数据：{e}");
                    Self::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                log::warn!("[dd-gui] usage.json 读取失败，忽略使用数据：{e}");
                Self::default()
            }
        }
    }

    /// 记录一次激活（count +1、last_ts 刷新）。
    pub fn record(&mut self, ext_id: &str, item_id: &str, now: u64) {
        let entry = self
            .entries
            .entry(usage_key(ext_id, item_id))
            .or_insert(UsageEntry {
                count: 0,
                last_ts: now,
            });
        entry.count = entry.count.saturating_add(1);
        entry.last_ts = now;
    }

    /// 该项的使用计数（未记录 = 0）。
    pub fn count(&self, ext_id: &str, item_id: &str) -> u32 {
        self.entries
            .get(&usage_key(ext_id, item_id))
            .map(|e| e.count)
            .unwrap_or(0)
    }

    /// 不可变排序快照（注入 `PanelState::set_usage`）。
    pub fn scores(&self) -> UsageScores {
        Arc::new(
            self.entries
                .iter()
                .map(|(k, e)| (k.clone(), e.count))
                .collect(),
        )
    }

    /// 原子落盘（复用 settings 的 `atomic_write`，R-02 同口径）；best-effort。
    pub fn save(&self) {
        let Some(path) = usage_file() else {
            return;
        };
        let file = UsageFile {
            version: 1,
            entries: self.entries.clone(),
        };
        match serde_json::to_vec(&file) {
            Ok(bytes) => {
                if let Err(e) = crate::settings::atomic_write(&path, &bytes) {
                    log::warn!("[dd-gui] usage.json 写盘失败（忽略）：{e}");
                }
            }
            Err(e) => log::warn!("[dd-gui] usage.json 序列化失败（忽略）：{e}"),
        }
    }

    /// 清空（设置页「重置使用数据」；调用方负责 save + 快照同步）。
    pub fn reset(&mut self) {
        self.entries.clear();
    }

    /// 是否无任何记录（测试/设置页空态判断）。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_increments_and_updates_ts() {
        let mut u = Usage::default();
        u.record("ext.a", "cmd1", 100);
        u.record("ext.a", "cmd1", 200);
        u.record("ext.a", "cmd2", 300);
        assert_eq!(u.count("ext.a", "cmd1"), 2);
        assert_eq!(u.count("ext.a", "cmd2"), 1);
        assert_eq!(u.count("ext.b", "cmd1"), 0);
        assert_eq!(u.entries[&usage_key("ext.a", "cmd1")].last_ts, 200);
    }

    #[test]
    fn json_roundtrip_and_corrupt_defaults() {
        let mut u = Usage::default();
        u.record("ext.a", "cmd1", 42);
        let file = UsageFile {
            version: 1,
            entries: u.entries.clone(),
        };
        let bytes = serde_json::to_vec(&file).unwrap();
        let parsed: UsageFile = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed.entries, u.entries);

        let dir = std::env::temp_dir().join("dd-run-usage-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("corrupt.json");
        std::fs::write(&path, b"{not json").unwrap();
        assert!(Usage::load_from(&path).is_empty(), "损坏文件 → 空表");

        let path2 = dir.join("future_version.json");
        std::fs::write(
            &path2,
            br#"{"version":99,"entries":{"x":{"count":1,"last_ts":0}}}"#,
        )
        .unwrap();
        assert!(
            Usage::load_from(&path2).is_empty(),
            "更高版本 → 空表 + warn"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn key_uses_unit_separator() {
        assert_eq!(usage_key("ext.a", "cmd1"), "ext.a\u{1f}cmd1");
        // 键单射前提 = ext_id / item_id 不含 \u{1f}（协议 id 为 token 文法，
        // 控制字符不出现；见模块文档）。混淆注入攻击面不存在：键只用于本机排序。
    }
}
