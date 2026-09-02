//! M0 完成判据第 3 条的运行时验证：
//! **宿主 spawn 示例扩展 → `initialize` → `top_level_commands` → `close` 全链路往返成功。**
//!
//! 与 `dd-protocol` 的一致性测试同构：本文件**不持有任何协议消息副本**，
//! 所有消息都由 `dd-host` 与 `dd-ext-sample` 在运行时按
//! [`docs/protocol.md`](../../docs/protocol.md) 产生——协议一改，测试立即按新行为跑。
//!
//! 示例扩展可执行文件由 cargo 注入：`env!("CARGO_BIN_EXE_dd-ext-sample")`。

use std::fs;
use std::path::{Path, PathBuf};

use dd_host::manifest::{Entry, LoadedExtension, Manifest, ScanOptions};
use dd_host::process::{ExtensionProcess, ProtocolError, TIMEOUT_GET_ITEMS, TIMEOUT_INVOKE};
use dd_protocol::messages::{
    error_codes, GetItemsParams, GetItemsResult, InvokeContext, InvokeParams,
};
use dd_protocol::model::{CommandRef, CommandResult, Sender};

/// 示例扩展可执行文件由 cargo 与测试二进制放在同一目录（测试二进制在 `deps/`
/// 子目录），按此定位——`CARGO_BIN_EXE_*` 只对**自身 package** 的 bin 生效，
/// 而 `dd-ext-sample` 是另一个 package，故不能用。
/// 定位示例扩展可执行文件（需先 `cargo build`，workspace 级别的 `cargo test` 会自动构建）。
fn sample_exe() -> PathBuf {
    let mut dir = std::env::current_exe().expect("当前测试二进制路径");
    dir.pop(); // 去掉测试二进制文件名
    if dir.file_name().and_then(|s| s.to_str()) == Some("deps") {
        dir.pop(); // 集成测试二进制位于 target/debug/deps/
    }
    let name = if cfg!(windows) {
        "dd-ext-sample.exe"
    } else {
        "dd-ext-sample"
    };
    let path = dir.join(name);
    assert!(
        path.is_file(),
        "找不到示例扩展可执行文件：{}（请先执行 `cargo build`）",
        path.display()
    );
    path
}

/// 测试用临时目录（避免引入 tempfile 依赖）。
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("系统时间早于 UNIX 纪元")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("dd-run-rt-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).expect("创建临时目录");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scan_options() -> ScanOptions {
    ScanOptions {
        platform: dd_host::manifest::current_platform().to_string(),
        host_version: "0.1.0".to_string(),
        home: std::env::temp_dir(),
    }
}

/// 在临时目录里写一份指向示例扩展的清单（命令用绝对路径，跨平台均可直接 spawn）。
fn write_sample_manifest(dir: &Path, id: &str) -> PathBuf {
    let manifest = serde_json::json!({
        "schema_version": "1.0",
        "id": id,
        "name": "Sample",
        "version": "1.0.0",
        "entry": { "command": sample_exe().to_string_lossy() },
        "frozen": true,
        "capabilities": [],
    });
    let path = dir.join(format!("{id}.json"));
    fs::write(&path, manifest.to_string()).expect("写入清单");
    path
}

fn load_sample(tmp: &TempDir) -> LoadedExtension {
    write_sample_manifest(tmp.path(), "com.example.sample");
    let outcome = dd_host::manifest::scan_dir(tmp.path(), &scan_options());
    assert!(
        outcome.skipped.is_empty(),
        "示例清单应通过全部校验：{:?}",
        outcome.skipped
    );
    assert_eq!(outcome.loaded.len(), 1);
    outcome.loaded.into_iter().next().expect("恰好一个扩展")
}

#[test]
fn scan_finds_sample_extension_and_resolves_entry() {
    let tmp = TempDir::new("scan");
    let ext = load_sample(&tmp);

    assert_eq!(ext.manifest.id, "com.example.sample");
    assert!(
        ext.command.is_file(),
        "entry.command 应解析为真实文件：{:?}",
        ext.command
    );
    assert_eq!(ext.cwd, tmp.path(), "cwd 缺省为清单所在目录");
}

