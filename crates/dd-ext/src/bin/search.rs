//! dd-ext-search —— 内置「文件搜索」扩展（`com.ddrun.filesearch`，Windows-only）。
//!
//! 功能（便捷 + 速度优化版）：
//! - **顶层**：一条「文件搜索」入口（`CommandRef::Page` → `files.results`）；
//! - **兜底**（§6.2）：每查询一条模板 `files.search.query`（title 含 `{query}`）；
//! - **子页**（§6.3，运行时 `PageHandler` 补齐）：`get_items(files.results, search_text)`
//!   经 Everything 官方 `es.exe`（IPC 通道，**免开 HTTP 服务器**）返回前 N 条文件项；
//! - **invoke**：回车经 `host/open_url`（`file://`）打开文件；上下文菜单三动作
//!   （v3.3 P1，协议零改动）：打开（默认主命令）/ `files.reveal.{pid}` 显示所在
//!   目录（扩展进程直接 `explorer /select`）/ `files.copy.{pid}` 复制路径
//!   （`ShowToast` + `host/set_clipboard`）。
//!
//! 速度打磨（扩展侧，零宿主改动即可生效）：
//! 1. **探测缓存**：`available()` 探测结果按 TTL 缓存（默认 3s），避免每次按键
//!    都探测 Everything（`es.exe -get-everything-version` IPC 探活）。
//! 2. **超时收紧**：探测 800ms、搜索 1.2s（宿主 `get_items` 超时 2000ms，留余量）。
//! 3. **结果数自适应**：默认 30 条，评分排序稳定；海量结果只取前 N。
//! 4. **查询直透**：Everything 全部搜索语法（`ext:`/`dm:`/`path:`/通配符/正则）
//!    原样透传，无需扩展侧解析。
//!
//! 依赖：仅 `fuzzy-matcher`（纯 Rust）+ `chrono`；查询走 `es.exe` 子进程 IPC，
//! **不使用 Everything HTTP / TcpStream**（v3.3 起废弃，历史方案见
//! [`docs/search-file.md`](../../docs/search-file.md)）。协议 v1.0 冻结：
//! **未新增任何协议方法**，完全复用 provider 模型。

use chrono::{DateTime, Local, Utc};
use dd_ext::{i18n::tr, run, Effect, ExtensionSpec};
use dd_protocol::messages::{GetItemsParams, GetItemsResult, InvokeParams};
use dd_protocol::model::{CommandItem, CommandRef, CommandResult, Details, Icon, IconKind};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const EXT_ID: &str = "com.ddrun.filesearch";
const PAGE_ID: &str = "files.results";
const RESULT_LIMIT: usize = 30;
/// availability 探测缓存 TTL：窗口内跳过重复探测。
const AVAIL_TTL: Duration = Duration::from_secs(3);
/// 单次 es.exe 查询超时（宿主 `get_items` 超时仅 2000ms，需留足余量）。
const ES_TIMEOUT: Duration = Duration::from_millis(1200);

// ─── 进程内路径索引（invoke 时按 id 找回完整路径）─────────────────────
// 进程随 stdin 循环常驻，索引单调递增、无碰撞；会话结束随进程退出释放。
// `HashMap::new()` 非 const，用 `LazyLock` 惰性初始化。
//
// ⚠️ 容量上限：扩展进程长驻，若只增不删，索引会随 `get_items` 调用次数无限
// 增长（每页最多 RESULT_LIMIT=30 条），与「连续 100 次请求无内存泄漏」的验收
// （§三 测试清单 / §8.4.5 第 10 项）直接冲突。故超出上限时淘汰**最旧**的一批：
// id 单调递增 → 最小即最旧；最近注册的 id 恒在保留范围内，因此
// 「注册后立即 lookup」永远命中（这是 invoke 链路的唯一用法）。
const PATH_INDEX_CAP: usize = 1024;
static PATH_INDEX: LazyLock<Mutex<HashMap<u64, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static PATH_NEXT: AtomicU64 = AtomicU64::new(1);

fn register_path(path: &str) -> u64 {
    let id = PATH_NEXT.fetch_add(1, Ordering::Relaxed);
    let mut idx = PATH_INDEX.lock().unwrap();
    if idx.len() >= PATH_INDEX_CAP {
        // 淘汰最旧的若干项，使容量回落到 PATH_INDEX_CAP - 1（为本次 insert 留位）
        let mut keys: Vec<u64> = idx.keys().copied().collect();
        keys.sort_unstable();
        let evict = keys.len() - PATH_INDEX_CAP + 1;
        for k in keys.into_iter().take(evict) {
            idx.remove(&k);
        }
    }
    idx.insert(id, path.to_string());
    id
}

fn lookup_path(id: u64) -> Option<String> {
    PATH_INDEX.lock().unwrap().get(&id).cloned()
}

/// 当前索引条目数（仅测试用，用于断言容量上限生效）。
#[cfg(test)]
fn path_index_len() -> usize {
    PATH_INDEX.lock().unwrap().len()
}

// ─── availability 探测缓存（TTL）────────────────────────────────────
struct AvailCache {
    ok: bool,
    at: Instant,
}
static AVAIL: Mutex<Option<AvailCache>> = Mutex::new(None);

/// Everything（经 es.exe / IPC）是否可用（带 TTL 缓存，避免每次按键都启动进程探测）。
///
/// `-get-everything-version` 是与 Everything 建立 IPC 的最轻量方式：Everything 未运行时
/// es 以非 0 退出码失败（8 = 无 IPC 窗口），此处只需判断命令能否成功执行。
fn everything_available() -> bool {
    let now = Instant::now();
    if let Some(c) = AVAIL.lock().unwrap().as_ref() {
        if now.duration_since(c.at) < AVAIL_TTL {
            return c.ok;
        }
    }
    let ok = match es_exe_path() {
        Some(exe) => run_es(&exe, &["-get-everything-version"]).is_ok(),
        None => false,
    };
    *AVAIL.lock().unwrap() = Some(AvailCache { ok, at: now });
    ok
}

