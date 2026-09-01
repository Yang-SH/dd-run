//! 协议一致性测试：`docs/protocol.md` 的每一条 ```json 示例都必须能被
//! `dd-protocol` 的类型正确反序列化，且关键字段值与文档描述一致。
//!
//! 用例来源 = 文档本体（运行时抽取，SSOT 永不漂移）：
//! - 以 `### N.M` 标题定位每个示例所属章节，按 (章节, 章节内序号) 映射到具体类型；
//! - 文档新增/删除示例会使本测试失败——这是**有意的**：强制同步更新映射表，
//!   使文档与实现永远一致（对应 implementation.md M0 完成判据）。

use std::path::Path;
use std::sync::OnceLock;

use dd_protocol::framing;
use dd_protocol::messages::*;
use dd_protocol::model::*;
use serde::Deserialize;
use serde_json::Value;

// ─── 示例抽取 ───────────────────────────────────────────────

struct Block {
    /// 所属 `### N.M` 章节号
    section: String,
    /// 章节内第几个 json 围栏（从 0 起）
    index: usize,
    text: String,
    /// 在 protocol.md 中的行号（失败报告用）
    line: usize,
}

/// 预期示例总数：protocol.md 的 ```json 围栏数。
/// 若文档增删示例，请同步更新 [CASES] 映射并修改此数。
const EXPECTED_BLOCK_COUNT: usize = 46;

fn extract_blocks() -> &'static Vec<Block> {
    static BLOCKS: OnceLock<Vec<Block>> = OnceLock::new();
    BLOCKS.get_or_init(|| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/protocol.md");
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("读取协议文档失败（{:?}）: {e}", path));

        let mut blocks = Vec::new();
        let mut section = String::new();
        let mut counts: std::collections::HashMap<String, usize> = Default::default();
        let mut in_json = false;
        let mut buf = String::new();
        let mut start_line = 0usize;
        for (i, line) in src.lines().enumerate() {
            if !in_json {
                if let Some(rest) = line.strip_prefix("### ") {
                    if let Some(num) = rest.split(|c: char| !c.is_ascii_digit() && c != '.').next()
                    {
                        if !num.is_empty() {
                            section = num.to_string();
                        }
                    }
                } else if line.trim_start().starts_with("```json") {
                    in_json = true;
                    buf.clear();
                    start_line = i + 1; // 供失败报告用的 1-based 行号
                }
                continue;
            }
            // in_json：累积围栏内容直到闭合围栏
            if line.trim_start().starts_with("```") {
                in_json = false;
                let idx = counts.get(&section).copied().unwrap_or(0);
                counts.insert(section.clone(), idx + 1);
                blocks.push(Block {
                    section: section.clone(),
                    index: idx,
                    text: buf.trim().to_string(),
                    line: start_line + 1,
                });
            } else {
                buf.push_str(line);
                buf.push('\n');
            }
        }
        blocks
    })
}

// ─── 断言辅助 ───────────────────────────────────────────────

