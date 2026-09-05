//! 测试共享夹具（仅 `cfg(test)` 编译）：应用构造器与死进程桩。

use crate::app::aggregate::AggregatePayload;
use crate::app::PaletteApp;
use dd_gui::hotkey::HotkeyEvent;
use dd_gui::state::PanelItem;
use dd_gui::tray::TrayEvent;
use dd_host::cache::ColdStartTimer;
use dd_host::manifest::{Entry, LoadedExtension, Manifest};
use dd_host::process::ExtensionProcess;
use dd_protocol::model::CommandRef;
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;

/// 构造指定 ext_id + 命令的列表项。
pub(crate) fn item_with(ext_id: &str, command: CommandRef) -> PanelItem {
    PanelItem {
        ext_id: ext_id.to_string(),
        command,
        ..PanelItem::new("x")
    }
}

// ── 夹具 ───────────────────────────────────────────────

/// 构造「立即退出（exit 1）」的扩展进程夹具（A8 崩溃 / A10 拉取失败回路）。
/// 与 `dd-host/tests/roundtrip.rs::stdout_eof_is_reported_as_process_exited` 同款约定。
pub(crate) fn dying_ext(id: &str) -> LoadedExtension {
    let (command, args): (String, Vec<String>) = if cfg!(windows) {
        (
            "cmd.exe".to_string(),
            vec!["/c".to_string(), "exit 1".to_string()],
        )
    } else {
        (
            "/bin/sh".to_string(),
            vec!["-c".to_string(), "exit 1".to_string()],
        )
    };
    LoadedExtension {
        manifest: Manifest {
            schema_version: "1.0".to_string(),
            id: id.to_string(),
            name: "Dying".to_string(),
            version: "1.0.0".to_string(),
            description: String::new(),
            author: String::new(),
            license: String::new(),
            homepage: String::new(),
            icon: None,
            entry: Entry {
                command: command.clone(),
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
    }
}

/// spawn 一个已退出的进程（返回前等待其确实退出，确保 refresh_health / poll 稳定检测）。
pub(crate) fn dying_process(id: &str) -> ExtensionProcess {
    let mut proc = ExtensionProcess::spawn(&dying_ext(id)).expect("spawn 立即退出的进程");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !proc.has_exited() {
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    proc
}

/// 不依赖真实扩展 / 聚合的 PaletteApp（空 channel 注入）。
pub(crate) fn make_app() -> PaletteApp {
    let (_etx, events_rx) = mpsc::channel::<HotkeyEvent>();
    let (_ttx, tray_rx) = mpsc::channel::<TrayEvent>();
    let (_atx, agg_rx) = mpsc::channel::<AggregatePayload>();
    PaletteApp::new(
        events_rx,
        tray_rx,
        Arc::new(AtomicBool::new(false)),
        agg_rx,
        ColdStartTimer::new(),
        None,
        dd_gui::settings::Settings::default(),
    )
}

/// headless egui Context（无窗口/渲染后端，仅供 request_repaint 等无副作用调用）。
pub(crate) fn ctx() -> egui::Context {
    egui::Context::default()
}
