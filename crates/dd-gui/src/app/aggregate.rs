//! 首屏聚合：后台收集线程 + `poll_aggregate` 结果落地。

use crate::app::PaletteApp;
use dd_gui::aggregator;
use dd_gui::aggregator::SourceStatus;
use dd_gui::navigation::PageState;
use dd_gui::state::PanelItem;
use dd_host::cache::FrozenCache;
use dd_host::manifest::LoadedExtension;
use dd_host::process::ExtensionProcess;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Instant;

/// 首屏聚合的后台线程回传内容。
pub struct AggregatePayload {
    pub(crate) items: Vec<PanelItem>,
    pub(crate) sources: Vec<aggregator::SourceSummary>,
    /// 保活进程：`(扩展清单 id, 进程)`（仅 warm；frozen 读桩无进程）。
    pub(crate) processes: Vec<(String, ExtensionProcess)>,
    /// 已扫描扩展（含 manifest frozen/entry），供桩复热 spawn（M3）。
    pub(crate) exts: Vec<LoadedExtension>,
    /// 聚合线程内从"开始 scan"到"完成 collect+flatten"耗时（ms）。
    /// 与 [`PaletteApp::cold`] 的"进程启动→首屏就绪"总耗时对照，便于 A2 瓶颈定位
    /// （implementation.md R2：未达标记录实测与瓶颈，不调目标）。
    pub(crate) agg_ms: u64,
}

/// 后台线程执行首屏收集（不阻塞 UI，A12）：扫描 → 注入搜索引擎配置 →
/// M3 分流（frozen 读桩 / fresh spawn）→ 合并 → 回传。
///
/// `engines_json` = `Settings::search_engines_env()`：spawn `dd-ext-websearch`
/// 前注入其进程环境（可配置搜索引擎，2026-09-05）；回传的 `exts` 已含注入，
/// 桩复热/后续 spawn 沿用同一环境。
pub fn spawn_aggregation(
    tx: mpsc::Sender<AggregatePayload>,
    cache: Option<FrozenCache>,
    engines_json: String,
    disabled: Vec<String>,
    lang: dd_gui::settings::Lang,
) {
    thread::spawn(move || {
        // A2 拆分计时的"数据平面"：从 scan 起到聚合完成止（不含 GUI/字体加载）
        let agg_start = Instant::now();
        // note（来源备注）不再进页脚（用户决策 2026-09-04）：丢弃即可，
        // 异常细节已由 load_extension_sources 内部日志输出。
        let (exts, _note) = aggregator::load_extension_sources();
        // M6 批次 6.3：停用扩展只从**聚合采集**中剔除——payload.exts 必须保留
        // 全集（self.exts 驱动设置页「扩展管理」列表，过滤掉会让已停用扩展从
        // 列表消失、无法再从 UI 启用，真机反馈 2026-09-05）。
        let mut active: Vec<LoadedExtension> = exts
            .iter()
            .filter(|e| !disabled.contains(&e.manifest.id))
            .cloned()
            .collect();
        aggregator::inject_websearch_env(&mut active, &engines_json);
        // 批次 D（2026-09-06）：注入生效语言到各扩展进程环境——扩展侧经
        // `DDRUN_LANG` 选 zh/en 文案。`lang` 已是 FollowSystem 解析后的具体语言
        // （zh_cn / en_us）；扩展解析未知值回落 zh_cn。
        let lang_str = lang.as_str().to_string();
        for ext in &mut active {
            ext.manifest
                .entry
                .env
                .insert("DDRUN_LANG".to_string(), lang_str.clone());
        }
        let result = aggregator::collect_top_level(&active, cache.as_ref());
        let (items, sources) = aggregator::flatten(&result.per_ext, lang);

        // 进程与 `ExtItems::Ready` 一一对应（collect 时按序 push）；Stub（读桩）无进程
        let mut procs = result.processes.into_iter();
        let mut processes = Vec::new();
        for ext in &result.per_ext {
            if ext.is_ready() {
                if let Some(proc) = procs.next() {
                    processes.push((ext.id().to_string(), proc));
                }
            }
        }

        let agg_ms = agg_start.elapsed().as_millis() as u64;

        let _ = tx.send(AggregatePayload {
            items,
            sources,
            processes,
            exts,
            agg_ms,
        });
    });
}

impl PaletteApp {
    // ── 后台结果轮询 ─────────────────────────────────────────

    /// 首屏聚合结果：替换 Root 列表（保留用户已输入的查询）。
    pub(crate) fn poll_aggregate(&mut self) {
        let Some(rx) = &self.aggregate_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(payload) => {
                let query = self.stack.current().list.query().to_owned();
                let mut root = PageState::root(payload.items);
                root.list.set_query(query);
                // 首屏视图（设置项）：默认功能 / 全部——真机反馈 2026-09-04
                root.list.set_empty_view(
                    if self.settings.open_view == dd_gui::settings::OpenView::All {
                        dd_gui::state::EmptyQueryView::All
                    } else {
                        dd_gui::state::EmptyQueryView::WithoutApps
                    },
                );
                *self.stack.root_mut() = root;
                self.sources = payload.sources;
                self.processes = payload.processes;
                self.exts = payload.exts;
                self.aggregating = false;
                self.aggregate_rx = None;
                // M3：cold-start 保活进程计入 LRU（超出容量即驱逐，一般场景不会触发）
                let mut victims = Vec::new();
                for (id, _) in &self.processes {
                    if let Some(v) = self.lru.access(id) {
                        if v != *id {
                            victims.push(v);
                        }
                    }
                }
                for v in victims {
                    self.evict_warm(&v);
                }
                // A6/A2 可观察日志：桩/warm 分流 + 冷启动耗时
                for s in &self.sources {
                    match &s.status {
                        SourceStatus::Stub { commands } => {
                            eprintln!(
                                "[dd-gui] 冷启动：{} 读桩 {} 命令（frozen，未拉起进程 A6）",
                                s.name, commands
                            );
                        }
                        SourceStatus::Warm { commands } => {
                            eprintln!("[dd-gui] 冷启动：{} warm（{} 命令）", s.name, commands);
                        }
                        SourceStatus::Failed { .. } => {}
                    }
                }
                self.cold.mark_first_interactive();
                if let Some(total_ms) = self.cold.duration_ms() {
                    // A2 拆分日志：total = 进程启动→首屏就绪；agg = 数据平面（scan+collect+flatten）。
                    // 差额 = GUI 初始化 + wgpu + 字体加载（msyh.ttc 数十 MB），R2 要求记录瓶颈而非调目标。
                    let gui_init_ms = total_ms.saturating_sub(payload.agg_ms);
                    eprintln!(
                        "[dd-gui] 冷启动完成：{total_ms} ms（A2 目标 <200ms：数据就绪 {} ms + GUI 初始化/字体加载 ~{gui_init_ms} ms，记录实测不调目标）",
                        payload.agg_ms
                    );
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.aggregating = false;
                self.aggregate_rx = None;
            }
        }
    }
}
