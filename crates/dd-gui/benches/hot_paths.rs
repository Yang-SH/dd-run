//! §4.5（dd-run-改进分析.md）：首屏热路径 bench——模糊匹配 / 查询重算 / flatten 合并。
//! 清单 §2.4（2026-10-29）：列表虚拟化判定腿——`panel_draw` / `panel_draw_gpu` 两组。
//!
//! 四个逻辑面对应分析文档验收「3 个 bench 有基线数据」+ 清单 §2.4 判定腿：
//! - `fuzzy_score_2000`：[`FuzzyMatcher::score`] 一次击键的纯打分成本（2000 项 × 8 字符查询）；
//! - `set_query_2000`：[`PanelState::set_query`] 完整击键路径（构建 matcher + 逐项打分 +
//!   稳定排序重建可见表）——`recompute_visible` 是私有方法，公共入口多个
//!   （`set_query` / `set_empty_view` / `set_apps_hidden` / `set_steam_hidden` / `reset`），
//!   bench 取用户击键驱动的 `set_query`；
//! - `flatten_5ext`：[`dd_gui::aggregator::flatten`] 合并 5 扩展 × 100 命令（≈ 真实上限量级）；
//! - `panel_draw`（kittest 默认 = 软件适配器，与快照管线同）、
//!   `panel_draw_gpu`（硬件适配器 = 生产 glow+GPU 的等价 wgpu 管线）与
//!   `panel_draw_layout`（零渲染分解组 = 虚拟化可节省成本的上界）：
//!   `ui/panel.rs` 单帧绘制（430 行夹具）——判定取**硬件组**数字并以
//!   布局组分解归因，≤ 8 ms/帧则虚拟化永不立项（清单 §2.4；
//!   软件组为 kittest 管线上界参照）。
//!
//! 基线比对（CI bench job / 本机）：`node tools/bench_compare.mjs <criterion目录> base head`。

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use dd_gui::aggregator::{flatten, ExtItems};
use dd_gui::app::{AggregateEvent, PaletteApp};
use dd_gui::fuzzy::FuzzyMatcher;
use dd_gui::hotkey::HotkeyThread;
use dd_gui::settings::{Lang, Settings, ThemePref};
use dd_gui::state::{PanelItem, PanelState};
use dd_gui::tray::TrayEvent;
use dd_host::cache::ColdStartTimer;
use dd_protocol::model::{CommandItem, CommandRef};
use eframe::egui;
use std::sync::mpsc;
use std::sync::{atomic::AtomicBool, Arc};

/// 2000 项测试集：混合英文标题 / CJK 标题（走拼音索引路径）/ 副标题 / 标签，
/// 形态贴近真实聚合结果（应用路径副标题、网页 URL 副标题、分组名、tag chip）。
fn bench_items(n: usize) -> Vec<PanelItem> {
    (0..n)
        .map(|i| {
            let cjk = i % 10 == 0;
            let title = if cjk {
                format!("计算器 {i}")
            } else {
                format!("Application {:04}", i)
            };
            let subtitle = if i % 3 == 0 {
                format!("C:\\Program Files\\App{i}\\app{i}.exe")
            } else if i % 3 == 1 {
                format!("https://example.com/app/{i}")
            } else {
                String::new()
            };
            PanelItem {
                id: format!("item-{i}"),
                ext_id: format!("ext.{}", i % 5),
                title,
                subtitle,
                section: format!("Section {}", i % 7),
                icon: None,
                tags: if i % 4 == 0 {
                    vec!["steam".into(), format!("tag{i}")]
                } else {
                    Vec::new()
                },
                result_category: Some("应用".into()),
                pinyin: if cjk {
                    format!("jisuanqi{i} jsq{i}")
                } else {
                    String::new()
                },
                command: CommandRef::Invoke,
                more_commands: Vec::new(),
            }
        })
        .collect()
}

/// 5 扩展 × 100 命令（Root 列表真实上限量级：400 应用 + 30 文件结果）。
fn bench_per_ext() -> Vec<ExtItems> {
    (0..5)
        .map(|e| ExtItems::Ready {
            id: format!("ext.{e}"),
            name: format!("Extension {e}"),
            items: (0..100)
                .map(|i| CommandItem {
                    id: format!("cmd-{e}-{i}"),
                    title: if i % 5 == 0 {
                        format!("设置 {e} {i}")
                    } else {
                        format!("Command {e} {i:03}")
                    },
                    subtitle: Some(format!("subtitle {e}-{i}")),
                    icon: None,
                    section: Some(format!("Section {}", i % 7)),
                    tags: None,
                    details: None,
                    text_to_suggest: None,
                    more_commands: None,
                    command: CommandRef::Invoke,
                })
                .collect(),
        })
        .collect()
}

fn bench_fuzzy_score(c: &mut Criterion) {
    let items = bench_items(2000);
    let query = "settings";
    c.bench_function("fuzzy_score_2000", |b| {
        b.iter(|| {
            let mut fm = FuzzyMatcher::new(query);
            let mut hits = 0usize;
            for item in &items {
                hits += fm.score(item).is_some() as usize;
            }
            black_box(hits)
        })
    });
}

