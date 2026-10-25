//! B1（dd-run-改进分析.md §3.2）：UI 快照金图测试（egui_kittest，仅 `cfg(test)`）。
//!
//! **金图职责边界**：只回归「布局 / 间距 / 配色 / 层级」——真实字体栈（msyh/
//! segoeui 是否有字、缺字码位）由 §4.9 的字体单测与真机走查覆盖，两者互补。
//!
//! **确定性设计**（金图跨机器 / 跨 runner 逐字节稳定的三个前提）：
//! 1. **字体**：dev-dependency 重开 eframe `default_fonts`——egui 内置字体
//!    字节随 epaint 源码分发，全机器一致；刻意**不**走 `setup_cjk_fonts`
//!    系统字体栈（runner 镜像更新会漂移，且该函数仅在 bin 入口 `main.rs`
//!    调用，本模块直构 `PaletteApp` 不经过它）。内置字体无 CJK → 中文渲染
//!    为方块，但方块字形同样逐字节确定，布局回归不受影响。
//! 2. **动画**：`ui_animations = false`（Settings 覆盖）+ 空查询静态态。
//! 3. **数据**：预发一条**完整** `AggregatePayload`（聚合已完成语义），
//!    避免 Loading 转圈等逐帧变化；列表内容为固定夹具项。
//!
//! 出图 / 更新金图（**必须带 `-- --ignored`**：六测试已标 `#[ignore]`，
//! 不加则不会执行）：
//! `UPDATE_SNAPSHOTS=1 cargo test -p dd-gui --lib snapshot -- --ignored`；
//! 快照文件在 `crates/dd-gui/tests/snapshots/`（kittest 默认位置）。
//! 与产品体积零关联：dev-dependencies 不进任何产物。

use crate::app::aggregate::{AggregateEvent, AggregatePayload};
use crate::settings::{Settings, ThemePref};
use crate::test_support::item_with;
use crate::tray::TrayEvent;
use dd_gui::state::PanelItem;
use dd_protocol::model::CommandRef;
use eframe::egui;
use std::sync::mpsc;
use std::sync::Arc;

/// 快照面板尺寸：与真机默认 650×440 一致（分析文档 §3.2 口径）。
const SNAPSHOT_SIZE: egui::Vec2 = egui::vec2(650.0, 440.0);

/// 固定夹具列表项：两条内置扩展命令，标题纯 ASCII（内置字体无 CJK，标题含
/// 中文会全行方块——确定但可读性差，夹具用英文）。两行 id/标题必须不同：
/// egui 对重复 widget ID 会在画布上叠加调试告警（debug 构建），毁掉金图。
fn fixture_items() -> Vec<PanelItem> {
    let mut calc = item_with("com.ddrun.calc", CommandRef::Invoke);
    calc.id = "Calculator".to_string();
    calc.title = "Calculator".to_string();
    calc.pinyin = dd_gui::state::pinyin_haystack(&calc.title);
    let mut apps = item_with(
        "com.ddrun.apps",
        CommandRef::Page {
            page_id: "apps".to_string(),
        },
    );
    apps.id = "All Apps".to_string();
    apps.title = "All Apps".to_string();
    apps.pinyin = dd_gui::state::pinyin_haystack(&apps.title);
    vec![calc, apps]
}

