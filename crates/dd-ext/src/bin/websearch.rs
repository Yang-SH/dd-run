//! dd-ext-websearch —— 内置「网络搜索」扩展（`com.ddrun.websearch`，✅ 跨平台）。
//!
//! 功能（M4 A10 核对点：WebSearch 开 URL）：
//! - **顶层**：每个搜索引擎一条入口命令（无关键词时打开引擎主页/搜索页，
//!   用于本轮真机验证 `host/open_url` 链路）；
//! - **兜底**（§6.2）：每引擎一条模板命令 `websearch.<engine>.query`，
//!   `title = "在 <引擎> 搜索 {query}"`——宿主渲染替换 `{query}`；
//! - **invoke**：按命令 id 定位引擎 → 对 `context.query` 做 URL 百分号编码 →
//!   拼搜索 URL → 发 `host/open_url` 请求（§7.4，capabilities 已声明）→ `Dismiss`。
//!
//! **可配置引擎**（2026-09-05）：宿主 spawn 前注入环境变量
//! `DD_WEBSEARCH_ENGINES`（JSON 数组 `[{"name":"...","template":"https://…{q}"}]`，
//! 见 dd-gui `Settings::search_engines_env`）——扩展按其构建引擎表；未注入 /
//! 解析失败 / 全部条目非法时回落内置默认 5 引擎。配置走进程环境
//! （manifest `entry.env` 既有机制），协议 v1.0 冻结零字段新增。
//!
//! 编码为手写 RFC 3986 percent-encode（UTF-8），无第三方依赖。
//! 参考实现：[`docs/m4-record.md`](../../docs/m4-record.md) P4 决策（扩展侧先行）。

use dd_ext::{i18n::tr, run, Effect, ExtensionSpec};
use dd_protocol::messages::InvokeParams;
use dd_protocol::model::{CommandItem, CommandRef, CommandResult, Icon, IconKind};

/// 单个搜索引擎（运行时持有；来源 = `DD_WEBSEARCH_ENGINES` 配置或内置默认）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Engine {
    /// 命令 id 后缀（`websearch.<suffix>` 与 `websearch.<suffix>.query`），
    /// 由展示名 slug 化 + 去重保证唯一。
    suffix: String,
    name: String,
    /// 主页（顶层无关键词时打开；由模板推导 scheme://host）
    home: String,
    /// 搜索模板（含 `{q}`）
    search: String,
}

impl Engine {
    /// 校验并构造：name 非空、template 含 `{q}` 且以 `http(s)://` 开头。
    fn new(name: &str, template: &str) -> Option<Self> {
        let name = name.trim();
        let template = template.trim();
        if name.is_empty()
            || !template.contains("{q}")
            || !(template.starts_with("http://") || template.starts_with("https://"))
        {
            return None;
        }
        Some(Self {
            suffix: slugify(name),
            name: name.to_string(),
            home: home_of(template),
            search: template.to_string(),
        })
    }
}

/// 内置默认引擎表（与 dd-gui `settings.rs::preset_search_engines` 保持一致；
/// 两侧各自定义——本表是环境变量缺失时的回落值）。
fn builtin_engines() -> Vec<Engine> {
    [
        ("google", "Google", "https://www.google.com/search?q={q}"),
        ("bing", "Bing", "https://www.bing.com/search?q={q}"),
        ("baidu", "Baidu", "https://www.baidu.com/s?wd={q}"),
        ("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q={q}"),
        ("github", "GitHub", "https://github.com/search?q={q}"),
    ]
    .iter()
    .map(|(suffix, name, search)| Engine {
        suffix: (*suffix).to_string(),
        name: (*name).to_string(),
        home: home_of(search),
        search: (*search).to_string(),
    })
    .collect()
}

/// 生效引擎表：宿主注入的 `DD_WEBSEARCH_ENGINES` 优先，否则内置默认。
/// 每次调用现读现解析（进程内配置恒定，开销可忽略）。
fn active_engines() -> Vec<Engine> {
    std::env::var("DD_WEBSEARCH_ENGINES")
        .ok()
        .and_then(|text| engines_from_json(&text))
        .unwrap_or_else(builtin_engines)
}

