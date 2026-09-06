//! dd-ext —— 共享扩展运行时。
//!
//! 背景与决策：M4 实施决策 **D1-A「共享扩展运行时」**（见 [`docs/m4-record.md`](../../docs/m4-record.md) §0）——
//! 5 个内置扩展（Apps / Calc / System / WebSearch / Shell）共用一套协议样板：
//! stdin 主循环、NDJSON 成帧、JSON-RPC 信封解析、方法分发、结果/错误/通知发送。
//! 若各扩展手写（`dd-ext-sample` 单文件 ~500 行样板），5 个扩展将产生 ~2500 行重复；
//! 抽到本 crate 后，扩展只声明 [`ExtensionSpec`]（身份 + 命令集合 + invoke 处理器），
//! 产物仍是 1 进程/扩展（ADR-1 子进程隔离不变，见 [`docs/implementation.md`](../../docs/implementation.md) §4）。
//!
//! 契约来源：[`docs/protocol.md`](../../docs/protocol.md)——
//! §2（NDJSON 成帧）、§3（信封与 id 空间）、§5（握手）、§6（host→ext 方法）、
//! §7（ext→host 方法）、§8（数据模型）、§9（错误码）。
//!
//! 支持的方法（host → extension）：
//! - `initialize`（§5.1）：回 [`messages::InitializeResult`]，`provider` 字段取自 [`ExtensionSpec`]；
//! - `top_level_commands`（§6.1）：回 [`spec.top_level`] 的结果；
//! - `fallback_commands`（§6.2）：回 [`spec.fallback`] 的结果（无兜底时回空列表，
//!   不报错——宿主只在 `has_fallback=true` 时才调用，这里做防御）；
//! - `get_command`（§6.4）：按 id 从顶层命令中查找，找不到回 `command: null`（正常结果）；
//! - `invoke`（§6.5）：交 [`spec.invoke`] 处理，成功回 [`model::CommandResult`]，
//!   处理器返回的副作用（host/* 请求、items_changed 通知）在响应**之后**按序发出；
//! - `get_items`（§6.3）：本运行时未提供子页注册点（5 个内置扩展均无子页），
//!   一律回 `-32005 Page not found`；
//! - `close`（§6.6）：回 `{}` 并置退出标志（后置规则 2：尽快自行退出）。
//!
//! 未注册的方法 → `-32601 Method not found`（§9.2）。

use std::io::{self, Read, Write};

use dd_protocol::framing::{encode, Decoder, Frame};
use dd_protocol::messages::{
    error_codes, CommandListResult, GetCommandParams, GetCommandResult, GetItemsParams,
    InitializeResult, InvokeParams, ProviderInfo, RawMessage, JSONRPC_VERSION,
};
use dd_protocol::model::{CommandItem, CommandResult};

/// 内置扩展轻量 i18n（批次 D）：环境注入生效语言，按 zh/en 选文案。
pub mod i18n;

/// §5.3：扩展回"不高于宿主所发版本"的版本；v1.0 阶段恒为 `"1.0"`。
pub const PROTOCOL_VERSION: &str = "1.0";

/// 一次 `invoke` 的副作用（在成功响应**之后**按序发给宿主）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// §3.3 / §7.2–§7.4：扩展 → 宿主请求（`host/*`）。宿主应答并记录，本运行时不等待。
    HostRequest {
        method: &'static str,
        params: serde_json::Value,
    },
    /// §7.1：通知宿主"某页/顶层集合变了"（不携带数据，宿主自行全量重拉）。
    ItemsChanged { page_id: Option<String> },
}

/// `invoke` 处理器：返回命令结果与副作用。
///
/// 与 `dd-ext-sample` 的"处理器内直写 stdout"不同，本模型是**纯函数**——
/// 副作用集中返回、由运行时统一发送，便于单测与保证发送顺序（响应先于副作用）。
pub type InvokeHandler = fn(&InvokeParams) -> (CommandResult, Vec<Effect>);

/// 顶层 / 兜底命令集合的构造器（无状态纯函数）。
pub type ListHandler = fn() -> Vec<CommandItem>;