/// 构造「聚合已完成 + 指定主题 + 关动画」的 `PaletteApp`（快照专用夹具）：
/// 空 channel 注入同 [`crate::test_support`]，但聚合通道预发一条完整 payload
/// （`payload_items` = 首帧落地后的 Root 列表内容），使首帧即「首屏就绪」态
/// （无 Loading 动画）；`configure` 在 `visible` 置真后对应用做最后的状态
/// 摆放（页面栈 / 右键菜单等）。
///
/// **注意时序**：payload 在首帧被 [`poll`](crate::app::poll_aggregate) 消费并
/// **重置 Root 列表**——configure 里对 `stack.root_mut().list` 的摆放会被
/// 覆盖（results_selected 首版即踩中，两图逐字节相同）；跨首帧的状态（如
/// 选中行）须经 `harness.state_mut()` 在首帧后摆放。
fn snapshot_app_with(
    theme: ThemePref,
    payload_items: Vec<PanelItem>,
    configure: impl FnOnce(&mut crate::app::PaletteApp),
) -> crate::app::PaletteApp {
    use dd_host::cache::ColdStartTimer;

    let settings = Settings {
        theme,
        ui_animations: false,
        ..Settings::default()
    };

    let (_ttx, tray_rx) = mpsc::channel::<TrayEvent>();
    let (atx, agg_rx) = mpsc::channel::<crate::app::aggregate::AggregateEvent>();
    // 预发完整聚合结果：空查询首屏 = 固定夹具项 + 单一 Warm 源摘要。
    let _ = atx.send(AggregateEvent::Full(AggregatePayload {
        items: payload_items,
        sources: vec![],
        processes: vec![],
        exts: vec![],
        trust: Default::default(),
        ledger_state: dd_host::trust::LedgerState::Ok,
        skipped: vec![],
        inproc_specs: Default::default(),
    }));

    let mut app = crate::app::PaletteApp::new(
        dd_gui::hotkey::HotkeyThread::dummy(),
        tray_rx,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        agg_rx,
        ColdStartTimer::new(),
        None,
        settings,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    // 构造点 visible = false（等热键唤起）——快照要画首屏，显式置真。
    // 直构不触 main.rs 的窗口显现链路（Show 队列等），仅应用态布尔。
    app.window.visible = true;
    configure(&mut app);
    app
}

/// 金图 harness：固定尺寸 + 固定主题 + 固定 pixels_per_point（1.0，屏蔽
/// 环境 DPI 差异）+ wgpu 渲染（`egui_kittest` `wgpu` feature，CI 走软件
/// 适配器：Windows = WARP / Linux = lavapipe）。
fn snapshot_harness(
    theme: egui::Theme,
    payload_items: Vec<PanelItem>,
    configure: impl FnOnce(&mut crate::app::PaletteApp),
) -> egui_kittest::Harness<'static, crate::app::PaletteApp> {
    egui_kittest::Harness::builder()
        .with_size(SNAPSHOT_SIZE)
        .with_theme(theme)
        .with_pixels_per_point(1.0)
        .wgpu()
        .build_eframe(move |_cc| {
            snapshot_app_with(
                match theme {
                    egui::Theme::Light => ThemePref::Light,
                    egui::Theme::Dark => ThemePref::Dark,
                },
                payload_items,
                configure,
            )
        })
}

/// 金图 1：空查询暗色（首屏基准态——搜索框 / 空态提示 / 页脚三段）。
///
/// `#[ignore]`：**不进常规三关**。三平台 CI job 的 `cargo test` 无 GPU 保障
/// （linux runner 无 lavapipe 预装时 wgpu 建适配器即 panic），且金图当前为
/// 本机 GPU 生成、WARP 像素一致性待首跑验证——由专用 `snapshot` job 显式
/// `-- --ignored` 运行（ci.yml），本地出图 / 更新见模块文档。
#[test]
#[ignore = "需 GPU/wgpu 适配器，由 CI snapshot job 显式运行（-- --ignored）"]
fn snapshot_empty_query_dark() {
    let mut harness = snapshot_harness(egui::Theme::Dark, fixture_items(), |_| {});
    harness.run();
    harness.snapshot("empty_query_dark");
}

/// 金图 2：空查询亮色（配色回归的第二主题腿；布局与暗色同构，只换 token）。
#[test]
#[ignore = "需 GPU/wgpu 适配器，由 CI snapshot job 显式运行（-- --ignored）"]
fn snapshot_empty_query_light() {
    let mut harness = snapshot_harness(egui::Theme::Light, fixture_items(), |_| {});
    harness.run();
    harness.snapshot("empty_query_light");
}

/// 5 条结果夹具（金图 3 / 6 用）：标题 ASCII、id 互异（重复 widget ID 会在
/// 画布叠加 debug 告警，见模块教训记档）。
fn fixture_results() -> Vec<PanelItem> {
    ["Calculator", "All Apps", "Clock", "Notepad", "Terminal"]
        .into_iter()
        .map(|t| {
            let mut it = PanelItem::new(t);
            it.ext_id = "com.ddrun.apps".to_string();
            it
        })
        .collect()
}

