//! dd-ext-sample —— M0 示例扩展 + M2 人工验收扩展。
//!
//! 契约来源：[`docs/protocol.md`](../../docs/protocol.md)。
//! M0：`initialize`（§5.1）、`top_level_commands`（§6.1）、`close`（§6.6）。
//! M2（人工验收支撑，见 m2-record.md §4 清单）：
//! - `invoke`（§6.5）：按命令 id 分派，覆盖全部 **8 种 `CommandResult` Kind**；
//! - `get_items`（§6.3）：`m2.page` 嵌套页（含 GoBack/GoHome/Dismiss 子命令）；
//! - `items_changed`（§7.1）：`m2.page.notify`（页级，验 A9 刷新）与
//!   `m2.top.notify`（顶层）两个触发命令。
//!
//! M3（桩复热支撑，见 m3-record.md）：`get_command`（§6.4）——按 id 从顶层目录
//! 取回真实命令，供宿主在 frozen 桩复热链路使用；找不到回 `command: null`（非错误）。
//!
//! M4（host/* 执行端支撑，见 m4-record.md P2）：声明 `host/show_status` /
//! `host/set_clipboard` / `host/open_url` 三个能力，并提供 3 条触发命令——
//! `invoke` 期间向宿主反向发 `host/*` 请求（§3.3），宿主应答并记录，
//! 由 dd-gui 消费执行真实副作用（Toast / 剪贴板 / 默认浏览器）。
//!
//! 传输层约定（§2）：stdin 读 NDJSON、stdout **只写协议消息**，
//! 任何日志一律走 stderr（§2.5）。

use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use dd_protocol::framing::{encode, Decoder, Frame};
use dd_protocol::messages::{
    error_codes, CommandListResult, GetCommandParams, GetCommandResult, GetItemsParams,
    GetItemsResult, InitializeResult, InvokeParams, ItemsChangedParams, ProviderInfo, RawMessage,
    JSONRPC_VERSION,
};
use dd_protocol::model::{
    CommandItem, CommandRef, CommandResult, Details, Icon, IconKind, MetadataEntry,
};

