//! dd-run 开发自检 CLI（M0 范围；产物名 `dd-run-cli`——对外 `dd-run.exe` 已由
//! GUI 宿主入口（`crates/dd-gui`）占用）。
//!
//! | 子命令 | 对应 M0 任务 | 说明 |
//! |---|---|---|
//! | `--list-extensions` | CLI | 扫描扩展目录，打印可用扩展与校验错误 |
//! | `--roundtrip` | 完成判据第 3 条 | spawn → `initialize` → `top_level_commands` → `close` 全链路自检 |
//! | `--conformance` | **M8（扩展生态验证）** | **全表面**一致性自检：在 `--roundtrip` 基础上补 `fallback_commands` / `get_command` / `get_items` / `invoke` / `host/*` 往返 —— 第三方扩展「绿灯即合规」 |
//!
//! 契约来源：[`docs/manifest-schema.md`](../../docs/manifest-schema.md)（扫描与校验）、
//! [`docs/protocol.md`](../../docs/protocol.md)（握手与全链路）。

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use dd_host::manifest::{
    self, LoadedExtension, ScanOptions, ScanOutcome, SkipReason, HOST_CAPABILITIES,
};
use dd_host::process::{self, ExtensionProcess};
use serde_json::Value;

/// 协议版本（§5.1：宿主发送它支持的**最高**版本）。
const PROTOCOL_VERSION: &str = "1.0";
/// 宿主版本，用于清单校验规则 6 与 `initialize` 的 `host.version`。
const HOST_VERSION: &str = env!("CARGO_PKG_VERSION");
/// 仓库内示例扩展目录（M0 默认；发布后回落到 §2 的平台目录）。
const SAMPLE_DIR: &str = "examples/extensions.d";
/// §8.3 `CommandResult` 的 8 种 `kind`（`--conformance` 据此校验 `invoke` 返回值）。
const RESULT_KINDS: [&str; 8] = [
    "Dismiss",
    "GoHome",
    "GoBack",
    "Hide",
    "KeepOpen",
    "GoToPage",
    "ShowToast",
    "Confirm",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    List,
    Roundtrip,
    Conformance,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut command: Option<Command> = None;
    let mut dir: Option<PathBuf> = None;
    let mut ext_id: Option<String> = None;
    let mut do_invoke = false;
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            "--list-extensions" => command = Some(Command::List),
            "--roundtrip" => command = Some(Command::Roundtrip),
            "--conformance" => command = Some(Command::Conformance),
            // §6.5 `invoke` 有真实副作用（改剪贴板 / 开浏览器 / 关机…）
            // → `--conformance` 默认**不**调用它，必须显式打开。
            "--invoke" => do_invoke = true,
            "--ext-id" => match iter.next() {
                Some(value) => ext_id = Some(value.clone()),
                None => {
                    eprintln!("错误：`--ext-id` 缺少扩展 id");
                    return ExitCode::FAILURE;
                }
            },
            "--extensions-dir" => match iter.next() {
                Some(value) => dir = Some(PathBuf::from(value)),
                None => {
                    eprintln!("错误：`--extensions-dir` 缺少目录参数");
                    return ExitCode::FAILURE;
                }
            },
            other => {
                eprintln!("错误：未知参数 `{other}`");
                print_usage();
                return ExitCode::FAILURE;
            }
        }
    }

    let Some(command) = command else {
        print_usage();
        return ExitCode::FAILURE;
    };

    let explicit_dir = dir.is_some();
    let dir = dir.unwrap_or_else(default_extensions_dir);
    let opts = ScanOptions {
        platform: manifest::current_platform().to_string(),
        host_version: HOST_VERSION.to_string(),
        home: manifest::home_dir().unwrap_or_default(),
    };

    match command {
        Command::List => list_extensions(&dir, &opts, explicit_dir),
        Command::Roundtrip => roundtrip(&dir, &opts, explicit_dir),
        Command::Conformance => {
            conformance(&dir, &opts, explicit_dir, do_invoke, ext_id.as_deref())
        }
    }
}

