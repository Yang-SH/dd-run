//! 扩展子进程管理与 JSON-RPC 客户端。
//!
//! 契约来源：[`docs/protocol.md`](../../docs/protocol.md)：
//! §2 传输层（NDJSON）、§3 消息格式与 id 空间、§5 握手与版本协商、
//! §6 host→ext 方法、§7 ext→host 方法、§10 超时、§11 崩溃检测。
//!
//! 范围边界（M0）：实现 **spawn → initialize → top_level_commands → close**
//! 这一条链路，以及宿主侧对扩展反向请求（`host/*`）的识别与应答。
//! 页面栈、缓存、LRU 属 M1–M3，不在此处。

use std::collections::BTreeMap;
use std::io::Read;
use std::io::Write as _;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use dd_protocol::envelope::{self, Envelope};
use dd_protocol::framing::{encode, Decoder, Frame, DEFAULT_MAX_MESSAGE_BYTES};
use dd_protocol::messages::{
    error_codes, CommandListResult, GetCommandParams, GetCommandResult, HostInfo, InitializeParams,
    InitializeResult, InvokeParams, ItemsChangedParams, RawMessage, RpcError, TransportInfo,
    JSONRPC_VERSION,
};
use dd_protocol::methods::{
    HOST_METHOD_PREFIX, METHOD_CLOSE, METHOD_FALLBACK_COMMANDS, METHOD_GET_COMMAND,
    METHOD_INITIALIZE, METHOD_INVOKE, METHOD_TOP_LEVEL_COMMANDS, NOTIFY_ITEMS_CHANGED,
};
use dd_protocol::model::{CommandItem, CommandResult};

use crate::manifest::{current_platform, LoadedExtension, HOST_CAPABILITIES};

/// §10 各阶段默认超时。
pub const TIMEOUT_INITIALIZE: Duration = Duration::from_millis(5_000);
pub const TIMEOUT_TOP_LEVEL_COMMANDS: Duration = Duration::from_millis(3_000);
/// §10 `fallback_commands` = 2000 ms。
pub const TIMEOUT_FALLBACK_COMMANDS: Duration = Duration::from_millis(2_000);
/// §10 `get_command` = 5000 ms（含冷启动进程的 spawn 开销，协议 §10 表）。
pub const TIMEOUT_GET_COMMAND: Duration = Duration::from_millis(5_000);
/// §10 `get_items` = 2000 ms（首屏路径上的热路径）。
pub const TIMEOUT_GET_ITEMS: Duration = Duration::from_millis(2_000);
/// §10 `invoke` = 10000 ms（命令可能耗时，如启动应用）。
pub const TIMEOUT_INVOKE: Duration = Duration::from_millis(10_000);
/// §6.6 后置规则 3：`close` 超时即强杀。
pub const TIMEOUT_CLOSE_RESPONSE: Duration = Duration::from_millis(1_000);
pub const TIMEOUT_CLOSE_EXIT: Duration = Duration::from_millis(1_000);

/// 扩展 stderr 的保留上限（§2.5：宿主应捕获扩展 stderr 用于崩溃诊断）。
const STDERR_CAPTURE_LIMIT: usize = 64 * 1024;

/// 诊断摘要里 stderr 末行的字符上限——整段日志塞进 Toast/卡片会失控，
/// 而根因（解释器不在 PATH、Python traceback 末行）几乎总在最后一行。
const STDERR_SUMMARY_CHARS: usize = 200;

/// 诊断总线容量：`ExtensionProcess` 的 `notifications` / `unmatched` 只入不清
/// （无消费方，仅诊断留档），长会话下会无界增长——超容即丢弃最旧一条，恒保留
/// 最近 [`DIAGNOSTIC_BUS_CAP`] 条。in-process 适配器（dd-gui `ext_inprocess`）同口径。
pub const DIAGNOSTIC_BUS_CAP: usize = 64;

/// R-03：入站帧队列容量。读线程把切分好的帧推入队列，消费端（`poll_notifications`
/// / `call`）只在面板可见或有 in-flight 请求时排空——无界队列会被流氓/缺陷扩展在
/// 面板隐藏期间刷 stdout 撑爆内存（每帧 ≤1 MiB 不触发 TooLarge）。超容**丢新帧**
/// 并置位溢出标志（见 [`InboundGate`]），单帧上界 × 容量 = RSS 上界
/// （1 MiB × 128 ≈ ≤128 MiB，不再随注入增长）。
pub const INBOUND_QUEUE_CAP: usize = 128;

/// R-03：`host/*` 反向请求记录容量（drop-oldest）。UI 隐藏期间不消费，溢出需
/// **连续 32 条**未被取走的反向请求（异常扩展才可能，记日志留痕），恒保留最新
/// [`HOST_REQUESTS_CAP`] 条。in-process 适配器同口径。
pub const HOST_REQUESTS_CAP: usize = 32;

