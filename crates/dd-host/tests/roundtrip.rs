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
use dd_host::process::{ExtensionProcess, ProtocolError};
use dd_protocol::messages::error_codes;
use dd_protocol::model::CommandRef;

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

    // ③ top_level_commands（§6.1：2 条硬编码命令）
    let commands = process.top_level_commands().expect("拉取顶层命令");
    assert_eq!(commands.len(), 2, "M0 任务表要求 2 条硬编码命令");
    let ids: Vec<&str> = commands.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["sample.hello", "sample.copy"]);
    for item in &commands {
        assert!(!item.title.is_empty());
        assert_eq!(item.command, CommandRef::Invoke);
    }
    // §8.1 可选字段：第 1 条带 section/tags/details/text_to_suggest
    assert_eq!(commands[0].section.as_deref(), Some("Sample"));
    assert!(commands[0].details.is_some());
    // 第 2 条带 more_commands（§8.1 上下文菜单）
    let more = commands[1].more_commands.as_ref().expect("more_commands");
    assert_eq!(more.len(), 1);
    assert_eq!(more[0].id, "sample.copy.plain");

    // ④ close（§6.6：返回 result 后进程自行退出）
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
