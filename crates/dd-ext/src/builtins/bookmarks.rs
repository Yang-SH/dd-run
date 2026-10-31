//! 内置扩展：浏览器书签搜索（N3，2026-10-04；future-features-plan §4.3）。
//!
//! - 数据：**只读**解析 Chromium 系书签 JSON（Chrome / Edge 的
//!   `%LOCALAPPDATA%\<vendor>\User Data\<profile>\Bookmarks`），按文件 mtime
//!   门控增量重建（进程内缓存，未变文件不重解析）；索引条目设上限
//!   （[`MAX_ENTRIES`]，超限截断 + 尾部提示条目）。
//! - 交互：顶层命令 = 扁平化书签条目（文件夹路径进 `subtitle`，拼音索引由
//!   宿主按 title 生成）；invoke → `host/open_url` `https` 档打开（S-03
//!   白名单天然生效）；**零网络请求**（favicon 拉取判不做，spec §4.3）。
//! - 信任面：in-process 运行、不经 spawn、不涉信任台账（随宿主单文件分发）；
//!   协议 / 清单零改动。隐私姿态：纯本地文件读取，与 README 非目标一致。
//! - 平台策略：Windows 优先；macOS / Linux 自 S3 起为**真实现**（同构
//!   Chromium 书签路径 + 同一套 mtime 缓存与解析，见 `sys` 的 not(windows)
//!   分支；数据源 = `vendor_dirs`），托盘 / DWM / Everything 专属面不涉本模块。

#[cfg(windows)]
use crate::Effect;
use crate::{i18n::tr, ExtensionSpec};
use dd_protocol::messages::InvokeParams;
use dd_protocol::methods::METHOD_HOST_OPEN_URL;
use dd_protocol::model::{CommandItem, CommandResult};
#[cfg(windows)]
use dd_protocol::model::{CommandRef, Icon, IconKind};

pub const EXT_ID: &str = "com.ddrun.bookmarks";

/// 索引条目上限（spec §4.3：防异常书签库把内存/首屏拖爆；超限截断 +
/// 尾部提示条目）。
pub const MAX_ENTRIES: usize = 2000;

#[cfg(windows)]
const GLYPH: char = '\u{E718}'; // FavoriteStar

pub fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: EXT_ID,
        display_name: tr("书签", "Bookmarks"),
        description: tr(
            "搜索并打开 Chromium 系浏览器（Chrome / Edge）的本地书签",
            "Search and open local bookmarks from Chromium browsers (Chrome / Edge)",
        ),
        // 书签文件随时可变 → fresh（不落磁盘桩；解析随聚合进行 + mtime 门控）
        frozen: false,
        has_fallback: false,
        capabilities: &[METHOD_HOST_OPEN_URL],
        log_tag: "dd-ext-bookmarks",
        pages: None, // 顶层命令 = 扁平书签条目，无子页（§6.3）
        top_level: sys::top_level_commands,
        fallback: None,
        invoke: sys::handle_invoke,
    }
}

// ────────────────────────────────────────────────────────────────
// 书签 JSON 解析（平台中立，纯函数——单测直接覆盖，不触文件系统）
// ────────────────────────────────────────────────────────────────

/// 一条扁平化书签（`folder` = 浏览器/根/子文件夹路径，进条目 subtitle）。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(any(windows, unix, test))]
pub(crate) struct Bookmark {
    pub title: String,
    pub url: String,
    pub folder: String,
}

