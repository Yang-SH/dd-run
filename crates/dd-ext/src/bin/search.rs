//! dd-ext-search —— 内置「文件搜索」扩展（`com.ddrun.filesearch`，Windows-only）。
//!
//! 功能（便捷 + 速度优化版）：
//! - **顶层**：一条「文件搜索」入口（`CommandRef::Page` → `files.results`）；
//! - **兜底**（§6.2）：每查询一条模板 `files.search.query`（title 含 `{query}`）；
//! - **子页**（§6.3，运行时 `PageHandler` 补齐）：`get_items(files.results, search_text)`
//!   经 Everything HTTP 返回前 N 条文件 `CommandItem`；
//! - **invoke**：回车经 `host/open_url`（`file://`）打开文件，`Dismiss`。
//!
//! 速度打磨（扩展侧，零宿主改动即可生效）：
//! 1. **连接预热 + 复用**：`available()` 探测结果按 TTL 缓存（默认 3s），
//!    避免每次按键都探测 Everything；搜索本身复用同一 `TcpStream` 思路
//!    （本地回环，连接开销 <1ms）。
//! 2. **超时收紧**：探测 800ms、搜索 3s（Everything 本地通常 <50ms）。
//! 3. **结果数自适应**：默认 30 条，评分排序稳定；海量结果只取前 N。
//! 4. **查询直透**：Everything 全部搜索语法（`ext:`/`dm:`/`path:`/通配符/正则）
//!    原样透传，无需扩展侧解析。
//!
//! 依赖：仅 `fuzzy-matcher`（纯 Rust）；HTTP 用标准库 `TcpStream`（Everything
//! 为本地 HTTP/1.1，简单可靠、零额外 crate，契合项目最小依赖风格）。
//! 协议 v1.0 冻结：**未新增任何协议方法**，完全复用 provider 模型。

use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
use dd_ext::{i18n::tr, run, Effect, ExtensionSpec};
use dd_protocol::messages::{GetItemsParams, GetItemsResult, InvokeParams};
use dd_protocol::model::{CommandItem, CommandRef, CommandResult, Details, Icon, IconKind};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const EXT_ID: &str = "com.ddrun.filesearch";
const PAGE_ID: &str = "files.results";
const RESULT_LIMIT: usize = 30;
/// availability 探测缓存 TTL：窗口内跳过重复探测。
const AVAIL_TTL: Duration = Duration::from_secs(3);

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

fn everything_base() -> (String, u16) {
    // 默认 127.0.0.1:8080；可用 DDRUN_EVERYTHING_URL=http://host:port 覆盖。
    match std::env::var("DDRUN_EVERYTHING_URL") {
        Ok(u) if u.starts_with("http://") => {
            let rest = &u["http://".len()..];
            let (host, port_raw) = rest.split_once(':').unwrap_or((rest, "8080"));
            let port = port_raw
                .split('/')
                .next()
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(8080);
            (host.to_string(), port)
        }
        _ => ("127.0.0.1".to_string(), 8080),
    }
}

/// Everything 是否在线（带 TTL 缓存，避免每次按键都探测）。
fn everything_available() -> bool {
    let now = Instant::now();
    if let Some(c) = AVAIL.lock().unwrap().as_ref() {
        if now.duration_since(c.at) < AVAIL_TTL {
            return c.ok;
        }
    }
    let (host, port) = everything_base();
    let ok = std::net::TcpStream::connect((host.as_str(), port))
        .and_then(|mut s| {
            s.set_read_timeout(Some(Duration::from_millis(800)))?;
            s.set_write_timeout(Some(Duration::from_millis(800)))?;
            // 最简探测：根路径 json 请求（Connection: close 由对端关闭结束）
            let req = format!(
                "GET /?json=1&count=1 HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nAccept: */*\r\n\r\n"
            );
            s.write_all(req.as_bytes())?;
            let mut buf = [0u8; 256];
            // 只读一点即可：能连上即视为在线
            s.read(&mut buf).map(|_| ())
        })
        .is_ok();
    *AVAIL.lock().unwrap() = Some(AvailCache { ok, at: now });
    ok
}