/// 解析宿主注入的引擎配置 JSON（`[{"name","template"}]`）：
/// 非法条目跳过、suffix 去重；数组为空或全部非法 → `None`（回落默认）。
fn engines_from_json(text: &str) -> Option<Vec<Engine>> {
    let value = serde_json::from_str::<serde_json::Value>(text).ok()?;
    let arr = value.as_array()?;
    let engines: Vec<Engine> = arr
        .iter()
        .filter_map(|e| {
            Engine::new(
                e.get("name").and_then(|x| x.as_str())?,
                e.get("template").and_then(|x| x.as_str())?,
            )
        })
        .collect();
    if engines.is_empty() {
        return None;
    }
    Some(dedupe_suffixes(engines))
}

/// suffix 去重：重名/同名 slug 依序追加 `-2`、`-3` …，保证命令 id 唯一。
fn dedupe_suffixes(mut engines: Vec<Engine>) -> Vec<Engine> {
    let mut used: Vec<String> = Vec::new();
    for e in &mut engines {
        let base = e.suffix.clone();
        let mut n = 1u32;
        loop {
            let cand = if n == 1 {
                base.clone()
            } else {
                format!("{base}-{n}")
            };
            if !used.contains(&cand) {
                used.push(cand.clone());
                e.suffix = cand;
                break;
            }
            n += 1;
        }
    }
    engines
}

/// 展示名 → 命令 id slug：小写、ASCII 字母数字保留、空白与 `-_.` 归一为
/// `-`、其余字符（CJK 等）丢弃；结果为空 → `engine`。
/// 例：`DuckDuckGo` → `duckduckgo`、`Stack Overflow` → `stack-overflow`。
fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if (c == ' ' || c == '-' || c == '_' || c == '.') && !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let slug = out.trim_matches('-').to_string();
    if slug.is_empty() {
        "engine".to_string()
    } else {
        slug
    }
}

/// 由搜索模板推导主页 URL：取 `{q}` 前缀、剥掉尾部的 `/ ? & =`，再截到
/// `scheme://host`。例：`https://www.google.com/search?q={q}` →
/// `https://www.google.com`。
fn home_of(template: &str) -> String {
    let base = template.split("{q}").next().unwrap_or(template);
    // origin = scheme://host：从 authority 之后遇到路径/查询/片段分隔符即止。
    let start = base.find("://").map(|i| i + 3).unwrap_or(0);
    match base[start..].find(['/', '?', '#']) {
        Some(j) => base[..start + j].to_string(),
        None => base.to_string(),
    }
}

fn main() {
    run(&spec());
}

fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: "com.ddrun.websearch",
        display_name: tr("网络搜索", "Web Search"),
        description: tr(
            "在配置的搜索引擎（默认 Google / Bing / Baidu / DuckDuckGo / GitHub）中搜索",
            "Search with the configured engines (defaults: Google / Bing / Baidu / DuckDuckGo / GitHub)",
        ),
        frozen: true,
        has_fallback: true,
        capabilities: &["host/open_url"],
        log_tag: "dd-ext-websearch",
        top_level: top_level_commands,
        fallback: Some(fallback_commands),
        invoke: handle_invoke,
    }
}

/// 顶层：每引擎一条入口（无关键词 → 打开主页；subtitle 提示兜底用法）。
fn top_level_commands() -> Vec<CommandItem> {
    active_engines()
        .iter()
        .map(|e| CommandItem {
            id: format!("websearch.{}", e.suffix),
            title: format!("Search with {}", e.name),
            subtitle: Some(
                tr(
                    "打开 {name}（输入关键词后选「在 {name} 搜索 …」结果项）",
                    "Open {name} (type a query, then pick “Search in {name} …”)",
                )
                .replace("{name}", &e.name),
            ),
            icon: Some(Icon {
                kind: IconKind::Glyph,
                value: "\u{E721}".to_string(), // Search
            }),
            section: Some(tr("网络搜索", "Web Search").to_string()),
            tags: Some(vec!["search".to_string(), e.suffix.clone()]),
            details: None,
            text_to_suggest: None,
            more_commands: None,
            command: CommandRef::Invoke,
        })
        .collect()
}

/// 兜底：每引擎一条模板命令（`title` 含 `{query}`，宿主渲染时替换）。
fn fallback_commands() -> Vec<CommandItem> {
    active_engines()
        .iter()
        .map(|e| CommandItem {
            id: format!("websearch.{}.query", e.suffix),
            title: tr("在 {name} 搜索 {query}", "Search {name} for {query}")
                .replace("{name}", &e.name),
            subtitle: Some(tr("{name} 搜索", "{name} Search").replace("{name}", &e.name)),
            icon: Some(Icon {
                kind: IconKind::Glyph,
                value: "\u{E721}".to_string(),
            }),
            section: Some(tr("网络搜索", "Web Search").to_string()),
            tags: None,
            details: None,
            text_to_suggest: None,
            more_commands: None,
            command: CommandRef::Invoke,
        })
        .collect()
}

