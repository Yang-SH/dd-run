//! 扩展子进程管理与 JSON-RPC 客户端。
//!
//! 契约来源：[`docs/protocol.md`](../../docs/protocol.md)：
//! §2 传输层（NDJSON）、§3 消息格式与 id 空间、§5 握手与版本协商、
//! §6 host→ext 方法、§7 ext→host 方法、§10 超时、§11 崩溃检测。
//!
//! 范围边界（M0）：实现 **spawn → initialize → top_level_commands → close**
//! 这一条链路，以及宿主侧对扩展反向请求（`host/*`）的识别与应答。
//! 页面栈、缓存、LRU 属 M1–M3，不在此处。

use std::io::Read;
use std::io::Write as _;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use dd_protocol::framing::{encode, Decoder, Frame, DEFAULT_MAX_MESSAGE_BYTES};
use dd_protocol::messages::{
    error_codes, CommandListResult, HostInfo, InitializeParams, InitializeResult, RawMessage,
    RpcError, TransportInfo, JSONRPC_VERSION,
};
use dd_protocol::model::CommandItem;

use crate::manifest::{current_platform, LoadedExtension, HOST_CAPABILITIES};

/// §10 各阶段默认超时。
pub const TIMEOUT_INITIALIZE: Duration = Duration::from_millis(5_000);
pub const TIMEOUT_TOP_LEVEL_COMMANDS: Duration = Duration::from_millis(3_000);
/// §6.6 后置规则 3：`close` 超时即强杀。
pub const TIMEOUT_CLOSE_RESPONSE: Duration = Duration::from_millis(1_000);
pub const TIMEOUT_CLOSE_EXIT: Duration = Duration::from_millis(1_000);

/// 扩展 stderr 的保留上限（§2.5：宿主应捕获扩展 stderr 用于崩溃诊断）。
const STDERR_CAPTURE_LIMIT: usize = 64 * 1024;

/// 协议层错误。
#[derive(Debug)]
pub enum ProtocolError {
    /// §10 超时。宿主应把 `-32001 extension_timeout` 交给调用方（UI 层）。
    Timeout { method: String, timeout: Duration },
    /// stdout EOF，即子进程已退出或崩溃（§11）
    ProcessExited,
    /// §2.3 单条消息超过上限：应回 `-32600` 并关闭连接
    MessageTooLarge { size: usize, max: usize },
    /// §2.2 规则 3：行内容不是合法 UTF-8
    InvalidUtf8,
    /// 不是合法 JSON-RPC 信封（§3.2）
    MalformedEnvelope(String),
    /// 扩展返回的错误响应（§9）
    Rpc(RpcError),
    /// §5.3 规则 4：扩展回的协议版本宿主不认识（高于所发版本或格式非法）
    BadProtocolVersion { got: String, requested: String },
    /// 写入子进程 stdin 失败
    Io(std::io::Error),
}

impl ProtocolError {
    /// §9.2：超时在 UI 层以 `-32001 extension_timeout` 呈现。
    pub fn as_rpc_error(&self) -> Option<RpcError> {
        match self {
            Self::Timeout { method, timeout } => Some(RpcError {
                code: error_codes::EXTENSION_TIMEOUT,
                message: "Extension timeout".to_string(),
                data: Some(serde_json::json!({
                    "method": method,
                    "timeout_ms": timeout.as_millis(),
                })),
            }),
            Self::ProcessExited => Some(RpcError {
                code: error_codes::PROVIDER_UNAVAILABLE,
                message: "Provider unavailable".to_string(),
                data: None,
            }),
            Self::Rpc(err) => Some(err.clone()),
            _ => None,
        }
    }
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout { method, timeout } => {
                write!(f, "`{method}` 超时（{} ms）", timeout.as_millis())
            }
            Self::ProcessExited => write!(f, "扩展进程已退出（stdout EOF）"),
            Self::MessageTooLarge { size, max } => {
                write!(f, "单条消息 {size} 字节超过上限 {max} 字节（§2.3）")
            }
            Self::InvalidUtf8 => write!(f, "消息不是合法 UTF-8（§2.2 规则 3）"),
            Self::MalformedEnvelope(msg) => write!(f, "非法 JSON-RPC 信封：{msg}"),
            Self::Rpc(err) => write!(f, "扩展返回错误 {}：{}", err.code, err.message),
            Self::BadProtocolVersion { got, requested } => {
                write!(
                    f,
                    "扩展回的协议版本 `{got}` 不高于所发版本的约束不成立（宿主发 `{requested}`）"
                )
            }
            Self::Io(e) => write!(f, "I/O 失败：{e}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

impl From<std::io::Error> for ProtocolError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for ProtocolError {
    fn from(e: serde_json::Error) -> Self {
        Self::MalformedEnvelope(e.to_string())
    }
}

/// `close` 阶段的错误（§6.6）。
#[derive(Debug)]
pub enum CloseError {
    /// `close` 请求本身失败（含超时）
    Protocol(ProtocolError),
    /// 进程在 1s 内未自行退出，宿主已强杀（§6.6 后置规则 3）
    ForceKilled,
    /// 进程以非 0 退出码结束
    NonZeroExit(Option<i32>),
    Io(std::io::Error),
}

impl std::fmt::Display for CloseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Protocol(e) => write!(f, "close 请求失败：{e}"),
            Self::ForceKilled => write!(f, "close 后进程未自行退出，已强杀"),
            Self::NonZeroExit(code) => write!(f, "进程以非 0 退出码结束：{code:?}"),
            Self::Io(e) => write!(f, "I/O 失败：{e}"),
        }
    }
}

