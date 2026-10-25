//! doc 契约测试（B2② 第二批，dd-run-改进分析.md §3.3）：把契约文档中
//! **GUI 宿主侧**的「实现现状」注从散文变成断言——测试名即锚点。
//!
//! 覆盖清单（与 24 处现状注对照；协议/宿主/扩展侧见 dd-protocol / dd-host /
//! dd-ext 的 `doc_contract` 文件）：
//! - protocol.md §7.2 现状注（`state` 解析后渲染侧忽略、一律 Info 样式；
//!   `duration_ms = 0` 未实现「常驻」）→
//!   [`show_status_state_parsed_but_rendered_as_info`] /
//!   [`toast_duration_zero_expires_next_frame_not_persistent`]；
//! - protocol.md §7.1 现状注（无脏标记、不可见页通知直接丢弃；命中也进
//!   100ms 合并非「立即」；`poll_notifications` 消费式取走）→
//!   [`notifications_for_invisible_pages_are_dropped`] /
//!   [`top_notification_enters_merge_window_not_immediate`] /
//!   [`poll_notifications_is_consuming_take`]；
//! - protocol.md §11 现状注（巡检仅非 0 退出码计崩溃；EOF 单独不计）→
//!   [`exit_zero_drops_to_stub_without_crash_count`]；
//! - extensions.md §5 现状注（spawn 失败只置 Failed、不进崩溃计数）→
//!   aggregator.rs 测试 `spawn_failure_yields_failed_outcome`（就近私有
//!   `load_one`，不入本文件）。
//!
//! `no-assert` 台账（dd-gui 侧）：
//! - protocol.md §4「宿主无状态枚举」：负向结构性断言无运行时载体；其行为
//!   半段（运行期崩溃回 stub）由 `app/health.rs::
//!   refresh_health_drops_to_stub_and_records_crash` 断言（退出码 1 → 崩溃
//!   计数 + 回落 stub），本文件的 exit-0 锚点补「EOF 不计」半段；
//! - protocol.md §8.4「`"list_item"` 无生产产出点」：负向结构性断言（全仓
//!   无产出方）无运行时载体，code review + 演进时同步；
//! - protocol.md §7.2「四种取值一律按 Info **样式**展示」的像素级呈现：
//!   行为锚点（kind == Info）已断言，像素回归由 B1 金图体系承接（后续金图
//!   扩面时覆盖 Toast）。

use crate::app::toast::{toast_duration_ms, ToastKind};
use crate::ext_client::ExtClient;
use crate::ext_inprocess::InProcessExtension;
use crate::test_support::{exiting_process, make_app};
use dd_ext::ExtensionSpec;
use dd_protocol::messages::RawMessage;
use dd_protocol::methods::{METHOD_HOST_SHOW_STATUS, NOTIFY_ITEMS_CHANGED};
use dd_protocol::model::{CommandItem, CommandRef, CommandResult};
use std::time::Duration;