/// invoke 分发：`websearch.<suffix>`（无 query → 主页）与
/// `websearch.<suffix>.query`（query → 搜索 URL）。
fn handle_invoke(params: &InvokeParams) -> (CommandResult, Vec<Effect>) {
    let query = params
        .context
        .as_ref()
        .and_then(|c| c.query.as_deref())
        .unwrap_or("")
        .trim();

    // 解析命令 id → 引擎
    let id = params.id.as_str();
    let engines = active_engines();
    let engine = engines.iter().find(|e| {
        id == format!("websearch.{}", e.suffix) || id == format!("websearch.{}.query", e.suffix)
    });
    let Some(engine) = engine else {
        return (
            CommandResult::ShowToast {
                message: tr("未知搜索命令：{id}", "Unknown search command: {id}")
                    .replace("{id}", id),
                duration_ms: Some(2_500),
            },
            Vec::new(),
        );
    };

    if query.is_empty() {
        // 顶层入口无关键词 → 打开主页
        return (
            CommandResult::KeepOpen,
            vec![Effect::HostRequest {
                method: "host/open_url",
                params: serde_json::json!({ "url": engine.home }),
            }],
        );
    }
    let url = engine.search.replace("{q}", &encode_query_component(query));
    (
        // 打开搜索结果后关闭面板
        CommandResult::Dismiss,
        vec![Effect::HostRequest {
            method: "host/open_url",
            params: serde_json::json!({ "url": url }),
        }],
    )
}

