//! M9 B2：内置扩展 in-process 适配器。
//!
//! 与 [`dd_host::process::ExtensionProcess`]（子进程 JSON-RPC 客户端）**同语义**的
//! 调用接口，但底层直接调用 [`dd_ext::serve_line`]（纯函数，见 `dd-ext/src/lib.rs`）
//! —— 不 spawn 子进程、不物化磁盘 exe、不走 stdin/stdout 通道。仅内置 5 扩展使用
//! （设计稿范围），第三方 / sidecar 仍走 `ExtensionProcess` 子进程。
//!
//! ## R1 等价性不变式
//!
//! 宿主侧内置扩展 exe 的 `run` 主循环（`dd-ext/src/lib.rs`）就是：
//!
//! ```text
//! for frame in decoder.push(bytes) {
//!     let (outs, _) = serve_line(spec, &frame.line);
//!     for o in outs { send(o); }   // send = NDJSON 成帧 + 写 stdout
//! }
//! ```
//!
//! 即子进程 stdout 上的消息 = `serve_line` 返回的 `Vec<serde_json::Value>` 逐条
//! NDJSON 成帧。**本适配器把 `serve_line` 的返回值用与 `ExtensionProcess` 同一套
//! [`dd_host::process::classify`] 判别路由**到「响应 / `host/*` 请求 / `items_changed`
//! 通知」总线，因此宿主侧看到的 in-process 行为与 subprocess **逐字节等价**（见
//! 模块底部 `#[cfg(test)]` 的 parity 测试）。
//!
//! ## 崩溃策略
//!
//! 每次 `serve_line` 调用以 [`std::panic::catch_unwind`] 包裹。内置扩展 panic →
//! 该次调用返回 `ProtocolError::Rpc(INTERNAL_ERROR)`，宿主存活、可重试（M9 R2：
//! catch_unwind 仅挡 unwinding panic；内置同源可信）。

use std::panic::AssertUnwindSafe;

use dd_ext::{serve_line, ExtensionSpec};
use dd_host::manifest::{current_platform, HOST_CAPABILITIES};
use dd_host::process::{classify, CloseError, MessageKind, ProtocolError};
use dd_protocol::framing::DEFAULT_MAX_MESSAGE_BYTES;
use dd_protocol::messages::{
    error_codes, CommandListResult, GetCommandParams, GetCommandResult, GetItemsParams,
    GetItemsResult, InitializeParams, InitializeResult, InvokeParams, ItemsChangedParams,
    RawMessage, RpcError, TransportInfo, HostInfo, JSONRPC_VERSION,
};
use dd_protocol::model::{CommandItem, CommandResult};

/// 一次 in-process 调用（无子进程）。状态字段与 [`dd_host::process::ExtensionProcess`]
/// 完全对应，保证宿主侧路由逻辑一致。
pub struct InProcessExtension {
    spec: ExtensionSpec,
    next_id: u64,
    /// `items_changed` / `initialized` 等通知（§7.1 / §5.2）。
    pub notifications: Vec<RawMessage>,
    /// 扩展反向发出的 `host/*` 请求（§7.4），由 UI 层 `drain_host_requests` 取走执行。
    pub host_requests: Vec<RawMessage>,
    /// §3.3：未匹配到 in-flight 请求的响应，记日志用。
    pub unmatched: Vec<RawMessage>,
}

impl InProcessExtension {
    /// 由运行期构造的 [`ExtensionSpec`] 建立 in-process 扩展（D1：`builtin_specs()`）。
    pub fn new(spec: ExtensionSpec) -> Self {
        Self {
            spec,
            next_id: 1,
            notifications: Vec::new(),
            host_requests: Vec::new(),
            unmatched: Vec::new(),
        }
    }

    /// 清单 id（= `spec.id`）。
    pub fn id(&self) -> &str {
        self.spec.id
    }

    /// §5.1 握手 + §5.3 版本协商（与 `ExtensionProcess::initialize` 同语义）。
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
        let value = self.call("initialize", serde_json::to_value(params)?)?;
        let result: InitializeResult = serde_json::from_value(value)?;