/// 必须与示例清单的 `id` 一致（清单 schema §8：不一致时宿主以清单为准并记警告）。
const PROVIDER_ID: &str = "com.example.sample";
/// §5.3：v1.0 阶段扩展回 `"1.0"`。
const PROTOCOL_VERSION: &str = "1.0";
/// M2 验收嵌套页 id（`CommandRef::Page` / `get_items` / 页级 `items_changed` 共用）。
const PAGE_ID: &str = "m2.page";
/// `m2.page` 被 `get_items` 拉取的次数（验证 A9：`items_changed` → 全量重拉后
/// 副标题计数自增，刷新肉眼可见）。
static PAGE_FETCHES: AtomicUsize = AtomicUsize::new(0);

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
        "get_command" => {
            let parsed = msg
                .params
                .and_then(|v| serde_json::from_value::<GetCommandParams>(v).ok());
            let Some(params) = parsed else {
                send_error(
                    out,
                    Some(id),
                    error_codes::INVALID_PARAMS,
                    "Invalid params",
                    None,
                );
                return false;
            };
            // §6.4：找不到 → `command: null`（正常结果，非错误）
            let command = top_level_commands().into_iter().find(|c| c.id == params.id);
            log(&format!(
                "-> get_command {} => {}",
                params.id,
                if command.is_some() { "found" } else { "null" }
            ));
            send_result(
                out,
                id,
                serde_json::to_value(GetCommandResult { command })
                    .expect("序列化 GetCommandResult"),
            );
        }
        "invoke" => {
            let parsed = msg
                .params
                .and_then(|v| serde_json::from_value::<InvokeParams>(v).ok());
            let Some(params) = parsed else {
                send_error(
                    out,
                    Some(id),
                    error_codes::INVALID_PARAMS,
                    "Invalid params",
                    None,
                );
                return false;
            };
            log(&format!(
                "<- invoke id={} confirmed={}",
                params.id,
                params
                    .context
                    .as_ref()
                    .and_then(|c| c.confirmed)
                    .unwrap_or(false)
            ));
            let result = invoke_result(&params);
            log(&format!("-> invoke {} => {:?}", params.id, result));
            send_result(
                out,
                id,
                serde_json::to_value(&result).expect("序列化 CommandResult"),
            );
            // §7.1：通知型命令在回包后补发 items_changed（验 A9：通知 + 全量重拉）
            match params.id.as_str() {
                "m2.page.notify" => send_items_changed(out, Some(PAGE_ID.to_string())),
                "m2.top.notify" => send_items_changed(out, None),
                // M4（host/* 执行端，见 m4-record.md P2）：回包后向宿主发 host/* 请求，
                // 由 dd-gui 消费执行真实副作用。宿主在 invoke 响应等待期间会应答
                // （§3.3：扩展反向请求），故无需在此等待应答。
                "m4.host.show_status" => {
                    send_host_request(
                        out,
                        "host/show_status",
                        &serde_json::json!({
                            "message": "M4 host/show_status：Toast 显示成功".to_string(),
                            "state": "success",
                            "duration_ms": 3000_u64,
                        }),
                    );
                }
                "m4.host.copy" | "sample.copy" | "sample.copy.plain" => {
                    send_host_request(
                        out,
                        "host/set_clipboard",
                        &serde_json::json!({
                            "text": "dd-run M4 clipboard demo：3.14159".to_string(),
                        }),
                    );
                }
                "m4.host.open_url" => {
                    send_host_request(
                        out,
                        "host/open_url",
                        &serde_json::json!({
                            "url": "https://github.com/Yang-SH/dd-run".to_string(),
                        }),
                    );
                }
                _ => {}
            }
        }
        "get_items" => {
            let parsed = msg
                .params
                .and_then(|v| serde_json::from_value::<GetItemsParams>(v).ok());
            let Some(params) = parsed else {
                send_error(
                    out,
                    Some(id),
                    error_codes::INVALID_PARAMS,
                    "Invalid params",
                    None,
                );
                return false;
            };
            if params.page_id == PAGE_ID {
                let result = GetItemsResult {
                    items: page_items(),
                    has_more_items: false,
                    is_loading: false,
                };
                send_result(
                    out,
                    id,
                    serde_json::to_value(result).expect("序列化 GetItemsResult"),
                );
            } else {
                // §9.2：未知页 → -32005
                send_error(
                    out,
                    Some(id),
                    error_codes::PAGE_NOT_FOUND,
                    "Page not found",
                    Some(serde_json::json!({ "page_id": params.page_id })),
                );
            }
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
        // M4（host/* 执行端验证，见 m4-record.md P2）：声明后宿主才应答
        // 并执行真实副作用（Toast / 剪贴板 / 开 URL，§7.4 能力前置）。
        capabilities: vec![
            "host/show_status".to_string(),
            "host/set_clipboard".to_string(),
            "host/open_url".to_string(),
        ],
        timeouts: None,
    }
}