/// RFC 3986 百分号编码（UTF-8 字节；保留 `-_.~` 与非保留字符）。
pub fn encode_query_component(input: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use dd_protocol::model::Sender;

    fn invoke(id: &str, query: &str) -> InvokeParams {
        InvokeParams {
            id: id.to_string(),
            sender: Sender::TopLevel,
            context: Some(dd_protocol::messages::InvokeContext {
                query: (!query.is_empty()).then(|| query.to_string()),
                selected_item_id: None,
                form_data: None,
                confirmed: None,
            }),
        }
    }

    fn open_url(effects: &[Effect]) -> String {
        for e in effects {
            if let Effect::HostRequest { method, params } = e {
                assert_eq!(*method, "host/open_url");
                return params["url"].as_str().unwrap().to_string();
            }
        }
        panic!("缺少 host/open_url 副作用");
    }

    #[test]
    fn encode_keeps_unreserved_and_encodes_rest() {
        assert_eq!(encode_query_component("hello world"), "hello%20world");
        assert_eq!(encode_query_component("a+b=c"), "a%2Bb%3Dc");
        assert_eq!(encode_query_component("中文"), "%E4%B8%AD%E6%96%87");
        assert_eq!(encode_query_component("a-b.c~d_"), "a-b.c~d_");
    }

    #[test]
    fn fallback_query_builds_search_url() {
        let (result, effects) =
            handle_invoke(&invoke("websearch.google.query", "rust command palette"));
        assert_eq!(result, CommandResult::Dismiss);
        let url = open_url(&effects);
        assert_eq!(
            url,
            "https://www.google.com/search?q=rust%20command%20palette"
        );
    }

    #[test]
    fn baidu_uses_wd_and_encodes_cjk() {
        let (_, effects) = handle_invoke(&invoke("websearch.baidu.query", "预算系统"));
        assert_eq!(
            open_url(&effects),
            "https://www.baidu.com/s?wd=%E9%A2%84%E7%AE%97%E7%B3%BB%E7%BB%9F"
        );
    }

    #[test]
    fn top_level_without_query_opens_home() {
        let (result, effects) = handle_invoke(&invoke("websearch.github", ""));
        assert_eq!(result, CommandResult::KeepOpen);
        assert_eq!(open_url(&effects), "https://github.com");
    }

    #[test]
    fn top_level_with_query_searches() {
        let (_, effects) = handle_invoke(&invoke("websearch.bing", "dd-run"));
        assert_eq!(open_url(&effects), "https://www.bing.com/search?q=dd-run");
    }

    #[test]
    fn unknown_id_reports_toast() {
        let (result, effects) = handle_invoke(&invoke("websearch.nope.query", "x"));
        assert!(matches!(result, CommandResult::ShowToast { .. }));
        assert!(effects.is_empty());
    }

    #[test]
    fn fallback_catalog_has_query_placeholder() {
        let cmds = fallback_commands();
        assert_eq!(cmds.len(), active_engines().len());
        assert!(
            cmds.iter().all(|c| c.title.contains("{query}")),
            "兜底模板必须含 {{query}} 占位"
        );
        // 顶层与兜底命令集 = 2 × 引擎数
        assert_eq!(top_level_commands().len(), active_engines().len());
    }

    // ── 可配置引擎（DD_WEBSEARCH_ENGINES）──

    #[test]
    fn env_json_overrides_engine_table() {
        let engines = engines_from_json(
            r#"[{"name":"Stack Overflow","template":"https://stackoverflow.com/search?q={q}"}]"#,
        )
        .expect("合法配置");
        assert_eq!(engines.len(), 1);
        assert_eq!(engines[0].suffix, "stack-overflow");
        assert_eq!(engines[0].name, "Stack Overflow");
        assert_eq!(engines[0].home, "https://stackoverflow.com");
        assert_eq!(engines[0].search, "https://stackoverflow.com/search?q={q}");
    }

    #[test]
    fn env_json_invalid_entries_skipped_empty_falls_back() {
        // 非法条目跳过；合法的留下
        let engines = engines_from_json(
            r#"[
                {"name":"Good","template":"https://a.com/?q={q}"},
                {"name":"NoQ","template":"https://b.com/"},
                {"template":"https://c.com/?q={q}"},
                {"name":"Ftp","template":"ftp://d.com/?q={q}"}
            ]"#,
        )
        .expect("存在合法条目");
        assert_eq!(engines.len(), 1);
        assert_eq!(engines[0].suffix, "good");
        // 全部非法 / 空数组 / 非法 JSON → None（回落内置默认）
        assert!(engines_from_json(r#"[{"name":"NoQ","template":"https://b.com/"}]"#).is_none());
        assert!(engines_from_json("[]").is_none());
        assert!(engines_from_json("not json").is_none());
    }

    #[test]
    fn env_json_duplicate_names_get_unique_suffixes() {
        let engines = engines_from_json(
            r#"[
                {"name":"My Engine","template":"https://a.com/?q={q}"},
                {"name":"my engine","template":"https://b.com/?q={q}"},
                {"name":"我的引擎","template":"https://c.com/?q={q}"}
            ]"#,
        )
        .expect("合法配置");
        let suffixes: Vec<&str> = engines.iter().map(|e| e.suffix.as_str()).collect();
        assert_eq!(suffixes, vec!["my-engine", "my-engine-2", "engine"]);
        // 命令 id 唯一（顶层 + 兜底均由 suffix 构造）
        let mut ids: Vec<String> = engines
            .iter()
            .flat_map(|e| {
                [
                    format!("websearch.{}", e.suffix),
                    format!("websearch.{}.query", e.suffix),
                ]
            })
            .collect();
        ids.sort();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "suffix 去重后命令 id 不得冲突");
    }

    #[test]
    fn home_is_derived_from_template_host() {
        assert_eq!(
            home_of("https://www.google.com/search?q={q}"),
            "https://www.google.com"
        );
        assert_eq!(home_of("https://example.com?q={q}"), "https://example.com");
        assert_eq!(home_of("https://example.com/{q}"), "https://example.com");
        assert_eq!(home_of("https://example.com?q={q}"), "https://example.com");
    }

    #[test]
    fn slugify_rules() {
        assert_eq!(slugify("DuckDuckGo"), "duckduckgo");
        assert_eq!(slugify("Stack Overflow"), "stack-overflow");
        assert_eq!(slugify("  Bing  "), "bing");
        assert_eq!(slugify("百度"), "engine", "全 CJK 名回落 engine");
        assert_eq!(slugify("Wiki 搜索"), "wiki");
    }

    #[test]
    fn builtin_defaults_are_five_valid_engines() {
        let engines = builtin_engines();
        assert_eq!(engines.len(), 5);
        for e in &engines {
            assert!(!e.suffix.is_empty());
            assert!(e.home.starts_with("https://"));
            assert!(e.search.contains("{q}"));
        }
        // 内置 suffix 与 dd-gui preset 名的 slug 一致（双侧表对齐哨兵）
        let suffixes: Vec<&str> = engines.iter().map(|e| e.suffix.as_str()).collect();
        assert_eq!(
            suffixes,
            vec!["google", "bing", "baidu", "duckduckgo", "github"]
        );
    }
}
