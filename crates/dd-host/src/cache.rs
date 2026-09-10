//! 缓存与懒加载（纯逻辑，不依赖 GUI/子进程，可单测）。
//!
//! 对齐 implementation.md §M3 与 design 文档 §6 / §8：
//! - [`FrozenCache`]：扩展 `top_level_commands` 落盘桩，键 = 扩展 id + `version`；
//!   version 变即失效。冷启动时先渲染桩（不拉起进程）满足 **A6** 的"frozen 不拉起"。
//! - [`LruWarmSet`]：保活 N 个扩展，超出则弹最久未用者（返回其 id 供宿主
//!   `close` + 终止进程 + 重新标 stub），满足 **A7**。
//! - [`ColdStartTimer`]：冷启动计时钩子，**A2** 的 200ms 为目标值、需真机实测；
//!   本结构只负责记录与计算耗时，不修改目标值。
//!
//! 本文件为 M3 的"逻辑层先行"部分；与 `dd-gui` 的聚合/进程接线（冷启动渲染桩、
//! 点击桩项复热）属下一轮 P-UI，不在本文件。

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;

use dd_protocol::model::CommandItem;
use serde::{Deserialize, Serialize};

/// 某扩展在某 `version` 下的冻结快照（即 `top_level_commands` 结果）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrozenSnapshot {
    pub ext_id: String,
    pub version: String,
    pub commands: Vec<CommandItem>,
}

/// 冻结桩磁盘缓存（键 = 扩展 id + version）。
///
/// 落盘后冷启动可直接读回命令列表而**不拉起子进程**（A6 的"不拉起"判定在
/// 逻辑层即成立：调用方只需 `load` 而无需 `spawn`）。
#[derive(Clone)]
pub struct FrozenCache {
    dir: PathBuf,
}

impl FrozenCache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// 缓存文件路径：`{dir}/{ext_id}.{version}.json`（id/version 做文件名安全化）。
    fn path(&self, ext_id: &str, version: &str) -> PathBuf {
        self.dir
            .join(format!("{}.{}.json", sanitize(ext_id), sanitize(version)))
    }

    /// 读回快照；文件缺失 / 损坏 / 反序列化失败均返回 `None`（视为无桩）。
    pub fn load(&self, ext_id: &str, version: &str) -> Option<FrozenSnapshot> {
        let bytes = std::fs::read(self.path(ext_id, version)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// 写入快照；目录不存在时自动创建。
    pub fn save(&self, snap: &FrozenSnapshot) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_vec(snap)?;
        std::fs::write(self.path(&snap.ext_id, &snap.version), json)
    }

    /// 若已存在同 id 但**不同 version** 的桩，删除之（返回是否删除）。
    /// 调用方在扩展 version 变化时调用，确保旧桩不污染冷启动。
    ///
    /// **版本判定以快照内 `version` 字段的精确字符串为准**（反序列化后比较），
    /// 不依赖文件名解析——文件名中的 version 经过 `sanitize`（`1.0`→`1_0`），
    /// 若直接拿它与原始 `version`（`1.0`）比较会恒不等，导致"同版本也被误删"
    /// （见下方单测）。解析失败的文件视为失效，一并删除。
    pub fn invalidate_if_version_changed(&self, ext_id: &str, version: &str) -> bool {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return false;
        };
        let prefix = format!("{}.", sanitize(ext_id));
        let mut removed = false;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix) && name.ends_with(".json") {
                let bytes = std::fs::read(entry.path()).unwrap_or_default();
                let parsed: Option<FrozenSnapshot> = serde_json::from_slice(&bytes).ok();
                let stale = match parsed {
                    Some(s) => s.version != version,
                    None => true,
                };
                if stale {
                    let _ = std::fs::remove_file(entry.path());
                    removed = true;
                }
            }
        }
        removed
    }

    /// 删除该扩展的**全部版本**桩（返回是否删除过）。
    ///
    /// 用途（M4 P4 宿主 fallback 轮）：设计文档 §6.3 规定"含兜底能力者一律视为
    /// fresh"——这类扩展必须保持活进程才能响应 `fallback_commands`，因此发现其
    /// 具备兜底能力后要把历史桩清掉，避免下次冷启动又读桩（无进程 → 兜底拉不到）。
    pub fn remove(&self, ext_id: &str) -> bool {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return false;
        };
        let prefix = format!("{}.", sanitize(ext_id));
        let mut removed = false;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix)
                && name.ends_with(".json")
                && std::fs::remove_file(entry.path()).is_ok()
            {
                removed = true;
            }
        }
        removed
    }
}

