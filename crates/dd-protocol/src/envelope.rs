//! §3.2 / §3.3 / §3.4 信封校验（单一来源）。
//!
//! 把「一行 NDJSON → 合法信封 / `-32700` / `-32600`」的三态判定收口在此，供宿主
//! （`dd-host`）与扩展（`dd-ext`）两侧共用——两条路径的校验行为因此逐条一致
//! （M9 R1「两条路径等价」口径的延伸），§3.2 的各项 `-32600` 规则也只需实现一次。
//!
//! 契约来源：`docs/protocol.md` §3.2（字段规则）、§3.3（`id` 规则）、
//! §3.4（不支持批处理）、§9.1（错误对象）、§9.2（错误码表）。

use serde_json::Value;

use crate::messages::{error_codes, RawMessage, RpcError, JSONRPC_VERSION};

/// 信封校验的三态结果（§3.2 / §3.4）。
#[derive(Debug, Clone, PartialEq)]
pub enum Envelope {
    /// 合法信封；继续按 §3.3 判形态（请求 / 通知 / 响应）。
    Valid(RawMessage),
    /// §9.2 `-32700 Parse error`：该行不是合法 JSON。
    ParseError,
    /// §9.2 `-32600 Invalid Request`：是合法 JSON，但不是合法信封。
    ///
    /// `id` 为「能从消息里取到的 id」；缺省、为 `null`、或 `id` 自身非法时均为
    /// `None`，回应时序列化为 `null`（§2.3）。
    InvalidRequest {
        id: Option<u64>,
        reason: InvalidReason,
    },
}

impl Envelope {
    /// 便捷构造 `-32600` 结果。
    pub fn invalid(id: Option<u64>, reason: InvalidReason) -> Self {
        Self::InvalidRequest { id, reason }
    }

    /// 按 §9.1 构造待发送的错误响应；[`Envelope::Valid`] 返回 `None`（无需回错）。
    ///
    /// 两侧（宿主 / 扩展）回包即用此函数，保证 `-32700` 与 `-32600` 的形状一致。
    pub fn to_error_response(&self) -> Option<Value> {
        match self {
            Self::Valid(_) => None,
            Self::ParseError => Some(error_response(
                None,
                error_codes::PARSE_ERROR,
                "Parse error",
                None,
            )),
            Self::InvalidRequest { id, reason } => Some(error_response(
                *id,
                error_codes::INVALID_REQUEST,
                "Invalid Request",
                Some(serde_json::json!({
                    "reason": reason.kind(),
                    "detail": reason.message(),
                })),
            )),
        }
    }
}

/// §3.2 / §3.4 触发 `-32600` 的具体原因（用于回应文案与单测断言）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidReason {
    /// §3.4：JSON-RPC 批处理数组不受支持。
    BatchArray,
    /// 顶层是合法 JSON 但不是对象（也不是数组）。
    NotAnObject,
    /// §3.2：`jsonrpc` 缺失。
    MissingJsonrpc,
    /// §3.2：`jsonrpc` 存在但不是字符串 `"2.0"`。
    UnsupportedJsonrpcVersion,
    /// §3.3：`id` 存在但不是非负整数（字符串 / 负数 / 小数 / 布尔）。
    InvalidId,
    /// §3.2：`method` 存在但不是字符串。
    InvalidMethod,
    /// §3.2：`params` 出现但不是对象（本协议不用数组形式）。
    NonObjectParams,
    /// §3.2：`result` 与 `error` 并存（二者互斥）。
    ResultAndError,
    /// §9.1：`error` 出现但不是合法错误对象。
    InvalidErrorObject,
}

impl InvalidReason {
    /// §9.1 要求 `message` 为简短英文描述（面向开发者，不直接展示给用户）。
    pub fn message(self) -> &'static str {
        match self {
            Self::BatchArray => "Batch requests are not supported",
            Self::NotAnObject => "Message must be a JSON object",
            Self::MissingJsonrpc => "Missing \"jsonrpc\"",
            Self::UnsupportedJsonrpcVersion => "\"jsonrpc\" must be \"2.0\"",
            Self::InvalidId => "\"id\" must be a non-negative integer",
            Self::InvalidMethod => "\"method\" must be a string",
            Self::NonObjectParams => "\"params\" must be an object",
            Self::ResultAndError => "\"result\" and \"error\" are mutually exclusive",
            Self::InvalidErrorObject => "\"error\" must be an error object",
        }
    }

    /// 稳定标识（写进错误对象 `data.reason`，便于对端与日志定位；不随文案改动而变）。
    pub fn kind(self) -> &'static str {
        match self {
            Self::BatchArray => "batch_array",
            Self::NotAnObject => "not_an_object",
            Self::MissingJsonrpc => "missing_jsonrpc",
            Self::UnsupportedJsonrpcVersion => "unsupported_jsonrpc_version",
            Self::InvalidId => "invalid_id",
            Self::InvalidMethod => "invalid_method",
            Self::NonObjectParams => "non_object_params",
            Self::ResultAndError => "result_and_error",
            Self::InvalidErrorObject => "invalid_error_object",
        }
    }
}