impl std::error::Error for CloseError {}

/// §3.3 消息形态判别（宿主视角）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    /// 带 `id` 且 `method` 以 `host/` 开头 → 扩展发来的**请求**，宿主须应答
    HostRequest,
    /// 有 `method` 无 `id` → 通知，**永不回复**（§3.3）
    Notification,
    /// 无 `method` 有 `id` → 宿主先前发出请求的响应
    Response(u64),
    /// 其余（无 id 无 method，或带 id 却不是 `host/*`）——按 §3.3 忽略
    Unknown,
}

/// §3.3：先看 `method` 是不是"自己能提供的"（对宿主即 `host/*`），
/// 再按有无 `id` 区分响应与通知。**两端 id 空间独立**，靠发出方向区分。
pub fn classify(msg: &RawMessage) -> MessageKind {
    match (&msg.method, msg.id) {
        (Some(method), Some(_)) if method.starts_with("host/") => MessageKind::HostRequest,
        (Some(_), None) => MessageKind::Notification,
        (None, Some(id)) => MessageKind::Response(id),
        _ => MessageKind::Unknown,
    }
}

/// §13 协议版本格式为 `MAJOR.MINOR`（**两段**），与清单 `version` 的 semver
///（`MAJOR.MINOR.PATCH`，三段）不同，故不能复用 [`crate::manifest::parse_semver`]。
pub fn parse_protocol_version(s: &str) -> Option<(u64, u64)> {
    let mut parts = s.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor))
}

/// 一个已拉起的扩展子进程（状态机 §4 的 `spawned` → `initializing` → `ready`）。
pub struct ExtensionProcess {
    /// 清单 id
    id: String,
    child: Child,
    stdin: ChildStdin,
    /// 后台读线程切出的完整消息（§2.4 增量缓冲）
    rx: Receiver<Frame>,
    next_id: u64,
    /// 扩展在 `initialize` 中声明的 `capabilities`（§7.4 能力前置校验用）
    declared: Vec<String>,
    /// 收到的通知：`initialized`（§5.2）/ `items_changed`（§7.1）
    pub notifications: Vec<RawMessage>,
    /// 扩展反向调用的 `host/*` 请求记录（M0：记录并应答，真实副作用属 M4）
    pub host_requests: Vec<RawMessage>,
    /// §3.3：未匹配到 in-flight 请求的响应 → 记日志并忽略
    pub unmatched: Vec<RawMessage>,
    /// §2.5：扩展 stderr（崩溃诊断用）
    stderr: Arc<Mutex<Vec<u8>>>,
}