/// 定位 es.exe（Everything 官方命令行工具，走 IPC，**无需开启 HTTP 服务器**）。
///
/// 优先级：`DDRUN_ES_PATH`（完整路径）→ PATH 中的 `es.exe`（winget 安装后即在 PATH）
/// → `DDRUN_EVERYTHING_DIR`（用户给出的 Everything 安装目录）。
fn es_exe_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("DDRUN_ES_PATH") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(paths) = std::env::var("PATH") {
        for dir in std::env::split_paths(&paths) {
            let cand = dir.join("es.exe");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    if let Ok(dir) = std::env::var("DDRUN_EVERYTHING_DIR") {
        let cand = PathBuf::from(dir).join("es.exe");
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// 执行 es.exe 并返回 stdout（**按代码页解码**：es 经管道输出为系统 OEM 代码页
/// GBK，必须 `decode_output` 转 UTF-8，否则中文路径损坏，见下方 decode 模块）。
///
/// 读取放在子线程持续消费管道，避免输出较多时子进程写满管道阻塞（死锁）；
/// 主线程用 `recv_timeout` 做超时兜底（宿主 `get_items` 超时仅 2000ms）。
fn run_es(exe: &std::path::Path, args: &[&str]) -> anyhow::Result<String> {
    let mut child = std::process::Command::new(exe)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| anyhow::anyhow!("启动 es.exe 失败：{e}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("无法获取 es.exe stdout"))?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let r = stdout.read_to_end(&mut buf).map(|_| buf);
        let _ = tx.send(r);
    });
    match rx.recv_timeout(ES_TIMEOUT) {
        Ok(Ok(bytes)) => {
            let _ = child.wait();
            Ok(decode_output(&bytes))
        }
        Ok(Err(e)) => {
            let _ = child.wait();
            Err(anyhow::anyhow!("读取 es.exe 输出失败：{e}"))
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(anyhow::anyhow!(
                "es.exe 查询超时（>{}ms）",
                ES_TIMEOUT.as_millis()
            ))
        }
    }
}

// ─── es.exe 输出解码（GBK 乱码修复）───────────────────────────────
/// es.exe 经管道输出的是系统 OEM 代码页（中文 Windows = 936/GBK），**并非 UTF-8**。
/// 直接 `from_utf8` 会把中文替换成 U+FFFD，导致中文路径损坏、搜索「搜不到」。
/// 解码策略（复用 `shell.rs` 惯例）：
/// ① 合法 UTF-8 直接采用；② 否则按 `GetConsoleOutputCP()` 转码；③ 转码失败回落 lossy。
#[cfg(windows)]
fn decode_output(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let cp = unsafe { windows_sys::Win32::System::Console::GetConsoleOutputCP() };
    decode_with_codepage(bytes, cp).unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(not(windows))]
fn decode_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// 按指定代码页把多字节串转 UTF-16 再到 `String`（`MultiByteToWideChar`）。
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

// ─── es.exe JSON 输出解析 ─────────────────────────────────────────
/// `es.exe -json -size -dm -attributes` 的单条输出（本机实测结构）。
#[derive(serde::Deserialize)]
struct EsEntry {
    /// 完整路径（目录以 `\` 结尾）
    filename: String,
    #[serde(default)]
    size: u64,
    /// Windows FILETIME（100ns 间隔，自 1601-01-01 UTC）
    #[serde(default)]
    date_modified: i64,
    /// Win32 文件属性位；`0x10` = FILE_ATTRIBUTE_DIRECTORY
    #[serde(default)]
    attributes: u32,
}

/// 解析后的单条文件（扩展内部表示）。
struct FileEntry {
    name: String,
    dir: String,
    size: u64,
    /// Unix 秒（解析失败为 0）；用于近因加分。
    modified: i64,
    /// Everything 原始日期串（用于详情展示，避免二次格式化）。
    modified_raw: String,
    is_dir: bool,
}

impl FileEntry {
    fn full_path(&self) -> String {
        if self.dir.is_empty() {
            self.name.clone()
        } else {
            format!("{}\\{}", self.dir.trim_end_matches('\\'), self.name)
        }
    }
}

/// Windows FILETIME（100ns 间隔，自 1601-01-01）→ Unix 秒；非正值返回 0（不 panic）。
///
/// es.exe 的 `-dm` 直接给出 FILETIME，**比原 HTTP 通道的日期字符串确定得多**——
/// 后者格式随 Everything 版本/设置而变（见 §三 2.2 已知坑），需猜测解析。
const FILETIME_UNIX_DIFF: i64 = 11_644_473_600; // 1601-01-01 → 1970-01-01 的秒数
fn filetime_to_unix(ft: i64) -> i64 {
    if ft <= 0 {
        0
    } else {
        ft / 10_000_000 - FILETIME_UNIX_DIFF
    }
}

/// FILETIME → 本地时间展示串（详情面板用）；无效返回 `-`。
fn format_filetime(ft: i64) -> String {
    let unix = filetime_to_unix(ft);
    if unix <= 0 {
        return "-".to_string();
    }
    match DateTime::from_timestamp(unix, 0) {
        Some(dt) => dt
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => "-".to_string(),
    }
}

/// 拆分 es 给出的完整路径 → `(目录, 文件名)`；目录项以 `\` 结尾，需先裁剪。
fn split_path(full: &str) -> (String, String) {
    let t = full.trim_end_matches(['\\', '/']);
    match t.rsplit_once(['\\', '/']) {
        Some((dir, name)) => (dir.to_string(), name.to_string()),
        None => (String::new(), t.to_string()),
    }
}

fn now_7days_unix() -> i64 {
    Utc::now().timestamp() - 7 * 24 * 3600
}

/// 目录启发式（best-effort）：**仅在 es 未给出 attributes（为 0）时兜底**；
/// 正常情况下由 `FILE_ATTRIBUTE_DIRECTORY` 精确判定。
fn guess_is_dir(name: &str, size: u64, type_field: &str) -> bool {
    if type_field.eq_ignore_ascii_case("folder") {
        return true;
    }
    !name.contains('.') && size == 0
}

/// 解析 es.exe 的 JSON 输出 → `FileEntry` 列表。
///
/// **纯函数（与传输解耦）**：可用 fixture 字符串离线单测（§8.4.2 第 2 条）。
/// ⚠️ es **无结果时输出空字符串**（不是 `[]`），必须按空结果处理。
fn parse_response(body: &str) -> anyhow::Result<Vec<FileEntry>> {
    let t = body.trim();
    if t.is_empty() {
        return Ok(Vec::new());
    }
    let entries: Vec<EsEntry> = serde_json::from_str(t)
        .map_err(|e| anyhow::anyhow!("es 输出解析失败：{e}（原始：{}）", &t[..t.len().min(200)]))?;
    Ok(entries
        .into_iter()
        .map(|e| {
            let (dir, name) = split_path(&e.filename);
            // attributes 有效（非 0）时用 FILE_ATTRIBUTE_DIRECTORY(0x10) 精确判定；
            // 缺失（0）时退回启发式
            let is_dir = if e.attributes == 0 {
                guess_is_dir(&name, e.size, "")
            } else {
                e.attributes & 0x10 != 0
            };
            FileEntry {
                name,
                dir,
                size: e.size,
                modified: filetime_to_unix(e.date_modified),
                modified_raw: format_filetime(e.date_modified),
                is_dir,
            }
        })
        .collect())
}

fn search(q: &str, limit: usize) -> anyhow::Result<Vec<FileEntry>> {
    let exe =
        es_exe_path().ok_or_else(|| anyhow::anyhow!("未找到 es.exe（Everything 命令行工具）"))?;
    // `-json` 输出 JSON；`-n` 限条数；`-size`/`-dm`/`-attributes` 取大小、修改时间、属性。
    // query 以 `-` / `/` 开头时用 `^` 转义，避免被 es 误判为开关（官方用法）。
    let query = if q.starts_with('-') || q.starts_with('/') {
        format!("^{q}")
    } else {
        q.to_string()
    };
    let n = limit.to_string();
    let out = run_es(
        &exe,
        &["-json", "-n", &n, "-size", "-dm", "-attributes", &query],
    )?;
    parse_response(&out)
}

// ─── 评分（归一化到 0~1）────────────────────────────────────────────
/// 把 skim 原始分（i64，量级数百~数千）归一化到 0~1。
fn norm(skim: i64) -> f64 {
    if skim <= 0 {
        0.0
    } else {
        (skim as f64 / 1000.0).min(1.0)
    }
}

fn score(entry: &FileEntry, query: &str) -> f64 {
    let matcher = SkimMatcherV2::default();
    let name_score = matcher
        .fuzzy_match(&entry.name, query)
        .map(norm)
        .unwrap_or(0.0);
    let path_score = matcher
        .fuzzy_match(&entry.full_path(), query)
        .map(|s| norm(s) * 0.3)
        .unwrap_or(0.0);
    let recency = if entry.modified > now_7days_unix() {
        0.1
    } else {
        0.0
    };
    (name_score + path_score + recency).clamp(0.0, 1.0)
}

fn human_size(bytes: u64) -> String {
    const U: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{:.1} {}", v, U[i])
    }
}

// ─── CommandItem 构造 ──────────────────────────────────────────────
fn file_glyph(is_dir: bool) -> &'static str {
    if is_dir {
        "\u{E8B7}" // 文件夹
    } else {
        "\u{E7C3}" // 文件
    }
}