/// 文件名安全化：非字母数字统一替换为 `_`。
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// LRU 保活集合：最多保活 `capacity` 个扩展，超出则弹最久未用者。
///
/// 被弹出的 id 由调用方负责 `close` + 终止进程 + 重新标 stub（**A7**）。
pub struct LruWarmSet {
    capacity: usize,
    /// 队首 = 最近使用，队尾 = 最久未用；每项带**最后触达时刻**
    /// （供 [`Self::idle_victims`] 做空闲超时回收判定）。
    order: VecDeque<(String, Instant)>,
}

impl LruWarmSet {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity >= 1, "LRU 容量至少为 1");
        Self {
            capacity,
            order: VecDeque::new(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn contains(&self, ext_id: &str) -> bool {
        self.order.iter().any(|(id, _)| id == ext_id)
    }

    /// 触达（访问/保活）某扩展：已存在则移到队首；不存在则入队首。
    /// 若超出容量，弹出队尾（最久未用）并返回其 id，供调用方释放并标 stub。
    /// 返回 `None` 表示未触发驱逐。
    pub fn access(&mut self, ext_id: &str) -> Option<String> {
        self.access_at(ext_id, Instant::now())
    }

    /// [`Self::access`] 的显式时刻版本（单测注入用，避免依赖真实时钟）。
    pub fn access_at(&mut self, ext_id: &str, now: Instant) -> Option<String> {
        if let Some(pos) = self.order.iter().position(|(id, _)| id == ext_id) {
            self.order.remove(pos);
        }
        self.order.push_front((ext_id.to_string(), now));
        if self.order.len() > self.capacity {
            return self.order.pop_back().map(|(id, _)| id);
        }
        None
    }

    /// 主动移除（如扩展崩溃退出后从保活集剔除）。
    pub fn remove(&mut self, ext_id: &str) {
        if let Some(pos) = self.order.iter().position(|(id, _)| id == ext_id) {
            self.order.remove(pos);
        }
    }

    /// 该扩展的最后触达时刻（不在集内 → `None`）。
    pub fn last_access(&self, ext_id: &str) -> Option<Instant> {
        self.order
            .iter()
            .find(|(id, _)| id == ext_id)
            .map(|(_, t)| *t)
    }

    /// 空闲超 `ttl` 的扩展 id（**只读**，不改动集合；调用方负责驱逐）。
    /// 判据 =「最后触达时刻」距 `now` ≥ `ttl`；返回顺序 = 队尾（最久未用）优先。
    ///
    /// 用途：warm 进程**空闲超时回收**——`capacity` 大于扩展总数时 LRU 永不触发
    /// 驱逐，保活集行为上"只增不减"（宿主侧见 `dd-gui` 的 `warm_idle_reclaim`）。
    pub fn idle_victims_at(&self, now: Instant, ttl: Duration) -> Vec<String> {
        self.order
            .iter()
            .rev()
            .filter(|(_, last)| now.saturating_duration_since(*last) >= ttl)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// [`Self::idle_victims_at`] 的当前时刻便捷版。
    pub fn idle_victims(&self, ttl: Duration) -> Vec<String> {
        self.idle_victims_at(Instant::now(), ttl)
    }
}

/// 冷启动计时钩子（A2 实测用）。
///
/// 仅记录两个时点并计算耗时；不修改 200ms 目标值——实测不达标时做法是
/// 记录实测值与瓶颈并据此决策，而非下调目标（见 implementation.md R2）。
pub struct ColdStartTimer {
    spawn_start: Option<Instant>,
    first_interactive: Option<Instant>,
}

impl ColdStartTimer {
    pub fn new() -> Self {
        Self {
            spawn_start: None,
            first_interactive: None,
        }
    }

    /// 标记"开始拉起扩展/渲染桩"的时刻（冷启动起点）。
    pub fn mark_spawn_start(&mut self) {
        self.spawn_start = Some(Instant::now());
    }

    /// 标记"首屏可交互"的时刻（仅在首次有效，避免重复覆盖）。
    pub fn mark_first_interactive(&mut self) {
        if self.first_interactive.is_none() {
            self.first_interactive = Some(Instant::now());
        }
    }

    /// 冷启动耗时（毫秒）；两个时点都标记后才可得。
    pub fn duration_ms(&self) -> Option<u64> {
        match (self.spawn_start, self.first_interactive) {
            (Some(s), Some(f)) => Some(f.duration_since(s).as_millis() as u64),
            _ => None,
        }
    }
}

impl Default for ColdStartTimer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_protocol::model::CommandItem;

    fn snap(ext_id: &str, version: &str, n: usize) -> FrozenSnapshot {
        FrozenSnapshot {
            ext_id: ext_id.to_string(),
            version: version.to_string(),
            commands: (0..n)
                .map(|i| CommandItem {
                    id: format!("{ext_id}.{i}"),
                    title: format!("Cmd {i}"),
                    subtitle: None,
                    icon: None,
                    section: None,
                    tags: None,
                    details: None,
                    text_to_suggest: None,
                    more_commands: None,
                    command: dd_protocol::model::CommandRef::Invoke,
                })
                .collect(),
        }
    }

    #[test]
    fn frozen_save_load_roundtrip() {
        let dir = std::env::temp_dir().join("dd-run-cache-test");
        let _ = std::fs::remove_dir_all(&dir);
        let cache = FrozenCache::new(&dir);
        let s = snap("sample", "1.0", 2);
        cache.save(&s).expect("save 应成功");
        let loaded = cache.load("sample", "1.0").expect("load 应成功");
        assert_eq!(loaded, s, "往返一致（A6：落盘后可无进程取回）");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn frozen_missing_or_corrupt_is_none() {
        let dir = std::env::temp_dir().join("dd-run-cache-test2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cache = FrozenCache::new(&dir);
        assert_eq!(cache.load("absent", "1.0"), None, "无桩返回 None");
        // 写坏文件
        std::fs::write(cache.path("bad", "1.0"), b"{not json").unwrap();
        assert_eq!(cache.load("bad", "1.0"), None, "损坏文件返回 None");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn frozen_invalidates_on_version_change() {
        let dir = std::env::temp_dir().join("dd-run-cache-test3");
        let _ = std::fs::remove_dir_all(&dir);
        let cache = FrozenCache::new(&dir);
        cache.save(&snap("ext.a", "1.0", 1)).unwrap();
        assert!(
            cache.invalidate_if_version_changed("ext.a", "2.0"),
            "version 变应删除旧桩"
        );
        assert_eq!(cache.load("ext.a", "1.0"), None, "旧版本桩已失效");
        // 同版本不应删除
        cache.save(&snap("ext.a", "2.0", 1)).unwrap();
        assert!(
            !cache.invalidate_if_version_changed("ext.a", "2.0"),
            "同版本不应删除"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn frozen_remove_deletes_all_versions() {
        // M4：含兜底能力者清除全部历史桩（设计文档 §6.3 → fresh）
        let dir = std::env::temp_dir().join("dd-run-cache-test4");
        let _ = std::fs::remove_dir_all(&dir);
        let cache = FrozenCache::new(&dir);
        cache.save(&snap("ext.a", "1.0", 1)).unwrap();
        cache.save(&snap("ext.a", "2.0", 1)).unwrap();
        cache.save(&snap("other", "1.0", 1)).unwrap();

        assert!(cache.remove("ext.a"), "应删除 ext.a 的历史桩");
        assert_eq!(cache.load("ext.a", "1.0"), None);
        assert_eq!(cache.load("ext.a", "2.0"), None);
        // 其他扩展不受影响；再次 remove 无可删 → false
        assert!(cache.load("other", "1.0").is_some(), "无关扩展桩保留");
        assert!(!cache.remove("ext.a"), "无桩可删时返回 false");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lru_evicts_least_recently_used_when_over_capacity() {
        // A7：容量 2，超出释放最久未用并重新标 stub
        let mut lru = LruWarmSet::new(2);
        assert_eq!(lru.access("a"), None);
        assert_eq!(lru.access("b"), None);
        // 第三次访问触发驱逐最久未用者 "a"
        assert_eq!(lru.access("c"), Some("a".to_string()), "a 被驱逐");
        assert!(!lru.contains("a"));
        // 再访问已不在集内的 "a"：驱逐当前最久未用者 "b"
        assert_eq!(lru.access("a"), Some("b".to_string()), "b 被驱逐");
        assert!(lru.contains("a"));
        assert!(lru.contains("c"));
    }

    #[test]
    fn lru_touch_keeps_recently_used() {
        let mut lru = LruWarmSet::new(2);
        lru.access("a");
        lru.access("b");
        // 重新触达 a，使其成为最近使用；再访问 c 时应驱逐 b 而非 a
        lru.access("a");
        assert_eq!(lru.access("c"), Some("b".to_string()));
        assert!(lru.contains("a"));
        assert!(!lru.contains("b"));
    }

    #[test]
    fn lru_remove_drops_from_set() {
        let mut lru = LruWarmSet::new(3);
        lru.access("a");
        lru.access("b");
        lru.remove("a");
        assert!(!lru.contains("a"));
        assert_eq!(lru.len(), 1);
    }

    /// C 批次（空闲回收）：触达刷新「最后触达时刻」——刚触达者不判空闲。
    #[test]
    fn lru_idle_victims_ignores_recently_touched() {
        let t0 = Instant::now();
        let mut lru = LruWarmSet::new(4);
        lru.access_at("a", t0);
        lru.access_at("b", t0);
        // a 在 t0+200s 被重新触达 → 只有 b 超时（ttl 120s）
        lru.access_at("a", t0 + Duration::from_secs(200));
        assert_eq!(
            lru.idle_victims_at(t0 + Duration::from_secs(200), Duration::from_secs(120)),
            vec!["b".to_string()],
            "仅久未触达者入选；刚触达的 a 不算空闲"
        );
    }

    /// C 批次（空闲回收）：边界——恰好等于 ttl 即视为空闲；不足则不动。
    #[test]
    fn lru_idle_victims_at_ttl_boundary() {
        let t0 = Instant::now();
        let mut lru = LruWarmSet::new(4);
        lru.access_at("a", t0);
        let ttl = Duration::from_secs(120);
        assert!(
            lru.idle_victims_at(t0 + Duration::from_secs(119), ttl)
                .is_empty(),
            "差 1s 未达阈值 → 不回收"
        );
        assert_eq!(
            lru.idle_victims_at(t0 + ttl, ttl),
            vec!["a".to_string()],
            "恰好等于 ttl → 回收（≥ 判据）"
        );
    }

    /// C 批次：返回顺序 = 队尾（最久未用）优先；`last_access` 与集内状态一致；
    /// `remove` 后不再参与空闲判定。
    #[test]
    fn lru_idle_victims_order_and_last_access() {
        let t0 = Instant::now();
        let mut lru = LruWarmSet::new(4);
        lru.access_at("a", t0);
        lru.access_at("b", t0 + Duration::from_secs(10));
        lru.access_at("c", t0 + Duration::from_secs(20));
        let now = t0 + Duration::from_secs(300);
        assert_eq!(
            lru.idle_victims_at(now, Duration::from_secs(60)),
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
            "队尾（最久未用）优先返回"
        );
        assert_eq!(lru.last_access("a"), Some(t0));
        assert_eq!(lru.last_access("nope"), None);
        lru.remove("b");
        assert!(
            lru.idle_victims_at(now, Duration::from_secs(60))
                .iter()
                .all(|id| id != "b"),
            "已移除项不再参与空闲判定"
        );
    }

    #[test]
    fn cold_start_timer_computes_duration() {
        let mut t = ColdStartTimer::new();
        assert_eq!(t.duration_ms(), None, "未标记时无耗时");
        t.mark_spawn_start();
        std::thread::sleep(std::time::Duration::from_millis(2));
        t.mark_first_interactive();
        let d = t.duration_ms().expect("两点标记后有耗时");
        assert!(d >= 2, "耗时应 >= 实际经过");
        // 二次标记不应覆盖首次
        std::thread::sleep(std::time::Duration::from_millis(2));
        t.mark_first_interactive();
        assert_eq!(t.duration_ms(), Some(d), "首次可交互时刻不被覆盖");
    }
}