/// 校验一行消息（§2.2 的一行 = 一条 NDJSON 消息）。
pub fn validate(line: &str) -> Envelope {
    match serde_json::from_str::<Value>(line) {
        Ok(value) => validate_value(value),
        Err(_) => Envelope::ParseError,
    }
}

/// 校验一个已解析的 JSON 值。
///
/// in-process 适配器直接拿到 `serde_json::Value`（而非 NDJSON 行），故与
/// [`validate`] 分开暴露，两侧共享同一套规则。
pub fn validate_value(value: Value) -> Envelope {
    let map = match value {
        Value::Object(map) => map,
        Value::Array(_) => return Envelope::invalid(None, InvalidReason::BatchArray),
        _ => return Envelope::invalid(None, InvalidReason::NotAnObject),
    };

    // §3.3：`id` 为非负整数；缺省或 `null` 视为「无 id」。
    //
    // ⚠️ 必须接受 `null`：本协议自身的 `-32600` / `-32700` 响应在无法确定 id 时
    //    即发 `"id": null`（§2.3）。若在此判为非法，对端收到这类**合法**错误响应
    //    会被再次判非法，往返互喷形成自激回路。
    let id = match map.get("id") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_u64() {
            Some(n) => Some(n),
            None => return Envelope::invalid(None, InvalidReason::InvalidId),
        },
    };

    // §3.2：`jsonrpc` 必填且恒为字符串 `"2.0"`。
    match map.get("jsonrpc") {
        None => return Envelope::invalid(id, InvalidReason::MissingJsonrpc),
        Some(Value::String(s)) if s == JSONRPC_VERSION => {}
        Some(_) => return Envelope::invalid(id, InvalidReason::UnsupportedJsonrpcVersion),
    }

    // §3.2：`method` 若出现必须是字符串。
    let method = match map.get("method") {
        None => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return Envelope::invalid(id, InvalidReason::InvalidMethod),
    };

    // §3.2：`params` 若出现必须是对象（本协议不用数组形式）。
    if let Some(params) = map.get("params") {
        if !params.is_object() {
            return Envelope::invalid(id, InvalidReason::NonObjectParams);
        }
    }

    // §3.2：`result` 与 `error` 互斥。
    if map.contains_key("result") && map.contains_key("error") {
        return Envelope::invalid(id, InvalidReason::ResultAndError);
    }

    // §9.1：`error` 若出现必须是合法错误对象。
    let error = match map.get("error") {
        None => None,
        Some(v) => match serde_json::from_value::<RpcError>(v.clone()) {
            Ok(e) => Some(e),
            Err(_) => return Envelope::invalid(id, InvalidReason::InvalidErrorObject),
        },
    };

    // 逐项校验已过，手工构造强类型信封（未知字段按 §13 忽略）。
    Envelope::Valid(RawMessage {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id,
        method,
        params: map.get("params").cloned(),
        result: map.get("result").cloned(),
        error,
    })
}