#[test]
fn roundtrip_initialize_top_level_commands_close() {
    let tmp = TempDir::new("roundtrip");
    let ext = load_sample(&tmp);

    // ① spawn（§4：discovered → spawned）
    let mut process = ExtensionProcess::spawn(&ext).expect("spawn 示例扩展");

    // ② initialize（§5：spawned → initializing → ready）
    let init = process.initialize("1.0", "0.1.0").expect("握手成功");
    assert_eq!(init.protocol_version, "1.0");
    assert_eq!(
        init.provider.id, "com.example.sample",
        "provider.id 必须与清单 id 一致（清单 schema §8）"
    );
    assert!(init.provider.frozen);
    assert!(!init.provider.has_fallback);
    assert!(
        init.capabilities.is_empty(),
        "M0 示例不使用任何 host/* 能力"
    );

    // ③ top_level_commands（§6.1：M0 2 条 + 「M2 验收」分组 9 条，共 11 条）
    let commands = process.top_level_commands().expect("拉取顶层命令");
    assert_eq!(
        commands.len(),
        11,
        "M0 2 条 + M2 验收 9 条（Page + 7 种 Kind + 顶层通知；GoBack 属 A5 边界只在嵌套页）"
    );
    let ids: Vec<&str> = commands.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "sample.hello",
            "sample.copy",
            "m2.page",
            "m2.kind.dismiss",
            "m2.kind.hide",
            "m2.kind.go_home",
            "m2.kind.keep_open",
            "m2.kind.go_to_page",
            "m2.kind.show_toast",
            "m2.kind.confirm",
            "m2.top.notify",
        ]
    );
    for item in &commands {
        assert!(!item.title.is_empty());
    }
    // M2：m2.page 为 Page 引用（A5 嵌套页），其余 10 条均为 Invoke
    assert_eq!(
        commands[2].command,
        CommandRef::Page {
            page_id: "m2.page".to_string()
        }
    );
    for item in commands.iter().skip(3) {
        assert_eq!(item.command, CommandRef::Invoke);
    }
    // §8.1 可选字段：第 1 条带 section/tags/details/text_to_suggest
    assert_eq!(commands[0].section.as_deref(), Some("Sample"));
    assert!(commands[0].details.is_some());
    // 第 2 条带 more_commands（§8.1 上下文菜单）
    let more = commands[1].more_commands.as_ref().expect("more_commands");
    assert_eq!(more.len(), 1);
    assert_eq!(more[0].id, "sample.copy.plain");
    // 「M2 验收」分组 section（第 3 条起）
    assert!(
        commands[2..]
            .iter()
            .all(|c| c.section.as_deref() == Some("M2 验收")),
        "M2 命令应在同一分组"
    );

    // ④ close（§6.6：返回 result 后进程自行退出）
    process.close().expect("优雅关闭");
}