impl ExtensionProcess {
    /// §4 `spawned`：按清单启动子进程，接管 stdin/stdout/stderr。
    pub fn spawn(ext: &LoadedExtension) -> Result<Self, std::io::Error> {
        let mut command = Command::new(&ext.command);
        command
            .args(&ext.manifest.entry.args)
            .envs(&ext.manifest.entry.env)
            .current_dir(&ext.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn()?;
        let stdin = child.stdin.take().expect("stdin 已 piped");
        let stdout = child.stdout.take().expect("stdout 已 piped");
        let stderr = child.stderr.take().expect("stderr 已 piped");

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || read_loop(stdout, tx));

        let sink = Arc::new(Mutex::new(Vec::new()));
        thread::spawn({
            let sink = Arc::clone(&sink);
            move || capture_stderr(stderr, sink)
        });

        Ok(Self {
            id: ext.manifest.id.clone(),
            child,
            stdin,
            rx,
            next_id: 1, // §3.3：宿主 id 空间从 1 开始自增
            declared: Vec::new(),
            notifications: Vec::new(),
            host_requests: Vec::new(),
            unmatched: Vec::new(),
            stderr: sink,
        })
    }

    /// 清单 id。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// §5.1 握手 + §5.3 版本协商。成功即从 `initializing` 进入 `ready`。
    pub fn initialize(
        &mut self,
        protocol_version: &str,
        host_version: &str,
    ) -> Result<InitializeResult, ProtocolError> {
        let params = InitializeParams {
            protocol_version: protocol_version.to_string(),
            host: HostInfo {
                name: "dd-run".to_string(),
                version: host_version.to_string(),
                platform: current_platform().to_string(),
            },
            transport: TransportInfo {
                framing: "ndjson".to_string(),
                max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES as u64,
            },
            capabilities: HOST_CAPABILITIES.iter().map(|s| (*s).to_string()).collect(),
            locale: None,
        };
        let value = self.call(
            "initialize",
            serde_json::to_value(params)?,
            TIMEOUT_INITIALIZE,
        )?;
        let result: InitializeResult = serde_json::from_value(value)?;

        // §5.3 规则 2/4：扩展回的版本不得高于宿主所发，且必须是合法版本号
        let bad_version = || ProtocolError::BadProtocolVersion {
            got: result.protocol_version.clone(),
            requested: protocol_version.to_string(),
        };
        let got = parse_protocol_version(&result.protocol_version).ok_or_else(bad_version)?;
        let requested = parse_protocol_version(protocol_version).ok_or_else(bad_version)?;
        if got > requested {
            return Err(bad_version());
        }

        self.declared = result.capabilities.clone();
        Ok(result)
    }

    /// §6.1 取首屏顶层命令。
    pub fn top_level_commands(&mut self) -> Result<Vec<CommandItem>, ProtocolError> {
        let value = self.call(
            "top_level_commands",
            serde_json::json!({}),
            TIMEOUT_TOP_LEVEL_COMMANDS,
        )?;
        let result: CommandListResult = serde_json::from_value(value)?;
        Ok(result.commands)
    }

    /// §6.6 优雅关闭：发 `close` → 等 result → 等进程自行退出；超时则强杀。
    pub fn close(mut self) -> Result<(), CloseError> {
        self.call("close", serde_json::json!({}), TIMEOUT_CLOSE_RESPONSE)
            .map_err(CloseError::Protocol)?;

        // §6.6 后置规则 1：此后不再期待任何响应，只等进程退出
        match wait_for_exit(&mut self.child, TIMEOUT_CLOSE_EXIT) {
            Ok(Some(status)) => {
                if status.success() {
                    Ok(())
                } else {
                    Err(CloseError::NonZeroExit(status.code()))
                }
            }
            Ok(None) => {
                let _ = self.child.kill();
                let _ = self.child.wait();
                Err(CloseError::ForceKilled)
            }
            Err(e) => Err(CloseError::Io(e)),
        }
    }

    /// §11：进程是否已退出（非阻塞）。
    pub fn has_exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    /// §2.5：已捕获的扩展 stderr（截断到 [`STDERR_CAPTURE_LIMIT`]）。
    pub fn stderr(&self) -> String {
        let bytes = self.stderr.lock().map(|g| g.clone()).unwrap_or_default();
        String::from_utf8_lossy(&bytes).to_string()
    }