/// 解析单份 `Bookmarks` JSON 文本 → 扁平条目（截断到 [`MAX_ENTRIES`]）。
///
/// 损坏文件（非 JSON / 无 `roots` / roots 非对象）→ `None`（调用方跳过该
/// 文件，不影响其余浏览器/配置档）；`url` 型节点缺 `url` 字段跳过；未知
/// `type` 跳过（向前兼容）。
#[cfg(any(windows, unix, test))]
pub(crate) fn parse_bookmarks_json(text: &str, browser: &str) -> Option<Vec<Bookmark>> {
    let value = serde_json::from_str::<serde_json::Value>(text).ok()?;
    let roots = value.get("roots")?.as_object()?;
    let mut out = Vec::new();
    // 已知三个根（bookmark_bar / other / synced）；多余根忽略（向前兼容）。
    // 根节点的 `name` 与 [`root_label`] 语义重复（Chrome 惯例 name 即「书签栏」），
    // 不经 flatten_node 重入——直接以其 `children` 为顶层递归，folder 起点 =
    // `浏览器/根显示名`。
    for key in ["bookmark_bar", "other", "synced"] {
        let Some(node) = roots.get(key) else {
            continue;
        };
        let base = format!("{browser}/{}", root_label(key));
        let Some(children) = node.get("children").and_then(|c| c.as_array()) else {
            continue;
        };
        for child in children {
            flatten_node(child, &base, &mut out);
        }
    }
    Some(out)
}

/// 根键显示名（书签栏 / 其他书签 / 移动设备书签）。
#[cfg(any(windows, unix, test))]
fn root_label(key: &str) -> &'static str {
    match key {
        "bookmark_bar" => tr("书签栏", "Bookmarks bar"),
        "other" => tr("其他书签", "Other bookmarks"),
        "synced" => tr("移动设备书签", "Mobile bookmarks"),
        _ => "",
    }
}

/// 递归扁平化一个书签节点；返回是否已触及截断上限（触顶后停止收集）。
#[cfg(any(windows, unix, test))]
fn flatten_node(node: &serde_json::Value, folder: &str, out: &mut Vec<Bookmark>) -> bool {
    if out.len() >= MAX_ENTRIES {
        return true;
    }
    match node.get("type").and_then(|t| t.as_str()) {
        Some("url") => {
            let title = node
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or_default()
                .trim();
            let Some(url) = node.get("url").and_then(|u| u.as_str()) else {
                return false;
            };
            if !title.is_empty() && !url.is_empty() {
                out.push(Bookmark {
                    title: title.to_string(),
                    url: url.to_string(),
                    folder: folder.to_string(),
                });
            }
            out.len() >= MAX_ENTRIES
        }
        Some("folder") => {
            let name = node.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let child_folder = if name.is_empty() {
                folder.to_string()
            } else {
                format!("{folder}/{name}")
            };
            if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
                for child in children {
                    if flatten_node(child, &child_folder, out) {
                        return true;
                    }
                }
            }
            out.len() >= MAX_ENTRIES
        }
        _ => {
            // 未知 type / 缺 type：跳过节点本身（向前兼容）；但若携带
            // `children` 则按文件夹递归——真实书签文件的根节点结构随版本
            // 演进可能缺 `type`，不能因此整棵丢弃。
            if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
                for child in children {
                    if flatten_node(child, folder, out) {
                        return true;
                    }
                }
            }
            out.len() >= MAX_ENTRIES
        }
    }
}

// ────────────────────────────────────────────────────────────────
// 平台实现（Windows：文件发现 + mtime 门控缓存）
// ────────────────────────────────────────────────────────────────