/// 最小 in-process 扩展 spec（无子页/无兜底；通知由测试直接注入队列）。
fn minimal_spec() -> ExtensionSpec {
    ExtensionSpec {
        id: "com.ddrun.doccontract",
        display_name: "DocContract",
        description: "doc 契约夹具",
        frozen: true,
        has_fallback: false,
        capabilities: &[],
        log_tag: "dd-ext-doccontract",
        top_level: || {
            vec![CommandItem {
                id: "doc.item".into(),
                title: "Item".into(),
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
        invoke: |_: &dd_protocol::messages::InvokeParams| (CommandResult::Dismiss, vec![]),
        pages: None,
    }
}

/// 构造一条 `items_changed` 通知（`page_id = None` 表示顶层变更）。
fn items_changed(page_id: Option<&str>) -> RawMessage {
    let params = serde_json::json!({ "page_id": page_id });
    RawMessage {
        jsonrpc: "2.0".to_string(),
        id: None,
        method: Some(NOTIFY_ITEMS_CHANGED.to_string()),
        params: Some(params),
        result: None,
        error: None,
    }
}

/// 带一条积压通知的 in-process 客户端。
fn inproc_client_with(notification: RawMessage) -> ExtClient {
    let mut ext = InProcessExtension::new(minimal_spec());
    ext.notifications.push(notification);
    ExtClient::InProcess {
        ext: Some(ext),
        timeout: Duration::from_millis(50),
    }
}

/// 契约锚：docs/protocol.md §7.2 现状注（核对至 2026-09-13）前半。
///
/// 现状注原文：宿主解析 `state` 后**渲染侧忽略**，四种取值一律按 Info 样式
/// 展示。
///
/// 断言：`host/show_status` 带 `state: "success"`（甚至 `"error"`）→ Toast
/// kind 仍为 `Info`（`ShowToast` 扩展路径默认 info）。有人把 `state` 接进
/// ToastKind 映射（协议演进）时此测试红，提示同步 protocol.md §7.2 注。
#[test]
fn show_status_state_parsed_but_rendered_as_info() {
    for state in ["info", "success", "error", "warning"] {
        let mut app = make_app();
        let msg = RawMessage {
            jsonrpc: "2.0".to_string(),
            id: Some(9),
            method: Some(METHOD_HOST_SHOW_STATUS.to_string()),
            params: Some(serde_json::json!({
                "message": "hello",
                "state": state,
                "duration_ms": 1_000,
            })),
            result: None,
            error: None,
        };
        app.execute_host_request("com.ddrun.doccontract", &msg);
        let toast = app
            .overlay
            .toast
            .as_ref()
            .unwrap_or_else(|| panic!("state={state} 应弹出 Toast"));
        assert_eq!(
            toast.kind,
            ToastKind::Info,
            "state={state} 渲染侧必须忽略（一律 Info）"
        );
        assert_eq!(toast.message, "hello");
    }
}

/// 契约锚：docs/protocol.md §7.2 现状注后半。
///
/// 现状注原文：`duration_ms = 0` 未实现「常驻」——原样写入过期时间导致
/// **下一帧即清空**。
///
/// 断言：`duration_ms(Info, Some(0)) == 0`（不回退默认值、不转常驻）——
/// 过期时间 = `now + 0`，`app/mod.rs` 的 `now >= expires` 清除条件在下一帧
/// 成立。有人实现「0 = 常驻」时此测试红，提示同步 protocol.md §7.2 注。
#[test]
fn toast_duration_zero_expires_next_frame_not_persistent() {
    assert_eq!(toast_duration_ms(ToastKind::Info, Some(0)), 0);
    assert_eq!(toast_duration_ms(ToastKind::Success, Some(0)), 0);
    // 对照：未指定走默认 2000ms（TOAST_DEFAULT_MS），非 0
    assert_eq!(toast_duration_ms(ToastKind::Info, None), 2_000);
}

/// 契约锚：docs/protocol.md §7.1 现状注（核对至 2026-09-14）前半。
///
/// 现状注原文：宿主**无脏标记机制**——不可见页的通知被直接丢弃。
///
/// 断言：Root 页（当前无嵌套页）收到其他页的 `items_changed` → 不产生任何
/// 刷新调度（`refresh` 保持 `None`）。有人加脏标记（页回前台补拉）时此测试
/// 红，提示同步 protocol.md §7.1 注。
#[test]
fn notifications_for_invisible_pages_are_dropped() {
    let mut app = make_app();
    app.pool.processes.push((
        "com.ddrun.doccontract".to_string(),
        inproc_client_with(items_changed(Some("calc.history"))),
    ));
    app.poll_notifications();
    assert!(
        app.view.refresh.is_none(),
        "不可见页通知必须直接丢弃（无脏标记）"
    );
}

/// 契约锚：docs/protocol.md §7.1 现状注后半。
///
/// 现状注原文：命中当前页的通知也进入 100ms 合并窗口，并非「立即」拉取。
///
/// 断言：顶层 `items_changed`（`page_id` 缺省）→ 刷新进入合并窗口
/// （`ready_at` ≥ now + 100ms、`top = true`），而非立即重聚合标志。
#[test]
fn top_notification_enters_merge_window_not_immediate() {
    let mut app = make_app();
    app.pool.processes.push((
        "com.ddrun.doccontract".to_string(),
        inproc_client_with(items_changed(None)),
    ));
    let before = std::time::Instant::now();
    app.poll_notifications();
    let r = app
        .view
        .refresh
        .as_ref()
        .expect("顶层 items_changed 必须进入刷新调度");
    assert!(r.top, "无 page_id 的通知是顶层刷新（A9 Root 重聚合）");
    assert!(
        r.ready_at >= before + crate::app::refresh::REFRESH_WINDOW,
        "必须先进 100ms 合并窗口，而非立即拉取"
    );
}

/// 契约锚：docs/protocol.md §7.1 现状注（2026-09-14 更正段）。
///
/// 现状注原文：in-process 路径的通知队列此前只增不清、同一 `page_id` 会被
/// 反复上报；现 `poll_notifications` 已改为**消费式取走**。
///
/// 断言：同一客户端连 poll 两次，第二次必须为空（事件只报告一次）。有人把
/// 通知队列改回账本式（只增不清）时此测试红。
#[test]
fn poll_notifications_is_consuming_take() {
    let mut client = inproc_client_with(items_changed(None));
    // 直接驱动客户端层（app 层消费在 refresh 调度断言中已覆盖）
    let first = client.poll_notifications();
    assert_eq!(first.len(), 1, "首次 poll 取走积压通知");
    assert!(
        client.poll_notifications().is_empty(),
        "第二次 poll 必须为空（消费式取走，非账本）"
    );
}

/// 契约锚：docs/protocol.md §11 现状注（核对至 2026-09-13）的 EOF 支。
///
/// 现状注原文：周期巡检仅以**非 0 退出码**计崩溃；「EOF 单独计崩溃」仅在
/// 巡检路径不成立。
///
/// 断言：进程以退出码 **0** 结束（EOF、无崩溃信号）→ 巡检把它从保活集移除
/// （回落 stub），但**不**进崩溃计数。对照支（退出码 1 → 计崩溃）由
/// `app/health.rs::refresh_health_drops_to_stub_and_records_crash` 断言。
/// 有人把 EOF 也计进崩溃（§11 演进）时此测试红，提示同步 protocol.md §11 注。
#[test]
fn exit_zero_drops_to_stub_without_crash_count() {
    let mut app = make_app();
    let ext_id = "com.example.graceful";
    app.pool.sources.push(dd_gui::aggregator::SourceSummary {
        id: ext_id.to_string(),
        name: "Graceful".to_string(),
        status: dd_gui::aggregator::SourceStatus::Warm { commands: 3 },
    });
    app.pool.processes.push((
        ext_id.to_string(),
        ExtClient::Subprocess(exiting_process(ext_id, 0)),
    ));

    app.refresh_health();

    assert!(
        app.pool.processes.iter().all(|(id, _)| id != ext_id),
        "已退出进程应从保活集移除（回落 stub）"
    );
    let status = &app
        .pool
        .sources
        .iter()
        .find(|s| s.id == ext_id)
        .expect("源摘要保留")
        .status;
    assert!(
        matches!(
            status,
            dd_gui::aggregator::SourceStatus::Stub { commands: 3 }
        ),
        "退出（EOF）回落 stub、保留命令数，实际 {status:?}"
    );
    assert!(
        app.pool.crash_guards.is_empty(),
        "退出码 0（EOF）不计崩溃——崩溃计数只认非 0 退出码"
    );
    assert!(!app.is_crash_tripped(ext_id), "EOF 不得触发熔断");
}
