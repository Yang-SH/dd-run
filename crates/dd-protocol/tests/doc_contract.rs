//! doc 契约测试（B2② 第二批，dd-run-改进分析.md §3.3）：把 `docs/protocol.md`
//! 的「实现现状」注从散文变成断言——**测试名即锚点**，实现漂移时测试红，
//! 提示同步文档 §条目。
//!
//! 覆盖清单（dd-protocol 侧；宿主/扩展侧行为见 `dd-host` / `dd-ext` 同名文件）：
//! - protocol.md §3.2/§3.4 现状注（信封校验表 + 批处理数组）→
//!   [`envelope_invalid_request_reasons_follow_doc_table`] /
//!   [`batch_array_is_invalid_request_not_parse_error`]；
//! - protocol.md §2.3 现状注（超限帧 `-32600` + `data.reason = message_too_large`
//!   的帧层判定；宿主侧回错 + 关闭连接的端到端由
//!   `dd-host/tests/roundtrip.rs::oversized_request_closes_connection_with_32600`
//!   断言，不在此重复）→ [`oversized_frame_is_detected_at_framing_layer`]；
//! - protocol.md §6.4 现状注（`GetCommandResult.command` 整体省略）→
//!   [`get_command_result_none_command_omits_field_on_wire`]；
//! - protocol.md §5.1 现状注（`timeouts` 仅 `get_items_ms` 一字段）→
//!   [`timeouts_suggestion_shape_is_single_field_get_items_ms`]；
//!   「宿主从不读取建议值」= `no-assert`：读取与否是消费侧行为，无运行时断言
//!   载体（补实现时请同步 protocol.md §10 表与 §5.1 注）。
//!
//! §2.5 日志现状注（stderr 后端 + `DDRUN_LOG` 默认 `Debug`）由
//! `logging.rs::default_level_keeps_legacy_visibility` 等 in-crate 测试覆盖；
//! 「后端恒写 stderr」= `no-assert`（无捕获 stderr 的稳定测试手段，机制上
//! 由 `logging.rs` 仅实现 `StderrLogger` 一个后端保证）。

use dd_protocol::envelope::{error_response, validate, Envelope, InvalidReason};
use dd_protocol::framing::{Decoder, Frame, DEFAULT_MAX_MESSAGE_BYTES};
use dd_protocol::messages::{
    error_codes, GetCommandResult, InitializeResult, Timeouts, JSONRPC_VERSION,
};

/// 契约锚：docs/protocol.md §3.2/§3.4 现状注（2026-09-15）。
///
/// 现状注原文：`jsonrpc` 缺失/非 `"2.0"`、`id` 类型非法、`params` 非对象、
/// `result`+`error` 并存、`method` 非字符串一律回 `-32600`，`data.reason`
/// 标明具体 kind；`id` 能解析时回带上，否则为 `null`。
///
/// 断言：逐条喂畸形信封 → `InvalidReason` kind 与文档一致；错误响应形状
/// （code / data.reason / id 保留规则）符合 §9.1。有人改判定或形状即红。
#[test]
fn envelope_invalid_request_reasons_follow_doc_table() {
    let cases: &[(&str, InvalidReason)] = &[
        (
            r#"{"id":1,"method":"initialize"}"#,
            InvalidReason::MissingJsonrpc,
        ),
        (
            r#"{"jsonrpc":"1.0","id":1,"method":"initialize"}"#,
            InvalidReason::UnsupportedJsonrpcVersion,
        ),
        (
            r#"{"jsonrpc":"2.0","id":"str","method":"initialize"}"#,
            InvalidReason::InvalidId,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"method":5}"#,
            InvalidReason::InvalidMethod,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"method":"get_items","params":[]}"#,
            InvalidReason::NonObjectParams,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{},"error":{"code":-1,"message":"x"}}"#,
            InvalidReason::ResultAndError,
        ),
    ];
    for (line, expected) in cases {
        let envelope = validate(line);
        let (id, reason) = match &envelope {
            Envelope::InvalidRequest { id, reason } => (*id, *reason),
            other => panic!("期望 InvalidRequest：{line} → {other:?}"),
        };
        assert_eq!(reason, *expected, "{line}");
        // §9.1 形状：code -32600 + data.reason；id 可解析则带上（本组 id=1 可解析）
        let resp = envelope.to_error_response().unwrap();
        assert_eq!(resp["jsonrpc"], JSONRPC_VERSION);
        assert_eq!(resp["error"]["code"], error_codes::INVALID_REQUEST);
        assert_eq!(resp["error"]["data"]["reason"], reason.kind());
        if id.is_some() {
            assert_eq!(resp["id"], 1, "id 能解析时必须回带（§2.3）：{line}");
        }
    }
    // id 无法解析（非法 id 类型）→ 回 `id: null`
    let resp = validate(r#"{"jsonrpc":"2.0","id":"bad","method":"initialize"}"#)
        .to_error_response()
        .unwrap();
    assert!(resp["id"].is_null(), "id 非法时回 null（§2.3）");
}