/// 一个扩展的完整声明（id / 展示信息 / 特性 / 命令集合 / invoke 处理器）。
#[derive(Debug, Clone)]
pub struct ExtensionSpec {
    /// 必须与宿主侧清单的 `id` 一致（清单 schema §8：不一致时宿主以清单为准并记警告）。
    pub id: &'static str,
    pub display_name: &'static str,
    /// 一句话描述（详情面板 / 诊断用）。
    pub description: &'static str,
    /// §5.1 `provider.frozen`：顶层命令是否可磁盘缓存（manifest `frozen` 的预期值一致）。
    pub frozen: bool,
    /// §5.1 `provider.has_fallback`：是否有兜底命令；`true` 时宿主**必须**视为 fresh。
    pub has_fallback: bool,
    /// 扩展需要用到（会在反向请求中发出）的 `host/*` 方法名集合（§7.4 能力前置）。
    pub capabilities: &'static [&'static str],
    /// 日志前缀（stderr），如 `"dd-ext-calc"`——每扩展一个进程，便于区分诊断。
    pub log_tag: &'static str,
    /// §6.1：顶层命令（首屏聚合）。
    pub top_level: ListHandler,
    /// §6.2：兜底命令**模板**。命令 `title` 中可含 `{query}` 占位符，
    /// 由宿主渲染时替换为当前搜索词；invoke 时经 `context.query` 携带原始搜索词。
    /// `None` 表示无兜底（等价 `has_fallback=false`）。
    pub fallback: Option<ListHandler>,
    /// §6.5：命令执行。按 `params.id`（+ `context.query`）分派到具体行为。
    pub invoke: InvokeHandler,
}

/// 运行扩展主循环（进程入口）：读 stdin 的 NDJSON，逐条响应，直到 `close` 或 stdin EOF。
pub fn run(spec: &ExtensionSpec) {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();

    log(spec, "已启动，等待 initialize");

    let mut decoder = Decoder::with_default_limit();
    let mut buf = [0u8; 4096];
    loop {
        match stdin.read(&mut buf) {
            Ok(0) => break, // stdin EOF：宿主已关闭管道
            Ok(n) => {
                let mut should_exit = false;
                for frame in decoder.push(&buf[..n]) {
                    if let Frame::Message(line) = frame {
                        let (outputs, exit) = serve_line(spec, &line);
                        for msg in outputs {
                            send(spec, &mut stdout, &msg);
                        }
                        should_exit |= exit;
                    }
                }
                if should_exit {
                    break;
                }
            }
            Err(e) => {
                log(spec, &format!("读取 stdin 失败：{e}"));
                break;
            }
        }
    }
    log(spec, "退出");
}