/// 有界入队：超容丢最旧、保序保最近（诊断用途，n ≤ cap，O(cap) 可忽略）。
/// 返回是否发生了「丢最旧」（调用方可据此记日志；诊断站点忽略返回值）。
fn push_capped(buf: &mut Vec<RawMessage>, cap: usize, msg: RawMessage) -> bool {
    let evicted = buf.len() >= cap;
    if evicted {
        buf.remove(0);
    }
    buf.push(msg);
    evicted
}

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
        (Some(method), Some(_)) if method.starts_with(HOST_METHOD_PREFIX) => {
            MessageKind::HostRequest
        }
        (Some(_), None) => MessageKind::Notification,
        (None, Some(id)) => MessageKind::Response(id),
        _ => MessageKind::Unknown,
    }
}

/// 一批消息（`serve_line` 产出，或子进程 stdout）按 §3.3 分类后的归属。
///
/// 子进程路径由后台读线程把消息分别入队（`poll_notifications` / `drain_host_requests`）；
/// **in-process 无读线程**，调用方直接对 `serve_line` 的返回值跑 [`route_messages`]，
/// 得到**同构**的归属——这是 M9 R1（两条路径逐字节等价）的单一事实来源。
#[derive(Debug, Default)]
pub struct RoutedMessages {
    /// 与请求 `id` 匹配的响应：`Ok(裸 result)` 或 `Err(RpcError)`。
    pub response: Option<Result<serde_json::Value, RpcError>>,
    /// §7.4 扩展 → 宿主的 `host/*` 反向请求。
    pub host_requests: Vec<RawMessage>,
    /// §7.1 通知（如 `items_changed`）。
    pub notifications: Vec<RawMessage>,
    /// 未匹配到 in-flight 请求的响应 / 非 `host/*` 的带 id 消息 / 信封非法（§3.2/§3.4）。
    pub unmatched: Vec<RawMessage>,
}

/// 把一批消息路由到「响应 / `host/*` 请求 / 通知 / 未匹配」。
///
/// - `request_id` = 期望匹配的响应 id（发起方自增计数器）。
/// - 非法 JSON-RPC 信封（§3.2/§3.4 校验失败）**不参与分发**，仅留痕、不致命（§9.3）；
///   校验规则与扩展侧共用 [`dd_protocol::envelope`]（单一来源）。
/// - 性能提示：`serve_line` 的返回**响应在前、副作用在后**，故本函数不提前返回，
///   一次遍历把同批次的 `host/*` 与通知全部收齐，与子进程「先收响应、副作用入队」
///   的终态等价。
pub fn route_messages(request_id: u64, outputs: Vec<serde_json::Value>) -> RoutedMessages {
    let mut routed = RoutedMessages::default();
    for value in outputs {
        let msg = match envelope::validate_value(value) {
            Envelope::Valid(msg) => msg,
            // in-process 路径不该出现非法信封（消息由本进程的 `serve_line` 产出），
            // 出现即实现侧缺陷；留痕以便诊断，不致命。
            Envelope::InvalidRequest { id, reason } => {
                routed
                    .unmatched
                    .push(unmatched_marker(id, Some(reason.kind())));
                continue;
            }
            Envelope::ParseError => {
                routed.unmatched.push(unmatched_marker(None, None));
                continue;
            }
        };
        match classify(&msg) {
            MessageKind::HostRequest => routed.host_requests.push(msg),
            MessageKind::Notification => routed.notifications.push(msg),
            MessageKind::Response(rid) => {
                if rid == request_id {
                    routed.response = Some(match msg.error {
                        Some(err) => Err(err),
                        None => Ok(msg.result.unwrap_or(serde_json::Value::Null)),
                    });
                } else {
                    routed.unmatched.push(msg);
                }
            }
            MessageKind::Unknown => routed.unmatched.push(msg),
        }
    }
    routed
}