    /// 发出一次请求并等待**属于该请求**的响应。
    ///
    /// 期间可能先收到：扩展的反向请求（`host/*`，须应答后继续等）、
    /// 通知（记录后继续等）、以及迟到的无关响应（记录后忽略，§3.3）。
    pub fn call(
        &mut self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, ProtocolError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = serde_json::json!({
            "jsonrpc": JSONRPC_VERSION,
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_message(&request)?;

        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ProtocolError::Timeout {
                    method: method.to_string(),
                    timeout,
                });
            }
            match self.rx.recv_timeout(remaining) {
                Ok(Frame::Message(line)) => {
                    if let Some(result) = self.handle_line(&line, id)? {
                        return Ok(result);
                    }
                }
                Ok(Frame::TooLarge { size, max }) => {
                    return Err(ProtocolError::MessageTooLarge { size, max })
                }
                Ok(Frame::InvalidUtf8) => return Err(ProtocolError::InvalidUtf8),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(ProtocolError::Timeout {
                        method: method.to_string(),
                        timeout,
                    })
                }
                Err(RecvTimeoutError::Disconnected) => return Err(ProtocolError::ProcessExited),
            }
        }
    }

    /// 处理一行消息；返回 `Some` 表示拿到了目标 id 的响应。
    fn handle_line(
        &mut self,
        line: &str,
        waiting_id: u64,
    ) -> Result<Option<serde_json::Value>, ProtocolError> {
        let msg: RawMessage = serde_json::from_str(line)?;
        if msg.jsonrpc != JSONRPC_VERSION {
            // §3.2：缺 jsonrpc 或非 "2.0" → 非法信封，按 §9.3 不致命，忽略
            self.unmatched.push(msg);
            return Ok(None);
        }
        match classify(&msg) {
            MessageKind::HostRequest => {
                self.answer_host_request(&msg)?;
                Ok(None)
            }
            MessageKind::Notification => {
                self.notifications.push(msg);
                Ok(None)
            }
            MessageKind::Response(rid) => {
                if rid != waiting_id {
                    // §3.3：未匹配到 in-flight 请求的响应，记日志并忽略
                    self.unmatched.push(msg);
                    return Ok(None);
                }
                match msg.error {
                    Some(err) => Err(ProtocolError::Rpc(err)),
                    None => Ok(Some(msg.result.unwrap_or(serde_json::Value::Null))),
                }
            }
            MessageKind::Unknown => {
                self.unmatched.push(msg);
                Ok(None)
            }
        }
    }

    /// §7.4 能力前置：扩展只能用 `initialize` 里声明过的 `host/*` 方法，
    /// 未声明而调用 → `-32601 Method not found`。
    fn answer_host_request(&mut self, msg: &RawMessage) -> Result<(), ProtocolError> {
        let method = msg.method.clone().unwrap_or_default();
        let id = msg.id.unwrap_or(0);
        self.host_requests.push(msg.clone());

        let declared = self.declared.contains(&method);
        let response = if declared {
            serde_json::json!({ "jsonrpc": JSONRPC_VERSION, "id": id, "result": {} })
        } else {
            serde_json::json!({
                "jsonrpc": JSONRPC_VERSION,
                "id": id,
                "error": {
                    "code": error_codes::METHOD_NOT_FOUND,
                    "message": "Method not found",
                    "data": { "method": method },
                },
            })
        };
        self.write_message(&response)
    }

    fn write_message(&mut self, value: &serde_json::Value) -> Result<(), ProtocolError> {
        let line = serde_json::to_string(value)?;
        let bytes = encode(&line).map_err(|_| {
            ProtocolError::MalformedEnvelope("消息内含裸换行（§2.2 规则 2）".into())
        })?;
        self.stdin.write_all(&bytes)?;
        self.stdin.flush()?;
        Ok(())
    }
}

impl Drop for ExtensionProcess {
    /// 未走 `close` 就丢弃时强杀，避免残留子进程（§11 扩展侧义务的对偶）。
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// §2.4 读循环：一次 `read` 可能返回半条或多条消息，交给 [`Decoder`] 累积切分。
fn read_loop(mut stdout: std::process::ChildStdout, tx: Sender<Frame>) {
    let mut decoder = Decoder::with_default_limit();
    let mut buf = [0u8; 8192];
    loop {
        match stdout.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for frame in decoder.push(&buf[..n]) {
                    if tx.send(frame).is_err() {
                        return;
                    }
                }
            }
            Err(_) => break,
        }
    }
}