fn bench_set_query(c: &mut Criterion) {
    let items = bench_items(2000);
    let mut state = PanelState::new(items);
    // 两个真实查询交替：`set_query` 对同查询不重算，交替保证每次迭代都走
    // 完整 recompute_visible（matcher 构建 + 打分 + 稳定排序）。
    let queries = ["settings", "settingsx"];
    let mut turn = 0usize;
    c.bench_function("set_query_2000", |b| {
        b.iter(|| {
            let q = queries[turn % 2];
            turn += 1;
            black_box(state.set_query(q))
        })
    });
}

fn bench_flatten(c: &mut Criterion) {
    let per_ext = bench_per_ext();
    c.bench_function("flatten_5ext", |b| {
        b.iter(|| {
            let (items, sources) = flatten(black_box(&per_ext), Lang::EnUs);
            black_box((items.len(), sources.len()))
        })
    });
}

// ── panel_draw（清单 §2.4 虚拟化判定腿，2026-10-29）────────────────────────────

/// Root 列表夹具（清单 §2.4 口径）：默认规模 = **400 应用 + 30 文件结果**。
/// 标题/副标题 ASCII——bench 直构 `PaletteApp` 不经 `setup_cjk_fonts`，内置字体
/// 无 CJK（与快照夹具同口径，中文渲染为方块无布局增益）；icon 全 `None` 走
/// glyph 占位路径；`section` 分 7 组引入组头行；文件行带 `files` 机器标签。
/// 判定腿的分解组共用同一生成器（400 应用 + 30 文件，仅传参区分规模）。
fn panel_fixture(n_apps: usize, n_files: usize) -> Vec<PanelItem> {
    let mut items = Vec::with_capacity(n_apps + n_files);
    for i in 0..n_apps {
        items.push(PanelItem {
            id: format!("bench-app-{i:03}"),
            ext_id: "com.ddrun.apps".to_string(),
            title: format!("Application {i:03}"),
            subtitle: if i % 2 == 0 {
                format!(r"C:\Program Files\App{i}\app{i}.exe")
            } else {
                format!("https://example.com/app/{i}")
            },
            section: format!("Group {}", i % 7),
            icon: None,
            tags: if i % 4 == 0 {
                vec!["steam".into()]
            } else {
                Vec::new()
            },
            result_category: Some("Apps".into()),
            pinyin: String::new(),
            command: CommandRef::Invoke,
            more_commands: Vec::new(),
        });
    }
    for i in 0..n_files {
        items.push(PanelItem {
            id: format!("bench-file-{i:02}"),
            ext_id: "com.ddrun.filesearch".to_string(),
            title: format!("document_{i:02}.txt"),
            subtitle: format!(
                r"C:\Users\demo\Documents\folder{}\document_{i:02}.txt",
                i % 5
            ),
            section: String::new(),
            icon: None,
            tags: vec!["files".into()],
            result_category: Some("Files".into()),
            pinyin: String::new(),
            command: CommandRef::Invoke,
            more_commands: Vec::new(),
        });
    }
    items
}