/// 为「信封非法」的消息造留痕载体：`RawMessage` 表达不了非法信封本身，
/// 故把原因放进 `params.invalid_reason`。仅诊断用，不参与分发。
fn unmatched_marker(id: Option<u64>, invalid_reason: Option<&str>) -> RawMessage {
    RawMessage {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id,
        method: None,
        params: invalid_reason.map(|r| serde_json::json!({ "invalid_reason": r })),
        result: None,
        error: None,
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
    /// R-03：入站帧队列溢出标志（读线程置位，[`Self::take_inbound_overflow`]
    /// 观察并复位）
    overflown: Arc<AtomicBool>,
    /// R-03：本溢出 episode 累计丢弃帧数（与 `overflown` 配对复位）
    dropped: Arc<AtomicU64>,
}

/// 清单 `entry.env` **不得覆盖**的宿主关键环境变量（S-10，2026-09-23）。
///
/// 为什么需要：`entry.env` 由清单作者（即任意本地 JSON 的作者）完全控制，加固前
/// 原样 `envs()` 注入意味着可改写 `PATH` / `ComSpec` / `SystemRoot` / `USERPROFILE` 等
/// ——被 spawn 的扩展及其**全部子进程**都会按被篡改的查找路径解析可执行文件与系统目录，
/// 构成劫持面（见 `docs/security-audit-2026-09-23.md` §5 S-10）。
///
/// 业务变量**不在**表内（`DDRUN_LANG`、`DD_WEBSEARCH_ENGINES` 等），注入行为不变。
/// 比对**大小写不敏感**（Windows 环境变量名不区分大小写，`Path` 与 `PATH` 等价）。
pub const PROTECTED_ENV_KEYS: &[&str] = &[
    "PATH",
    "PATHEXT",
    "COMSPEC",
    "SYSTEMROOT",
    "WINDIR",
    "SYSTEMDRIVE",
    "SYSTEM32",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PROGRAMW6432",
    "OS",
    "PROCESSOR_ARCHITECTURE",
    "TEMP",
    "TMP",
];

/// 过滤清单声明的环境变量覆盖 → `(保留, 被拒)`（纯函数，便于单测）。
///
/// 返回顺序与 [`BTreeMap`] 的键序一致（确定性）；被拒键交由调用方记日志，
/// **不静默丢弃**（否则表现为"环境变量莫名不生效"）。
pub fn filter_env_overrides(
    env: &BTreeMap<String, String>,
) -> (Vec<(String, String)>, Vec<String>) {
    let mut kept = Vec::with_capacity(env.len());
    let mut rejected = Vec::new();
    for (key, value) in env {
        if PROTECTED_ENV_KEYS
            .iter()
            .any(|p| p.eq_ignore_ascii_case(key))
        {
            rejected.push(key.to_string());
        } else {
            kept.push((key.to_string(), value.to_string()));
        }
    }
    (kept, rejected)
}

impl ExtensionProcess {
    /// §4 `spawned`：按清单启动子进程，接管 stdin/stdout/stderr。
    ///
    /// S-10：`entry.env` 经 [`filter_env_overrides`] 过滤——受保护的宿主关键变量
    /// （`PATH` / `SystemRoot` / …）被拒并记 warn，其余照常注入。
    pub fn spawn(ext: &LoadedExtension) -> Result<Self, std::io::Error> {
        let (env_keep, env_rejected) = filter_env_overrides(&ext.manifest.entry.env);
        if !env_rejected.is_empty() {
            log::warn!(
                "[dd-host] 扩展 {} 的 entry.env 试图覆盖受保护的宿主环境变量，已忽略：{}",
                ext.manifest.id,
                env_rejected.join(", ")
            );
        }
        let mut command = Command::new(&ext.command);
        command
            .args(&ext.manifest.entry.args)
            .envs(env_keep)
            .current_dir(&ext.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW（真机 2026-09-05 反馈）：宿主改为 windows 子系统
            // （无控制台）后，console 子系统的扩展进程若不加此标志会各自弹出
            // 独立控制台窗口（此前宿主是 console 子系统，子进程继承其控制台，
            // 问题被掩盖）。stdio 全部 piped，隐藏控制台不影响 NDJSON 通道。
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command.spawn()?;
        let stdin = child.stdin.take().expect("stdin 已 piped");
        let stdout = child.stdout.take().expect("stdout 已 piped");
        let stderr = child.stderr.take().expect("stderr 已 piped");

        let (tx, rx) = mpsc::sync_channel(INBOUND_QUEUE_CAP);
        let overflown = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicU64::new(0));
        thread::spawn({
            let overflown = Arc::clone(&overflown);
            let dropped = Arc::clone(&dropped);
            move || {
                read_loop(
                    stdout,
                    InboundGate {
                        tx,
                        overflown,
                        dropped,
                    },
                )
            }
        });

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
            overflown,
            dropped,
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
            METHOD_INITIALIZE,
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
            METHOD_TOP_LEVEL_COMMANDS,
            serde_json::json!({}),
            TIMEOUT_TOP_LEVEL_COMMANDS,
        )?;
        let result: CommandListResult = serde_json::from_value(value)?;
        Ok(result.commands)
    }

    /// §6.4 按 id 取回真实命令——**frozen 桩复热链路**的一环
    /// （协议 §6.4：点击 frozen 桩 → spawn → `initialize` → `get_command` → 取回真实命令后执行）。
    ///
    /// `Ok(None)` = 扩展答复 `command: null`（该桩已失效，**正常结果、非错误**，
    /// 宿主应回退 stub 并向用户报错）；`Err` 才是协议/超时/进程故障。
    pub fn get_command(&mut self, id: &str) -> Result<Option<CommandItem>, ProtocolError> {
        let value = self.call(
            METHOD_GET_COMMAND,
            serde_json::to_value(GetCommandParams { id: id.to_string() })?,
            TIMEOUT_GET_COMMAND,
        )?;
        let result: GetCommandResult = serde_json::from_value(value)?;
        Ok(result.command)
    }

    /// §6.2 拉取兜底命令模板列表（用户输入未命中顶层命令时宿主调用）。
    ///
    /// 返回的命令 `title` 含 `{query}` 占位符，宿主渲染时替换为当前搜索词；
    /// **空列表表示该 provider 无兜底能力**（协议 §6.2：宿主以结果非空判定 fresh）。
    pub fn fallback_commands(&mut self) -> Result<Vec<CommandItem>, ProtocolError> {
        let value = self.call(
            METHOD_FALLBACK_COMMANDS,
            serde_json::json!({}),
            TIMEOUT_FALLBACK_COMMANDS,
        )?;
        let result: CommandListResult = serde_json::from_value(value)?;
        Ok(result.commands)
    }

    /// §6.5 执行一条命令。`params` 携带 id / sender / context（含 `query`、`confirmed` 等）。
    ///
    /// 返回 §8.3 `CommandResult` 本体——`call` 已解开 JSON-RPC 信封，其内层
    /// `result` 即 `CommandResult`，直接解析；**不要**再用 `InvokeResult`
    /// （要求 `result` 字段）包一层，否则成功响应会报「missing field `result`」
    /// （M2 修复记录，见 dd-gui `invoke_on` 注释）。
    pub fn invoke(&mut self, params: &InvokeParams) -> Result<CommandResult, ProtocolError> {
        let value = self.call(METHOD_INVOKE, serde_json::to_value(params)?, TIMEOUT_INVOKE)?;
        let result: CommandResult = serde_json::from_value(value)?;
        Ok(result)
    }

    /// §6.6 优雅关闭：发 `close` → 等 result → 等进程自行退出；超时则强杀。
    pub fn close(mut self) -> Result<(), CloseError> {
        self.call(METHOD_CLOSE, serde_json::json!({}), TIMEOUT_CLOSE_RESPONSE)
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

    /// §11：进程已退出时的退出码（`None` = 仍在运行 / 信号终止等无码退出）。
    /// 用于区分「崩溃（非 0 退出码 / EOF）」与「正常退出（0）」——M4 熔断只对崩溃计数。
    pub fn exit_status(&mut self) -> Option<ExitStatus> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status),
            _ => None,
        }
    }

    /// §2.5：已捕获的扩展 stderr（截断到 [`STDERR_CAPTURE_LIMIT`]）。
    pub fn stderr(&self) -> String {
        let bytes = self.stderr.lock().map(|g| g.clone()).unwrap_or_default();
        String::from_utf8_lossy(&bytes).to_string()
    }

    /// 已捕获 stderr 的**最后一条非空行**（失败提示用），超长按**字符**截断。
    pub fn stderr_last_line(&self) -> Option<String> {
        last_nonempty_line(&self.stderr(), STDERR_SUMMARY_CHARS)
    }

    /// 失败诊断摘要：`退出码 N / 被信号终止；stderr: <末行>`（任一部分缺失则省略）。
    ///
    /// **为什么需要**（2026-09-10 真机反馈驱动）：宿主此前在扩展启动失败/崩溃时只报
    /// 「连续崩溃 N 次」这类无信息量文案，根因（`'python' 不是内部或外部命令`、扩展
    /// 自身的 traceback）全都躺在已捕获的 stderr 里没人读——两次真机排查都因此绕远路。
    /// 调用方把本摘要拼进错误提示与日志即可让用户/开发者直接看到原因。
    ///
    /// 进程尚存活时（握手超时）`exit_status()` 为 `None`，此时仍有 stderr 末行可看。
    pub fn failure_detail(&mut self) -> Option<String> {
        let exit = self.exit_status().map(|st| match st.code() {
            Some(code) => format!("退出码 {code}"),
            None => "被信号终止".to_string(),
        });
        compose_failure_detail(exit, self.stderr_last_line())
    }

    /// §7.1 通知轮询（非阻塞）：在没有 in-flight 请求时消费扩展发来的消息。
    ///
    /// 返回本次轮询收到的 `items_changed` 的 `page_id`（`None` 表示"顶层
    /// 命令变了"）。宿主据此在 UI 空闲时触发**全量重拉**
    /// （§6.3 + 验收 A9：协议层不做增量推送）。
    ///
    /// M4（`host/*` 执行端，见 [`docs/m4-record.md`](../../../docs/m4-record.md) P2）：
    /// 空闲轮询同样处理扩展发来的 **`host/*` 请求**——按 §7.4 能力前置应答
    /// （已声明回 `{}`，未声明回 `-32601`），并把请求记入 [`Self::host_requests`]
    /// 供 UI 层取走执行真实副作用（Toast / 剪贴板 / 开 URL）。此前这类请求只
    /// 在 [`Self::call`] 等待期间应答，空闲到达会被静默丢弃导致扩展等待超时。
    ///
    /// 所有通知仍会记入 [`Self::notifications`]（有界：恒保留最近
    /// [`DIAGNOSTIC_BUS_CAP`] 条），供诊断与后续处理。
    pub fn poll_notifications(&mut self) -> Vec<Option<String>> {
        let mut changed = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(Frame::Message(line)) => {
                    let msg = match envelope::validate(&line) {
                        Envelope::Valid(msg) => msg,
                        // §3.2/§3.4：非法信封 → 回错（`-32700` / `-32600`）后继续，非致命（§9.3）
                        other => {
                            let _ = self.reply_envelope_error(&other);
                            continue;
                        }
                    };
                    match classify(&msg) {
                        // §7.4：host/* 请求 → 应答并记录（UI 层消费执行副作用）
                        MessageKind::HostRequest => {
                            let _ = self.answer_host_request(&msg);
                        }
                        _ => {
                            if msg.method.as_deref() == Some(NOTIFY_ITEMS_CHANGED) {
                                // §7.1：params 可缺省，缺省即"顶层"
                                let page_id = msg
                                    .params
                                    .as_ref()
                                    .and_then(|v| {
                                        serde_json::from_value::<ItemsChangedParams>(v.clone()).ok()
                                    })
                                    .and_then(|p| p.page_id);
                                changed.push(page_id);
                            }
                            push_capped(&mut self.notifications, DIAGNOSTIC_BUS_CAP, msg);
                        }
                    }
                }
                // §2.3 + §9.3：超限帧 → 回 `-32600` 并**关闭连接**（`-32600` 中唯一致命的场景）
                Ok(Frame::TooLarge { size, max }) => {
                    self.abort_oversized(size, max);
                    break;
                }
                // §2.2 规则 6：非 UTF-8 无对应 JSON-RPC 错误码，通知路径静默丢弃
                Ok(Frame::InvalidUtf8) => {}
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        changed
    }

    /// 取走并清空积压的 `host/*` 请求记录（M4 P2：UI 层消费并执行真实副作用）。
    pub fn drain_host_requests(&mut self) -> Vec<RawMessage> {
        std::mem::take(&mut self.host_requests)
    }

    /// R-03：观察入站帧队列是否发生过溢出（面板隐藏期间扩展刷 stdout，读线程
    /// 丢新帧——见 [`INBOUND_QUEUE_CAP`]）。有则返回**本 episode 丢弃的帧数**
    /// 并复位标志（下次溢出重新置位），无则 `None`。
    ///
    /// 消费端（dd-gui 轮询）借此**合成一条告警**：本方法的 `log::warn!` +
    /// UI 层 toast（每扩展每会话至多一次，防连续溢出刷屏）。复位后再次溢出
    /// 会再次置位——持续异常的扩展在日志里逐 episode 留痕，UI 不重复轰炸。
    pub fn take_inbound_overflow(&self) -> Option<u64> {
        let n = take_overflow_flag(&self.overflown, &self.dropped)?;
        log::warn!(
            "[dd-host] 扩展 {} 入站队列溢出（容量 {} 帧），本 episode 丢弃 {n} 帧（输出过快，面板不可见期间无人排空）",
            self.id,
            INBOUND_QUEUE_CAP
        );
        Some(n)
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
                    // §2.3 + §9.3：回 `-32600` 并关闭连接（致命——继续读取会导致流错位）
                    self.abort_oversized(size, max);
                    return Err(ProtocolError::MessageTooLarge { size, max });
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
        let msg = match envelope::validate(line) {
            Envelope::Valid(msg) => msg,
            // §3.2/§3.4：非法信封 → 回错（`-32700` / `-32600`）后继续等目标响应（§9.3 不致命）
            other => {
                self.reply_envelope_error(&other)?;
                return Ok(None);
            }
        };
        match classify(&msg) {
            MessageKind::HostRequest => {
                self.answer_host_request(&msg)?;
                Ok(None)
            }
            MessageKind::Notification => {
                push_capped(&mut self.notifications, DIAGNOSTIC_BUS_CAP, msg);
                Ok(None)
            }
            MessageKind::Response(rid) => {
                if rid != waiting_id {
                    // §3.3：未匹配到 in-flight 请求的响应，记日志并忽略
                    push_capped(&mut self.unmatched, DIAGNOSTIC_BUS_CAP, msg);
                    return Ok(None);
                }
                match msg.error {
                    Some(err) => Err(ProtocolError::Rpc(err)),
                    None => Ok(Some(msg.result.unwrap_or(serde_json::Value::Null))),
                }
            }
            MessageKind::Unknown => {
                push_capped(&mut self.unmatched, DIAGNOSTIC_BUS_CAP, msg);
                Ok(None)
            }
        }
    }

    /// 把信封校验失败的结果回给对端（`-32700` / `-32600`），并留痕进 [`Self::unmatched`]。
    ///
    /// **非致命**：连接保持（§9.3）。致命的那一支是超限帧，见 [`Self::abort_oversized`]。
    fn reply_envelope_error(&mut self, result: &Envelope) -> Result<(), ProtocolError> {
        let (id, reason) = match result {
            Envelope::InvalidRequest { id, reason } => (*id, Some(reason.kind())),
            _ => (None, None),
        };
        push_capped(
            &mut self.unmatched,
            DIAGNOSTIC_BUS_CAP,
            unmatched_marker(id, reason),
        );
        match result.to_error_response() {
            Some(response) => self.write_message(&response),
            None => Ok(()),
        }
    }

    /// §2.3 + §9.3：超限帧的处置——回 `-32600 Invalid Request`（id 无法可靠取得，用 `null`）
    /// 并**关闭连接**。关闭即终止子进程：继续读取会导致流错位，且扩展已无法自证同步。
    ///
    /// `-32600` 有多类触发，§9.3 只把「消息超上限」这一支列为致命；批处理 / 非法 `jsonrpc`
    /// 等走 [`Self::reply_envelope_error`]，回错后连接保持。
    fn abort_oversized(&mut self, size: usize, max: usize) {
        let response = dd_protocol::envelope::error_response(
            None,
            error_codes::INVALID_REQUEST,
            "Invalid Request",
            Some(serde_json::json!({
                "reason": "message_too_large",
                "size": size,
                "max": max,
            })),
        );
        // 尽力送达：对端已错位时写入失败属预期，不应掩盖原始超限错误
        let _ = self.write_message(&response);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// §7.4 能力前置：扩展只能用 `initialize` 里声明过的 `host/*` 方法，
    /// 未声明而调用 → `-32601 Method not found`。
    fn answer_host_request(&mut self, msg: &RawMessage) -> Result<(), ProtocolError> {
        let method = msg.method.clone().unwrap_or_default();
        let id = msg.id.unwrap_or(0);
        // R-03：drop-oldest 定容——溢出需连续 32 条未被 UI 取走的反向请求
        //（正常扩展不可能），记日志留痕后仍应答本条（§7.4 语义不变）
        if push_capped(&mut self.host_requests, HOST_REQUESTS_CAP, msg.clone()) {
            log::warn!(
                "[dd-host] 扩展 {} host/* 请求积压超 {} 条（UI 未消费），丢弃最旧一条",
                self.id,
                HOST_REQUESTS_CAP
            );
        }

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

/// R-03：读线程 → 消费端的**有界**入站门（容量 [`INBOUND_QUEUE_CAP`]）。
///
/// 满则**丢新帧**并置位 `overflown`、累计 `dropped`（消费端经
/// [`ExtensionProcess::take_inbound_overflow`] 观察后合成告警）。**不阻塞**
/// 读线程——阻塞会反压子进程 stdout 管道（缺陷扩展被"冻结"且宿主读线程失去
/// 响应性），丢帧让异常只影响该扩展自身的事件流，不殃及宿主。
struct InboundGate {
    tx: SyncSender<Frame>,
    overflown: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
}

impl InboundGate {
    /// 投递一帧。返回 `false` = 消费端已消失（进程被丢弃），读线程应退出；
    /// 队列满（丢新帧）与成功均返回 `true`（继续读）。
    fn deliver(&self, frame: Frame) -> bool {
        match self.tx.try_send(frame) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                self.overflown.store(true, Ordering::Relaxed);
                self.dropped.fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }
}

/// R-03：观察并复位溢出标志——[`ExtensionProcess::take_inbound_overflow`]
/// 与单测夹具共用的单一事实来源（`Some` = 本 episode 丢弃帧数，取走即复位）。
fn take_overflow_flag(overflown: &AtomicBool, dropped: &AtomicU64) -> Option<u64> {
    if overflown.swap(false, Ordering::Relaxed) {
        Some(dropped.swap(0, Ordering::Relaxed))
    } else {
        None
    }
}

/// §2.4 读循环：一次 `read` 可能返回半条或多条消息，交给 [`Decoder`] 累积切分。
fn read_loop(mut stdout: std::process::ChildStdout, gate: InboundGate) {
    let mut decoder = Decoder::with_default_limit();
    let mut buf = [0u8; 8192];
    loop {
        match stdout.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for frame in decoder.push(&buf[..n]) {
                    if !gate.deliver(frame) {
                        return;
                    }
                }
            }
            Err(_) => break,
        }
    }
}