/// 金图 3：有结果 + 选中第三行（5 条结果 + `move_down` ×2）——回归选中
/// 高亮条在列表中段的绘制。选中必须在**首帧后**经 `state_mut` 摆放：
/// payload 在首帧被消费并重置 Root 列表，首帧前摆的会被覆盖（首版踩中）。
#[test]
#[ignore = "需 GPU/wgpu 适配器，由 CI snapshot job 显式运行（-- --ignored）"]
fn snapshot_results_selected() {
    let mut harness = snapshot_harness(egui::Theme::Dark, fixture_results(), |_| {});
    harness.run(); // 首帧：消费 payload，Root 列表落地
    let app = harness.state_mut();
    let root = app.view.stack.root_mut();
    root.list.move_down();
    root.list.move_down();
    harness.run(); // 次帧：中段选中态渲染
    harness.snapshot("results_selected");
}

/// 金图 4：嵌套页（Calculator 页，placeholder = 页标题、列表 3 项、
/// 页脚含 GoBack 键帽）——回归嵌套页 chrome 与 Root 的差异面。
#[test]
#[ignore = "需 GPU/wgpu 适配器，由 CI snapshot job 显式运行（-- --ignored）"]
fn snapshot_nested_page() {
    let mut harness = snapshot_harness(egui::Theme::Dark, fixture_items(), |app| {
        let mut items: Vec<PanelItem> = ["sin(0)", "cos(0)", "2+2"]
            .into_iter()
            .map(PanelItem::new)
            .collect();
        for it in &mut items {
            it.ext_id = "com.ddrun.calc".to_string();
        }
        app.view.stack.push(dd_gui::navigation::PageState::nested(
            "calc",
            "Calculator",
            "com.ddrun.calc",
            items,
        ));
    });
    harness.run();
    harness.snapshot("nested_page");
}

/// 金图 5：设置-外观页（`is_settings` 页 + 默认分类 = Appearance）——
/// 回归设置视图卡片的排版基线（标题行 / pill 组 / 控件列）。
#[test]
#[ignore = "需 GPU/wgpu 适配器，由 CI snapshot job 显式运行（-- --ignored）"]
fn snapshot_settings_appearance() {
    let mut harness = snapshot_harness(egui::Theme::Dark, fixture_items(), |app| {
        let mut page = dd_gui::navigation::PageState::root(Vec::new());
        page.is_settings = true;
        app.view.stack.push(page);
    });
    harness.run();
    harness.snapshot("settings_appearance");
}

/// 金图 6：右键菜单（锚定首行、锚点固定窗口坐标）——回归菜单卡片的
/// 分层 / 行排布 / 分隔线 / 快捷键右对齐。
#[test]
#[ignore = "需 GPU/wgpu 适配器，由 CI snapshot job 显式运行（-- --ignored）"]
fn snapshot_ctx_menu() {
    let mut harness = snapshot_harness(egui::Theme::Dark, fixture_items(), |app| {
        use crate::app::ctx_menu::{CtxAction, CtxEntry, CtxMenuState, CtxRow};
        use crate::text::{CTX_GLYPH_COPY, CTX_GLYPH_FOLDER, CTX_GLYPH_OPEN};
        app.overlay.ctx_menu = Some(CtxMenuState {
            visible_idx: 0,
            item_id: "Calculator".to_string(),
            rows: vec![
                CtxRow::Entry(CtxEntry {
                    glyph: CTX_GLYPH_OPEN,
                    label: "Open".to_string(),
                    shortcut: "Enter",
                    action: CtxAction::Default,
                }),
                CtxRow::Separator,
                CtxRow::Entry(CtxEntry {
                    glyph: CTX_GLYPH_FOLDER,
                    label: "Reveal in folder".to_string(),
                    shortcut: "",
                    action: CtxAction::RevealInFolder {
                        path: r"C:\Windows\System32\calc.exe".to_string(),
                    },
                }),
                CtxRow::Entry(CtxEntry {
                    glyph: CTX_GLYPH_COPY,
                    label: "Copy path".to_string(),
                    shortcut: "",
                    action: CtxAction::CopyText {
                        text: r"C:\Windows\System32\calc.exe".to_string(),
                    },
                }),
            ],
            anchor: egui::pos2(180.0, 118.0),
            focus: 0,
        });
    });
    harness.run();
    harness.snapshot("ctx_menu");
}