/// 契约锚：docs/protocol.md §3.4 现状注（批处理数组 → `-32600`，非 `-32700`）。
#[test]
fn batch_array_is_invalid_request_not_parse_error() {
    let envelope = validate(r#"[{"jsonrpc":"2.0","id":1,"method":"initialize"}]"#);
    assert!(matches!(
        envelope,
        Envelope::InvalidRequest {
            reason: InvalidReason::BatchArray,
            ..
        }
    ));
    let resp = envelope.to_error_response().unwrap();
    assert_eq!(resp["error"]["code"], error_codes::INVALID_REQUEST);
    assert_eq!(resp["error"]["data"]["reason"], "batch_array");
}

/// 契约锚：docs/protocol.md §2.3 / §9.3 现状注（2026-09-15）的帧层半段。
///
/// 现状注原文：收到超限帧即回 `-32600`（`id: null`，`data.reason =
/// message_too_large`）并关闭连接。帧层判定出自 `framing` 的 `Frame::TooLarge`；
/// 宿主侧「回错 + 终止子进程」由 `dd-host/tests/roundtrip.rs::
/// oversized_request_closes_connection_with_32600` 端到端断言。
///
/// 断言：超过 `DEFAULT_MAX_MESSAGE_BYTES` 的行 → `Frame::TooLarge`（而非
/// panic / 静默丢弃）；错误响应形状（-32600 + id null + reason）由
/// `error_response` 组出的线上形状与文档一致。
#[test]
fn oversized_frame_is_detected_at_framing_layer() {
    let mut dec = Decoder::new(DEFAULT_MAX_MESSAGE_BYTES);
    let mut line = String::from("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}");
    line.push_str(&"x".repeat(DEFAULT_MAX_MESSAGE_BYTES));
    line.push('\n');
    let frames = dec.push(line.as_bytes());
    assert!(
        frames.iter().any(|f| matches!(f, Frame::TooLarge { .. })),
        "超限帧必须产出 Frame::TooLarge"
    );

    // 宿主/扩展回错的线上形状（两侧共用 envelope::error_response 组包）：
    // code -32600、id null（超限帧无法可靠取 id）、data.reason = message_too_large。
    // 参数与 `dd-host` `process.rs` 的 abort_oversized 同口径。
    let resp = error_response(
        None,
        error_codes::INVALID_REQUEST,
        "Invalid Request",
        Some(serde_json::json!({
            "reason": "message_too_large",
            "size": line.len() - 1,
            "max": DEFAULT_MAX_MESSAGE_BYTES,
        })),
    );
    assert_eq!(resp["id"], serde_json::Value::Null);
    assert_eq!(resp["error"]["code"], -32600);
    assert_eq!(resp["error"]["data"]["reason"], "message_too_large");
}

/// 契约锚：docs/protocol.md §6.4 现状注（核对至 2026-09-13）。
///
/// 现状注原文：`GetCommandResult.command` 带 `skip_serializing_if`，
/// `command: null` 在线上序列化为 `{"result":{}}`（字段整体省略）；接收方
/// 不得假设该字段必然出现。
///
/// 断言：`command: None` 序列化后无 `command` 键（不是 `null`）。
#[test]
fn get_command_result_none_command_omits_field_on_wire() {
    let wire = serde_json::to_value(GetCommandResult { command: None }).unwrap();
    assert!(
        wire.get("command").is_none(),
        "command: None 必须整体省略（线上即 result={{}}），实际 {wire}"
    );
    // 反向：有值时字段出现（对称性，防止 skip 条件误扩）
    let item = dd_protocol::model::CommandItem {
        id: "x".into(),
        title: "X".into(),
        subtitle: None,
        icon: None,
        section: None,
        tags: None,
        details: None,
        text_to_suggest: None,
        more_commands: None,
        command: dd_protocol::model::CommandRef::Invoke,
    };
    let wire = serde_json::to_value(GetCommandResult {
        command: Some(item),
    })
    .unwrap();
    assert!(wire.get("command").is_some());
}

/// 契约锚：docs/protocol.md §5.1 / §10 现状注（核对至 2026-09-13）。
///
/// 现状注原文：`result.timeouts` 仅 `get_items_ms` 一个字段且宿主从不读取。
///
/// 断言（前半）：`Timeouts` 序列化后**只有** `get_items_ms` 一个键——有人
/// 给建议值结构加字段（协议演进）时此测试红，提示同步 §10 表与 §5.1 注。
/// 后半（「宿主不读取」）= `no-assert`：消费侧行为，无运行时断言载体。
#[test]
fn timeouts_suggestion_shape_is_single_field_get_items_ms() {
    let wire = serde_json::to_value(Timeouts {
        get_items_ms: Some(250),
    })
    .unwrap();
    let keys: Vec<&str> = wire
        .as_object()
        .expect("Timeouts 序列化为对象")
        .keys()
        .map(|k| k.as_str())
        .collect();
    assert_eq!(
        keys,
        vec!["get_items_ms"],
        "§10 表的 timeouts.* 现状：单字段"
    );

    // 初始化响应里 timeouts 为 None 时同样整体省略（InitializeResult 侧对称）
    let init = InitializeResult {
        protocol_version: "1.0".into(),
        provider: dd_protocol::messages::ProviderInfo {
            id: "x".into(),
            display_name: "X".into(),
            frozen: true,
            has_fallback: false,
        },
        capabilities: vec![],
        timeouts: None,
    };
    let wire = serde_json::to_value(init).unwrap();
    assert!(wire.get("timeouts").is_none());
}
