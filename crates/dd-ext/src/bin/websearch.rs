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
//! 编码为手写 RFC 3986 percent-encode（UTF-8），无第三方依赖。
//! 参考实现：[`docs/m4-record.md`](../../docs/m4-record.md) P4 决策（扩展侧先行）。

use dd_ext::{run, Effect, ExtensionSpec};
use dd_protocol::messages::InvokeParams;
use dd_protocol::model::{CommandItem, CommandRef, CommandResult, Icon, IconKind};

/// 内置引擎表：id 后缀 / 展示名 / 搜索 URL 模板（`{q}` 替换为编码后的关键词）。
struct Engine {
    /// id 后缀（`websearch.<suffix>` 与 `websearch.<suffix>.query`）
    suffix: &'static str,
    name: &'static str,
    /// 主页（顶层无关键词时打开）
    home: &'static str,
    /// 搜索模板（含 `{q}`）
    search: &'static str,
}

const ENGINES: &[Engine] = &[
    Engine {
        suffix: "google",
        name: "Google",
        home: "https://www.google.com",
        search: "https://www.google.com/search?q={q}",
    },
    Engine {
        suffix: "bing",
        name: "Bing",
        home: "https://www.bing.com",
        search: "https://www.bing.com/search?q={q}",
    },
    Engine {
        suffix: "baidu",
        name: "Baidu",
        home: "https://www.baidu.com",
        search: "https://www.baidu.com/s?wd={q}",
    },
    Engine {
        suffix: "duckduckgo",
        name: "DuckDuckGo",
        home: "https://duckduckgo.com",
        search: "https://duckduckgo.com/?q={q}",
    },
    Engine {
        suffix: "github",
        name: "GitHub",
        home: "https://github.com",
        search: "https://github.com/search?q={q}",
    },
];

fn main() {
    run(&spec());
}

fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: "com.ddrun.websearch",
        display_name: "Web Search",
        description: "在 Google / Bing / Baidu / DuckDuckGo / GitHub 中搜索",
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
    ENGINES
        .iter()
        .map(|e| CommandItem {
            id: format!("websearch.{}", e.suffix),
            title: format!("Search with {}", e.name),
            subtitle: Some(format!(
                "打开 {}（输入关键词后选「在 {} 搜索 …」结果项）",
                e.name, e.name
            )),
            icon: Some(Icon {
                kind: IconKind::Glyph,
                value: "\u{E721}".to_string(), // Search
            }),
            section: Some("网络搜索".to_string()),
            tags: Some(vec!["search".to_string(), e.suffix.to_string()]),
            details: None,
            text_to_suggest: None,
            more_commands: None,
            command: CommandRef::Invoke,
        })
        .collect()
}

/// 兜底：每引擎一条模板命令（`title` 含 `{query}`，宿主渲染时替换）。
fn fallback_commands() -> Vec<CommandItem> {
    ENGINES
        .iter()
        .map(|e| CommandItem {
            id: format!("websearch.{}.query", e.suffix),
            title: format!("在 {} 搜索 {{query}}", e.name),
            subtitle: Some(format!("{} 搜索", e.name)),
            icon: Some(Icon {
                kind: IconKind::Glyph,
                value: "\u{E721}".to_string(),
            }),
            section: Some("网络搜索".to_string()),
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
    let engine = ENGINES.iter().find(|e| {
        id == format!("websearch.{}", e.suffix) || id == format!("websearch.{}.query", e.suffix)
    });
    let Some(engine) = engine else {
        return (
            CommandResult::ShowToast {
                message: format!("未知搜索命令：{id}"),
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
        assert_eq!(cmds.len(), ENGINES.len());
        assert!(
            cmds.iter().all(|c| c.title.contains("{query}")),
            "兜底模板必须含 {{query}} 占位"
        );
        // 顶层与兜底命令集 = 2 × 引擎数
        assert_eq!(top_level_commands().len(), ENGINES.len());
    }
}