fn to_command_item(entry: &FileEntry) -> CommandItem {
    let path = entry.full_path();
    let pid = register_path(&path);
    // v3.3 P1（§9.2）：三个动作共享同一 pid（PATH_INDEX 单次注册），上下文菜单
    // = 打开（默认主命令）/ 显示所在目录 / 复制路径，对齐 ECP 高频三动作。
    let more =
        |id: String, title_zh: &'static str, title_en: &'static str, glyph: &str| CommandItem {
            id,
            title: tr(title_zh, title_en).to_string(),
            subtitle: None,
            icon: Some(Icon {
                kind: IconKind::Glyph,
                value: glyph.to_string(),
            }),
            section: None,
            tags: None,
            details: None,
            text_to_suggest: None,
            more_commands: None,
            command: CommandRef::Invoke,
        };
    CommandItem {
        id: format!("files.open.{pid}"),
        title: entry.name.clone(),
        subtitle: Some(path.clone()),
        icon: Some(Icon {
            kind: IconKind::Glyph,
            value: file_glyph(entry.is_dir).to_string(),
        }),
        section: Some(tr("文件", "Files").to_string()),
        tags: Some(vec!["files".to_string()]),
        details: Some(Details {
            title: entry.name.clone(),
            body: format!(
                "{} · {}",
                human_size(entry.size),
                if entry.modified_raw.is_empty() {
                    "-".to_string()
                } else {
                    entry.modified_raw.clone()
                }
            ),
            metadata: None,
        }),
        text_to_suggest: None,
        more_commands: Some(vec![
            more(
                format!("files.reveal.{pid}"),
                "显示所在目录",
                "Show in folder",
                "\u{E8DA}", // FolderOpen，与宿主静态菜单「打开所在位置」同字形
            ),
            more(
                format!("files.copy.{pid}"),
                "复制路径",
                "Copy path",
                "\u{E8C8}", // Copy，与宿主静态菜单「复制路径」同字形
            ),
        ]),
        command: CommandRef::Invoke,
    }
}

fn hint_item() -> CommandItem {
    CommandItem {
        id: "files.hint".into(),
        title: tr("输入关键词搜索文件", "Type to search files").into(),
        subtitle: Some(tr("Everything 已就绪", "Everything is ready").into()),
        icon: Some(Icon {
            kind: IconKind::Glyph,
            value: file_glyph(false).to_string(),
        }),
        section: Some(tr("文件", "Files").into()),
        tags: None,
        details: None,
        text_to_suggest: None,
        more_commands: None,
        command: CommandRef::Invoke,
    }
}