/// 处理单行请求（纯函数，便于单测）。返回 `(待发送消息列表, 是否应退出)`。
pub fn serve_line(spec: &ExtensionSpec, line: &str) -> (Vec<serde_json::Value>, bool) {
    let msg: RawMessage = match serde_json::from_str(line) {
        Ok(msg) => msg,
        Err(_) => {
            // §9.2：无法解析的行 → -32700（id 无法取得，故为 null）
            return (
                vec![make_error(
                    None,
                    error_codes::PARSE_ERROR,
                    "Parse error",
                    None,
                )],
                false,
            );
        }
    };
    if msg.jsonrpc != JSONRPC_VERSION {
        return (
            vec![make_error(
                msg.id,
                error_codes::INVALID_REQUEST,
                "Invalid Request",
                None,
            )],
            false,
        );
    }

    // §3.3：只有"有 method 且有 id"才是请求；通知与响应本扩展不处理
    let (Some(method), Some(id)) = (msg.method.clone(), msg.id) else {
        return (Vec::new(), false);
    };

    log(spec, &format!("<- {method} (id={id})"));
    match method.as_str() {
        "initialize" => {
            let result =
                serde_json::to_value(initialize_result(spec)).expect("序列化 InitializeResult");
            (vec![make_result(id, result)], false)
        }
        "top_level_commands" => {
            let result = serde_json::to_value(CommandListResult {
                commands: (spec.top_level)(),
            })
            .expect("序列化 CommandListResult");
            (vec![make_result(id, result)], false)
        }
        "fallback_commands" => {
            // §6.2：无兜底时回空列表（防御——宿主只在 has_fallback=true 时调用）
            let commands = spec.fallback.map(|f| f()).unwrap_or_default();
            log(
                spec,
                &format!("-> fallback_commands => {} 条模板", commands.len()),
            );
            let result = serde_json::to_value(CommandListResult { commands })
                .expect("序列化 CommandListResult");
            (vec![make_result(id, result)], false)
        }
        "get_command" => {
            let parsed = msg
                .params
                .and_then(|v| serde_json::from_value::<GetCommandParams>(v).ok());
            let Some(params) = parsed else {
                return (
                    vec![make_error(
                        Some(id),
                        error_codes::INVALID_PARAMS,
                        "Invalid params",
                        None,
                    )],
                    false,
                );
            };
            // §6.4：找不到 → command: null（正常结果，非错误）。只在顶层命令中查找
            //（兜底模板随输入动态生成，无 id 缓存语义，宿主用 fallback_commands 重新获取）。
            let command = (spec.top_level)().into_iter().find(|c| c.id == params.id);
            log(
                spec,
                &format!(
                    "-> get_command {} => {}",
                    params.id,
                    if command.is_some() { "found" } else { "null" }
                ),
            );
            let result = serde_json::to_value(GetCommandResult { command })
                .expect("序列化 GetCommandResult");
            (vec![make_result(id, result)], false)
        }
        "invoke" => {
            let parsed = msg
                .params
                .and_then(|v| serde_json::from_value::<InvokeParams>(v).ok());
            let Some(params) = parsed else {
                return (
                    vec![make_error(
                        Some(id),
                        error_codes::INVALID_PARAMS,
                        "Invalid params",
                        None,
                    )],
                    false,
                );
            };
            log(
                spec,
                &format!(
                    "<- invoke id={} query={:?}",
                    params.id,
                    params.context.as_ref().and_then(|c| c.query.as_deref())
                ),
            );
            let (result, effects) = (spec.invoke)(&params);
            log(spec, &format!("-> invoke {} => {:?}", params.id, result));
            // 发送顺序：结果响应在前，副作用（host/* 请求、items_changed）在后
            let mut outputs = vec![make_result(
                id,
                serde_json::to_value(&result).expect("序列化 CommandResult"),
            )];
            for effect in effects {
                match effect {
                    Effect::HostRequest { method, params } => {
                        outputs.push(make_host_request(spec, method, params));
                    }
                    Effect::ItemsChanged { page_id } => {
                        outputs.push(make_items_changed(spec, page_id));
                    }
                }
            }
            (outputs, false)
        }
        "get_items" => {
            // 本运行时未提供子页注册点（5 个内置扩展均无子页）→ 一律 -32005
            let page_id = msg
                .params
                .and_then(|v| serde_json::from_value::<GetItemsParams>(v).ok())
                .map(|p| p.page_id)
                .unwrap_or_default();
            log(spec, &format!("-> get_items {page_id} => 无子页（-32005）"));
            (
                vec![make_error(
                    Some(id),
                    error_codes::PAGE_NOT_FOUND,
                    "Page not found",
                    Some(serde_json::json!({ "page_id": page_id })),
                )],
                false,
            )
        }
        "close" => {
            // §6.6 后置规则 2：返回 result 后尽快自行退出
            (vec![make_result(id, serde_json::json!({}))], true)
        }
        other => (
            vec![make_error(
                Some(id),
                error_codes::METHOD_NOT_FOUND,
                "Method not found",
                Some(serde_json::json!({ "method": other })),
            )],
            false,
        ),
    }
}

/// §5.1 `initialize` 的成功响应；字段全部取自 [`ExtensionSpec`]。
fn initialize_result(spec: &ExtensionSpec) -> InitializeResult {
    InitializeResult {
        protocol_version: PROTOCOL_VERSION.to_string(),
        provider: ProviderInfo {
            id: spec.id.to_string(),
            display_name: spec.display_name.to_string(),
            frozen: spec.frozen,
            has_fallback: spec.has_fallback,
        },
        // §7.4 能力前置：声明后宿主才会应答对应 host/* 方法
        capabilities: spec.capabilities.iter().map(|s| s.to_string()).collect(),
        timeouts: None,
    }
}