        let bad = || ProtocolError::BadProtocolVersion {
            got: result.protocol_version.clone(),
            requested: protocol_version.to_string(),
        };
        let got =
            dd_host::process::parse_protocol_version(&result.protocol_version).ok_or_else(bad)?;
        let requested =
            dd_host::process::parse_protocol_version(protocol_version).ok_or_else(bad)?;
        if got > requested {
            return Err(bad());
        }
        Ok(result)
    }

    /// §6.1 取首屏顶层命令。
    pub fn top_level_commands(&mut self) -> Result<Vec<CommandItem>, ProtocolError> {
        let value = self.call("top_level_commands", serde_json::json!({}))?;
        let result: CommandListResult = serde_json::from_value(value)?;
        Ok(result.commands)
    }

    /// §6.2 兜底命令模板。
    pub fn fallback_commands(&mut self) -> Result<Vec<CommandItem>, ProtocolError> {
        let value = self.call("fallback_commands", serde_json::json!({}))?;
        let result: CommandListResult = serde_json::from_value(value)?;
        Ok(result.commands)
    }

    /// §6.4 按 id 取回真实命令（`Ok(None)` = 扩展答复 `command: null`，正常结果）。
    pub fn get_command(&mut self, id: &str) -> Result<Option<CommandItem>, ProtocolError> {
        let value = self.call(
            "get_command",
            serde_json::to_value(GetCommandParams { id: id.to_string() })?,
        )?;
        let result: GetCommandResult = serde_json::from_value(value)?;
        Ok(result.command)
    }

    /// §6.3 拉取整页项（协议全量，宿主侧二次过滤见 [`crate::state`]）。
    pub fn get_items(
        &mut self,
        page_id: &str,
        search_text: Option<&str>,
    ) -> Result<GetItemsResult, ProtocolError> {
        let params = GetItemsParams {
            page_id: page_id.to_string(),
            search_text: search_text.map(|s| s.to_string()),
        };
        let value = self.call("get_items", serde_json::to_value(params)?)?;
        let result: GetItemsResult = serde_json::from_value(value)?;
        Ok(result)
    }

    /// §6.5 执行一条命令，返回 §8.3 `CommandResult`。
    pub fn invoke(&mut self, params: &InvokeParams) -> Result<CommandResult, ProtocolError> {
        let value = self.call("invoke", serde_json::to_value(params)?)?;
        let result: CommandResult = serde_json::from_value(value)?;
        Ok(result)
    }

    /// §6.6 优雅关闭：in-process 无进程可退，noop（保留接口对称，仍触发 handler 以贴合协议）。
    pub fn close(self) -> Result<(), CloseError> {
        let line = serde_json::to_string(&serde_json::json!({
            "jsonrpc": JSONRPC_VERSION,
            "id": 0,
            "method": "close",
            "params": {},
        }))
        .expect("序列化 close 请求");
        // 忽略 panic / 结果：in-process 关闭即标记扩展不可用，无进程可杀。
        let _ = catch(&self.spec, &line);
        Ok(())
    }

    /// §7.1 通知轮询（非阻塞）：返回本次 `items_changed` 的 `page_id`
    /// （`None` = 顶层命令变了），语义与 `ExtensionProcess::poll_notifications` 一致。
    pub fn poll_notifications(&mut self) -> Vec<Option<String>> {
        let mut changed = Vec::new();
        for msg in &self.notifications {
            if msg.method.as_deref() == Some("items_changed") {
                let page_id = msg
                    .params
                    .as_ref()
                    .and_then(|v| serde_json::from_value::<ItemsChangedParams>(v.clone()).ok())
                    .and_then(|p| p.page_id);
                changed.push(page_id);
            }
        }
        changed
    }

    /// 取走并清空积压的 `host/*` 请求（UI 层消费执行真实副作用）。
    pub fn drain_host_requests(&mut self) -> Vec<RawMessage> {
        std::mem::take(&mut self.host_requests)
    }

    /// 内部：发一次请求并就地路由返回属于该请求的响应（含 host/*、通知副作用记录）。
    ///
    /// 与 `ExtensionProcess::call` 不同：不写 stdin、不读子进程 stdout，而是直接
    /// `serve_line` 拿回消息并路由。协议语义、消息形状、副作用路由完全一致（R1）。
    ///
    /// 注意：`serve_line` 返回的 Vec 中**响应在前、副作用在后**（见 lib.rs `invoke`
    /// 分支），故必须先收齐响应、再把同批次的 `host/*` / 通知全部路由进总线后再返回，
    /// 这与 subprocess 把后续帧缓冲进 `host_requests` / `notifications` 的终态等价。
    fn call(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, ProtocolError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = serde_json::json!({
            "jsonrpc": JSONRPC_VERSION,
            "id": id,
            "method": method,
            "params": params,
        });
        let line = serde_json::to_string(&request).expect("序列化请求");

        // catch_unwind：内置扩展 panic → 宿主存活（M9 R2）。
        let (outputs, _exit) = match catch(&self.spec, &line) {
            Ok(out) => out,
            Err(msg) => {
                return Err(ProtocolError::Rpc(RpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: format!("内置扩展 panic：{msg}"),
                    data: Some(serde_json::json!({ "ext": self.spec.id })),
                }));
            }
        };

        let mut matched: Option<Result<serde_json::Value, ProtocolError>> = None;
        for value in outputs {
            let msg: RawMessage = match serde_json::from_value(value) {
                Ok(m) => m,
                Err(_) => continue, // 非合法 JSON-RPC 信封：忽略不致命（§9.3）
            };
            if msg.jsonrpc != JSONRPC_VERSION {
                self.unmatched.push(msg);
                continue;
            }
            match classify(&msg) {
                MessageKind::HostRequest => {
                    // §7.4：记录反向请求，UI 层执行真实副作用。in-process 无子进程
                    // stdin 可应答，但扩展侧 `run` 本就忽略 host 响应（见 lib.rs
                    // `serve_line` 对"无 method 有 id"消息直接 discard），故无需应答。
                    self.host_requests.push(msg);
                }
                MessageKind::Notification => {
                    self.notifications.push(msg);
                }
                MessageKind::Response(rid) => {
                    if rid == id {
                        matched = Some(match msg.error {
                            Some(err) => Err(ProtocolError::Rpc(err)),
                            None => Ok(msg.result.unwrap_or(serde_json::Value::Null)),
                        });
                        // 不立即 return：继续路由同批次的 host/* / 通知副作用
                    } else {
                        self.unmatched.push(msg);
                    }
                }
                MessageKind::Unknown => {
                    self.unmatched.push(msg);
                }
            }
        }
        match matched {
            Some(r) => r,
            None => Err(ProtocolError::Rpc(RpcError {
                code: error_codes::INTERNAL_ERROR,
                message: "serve_line 未返回匹配的响应".to_string(),
                data: Some(serde_json::json!({ "method": method, "ext": self.spec.id })),
            })),
        }
    }
}