fn guide_item() -> CommandItem {
    CommandItem {
        id: "files.guide".into(),
        title: tr("未检测到 Everything", "Everything not detected").into(),
        subtitle: Some(
            tr(
                "请安装并运行 Everything，并确保 es.exe 可用（或设 DDRUN_ES_PATH / DDRUN_EVERYTHING_DIR）",
                "Install and run Everything, and make sure es.exe is available (or set DDRUN_ES_PATH / DDRUN_EVERYTHING_DIR)",
            )
            .into(),
        ),
        icon: Some(Icon {
            kind: IconKind::Glyph,
            value: file_glyph(false).to_string(),
        }),
        section: Some(tr("文件", "Files").into()),
        tags: None,
        details: None,
        text_to_suggest: None,
        more_commands: None,
        command: CommandRef::Invoke,
    }
}

fn error_item(msg: &str) -> CommandItem {
    CommandItem {
        id: "files.error".into(),
        title: tr("文件搜索出错", "File search error").into(),
        subtitle: Some(msg.to_string()),
        icon: Some(Icon {
            kind: IconKind::Glyph,
            value: file_glyph(false).to_string(),
        }),
        section: Some(tr("文件", "Files").into()),
        tags: None,
        details: None,
        text_to_suggest: None,
        more_commands: None,
        command: CommandRef::Invoke,
    }
}

// ─── ExtensionSpec ─────────────────────────────────────────────────
fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: EXT_ID,
        display_name: tr("文件搜索", "File Search"),
        description: tr(
            "基于 Everything 的本地文件搜索（输入 f 后空格直接进入）",
            "Local file search powered by Everything (type 'f ' to jump in)",
        ),
        frozen: false, // 结果随 query 变化，不可冻结缓存
        has_fallback: true,
        // v3.3 P1：复制路径动作需要剪贴板能力——依据能力前置规则（§7.4），
        // 未声明的 host/* 请求将被宿主回 -32601，故 spec 与 manifest 必须同步
        //（一致性断言见 tests::spec_manifest_capabilities_consistent）。
        capabilities: &["host/open_url", "host/show_status", "host/set_clipboard"],
        log_tag: "dd-ext-filesearch",
        top_level: top_level_commands,
        fallback: Some(fallback_commands),
        invoke: handle_invoke,
        pages: Some(get_file_items),
    }
}

fn top_level_commands() -> Vec<CommandItem> {
    vec![CommandItem {
        id: "files.search".into(),
        title: tr("文件搜索", "File Search").into(),
        subtitle: Some(
            tr(
                "输入关键词搜索本地文件（或输入 f 后空格直接进入）",
                "Type a query to search local files (or type 'f ' to jump in)",
            )
            .into(),
        ),
        icon: Some(Icon {
            kind: IconKind::Glyph,
            value: file_glyph(false).to_string(),
        }),
        section: Some(tr("文件", "Files").into()),
        tags: Some(vec!["files".to_string()]),
        details: None,
        text_to_suggest: None,
        more_commands: None,
        command: CommandRef::Page {
            page_id: PAGE_ID.into(),
        },
    }]
}

fn fallback_commands() -> Vec<CommandItem> {
    vec![CommandItem {
        id: "files.search.query".into(),
        title: tr("在文件中搜索 {query}", "Search files for {query}").into(),
        subtitle: Some(
            tr(
                "用 Everything 搜索本地文件",
                "Search local files with Everything",
            )
            .into(),
        ),
        icon: Some(Icon {
            kind: IconKind::Glyph,
            value: file_glyph(false).to_string(),
        }),
        section: Some(tr("文件", "Files").into()),
        tags: Some(vec!["files".to_string()]),
        details: None,
        text_to_suggest: None,
        more_commands: None,
        command: CommandRef::Page {
            page_id: PAGE_ID.into(),
        },
    }]
}

/// §6.3 子页内容构造器：返回 `files.results` 页的当前查询文件项。
fn get_file_items(params: &GetItemsParams) -> GetItemsResult {
    get_file_items_with(params, everything_available())
}

/// 可用性注入版（§9.3 L1：单测不依赖 Everything/es.exe——离线确定性）。
fn get_file_items_with(params: &GetItemsParams, available: bool) -> GetItemsResult {
    let query = params.search_text.as_deref().unwrap_or("").trim();
    if query.is_empty() {
        return GetItemsResult {
            items: vec![hint_item()],
            has_more_items: false,
            is_loading: false,
        };
    }
    if !available {
        return GetItemsResult {
            items: vec![guide_item()],
            has_more_items: false,
            is_loading: false,
        };
    }
    match search(query, RESULT_LIMIT) {
        Ok(entries) => {
            let mut scored: Vec<(f64, FileEntry)> =
                entries.into_iter().map(|e| (score(&e, query), e)).collect();
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            let items: Vec<CommandItem> = scored
                .into_iter()
                .take(RESULT_LIMIT)
                .map(|(_, e)| to_command_item(&e))
                .collect();
            GetItemsResult {
                items,
                has_more_items: false,
                is_loading: false,
            }
        }
        Err(e) => GetItemsResult {
            items: vec![error_item(&e.to_string())],
            has_more_items: false,
            is_loading: false,
        },
    }
}

// ─── v3.3 P1：上下文菜单动作（显示所在目录 / 复制路径）────────────────

/// 「显示所在目录」路径合法性（纯函数，单测锚点）：Windows 路径不允许双引号
/// （`explorer /select,"<path>"` 手动引号会与非法引号冲突），且不得为空。
fn valid_reveal_path(path: &str) -> bool {
    !path.is_empty() && !path.contains('"')
}