/// M2 链路运行时验证（支撑 m2-record.md §4 人工验收清单）：
/// `invoke`（8 种 Kind 分派 + Confirm 重发）/ `get_items`（嵌套页 + 未知页 -32005）/
/// `items_changed`（页级 + 顶层通知，A9 全量重拉计数自增）。
#[test]
fn roundtrip_m2_invoke_get_items_items_changed() {
    let tmp = TempDir::new("m2");
    let ext = load_sample(&tmp);
    let mut process = ExtensionProcess::spawn(&ext).expect("spawn 示例扩展");
    process.initialize("1.0", "0.1.0").expect("握手成功");

    let invoke = |process: &mut ExtensionProcess, id: &str, confirmed: bool| {
        process
            .call(
                "invoke",
                serde_json::json!(InvokeParams {
                    id: id.to_string(),
                    sender: Sender::TopLevel,
                    context: Some(InvokeContext {
                        query: None,
                        selected_item_id: None,
                        form_data: None,
                        confirmed: confirmed.then_some(true),
                    }),
                }),
                TIMEOUT_INVOKE,
            )
            .expect("invoke 成功")
    };

    // ① sample.hello → ShowToast（M2 清单 #1/#5）
    let value = invoke(&mut process, "sample.hello", false);
    let result: CommandResult = serde_json::from_value(value).expect("解析 CommandResult");
    assert_eq!(
        result,
        CommandResult::ShowToast {
            message: "Hello from dd-ext-sample！".to_string(),
            duration_ms: None,
        }
    );

    // ② Confirm 首发 → Confirm Kind；confirmed=true 重发 → ShowToast（§8.3 注，清单 #6）
    let value = invoke(&mut process, "m2.kind.confirm", false);
    let result: CommandResult = serde_json::from_value(value).expect("解析 CommandResult");
    assert!(
        matches!(result, CommandResult::Confirm { .. }),
        "首发应返回 Confirm，实际 {:?}",
        result
    );
    let value = invoke(&mut process, "m2.kind.confirm", true);
    let result: CommandResult = serde_json::from_value(value).expect("解析 CommandResult");
    assert!(
        matches!(result, CommandResult::ShowToast { .. }),
        "确认重发应返回 ShowToast，实际 {:?}",
        result
    );

    // ③ get_items（§6.3）：m2.page 4 条 + 拉取计数（清单 #2）；未知页 → -32005
    let get_page = |process: &mut ExtensionProcess, page_id: &str| {
        process.call(
            "get_items",
            serde_json::json!(GetItemsParams {
                page_id: page_id.to_string(),
                search_text: None,
            }),
            TIMEOUT_GET_ITEMS,
        )
    };
    let value = get_page(&mut process, "m2.page").expect("get_items 成功");
    let page: GetItemsResult = serde_json::from_value(value).expect("解析 GetItemsResult");
    assert_eq!(page.items.len(), 4, "m2.page 应返回 4 条子命令");
    assert!(
        page.items[0]
            .subtitle
            .as_deref()
            .unwrap_or_default()
            .contains("第 1 次被拉取"),
        "首次拉取计数应为 1：{:?}",
        page.items[0].subtitle
    );
    let err = get_page(&mut process, "nope").expect_err("未知页应报错");
    match err {
        ProtocolError::Rpc(e) => assert_eq!(e.code, error_codes::PAGE_NOT_FOUND, "§9.2 -32005"),
        other => panic!("应为 Rpc 错误，实际 {other:?}"),
    }

    // ④ items_changed（§7.1，A9）：页级通知 → poll 收到 → 重拉后计数 +1（清单 #8）
    let value = invoke(&mut process, "m2.page.notify", false);
    let result: CommandResult = serde_json::from_value(value).expect("解析 CommandResult");
    assert!(matches!(result, CommandResult::ShowToast { .. }));
    assert_eq!(
        process.poll_notifications(),
        vec![Some("m2.page".to_string())],
        "应收到页级 items_changed"
    );
    let value = get_page(&mut process, "m2.page").expect("通知后重拉");
    let page: GetItemsResult = serde_json::from_value(value).expect("解析 GetItemsResult");
    assert!(
        page.items[0]
            .subtitle
            .as_deref()
            .unwrap_or_default()
            .contains("第 2 次被拉取"),
        "重拉后计数应为 2：{:?}",
        page.items[0].subtitle
    );

    // ⑤ 顶层通知（page_id 缺省 = 顶层命令变了）
    let value = invoke(&mut process, "m2.top.notify", false);
    let result: CommandResult = serde_json::from_value(value).expect("解析 CommandResult");
    assert_eq!(result, CommandResult::KeepOpen);
    assert_eq!(
        process.poll_notifications(),
        vec![None],
        "page_id 缺省应表示顶层"
    );

    process.close().expect("优雅关闭");
}

#[test]
fn unknown_method_returns_method_not_found() {
    let tmp = TempDir::new("unknown");
    let ext = load_sample(&tmp);
    let mut process = ExtensionProcess::spawn(&ext).expect("spawn");
    process.initialize("1.0", "0.1.0").expect("握手");

    let err = process
        .call(
            "no_such_method",
            serde_json::json!({}),
            std::time::Duration::from_secs(3),
        )
        .expect_err("未知方法应返回错误响应");

    match err {
        ProtocolError::Rpc(rpc) => assert_eq!(rpc.code, error_codes::METHOD_NOT_FOUND),
        other => panic!("期望 RPC 错误响应，实际：{other}"),
    }
    process.close().expect("关闭");
}

