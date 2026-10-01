//! M9 B3：扩展客户端统一抽象（内置 in-process / 第三方·sidecar 子进程）。
//!
//! 迁移前宿主只持 `dd_host::process::ExtensionProcess`（子进程 JSON-RPC）。M9 起
//! 5 个内置扩展改为**进程内**驱动（[`crate::ext_inprocess::InProcessExtension`]，
//! 直接调 `dd_ext::serve_line`，不 spawn / 不物化磁盘），但宿主侧其余链路
//! （进程池 / 复热 / 通知轮询 / host/* 执行 / 崩溃巡检）**语义不变**。
//!
//! [`ExtClient`] 把两种后端收敛到**同一套方法表面**（与 `ExtensionProcess` 逐方法
//! 对应），故 `app/*` 各调用点只需把类型从 `ExtensionProcess` 换成 `ExtClient`，
//! 逻辑保持逐字不变——这是 B3 的低回归落法（D3 落法 1：独立注册结构 + 枚举调度）。
//!
//! 差异口径（in-process 侧）：
//! - `has_exited` 恒 `false`（无独立进程）、`exit_status` / `failure_detail` 恒
//!   `None`（无退出码 / stderr）→ 崩溃巡检与熔断对内置天然不触发；
//! - `close` 为 noop（无进程可退，仅触发扩展侧 handler 以贴合协议 §6.6）；
//! - 协议方法（initialize / top_level / fallback / get_command / get_items /
//!   invoke）与子进程路径**同语义**，返回消息形状一致（B2 已用 parity 单测锁定）。

use std::process::ExitStatus;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use dd_ext::ExtensionSpec;
use dd_host::manifest::LoadedExtension;
use dd_host::process::{CloseError, ExtensionProcess, ProtocolError, TIMEOUT_GET_ITEMS};
use dd_protocol::messages::{
    error_codes, GetItemsParams, GetItemsResult, InitializeResult, InvokeParams, RawMessage,
    RpcError,
};
use dd_protocol::methods::{
    METHOD_FALLBACK_COMMANDS, METHOD_GET_COMMAND, METHOD_GET_ITEMS, METHOD_INITIALIZE,
    METHOD_INVOKE, METHOD_TOP_LEVEL_COMMANDS,
};
use dd_protocol::model::{CommandItem, CommandResult};

use crate::ext_inprocess::InProcessExtension;

/// R-04：in-process 调用统一超时（V-11 判据 T，**初值 5 s**；真机首屏实测
/// 校准后在方案 §10 留痕）。
///
/// in-process 本是纯函数调用，但内置扩展的处理函数可能做**阻塞 I/O**——apps
/// 首调枚举 ~400 应用（COM + 逐项图标提取），死 UNC 快捷方式的 `is_file()` /
/// `GetImage` 可阻塞至网络超时：一旦挂死，聚合 `thread::scope` 的 join 即永久
/// 等待（首屏永不落地「加载中」）或该扩展 `inflight` 永占（回复 busy 直到重启）。
/// 故 in-process 调用与子进程路径一样包超时：超时返回协议层 `Timeout`，扩展
/// 对象随工作线程**自然滞留**（不强行 kill），本扩展本会话呈 Failed + Retry。
pub const INPROCESS_CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// 按注册信息打开客户端（M9 分流收口）：`spec` 命中 → in-process；否则子进程 spawn。
///
/// 供聚合（[`crate::aggregator`]）与复热链路（`app::pool` / `invoke` / `page`）共用，
/// 保证两条路径的后端选择口径一致。背景线程内亦可安全调用（参数按值捕获）。
pub fn open(
    spec: Option<ExtensionSpec>,
    ext: Option<&LoadedExtension>,
) -> Result<(ExtClient, InitializeResult), String> {
    match (spec, ext) {
        (Some(spec), _) => ExtClient::open_builtin(spec),
        (None, Some(ext)) => ExtClient::spawn_subprocess(ext),
        (None, None) => Err("扩展无注册信息（既非内置也无可执行文件）".to_string()),
    }
}

/// 宿主可调用的扩展客户端（两种后端，同一表面）。
pub enum ExtClient {
    /// 第三方 / sidecar 扩展：子进程 JSON-RPC（ADR-1 进程隔离仍对其生效）。
    Subprocess(ExtensionProcess),
    /// 内置扩展：进程内直接 `serve_line`（M9；无子进程、无 exe）。
    /// R-04：`ext` 在调用期间移入工作线程——`None` = 上次调用超时、对象随
    /// 线程滞留未归还（本会话不可用，呈 Failed）；`timeout` 可注入（单测缩短）。
    InProcess {
        ext: Option<InProcessExtension>,
        timeout: Duration,
    },
}