/// 极简 logger（`log` 已是 dd-gui 直接依赖，零新增）：把 wgpu 初始化日志转
/// stderr——判定口径要求台账记明实际适配器。Info 级放行（软件适配器命中
/// egui-wgpu 的 WARN 行），`egui_wgpu` 目标的 Debug 行也放行（硬件适配器的
/// 选择结果只落 Debug）。criterion 自身不装 logger，全进程仅装这一次。
struct AdapterLogger;
const ADAPTER_LOGGER: AdapterLogger = AdapterLogger;
impl log::Log for AdapterLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
            || (m.level() == log::Level::Debug && m.target().starts_with("egui_wgpu"))
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            eprintln!("[bench] {} {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}

fn init_adapter_logger() {
    let _ = log::set_boxed_logger(Box::new(ADAPTER_LOGGER));
    log::set_max_level(log::LevelFilter::Info);
}

/// 构建判定 harness（范式复用 `src/snapshot_tests.rs` 的 `snapshot_harness`——
/// 同尺寸 650×440 / 主题暗色 / ppp 1.0 / 关动画；预发完整聚合 payload 使首帧
/// 即「首屏就绪」态）。`hw` 选适配器策略：
/// - `Some(false)` = kittest 默认：其 `default_wgpu_setup` **显式优先 CPU 软件
///   适配器**（快照确定性设计，源码自注 `"CPU is the best for our purposes"`）
///   ——本机即 WARP；
/// - `Some(true)` = 硬件管线：不设选择器，`power_preference` 走 wgpu 默认序
///   （独显 > 集显 > CPU）——生产 glow+GPU 路径的等价 wgpu 管线，判定取此组；
/// - `None` = 不建渲染器（kittest LazyRenderer 桩）：配 `render = false` 供
///   布局分解组隔离「update + 逐行 widget + shape 生成」成本（虚拟化
///   `show_rows` 可节省的正是这一段）。
fn build_panel_harness(
    hw: Option<bool>,
    render: bool,
    items: Vec<PanelItem>,
) -> egui_kittest::Harness<'static, PaletteApp> {
    let (_ttx, tray_rx) = mpsc::channel::<TrayEvent>();
    let (atx, agg_rx) = mpsc::channel::<AggregateEvent>();
    // 预发完整聚合结果：首帧被 poll_aggregate 消费并重置 Root 列表
    // （快照夹具同语义）；sender 随即断开，不再有重建事件
    // （results_selected 时序实证：断开不擦除已落地列表）。
    let _ = atx.send(AggregateEvent::full_fixture(items));
    drop(atx);
    let settings = Settings {
        theme: ThemePref::Dark,
        ui_animations: false,
        ..Settings::default()
    };
    let mut app = PaletteApp::new(
        HotkeyThread::dummy(),
        tray_rx,
        Arc::new(AtomicBool::new(false)),
        agg_rx,
        ColdStartTimer::new(),
        None,
        settings,
        Arc::new(AtomicBool::new(false)),
    );
    // 构造点 visible = false（等热键唤起）——判定要画首屏，显式置真
    // （字段 pub(crate)，经 doc-hidden 钩子等价于快照的直赋值）。
    app.set_window_visible(true);
    let builder = egui_kittest::Harness::builder()
        .with_size(egui::vec2(650.0, 440.0))
        .with_theme(egui::Theme::Dark)
        .with_pixels_per_point(1.0)
        // step() 默认只走布局 + shape 生成；完整帧口径再叠加渲染
        // （tessellation + 绘制 + 回读，与快照管线一致）。
        .with_render_every_step(render);
    let builder = match hw {
        Some(true) => {
            // 绕开软件优先选择器：显式 setup 不设 `native_adapter_selector`。
            let setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
            builder.renderer(egui_kittest::wgpu::WgpuTestRenderer::from_setup(
                egui_wgpu::WgpuSetup::CreateNew(setup),
            ))
        }
        Some(false) => builder.wgpu(),
        None => builder,
    };
    builder.build_eframe(move |_cc| app)
}

/// 判定组公共骨架：建 harness（无适配器环境 panic → `catch_unwind` 截住后
/// 仅提示跳过，不拖垮同 binary 其余组——基线缺失时 `bench_compare.mjs` 按
/// 「单侧缺失不判红」处理）→ 首帧消费 payload 落地 Root 列表 → 单帧步进。
fn bench_panel_frame(
    c: &mut Criterion,
    id: &str,
    hw: Option<bool>,
    render: bool,
    rows: (usize, usize),
) {
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build_panel_harness(hw, render, panel_fixture(rows.0, rows.1))
    }));
    let mut harness = match built {
        Ok(h) => h,
        Err(_) => {
            eprintln!("{id}: skipped（无 wgpu 适配器；bench_compare 按单侧缺失不判红）");
            return;
        }
    };
    harness.run(); // 首帧：消费 payload，Root 列表落地（含动画关闭的静态态）
    c.bench_function(id, |b| b.iter(|| harness.step()));
}

/// 软件适配器组（kittest 默认 = 本机 WARP / CI lavapipe）：快照管线同源数字，
/// 作为 `panel_draw_gpu` 的上界参照记档——软件光栅化主导，不代表生产管线。
fn bench_panel_draw(c: &mut Criterion) {
    init_adapter_logger();
    bench_panel_frame(c, "panel_draw", Some(false), true, (400, 30));
}

/// 硬件适配器组（**判定组**，清单 §2.4 口径「本机数字」的实际承载体）：
/// 布局 + 逐行 widget + tessellation + GPU 绘制 + 回读（回读为 kittest
/// 截图管线固有开销，生产 glow 路径无此项——数字含此保守余量）。
fn bench_panel_draw_gpu(c: &mut Criterion) {
    init_adapter_logger();
    bench_panel_frame(c, "panel_draw_gpu", Some(true), true, (400, 30));
}

/// 布局分解组（判定辅助）：不渲染，隔离 `update + 逐行 widget + shape 生成`
/// ——恰为虚拟化 `show_rows` 能节省的成本上界；与 `panel_draw_gpu` 之差 =
/// tessellation + GPU 绘制 + 回读（虚拟化救不了的固定段）。本组零 GPU 依赖，
/// 在任何 CI runner 上都可跑，是 panel 侧最稳的趋势回归锚。
fn bench_panel_draw_layout(c: &mut Criterion) {
    bench_panel_frame(c, "panel_draw_layout", None, false, (400, 30));
}

criterion_group!(
    hot_paths,
    bench_fuzzy_score,
    bench_set_query,
    bench_flatten,
    bench_panel_draw,
    bench_panel_draw_gpu,
    bench_panel_draw_layout
);
criterion_main!(hot_paths);
