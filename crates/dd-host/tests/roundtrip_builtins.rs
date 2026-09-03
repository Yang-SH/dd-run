//! M4 P4 内置扩展（`dd-ext-*`）的全链路往返测试（支撑 m4-record.md P4 / A10）。
//!
//! 与 `roundtrip.rs` 同构：不持有协议消息副本，全部由 `dd-host` 与 5 个内置
//! 扩展在运行时按 [`docs/protocol.md`](../../docs/protocol.md) 产生。
//!
//! 覆盖点（P4 决策「扩展侧先行」+ Task #4）：
//! 1. **握手**：`initialize` 返回的 `ProviderInfo`（id / display_name / frozen）
//!    与 [`dd_host::builtin::BUILTINS`] 注册表一致（防注册表与扩展自述漂移）；
//! 2. **顶层命令**：`top_level_commands` 每扩展可取回（非空）；
//! 3. **兜底契约**（§6.2）：calc / websearch / shell 的 `fallback_commands` 非空
//!    且模板含 `{query}`；apps / system 无兜底 → 空列表；
//! 4. **invoke 关键链路**：
//!    - calc：表达式求值 → `ShowToast("= 2")` + `host/set_clipboard` 被记录；
//!    - websearch：fallback query → `Dismiss` + `host/open_url` URL 正确（编码）；
//!    - system：危险命令首发 → `Confirm{is_critical:true}`（**不重发**，重发会真关机）；
//!    - apps / shell：**不 invoke**（会真启动应用 / 真执行命令，属 A10 真机核对项）。
//!
//! 运行前置：正常 `cargo test --workspace` 会先构建 5 个 `dd-ext-*.exe`。
//! 若产物缺失（干净 target / 单独 `cargo test -p dd-host`），测试**跳过**而非失败。

use std::path::{Path, PathBuf};

use dd_host::builtin::{ensure_builtins, BUILTINS};
use dd_host::manifest::{from_builtin, LoadedExtension};
use dd_host::process::ExtensionProcess;
use dd_protocol::messages::{InvokeContext, InvokeParams};
use dd_protocol::model::{CommandRef, CommandResult, Sender};

/// 定位内置扩展可执行文件所在目录（与 `roundtrip.rs::sample_exe` 同款逻辑：
/// 集成测试二进制在 `target/debug/deps/`，产物在上一级 `target/debug/`）。
fn builtin_exe_dir() -> PathBuf {
    let mut dir = std::env::current_exe().expect("当前测试二进制路径");
    dir.pop();
    if dir.file_name().and_then(|s| s.to_str()) == Some("deps") {
        dir.pop();
    }
    dir
}

/// 5 个内置扩展是否**全部**已构建（产物存在）。未全部构建时测试应**跳过**
/// 而非 assert 失败——避免干净 target / 单独 `cargo test -p dd-host` 的"神秘失败"。
/// 正常 `cargo test --workspace` 会先构建 dd-ext，故该前置总是满足。
fn all_builtins_present(exe_dir: &Path) -> bool {
    ensure_builtins(exe_dir).len() == BUILTINS.len()
}

/// 按注册表构造某个内置扩展的 [`LoadedExtension`]（指向真实 exe）。
fn load_builtin(exe_dir: &Path, spec: &dd_host::builtin::BuiltinSpec) -> LoadedExtension {
    let exe = if cfg!(windows) {
        format!("{}.exe", spec.exe)
    } else {
        spec.exe.to_string()
    };
    let command = exe_dir.join(exe);
    assert!(
        command.is_file(),
        "找不到内置扩展：{}（请先执行 `cargo build`）",
        command.display()
    );
    from_builtin(
        command,
        spec.id,
        spec.name,
        spec.host_frozen(), // 与 ensure_builtins 同口径（宿主缓存策略）
        spec.capabilities,
        env!("CARGO_PKG_VERSION"),
    )
}

/// helper：spawn + 握手一个内置扩展。
fn spawn_builtin(exe_dir: &Path, spec: &dd_host::builtin::BuiltinSpec) -> ExtensionProcess {
    let ext = load_builtin(exe_dir, spec);
    let mut process = ExtensionProcess::spawn(&ext).expect("spawn 内置扩展");
    process.initialize("1.0", "0.1.0").expect("握手成功");
    process
}