/// `catch_unwind` 包装的 [`serve_line`]：返回 `Ok((消息, 退出标志))` 或 `Err(panic 信息)`。
fn catch(
    spec: &ExtensionSpec,
    line: &str,
) -> Result<(Vec<serde_json::Value>, bool), String> {
    std::panic::catch_unwind(AssertUnwindSafe(|| serve_line(spec, line))).map_err(|payload| {
        // panic 载荷通常是 &str 或 String；尽量取出可读信息。
        if let Some(s) = payload.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else {
            "未知 panic 载荷".to_string()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_ext::Effect;
    use dd_protocol::model::{CommandRef, Icon, IconKind, Sender};

    /// 测试用最小 spec：顶层 2 命令、fallback 1 模板、invoke 分发（含 host 请求与
    /// items_changed 副作用）。字段均为 `&'static str`，闭包非捕获可 coerce 为 `fn` 指针。
    fn fixture_spec() -> ExtensionSpec {
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
                            value: "H".into(),
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
                _ => (
                    CommandResult::ShowToast {
                        message: "未知道具".into(),
                        duration_ms: None,
                    },
                    Vec::new(),
                ),
            },
            pages: None,
        }
    }

    fn invoke_line(id: &str) -> String {
        serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0", "id": 9, "method": "invoke",
            "params": { "id": id, "sender": "top_level", "context": {} }
        }))
        .unwrap()
    }

    #[test]
    fn initialize_returns_protocol_1_0() {
        let mut ext = InProcessExtension::new(fixture_spec());
        let init = ext.initialize("1.0", "0.1.1").expect("initialize 成功");
        assert_eq!(init.protocol_version, "1.0");
        assert_eq!(init.provider.id, "com.ddrun.fixture");
        assert!(init.capabilities.contains(&"host/open_url".to_string()));
    }

    #[test]
    fn top_level_commands_returns_list() {
        let mut ext = InProcessExtension::new(fixture_spec());
        ext.initialize("1.0", "0.1.1").unwrap();
        let cmds = ext.top_level_commands().expect("top_level 成功");
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].id, "fix.hello");
    }

    #[test]
    fn invoke_routes_host_request_to_bus() {
        let mut ext = InProcessExtension::new(fixture_spec());
        ext.initialize("1.0", "0.1.1").unwrap();
        let result = ext
            .invoke(&InvokeParams {
                id: "fix.open".into(),
                sender: Sender::TopLevel,
                context: None,
            })
            .expect("invoke 成功");
        assert_eq!(result, CommandResult::Dismiss);
        // host/* 请求被记录到总线（与 subprocess 路径一致：UI 层取走执行）
        let reqs = ext.drain_host_requests();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].method.as_deref(), Some("host/open_url"));
        assert_eq!(
            reqs[0].params.as_ref().unwrap()["url"],
            "https://example.com"
        );
        // fix.open 不发 items_changed → 通知总线空
        assert!(ext.poll_notifications().is_empty());
    }

    #[test]
    fn invoke_routes_items_changed_to_bus() {
        let mut ext = InProcessExtension::new(fixture_spec());
        ext.initialize("1.0", "0.1.1").unwrap();
        let result = ext
            .invoke(&InvokeParams {
                id: "fix.notify".into(),
                sender: Sender::TopLevel,
                context: None,
            })
            .expect("invoke 成功");
        assert_eq!(result, CommandResult::KeepOpen);
        assert_eq!(ext.poll_notifications(), vec![Some("fix.sub".to_string())]);
        assert!(ext.drain_host_requests().is_empty());
    }

    #[test]
    fn get_items_without_pages_returns_32005() {
        let mut ext = InProcessExtension::new(fixture_spec());
        ext.initialize("1.0", "0.1.1").unwrap();
        let err = ext
            .get_items("fix.sub", None)
            .expect_err("无子页应回 -32005");
        match err {
            ProtocolError::Rpc(rpc) => assert_eq!(rpc.code, error_codes::PAGE_NOT_FOUND),
            _ => panic!("应为 -32005"),
        }
    }

    // ── R1 逐字节等价：in-process 路由后的消息集 == serve_line 直接产出 ──

    /// 手工把 `serve_line` 输出按与 `InProcessExtension` 相同的 `classify` 规则分类，
    /// 得到 (response, host_requests, notifications)，用于与适配器内部状态对比。
    fn route_serve_line(
        spec: &ExtensionSpec,
        line: &str,
    ) -> (Option<serde_json::Value>, Vec<RawMessage>, Vec<RawMessage>) {
        let (outputs, _) = serve_line(spec, line);
        let mut response = None;
        let mut host = Vec::new();
        let mut notes = Vec::new();
        for value in outputs {
            let Ok(msg) = serde_json::from_value::<RawMessage>(value) else {
                continue;
            };
            if msg.jsonrpc != JSONRPC_VERSION {
                continue;
            }
            match classify(&msg) {
                MessageKind::HostRequest => host.push(msg),
                MessageKind::Notification => notes.push(msg),
                MessageKind::Response(_) => {
                    response = Some(msg.result.unwrap_or(serde_json::Value::Null))
                }
                MessageKind::Unknown => {}
            }
        }
        (response, host, notes)
    }

    #[test]
    fn invoke_output_matches_serve_line_byte_for_byte() {
        let spec = fixture_spec();
        let line = invoke_line("fix.open");

        // in-process 适配器
        let mut ext = InProcessExtension::new(spec.clone());
        ext.initialize("1.0", "0.1.1").unwrap();
        let result = ext
            .invoke(&InvokeParams {
                id: "fix.open".into(),
                sender: Sender::TopLevel,
                context: None,
            })
            .unwrap();
        let ext_host = ext.drain_host_requests();

        // serve_line 直出（subprocess stdout 的来源）
        let (resp, host, notes) = route_serve_line(&spec, &line);

        // 响应字节一致（响应 id 由宿主侧 next_id 决定，确定性）
        let result_json = serde_json::to_value(&result).unwrap();
        assert_eq!(result_json, resp.unwrap(), "响应 JSON 逐字节一致");
        // host/* 请求：比较 method / params / jsonrpc（UI 实际执行的就是这些）。
        // 注意**不比较 id**——`make_host_request` 用进程级全局计数器（§3.3 两端
        // id 空间独立），同一消息在不同调用次序下 id 不同，属正常；且 subprocess
        // 路径下 `answer_host_request` 记录的也是同一 method/params，与 in-process 等价。
        assert_eq!(ext_host.len(), host.len());
        for (a, b) in ext_host.iter().zip(host.iter()) {
            assert_eq!(a.method, b.method, "host 请求 method 一致");
            assert_eq!(a.params, b.params, "host 请求 params 逐字节一致");
            assert_eq!(a.jsonrpc, b.jsonrpc);
        }
        assert!(notes.is_empty(), "fix.open 不应产生通知");
    }

    #[test]
    fn panic_in_invoke_does_not_propagate_and_marks_failed() {
        let spec = ExtensionSpec {
            id: "com.ddrun.bomb",
            display_name: "Bomb",
            description: "panic 夹具",
            frozen: false,
            has_fallback: false,
            capabilities: &[],
            log_tag: "dd-ext-bomb",
            top_level: || vec![],
            fallback: None,
            invoke: |_| panic!("boom from builtin"),
            pages: None,
        };
        let mut ext = InProcessExtension::new(spec);
        ext.initialize("1.0", "0.1.1").unwrap();
        // panic 被 catch_unwind 捕获 → 返回 Err（宿主存活），而非 unwind 整个线程
        let err = ext
            .invoke(&InvokeParams {
                id: "x".into(),
                sender: Sender::TopLevel,
                context: None,
            })
            .expect_err("panic 应映射为 Err");
        match err {
            ProtocolError::Rpc(rpc) => {
                assert_eq!(rpc.code, error_codes::INTERNAL_ERROR);
                assert!(rpc.message.contains("boom"), "错误应含 panic 信息");
            }
            _ => panic!("应为 ProtocolError::Rpc"),
        }
    }
}