fn print_usage() {
    println!(
        "dd-run {HOST_VERSION}（扩展自检 CLI）\n\
         \n\
         用法：\n\
         \x20 dd-run --list-extensions    [--extensions-dir <DIR>]\n\
         \x20 dd-run --roundtrip          [--extensions-dir <DIR>]\n\
         \x20 dd-run --conformance        [--extensions-dir <DIR>] [--ext-id <ID>] [--invoke]\n\
         \x20 dd-run --help\n\
         \n\
         \x20 --list-extensions   扫描扩展目录，打印可用扩展与校验错误\n\
         \x20 --roundtrip         spawn 首个可用扩展，走 initialize → top_level_commands → close\n\
         \x20 --conformance       全表面一致性自检（补 fallback / get_command / get_items / invoke / host/*）\n\
         \x20 --invoke            --conformance 时也执行一次 invoke（**有真实副作用**，默认跳过）\n\
         \x20 --ext-id <ID>       指定要自检的扩展 id（目录内有多个时用）\n\
         \x20 --extensions-dir    覆盖扫描目录（默认 {SAMPLE_DIR}，不存在时回落到平台目录）"
    );
}

/// 内置示例扩展：与 `dd-run` 同目录的 `dd-ext-sample`
///（cargo 把 workspace 所有 bin 产物放在同一目录）。
fn builtin_sample() -> Option<LoadedExtension> {
    let mut dir = std::env::current_exe().ok()?;
    dir.pop();
    let name = if cfg!(windows) {
        "dd-ext-sample.exe"
    } else {
        "dd-ext-sample"
    };
    let command = dir.join(name);
    if !command.is_file() {
        return None;
    }
    Some(manifest::from_executable(
        command,
        "com.example.sample",
        "Sample",
    ))
}

/// 默认扫描目录：优先仓库内示例目录，否则按 §2 取平台目录。
fn default_extensions_dir() -> PathBuf {
    let sample = PathBuf::from(SAMPLE_DIR);
    if sample.is_dir() {
        sample
    } else {
        manifest::extensions_dir().unwrap_or(sample)
    }
}

/// M0 任务表「CLI」：`dd-run --list-extensions`。
///
/// 与 `--roundtrip` 一致：未显式指定 `--extensions-dir` 且扫描无可用扩展时，
/// 兜底显示内置示例扩展（示例清单的 `entry.command` 指向部署形态的同目录
/// 二进制，仓库里不含构建产物，属预期报错）。
fn list_extensions(dir: &Path, opts: &ScanOptions, explicit_dir: bool) -> ExitCode {
    println!("扩展目录：{}", dir.display());
    let outcome = manifest::scan_dir(dir, opts);
    print_scan(&outcome);

    if !outcome.loaded.is_empty() {
        return ExitCode::SUCCESS;
    }
    if !explicit_dir {
        if let Some(ext) = builtin_sample() {
            println!(
                "\n（示例目录无可用扩展，兜底使用内置示例扩展：{}）",
                ext.command.display()
            );
            println!(
                "✓  {:<24} {:<16} v{:<10} frozen={:<5} caps={}",
                ext.manifest.id,
                ext.manifest.name,
                ext.manifest.version,
                ext.manifest.frozen,
                ext.manifest.capabilities.len()
            );
            println!("     入口：{}", ext.command.display());
            return ExitCode::SUCCESS;
        }
    }
    ExitCode::FAILURE
}