impl ExtClient {
    /// 子进程后端：spawn + §5.1 握手（复用既有 [`crate::aggregator`] 链路，
    /// 失败信息带可操作线索：命令路径 / stderr 末行）。
    pub fn spawn_subprocess(ext: &LoadedExtension) -> Result<(Self, InitializeResult), String> {
        let (proc, init) = crate::aggregator::spawn_and_initialize_with_info(ext)?;
        Ok((Self::Subprocess(proc), init))
    }

    /// 进程内后端：以运行期构造的 [`ExtensionSpec`] 建 in-process 扩展 + §5.1 握手。
    pub fn open_builtin(spec: ExtensionSpec) -> Result<(Self, InitializeResult), String> {
        let ext = InProcessExtension::new(spec);
        let id = ext.id().to_string();
        // R-04：initialize 同口径包超时（内置握手虽轻，统一防线）——构造即超时
        // 的扩展直接 open 失败，不会进入保活集。
        let (returned, init) =
            run_with_timeout(ext, METHOD_INITIALIZE, INPROCESS_CALL_TIMEOUT, |ext| {
                ext.initialize(
                    crate::aggregator::PROTOCOL_VERSION,
                    crate::aggregator::HOST_VERSION,
                )
            });
        let Some(ext) = returned else {
            return Err(format!(
                "in-process 扩展 {id} initialize 超时（{} ms）",
                INPROCESS_CALL_TIMEOUT.as_millis()
            ));
        };
        let init = init.map_err(|e| format!("initialize 失败：{e}"))?;
        Ok((
            Self::InProcess {
                ext: Some(ext),
                timeout: INPROCESS_CALL_TIMEOUT,
            },
            init,
        ))
    }

    /// 是否内置 in-process（诊断 / 单测用）。
    pub fn is_in_process(&self) -> bool {
        matches!(self, Self::InProcess { .. })
    }

    /// R-04：in-process 扩展对象是否**滞留**（上次调用超时、随工作线程未归还）
    /// ——`store_warm_process` 据此拒绝归回保活集并把该扩展本会话标记 Failed。
    pub fn is_in_process_hung(&self) -> bool {
        matches!(self, Self::InProcess { ext: None, .. })
    }

    /// §5.1 握手 + §5.3 版本协商（两后端同语义）。
    pub fn initialize(
        &mut self,
        protocol_version: &str,
        host_version: &str,
    ) -> Result<InitializeResult, ProtocolError> {
        match self {
            Self::Subprocess(p) => p.initialize(protocol_version, host_version),
            Self::InProcess { ext, timeout } => {
                let pv = protocol_version.to_string();
                let hv = host_version.to_string();
                Self::inproc_call(ext, *timeout, METHOD_INITIALIZE, move |ext| {
                    ext.initialize(&pv, &hv)
                })
            }
        }
    }

    /// §6.1 首屏顶层命令。
    pub fn top_level_commands(&mut self) -> Result<Vec<CommandItem>, ProtocolError> {
        match self {
            Self::Subprocess(p) => p.top_level_commands(),
            Self::InProcess { ext, timeout } => {
                Self::inproc_call(ext, *timeout, METHOD_TOP_LEVEL_COMMANDS, |ext| {
                    ext.top_level_commands()
                })
            }
        }
    }

    /// §6.2 兜底命令模板。
    pub fn fallback_commands(&mut self) -> Result<Vec<CommandItem>, ProtocolError> {
        match self {
            Self::Subprocess(p) => p.fallback_commands(),
            Self::InProcess { ext, timeout } => {
                Self::inproc_call(ext, *timeout, METHOD_FALLBACK_COMMANDS, |ext| {
                    ext.fallback_commands()
                })
            }
        }
    }

    /// §6.4 按 id 取回真实命令（`Ok(None)` = 扩展答复 `command: null`）。
    pub fn get_command(&mut self, id: &str) -> Result<Option<CommandItem>, ProtocolError> {
        match self {
            Self::Subprocess(p) => p.get_command(id),
            Self::InProcess { ext, timeout } => {
                let id = id.to_string();
                Self::inproc_call(ext, *timeout, METHOD_GET_COMMAND, move |ext| {
                    ext.get_command(&id)
                })
            }
        }
    }

    /// §6.5 执行一条命令。
    pub fn invoke(&mut self, params: &InvokeParams) -> Result<CommandResult, ProtocolError> {
        match self {
            Self::Subprocess(p) => p.invoke(params),
            Self::InProcess { ext, timeout } => {
                let params = params.clone();
                Self::inproc_call(ext, *timeout, METHOD_INVOKE, move |ext| ext.invoke(&params))
            }
        }
    }