/// ① 注册表与握手一致性：5 个内置扩展全部能 spawn + 握手，且 `ProviderInfo`
/// 与 [`BUILTINS`] **扩展自述侧**逐字段一致（id / display_name / frozen /
/// has_fallback / capabilities——宿主落桩策略 `host_frozen` 是宿主侧编排，
/// 见 dd-host builtin.rs 单测，不在此断言）。
#[test]
fn builtin_initialize_matches_registry() {
    let exe_dir = builtin_exe_dir();
    if !all_builtins_present(&exe_dir) {
        eprintln!(
            "SKIP: 内置扩展未全部构建，先 `cargo build`（cargo test --workspace 会自动构建）"
        );
        return;
    }

    for spec in BUILTINS {
        // 每次 spawn + 握手并读回 ProviderInfo
        let ext = load_builtin(&exe_dir, spec);
        let mut p2 = ExtensionProcess::spawn(&ext).expect("spawn");
        let init = p2.initialize("1.0", "0.1.0").expect("握手");
        assert_eq!(init.provider.id, spec.id, "provider.id 与注册表一致");
        assert_eq!(init.provider.display_name, spec.name);
        // 扩展自述 frozen / has_fallback（与 dd-ext 各 bin spec() 一致）
        assert_eq!(
            init.provider.frozen, spec.frozen,
            "provider.frozen 为扩展自述（与 spec() 对齐）"
        );
        assert_eq!(
            init.provider.has_fallback, spec.has_fallback,
            "provider.has_fallback 与注册表一致"
        );
        assert_eq!(
            init.capabilities,
            spec.capabilities.to_vec(),
            "capabilities 与注册表一致（能力前置白名单）"
        );
        p2.close().expect("优雅关闭");
    }
}

/// ② 顶层命令：每扩展可取回且**非空**（apps 在测试机枚举开始菜单 + PATH，
/// 与 `dd-ext-apps` 自身单测同环境，可断言非空）。
#[test]
fn builtin_top_level_commands_non_empty() {
    let exe_dir = builtin_exe_dir();
    if !all_builtins_present(&exe_dir) {
        eprintln!(
            "SKIP: 内置扩展未全部构建，先 `cargo build`（cargo test --workspace 会自动构建）"
        );
        return;
    }
    for spec in BUILTINS {
        let mut process = spawn_builtin(&exe_dir, spec);
        let cmds = process
            .top_level_commands()
            .expect("top_level_commands 成功");
        assert!(
            !cmds.is_empty(),
            "{} 顶层命令不应为空（id={}）",
            spec.id,
            spec.name
        );
        // 顶层的 command 引用必须是 Invoke 或 Page（可执行/可导航）
        for cmd in &cmds {
            assert!(
                matches!(cmd.command, CommandRef::Invoke | CommandRef::Page { .. }),
                "{} 顶层命令 command 应为 Invoke/Page：{:?}",
                spec.id,
                cmd.command
            );
        }
        process.close().expect("优雅关闭");
    }
}

/// ③ 兜底契约（§6.2）：有兜底的 3 个扩展 → 模板非空含 `{query}`；
/// 无兜底的 2 个（apps/system）→ 空列表（宿主据此判定"无兜底能力"）。
#[test]
fn builtin_fallback_commands_contract() {
    let exe_dir = builtin_exe_dir();
    if !all_builtins_present(&exe_dir) {
        eprintln!(
            "SKIP: 内置扩展未全部构建，先 `cargo build`（cargo test --workspace 会自动构建）"
        );
        return;
    }
    let expects_fallback = ["com.ddrun.calc", "com.ddrun.websearch", "com.ddrun.shell"];
    let no_fallback = ["com.ddrun.apps", "com.ddrun.system"];

    for spec in BUILTINS {
        let mut process = spawn_builtin(&exe_dir, spec);
        let templates = process.fallback_commands().expect("fallback_commands 成功");
        if expects_fallback.contains(&spec.id) {
            assert!(
                !templates.is_empty(),
                "{} 应有兜底模板（has_fallback=true）",
                spec.id
            );
            for t in &templates {
                assert!(
                    t.title.contains("{query}"),
                    "{} 兜底模板 title 必须含 {{query}}：{:?}",
                    spec.id,
                    t.title
                );
                assert!(
                    matches!(t.command, CommandRef::Invoke),
                    "{} 兜底模板必须是 Invoke：{:?}",
                    spec.id,
                    t.command
                );
            }
        } else {
            assert!(no_fallback.contains(&spec.id), "未知扩展 id：{}", spec.id);
            assert!(
                templates.is_empty(),
                "{} 无兜底（has_fallback=false）→ 空列表，实际 {} 条",
                spec.id,
                templates.len()
            );
        }
        process.close().expect("优雅关闭");
    }
}