/// 取文本中**最后一条非空行**并按字符截断（`\r` 一并 trim，兼容 CRLF 与中文）。
///
/// 纯函数便于单测：截断按**字符**计数，中文不会被截成半个字。
fn last_nonempty_line(text: &str, max_chars: usize) -> Option<String> {
    let line = text.lines().rev().find(|l| !l.trim().is_empty())?.trim();
    let count = line.chars().count();
    if count <= max_chars {
        return Some(line.to_string());
    }
    let mut out: String = line.chars().take(max_chars).collect();
    out.push('…');
    Some(out)
}

/// 纯函数：把「退出码描述 + stderr 末行」拼成诊断摘要（两者皆空 → `None`）。
fn compose_failure_detail(exit: Option<String>, stderr_tail: Option<String>) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(exit) = exit {
        parts.push(exit);
    }
    if let Some(tail) = stderr_tail {
        parts.push(format!("stderr: {tail}"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("；"))
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

    /// R-03：入站队列定容——注入 >[`INBOUND_QUEUE_CAP`] 帧，队列长度恒为容量、
    /// `overflown` 标志置位、丢弃数准确（drop-new：保留最早 128 帧，丢最新的）。
    #[test]
    fn r03_inbound_queue_bounded_flag() {
        let (tx, rx) = mpsc::sync_channel(INBOUND_QUEUE_CAP);
        let gate = InboundGate {
            tx,
            overflown: Arc::new(AtomicBool::new(false)),
            dropped: Arc::new(AtomicU64::new(0)),
        };
        let excess = 72u64;
        for _ in 0..(INBOUND_QUEUE_CAP as u64 + excess) {
            assert!(gate.deliver(Frame::InvalidUtf8), "读线程侧投递不因满而失败");
        }
        assert!(gate.overflown.load(Ordering::Relaxed), "溢出标志应置位");
        assert_eq!(
            gate.dropped.load(Ordering::Relaxed),
            excess,
            "丢弃数应恰为超出量"
        );
        let mut received = 0;
        while rx.try_recv().is_ok() {
            received += 1;
        }
        assert_eq!(
            received, INBOUND_QUEUE_CAP,
            "队列长度恒为容量（无界增长已封顶）"
        );
        // take_inbound_overflow 观察语义：取走即复位
        let n = take_overflow_flag(&gate.overflown, &gate.dropped).expect("溢出应可被观察");
        assert_eq!(n, excess);
        assert!(
            take_overflow_flag(&gate.overflown, &gate.dropped).is_none(),
            "复位后下一次观察应为 None（episode 语义）"
        );
    }

    /// R-03 负例：恰达容量不置位溢出（正常流量零告警）。
    #[test]
    fn r03_inbound_queue_at_capacity_no_flag() {
        let (tx, rx) = mpsc::sync_channel(INBOUND_QUEUE_CAP);
        let gate = InboundGate {
            tx,
            overflown: Arc::new(AtomicBool::new(false)),
            dropped: Arc::new(AtomicU64::new(0)),
        };
        for _ in 0..INBOUND_QUEUE_CAP {
            assert!(gate.deliver(Frame::InvalidUtf8));
        }
        assert!(
            !gate.overflown.load(Ordering::Relaxed),
            "未超容不得置位（不误报）"
        );
        assert_eq!(gate.dropped.load(Ordering::Relaxed), 0);
        assert!(
            take_overflow_flag(&gate.overflown, &gate.dropped).is_none(),
            "未超容不得置位（不误报）"
        );
        let mut received = 0;
        while rx.try_recv().is_ok() {
            received += 1;
        }
        assert_eq!(received, INBOUND_QUEUE_CAP);
    }

    /// R-03：`host/*` 请求记录 drop-oldest——>[`HOST_REQUESTS_CAP`] 条时恒保留
    /// 最新 32 条、最早被丢（UI 隐藏期间积压不无界增长）。
    #[test]
    fn r03_host_requests_drop_oldest() {
        let mut buf: Vec<RawMessage> = Vec::new();
        let total = HOST_REQUESTS_CAP + 8;
        for i in 0..total {
            let evicted = push_capped(&mut buf, HOST_REQUESTS_CAP, request(i as u64, "host/noop"));
            assert_eq!(evicted, i >= HOST_REQUESTS_CAP, "第 {} 条的逐出语义", i);
        }
        assert_eq!(buf.len(), HOST_REQUESTS_CAP, "恒保留最新 32 条");
        assert_eq!(
            buf[0].id,
            Some(8),
            "最早的 8 条已丢（首条应为第 9 条，id=8）"
        );
        assert_eq!(
            buf.last().unwrap().id,
            Some(total as u64 - 1),
            "最新一条在尾（保序保最近）"
        );
    }

    /// M9：`route_messages` 把一批消息正确切到「响应 / host 请求 / 通知 / 未匹配」。
    #[test]
    fn route_messages_splits_by_kind() {
        let mut resp = response(7);
        resp.result = Some(serde_json::json!({ "ok": true }));
        let outputs = vec![
            serde_json::to_value(resp).unwrap(),
            serde_json::to_value(request(1, "host/set_clipboard")).unwrap(),
            serde_json::to_value(notification("items_changed")).unwrap(),
            serde_json::to_value(response(99)).unwrap(), // id 不符 → unmatched
            serde_json::json!({ "garbage": true }),      // 信封非法 → 也留痕 unmatched
        ];
        let routed = route_messages(7, outputs);
        let ok = match routed.response {
            Some(Ok(v)) => v,
            _ => panic!("应有成功响应"),
        };
        assert_eq!(ok, serde_json::json!({ "ok": true }));
        assert_eq!(routed.host_requests.len(), 1);
        assert_eq!(
            routed.host_requests[0].method.as_deref(),
            Some("host/set_clipboard")
        );
        assert_eq!(routed.notifications.len(), 1);
        assert_eq!(
            routed.notifications[0].method.as_deref(),
            Some("items_changed")
        );
        assert_eq!(
            routed.unmatched.len(),
            2,
            "id 不符的响应 + 信封非法者都进 unmatched（后者仅留痕，不致命）"
        );
        assert_eq!(routed.unmatched[0].id, Some(99));
        assert_eq!(
            routed.unmatched[1].params.as_ref().unwrap()["invalid_reason"],
            "missing_jsonrpc"
        );
    }

    /// M9：错误响应映射为 `Err(RpcError)`，且**不影响**同批次副作用的路由
    /// （`serve_line` 响应在前、副作用在后）。
    #[test]
    fn route_messages_maps_error_response_and_keeps_effects() {
        let mut err = response(3);
        err.result = None;
        err.error = Some(RpcError {
            code: -32005,
            message: "Page not found".to_string(),
            data: None,
        });
        let outputs = vec![
            serde_json::to_value(err).unwrap(),
            serde_json::to_value(notification("items_changed")).unwrap(),
        ];
        let routed = route_messages(3, outputs);
        let code = match routed.response {
            Some(Err(e)) => e.code,
            _ => panic!("应为 Err(RpcError)"),
        };
        assert_eq!(code, -32005);
        assert_eq!(
            routed.notifications.len(),
            1,
            "错误响应同批次的副作用仍被路由"
        );
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
    fn route_messages_marks_invalid_envelopes_without_fatal() {
        // §3.2/§3.4：非法信封不参与分发，仅留痕（§9.3 非致命）；
        // 同批次里的合法响应仍必须匹配成功。
        let outputs = vec![
            serde_json::json!([{ "jsonrpc": "2.0", "id": 1, "method": "initialize" }]),
            serde_json::json!({ "jsonrpc": "1.0", "id": 2, "method": "initialize" }),
            serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": { "ok": true } }),
        ];
        let routed = route_messages(1, outputs);
        assert!(routed.response.is_some(), "合法响应仍应匹配 in-flight id");
        assert_eq!(routed.unmatched.len(), 2, "两条非法信封应留痕");
        assert_eq!(
            routed.unmatched[0].params.as_ref().unwrap()["invalid_reason"],
            "batch_array"
        );
        assert_eq!(
            routed.unmatched[1].params.as_ref().unwrap()["invalid_reason"],
            "unsupported_jsonrpc_version"
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

    // ── 失败诊断摘要（2026-09-10 真机反馈驱动） ──

    /// 取**最后一条非空行**：忽略尾部空行，CRLF 的 `\r` 被 trim。
    #[test]
    fn last_nonempty_line_picks_tail_and_filters_blanks() {
        assert_eq!(last_nonempty_line("", 100), None);
        assert_eq!(last_nonempty_line("  \n\n\t\r\n", 100), None);
        assert_eq!(
            last_nonempty_line("first\nsecond\n\n", 100).as_deref(),
            Some("second")
        );
        assert_eq!(
            last_nonempty_line("a\r\nb\r\n", 100).as_deref(),
            Some("b"),
            "CRLF 的 \\r 应被 trim"
        );
    }

    /// 截断按**字符**计数并加省略号（中文不得被截成半个字）。
    #[test]
    fn last_nonempty_line_truncates_by_chars() {
        assert_eq!(last_nonempty_line("abcdef", 3).as_deref(), Some("abc…"));
        assert_eq!(
            last_nonempty_line("中文诊断日志", 2).as_deref(),
            Some("中文…")
        );
        assert_eq!(
            last_nonempty_line("abc", 3).as_deref(),
            Some("abc"),
            "恰好等长不截断"
        );
    }

    /// 摘要拼装：退出码在前、stderr 在后；两者皆空 → `None`（不产出空壳提示）。
    #[test]
    fn compose_failure_detail_joins_parts() {
        assert_eq!(compose_failure_detail(None, None), None);
        assert_eq!(
            compose_failure_detail(Some("退出码 1".into()), None).as_deref(),
            Some("退出码 1")
        );
        assert_eq!(
            compose_failure_detail(None, Some("boom".into())).as_deref(),
            Some("stderr: boom"),
            "进程仍存活（握手超时）时只有 stderr 可看"
        );
        assert_eq!(
            compose_failure_detail(Some("退出码 1".into()), Some("boom".into())).as_deref(),
            Some("退出码 1；stderr: boom")
        );
    }

    // ── S-10（2026-09-23）：entry.env 不得覆盖宿主关键环境变量 ──────────

    /// 受保护键（大小写不敏感）被拒并**回传**给调用方记日志；其余照常保留。
    #[test]
    fn filter_env_overrides_blocks_protected_keys_case_insensitively() {
        let mut env = BTreeMap::new();
        env.insert("PATH".to_string(), r"C:\evil".to_string());
        env.insert("Path".to_string(), r"C:\evil2".to_string());
        env.insert("systemroot".to_string(), r"C:\fake".to_string());
        env.insert("ComSpec".to_string(), r"C:\evil\cmd.exe".to_string());
        env.insert("DD_EXT_FOO".to_string(), "1".to_string());

        let (kept, rejected) = filter_env_overrides(&env);
        assert_eq!(
            kept,
            vec![("DD_EXT_FOO".to_string(), "1".to_string())],
            "仅业务变量保留"
        );
        assert_eq!(
            rejected,
            vec!["ComSpec", "PATH", "Path", "systemroot"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            "受保护键全部被拒（键序 = BTreeMap 序）"
        );
    }

    /// 业务变量与语言通道必须**原样保留**（改这两条会切断 i18n 与搜索引擎配置）。
    #[test]
    fn filter_env_overrides_keeps_business_vars() {
        let mut env = BTreeMap::new();
        env.insert("DDRUN_LANG".to_string(), "en_us".to_string());
        env.insert("DD_WEBSEARCH_ENGINES".to_string(), "[]".to_string());
        env.insert("CUSTOM_FLAG".to_string(), "yes".to_string());

        let (kept, rejected) = filter_env_overrides(&env);
        assert!(rejected.is_empty(), "业务变量不得被拒：{rejected:?}");
        assert_eq!(kept.len(), 3);
        assert!(kept.iter().any(|(k, v)| k == "DDRUN_LANG" && v == "en_us"));
        assert!(kept
            .iter()
            .any(|(k, v)| k == "DD_WEBSEARCH_ENGINES" && v == "[]"));
    }
}