/// 执行「显示所在目录」。
///
/// Windows 必须 `CommandExt::raw_arg`：Rust 默认给含空格参数自动加引号，与
/// 手写的 `/select,"..."` 引号叠加会导致含空格路径解析异常（§9.2 P1.4 引号坑）。
/// 非 Windows 平台无 explorer 语义，回退打开父目录（xdg-open）。
#[cfg(windows)]
fn spawn_reveal(path: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    if !valid_reveal_path(path) {
        return Err(tr("路径不合法", "Invalid path").to_string());
    }
    std::process::Command::new("explorer.exe")
        .raw_arg(format!("/select,\"{path}\""))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn spawn_reveal(path: &str) -> Result<(), String> {
    if !valid_reveal_path(path) {
        return Err(tr("路径不合法", "Invalid path").to_string());
    }
    let Some(parent) = std::path::Path::new(path)
        .parent()
        .and_then(|p| p.to_str())
        .map(str::to_string)
    else {
        return Err(tr("路径不合法", "Invalid path").to_string());
    };
    std::process::Command::new("xdg-open")
        .arg(parent)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// reveal/copy 的**纯决策**（不执行副作用，单测不拉起 explorer / 不碰剪贴板，
/// §9.3 L1）：前缀 + pid 数字 + pid 在索引中 + `context.selected_item_id`
/// 冗余一致，任一失败即拒绝。成功返回（动作前缀，路径）。
fn resolve_path_action(
    prefix: &'static str,
    pid_str: &str,
    context: Option<&dd_protocol::messages::InvokeContext>,
) -> Result<(&'static str, String), ()> {
    let Ok(pid) = pid_str.parse::<u64>() else {
        return Err(());
    };
    // pid 已嵌入 id；selected_item_id 仅作冗余校验（应指向同 pid 的主命令）
    if let Some(ctx) = context {
        if let Some(sel) = &ctx.selected_item_id {
            if *sel != format!("files.open.{pid}") {
                return Err(());
            }
        }
    }
    let Some(path) = lookup_path(pid) else {
        return Err(());
    };
    if prefix == "files.reveal." && !valid_reveal_path(&path) {
        return Err(());
    }
    Ok((prefix, path))
}

fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
    // v3.3 P1：上下文菜单动作路由（files.reveal.{pid} / files.copy.{pid}）。
    for prefix in ["files.reveal.", "files.copy."] {
        if let Some(pid_str) = params.id.strip_prefix(prefix) {
            return match resolve_path_action(prefix, pid_str, params.context.as_ref()) {
                // 拒绝：明确 Toast，无副作用（§9.2 P1.3）
                Err(()) => (
                    CommandResult::ShowToast {
                        message: tr("路径已失效，请重新搜索", "Path is stale — search again")
                            .to_string(),
                        duration_ms: Some(2000),
                    },
                    Vec::new(),
                ),
                Ok(("files.reveal.", path)) => match spawn_reveal(&path) {
                    Ok(()) => (CommandResult::Dismiss, Vec::new()),
                    Err(e) => (
                        CommandResult::ShowToast {
                            message: tr("打开所在目录失败：", "Failed to reveal: ").to_string()
                                + &e,
                            duration_ms: Some(2500),
                        },
                        Vec::new(),
                    ),
                },
                Ok((_, path)) => (
                    // 复制路径：Toast 先回，剪贴板副作用由运行时在响应后按序发送
                    CommandResult::ShowToast {
                        message: tr("路径已复制", "Path copied").to_string(),
                        duration_ms: Some(2000),
                    },
                    vec![Effect::HostRequest {
                        method: "host/set_clipboard",
                        params: serde_json::json!({ "text": path }),
                    }],
                ),
            };
        }
    }
    if let Some(n_str) = params.id.strip_prefix("files.open.") {
        if let Ok(n) = n_str.parse::<u64>() {
            if let Some(path) = lookup_path(n) {
                let url = format!("file:///{}", path.replace('\\', "/"));
                return (
                    CommandResult::Dismiss,
                    vec![Effect::HostRequest {
                        method: "host/open_url",
                        params: serde_json::json!({ "url": url }),
                    }],
                );
            }
        }
    }
    // 静态占位项点击：不落回通用「未知命令」（§7.4 已知边界修复）——给出与该项
    // 含义匹配的提示：hint = 需先输入关键词；guide/error = Everything/错误说明。
    match params.id.as_str() {
        "files.hint" => (
            CommandResult::ShowToast {
                message: tr(
                    "请在搜索框输入关键词后回车",
                    "Type a keyword in the search box and press Enter",
                )
                .to_string(),
                duration_ms: Some(2000),
            },
            Vec::new(),
        ),
        "files.guide" => (
            CommandResult::ShowToast {
                message: tr(
                    "未检测到 Everything（或 es.exe 不在 PATH），请先安装/启动后重试",
                    "Everything (or es.exe) not detected — install/start it and retry",
                )
                .to_string(),
                duration_ms: Some(2500),
            },
            Vec::new(),
        ),
        "files.error" => (
            CommandResult::ShowToast {
                message: tr(
                    "文件搜索出现错误，请查看详情",
                    "File search hit an error — see the item details",
                )
                .to_string(),
                duration_ms: Some(2000),
            },
            Vec::new(),
        ),
        _ => (
            CommandResult::ShowToast {
                message: tr("未知命令", "Unknown command").to_string(),
                duration_ms: Some(2000),
            },
            Vec::new(),
        ),
    }
}

fn main() {
    run(&spec());
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_protocol::model::Sender;

    #[test]
    fn filetime_to_unix_converts_and_guards() {
        // 1970-01-01 的 FILETIME 基准 → 0；每 10^7 为 1 秒
        const BASE: i64 = 116_444_736_000_000_000;
        assert_eq!(filetime_to_unix(BASE), 0);
        assert_eq!(filetime_to_unix(BASE + 10_000_000), 1);
        // 实测样本（2026 年前后的文件）
        assert!(filetime_to_unix(134_195_186_392_659_119) > 1_700_000_000);
        // 非法/缺失值受保护，不 panic
        assert_eq!(filetime_to_unix(0), 0);
        assert_eq!(filetime_to_unix(-1), 0);
    }

    #[test]
    fn split_path_handles_files_dirs_and_roots() {
        assert_eq!(
            split_path("C:\\d\\f.txt"),
            ("C:\\d".to_string(), "f.txt".to_string())
        );
        // 目录项以反斜杠结尾（es 实测输出）→ 裁剪后末段为名
        assert_eq!(
            split_path("C:\\Program Files\\Everything\\"),
            ("C:\\Program Files".to_string(), "Everything".to_string())
        );
        assert_eq!(split_path("C:\\"), (String::new(), "C:".to_string()));
        assert_eq!(split_path("f.txt"), (String::new(), "f.txt".to_string()));
    }

    #[test]
    fn guess_is_dir_falls_back_when_attributes_missing() {
        // es 给出 attributes 时不走启发式；这里锁定 attributes==0（缺失）时的兜底行为
        assert!(guess_is_dir("README", 0, ""));
        assert!(!guess_is_dir("README.md", 0, ""));
        assert!(!guess_is_dir("README", 1, ""));
    }

    #[test]
    fn score_normalizes_and_ranks_name_over_path() {
        let q = "dd";
        let name_hit = FileEntry {
            name: "dd-run".into(),
            dir: "C:\\x".into(),
            size: 0,
            modified: 0,
            modified_raw: String::new(),
            is_dir: false,
        };
        let path_only = FileEntry {
            name: "readme".into(),
            dir: "C:\\dd-run".into(),
            size: 0,
            modified: 0,
            modified_raw: String::new(),
            is_dir: false,
        };
        let s_name = score(&name_hit, q);
        let s_path = score(&path_only, q);
        assert!((0.0..=1.0).contains(&s_name));
        assert!((0.0..=1.0).contains(&s_path));
        assert!(s_name > s_path, "文件名命中应高于仅路径命中");
    }

    #[test]
    fn score_recency_bonus_for_recent() {
        let recent = FileEntry {
            name: "x".into(),
            dir: String::new(),
            size: 0,
            modified: now_7days_unix() + 3600,
            modified_raw: String::new(),
            is_dir: false,
        };
        let old = FileEntry {
            name: "x".into(),
            dir: String::new(),
            size: 0,
            modified: now_7days_unix() - 3600,
            modified_raw: String::new(),
            is_dir: false,
        };
        assert!(score(&recent, "x") > score(&old, "x"));
    }

    #[test]
    fn path_index_roundtrip() {
        let p = register_path("C:\\a\\b.txt");
        assert_eq!(lookup_path(p), Some("C:\\a\\b.txt".to_string()));
        assert_eq!(lookup_path(999_999_999), None);
    }

    #[test]
    fn full_path_joins_dir_and_name() {
        assert_eq!(
            FileEntry {
                name: "f.txt".into(),
                dir: "C:\\d".into(),
                size: 0,
                modified: 0,
                modified_raw: String::new(),
                is_dir: false,
            }
            .full_path(),
            "C:\\d\\f.txt"
        );
    }

    #[test]
    fn get_file_items_empty_query_returns_hint() {
        let r = get_file_items(&GetItemsParams {
            page_id: PAGE_ID.into(),
            search_text: None,
        });
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].id, "files.hint");
    }

    #[test]
    fn everything_unavailable_returns_guide() {
        // 连不上时返回引导项；可用性经参数注入（§9.3 L1：单测不依赖
        // Everything/es.exe——本机 Everything 在跑时原环境探测版会真搜出结果）
        let r = get_file_items_with(
            &GetItemsParams {
                page_id: PAGE_ID.into(),
                search_text: Some("anything".into()),
            },
            false,
        );
        assert_eq!(r.items.len(), 1);
        assert!(r.items[0].id == "files.guide" || r.items[0].id == "files.error");
    }

    // ─── es 输出解析（纯函数，fixture 离线，不依赖 es / Everything）──
    #[test]
    fn parse_response_maps_es_fields() {
        // 本机 es.exe -json -size -dm -attributes "提示词" 的真实输出样本
        let json = r#"[{"filename":"E:\\AI\\kb\\cc-提示词.md","size":44222,
            "date_modified":134195186392659119,"attributes":32}]"#;
        let entries = parse_response(json).expect("合法 es 输出应解析成功");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "cc-提示词.md");
        assert_eq!(entries[0].dir, "E:\\AI\\kb");
        assert_eq!(entries[0].size, 44222);
        assert_eq!(entries[0].full_path(), "E:\\AI\\kb\\cc-提示词.md");
        assert!(
            entries[0].modified > 1_700_000_000,
            "FILETIME 应换算为 Unix 秒"
        );
        assert!(!entries[0].is_dir, "attributes=32(ARCHIVE) 非目录");
        assert!(
            !entries[0].modified_raw.is_empty() && entries[0].modified_raw != "-",
            "详情应格式化出修改时间"
        );
    }

    #[test]
    fn parse_response_directory_attribute_marks_is_dir() {
        let json = r#"[{"filename":"C:\\Program Files\\Everything\\","size":0,
            "date_modified":0,"attributes":16}]"#;
        let entries = parse_response(json).expect("应解析成功");
        assert!(entries[0].is_dir, "attributes=0x10 应判定为目录");
        assert_eq!(entries[0].name, "Everything", "目录尾部反斜杠应被裁剪");
        assert_eq!(entries[0].modified, 0, "无效 FILETIME 应为 0（不 panic）");
    }

    #[test]
    fn parse_response_empty_output_means_no_results() {
        // ⚠️ 实测：es 无结果时输出**空字符串**，不是 []——必须按空结果处理
        let entries = parse_response("").expect("空输出应视为无结果而非报错");
        assert!(entries.is_empty());
        let entries = parse_response("  \n ").expect("纯空白亦视为无结果");
        assert!(entries.is_empty());
    }

    // ─── GBK 解码（es 管道输出为系统 OEM 代码页，非 UTF-8）───
    #[cfg(windows)]
    #[test]
    fn decode_with_codepage_converts_gbk_filename() {
        // "提示词" 的 GBK(CP936) 编码字节（python: '提示词'.encode('gbk')）
        let gbk = [0xCCu8, 0xE1, 0xCA, 0xBE, 0xB4, 0xCA];
        let s = decode_with_codepage(&gbk, 936).expect("GBK 应可解码");
        assert_eq!(s, "提示词", "中文文件名须正确还原，不得为 U+FFFD");
    }

    #[test]
    fn decode_output_keeps_utf8_json() {
        // 纯 ASCII/UTF-8 的 JSON 应原样保留（快路径），不被转码破坏
        let bytes = br#"[{"filename":"C:\a.txt","size":1}]"#;
        assert_eq!(
            decode_output(bytes),
            r#"[{"filename":"C:\a.txt","size":1}]"#
        );
    }

    #[test]
    fn parse_response_malformed_returns_err() {
        assert!(parse_response("not json").is_err(), "非法 JSON 应返回 Err");
        assert!(
            parse_response("[{\"filename\":123}]").is_err(),
            "字段类型不符应返回 Err"
        );
        assert!(parse_response("[]").is_ok(), "空数组是合法输出");
    }

    // ─── T-15 invoke 分发 ────────────────────────────────────────
    #[test]
    fn handle_invoke_opens_file_via_host_request() {
        let pid = register_path("C:\\proj\\src\\main.rs");
        let (result, effects) = handle_invoke(&InvokeParams {
            id: format!("files.open.{pid}"),
            sender: Sender::ListItem,
            context: None,
        });
        assert!(
            matches!(result, CommandResult::Dismiss),
            "打开文件后应关闭面板"
        );
        assert_eq!(effects.len(), 1, "应发出一个 host/open_url 反向请求");
        match &effects[0] {
            Effect::HostRequest { method, params } => {
                assert_eq!(*method, "host/open_url");
                let url = params["url"].as_str().expect("应带 url 参数");
                assert_eq!(url, "file:///C:/proj/src/main.rs", "反斜杠应转为正斜杠");
            }
            other => panic!("期望 HostRequest，实际 {other:?}"),
        }
    }

    #[test]
    fn handle_invoke_unknown_command_toast() {
        // 引导项/占位项（files.hint/guide/error）点击给出**专属文案** Toast（v3.3 修复，
        // 不再落回通用「未知命令」）；未知 id 仍回通用 Toast。锁定「不 panic、有明确
        // 反馈、无副作用」不变量。
        for id in [
            "files.hint",
            "files.guide",
            "files.error",
            "files.open.notanumber",
        ] {
            let (result, effects) = handle_invoke(&InvokeParams {
                id: id.into(),
                sender: Sender::ListItem,
                context: None,
            });
            assert!(
                matches!(result, CommandResult::ShowToast { .. }),
                "{id} 应回 Toast"
            );
            assert!(effects.is_empty(), "{id} 不应发出副作用");
        }
        // 专属文案抽查：hint/guide/error 不再与「未知命令」同文案
        let (hint_toast, _) = handle_invoke(&InvokeParams {
            id: "files.hint".into(),
            sender: Sender::ListItem,
            context: None,
        });
        let (unknown_toast, _) = handle_invoke(&InvokeParams {
            id: "files.open.notanumber".into(),
            sender: Sender::ListItem,
            context: None,
        });
        assert_ne!(
            hint_toast, unknown_toast,
            "hint 点击文案应与通用「未知命令」区分"
        );
    }

    // ─── v3.3 P1：more_commands 三动作（§9.2/§9.3）─────────────────

    fn sample_entry() -> FileEntry {
        FileEntry {
            name: "报告 2026.txt".into(),
            dir: r"G:\AI\dd-run".into(),
            size: 1234,
            modified: 0,
            modified_raw: String::new(),
            is_dir: false,
        }
    }

    #[test]
    fn to_command_item_builds_three_actions_sharing_pid() {
        // P1.1：三个动作共享同一路径 pid；「注册后立即 lookup」恒安全
        let item = to_command_item(&sample_entry());
        let pid = item
            .id
            .strip_prefix("files.open.")
            .and_then(|s| s.parse::<u64>().ok())
            .expect("主命令 id = files.open.{pid}");
        let more = item
            .more_commands
            .as_ref()
            .expect("文件结果带 more_commands");
        assert_eq!(
            more.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec![format!("files.reveal.{pid}"), format!("files.copy.{pid}")],
            "reveal/copy 与主命令共用同一 pid"
        );
        let path = lookup_path(pid).expect("pid 已注册进 PATH_INDEX");
        assert_eq!(path, r"G:\AI\dd-run\报告 2026.txt");
        // 菜单项形态：Invoke 命令、无嵌套、带图标字形
        for m in more {
            assert!(matches!(m.command, CommandRef::Invoke));
            assert!(m.more_commands.is_none(), "不递归嵌套");
            assert!(matches!(
                m.icon,
                Some(Icon {
                    kind: IconKind::Glyph,
                    ..
                })
            ));
        }
    }

    #[test]
    fn resolve_path_action_validates_pid_and_context() {
        let item = to_command_item(&sample_entry());
        let pid = item.id["files.open.".len()..].to_string();
        use dd_protocol::messages::InvokeContext;
        let ctx = |sel: Option<String>| {
            Some(InvokeContext {
                query: None,
                selected_item_id: sel,
                form_data: None,
                confirmed: None,
            })
        };
        // 有效：reveal / copy 均放行并返回路径
        let (pfx, path) =
            resolve_path_action("files.reveal.", &pid, ctx(None).as_ref()).expect("有效 pid 放行");
        assert_eq!(pfx, "files.reveal.");
        assert_eq!(path, r"G:\AI\dd-run\报告 2026.txt");
        assert!(resolve_path_action("files.copy.", &pid, None).is_ok());
        // 无效 pid 数字 / 未注册 pid → 拒绝
        assert!(resolve_path_action("files.reveal.", "abc", None).is_err());
        assert!(resolve_path_action("files.reveal.", "999999999", None).is_err());
        // context.selected_item_id 冗余校验：指向别的主命令 → 拒绝（无副作用）
        assert!(
            resolve_path_action(
                "files.copy.",
                &pid,
                ctx(Some("files.open.7".into())).as_ref()
            )
            .is_err(),
            "selected_item_id 与 pid 不一致应拒绝"
        );
        assert!(
            resolve_path_action(
                "files.copy.",
                &pid,
                ctx(Some(format!("files.open.{pid}"))).as_ref()
            )
            .is_ok(),
            "一致时放行"
        );
    }

    #[test]
    fn invoke_copy_returns_toast_and_clipboard_effect_in_order() {
        let item = to_command_item(&sample_entry());
        let pid = item.id["files.open.".len()..].to_string();
        let (result, effects) = handle_invoke(&InvokeParams {
            id: format!("files.copy.{pid}"),
            sender: Sender::ContextMenu,
            context: None,
        });
        assert!(
            matches!(result, CommandResult::ShowToast { .. }),
            "复制回 Toast"
        );
        assert_eq!(effects.len(), 1, "恰好一条剪贴板副作用");
        match &effects[0] {
            Effect::HostRequest { method, params } => {
                assert_eq!(*method, "host/set_clipboard");
                assert_eq!(
                    params["text"],
                    serde_json::json!(r"G:\AI\dd-run\报告 2026.txt"),
                    "剪贴板内容 = 完整路径"
                );
            }
            other => panic!("应为 HostRequest，实际 {other:?}"),
        }
    }

    #[test]
    fn invoke_reveal_rejects_stale_pid_and_invalid_path_without_effects() {
        // 失效 pid（未注册）：明确 Toast、零副作用（§9.2 P1.3）
        let (result, effects) = handle_invoke(&InvokeParams {
            id: "files.reveal.424242".into(),
            sender: Sender::ContextMenu,
            context: None,
        });
        assert!(matches!(result, CommandResult::ShowToast { .. }));
        assert!(effects.is_empty());
        // 非法路径（含双引号）：valid_reveal_path 门控拒绝，不 spawn explorer
        let bad_pid = register_path(r#"G:\ba"dd\x.txt"#);
        let (result, effects) = handle_invoke(&InvokeParams {
            id: format!("files.reveal.{bad_pid}"),
            sender: Sender::ContextMenu,
            context: None,
        });
        assert!(matches!(result, CommandResult::ShowToast { .. }));
        assert!(effects.is_empty());
        assert!(!valid_reveal_path(r#"G:\ba"dd\x.txt"#));
        assert!(valid_reveal_path(r"G:\AI\dd-run\ok.txt"));
        assert!(!valid_reveal_path(""));
    }

    #[test]
    fn spec_manifest_capabilities_consistent() {
        // A-33-04：spec 与 manifest 能力集合完全相等（能力前置 §7.4 的契约锚点）
        let manifest = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/extensions.d/com.ddrun.filesearch.json"
        ));
        let v: serde_json::Value = serde_json::from_str(manifest).expect("manifest JSON 合法");
        let mut declared: Vec<String> = v["capabilities"]
            .as_array()
            .expect("manifest 带 capabilities")
            .iter()
            .map(|c| c.as_str().expect("字符串能力").to_string())
            .collect();
        let mut spec_caps: Vec<String> =
            spec().capabilities.iter().map(|s| s.to_string()).collect();
        declared.sort();
        spec_caps.sort();
        assert_eq!(declared, spec_caps, "manifest 与 spec 能力集合必须一致");
    }

    // ─── T-16 CommandItem 映射 ───────────────────────────────────
    #[test]
    fn to_command_item_maps_fields() {
        let entry = FileEntry {
            name: "main.rs".into(),
            dir: "C:\\proj\\src".into(),
            size: 1024,
            modified: 1717072496,
            modified_raw: "2024-05-30 12:34:56".into(),
            is_dir: false,
        };
        let item = to_command_item(&entry);
        assert!(
            item.id.starts_with("files.open."),
            "id 应为 files.open.<u64>"
        );
        assert_eq!(item.title, "main.rs");
        assert_eq!(item.subtitle.as_deref(), Some("C:\\proj\\src\\main.rs"));
        assert_eq!(item.section.as_deref(), Some("文件"));
        assert!(matches!(item.command, CommandRef::Invoke));
        let details = item.details.as_ref().expect("应有 details");
        assert_eq!(details.title, "main.rs");
        assert!(
            details.body.contains("2024-05-30 12:34:56"),
            "详情应含修改时间"
        );
        assert!(item.icon.is_some(), "应有图标字形");
    }

    #[test]
    fn hint_guide_error_items_shape() {
        for (item, expected_id) in [
            (hint_item(), "files.hint"),
            (guide_item(), "files.guide"),
            (error_item("boom"), "files.error"),
        ] {
            assert_eq!(item.id, expected_id);
            assert!(
                matches!(item.command, CommandRef::Invoke),
                "{expected_id} 应为 Invoke"
            );
            assert!(!item.title.is_empty(), "{expected_id} 应有标题");
        }
    }

    // ─── T-13 索引容量上限（长跑内存泄漏防线）─────────────────────
    #[test]
    fn path_index_evicts_beyond_capacity() {
        let oldest = register_path("C:\\oldest");
        let mut newest = oldest;
        for i in 0..(PATH_INDEX_CAP + 20) {
            newest = register_path(&format!("C:\\p{i}"));
        }
        assert!(
            path_index_len() <= PATH_INDEX_CAP,
            "索引不得超过容量上限，实际 {}",
            path_index_len()
        );
        assert!(lookup_path(newest).is_some(), "最近注册的路径不得被淘汰");
        assert_eq!(lookup_path(oldest), None, "最旧的路径应已被淘汰");
    }

    // ─── T-26 契约（v1.0 已定义字段，未新增协议方法）──────────────
    #[test]
    fn spec_contract_fields() {
        let s = spec();
        assert_eq!(s.id, EXT_ID);
        assert!(s.has_fallback, "文件搜索必须有 fallback 入口");
        assert!(!s.frozen, "结果随 query 变化，不可冻结缓存");
        assert!(s.pages.is_some(), "必须注册 files.results 子页处理器");
        assert!(
            s.capabilities.contains(&"host/open_url"),
            "capabilities 必须含 host/open_url"
        );
    }

    #[test]
    fn commands_contract_page_and_top_level() {
        let top = top_level_commands();
        assert!(!top.is_empty(), "顶层应有文件搜索入口（发现性）");
        let fb = fallback_commands();
        assert_eq!(fb.len(), 1, "每个非空 query 应返回一条入口项");
        assert!(
            matches!(fb[0].command, CommandRef::Page { .. }),
            "fallback 入口必须指向 files.results 子页"
        );
        assert!(fb[0].title.contains("{query}"), "入口标题应含 query 占位符");
    }
}