    /// §6.3 拉取整页项。子进程侧沿用既有 `call` + [`TIMEOUT_GET_ITEMS`] 口径；
    /// in-process 包 R-04 超时（此前「纯函数调用无超时」——apps 页 / 慢处理
    /// 同样可能挂死 `inflight`）。
    pub fn get_items(
        &mut self,
        page_id: &str,
        search_text: Option<String>,
    ) -> Result<GetItemsResult, ProtocolError> {
        match self {
            Self::Subprocess(p) => {
                let params = GetItemsParams {
                    page_id: page_id.to_string(),
                    search_text,
                };
                let value = serde_json::to_value(&params)?;
                let value = p.call(METHOD_GET_ITEMS, value, TIMEOUT_GET_ITEMS)?;
                Ok(serde_json::from_value(value)?)
            }
            Self::InProcess { ext, timeout } => {
                let page_id = page_id.to_string();
                Self::inproc_call(ext, *timeout, METHOD_GET_ITEMS, move |ext| {
                    ext.get_items(&page_id, search_text.as_deref())
                })
            }
        }
    }

    /// §7.1 通知轮询（非阻塞）：返回本次 `items_changed` 的 `page_id`。
    pub fn poll_notifications(&mut self) -> Vec<Option<String>> {
        match self {
            Self::Subprocess(p) => p.poll_notifications(),
            Self::InProcess { ext, .. } => ext
                .as_mut()
                .map(|p| p.poll_notifications())
                .unwrap_or_default(),
        }
    }

    /// 取走并清空积压的 `host/*` 请求。
    pub fn drain_host_requests(&mut self) -> Vec<RawMessage> {
        match self {
            Self::Subprocess(p) => p.drain_host_requests(),
            Self::InProcess { ext, .. } => ext
                .as_mut()
                .map(|p| p.drain_host_requests())
                .unwrap_or_default(),
        }
    }

    /// R-03：观察入站帧队列溢出并复位（episode 语义见
    /// [`ExtensionProcess::take_inbound_overflow`]）。in-process 无读线程队列
    /// （`serve_line` 同步产出、即产即路由），恒 `None`。
    pub fn take_inbound_overflow(&mut self) -> Option<u64> {
        match self {
            Self::Subprocess(p) => p.take_inbound_overflow(),
            Self::InProcess { .. } => None,
        }
    }

    /// 后端是否已退出。in-process 恒 `false`（无独立进程；滞留态不走此处，
    /// 由 [`Self::is_in_process_hung`] 先行判别）。
    pub fn has_exited(&mut self) -> bool {
        match self {
            Self::Subprocess(p) => p.has_exited(),
            Self::InProcess { .. } => false,
        }
    }

    /// 退出状态（`None` = 仍在运行；in-process 恒 `None`）。
    pub fn exit_status(&mut self) -> Option<ExitStatus> {
        match self {
            Self::Subprocess(p) => p.exit_status(),
            Self::InProcess { .. } => None,
        }
    }

    /// 失败诊断摘要（退出码 + stderr 末行；in-process 恒 `None`）。
    pub fn failure_detail(&mut self) -> Option<String> {
        match self {
            Self::Subprocess(p) => p.failure_detail(),
            Self::InProcess { .. } => None,
        }
    }

    /// §6.6 优雅关闭。in-process 为 noop（无进程可退；滞留态无可关闭对象，
    /// 扩展随工作线程自然结束）。
    pub fn close(self) -> Result<(), CloseError> {
        match self {
            Self::Subprocess(p) => p.close(),
            Self::InProcess { ext, .. } => match ext {
                Some(p) => p.close(),
                None => Ok(()),
            },
        }
    }

    /// R-04：in-process 调用的线程 + `recv_timeout` 包装（本枚举内的单一事实
    /// 来源）。`ext` 在调用期间移入工作线程：按时归还 → 放回槽位；超时 →
    /// 槽位保持 `None`（扩展对象随线程自然滞留），返回协议层 `Timeout`。
    fn inproc_call<T, F>(
        slot: &mut Option<InProcessExtension>,
        timeout: Duration,
        method: &'static str,
        op: F,
    ) -> Result<T, ProtocolError>
    where
        T: Send + 'static,
        F: FnOnce(&mut InProcessExtension) -> Result<T, ProtocolError> + Send + 'static,
    {
        let ext = slot.take().ok_or_else(hung_error)?;
        let (returned, result) = run_with_timeout(ext, method, timeout, op);
        *slot = returned;
        result
    }
}

/// R-04：滞留态调用的错误——上次调用超时后扩展对象未归还，本会话不可用
///（与熔断/复热的「暂时不可用」提示同族，UI 层已有 Failed + Retry 呈现）。
fn hung_error() -> ProtocolError {
    ProtocolError::Rpc(RpcError {
        code: error_codes::PROVIDER_UNAVAILABLE,
        message: "in-process 扩展滞留（上次调用超时，待其自然结束）".to_string(),
        data: None,
    })
}