/// §3.3：扩展向宿主发 `host/*` **请求**（带自增 id，等待应答——宿主在 in-flight
/// 等待期间或空闲轮询时应答；本运行时 fire-and-forget，不阻塞主循环）。
fn make_host_request(
    spec: &ExtensionSpec,
    method: &'static str,
    params: serde_json::Value,
) -> serde_json::Value {
    static REQ_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = REQ_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    log(spec, &format!("-> {method} (id={id})"));
    serde_json::json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "method": method,
        "params": params
    })
}

/// §7.1 `items_changed` 通知；`page_id=None` 表示顶层命令变了。
fn make_items_changed(spec: &ExtensionSpec, page_id: Option<String>) -> serde_json::Value {
    log(spec, "-> items_changed");
    serde_json::json!({
        "jsonrpc": JSONRPC_VERSION,
        "method": "items_changed",
        "params": { "page_id": page_id }
    })
}

fn make_result(id: u64, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "jsonrpc": JSONRPC_VERSION, "id": id, "result": result })
}

fn make_error(
    id: Option<u64>,
    code: i32,
    message: &str,
    data: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut error = serde_json::json!({ "code": code, "message": message });
    if let Some(data) = data {
        error["data"] = data;
    }
    serde_json::json!({ "jsonrpc": JSONRPC_VERSION, "id": id, "error": error })
}

/// §2.2：一行一条紧凑 JSON，以 `\n` 结尾；失败只记日志（§2.5 日志走 stderr）。
fn send(spec: &ExtensionSpec, out: &mut dyn Write, value: &serde_json::Value) {
    match serde_json::to_string(value) {
        Ok(line) => match encode(&line) {
            Ok(bytes) => {
                let _ = out.write_all(&bytes);
                let _ = out.flush();
            }
            Err(_) => log(spec, "消息内含裸换行，已丢弃（§2.2 规则 2）"),
        },
        Err(e) => log(spec, &format!("序列化失败：{e}")),
    }
}

