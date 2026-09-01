//! dd-ext-sample —— M0 示例扩展。
//!
//! 契约来源：[`docs/protocol.md`](../../docs/protocol.md)。
//! 覆盖 M0 任务表中的"示例扩展"一项：响应 `initialize`（§5.1）、
//! `top_level_commands`（§6.1，返回 2 条硬编码命令）、`close`（§6.6）。
//!
//! 传输层约定（§2）：stdin 读 NDJSON、stdout **只写协议消息**，
//! 任何日志一律走 stderr（§2.5）。

use std::io::{self, Read, Write};

use dd_protocol::framing::{encode, Decoder, Frame};
use dd_protocol::messages::{
    error_codes, CommandListResult, InitializeResult, ProviderInfo, RawMessage, JSONRPC_VERSION,
};
use dd_protocol::model::{CommandItem, CommandRef, Details, Icon, IconKind, MetadataEntry};

/// 必须与示例清单的 `id` 一致（清单 schema §8：不一致时宿主以清单为准并记警告）。
const PROVIDER_ID: &str = "com.example.sample";
/// §5.3：v1.0 阶段扩展回 `"1.0"`。
const PROTOCOL_VERSION: &str = "1.0";

fn main() {
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    log("已启动，等待 initialize");

    let mut decoder = Decoder::with_default_limit();
    let mut buf = [0u8; 4096];
    loop {
        match stdin.read(&mut buf) {
            Ok(0) => break, // stdin EOF：宿主已关闭管道
            Ok(n) => {
                let mut should_exit = false;
                for frame in decoder.push(&buf[..n]) {
                    if let Frame::Message(line) = frame {
                        if handle(&line, &mut stdout) {
                            should_exit = true;
                        }
                    }
                }
                if should_exit {
                    break;
                }
            }
            Err(e) => {
                log(&format!("读取 stdin 失败：{e}"));
                break;
            }
        }
    }
    log("退出");
}

/// 处理一条消息；返回 `true` 表示已处理 `close`、可以退出。
fn handle(line: &str, out: &mut dyn Write) -> bool {
    let msg: RawMessage = match serde_json::from_str(line) {
        Ok(msg) => msg,
        Err(_) => {
            // §9.2：无法解析的行 → -32700（id 无法取得，故为 null）
            send_error(out, None, error_codes::PARSE_ERROR, "Parse error", None);
            return false;
        }
    };
    if msg.jsonrpc != JSONRPC_VERSION {
        send_error(
            out,
            msg.id,
            error_codes::INVALID_REQUEST,
            "Invalid Request",
            None,
        );
        return false;
    }

    // §3.3：只有"有 method 且有 id"才是请求；通知与响应本示例不处理
    let (Some(method), Some(id)) = (msg.method.clone(), msg.id) else {
        return false;
    };

    log(&format!("<- {method} (id={id})"));
    match method.as_str() {
        "initialize" => {
            let result =
                serde_json::to_value(initialize_result()).expect("序列化 InitializeResult");
            send_result(out, id, result);
        }
        "top_level_commands" => {
            let result = serde_json::to_value(CommandListResult {
                commands: top_level_commands(),
            })
            .expect("序列化 CommandListResult");
            send_result(out, id, result);
        }
        "close" => {
            // §6.6 后置规则 2：返回 result 后尽快自行退出
            send_result(out, id, serde_json::json!({}));
            return true;
        }
        other => send_error(
            out,
            Some(id),
            error_codes::METHOD_NOT_FOUND,
            "Method not found",
            Some(serde_json::json!({ "method": other })),
        ),
    }
    false
}

/// §5.1 `initialize` 的成功响应。
fn initialize_result() -> InitializeResult {
    InitializeResult {
        protocol_version: PROTOCOL_VERSION.to_string(),
        provider: ProviderInfo {
            id: PROVIDER_ID.to_string(),
            display_name: "Sample".to_string(),
            frozen: true,
            has_fallback: false,
        },
        // M0 不调用任何 host/* 方法；声明后宿主才会响应（§7.4 能力前置）
        capabilities: vec![],
        timeouts: None,
    }
}

/// §6.1：2 条硬编码顶层命令。字段覆盖 §8.1 的可选成员，供 M1 渲染联调。
fn top_level_commands() -> Vec<CommandItem> {
    vec![
        CommandItem {
            id: "sample.hello".to_string(),
            title: "Say Hello".to_string(),
            subtitle: Some("M0 smoke test command".to_string()),
            icon: Some(Icon {
                kind: IconKind::Glyph,
                value: "\u{E8C8}".to_string(),
            }),
            section: Some("Sample".to_string()),
            tags: Some(vec!["demo".to_string(), "m0".to_string()]),
            details: Some(Details {
                title: "Say Hello".to_string(),
                body:
                    "验证首屏聚合链路：title / subtitle / icon / section / tags / details 全字段。"
                        .to_string(),
                metadata: Some(vec![MetadataEntry {
                    key: "Provider".to_string(),
                    value: PROVIDER_ID.to_string(),
                }]),
            }),
            text_to_suggest: Some("sample ".to_string()),
            more_commands: None,
            command: CommandRef::Invoke,
        },
        CommandItem {
            id: "sample.copy".to_string(),
            title: "Copy Sample Text".to_string(),
            subtitle: Some("演示 §7.3 host/set_clipboard 的调用路径（M4 接入）".to_string()),
            icon: Some(Icon {
                kind: IconKind::Glyph,
                value: "\u{E8C8}".to_string(),
            }),
            section: Some("Sample".to_string()),
            tags: Some(vec!["demo".to_string()]),
            details: None,
            text_to_suggest: None,
            // §8.1 more_commands：上下文菜单项（设计文档 §5.6）
            more_commands: Some(vec![CommandItem {
                id: "sample.copy.plain".to_string(),
                title: "Copy without formatting".to_string(),
                subtitle: None,
                icon: None,
                section: None,
                tags: None,
                details: None,
                text_to_suggest: None,
                more_commands: None,
                command: CommandRef::Invoke,
            }]),
            command: CommandRef::Invoke,
        },
    ]
}

fn send_result(out: &mut dyn Write, id: u64, result: serde_json::Value) {
    send(
        out,
        &serde_json::json!({ "jsonrpc": JSONRPC_VERSION, "id": id, "result": result }),
    );
}

fn send_error(
    out: &mut dyn Write,
    id: Option<u64>,
    code: i32,
    message: &str,
    data: Option<serde_json::Value>,
) {
    let mut error = serde_json::json!({ "code": code, "message": message });
    if let Some(data) = data {
        error["data"] = data;
    }
    send(
        out,
        &serde_json::json!({ "jsonrpc": JSONRPC_VERSION, "id": id, "error": error }),
    );
}

/// §2.2：一行一条紧凑 JSON，以 `\n` 结尾。
fn send(out: &mut dyn Write, value: &serde_json::Value) {
    match serde_json::to_string(value) {
        Ok(line) => match encode(&line) {
            Ok(bytes) => {
                let _ = out.write_all(&bytes);
                let _ = out.flush();
            }
            Err(_) => log("消息内含裸换行，已丢弃（§2.2 规则 2）"),
        },
        Err(e) => log(&format!("序列化失败：{e}")),
    }
}

/// §2.5：日志只走 stderr。
fn log(message: &str) {
    eprintln!("[dd-ext-sample] {message}");
}