/// §6.1：顶层命令 = M0 原有 2 条 + 「M2 验收」分组（8 种 Kind 除 GoBack 外
/// 全部在顶层可触发；GoBack 在 Root 上无上级、属 A5 边界，放进嵌套页验证）。
fn top_level_commands() -> Vec<CommandItem> {
    let mut cmds = vec![
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
            subtitle: Some("M4：host/set_clipboard → 写剪贴板（粘贴验证）".to_string()),
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
    ];
    // 「M2 验收」分组：每条命令的标题即其验证点
    cmds.push(item(
        "m2.page",
        "Page：进入子页",
        "验证 A5：嵌套页 + get_items + Esc 返回",
        &["page"],
        CommandRef::Page {
            page_id: PAGE_ID.to_string(),
        },
    ));
    cmds.push(item(
        "m2.kind.dismiss",
        "Kind：Dismiss",
        "关闭并清空状态：再次唤起回到首页（与 Hide 对比）",
        &["kind"],
        CommandRef::Invoke,
    ));
    cmds.push(item(
        "m2.kind.hide",
        "Kind：Hide",
        "隐藏但保留状态：再次唤起仍回到当前页与查询（与 Dismiss 对比）",
        &["kind"],
        CommandRef::Invoke,
    ));
    cmds.push(item(
        "m2.kind.go_home",
        "Kind：GoHome",
        "清空嵌套页回首页（Root 上执行无视觉变化，正常）",
        &["kind"],
        CommandRef::Invoke,
    ));
    cmds.push(item(
        "m2.kind.keep_open",
        "Kind：KeepOpen",
        "面板保持打开（无视觉变化，正常）",
        &["kind"],
        CommandRef::Invoke,
    ));
    cmds.push(item(
        "m2.kind.go_to_page",
        "Kind：GoToPage",
        "跳转到 m2.page（等价于 Page 命令的 invoke 形态）",
        &["kind"],
        CommandRef::Invoke,
    ));
    cmds.push(item(
        "m2.kind.show_toast",
        "Kind：ShowToast",
        "底部弹出提示条，3 秒后消失",
        &["kind"],
        CommandRef::Invoke,
    ));
    cmds.push(item(
        "m2.kind.confirm",
        "Kind：Confirm",
        "弹二次确认；确认后宿主带 confirmed=true 重发本命令",
        &["kind"],
        CommandRef::Invoke,
    ));
    cmds.push(item(
        "m2.top.notify",
        "通知：顶层 items_changed",
        "发送顶层通知（宿主当前仅提示，Root 重聚合属已知遗留）",
        &["a9"],
        CommandRef::Invoke,
    ));
    // 「M4 host/* 执行端」分组（见 m4-record.md P2 / §4 清单 #6–#8）
    cmds.push(item(
        "m4.host.show_status",
        "M4：host/show_status",
        "调宿主 Toast（扩展请求宿主显示状态，§7.2）",
        &["m4", "host"],
        CommandRef::Invoke,
    ));
    cmds.push(item(
        "m4.host.copy",
        "M4：host/set_clipboard",
        "调宿主写剪贴板（§7.3），随后粘贴验证文本",
        &["m4", "host"],
        CommandRef::Invoke,
    ));
    cmds.push(item(
        "m4.host.open_url",
        "M4：host/open_url",
        "调宿主用默认浏览器打开仓库主页（§7.4）",
        &["m4", "host"],
        CommandRef::Invoke,
    ));
    cmds
}

/// §6.3 `m2.page` 内容：GoBack / GoHome / Dismiss 子命令 + 页级 items_changed 触发。
/// 副标题带拉取计数：`items_changed` → 宿主全量重拉 → 计数 +1（A9 刷新肉眼可见）。
fn page_items() -> Vec<CommandItem> {
    let n = PAGE_FETCHES.fetch_add(1, Ordering::Relaxed) + 1;
    let fetched = format!("本页第 {n} 次被拉取");
    vec![
        item(
            "m2.page.back",
            "GoBack：返回上一级",
            &fetched,
            &["kind"],
            CommandRef::Invoke,
        ),
        item(
            "m2.page.home",
            "GoHome：回首页",
            &fetched,
            &["kind"],
            CommandRef::Invoke,
        ),
        item(
            "m2.page.dismiss",
            "Dismiss：关闭面板",
            &fetched,
            &["kind"],
            CommandRef::Invoke,
        ),
        item(
            "m2.page.notify",
            "通知：本页 items_changed",
            &fetched,
            &["a9"],
            CommandRef::Invoke,
        ),
    ]
}

