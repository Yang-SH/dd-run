//! JSON-RPC 2.0 信封、12 个方法的参数/结果类型与 §9.2 错误码。
//!
//! 契约来源：`docs/protocol.md` §3（信封）、§5–§7（方法）、§9（错误）。

use serde::{Deserialize, Serialize};

use crate::model::{CommandItem, CommandRef, CommandResult, Sender};

/// §3.2 `jsonrpc` 恒为 `"2.0"`。
pub const JSONRPC_VERSION: &str = "2.0";

/// §9.1 错误对象。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// §9.2 JSON-RPC 2.0 标准错误码。
pub mod error_codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;

    /// dd-run 自定义码（§9.2）
    pub const EXTENSION_TIMEOUT: i32 = -32001;
    pub const COMMAND_NOT_FOUND: i32 = -32002;
    pub const PROVIDER_UNAVAILABLE: i32 = -32003;
    pub const VERSION_MISMATCH: i32 = -32004;
    pub const PAGE_NOT_FOUND: i32 = -32005;
}

/// §3.1 通用信封（弱类型投影）。
/// 用于对无法静态判定 payload 形状的消息做结构校验（如 §3.1 的四个示例）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawMessage {
    pub jsonrpc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

// ─── §5 握手 ───────────────────────────────────────────────

/// §5.1 `initialize` 请求参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitializeParams {
    pub protocol_version: String,
    pub host: HostInfo,
    pub transport: TransportInfo,
    /// 宿主支持的 `host/*` 方法名集合
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
}

/// §5.1 `host` 信息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostInfo {
    /// 恒为 `"dd-run"`
    pub name: String,
    pub version: String,
    /// `"windows"` / `"macos"` / `"linux"`
    pub platform: String,
}

/// §5.1 `transport` 信息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransportInfo {
    /// v1.0 恒为 `"ndjson"`
    pub framing: String,
    pub max_message_bytes: u64,
}

/// §5.1 `initialize` 成功响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitializeResult {
    /// 扩展选定的版本（§5.3）
    pub protocol_version: String,
    pub provider: ProviderInfo,
    /// 扩展需要用到的 `host/*` 方法
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeouts: Option<Timeouts>,
}

/// §5.1 `provider` 信息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub display_name: String,
    /// 顶层命令是否可缓存（设计文档 §6.3）
    pub frozen: bool,
    /// 是否有兜底命令；`true` 时宿主必须视为 fresh
    pub has_fallback: bool,
}

/// §5.1 `timeouts`：扩展建议的超时值（毫秒），宿主可覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timeouts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub get_items_ms: Option<u64>,
}

/// §5.2 `initialized` 通知参数（空对象）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmptyParams {}

// ─── §6 方法：host → extension ─────────────────────────────

/// §6.3 `get_items` 请求参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetItemsParams {
    pub page_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_text: Option<String>,
}

/// §6.3 `get_items` 成功响应（全量，不含增量）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetItemsResult {
    pub items: Vec<CommandItem>,
    pub has_more_items: bool,
    pub is_loading: bool,
}

/// §6.4 `get_command` 请求参数（桩复热）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetCommandParams {
    pub id: String,
}

/// §6.4 `get_command` 成功响应；`command == None` 表示桩已失效（正常结果，非错误）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetCommandResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandItem>,
}

/// §6.5 `invoke` 请求参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvokeParams {
    pub id: String,
    pub sender: Sender,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<InvokeContext>,
}

/// §6.5 `invoke` 的 `context`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvokeContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_item_id: Option<String>,
    /// 表单提交内容（FormPage 场景）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form_data: Option<serde_json::Value>,
    /// §8.3 注：Confirm 确认后宿主重新 invoke 时带上
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed: Option<bool>,
}

/// §6.5 `invoke` 成功响应：`result` 字段为 §8.3 CommandResult。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvokeResult {
    #[serde(rename = "result")]
    pub command_result: CommandResult,
}

/// §6.1 / §6.2 成功响应：顶层命令或兜底命令列表。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandListResult {
    pub commands: Vec<CommandItem>,
}

/// §6.6 `close` 成功响应（空对象）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmptyResult {}

// ─── §7 方法：extension → host ─────────────────────────────

/// §7.1 `items_changed` 通知参数；`page_id` 缺省表示"顶层命令变了"。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemsChangedParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_id: Option<String>,
}

/// §7.2 `host/show_status` 请求参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShowStatusParams {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<StatusState>,
    /// 显示时长；`0` 表示常驻直到被替换
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// §7.2 `state` 取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusState {
    Info,
    Success,
    Warning,
    Error,
}

/// §7.3 `host/set_clipboard` 请求参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetClipboardParams {
    pub text: String,
}

/// §7.4 `host/open_url` 请求参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenUrlParams {
    pub url: String,
}

/// §8.2 `CommandRef::Page` 的 `page_id` 复核辅助（防误用普通字符串）。
pub fn page_ref(page_id: impl Into<String>) -> CommandRef {
    CommandRef::Page {
        page_id: page_id.into(),
    }
}