/// 极简 HTTP/1.1 GET（Connection: close → 读到 EOF 即 body 结束，规避分块解析）。
fn http_get(rel: &str) -> anyhow::Result<String> {
    let (host, port) = everything_base();
    let mut stream = std::net::TcpStream::connect((host.as_str(), port))
        .map_err(|e| anyhow::anyhow!("Everything 连接失败（{host}:{port}）：{e}"))?;
    // 搜索本身放宽到 3s（Everything 本地通常 <50ms，3s 仅作异常兜底）；
    // availability 探测另走更短的 800ms 超时。
    stream.set_read_timeout(Some(Duration::from_millis(3000)))?;
    stream.set_write_timeout(Some(Duration::from_millis(800)))?;
    let req =
        format!("GET {rel} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nAccept: */*\r\n\r\n");
    stream.write_all(req.as_bytes())?;
    let mut buf = Vec::with_capacity(16 * 1024);
    stream.read_to_end(&mut buf)?;
    let text = String::from_utf8_lossy(&buf);
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .or_else(|| text.split_once("\n\n").map(|(_, b)| b))
        .unwrap_or("");
    Ok(body.to_string())
}

/// 仅编码 URL 不安全字节（RFC 3986 非保留字符保留），供 Everything 查询拼 URL。
fn pct_encode(input: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(input.len());
    for b in input.as_bytes() {
        let unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~');
        if unreserved {
            out.push(*b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0F) as usize] as char);
        }
    }
    out
}

// ─── Everything 响应解析 ───────────────────────────────────────────
#[derive(serde::Deserialize)]
struct RawEntry {
    name: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    date_modified: String,
    #[serde(default)]
    r#type: String,
}