fn print_scan(outcome: &ScanOutcome) {
    if let Some(err) = &outcome.dir_error {
        println!("!  目录不可读（视为无扩展）：{err}");
    }
    for ext in &outcome.loaded {
        println!(
            "✓  {:<24} {:<16} v{:<10} frozen={:<5} caps={}",
            ext.manifest.id,
            ext.manifest.name,
            ext.manifest.version,
            ext.manifest.frozen,
            ext.manifest.capabilities.len()
        );
        println!("     清单：{}", ext.path.display());
        println!("     入口：{}", ext.command.display());
    }
    for skipped in &outcome.skipped {
        let mark = if skipped.reason.is_error() {
            "✗"
        } else {
            "-"
        };
        println!("{mark}  {} → {}", skipped.path.display(), skipped.reason);
        if let SkipReason::EntryNotExecutable(_) = skipped.reason {
            println!("     提示：entry.command 需指向已构建的扩展可执行文件");
        }
    }
    let errors = outcome
        .skipped
        .iter()
        .filter(|s| s.reason.is_error())
        .count();
    println!(
        "共 {} 个可用，{} 个被跳过（其中 {} 个为错误）",
        outcome.loaded.len(),
        outcome.skipped.len(),
        errors
    );
}

/// M0 完成判据第 3 条：宿主 spawn 示例扩展 → `initialize` → `top_level_commands`
/// → `close` 全链路往返成功。
///
/// `explicit_dir` 为 `false`（未传 `--extensions-dir`）且扫描无可用扩展时，
/// 兜底使用与 `dd-run` 同目录的内置示例扩展——保证自检开箱可跑。
fn roundtrip(dir: &Path, opts: &ScanOptions, explicit_dir: bool) -> ExitCode {
    println!("扩展目录：{}", dir.display());
    let outcome = manifest::scan_dir(dir, opts);
    print_scan(&outcome);

    let (ext, from_builtin) = match pick_extension(outcome, explicit_dir, None) {
        Some(pair) => pair,
        None => {
            println!("\n✗ 没有可用扩展，无法进行全链路自检");
            return ExitCode::FAILURE;
        }
    };
    if from_builtin {
        println!(
            "\n（示例目录无可用扩展，兜底使用内置示例扩展：{}）",
            ext.command.display()
        );
    }

    println!("\n全链路自检：{}", ext.manifest.id);
    let started = Instant::now();

    // ① spawn（§4 discovered → spawned）
    let mut process = match ExtensionProcess::spawn(&ext) {
        Ok(process) => process,
        Err(e) => {
            println!("  1) spawn         ✗ {e}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "  1) spawn         ✓ {}（{} ms）",
        ext.command.display(),
        started.elapsed().as_millis()
    );

    // ② initialize（§5 spawned → initializing → ready）
    let step = Instant::now();
    let init = match process.initialize(PROTOCOL_VERSION, HOST_VERSION) {
        Ok(result) => result,
        Err(e) => {
            println!("  2) initialize    ✗ {e}");
            if !process.stderr().is_empty() {
                println!("     扩展 stderr：{}", process.stderr().trim());
            }
            return ExitCode::FAILURE;
        }
    };
    if init.provider.id != ext.manifest.id {
        // 清单 schema §8：不一致时宿主以清单为准并记警告
        println!(
            "  2) initialize    ⚠ provider.id `{}` 与清单 id 不一致（以清单为准）",
            init.provider.id
        );
    }
    println!(
        "  2) initialize    ✓ 协议 {} · provider {} · frozen={} · has_fallback={}（{} ms）",
        init.protocol_version,
        init.provider.id,
        init.provider.frozen,
        init.provider.has_fallback,
        step.elapsed().as_millis()
    );

    // ③ top_level_commands（§6.1）
    let step = Instant::now();
    let commands = match process.top_level_commands() {
        Ok(commands) => commands,
        Err(e) => {
            println!("  3) top_level     ✗ {e}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "  3) top_level     ✓ {} 条命令（{} ms）",
        commands.len(),
        step.elapsed().as_millis()
    );
    for item in &commands {
        println!(
            "       - {:<16} {}{}",
            item.id,
            item.title,
            item.section
                .as_ref()
                .map(|s| format!("  [{s}]"))
                .unwrap_or_default()
        );
    }

    // ④ close（§6.6：发 close → 等 result → 等进程自行退出，超时强杀）
    let step = Instant::now();
    match process.close() {
        Ok(()) => {
            println!(
                "  4) close         ✓ 进程已退出（{} ms）",
                step.elapsed().as_millis()
            );
        }
        Err(e) => {
            println!("  4) close         ✗ {e}");
            return ExitCode::FAILURE;
        }
    }

    println!(
        "\n✓ 全链路往返成功（总耗时 {} ms）",
        started.elapsed().as_millis()
    );
    ExitCode::SUCCESS
}

/// 从扫描结果挑出要自检的扩展。
///
/// 顺序：`--ext-id` 精确匹配 → 第一个可用扩展 → （未显式指定目录时）内置示例扩展兜底。
/// 返回 `(扩展, 是否来自内置兜底)`——兜底路径绕过了 §7 磁盘校验，调用方**必须**注明。
fn pick_extension(
    outcome: ScanOutcome,
    explicit_dir: bool,
    ext_id: Option<&str>,
) -> Option<(LoadedExtension, bool)> {
    let mut loaded = outcome.loaded;
    if let Some(wanted) = ext_id {
        let pos = loaded.iter().position(|e| e.manifest.id == wanted)?;
        return Some((loaded.swap_remove(pos), false));
    }
    if !loaded.is_empty() {
        return Some((loaded.swap_remove(0), false));
    }
    if explicit_dir {
        return None;
    }
    builtin_sample().map(|ext| (ext, true))
}

// ── M8：`--conformance` 全表面一致性自检 ────────────────────────────────

/// 自检结果计数器：任何 `✗` 都让退出码非 0；`⚠` 仅提示，不算失败。
struct Check {
    failures: usize,
}

impl Check {
    fn pass(&self, label: &str, detail: impl AsRef<str>) {
        println!("  {:<18} ✓ {}", label, detail.as_ref());
    }

    fn warn(&self, label: &str, detail: impl AsRef<str>) {
        println!("  {:<18} ⚠ {}", label, detail.as_ref());
    }

    fn fail(&mut self, label: &str, detail: impl AsRef<str>) {
        self.failures += 1;
        println!("  {:<18} ✗ {}", label, detail.as_ref());
    }
}

/// 发起一次请求并取回 `result` 的**原始 JSON**。
///
/// `--conformance` 刻意不用强类型封装：一致性检查的对象是**线上的 JSON 形状**
/// （协议 §6/§8 逐字规定），强类型反序列化成功反而会掩盖「多字段/错类型」类问题。
fn call_json(
    proc: &mut ExtensionProcess,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, String> {
    proc.call(method, params, timeout)
        .map_err(|e| e.to_string())
}

/// §6.2 一致性判据：`fallback_commands` **非空** ⟺ `provider.has_fallback`。
///
/// 两个方向都要拦：声明了却没给（宿主取不到兜底项）与给了却没声明
/// （宿主按 `has_fallback=false` 落盘桩，兜底项永远不会被展示）。
fn fallback_is_consistent(template_count: usize, has_fallback: bool) -> bool {
    (template_count > 0) == has_fallback
}

/// §6.5：`invoke` 成功响应**解开 JSON-RPC 信封后的内层 `result`** 就是 §8.3
/// `CommandResult` 本体，判别字段是 `kind`。
///
/// 特意把「多包了一层 `{"result": ...}`」单独识别出来**并指名**：这是最容易被
/// 文档示例带偏的一步。首版自检器读的是 `v["result"]["kind"]`——与错误形状
/// **恰好吻合**，于是对不合规的扩展判绿（真机才暴露）。纯函数 + 单测锁死。
fn command_result_kind(value: &Value) -> Result<&str, String> {
    let Some(obj) = value.as_object() else {
        return Err(format!("result 不是对象（§8.3）：{value}"));
    };
    if let Some(kind) = obj.get("kind").and_then(Value::as_str) {
        return Ok(kind);
    }
    if obj.get("result").is_some_and(Value::is_object) {
        return Err(
            "result 被多包了一层 `{\"result\": ...}`——§6.5 信封的 `result` 就是 \
             CommandResult 本体，不要再嵌套"
                .to_string(),
        );
    }
    Err("result 缺 `kind` 字段（§8.3 CommandResult）".to_string())
}

/// 全表面一致性自检（M8）：把协议 §5–§8 的每个方法与数据模型逐条走一遍。
///
/// 与 `--roundtrip` 的差别：后者只证明「能拉起、能握手、能列命令、能关闭」；
/// 本命令额外覆盖 **兜底 / 桩复热取命令 / 嵌套页 / 执行 / 反向 host 请求**，
/// 让第三方扩展作者在**没有 GUI** 的情况下判断自己是否合规（绿灯即合规）。
///
/// `do_invoke` 默认 `false`：`invoke` 会产生**真实副作用**，必须显式打开。
fn conformance(
    dir: &Path,
    opts: &ScanOptions,
    explicit_dir: bool,
    do_invoke: bool,
    ext_id: Option<&str>,
) -> ExitCode {
    println!("扩展目录：{}", dir.display());
    let outcome = manifest::scan_dir(dir, opts);
    print_scan(&outcome);

    let total = outcome.loaded.len();
    let (ext, from_builtin) = match pick_extension(outcome, explicit_dir, ext_id) {
        Some(pair) => pair,
        None => {
            println!("\n✗ 没有可自检的扩展（检查 `--ext-id` 或目录内容）");
            return ExitCode::FAILURE;
        }
    };
    if from_builtin {
        println!(
            "\n（示例目录无可用扩展，兜底使用内置示例扩展：{}）",
            ext.command.display()
        );
    }
    if ext_id.is_none() && total > 1 {
        println!("（目录内有 {total} 个扩展，本命令只自检第一个；用 `--ext-id <ID>` 指定其他）");
    }

    println!("\n一致性自检（协议全表面）：{}", ext.manifest.id);
    println!("契约：docs/protocol.md · docs/manifest-schema.md");
    let mut check = Check { failures: 0 };
    let started = Instant::now();

    // ① spawn（§4 discovered → spawned）
    let mut proc = match ExtensionProcess::spawn(&ext) {
        Ok(p) => {
            check.pass("1) spawn", ext.command.display().to_string());
            p
        }
        Err(e) => {
            check.fail("1) spawn", e.to_string());
            return summarize(check, started);
        }
    };

    // ② initialize（§5 握手 + §5.3 版本协商）
    let init = match proc.initialize(PROTOCOL_VERSION, HOST_VERSION) {
        Ok(r) => r,
        Err(e) => {
            check.fail("2) initialize", e.to_string());
            let err = proc.stderr();
            if !err.trim().is_empty() {
                println!("     扩展 stderr 末尾：{}", tail(&err, 400));
            }
            return summarize(check, started);
        }
    };
    check.pass(
        "2) initialize",
        format!(
            "协议 {} · provider {} · frozen={} · has_fallback={}",
            init.protocol_version,
            init.provider.id,
            init.provider.frozen,
            init.provider.has_fallback
        ),
    );
    if init.provider.id != ext.manifest.id {
        check.warn(
            "2a) provider.id",
            format!(
                "`{}` 与清单 `{}` 不一致（宿主以清单为准并记警告，§7 规则注释）",
                init.provider.id, ext.manifest.id
            ),
        );
    }
    for cap in &init.capabilities {
        if !HOST_CAPABILITIES.contains(&cap.as_str()) {
            check.fail(
                "2b) capabilities",
                format!("自述能力 `{cap}` 不在宿主白名单内（§7.4 只会回 -32601）"),
            );
        }
    }

    // ③ top_level_commands（§6.1）+ 结构校验（§8.1 / §8.2）
    let commands = match call_json(
        &mut proc,
        "top_level_commands",
        serde_json::json!({}),
        process::TIMEOUT_TOP_LEVEL_COMMANDS,
    ) {
        Ok(v) => match v.get("commands").and_then(Value::as_array) {
            Some(list) => list.clone(),
            None => {
                check.fail("3) top_level", "result 缺少 `commands` 数组（§6.1）");
                return summarize(check, started);
            }
        },
        Err(e) => {
            check.fail("3) top_level", e);
            return summarize(check, started);
        }
    };
    if commands.is_empty() {
        check.warn("3) top_level", "空数组（合法，但首屏不会出现本扩展的命令）");
    } else {
        check.pass("3) top_level", format!("{} 条命令", commands.len()));
    }
    let mut structure: Vec<String> = Vec::new();
    for (index, item) in commands.iter().enumerate() {
        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        let title = item
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if id.trim().is_empty() {
            structure.push(format!("#{index} 缺 `id` 或 id 为空"));
        }
        if title.trim().is_empty() {
            structure.push(format!("`{id}` 缺 `title` 或 title 为空"));
        }
        match item.get("command") {
            None => structure.push(format!("`{id}` 缺必填字段 `command`（§8.1）")),
            Some(cmd) => match cmd.get("kind").and_then(Value::as_str) {
                Some("invoke") => {}
                Some("page") => {
                    let page_ok = cmd
                        .get("page_id")
                        .and_then(Value::as_str)
                        .is_some_and(|p| !p.trim().is_empty());
                    if !page_ok {
                        structure.push(format!("`{id}` 的 page 命令缺有效 `page_id`（§8.2）"));
                    }
                }
                Some(other) => structure.push(format!(
                    "`{id}` 的 `command.kind` = `{other}` 非法（只允许 invoke / page）"
                )),
                None => structure.push(format!("`{id}` 的 `command` 缺 `kind`")),
            },
        }
    }
    if structure.is_empty() {
        check.pass("3a) items", format!("{} 项字段齐全", commands.len()));
    } else {
        for issue in structure {
            check.fail("3a) items", issue);
        }
    }

    // ④ fallback_commands（§6.2）—— 非空 ⟺ has_fallback
    let fallback = match call_json(
        &mut proc,
        "fallback_commands",
        serde_json::json!({}),
        process::TIMEOUT_FALLBACK_COMMANDS,
    ) {
        Ok(v) => v
            .get("commands")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        Err(e) => {
            check.fail("4) fallback", e);
            return summarize(check, started);
        }
    };
    if fallback_is_consistent(fallback.len(), init.provider.has_fallback) {
        check.pass(
            "4) fallback",
            format!(
                "{} 条模板 · has_fallback={} · 一致",
                fallback.len(),
                init.provider.has_fallback
            ),
        );
    } else {
        check.fail(
            "4) fallback",
            format!(
                "返回 {} 条但 has_fallback={} —— §6.2 要求两者一致（非空 ⟺ 具备兜底能力）",
                fallback.len(),
                init.provider.has_fallback
            ),
        );
    }
    for item in &fallback {
        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        let title = item
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !title.contains("{query}") {
            check.warn(
                "4a) template",
                format!("兜底项 `{id}` 的 title 不含 `{{query}}`（§6.2 占位符约定在 title）"),
            );
        }
    }

    // ⑤ get_command（§6.4）—— 顶层 id 必须可复热，否则桩态点击必失败
    let mut not_reheatable: Vec<String> = Vec::new();
    for item in &commands {
        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        match call_json(
            &mut proc,
            "get_command",
            serde_json::json!({ "id": id }),
            process::TIMEOUT_GET_COMMAND,
        ) {
            Ok(v) => {
                let found = v.get("command").is_some_and(|c| !c.is_null());
                if !found {
                    not_reheatable.push(id.to_string());
                }
            }
            Err(e) => check.fail("5) get_command", format!("`{id}`：{e}")),
        }
    }
    if not_reheatable.is_empty() {
        check.pass(
            "5) get_command",
            format!("{} 个顶层 id 均可复热", commands.len()),
        );
    } else {
        check.fail(
            "5) get_command",
            format!(
                "以下顶层 id 返回 `command: null` —— 桩复热会失败：{}",
                not_reheatable.join(", ")
            ),
        );
    }

    // ⑥ get_items（§6.3）—— 把每个 page 命令走一遍
    let pages: Vec<String> = commands
        .iter()
        .filter_map(|item| {
            let cmd = item.get("command")?;
            if cmd.get("kind").and_then(Value::as_str) != Some("page") {
                return None;
            }
            cmd.get("page_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    if pages.is_empty() {
        check.warn("6) get_items", "无 page 命令 —— 未覆盖嵌套页路径");
    } else {
        let mut problems: Vec<String> = Vec::new();
        for page_id in &pages {
            match call_json(
                &mut proc,
                "get_items",
                serde_json::json!({ "page_id": page_id }),
                process::TIMEOUT_GET_ITEMS,
            ) {
                Ok(v) => {
                    let items = match v.get("items").and_then(Value::as_array) {
                        Some(items) => items,
                        None => {
                            problems.push(format!("`{page_id}`：result 缺 `items` 数组（§6.3）"));
                            continue;
                        }
                    };
                    if v.get("has_more_items").and_then(Value::as_bool).is_none() {
                        problems.push(format!("`{page_id}`：result 缺布尔 `has_more_items`"));
                    }
                    if v.get("is_loading").and_then(Value::as_bool).is_none() {
                        problems.push(format!("`{page_id}`：result 缺布尔 `is_loading`"));
                    }
                    let blank = items
                        .iter()
                        .filter(|it| {
                            it.get("id")
                                .and_then(Value::as_str)
                                .is_none_or(|s| s.trim().is_empty())
                        })
                        .count();
                    if blank > 0 {
                        problems.push(format!("`{page_id}`：{blank} 项的 `id` 缺失或为空"));
                    }
                }
                Err(e) => problems.push(format!("`{page_id}`：{e}")),
            }
        }
        if problems.is_empty() {
            check.pass("6) get_items", format!("{} 个页结构合规", pages.len()));
        } else {
            for issue in problems {
                check.fail("6) get_items", issue);
            }
        }
    }

    // ⑦ invoke（§6.5 + §8.3）—— 默认跳过（真实副作用）
    if do_invoke {
        match commands
            .first()
            .and_then(|item| item.get("id"))
            .and_then(Value::as_str)
        {
            Some(id) => {
                let params = serde_json::json!({
                    "id": id,
                    "sender": "top_level",
                    "context": { "query": "dd-run-conformance" }
                });
                match call_json(&mut proc, "invoke", params, process::TIMEOUT_INVOKE) {
                    // 注意：`call` 已解开信封，直接拿内层 result 判 `kind`
                    // （**不是** `result.kind`——那正是漏检过的错误形状）。
                    Ok(v) => match command_result_kind(&v) {
                        Ok(kind) if RESULT_KINDS.contains(&kind) => {
                            check.pass("7) invoke", format!("`{id}` → {kind}"))
                        }
                        Ok(kind) => check.fail(
                            "7) invoke",
                            format!("返回的 `kind` = `{kind}` 不在 §8.3 的 8 种之内"),
                        ),
                        Err(issue) => check.fail("7) invoke", issue),
                    },
                    Err(e) => check.fail("7) invoke", format!("`{id}`：{e}")),
                }
            }
            None => check.warn("7) invoke", "无顶层命令可执行"),
        }
    } else {
        check.warn("7) invoke", "已跳过（有真实副作用；需要时加 --invoke）");
    }

    // ⑧ host/* 反向请求（§7）—— dd-host 已自动应答（已声明回 {}，未声明回 -32601）
    let used = proc.drain_host_requests();
    if used.is_empty() {
        check.pass(
            "8) host/*",
            "本次未发起反向请求（扩展不需要 host 能力时属正常）",
        );
    } else {
        let mut names: Vec<String> = used.iter().filter_map(|msg| msg.method.clone()).collect();
        names.sort();
        names.dedup();
        let undeclared: Vec<String> = names
            .iter()
            .filter(|m| !init.capabilities.contains(m))
            .cloned()
            .collect();
        if undeclared.is_empty() {
            check.pass(
                "8) host/*",
                format!(
                    "{} 次反向请求：{}（均已声明）",
                    used.len(),
                    names.join(", ")
                ),
            );
        } else {
            check.fail(
                "8) host/*",
                format!(
                    "使用了未在 initialize 中声明的能力：{}（§7.4 会被回 -32601）",
                    undeclared.join(", ")
                ),
            );
        }
    }

    // ⑨ close（§6.6）
    match proc.close() {
        Ok(()) => check.pass("9) close", "进程已优雅退出"),
        Err(e) => check.fail("9) close", e.to_string()),
    }

    summarize(check, started)
}

/// 输出结论并按失败数决定退出码。
fn summarize(check: Check, started: Instant) -> ExitCode {
    let ms = started.elapsed().as_millis();
    if check.failures == 0 {
        println!("\n✓ 一致性自检通过（{ms} ms）—— 该扩展可用于 dd-run");
        ExitCode::SUCCESS
    } else {
        println!("\n✗ 一致性自检失败：{} 项不合规（{ms} ms）", check.failures);
        ExitCode::FAILURE
    }
}

/// 取字符串末尾 `n` 个**字符**（诊断输出截断，避免刷屏）。
fn tail(text: &str, n: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= n {
        text.to_string()
    } else {
        chars[chars.len() - n..].iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §6.2 一致性判据的四象限——两个「不一致」方向都必须被拦下。
    ///
    /// 该用例的存在理由：`--conformance` 首版把判据写反（`is_empty() ==
    /// has_fallback`），对**完全正确**的示例报了红。纯函数 + 单测消费掉这类反转。
    #[test]
    fn fallback_consistency_matrix() {
        assert!(
            fallback_is_consistent(1, true),
            "非空 + has_fallback → 一致"
        );
        assert!(fallback_is_consistent(0, false), "空 + 无兜底 → 一致");
        assert!(
            !fallback_is_consistent(1, false),
            "给了兜底项却没声明 has_fallback → 不一致"
        );
        assert!(
            !fallback_is_consistent(0, true),
            "声明了 has_fallback 却没给兜底项 → 不一致"
        );
    }

    /// §8.3 规定 `CommandResult` 恰 8 种 `kind`，且互不重复。
    #[test]
    fn result_kinds_cover_all_eight() {
        assert_eq!(RESULT_KINDS.len(), 8, "§8.3 CommandResult 共 8 种");
        let unique: std::collections::HashSet<&str> = RESULT_KINDS.iter().copied().collect();
        assert_eq!(unique.len(), 8, "8 种 kind 不得重复");
    }

    /// §6.5 的 `invoke` 形状判据——**单层才对**，双层必须被拦下并指名。
    ///
    /// 该用例的存在理由：首版自检器读 `v["result"]["kind"]`，与错误形状吻合，
    /// 于是对双层响应的扩展判了绿，直到真机 GUI 才报
    /// `非法 JSON-RPC 信封：missing field \`kind\``。
    #[test]
    fn invoke_result_must_be_single_wrapped() {
        let single = serde_json::json!({
            "kind": "ShowToast",
            "args": {"message": "hi", "duration_ms": 2000}
        });
        assert_eq!(command_result_kind(&single).unwrap(), "ShowToast");

        let double = serde_json::json!({
            "result": {"kind": "ShowToast", "args": {"message": "hi"}}
        });
        let err = command_result_kind(&double).unwrap_err();
        assert!(
            err.contains("多包了一层"),
            "双层必须被指名拦下，实得：{err}"
        );

        let no_kind = serde_json::json!({ "args": {} });
        assert!(command_result_kind(&no_kind).is_err(), "缺 kind 必须报错");

        for not_object in [serde_json::Value::Null, serde_json::json!("ShowToast")] {
            assert!(
                command_result_kind(&not_object).is_err(),
                "非对象必须报错：{not_object}"
            );
        }
    }

    /// 诊断截断按**字符**计数（中文不应被按字节截断成乱码）。
    #[test]
    fn tail_slices_by_chars() {
        assert_eq!(tail("abcdef", 3), "def");
        assert_eq!(tail("abc", 5), "abc");
        assert_eq!(tail("", 5), "");
        assert_eq!(tail("中文诊断日志", 2), "日志");
    }
}