/// R-04：把 in-process 扩展移入工作线程执行 `op`，`recv_timeout` 等待结果。
///
/// 按时归还 → `(Some(ext), result)`；超时 → `(None, Err(Timeout))`（**不阻塞
/// 也不 kill**：滞留线程受 OS 网络超时上界约束自然结束——apps 枚举的死 UNC
/// `is_file()` / `GetImage` 最终会返回，扩展对象随之释放）；线程消失（理论
/// 不可达——扩展 panic 已被 M9 `catch_unwind` 挡住）按滞留口径报内部错误。
fn run_with_timeout<T, F>(
    ext: InProcessExtension,
    method: &'static str,
    timeout: Duration,
    op: F,
) -> (Option<InProcessExtension>, Result<T, ProtocolError>)
where
    T: Send + 'static,
    F: FnOnce(&mut InProcessExtension) -> Result<T, ProtocolError> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut ext = ext;
        let result = op(&mut ext);
        let _ = tx.send((ext, result));
    });
    match rx.recv_timeout(timeout) {
        Ok((ext, result)) => (Some(ext), result),
        Err(RecvTimeoutError::Timeout) => (
            None,
            Err(ProtocolError::Timeout {
                method: method.to_string(),
                timeout,
            }),
        ),
        Err(RecvTimeoutError::Disconnected) => (
            None,
            Err(ProtocolError::Rpc(RpcError {
                code: error_codes::INTERNAL_ERROR,
                message: "in-process 调用线程消失（panic 应已被 catch_unwind 挡住）".to_string(),
                data: None,
            })),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_protocol::model::{CommandRef, Sender};

    /// 最小内置规格夹具（供 in-process 分支验证）。
    fn fixture_spec() -> ExtensionSpec {
        ExtensionSpec {
            id: "com.ddrun.fixture",
            display_name: "Fixture",
            description: "单测夹具",
            frozen: true,
            has_fallback: false,
            capabilities: &[],
            log_tag: "dd-ext-fixture",
            top_level: || {
                vec![CommandItem {
                    id: "fix.hello".into(),
                    title: "Hello".into(),
                    subtitle: None,
                    icon: None,
                    section: None,
                    tags: None,
                    details: None,
                    text_to_suggest: None,
                    more_commands: None,
                    command: CommandRef::Invoke,
                }]
            },
            fallback: None,
            invoke: |_| {
                (
                    CommandResult::ShowToast {
                        message: "ok".into(),
                        duration_ms: None,
                    },
                    Vec::new(),
                )
            },
            pages: None,
        }
    }

    /// M9 B3：`spec` 命中 → in-process 客户端（**不 spawn 子进程**）；协议调用正常，
    /// 且崩溃巡检相关方法对 in-process 恒为"未退出/无诊断"。
    #[test]
    fn open_with_spec_yields_in_process_client() {
        let (mut client, init) =
            open(Some(fixture_spec()), None).expect("内置 spec 应能打开 in-process 客户端");
        assert!(client.is_in_process(), "spec 命中 → 必须走 in-process 后端");
        assert_eq!(init.provider.id, "com.ddrun.fixture");
        assert_eq!(init.protocol_version, "1.0");

        // 协议调用可用（顶层命令）
        let cmds = client.top_level_commands().expect("top_level 成功");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].id, "fix.hello");

        // invoke 亦可用
        let result = client
            .invoke(&InvokeParams {
                id: "fix.hello".into(),
                sender: Sender::TopLevel,
                context: None,
            })
            .expect("invoke 成功");
        assert_eq!(
            result,
            CommandResult::ShowToast {
                message: "ok".into(),
                duration_ms: None,
            }
        );

        // 无子进程：崩溃巡检相关方法恒为"未退出/无状态/无诊断"
        assert!(!client.has_exited(), "in-process 恒未退出");
        assert!(client.exit_status().is_none());
        assert!(client.failure_detail().is_none());
        assert!(client.poll_notifications().is_empty());
        assert!(client.drain_host_requests().is_empty());
        client.close().expect("in-process close 为 noop");
    }

    /// 既无 spec 也无 exe → 明确报错（不 panic）。
    #[test]
    fn open_without_any_source_errors() {
        // 不用 `expect_err`：`ExtClient` 无可 `Debug`（其内含 `ExtensionProcess`），
        // 用 match 取出 Err 侧即可。
        let err = match open(None, None) {
            Ok(_) => panic!("无注册信息应报错"),
            Err(e) => e,
        };
        assert!(err.contains("无注册信息"), "got {err}");
    }
}