#[derive(serde::Deserialize)]
struct EvResponse {
    #[serde(default)]
    results: Vec<RawEntry>,
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

/// 把 Everything 的 `date_modified` 规范化为 Unix 秒；失败返回 0（不 panic）。
///
/// 真实格式以本机响应为准（常见 `2024-05-30 12:34:56.789` 或 `2024/05/30 ...`）。
/// 日期/时间分隔符统一切分后交给 `chrono` 构造 `NaiveDateTime`（按 UTC 取 timestamp，
/// 仅用于近因加分，时区误差可忽略）。手搓 civil→days 易错，故用成熟库保证正确。
fn parse_everything_date(s: &str) -> i64 {
    let s = s.trim();
    if s.is_empty() {
        return 0;
    }
    let (date_part, time_part) = match s.split_once(' ') {
        Some((d, t)) => (d, t),
        None => (s, "00:00:00"),
    };
    let mut diter = date_part.split(['-', '/']);
    let (yy, mo, dd) = match (diter.next(), diter.next(), diter.next()) {
        (Some(a), Some(b), Some(c)) => (a, b, c),
        _ => return 0,
    };
    let (yy, mo, dd) = match (yy.parse::<i32>(), mo.parse::<u32>(), dd.parse::<u32>()) {
        (Ok(a), Ok(b), Ok(c)) => (a, b, c),
        _ => return 0,
    };
    let tp = time_part.trim();
    let mut titer = tp.split(':');
    let h = titer
        .next()
        .and_then(|x| x.parse::<u32>().ok())
        .unwrap_or(0);
    let mi = titer
        .next()
        .and_then(|x| x.parse::<u32>().ok())
        .unwrap_or(0);
    let sec_raw = titer.next().unwrap_or("0");
    // 秒可能带 .fff 子秒，截断即可（对排序无影响）
    let sec = sec_raw
        .split_once('.')
        .map(|(a, _)| a)
        .unwrap_or(sec_raw)
        .parse::<u32>()
        .unwrap_or(0);
    let date = match NaiveDate::from_ymd_opt(yy, mo, dd) {
        Some(d) => d,
        None => return 0,
    };
    let ndt: NaiveDateTime = match date.and_hms_opt(h, mi, sec) {
        Some(t) => t,
        None => date.and_hms_opt(0, 0, 0).unwrap_or_else(|| {
            NaiveDate::from_ymd_opt(1970, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
        }),
    };
    Utc.from_utc_datetime(&ndt).timestamp()
}

fn now_7days_unix() -> i64 {
    Utc::now().timestamp() - 7 * 24 * 3600
}

/// 目录启发式（best-effort，v0.1）：`type=="folder"` 优先；否则无扩展名且 size==0 粗判。
fn guess_is_dir(name: &str, size: u64, type_field: &str) -> bool {
    if type_field.eq_ignore_ascii_case("folder") {
        return true;
    }
    !name.contains('.') && size == 0
}

/// 解析 Everything JSON 响应体 → `FileEntry` 列表。
///
/// **纯函数（与传输解耦）**：`search()` 只负责取回字符串再交给这里，使解析逻辑
/// 可用 fixture 字符串离线单测，符合 §8.4.2 第 2 条（解析与传输解耦）。
fn parse_response(body: &str) -> anyhow::Result<Vec<FileEntry>> {
    let resp: EvResponse = serde_json::from_str(body).map_err(|e| {
        anyhow::anyhow!(
            "Everything 响应解析失败：{e}（原始：{}）",
            &body[..body.len().min(200)]
        )
    })?;
    Ok(resp
        .results
        .into_iter()
        .map(|r| {
            // 先借用 r.name 计算 is_dir，再把 r.name 移入 FileEntry（避免 use-after-move）
            let is_dir = guess_is_dir(&r.name, r.size, &r.r#type);
            FileEntry {
                name: r.name,
                dir: r.path,
                size: r.size,
                modified: parse_everything_date(&r.date_modified),
                modified_raw: r.date_modified,
                is_dir,
            }
        })
        .collect())
}

fn search(q: &str, limit: usize) -> anyhow::Result<Vec<FileEntry>> {
    let rel = format!(
        "/?search={}&json=1&count={}&path_column=1&size_column=1&date_modified_column=1",
        pct_encode(q),
        limit
    );
    let body = http_get(&rel)?;
    parse_response(&body)
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
        more_commands: None,
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
                "请在 Everything 中启用 HTTP 服务器（端口 8080，仅绑 127.0.0.1）",
                "Enable Everything HTTP server (port 8080, bind 127.0.0.1)",
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
        capabilities: &["host/open_url", "host/show_status"],
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
    let query = params.search_text.as_deref().unwrap_or("").trim();
    if query.is_empty() {
        return GetItemsResult {
            items: vec![hint_item()],
            has_more_items: false,
            is_loading: false,
        };
    }
    if !everything_available() {
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

fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
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
    (
        CommandResult::ShowToast {
            message: tr("未知命令", "Unknown command").to_string(),
            duration_ms: Some(2000),
        },
        Vec::new(),
    )
}

fn main() {
    run(&spec());
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_protocol::model::Sender;

    #[test]
    fn pct_encode_keeps_unreserved_and_encodes_rest() {
        assert_eq!(pct_encode("hello world"), "hello%20world");
        assert_eq!(pct_encode("a+b=c"), "a%2Bb%3Dc");
        assert_eq!(pct_encode("中文"), "%E4%B8%AD%E6%96%87");
        assert_eq!(pct_encode("a-b.c~d_"), "a-b.c~d_");
    }

    #[test]
    fn date_parse_known_formats() {
        // 常见 Everything 格式（秒级 / 带子秒 / 斜杠分隔）应得到同一 Unix 秒
        let t = 1717072496; // 2024-05-30 12:34:56 UTC（1704067200 + 150d + 12:34:56）
        assert_eq!(parse_everything_date("2024-05-30 12:34:56"), t);
        assert_eq!(parse_everything_date("2024-05-30 12:34:56.789"), t);
        assert_eq!(parse_everything_date("2024/05/30 12:34:56"), t);
        // 基准：2024-01-01 00:00:00 UTC
        assert_eq!(parse_everything_date("2024-01-01 00:00:00"), 1704067200);
    }

    #[test]
    fn date_parse_invalid_returns_zero() {
        assert_eq!(parse_everything_date(""), 0);
        assert_eq!(parse_everything_date("not-a-date"), 0);
        assert_eq!(parse_everything_date("2024-13-40"), 0, "非法月日 → 0");
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
        // 连不上时返回引导项；不改 AVAIL 缓存状态之外的全局
        // （CI / 未装 Everything 环境必然走此分支，验证不 panic）
        let r = get_file_items(&GetItemsParams {
            page_id: PAGE_ID.into(),
            search_text: Some("anything".into()),
        });
        assert_eq!(r.items.len(), 1);
        assert!(r.items[0].id == "files.guide" || r.items[0].id == "files.error");
    }

    // ─── T-11 目录判定 ────────────────────────────────────────────
    #[test]
    fn guess_is_dir_folder_priority() {
        // type=="folder" 优先（大小写不敏感），即便有扩展名、size 非 0
        assert!(guess_is_dir("src.txt", 4096, "folder"));
        assert!(guess_is_dir("src", 0, "FOLDER"));
        // 明确的文件类型 → 走启发式，不应误判为目录
        assert!(!guess_is_dir("a.txt", 10, "file"));
    }

    #[test]
    fn guess_is_dir_heuristic_boundary() {
        assert!(guess_is_dir("README", 0, ""), "无扩展名且 size==0 → 目录");
        assert!(!guess_is_dir("README.md", 0, ""), "有扩展名 → 非目录");
        assert!(!guess_is_dir("README", 1, ""), "无扩展名但 size>0 → 非目录");
    }

    // ─── T-09 响应解析（纯函数，fixture 离线，不依赖 Everything）───
    #[test]
    fn parse_response_maps_everything_fields() {
        let json = r#"{"results":[{"type":"file","name":"main.rs","path":"C:\\proj\\src",
            "size":1024,"date_modified":"2024-05-30 12:34:56"}]}"#;
        let entries = parse_response(json).expect("合法响应应解析成功");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "main.rs");
        assert_eq!(entries[0].dir, "C:\\proj\\src");
        assert_eq!(entries[0].size, 1024);
        assert_eq!(entries[0].full_path(), "C:\\proj\\src\\main.rs");
        assert!(!entries[0].is_dir);
        assert_eq!(
            entries[0].modified, 1717072496,
            "date_modified 应为 Unix 秒"
        );
    }

    #[test]
    fn parse_response_folder_type_marks_is_dir() {
        let json = r#"{"results":[{"type":"folder","name":"src","path":"C:\\proj",
            "size":0,"date_modified":""}]}"#;
        let entries = parse_response(json).expect("应解析成功");
        assert!(entries[0].is_dir, "type=folder 应判定为目录");
        assert_eq!(entries[0].modified, 0, "空日期应为 0 而非 panic");
    }

    #[test]
    fn parse_response_malformed_returns_err() {
        assert!(parse_response("").is_err(), "空串应返回 Err");
        assert!(parse_response("not json").is_err(), "非法 JSON 应返回 Err");
        assert!(
            parse_response("{}").is_ok(),
            "缺 results 字段视为合法（serde default → 空列表）"
        );
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
        // 引导项/占位项（files.hint/guide/error）当前落回通用 Toast——属 §7.4 已知边界，
        // 此处锁定「不 panic、有明确反馈、无副作用」这一不变量
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