/// §2.5：日志只走 stderr。
fn log(spec: &ExtensionSpec, message: &str) {
    eprintln!("[{}] {message}", spec.log_tag);
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_protocol::messages::InvokeContext;
    use dd_protocol::model::{CommandRef, Icon, IconKind, Sender};

    /// 测试用最小 spec：顶层 2 命令、fallback 1 模板、invoke 分发。
    fn spec() -> ExtensionSpec {
        ExtensionSpec {
            id: "com.ddrun.fixture",
            display_name: "Fixture",
            description: "单测夹具",
            frozen: true,
            has_fallback: true,
            capabilities: &["host/open_url"],
            log_tag: "dd-ext-fixture",
            top_level: || {
                vec![
                    CommandItem {
                        id: "fix.hello".into(),
                        title: "Hello".into(),
                        subtitle: None,
                        icon: Some(Icon {
                            kind: IconKind::Glyph,
                            value: "\u{E8C8}".into(),
                        }),
                        section: None,
                        tags: None,
                        details: None,
                        text_to_suggest: None,
                        more_commands: None,
                        command: CommandRef::Invoke,
                    },
                    CommandItem {
                        id: "fix.page".into(),
                        title: "Page".into(),
                        subtitle: None,
                        icon: None,
                        section: None,
                        tags: None,
                        details: None,
                        text_to_suggest: None,
                        more_commands: None,
                        command: CommandRef::Page {
                            page_id: "fix.sub".into(),
                        },
                    },
                ]
            },
            fallback: Some(|| {
                vec![CommandItem {
                    id: "fix.fallback".into(),
                    title: "Do {query}".into(),
                    subtitle: None,
                    icon: None,
                    section: None,
                    tags: None,
                    details: None,
                    text_to_suggest: None,
                    more_commands: None,
                    command: CommandRef::Invoke,
                }]
            }),
            invoke: |params: &InvokeParams| match params.id.as_str() {
                "fix.open" => (
                    CommandResult::Dismiss,
                    vec![Effect::HostRequest {
                        method: "host/open_url",
                        params: serde_json::json!({ "url": "https://example.com" }),
                    }],
                ),
                "fix.notify" => (
                    CommandResult::KeepOpen,
                    vec![Effect::ItemsChanged {
                        page_id: Some("fix.sub".into()),
                    }],
                ),
                "fix.echo" => (
                    CommandResult::ShowToast {
                        message: format!(
                            "= {}",
                            params
                                .context
                                .as_ref()
                                .and_then(|c| c.query.clone())
                                .unwrap_or_default()
                        ),
                        duration_ms: None,
                    },
                    Vec::new(),
                ),
                _ => (
                    CommandResult::ShowToast {
                        message: "未知".into(),
                        duration_ms: None,
                    },
                    Vec::new(),
                ),
            },
        }
    }

    fn invoke_line(id: &str, query: Option<&str>) -> String {
        serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0", "id": 9, "method": "invoke",
            "params": {
                "id": id,
                "sender": "top_level",
                "context": { "query": query }
            }
        }))
        .unwrap()
    }

    #[test]
    fn initialize_returns_spec_fields() {
        let (out, exit) = serve_line(
            &spec(),
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        );
        assert!(!exit);
        assert_eq!(out.len(), 1);
        let result = &out[0]["result"];
        assert_eq!(result["protocol_version"], "1.0");
        assert_eq!(result["provider"]["id"], "com.ddrun.fixture");
        assert_eq!(result["provider"]["display_name"], "Fixture");
        assert_eq!(result["provider"]["frozen"], true);
        assert_eq!(result["provider"]["has_fallback"], true);
        assert_eq!(result["capabilities"], serde_json::json!(["host/open_url"]));
    }

    #[test]
    fn top_level_commands_returns_command_list() {
        let (out, _) = serve_line(
            &spec(),
            r#"{"jsonrpc":"2.0","id":2,"method":"top_level_commands","params":{}}"#,
        );
        let cmds = &out[0]["result"]["commands"];
        assert_eq!(cmds.as_array().unwrap().len(), 2);
        assert_eq!(cmds[0]["id"], "fix.hello");
        assert_eq!(cmds[1]["command"]["kind"], "page");
    }

    #[test]
    fn fallback_commands_returns_templates() {
        let (out, _) = serve_line(
            &spec(),
            r#"{"jsonrpc":"2.0","id":3,"method":"fallback_commands","params":{}}"#,
        );
        let cmds = &out[0]["result"]["commands"];
        assert_eq!(cmds.as_array().unwrap().len(), 1);
        assert_eq!(cmds[0]["id"], "fix.fallback");
        assert_eq!(
            cmds[0]["title"], "Do {query}",
            "{{query}} 占位由宿主渲染替换"
        );
    }

    #[test]
    fn fallback_commands_empty_when_no_fallback() {
        let mut s = spec();
        s.has_fallback = false;
        s.fallback = None;
        let (out, _) = serve_line(
            &s,
            r#"{"jsonrpc":"2.0","id":3,"method":"fallback_commands","params":{}}"#,
        );
        assert_eq!(out[0]["result"]["commands"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn get_command_finds_top_level_or_returns_null() {
        let line = r#"{"jsonrpc":"2.0","id":4,"method":"get_command","params":{"id":"fix.hello"}}"#;
        let (out, _) = serve_line(&spec(), line);
        assert_eq!(out[0]["result"]["command"]["id"], "fix.hello");

        let line =
            r#"{"jsonrpc":"2.0","id":4,"method":"get_command","params":{"id":"fix.missing"}}"#;
        let (out, _) = serve_line(&spec(), line);
        assert!(
            out[0]["result"]["command"].is_null(),
            "找不到 → null（非错误）"
        );
    }

    #[test]
    fn invoke_returns_result_then_effects_in_order() {
        // 结果响应在前，host 请求在后（§3.3 反向请求）
        let (out, _) = serve_line(&spec(), &invoke_line("fix.open", None));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["result"]["kind"], "Dismiss");
        assert_eq!(out[1]["method"], "host/open_url");
        assert_eq!(out[1]["params"]["url"], "https://example.com");
        assert!(out[1]["id"].is_u64(), "host 请求带自增 id");
        // host 请求与宿主响应的 id 空间独立（扩展侧从此请求 id 之后继续自增）
    }

    #[test]
    fn invoke_carries_query_into_handler() {
        let (out, _) = serve_line(&spec(), &invoke_line("fix.echo", Some("1+1")));
        assert_eq!(out[0]["result"]["kind"], "ShowToast");
        assert_eq!(
            out[0]["result"]["args"]["message"], "= 1+1",
            "context.query 透传"
        );
    }

    #[test]
    fn invoke_items_changed_notification() {
        let (out, _) = serve_line(&spec(), &invoke_line("fix.notify", None));
        assert_eq!(out.len(), 2);
        assert_eq!(out[1]["method"], "items_changed");
        assert_eq!(out[1]["params"]["page_id"], "fix.sub");
        assert!(out[1].get("id").is_none(), "通知无 id");
    }

    #[test]
    fn get_items_unknown_page_returns_32005() {
        let line =
            r#"{"jsonrpc":"2.0","id":5,"method":"get_items","params":{"page_id":"fix.sub"}}"#;
        let (out, _) = serve_line(&spec(), line);
        assert_eq!(out[0]["error"]["code"], -32005);
        assert_eq!(out[0]["error"]["data"]["page_id"], "fix.sub");
    }

    #[test]
    fn close_returns_result_and_exit_flag() {
        let (out, exit) = serve_line(
            &spec(),
            r#"{"jsonrpc":"2.0","id":7,"method":"close","params":{}}"#,
        );
        assert!(exit, "close 后应退出");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["result"], serde_json::json!({}));
    }

    #[test]
    fn unknown_method_returns_32601() {
        let line = r#"{"jsonrpc":"2.0","id":8,"method":"nope","params":{}}"#;
        let (out, _) = serve_line(&spec(), line);
        assert_eq!(out[0]["error"]["code"], error_codes::METHOD_NOT_FOUND);
        assert_eq!(out[0]["error"]["data"]["method"], "nope");
    }

    #[test]
    fn unparsable_line_returns_32700_with_null_id() {
        let (out, _) = serve_line(&spec(), "{not json");
        assert_eq!(out[0]["error"]["code"], error_codes::PARSE_ERROR);
        assert!(out[0]["id"].is_null(), "解析失败 id 为 null（§9.2）");
    }

    #[test]
    fn wrong_jsonrpc_version_returns_32600() {
        let line = r#"{"jsonrpc":"1.0","id":1,"method":"initialize","params":{}}"#;
        let (out, _) = serve_line(&spec(), line);
        assert_eq!(out[0]["error"]["code"], error_codes::INVALID_REQUEST);
    }

    #[test]
    fn invoke_with_bad_params_returns_32602() {
        let line = r#"{"jsonrpc":"2.0","id":9,"method":"invoke","params":{"sender":"top_level"}}"#;
        let (out, _) = serve_line(&spec(), line);
        assert_eq!(out[0]["error"]["code"], error_codes::INVALID_PARAMS);
    }

    #[test]
    fn notifications_and_responses_are_ignored() {
        // 无 method（响应）与无 id（通知）都不分发、不回复
        let resp = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        let (out, _) = serve_line(&spec(), resp);
        assert!(out.is_empty());

        let note = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
        let (out, _) = serve_line(&spec(), note);
        assert!(out.is_empty());
    }

    #[test]
    fn invoke_sender_and_context_types_roundtrip() {
        // 确认 InvokeParams 反序列化覆盖 sender 枚举与 context 全字段
        let line = r#"{"jsonrpc":"2.0","id":9,"method":"invoke","params":{"id":"fix.echo","sender":"context_menu","context":{"query":"x","selected_item_id":"fix.hello","confirmed":true}}}"#;
        let parsed: InvokeParams = serde_json::from_str(
            &serde_json::to_string(
                &serde_json::from_str::<serde_json::Value>(line).unwrap()["params"],
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(parsed.sender, Sender::ContextMenu);
        let ctx: InvokeContext = parsed.context.unwrap();
        assert_eq!(ctx.query.as_deref(), Some("x"));
        assert_eq!(ctx.selected_item_id.as_deref(), Some("fix.hello"));
        assert_eq!(ctx.confirmed, Some(true));
    }
}