#[cfg(windows)]
mod sys {
    use super::*;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, RwLock};
    use std::time::SystemTime;

    /// 源文件解析快照：`(路径, mtime, 该文件条目)`——mtime 未变的文件直接
    /// 复用上次解析结果（spec「增量重建」门控）。
    struct CachedFile {
        path: PathBuf,
        mtime: SystemTime,
        entries: Vec<Bookmark>,
    }

    struct Index {
        files: Vec<CachedFile>,
        /// 全量扁平条目（cap 已应用）。
        entries: Vec<Bookmark>,
        truncated: bool,
    }

    static INDEX: RwLock<Option<Arc<Index>>> = RwLock::new(None);

    /// 书签源文件清单：Chrome / Edge 的 `User Data\<profile>\Bookmarks`
    /// （每个含 Bookmarks 文件的子目录视为一个配置档）。
    fn bookmark_files() -> Vec<(PathBuf, &'static str)> {
        let Some(local) = std::env::var_os("LOCALAPPDATA") else {
            return Vec::new();
        };
        let local = PathBuf::from(local);
        let mut out = Vec::new();
        for (vendor_dir, browser) in [
            (local.join(r"Google\Chrome\User Data"), "Chrome"),
            (local.join(r"Microsoft\Edge\User Data"), "Edge"),
        ] {
            let Ok(profiles) = std::fs::read_dir(&vendor_dir) else {
                continue;
            };
            for p in profiles.flatten() {
                let f = p.path().join("Bookmarks");
                if p.path().is_dir() && f.is_file() {
                    out.push((f, browser));
                }
            }
        }
        out
    }

    /// 当前快照（mtime 门控：全部源文件未变 → 直接复用；任一变化 → 增量
    /// 重建，未变文件沿用旧解析）。
    fn snapshot() -> Arc<Index> {
        let sources = bookmark_files();
        // 快路径：缓存命中（文件集合与 mtime 全部一致；同路径浏览器标签恒定
        // ——路径含 vendor 目录，无需单独比较）
        {
            let guard = INDEX.read().unwrap_or_else(|e| e.into_inner());
            if let Some(idx) = guard.as_ref() {
                let fresh = idx.files.len() == sources.len()
                    && idx.files.iter().zip(sources.iter()).all(|(c, (p, _))| {
                        &c.path == p && c.mtime == file_mtime(p).unwrap_or(SystemTime::UNIX_EPOCH)
                    });
                if fresh {
                    return idx.clone();
                }
            }
        }
        // 增量重建：未变文件沿用旧解析
        let old: HashMap<PathBuf, (SystemTime, Vec<Bookmark>)> = INDEX
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|idx| {
                idx.files
                    .iter()
                    .map(|c| (c.path.clone(), (c.mtime, c.entries.clone())))
                    .collect()
            })
            .unwrap_or_default();
        let mut files = Vec::new();
        for (path, browser) in sources {
            let mtime = file_mtime(&path).unwrap_or(SystemTime::UNIX_EPOCH);
            let entries = match old.get(&path) {
                Some((t, entries)) if *t == mtime => entries.clone(),
                _ => match std::fs::read_to_string(&path) {
                    Ok(text) => match parse_bookmarks_json(&text, browser) {
                        Some(entries) => {
                            log::debug!(
                                "[dd-ext-bookmarks] 已解析 {path:?}（{} 条）",
                                entries.len()
                            );
                            entries
                        }
                        None => {
                            log::warn!("[dd-ext-bookmarks] 书签文件损坏，跳过：{path:?}");
                            Vec::new()
                        }
                    },
                    Err(e) => {
                        log::warn!("[dd-ext-bookmarks] 书签文件读取失败，跳过：{path:?}（{e}）");
                        Vec::new()
                    }
                },
            };
            files.push(CachedFile {
                path,
                mtime,
                entries,
            });
        }
        let mut entries: Vec<Bookmark> = Vec::new();
        let mut truncated = false;
        for f in &files {
            for b in &f.entries {
                if entries.len() >= MAX_ENTRIES {
                    truncated = true;
                    break;
                }
                entries.push(b.clone());
            }
            if truncated {
                break;
            }
        }
        let idx = Arc::new(Index {
            files,
            entries,
            truncated,
        });
        *INDEX.write().unwrap_or_else(|e| e.into_inner()) = Some(idx.clone());
        idx
    }

    fn file_mtime(path: &Path) -> Option<SystemTime> {
        std::fs::metadata(path).ok()?.modified().ok()
    }

    pub fn top_level_commands() -> Vec<CommandItem> {
        let idx = snapshot();
        let mut items: Vec<CommandItem> = idx
            .entries
            .iter()
            .enumerate()
            .map(|(i, b)| CommandItem {
                id: format!("bookmarks.open.{i}"),
                title: b.title.clone(),
                subtitle: Some(b.folder.clone()),
                icon: Some(Icon {
                    kind: IconKind::Glyph,
                    value: GLYPH.to_string(),
                }),
                section: Some(tr("书签", "Bookmarks").to_string()),
                tags: None,
                details: None,
                text_to_suggest: None,
                more_commands: None,
                command: CommandRef::Invoke,
            })
            .collect();
        // 尾部截断提示（spec §4.3）：不可打开的提示条目，invoke 只 toast。
        if idx.truncated {
            items.push(CommandItem {
                id: "bookmarks.overflow".to_string(),
                title: tr(
                    "…书签过多，仅索引前 {n} 条",
                    "…Too many bookmarks, only the first {n} indexed",
                )
                .replace("{n}", &MAX_ENTRIES.to_string()),
                subtitle: None,
                icon: Some(Icon {
                    kind: IconKind::Glyph,
                    value: GLYPH.to_string(),
                }),
                section: Some(tr("书签", "Bookmarks").to_string()),
                tags: None,
                details: None,
                text_to_suggest: None,
                more_commands: None,
                command: CommandRef::Invoke,
            });
        }
        items
    }

    pub fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
        if params.id == "bookmarks.overflow" {
            return (
                CommandResult::ShowToast {
                    message: tr(
                        "书签过多，仅索引前 {n} 条（可在浏览器整理书签后重试）",
                        "Too many bookmarks, only the first {n} indexed",
                    )
                    .replace("{n}", &MAX_ENTRIES.to_string()),
                    duration_ms: Some(2_500),
                },
                Vec::new(),
            );
        }
        let Some(seq) = params.id.strip_prefix("bookmarks.open.") else {
            return (
                CommandResult::ShowToast {
                    message: tr("未知书签命令：{id}", "Unknown bookmark command: {id}")
                        .replace("{id}", &params.id),
                    duration_ms: Some(2_500),
                },
                Vec::new(),
            );
        };
        let Ok(i) = seq.parse::<usize>() else {
            return (
                CommandResult::ShowToast {
                    message: tr("未知书签命令：{id}", "Unknown bookmark command: {id}")
                        .replace("{id}", &params.id),
                    duration_ms: Some(2_500),
                },
                Vec::new(),
            );
        };
        let idx = snapshot();
        let Some(b) = idx.entries.get(i) else {
            return (
                CommandResult::ShowToast {
                    message: tr(
                        "书签不存在或列表已变化：{id}",
                        "Bookmark not found or list changed: {id}",
                    )
                    .replace("{id}", &params.id),
                    duration_ms: Some(2_500),
                },
                Vec::new(),
            );
        };
        (
            CommandResult::KeepOpen,
            vec![Effect::HostRequest {
                method: METHOD_HOST_OPEN_URL,
                params: serde_json::json!({ "url": b.url }),
            }],
        )
    }
}