/// §6.5 `invoke`：按命令 id 分派，覆盖全部 8 种 Kind（A4）。
/// Confirm 二次确认靠 `context.confirmed` 区分首发/重发（§8.3 注）。
fn invoke_result(params: &InvokeParams) -> CommandResult {
    let confirmed = params
        .context
        .as_ref()
        .and_then(|c| c.confirmed)
        .unwrap_or(false);
    match params.id.as_str() {
        "sample.hello" => CommandResult::ShowToast {
            message: "Hello from dd-ext-sample！".to_string(),
            duration_ms: None,
        },
        // 保持打开：M4 接入 set_clipboard 前仅演示 KeepOpen Kind
        "sample.copy" | "sample.copy.plain" => CommandResult::KeepOpen,
        // M4：host/* 触发命令——真实副作用在 dd-gui 执行端（Toast/剪贴板/浏览器），
        // 此处仅回 KeepOpen（面板保持打开以便观察），host 请求在 invoke 回包后补发。
        "m4.host.show_status" | "m4.host.copy" | "m4.host.open_url" => CommandResult::KeepOpen,
        "m2.kind.dismiss" | "m2.page.dismiss" => CommandResult::Dismiss,
        "m2.kind.hide" => CommandResult::Hide,
        "m2.kind.go_home" | "m2.page.home" => CommandResult::GoHome,
        "m2.page.back" => CommandResult::GoBack,
        "m2.kind.keep_open" => CommandResult::KeepOpen,
        "m2.kind.go_to_page" => CommandResult::GoToPage {
            page_id: PAGE_ID.to_string(),
        },
        "m2.kind.show_toast" => CommandResult::ShowToast {
            message: "ShowToast：3 秒后自动消失".to_string(),
            duration_ms: Some(3_000),
        },
        "m2.kind.confirm" => {
            if confirmed {
                CommandResult::ShowToast {
                    message: "确认流程闭环：已收到 confirmed=true 重发".to_string(),
                    duration_ms: None,
                }
            } else {
                CommandResult::Confirm {
                    title: "二次确认".to_string(),
                    description:
                        "这是一条 Confirm 结果。确认后宿主应带 confirmed=true 重新 invoke 本命令。"
                            .to_string(),
                    confirm_label: "确认执行".to_string(),
                    is_critical: false,
                }
            }
        }
        "m2.page.notify" => CommandResult::ShowToast {
            message: "已发送本页 items_changed，观察副标题计数 +1".to_string(),
            duration_ms: Some(3_000),
        },
        // 顶层通知不改 Toast，避免覆盖宿主的「扩展命令已更新」提示
        "m2.top.notify" => CommandResult::KeepOpen,
        other => CommandResult::ShowToast {
            message: format!("未知命令：{other}"),
            duration_ms: Some(3_000),
        },
    }
}

/// §7.1 `items_changed` 通知；`page_id=None` 表示顶层命令变了。
fn send_items_changed(out: &mut dyn Write, page_id: Option<String>) {
    let params =
        serde_json::to_value(ItemsChangedParams { page_id }).expect("序列化 ItemsChangedParams");
    send(
        out,
        &serde_json::json!({
            "jsonrpc": JSONRPC_VERSION,
            "method": "items_changed",
            "params": params
        }),
    );
    log("-> items_changed");
}

/// §3.3：扩展向宿主发 `host/*` **请求**（带 id，等待应答）。
/// 宿主在 in-flight 等待期间应答（`call` 内），或空闲轮询应答（M4 起），
/// 应答结果本示例不等待（Fire-and-forget，宿主必应答后记录）。
fn send_host_request(out: &mut dyn Write, method: &str, params: &serde_json::Value) {
    static REQ_ID: AtomicU64 = AtomicU64::new(1);
    let id = REQ_ID.fetch_add(1, Ordering::Relaxed);
    send(
        out,
        &serde_json::json!({
            "jsonrpc": JSONRPC_VERSION,
            "id": id,
            "method": method,
            "params": params
        }),
    );
    log(&format!("-> {method} (id={id})"));
}

/// 构造一条「M2 验收」分组命令（section 固定，字段取最小集）。
fn item(id: &str, title: &str, subtitle: &str, tags: &[&str], command: CommandRef) -> CommandItem {
    CommandItem {
        id: id.to_string(),
        title: title.to_string(),
        subtitle: (!subtitle.is_empty()).then(|| subtitle.to_string()),
        icon: None,
        section: Some("M2 验收".to_string()),
        tags: (!tags.is_empty()).then(|| tags.iter().map(|t| t.to_string()).collect()),
        details: None,
        text_to_suggest: None,
        more_commands: None,
        command,
    }
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
