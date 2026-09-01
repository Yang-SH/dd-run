//! dd-run 宿主 CLI（M0 范围）。
//!
//! | 子命令 | 对应 M0 任务 | 说明 |
//! |---|---|---|
//! | `--list-extensions` | CLI | 扫描扩展目录，打印可用扩展与校验错误 |
//! | `--roundtrip` | 完成判据第 3 条 | spawn → `initialize` → `top_level_commands` → `close` 全链路自检 |
//!
//! 契约来源：[`docs/manifest-schema.md`](../../docs/manifest-schema.md)（扫描与校验）、
//! [`docs/protocol.md`](../../docs/protocol.md)（握手与全链路）。

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use dd_host::manifest::{self, LoadedExtension, ScanOptions, ScanOutcome, SkipReason};
use dd_host::process::ExtensionProcess;

/// 协议版本（§5.1：宿主发送它支持的**最高**版本）。
const PROTOCOL_VERSION: &str = "1.0";
/// 宿主版本，用于清单校验规则 6 与 `initialize` 的 `host.version`。
const HOST_VERSION: &str = env!("CARGO_PKG_VERSION");
/// 仓库内示例扩展目录（M0 默认；发布后回落到 §2 的平台目录）。
const SAMPLE_DIR: &str = "examples/extensions.d";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    List,
    Roundtrip,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut command: Option<Command> = None;
    let mut dir: Option<PathBuf> = None;
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            "--list-extensions" => command = Some(Command::List),
            "--roundtrip" => command = Some(Command::Roundtrip),
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
    }
}

fn print_usage() {
    println!(
        "dd-run {HOST_VERSION}（M0：协议地基自检）\n\
         \n\
         用法：\n\
         \x20 dd-run --list-extensions [--extensions-dir <DIR>]\n\
         \x20 dd-run --roundtrip        [--extensions-dir <DIR>]\n\
         \x20 dd-run --help\n\
         \n\
         \x20 --list-extensions   扫描扩展目录，打印可用扩展与校验错误\n\
         \x20 --roundtrip         spawn 首个可用扩展，走 initialize → top_level_commands → close\n\
         \x20 --extensions-dir   覆盖扫描目录（默认 {SAMPLE_DIR}，不存在时回落到平台目录）"
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

    let mut loaded = outcome.loaded.into_iter();
    let ext = match loaded.next() {
        Some(ext) => ext,
        None if !explicit_dir => match builtin_sample() {
            Some(ext) => {
                println!(
                    "\n（示例目录无可用扩展，兜底使用内置示例扩展：{}）",
                    ext.command.display()
                );
                ext
            }
            None => {
                println!("\n✗ 没有可用扩展，无法进行全链路自检");
                return ExitCode::FAILURE;
            }
        },
        None => {
            println!("\n✗ 没有可用扩展，无法进行全链路自检");
            return ExitCode::FAILURE;
        }
    };

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