// 以下三个信封结构：字段本身（jsonrpc/id 的存在性与形态）就是断言对象，
// 部分字段无需显式读取，故允许 dead_code。
#[allow(dead_code)]
#[derive(Deserialize)]
struct Req<P> {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Option<P>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct Notif<P> {
    jsonrpc: String,
    method: String,
    params: Option<P>,
}

/// 成功响应信封（§3.1：响应 = jsonrpc + id + result）。
#[allow(dead_code)]
#[derive(Deserialize)]
struct Resp<P> {
    jsonrpc: String,
    id: u64,
    result: P,
}

fn parse<T: for<'de> Deserialize<'de>>(b: &Block, label: &str) -> T {
    serde_json::from_str(&b.text).unwrap_or_else(|e| {
        panic!(
            "§{} 示例#{}（{}）反序列化失败: {e}\n原文: {}",
            b.section,
            b.index + 1,
            label,
            b.text
        )
    })
}

fn parse_value(b: &Block) -> Value {
    serde_json::from_str(&b.text)
        .unwrap_or_else(|e| panic!("§{} 示例#{} 不是合法 JSON: {e}", b.section, b.index + 1))
}

// ─── 主测试 ─────────────────────────────────────────────────

#[test]
fn example_count_matches_mapping() {
    let blocks = extract_blocks();
    assert_eq!(
        blocks.len(),
        EXPECTED_BLOCK_COUNT,
        "docs/protocol.md 的 ```json 示例数发生变化 → 请同步更新本测试的 (章节, 序号) 映射表（对应 §13 协议演进）。\n实际: {:?}",
        blocks.iter().map(|b| (b.section.clone(), b.index)).collect::<Vec<_>>()
    );
}

#[test]
fn every_example_deserializes_to_typed_contract() {
    let blocks = extract_blocks();
    for b in blocks {
        check(b);
    }
}

fn check(b: &Block) {
    match (b.section.as_str(), b.index) {
        // §3.1 三种消息（弱类型结构校验）
        ("3.1", 0) => {
            let r: Req<EmptyParams> = parse(b, "请求");
            assert_eq!(r.jsonrpc, "2.0");
            assert_eq!(r.method, "top_level_commands");
            assert!(r.params.is_some());
        }
        ("3.1", 1) => {
            let v = parse_value(b);
            assert_eq!(v["jsonrpc"], "2.0");
            assert_eq!(v["result"]["commands"], Value::Array(vec![]));
            assert!(v.get("error").is_none());
        }
        ("3.1", 2) => {
            let v = parse_value(b);
            let e: RpcError = serde_json::from_value(v["error"].clone()).unwrap();
            assert_eq!(e.code, error_codes::METHOD_NOT_FOUND);
            assert_eq!(e.data.as_ref().unwrap()["method"], "nope");
        }
        ("3.1", 3) => {
            let v = parse_value(b);
            // §3.2/§3.3：通知不得携带 id 字段
            assert!(v.get("id").is_none(), "通知含 id 字段，违反 §3.2");
            let n: Notif<ItemsChangedParams> = parse(b, "通知");
            assert_eq!(n.method, "items_changed");
            assert_eq!(n.params.unwrap().page_id.as_deref(), Some("calc.history"));
        }

        // §5.1 initialize
        ("5.1", 0) => {
            let r: Req<InitializeParams> = parse(b, "initialize 请求");
            assert_eq!(r.method, "initialize");
            let p = r.params.unwrap();
            assert_eq!(p.protocol_version, "1.0");
            assert_eq!(p.host.name, "dd-run");
            assert_eq!(p.host.platform, "windows");
            assert_eq!(p.transport.framing, "ndjson");
            assert_eq!(p.transport.max_message_bytes, 1_048_576);
            assert_eq!(p.capabilities.len(), 3);
            assert_eq!(p.locale.as_deref(), Some("zh-CN"));
        }
        ("5.1", 1) => {
            let r: Resp<InitializeResult> = parse(b, "initialize 成功响应");
            let r = r.result;
            assert_eq!(r.protocol_version, "1.0");
            assert_eq!(r.provider.id, "com.example.calc");
            assert!(r.provider.frozen);
            assert!(!r.provider.has_fallback);
            assert!(r.capabilities.is_empty());
            assert_eq!(r.timeouts.and_then(|t| t.get_items_ms), Some(2000));
        }
        ("5.1", 2) => {
            let v = parse_value(b);
            let e: RpcError = serde_json::from_value(v["error"].clone()).unwrap();
            assert_eq!(e.code, error_codes::VERSION_MISMATCH);
            assert_eq!(e.data.as_ref().unwrap()["supported_versions"][0], "1.0");
        }

        // §5.2 initialized 通知
        ("5.2", 0) => {
            let v = parse_value(b);
            assert!(
                v.get("id").is_none(),
                "initialized 是通知，不得携带 id（§3.2）"
            );
            let n: Notif<EmptyParams> = parse(b, "initialized 通知");
            assert_eq!(n.method, "initialized");
        }

        // §6.1 top_level_commands
        ("6.1", 0) => {
            let r: Req<EmptyParams> = parse(b, "top_level_commands 请求");
            assert_eq!(r.method, "top_level_commands");
        }
        ("6.1", 1) => {
            let r: Resp<CommandListResult> = parse(b, "top_level_commands 响应");
            let c = &r.result.commands[0];
            assert_eq!(c.id, "calc.eval");
            assert_eq!(c.icon.as_ref().unwrap().kind, IconKind::Glyph);
            assert_eq!(c.icon.as_ref().unwrap().value, "\u{E8C8}");
            assert_eq!(c.tags.as_ref().unwrap(), &["math".to_string()][..]);
            assert_eq!(c.command, CommandRef::Invoke);
            assert_eq!(c.text_to_suggest.as_deref(), Some("calc "));
        }

        // §6.2 fallback_commands
        ("6.2", 0) => {
            let r: Req<EmptyParams> = parse(b, "fallback_commands 请求");
            assert_eq!(r.method, "fallback_commands");
        }
        ("6.2", 1) => {
            let r: Resp<CommandListResult> = parse(b, "fallback_commands 响应");
            assert_eq!(r.result.commands[0].title, "Calculate “{query}”");
        }

        // §6.3 get_items
        ("6.3", 0) => {
            let r: Req<GetItemsParams> = parse(b, "get_items 请求");
            let p = r.params.unwrap();
            assert_eq!(p.page_id, "calc.history");
            assert_eq!(p.search_text.as_deref(), Some("3.14"));
        }
        ("6.3", 1) => {
            let r: Resp<GetItemsResult> = parse(b, "get_items 响应");
            let r = r.result;
            assert_eq!(r.items.len(), 1);
            assert_eq!(r.items[0].id, "h1");
            assert!(!r.has_more_items);
            assert!(!r.is_loading);
        }

        // §6.4 get_command（含 command:null 正常结果）
        ("6.4", 0) => {
            let r: Req<GetCommandParams> = parse(b, "get_command 请求");
            assert_eq!(r.params.unwrap().id, "calc.eval");
        }
        ("6.4", 1) => {
            let r: Resp<GetCommandResult> = parse(b, "get_command 响应");
            assert_eq!(r.result.command.as_ref().unwrap().title, "Calculator");
        }
        ("6.4", 2) => {
            let r: Resp<GetCommandResult> = parse(b, "get_command 空结果");
            assert!(
                r.result.command.is_none(),
                "§6.4：找不到时 command 为 null，不是错误"
            );
        }

        // §6.5 invoke
        ("6.5", 0) => {
            let r: Req<InvokeParams> = parse(b, "invoke 请求");
            let p = r.params.unwrap();
            assert_eq!(p.id, "calc.eval");
            assert_eq!(p.sender, Sender::TopLevel);
            assert_eq!(p.context.as_ref().unwrap().query.as_deref(), Some("1+1"));
        }
        ("6.5", 1) => {
            let r: Resp<InvokeResult> = parse(b, "invoke 响应");
            assert_eq!(
                r.result.command_result,
                CommandResult::ShowToast {
                    message: "= 2".into(),
                    duration_ms: Some(2000)
                }
            );
        }

        // §6.6 close
        ("6.6", 0) => {
            let r: Req<EmptyParams> = parse(b, "close 请求");
            assert_eq!(r.method, "close");
        }
        ("6.6", 1) => {
            let _: Resp<EmptyResult> = parse(b, "close 响应");
        }

        // §7.1 items_changed 通知
        ("7.1", 0) => {
            let v = parse_value(b);
            assert!(
                v.get("id").is_none(),
                "items_changed 是通知，不得携带 id（§3.2）"
            );
            let n: Notif<ItemsChangedParams> = parse(b, "items_changed 通知");
            assert_eq!(n.params.unwrap().page_id.as_deref(), Some("calc.history"));
        }

        // §7.2 host/show_status
        ("7.2", 0) => {
            let r: Req<ShowStatusParams> = parse(b, "host/show_status 请求");
            let p = r.params.unwrap();
            assert_eq!(p.message, "Copied to clipboard");
            assert_eq!(p.state, Some(StatusState::Success));
            assert_eq!(p.duration_ms, Some(2000));
        }
        ("7.2", 1) => {
            let _: Resp<EmptyResult> = parse(b, "host/show_status 响应");
        }

        // §7.3 host/set_clipboard
        ("7.3", 0) => {
            let r: Req<SetClipboardParams> = parse(b, "host/set_clipboard 请求");
            assert_eq!(r.params.unwrap().text, "3.14159");
        }
        ("7.3", 1) => {
            let _: Resp<EmptyResult> = parse(b, "host/set_clipboard 响应");
        }

        // §7.4 host/open_url
        ("7.4", 0) => {
            let r: Req<OpenUrlParams> = parse(b, "host/open_url 请求");
            assert_eq!(r.params.unwrap().url, "https://example.com/search?q=dd-run");
        }
        ("7.4", 1) => {
            let _: Resp<EmptyResult> = parse(b, "host/open_url 响应");
        }

        // §8.1 CommandItem 全字段示例
        ("8.1", 0) => {
            let c: CommandItem = parse(b, "CommandItem");
            assert_eq!(c.id, "calc.eval");
            assert_eq!(c.tags.as_ref().unwrap().len(), 2);
            assert_eq!(c.details.as_ref().unwrap().title, "Calculator");
            let more = c.more_commands.as_ref().unwrap();
            assert_eq!(more.len(), 1);
            assert_eq!(more[0].id, "calc.copy");
            assert_eq!(c.command, CommandRef::Invoke);
        }

        // §8.2 CommandRef
        ("8.2", 0) => {
            let r: CommandRef = parse(b, "CommandRef invoke");
            assert_eq!(r, CommandRef::Invoke);
        }
        ("8.2", 1) => {
            let r: CommandRef = parse(b, "CommandRef page");
            assert_eq!(
                r,
                CommandRef::Page {
                    page_id: "calc.history".into()
                }
            );
        }

        // §8.3 CommandResult 全 8 种 Kind（验收 A4）
        ("8.3", 0) => assert_eq!(parse::<CommandResult>(b, "Dismiss"), CommandResult::Dismiss),
        ("8.3", 1) => assert_eq!(parse::<CommandResult>(b, "GoHome"), CommandResult::GoHome),
        ("8.3", 2) => assert_eq!(parse::<CommandResult>(b, "GoBack"), CommandResult::GoBack),
        ("8.3", 3) => assert_eq!(parse::<CommandResult>(b, "Hide"), CommandResult::Hide),
        ("8.3", 4) => assert_eq!(
            parse::<CommandResult>(b, "KeepOpen"),
            CommandResult::KeepOpen
        ),
        ("8.3", 5) => assert_eq!(
            parse::<CommandResult>(b, "GoToPage"),
            CommandResult::GoToPage {
                page_id: "calc.history".into()
            }
        ),
        ("8.3", 6) => assert_eq!(
            parse::<CommandResult>(b, "ShowToast"),
            CommandResult::ShowToast {
                message: "Copied to clipboard".into(),
                duration_ms: Some(2000)
            }
        ),
        ("8.3", 7) => assert_eq!(
            parse::<CommandResult>(b, "Confirm"),
            CommandResult::Confirm {
                title: "Delete entry?".into(),
                description: "This cannot be undone.".into(),
                confirm_label: "Delete".into(),
                is_critical: true,
            }
        ),

        // §8.5 Page 元信息
        ("8.5", 0) => {
            let p: PageInfo = parse(b, "PageInfo");
            assert_eq!(p.kind, PageKind::List);
            assert_eq!(p.page_id, "calc.history");
            assert_eq!(p.show_details, Some(true));
            assert_eq!(p.grid.as_ref().unwrap().columns, Some(4));
        }

        // §8.6 Icon 三态
        ("8.6", 0) => {
            let i: Icon = parse(b, "Icon glyph");
            assert_eq!(i.kind, IconKind::Glyph);
            assert_eq!(i.value, "\u{E8C8}");
        }
        ("8.6", 1) => {
            let i: Icon = parse(b, "Icon path");
            assert_eq!(i.kind, IconKind::Path);
        }
        ("8.6", 2) => {
            let i: Icon = parse(b, "Icon url");
            assert_eq!(i.kind, IconKind::Url);
        }

        // §8.7 Details / EmptyContent
        ("8.7", 0) => {
            let d: Details = parse(b, "Details");
            assert_eq!(d.metadata.as_ref().unwrap()[0].key, "Version");
        }
        ("8.7", 1) => {
            let e: EmptyContent = parse(b, "EmptyContent");
            assert_eq!(e.title, "No results");
            assert_eq!(e.icon.as_ref().unwrap().value, "\u{E710}");
            assert_eq!(e.command, Some(CommandRef::Invoke));
        }

        // §9.1 错误对象
        ("9.1", 0) => {
            let e: RpcError = parse(b, "RpcError");
            assert_eq!(e.code, error_codes::INVALID_PARAMS);
            assert_eq!(e.data.as_ref().unwrap()["missing"][0], "page_id");
        }

        // 未知 (章节, 序号) —— 映射表缺项即失败
        (s, i) => panic!(
            "docs/protocol.md 出现未映射的示例：§{} #{}（行 {}）。\
             请在 check() 的 match 中补充映射（§13 协议演进要求文档与实现同步）。",
            s,
            i + 1,
            b.line
        ),
    }
}

// ─── NDJSON 成帧 ↔ 文档规则的一致性 ────────────────────────

#[test]
fn framing_follows_section_2() {
    use dd_protocol::Frame;

    // §2.2 示例行：紧凑 JSON + \n
    let line = r#"{"jsonrpc":"2.0","id":1,"method":"top_level_commands","params":{}}"#;
    let mut d = framing::Decoder::with_default_limit();
    let frames = d.push(framing::encode(line).unwrap().as_slice());
    assert_eq!(frames, vec![Frame::Message(line.to_string())]);

    // 解出的行必须是合法 JSON 且 jsonrpc=="2.0"（§3.2）
    let msg = match &frames[0] {
        Frame::Message(s) => s,
        other => panic!("预期 Message 帧，实际: {other:?}"),
    };
    let raw: RawMessage = framing::decode_message(msg).unwrap();
    assert_eq!(raw.jsonrpc, "2.0");
    assert_eq!(raw.method.as_deref(), Some("top_level_commands"));
}