/// ④ calc：fallback invoke（query 求值）→ `ShowToast("= 2")` +
/// `host/set_clipboard` 请求被宿主应答并记录（text="= 2"，M4 P2 记录语义）。
#[test]
fn builtin_calc_invoke_evaluates_and_requests_clipboard() {
    let exe_dir = builtin_exe_dir();
    if !all_builtins_present(&exe_dir) {
        eprintln!(
            "SKIP: 内置扩展未全部构建，先 `cargo build`（cargo test --workspace 会自动构建）"
        );
        return;
    }
    let spec = &BUILTINS[1]; // com.ddrun.calc
    let mut process = spawn_builtin(&exe_dir, spec);

    let params = InvokeParams {
        id: "calc.eval.query".to_string(),
        sender: Sender::TopLevel,
        context: Some(InvokeContext {
            query: Some("1+1".to_string()),
            selected_item_id: None,
            form_data: None,
            confirmed: None,
        }),
    };
    let result = process.invoke(&params).expect("invoke 成功");
    assert_eq!(
        result,
        CommandResult::ShowToast {
            message: "= 2".to_string(),
            duration_ms: Some(3_000),
        },
        "1+1 应计算为 = 2"
    );

    // host/set_clipboard：invoke 响应后扩展异步发请求，轮询应答 + drain 取走
    let mut requests = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        let _ = process.poll_notifications();
        requests = process.drain_host_requests();
        if !requests.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        requests.iter().any(|m| {
            m.method.as_deref() == Some("host/set_clipboard")
                && m.params
                    .as_ref()
                    .and_then(|p| p.get("text"))
                    .and_then(|v| v.as_str())
                    == Some("= 2")
        }),
        "应记录 host/set_clipboard(text=\"= 2\")：{requests:?}"
    );

    // 求值错误：ShowToast 报错且无副作用
    let bad = InvokeParams {
        id: "calc.eval.query".to_string(),
        sender: Sender::TopLevel,
        context: Some(InvokeContext {
            query: Some("1/0".to_string()),
            selected_item_id: None,
            form_data: None,
            confirmed: None,
        }),
    };
    let result = process.invoke(&bad).expect("invoke 成功");
    assert!(
        matches!(result, CommandResult::ShowToast { ref message, .. } if message.contains("除以 0")),
        "1/0 应报错"
    );

    process.close().expect("优雅关闭");
}

/// ⑤ websearch：fallback query → `Dismiss` + `host/open_url`（q 参数 RFC 3986 编码）。
#[test]
fn builtin_websearch_fallback_invoke_builds_encoded_url() {
    let exe_dir = builtin_exe_dir();
    if !all_builtins_present(&exe_dir) {
        eprintln!(
            "SKIP: 内置扩展未全部构建，先 `cargo build`（cargo test --workspace 会自动构建）"
        );
        return;
    }
    let spec = &BUILTINS[3]; // com.ddrun.websearch
    let mut process = spawn_builtin(&exe_dir, spec);

    let params = InvokeParams {
        id: "websearch.google.query".to_string(),
        sender: Sender::TopLevel,
        context: Some(InvokeContext {
            query: Some("rust command palette".to_string()),
            selected_item_id: None,
            form_data: None,
            confirmed: None,
        }),
    };
    let result = process.invoke(&params).expect("invoke 成功");
    assert_eq!(result, CommandResult::Dismiss, "有 query 搜索后关闭面板");

    let mut requests = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        let _ = process.poll_notifications();
        requests = process.drain_host_requests();
        if !requests.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        requests.iter().any(|m| {
            m.method.as_deref() == Some("host/open_url")
                && m.params
                    .as_ref()
                    .and_then(|p| p.get("url"))
                    .and_then(|v| v.as_str())
                    == Some("https://www.google.com/search?q=rust%20command%20palette")
        }),
        "应记录 host/open_url（编码后的 URL）：{requests:?}"
    );

    process.close().expect("优雅关闭");
}

/// ⑥ system：危险命令首发 → `Confirm{is_critical:true}`（§8.3）。
/// **不重发 confirmed=true**——那会真的 shutdown/restart/logoff（A10 真机人工核对项）。
#[test]
fn builtin_system_dangerous_first_invoke_confirms() {
    let exe_dir = builtin_exe_dir();
    if !all_builtins_present(&exe_dir) {
        eprintln!(
            "SKIP: 内置扩展未全部构建，先 `cargo build`（cargo test --workspace 会自动构建）"
        );
        return;
    }
    let spec = &BUILTINS[2]; // com.ddrun.system
    let mut process = spawn_builtin(&exe_dir, spec);

    let params = InvokeParams {
        id: "system.shutdown".to_string(),
        sender: Sender::TopLevel,
        context: None,
    };
    let result = process.invoke(&params).expect("invoke 成功");
    assert!(
        matches!(
            result,
            CommandResult::Confirm {
                is_critical: true,
                ..
            }
        ),
        "关机首发应返回 Confirm(is_critical=true)"
    );

    // 未知命令 → ShowToast（扩展侧防御）
    let nope = InvokeParams {
        id: "system.nope".to_string(),
        sender: Sender::TopLevel,
        context: None,
    };
    let result = process.invoke(&nope).expect("invoke 成功");
    assert!(
        matches!(result, CommandResult::ShowToast { .. }),
        "未知命令应 ShowToast：{result:?}"
    );

    process.close().expect("优雅关闭");
}