#[test]
fn version_negotiation_rejects_version_higher_than_offered() {
    let tmp = TempDir::new("version");
    let ext = load_sample(&tmp);
    let mut process = ExtensionProcess::spawn(&ext).expect("spawn");

    // §5.3 规则 4：扩展回的版本高于宿主所发（0.9）时，宿主必须拒绝
    let err = process
        .initialize("0.9", "0.1.0")
        .expect_err("扩展回 1.0 > 宿主所发 0.9，应被拒绝");
    assert!(
        matches!(err, ProtocolError::BadProtocolVersion { .. }),
        "期望 BadProtocolVersion，实际：{err}"
    );
}

#[test]
fn stdout_eof_is_reported_as_process_exited() {
    // §11 崩溃检测的基础：子进程 stdout 立即 EOF 时，in-flight 请求必须失败而非挂起
    let (command, args): (&str, Vec<String>) = if cfg!(windows) {
        ("cmd.exe", vec!["/c".to_string(), "exit 1".to_string()])
    } else {
        ("/bin/sh", vec!["-c".to_string(), "exit 1".to_string()])
    };
    let ext = LoadedExtension {
        manifest: Manifest {
            schema_version: "1.0".to_string(),
            id: "com.example.dying".to_string(),
            name: "Dying".to_string(),
            version: "1.0.0".to_string(),
            description: String::new(),
            author: String::new(),
            license: String::new(),
            homepage: String::new(),
            icon: None,
            entry: Entry {
                command: command.to_string(),
                args,
                env: Default::default(),
                cwd: None,
            },
            frozen: true,
            capabilities: vec![],
            platforms: None,
            min_host_version: None,
        },
        path: PathBuf::from("dying.json"),
        dir: PathBuf::from("."),
        command: PathBuf::from(command),
        cwd: PathBuf::from("."),
    };

    let mut process = ExtensionProcess::spawn(&ext).expect("spawn 立即退出的进程");
    let err = process
        .call(
            "initialize",
            serde_json::json!({}),
            std::time::Duration::from_secs(5),
        )
        .expect_err("stdout EOF 应立即失败");

    assert!(
        matches!(err, ProtocolError::ProcessExited),
        "期望 ProcessExited，实际：{err}"
    );
    assert_eq!(
        err.as_rpc_error().map(|e| e.code),
        Some(error_codes::PROVIDER_UNAVAILABLE),
        "进程退出在 UI 层应呈现为 -32003 provider_unavailable（§11）"
    );
}

/// M3 桩复热链路 §6.4 运行时验证（支撑 m3-record.md §4）：
/// `get_command` 取回真实命令；未知 id 回 `command: null`（**正常结果、非错误**）。
#[test]
fn roundtrip_m3_get_command_reheat_chain() {
    let tmp = TempDir::new("m3");
    let ext = load_sample(&tmp);
    let mut process = ExtensionProcess::spawn(&ext).expect("spawn 示例扩展");
    process.initialize("1.0", "0.1.0").expect("握手成功");

    // ① 已知 Invoke 命令 id → Some，且与顶层目录定义一致（复热后可直接 invoke）
    let cmd = process
        .get_command("sample.hello")
        .expect("get_command 成功")
        .expect("sample.hello 应存在");
    assert_eq!(cmd.id, "sample.hello");
    assert_eq!(cmd.command, CommandRef::Invoke);
    assert!(!cmd.title.is_empty(), "取回的命令应带完整定义");

    // ② Page 命令 id → Some 且仍是 Page 引用（复热后仍可导航嵌套页，A5）
    let page_cmd = process
        .get_command("m2.page")
        .expect("get_command 成功")
        .expect("m2.page 应存在");
    assert_eq!(
        page_cmd.command,
        CommandRef::Page {
            page_id: "m2.page".to_string()
        }
    );

    // ③ 未知 id → Ok(None)（协议 §6.4：command:null 是正常结果，桩已失效由宿主回退）
    assert_eq!(
        process.get_command("no.such.command").expect("应 Ok"),
        None,
        "未知命令应回 null 而非错误"
    );

    process.close().expect("优雅关闭");
}