#[cfg(not(windows))]
mod sys {
    //! macOS / Linux 实现（S3，2026-10-31）：与 Windows 分支同构——同一套
    //! mtime 门控缓存 + 共享解析层（`parse_bookmarks_json`），仅**数据源**
    //! 不同（`vendor_dirs`：macOS `~/Library/Application Support/...`、
    //! Linux `~/.config/...`）。缓存机械与命令构建在此为刻意副本：Windows
    //! 分支受「零字节改动」红线约束不参与归并，去重留后续批次。
    use super::*;
    use crate::Effect;
    use dd_protocol::model::{CommandRef, Icon, IconKind};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, RwLock};
    use std::time::SystemTime;

    const GLYPH: char = '\u{E718}'; // FavoriteStar（与 Windows 分支同 glyph）

    /// 候选 vendor 目录（macOS / Linux × Chrome / Edge）。纯函数（home 注入）
    /// ——单测直接覆盖，不触文件系统。
    fn vendor_dirs(home: &Path) -> Vec<(PathBuf, &'static str)> {
        // macOS 与 Linux 的 Chromium 配置根不同；Edge 的目录名两家平台亦不同
        // （macOS 带空格 "Microsoft Edge"，Linux 为 "microsoft-edge"）。
        if cfg!(target_os = "macos") {
            let base = home.join("Library/Application Support");
            vec![
                (base.join("Google/Chrome"), "Chrome"),
                (base.join("Microsoft Edge"), "Edge"),
            ]
        } else {
            let base = home.join(".config");
            vec![
                (base.join("google-chrome"), "Chrome"),
                (base.join("microsoft-edge"), "Edge"),
            ]
        }
    }

    /// 书签源文件清单：每个**含 Bookmarks 文件的子目录**视为一个配置档
    /// （与 Windows 分支同一口径，含 Default / Profile N 等）。
    fn bookmark_files() -> Vec<(PathBuf, &'static str)> {
        let Some(home) = std::env::var_os("HOME") else {
            return Vec::new();
        };
        let home = PathBuf::from(home);
        let mut out = Vec::new();
        for (vendor_dir, browser) in vendor_dirs(&home) {
            collect_profiles(&vendor_dir, browser, &mut out);
        }
        out
    }

    /// 扫描单个 vendor 目录下的配置档（提取自 `bookmark_files`，纯文件系统
    /// 操作——单测可用临时目录覆盖发现口径）。
    fn collect_profiles(
        vendor_dir: &Path,
        browser: &'static str,
        out: &mut Vec<(PathBuf, &'static str)>,
    ) {
        let Ok(profiles) = std::fs::read_dir(vendor_dir) else {
            return;
        };
        for p in profiles.flatten() {
            let f = p.path().join("Bookmarks");
            if p.path().is_dir() && f.is_file() {
                out.push((f, browser));
            }
        }
    }

    /// 源文件解析快照：`(路径, mtime, 该文件条目)`——mtime 未变的文件直接
    /// 复用上次解析结果。
    struct CachedFile {
        path: PathBuf,
        mtime: SystemTime,
        entries: Vec<Bookmark>,
    }

    struct Index {
        files: Vec<CachedFile>,
        /// 全量扁平条目（cap 已应用）。
        entries: Vec<Bookmark>,
        truncated: bool,
    }

    static INDEX: RwLock<Option<Arc<Index>>> = RwLock::new(None);

    fn file_mtime(path: &Path) -> Option<SystemTime> {
        std::fs::metadata(path).ok()?.modified().ok()
    }

    /// 当前快照（mtime 门控：全部源文件未变 → 直接复用；任一变化 → 增量
    /// 重建，未变文件沿用旧解析）。与 Windows 分支同一算法。
    fn snapshot() -> Arc<Index> {
        let sources = bookmark_files();
        {
            let guard = INDEX.read().unwrap_or_else(|e| e.into_inner());
            if let Some(idx) = guard.as_ref() {
                let fresh = idx.files.len() == sources.len()
                    && idx.files.iter().zip(sources.iter()).all(|(c, (p, _))| {
                        &c.path == p && c.mtime == file_mtime(p).unwrap_or(SystemTime::UNIX_EPOCH)
                    });
                if fresh {
                    return idx.clone();
                }
            }
        }
        let old: HashMap<PathBuf, (SystemTime, Vec<Bookmark>)> = INDEX
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|idx| {
                idx.files
                    .iter()
                    .map(|c| (c.path.clone(), (c.mtime, c.entries.clone())))
                    .collect()
            })
            .unwrap_or_default();
        let mut files = Vec::new();
        for (path, browser) in sources {
            let mtime = file_mtime(&path).unwrap_or(SystemTime::UNIX_EPOCH);
            let entries = match old.get(&path) {
                Some((t, entries)) if *t == mtime => entries.clone(),
                _ => match std::fs::read_to_string(&path) {
                    Ok(text) => match parse_bookmarks_json(&text, browser) {
                        Some(entries) => {
                            log::debug!(
                                "[dd-ext-bookmarks] 已解析 {path:?}（{} 条）",
                                entries.len()
                            );
                            entries
                        }
                        None => {
                            log::warn!("[dd-ext-bookmarks] 书签文件损坏，跳过：{path:?}");
                            Vec::new()
                        }
                    },
                    Err(e) => {
                        log::warn!("[dd-ext-bookmarks] 书签文件读取失败，跳过：{path:?}（{e}）");
                        Vec::new()
                    }
                },
            };
            files.push(CachedFile {
                path,
                mtime,
                entries,
            });
        }
        let mut entries: Vec<Bookmark> = Vec::new();
        let mut truncated = false;
        for f in &files {
            for b in &f.entries {
                if entries.len() >= MAX_ENTRIES {
                    truncated = true;
                    break;
                }
                entries.push(b.clone());
            }
            if truncated {
                break;
            }
        }
        let idx = Arc::new(Index {
            files,
            entries,
            truncated,
        });
        *INDEX.write().unwrap_or_else(|e| e.into_inner()) = Some(idx.clone());
        idx
    }

    pub fn top_level_commands() -> Vec<CommandItem> {
        let idx = snapshot();
        let mut items: Vec<CommandItem> = idx
            .entries
            .iter()
            .enumerate()
            .map(|(i, b)| CommandItem {
                id: format!("bookmarks.open.{i}"),
                title: b.title.clone(),
                subtitle: Some(b.folder.clone()),
                icon: Some(Icon {
                    kind: IconKind::Glyph,
                    value: GLYPH.to_string(),
                }),
                section: Some(tr("书签", "Bookmarks").to_string()),
                tags: None,
                details: None,
                text_to_suggest: None,
                more_commands: None,
                command: CommandRef::Invoke,
            })
            .collect();
        if idx.truncated {
            items.push(CommandItem {
                id: "bookmarks.overflow".to_string(),
                title: tr(
                    "…书签过多，仅索引前 {n} 条",
                    "…Too many bookmarks, only the first {n} indexed",
                )
                .replace("{n}", &MAX_ENTRIES.to_string()),
                subtitle: None,
                icon: Some(Icon {
                    kind: IconKind::Glyph,
                    value: GLYPH.to_string(),
                }),
                section: Some(tr("书签", "Bookmarks").to_string()),
                tags: None,
                details: None,
                text_to_suggest: None,
                more_commands: None,
                command: CommandRef::Invoke,
            });
        }
        // 空态占位（S3）：未发现任何 Chromium 书签文件（无浏览器 / 无配置档）
        // 时给出一条信息行——顶层列表恒非空（roundtrip 非空契约），且用户
        // 能得到「为什么没有书签」的直接解释而非空白。
        if items.is_empty() {
            items.push(CommandItem {
                id: "bookmarks.none".to_string(),
                title: tr(
                    "未发现 Chrome / Edge 书签文件",
                    "No Chrome / Edge bookmark files found",
                )
                .to_string(),
                subtitle: Some(
                    tr(
                        "安装并启动一次 Chrome 或 Edge 后重试",
                        "Install and launch Chrome or Edge once, then retry",
                    )
                    .to_string(),
                ),
                icon: Some(Icon {
                    kind: IconKind::Glyph,
                    value: GLYPH.to_string(),
                }),
                section: Some(tr("书签", "Bookmarks").to_string()),
                tags: None,
                details: None,
                text_to_suggest: None,
                more_commands: None,
                command: CommandRef::Invoke,
            });
        }
        items
    }

    pub fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
        if params.id == "bookmarks.none" {
            return (
                CommandResult::ShowToast {
                    message: tr(
                        "未发现书签文件：装好浏览器并至少启动一次后会出现在这里",
                        "No bookmark files yet: launch Chrome or Edge once and they will show up here",
                    )
                    .to_string(),
                    duration_ms: Some(2_500),
                },
                Vec::new(),
            );
        }
        if params.id == "bookmarks.overflow" {
            return (
                CommandResult::ShowToast {
                    message: tr(
                        "书签过多，仅索引前 {n} 条（可在浏览器整理书签后重试）",
                        "Too many bookmarks, only the first {n} indexed",
                    )
                    .replace("{n}", &MAX_ENTRIES.to_string()),
                    duration_ms: Some(2_500),
                },
                Vec::new(),
            );
        }
        let Some(seq) = params.id.strip_prefix("bookmarks.open.") else {
            return (
                CommandResult::ShowToast {
                    message: tr("未知书签命令：{id}", "Unknown bookmark command: {id}")
                        .replace("{id}", &params.id),
                    duration_ms: Some(2_500),
                },
                Vec::new(),
            );
        };
        let Ok(i) = seq.parse::<usize>() else {
            return (
                CommandResult::ShowToast {
                    message: tr("未知书签命令：{id}", "Unknown bookmark command: {id}")
                        .replace("{id}", &params.id),
                    duration_ms: Some(2_500),
                },
                Vec::new(),
            );
        };
        let idx = snapshot();
        let Some(b) = idx.entries.get(i) else {
            return (
                CommandResult::ShowToast {
                    message: tr(
                        "书签不存在或列表已变化：{id}",
                        "Bookmark not found or list changed: {id}",
                    )
                    .replace("{id}", &params.id),
                    duration_ms: Some(2_500),
                },
                Vec::new(),
            );
        };
        (
            CommandResult::KeepOpen,
            vec![Effect::HostRequest {
                method: METHOD_HOST_OPEN_URL,
                params: serde_json::json!({ "url": b.url }),
            }],
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// S3：vendor 目录随平台布局（macOS Application Support / Linux
        /// .config），Chrome 与 Edge 各一；macOS Edge 目录名带空格。
        #[test]
        fn vendor_dirs_cover_chrome_and_edge_per_platform() {
            let home = Path::new("/home/u");
            let dirs = vendor_dirs(home);
            assert_eq!(dirs.len(), 2, "Chrome + Edge");
            assert_eq!(dirs[0].1, "Chrome");
            assert_eq!(dirs[1].1, "Edge");
            let joined = |i: usize| dirs[i].0.to_string_lossy().replace('\\', "/");
            if cfg!(target_os = "macos") {
                assert!(joined(0).ends_with("Library/Application Support/Google/Chrome"));
                assert!(joined(1).ends_with("Library/Application Support/Microsoft Edge"));
            } else {
                assert!(joined(0).ends_with(".config/google-chrome"));
                assert!(joined(1).ends_with(".config/microsoft-edge"));
            }
        }

        /// S3：配置档发现口径 = 「含 Bookmarks 文件的子目录」（与 Windows
        /// 分支同构）；无 Bookmarks 的子目录与文件条目跳过。
        #[test]
        fn collect_profiles_only_picks_dirs_with_bookmarks_file() {
            let tmp = std::env::temp_dir().join(format!("ddrun-bm-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&tmp);
            for prof in ["Default", "Profile 1", "System Profile"] {
                std::fs::create_dir_all(tmp.join(prof)).unwrap();
            }
            std::fs::write(tmp.join("Default/Bookmarks"), "{}").unwrap();
            std::fs::write(tmp.join("Profile 1/Bookmarks"), "{}").unwrap();
            std::fs::write(tmp.join("loose-file"), "x").unwrap();
            let mut out = Vec::new();
            collect_profiles(&tmp, "Chrome", &mut out);
            let _ = std::fs::remove_dir_all(&tmp);
            assert_eq!(out.len(), 2, "Default + Profile 1 含书签文件");
            assert_eq!(out[0].1, "Chrome");
            assert!(out[0].0.ends_with("Default/Bookmarks"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// N3：Chrome 真实结构样例——三根（书签栏/其他/移动设备）、嵌套文件夹、
    /// url 节点；扁平化顺序 = 根序 + 深度优先，folder 路径 = `浏览器/根/子文件夹`。
    #[test]
    fn n3_parse_chrome_fixture_flattens_folders() {
        let json = r#"{
            "roots": {
                "bookmark_bar": {
                    "children": [
                        {"type": "url", "name": "GitHub", "url": "https://github.com"},
                        {"type": "folder", "name": "开发", "children": [
                            {"type": "url", "name": "Rust", "url": "https://rust-lang.org"},
                            {"type": "url", "name": "docs.rs", "url": "https://docs.rs"}
                        ]},
                        {"type": "url", "name": "  带空格名  ", "url": "https://example.com"}
                    ],
                    "name": "书签栏", "type": "folder"
                },
                "other": {"children": [], "name": "其他书签", "type": "folder"},
                "synced": {"children": [
                    {"type": "url", "name": "手机收藏", "url": "https://m.example.com"}
                ], "name": "移动设备书签", "type": "folder"}
            },
            "version": 1
        }"#;
        let out = parse_bookmarks_json(json, "Chrome").expect("合法结构应解析成功");
        assert_eq!(out.len(), 5);
        assert_eq!(out[0].title, "GitHub");
        assert_eq!(out[0].folder, "Chrome/书签栏");
        assert_eq!(out[1].title, "Rust");
        assert_eq!(out[1].folder, "Chrome/书签栏/开发");
        assert_eq!(out[2].title, "docs.rs");
        assert_eq!(out[2].folder, "Chrome/书签栏/开发");
        assert_eq!(out[3].title, "带空格名", "标题 trim");
        assert_eq!(out[4].title, "手机收藏");
        assert_eq!(out[4].folder, "Chrome/移动设备书签");
    }

    /// N3：损坏 / 非法结构回落 None（调用方跳过该文件，不挂起不 panic）；
    /// 缺 url 字段的节点跳过、未知 type 跳过（向前兼容）。
    #[test]
    fn n3_parse_corrupt_and_unknown_nodes_are_tolerated() {
        assert!(parse_bookmarks_json("not json", "Chrome").is_none());
        assert!(parse_bookmarks_json("{}", "Chrome").is_none(), "缺 roots");
        assert!(
            parse_bookmarks_json(r#"{"roots": "x"}"#, "Chrome").is_none(),
            "roots 非对象"
        );
        let weird = r#"{"roots": {
            "bookmark_bar": {"children": [
                {"type": "url", "name": "缺url"},
                {"type": "weird", "name": "未知类型"},
                {"type": "url", "name": "正常", "url": "https://ok"},
                {"type": "url", "name": "", "url": "https://empty-title"}
            ]},
            "other": {"children": [], "type": "folder"},
            "synced": {"children": [], "type": "folder"}
        }}"#;
        let out = parse_bookmarks_json(weird, "Edge").expect("结构合法应成功");
        assert_eq!(out.len(), 1, "缺 url / 空 title / 未知 type 均跳过");
        assert_eq!(out[0].title, "正常");
        assert_eq!(out[0].folder, "Edge/书签栏");
    }

    /// N3：条目上限——超过 [`MAX_ENTRIES`] 截断且解析不挂起（cap 语义在
    /// 解析层收口，顶层构建与 invoke 共用同一快照）。
    #[test]
    fn n3_parse_truncates_at_cap() {
        let children: Vec<String> = (0..MAX_ENTRIES + 500)
            .map(|i| format!(r#"{{"type": "url", "name": "b{i}", "url": "https://x/{i}"}}"#))
            .collect();
        let json = format!(
            r#"{{"roots": {{"bookmark_bar": {{"children": [{}], "type": "folder"}},
            "other": {{"children": [], "type": "folder"}}, "synced": {{"children": [], "type": "folder"}}}}}}"#,
            children.join(",")
        );
        let out = parse_bookmarks_json(&json, "Chrome").expect("应解析成功");
        assert_eq!(out.len(), MAX_ENTRIES, "恰在 cap 截断");
        assert_eq!(out[MAX_ENTRIES - 1].title, format!("b{}", MAX_ENTRIES - 1));
    }
}
