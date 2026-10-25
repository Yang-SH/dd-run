//! doc 契约测试（B2② 第二批，dd-run-改进分析.md §3.3）：把契约文档中
//! **扩展共享运行时**侧的「实现现状」注从散文变成断言——测试名即锚点。
//!
//! 覆盖清单：
//! - protocol.md §5.3 现状注（`initialize` 不读 `params.protocol_version`，
//!   恒回 `"1.0"`；`-32004` 无产出点）→
//!   [`initialize_ignores_offered_version_and_never_emits_32004`]；
//! - protocol.md §6.3 现状注（`page_id` 缺失/未注册统一回 `-32005`，非
//!   `-32602`）→ [`get_items_unregistered_page_returns_page_not_found`]；
//! - extensions.md §12 现状注（`-32002 command_not_found` 共享运行时无抛出点）
//!   → [`shared_runtime_has_no_32002_emission_point`]。

use dd_ext::{serve_line, ExtensionSpec};
use dd_protocol::messages::error_codes;
use dd_protocol::model::{CommandItem, CommandRef};

/// 最小合法 spec（无子页 → get_items 走 -32005 分支；与 in-crate 夹具同形状）。
fn spec() -> ExtensionSpec {
    ExtensionSpec {
        id: "com.ddrun.contract",
        display_name: "Contract",
        description: "doc 契约夹具",
        frozen: true,
        has_fallback: false,
        capabilities: &[],
        log_tag: "dd-ext-contract",
        top_level: || {
            vec![CommandItem {
                id: "doc.hello".into(),
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
        invoke: |_: &dd_protocol::messages::InvokeParams| {
            (dd_protocol::model::CommandResult::Dismiss, vec![])
        },
        pages: None,
    }
}

fn request_line(method: &str, params: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":1,"method":"{method}","params":{params}}}"#)
}

/// 某组输出里是否存在指定错误码。
fn has_error_code(outputs: &[serde_json::Value], code: i32) -> bool {
    outputs
        .iter()
        .any(|o| o["error"]["code"].as_i64() == Some(code as i64))
}

/// 契约锚：docs/protocol.md §5.3 现状注（核对至 2026-09-13）。
///
/// 现状注原文：扩展侧共享运行时的 `initialize` 不读取
/// `params.protocol_version`，恒回 `"1.0"`；`-32004 version_mismatch` 在
/// `dd-ext` 共享运行时无任何产出点。
///
/// 断言：即便宿主握手送上低版本（"0.9"）或高版本（"9.9"），
/// `initialize` 仍回 `protocol_version = "1.0"` 且不产出 `-32004`。
/// 有人把版本协商接进共享运行时（§5.3 规则 3）时此测试红，提示同步文档。
#[test]
fn initialize_ignores_offered_version_and_never_emits_32004() {
    for offered in ["0.9", "1.0", "9.9"] {
        let line = request_line(
            "initialize",
            &format!(r#"{{"protocol_version":"{offered}","host_version":"0.0.0"}}"#),
        );
        let (out, _) = serve_line(&spec(), &line);
        assert_eq!(
            out[0]["result"]["protocol_version"], "1.0",
            "共享运行时恒回 1.0（offered={offered}）"
        );
        assert!(
            !has_error_code(&out, error_codes::VERSION_MISMATCH),
            "共享运行时不得产出 -32004（offered={offered}）：{out:?}"
        );
    }
}

/// 契约锚：docs/protocol.md §6.3 现状注（核对至 2026-09-13）。
///
/// 现状注原文：`page_id` 缺失或未注册时，扩展共享运行时统一回
/// `-32005 page_not_found`，不回 `-32602`。
///
/// 断言：未注册页与参数缺失两条路径都回 `-32005`（非 `-32602` / 非 panic）。
/// （in-crate `get_items_unknown_page_returns_32005` 覆盖未注册页一支；
/// 此处按文档锚点口径补齐「缺失」一支并负向断言 `-32602`。）
#[test]
fn get_items_unregistered_page_returns_page_not_found() {
    for line in [
        request_line("get_items", r#"{"page_id":"no.such.page"}"#),
        request_line("get_items", r#"{}"#),
    ] {
        let (out, _) = serve_line(&spec(), &line);
        assert_eq!(
            out[0]["error"]["code"],
            error_codes::PAGE_NOT_FOUND,
            "统一回 -32005：{line}"
        );
        assert!(
            !has_error_code(&out, error_codes::INVALID_PARAMS),
            "不得回 -32602：{line}"
        );
    }
}

/// 契约锚：docs/extensions.md §12 现状注（核对至 2026-09-13）。
///
/// 现状注原文：`-32002 command_not_found` 虽在 `dd-protocol` 定义了常量，
/// 但宿主与 `dd-ext` 共享运行时均无抛出点；「命令不存在」由各扩展自身的
/// handler 兜底。
///
/// 断言：对共享运行时全方法面（已知方法 + 未知方法 + 畸形参数）打一轮，
/// 任何输出都不得携带 `-32002`——运行时层面确实没有抛出点。有人给运行时
/// 加 `-32002` 产出（协议演进）时此测试红，提示同步 extensions.md。
#[test]
fn shared_runtime_has_no_32002_emission_point() {
    let battery = [
        request_line("initialize", r#"{}"#),
        request_line("top_level_commands", r#"{}"#),
        request_line("fallback_commands", r#"{}"#),
        request_line("get_command", r#"{"id":"doc.hello"}"#),
        request_line("get_command", r#"{"id":"no.such.command"}"#),
        request_line("invoke", r#"{"id":"anything","sender":"top_level"}"#),
        request_line("get_items", r#"{"page_id":"x"}"#),
        // 未知方法 → -32601（不是 -32002）
        request_line("no_such_method", r#"{}"#),
        // 畸形信封 → -32700/-32600（不是 -32002）
        "not json".to_string(),
        r#"[{"jsonrpc":"2.0","id":1,"method":"initialize"}]"#.to_string(),
    ];
    for line in &battery {
        let (out, _) = serve_line(&spec(), line);
        assert!(
            !has_error_code(&out, error_codes::COMMAND_NOT_FOUND),
            "共享运行时不得产出 -32002：{line} → {out:?}"
        );
    }
}