/// §2.5 stderr 只用于日志，宿主捕获其文本供崩溃诊断（验收 A8 的可观测性）。
fn capture_stderr(mut stderr: std::process::ChildStderr, sink: Arc<Mutex<Vec<u8>>>) {
    let mut buf = [0u8; 1024];
    let mut acc: Vec<u8> = Vec::new();
    loop {
        match stderr.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if acc.len() < STDERR_CAPTURE_LIMIT {
                    acc.extend_from_slice(&buf[..n]);
                }
            }
        }
    }
    if let Ok(mut guard) = sink.lock() {
        *guard = acc;
    }
}

/// 等待进程退出；超时返回 `Ok(None)`（由调用方强杀，§6.6 后置规则 3）。
fn wait_for_exit(child: &mut Child, timeout: Duration) -> std::io::Result<Option<ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_protocol::messages::{ItemsChangedParams, SetClipboardParams};

    fn request(id: u64, method: &str) -> RawMessage {
        RawMessage {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Some(id),
            method: Some(method.to_string()),
            params: None,
            result: None,
            error: None,
        }
    }

    fn response(id: u64) -> RawMessage {
        RawMessage {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Some(id),
            method: None,
            params: None,
            result: Some(serde_json::json!({})),
            error: None,
        }
    }

    fn notification(method: &str) -> RawMessage {
        RawMessage {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: None,
            method: Some(method.to_string()),
            params: None,
            result: None,
            error: None,
        }
    }

    #[test]
    fn classifies_host_request_by_method_prefix() {
        // §3.3：带 id 且 method 是 host/* → 对端请求（即使 id 与本方空间重合）
        assert_eq!(
            classify(&request(1, "host/set_clipboard")),
            MessageKind::HostRequest
        );
        assert_eq!(
            classify(&request(1, "host/show_status")),
            MessageKind::HostRequest
        );
    }

    #[test]
    fn classifies_notification_and_response() {
        assert_eq!(
            classify(&notification("initialized")),
            MessageKind::Notification
        );
        assert_eq!(
            classify(&notification("items_changed")),
            MessageKind::Notification
        );
        assert_eq!(classify(&response(7)), MessageKind::Response(7));
    }

    #[test]
    fn unknown_messages_are_ignored_not_fatal() {
        let neither = RawMessage {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: None,
            method: None,
            params: None,
            result: None,
            error: None,
        };
        assert_eq!(classify(&neither), MessageKind::Unknown);
        // 带 id 但不是 host/*：对宿主而言形态非法，按 §3.3 忽略
        assert_eq!(
            classify(&request(1, "top_level_commands")),
            MessageKind::Unknown
        );
    }

    #[test]
    fn notification_shapes_stay_untouched() {
        // §7.1 items_changed 的参数可缺省 page_id
        let raw = r#"{"jsonrpc":"2.0","method":"items_changed","params":{}}"#;
        let parsed: ItemsChangedParams = serde_json::from_value(
            serde_json::from_str::<serde_json::Value>(raw).unwrap()["params"].clone(),
        )
        .expect("page_id 缺省应可解析");
        assert_eq!(parsed.page_id, None);

        let raw =
            r#"{"jsonrpc":"2.0","id":1,"method":"host/set_clipboard","params":{"text":"3.14159"}}"#;
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        let params: SetClipboardParams = serde_json::from_value(value["params"].clone()).unwrap();
        assert_eq!(params.text, "3.14159");
    }

    #[test]
    fn protocol_version_is_two_part_not_three() {
        // §13：协议版本是 MAJOR.MINOR 两段；"1.0.0" 对协议而言非法
        assert_eq!(parse_protocol_version("1.0"), Some((1, 0)));
        assert_eq!(parse_protocol_version("1.10"), Some((1, 10)));
        assert_eq!(parse_protocol_version("1.0.0"), None);
        assert_eq!(parse_protocol_version("1"), None);
    }

    #[test]
    fn timeout_error_maps_to_extension_timeout_code() {
        let err = ProtocolError::Timeout {
            method: "get_items".to_string(),
            timeout: Duration::from_millis(2000),
        };
        let rpc = err.as_rpc_error().expect("超时应映射为 RPC 错误");
        assert_eq!(rpc.code, error_codes::EXTENSION_TIMEOUT);

        let exited = ProtocolError::ProcessExited
            .as_rpc_error()
            .expect("进程退出应映射");
        assert_eq!(exited.code, error_codes::PROVIDER_UNAVAILABLE);
    }
}