/// 构造 §9.1 错误响应（`{"jsonrpc":"2.0","id":…,"error":{…}}`）。
///
/// `id` 为 `None` 时序列化为 `null`（§2.3：无法确定 id 的错误响应）。
pub fn error_response(id: Option<u64>, code: i32, message: &str, data: Option<Value>) -> Value {
    let mut error = serde_json::json!({ "code": code, "message": message });
    if let Some(data) = data {
        error["data"] = data;
    }
    serde_json::json!({ "jsonrpc": JSONRPC_VERSION, "id": id, "error": error })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(e: &Envelope) -> InvalidReason {
        match e {
            Envelope::InvalidRequest { reason, .. } => *reason,
            other => panic!("期望 InvalidRequest，实际 {other:?}"),
        }
    }

    #[test]
    fn accepts_request_notification_and_response() {
        for line in [
            r#"{"jsonrpc":"2.0","id":1,"method":"top_level_commands","params":{}}"#,
            r#"{"jsonrpc":"2.0","method":"items_changed","params":{"page_id":"calc.history"}}"#,
            r#"{"jsonrpc":"2.0","id":1,"result":{"commands":[]}}"#,
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#,
        ] {
            assert!(
                matches!(validate(line), Envelope::Valid(_)),
                "应判合法：{line}"
            );
        }
    }

    #[test]
    fn accepts_null_id_as_absent() {
        // §2.3：无法确定 id 的错误响应即 `"id": null`，必须判合法（否则往返互喷）。
        let e = validate(r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"x"}}"#);
        match e {
            Envelope::Valid(msg) => assert_eq!(msg.id, None),
            other => panic!("`id: null` 应判合法，实际 {other:?}"),
        }
    }

    #[test]
    fn reports_parse_error_for_non_json() {
        assert_eq!(validate("not json at all"), Envelope::ParseError);
        assert_eq!(validate(""), Envelope::ParseError);
    }

    #[test]
    fn rejects_batch_array() {
        // §3.4：数组 → -32600（不是 -32700）
        assert_eq!(
            reason(&validate(
                r#"[{"jsonrpc":"2.0","id":1,"method":"initialize"}]"#
            )),
            InvalidReason::BatchArray
        );
    }

    #[test]
    fn rejects_non_object_json() {
        assert_eq!(reason(&validate("42")), InvalidReason::NotAnObject);
        assert_eq!(reason(&validate("\"str\"")), InvalidReason::NotAnObject);
    }

    #[test]
    fn rejects_missing_or_wrong_jsonrpc() {
        assert_eq!(
            reason(&validate(r#"{"id":1,"method":"initialize"}"#)),
            InvalidReason::MissingJsonrpc
        );
        assert_eq!(
            reason(&validate(
                r#"{"jsonrpc":"1.0","id":1,"method":"initialize"}"#
            )),
            InvalidReason::UnsupportedJsonrpcVersion
        );
        assert_eq!(
            reason(&validate(r#"{"jsonrpc":2,"id":1,"method":"initialize"}"#)),
            InvalidReason::UnsupportedJsonrpcVersion
        );
    }

    #[test]
    fn rejects_invalid_id_types() {
        for line in [
            r#"{"jsonrpc":"2.0","id":"1","method":"initialize"}"#,
            r#"{"jsonrpc":"2.0","id":-1,"method":"initialize"}"#,
            r#"{"jsonrpc":"2.0","id":1.5,"method":"initialize"}"#,
            r#"{"jsonrpc":"2.0","id":true,"method":"initialize"}"#,
        ] {
            assert_eq!(reason(&validate(line)), InvalidReason::InvalidId, "{line}");
        }
    }

    #[test]
    fn keeps_parsable_id_in_invalid_request() {
        // `jsonrpc` 非法但 id 可解析 → 回错时带上 id（§2.3 只在无法解析时才用 null）
        match validate(r#"{"jsonrpc":"1.0","id":7,"method":"x"}"#) {
            Envelope::InvalidRequest { id, .. } => assert_eq!(id, Some(7)),
            other => panic!("期望 InvalidRequest，实际 {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_method_params_and_error() {
        assert_eq!(
            reason(&validate(r#"{"jsonrpc":"2.0","id":1,"method":5}"#)),
            InvalidReason::InvalidMethod
        );
        assert_eq!(
            reason(&validate(
                r#"{"jsonrpc":"2.0","id":1,"method":"get_items","params":[]}"#
            )),
            InvalidReason::NonObjectParams
        );
        assert_eq!(
            reason(&validate(r#"{"jsonrpc":"2.0","id":1,"error":"boom"}"#)),
            InvalidReason::InvalidErrorObject
        );
    }

    #[test]
    fn rejects_result_and_error_together() {
        assert_eq!(
            reason(&validate(
                r#"{"jsonrpc":"2.0","id":1,"result":{},"error":{"code":-32603,"message":"x"}}"#
            )),
            InvalidReason::ResultAndError
        );
    }

    #[test]
    fn error_response_shape_follows_section_9_1() {
        let parse = validate("oops").to_error_response().unwrap();
        assert_eq!(parse["jsonrpc"], "2.0");
        assert_eq!(parse["id"], Value::Null);
        assert_eq!(parse["error"]["code"], error_codes::PARSE_ERROR);

        let invalid = validate(r#"[1]"#).to_error_response().unwrap();
        assert_eq!(invalid["id"], Value::Null);
        assert_eq!(invalid["error"]["code"], error_codes::INVALID_REQUEST);
        assert_eq!(invalid["error"]["data"]["reason"], "batch_array");

        // 合法信封不回错
        assert!(validate(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#)
            .to_error_response()
            .is_none());
    }
}
