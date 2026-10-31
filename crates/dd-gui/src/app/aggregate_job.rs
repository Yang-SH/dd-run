//! 首屏聚合：后台收集线程 + `poll_aggregate` 事件落地。
//!
//! C2（dd-run-改进分析.md §4.1）：聚合从「全量等待」改为**三段式流**——
//! `Meta`（扫描期即得：清单 / 信任 / 规格表）→ `Ext`×N（每扩展**完成即发**，
//! 完成序，快扩展不被慢扩展扣住）→ `Done`（收尾计时）。UI 每收到 `Ext`
//! 就以「已到达集合」重跑一次纯函数 `flatten`（扩展数个位数，微秒级），
//! 首项数据落地即可交互——列表加载骨架只在**零项期**展示。
//!
//! `AggregateEvent::Full`：一次性整包（旧 `AggregatePayload` 语义原样保留），
//! **仅测试夹具使用**（快照金图直构列表，不经 flatten），生产链路恒走
//! Meta → Ext×N → Done。

use crate::aggregator;
use crate::aggregator::{ExtItems, SourceStatus};
use crate::app::PaletteApp;
use crate::ext_client::ExtClient;
use crate::navigation::PageState;
use dd_ext::ExtensionSpec;
use dd_host::cache::FrozenCache;
use dd_host::manifest::LoadedExtension;
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Instant;

/// 首屏聚合的后台线程回传事件（三段式流，见模块文档）。
pub enum AggregateEvent {
    /// ① 扫描阶段即得的全集元数据（最先发）：设置页扩展列表与信任表立即可用。
    Meta(AggregateMeta),
    /// ② 单个扩展结果到达（完成序）：UI 并入已到达集合并重建 Root 列表。
    Ext(ExtArrival),
    /// ③ 收尾：停加载态 / LRU 计入 / A2 计时日志。
    Done { agg_ms: u64 },
    /// 一次性整包（**仅测试夹具**，快照金图直构列表不经 flatten；语义与
    /// C2 前的 `poll_aggregate` 单 payload 落地逐字一致）。
    Full(AggregatePayload),
}

/// `Meta` 段：扫描期即得、与逐扩展拉取无关的全集数据。
pub struct AggregateMeta {
    /// 已扫描扩展（含 manifest frozen/entry），供桩复热 spawn（M3）。
    pub(crate) exts: Vec<LoadedExtension>,
    /// S-05 信任判定表（`id → 判定`）：设置页审批 UI 与面板页脚提示的数据源。
    pub(crate) trust: HashMap<String, dd_host::trust::Assessment>,
    /// S-05 信任台账读取状态（`Corrupt` → 设置页提示）。
    pub(crate) ledger_state: dd_host::trust::LedgerState,
    /// R-20：解析失败的清单（路径, 原因）——扩展卡顶部警告行的数据源。
    pub(crate) skipped: Vec<(String, String)>,
    /// M9：内置 in-process 规格表（`id → ExtensionSpec`），供复热链路重建内置客户端。
    pub(crate) inproc_specs: HashMap<String, ExtensionSpec>,
}

/// `Ext` 段：单个扩展的到达结果（`Ready` 携带保活客户端；`Stub`/`Failed` 无进程）。
pub struct ExtArrival {
    pub(crate) item: ExtItems,
    pub(crate) proc: Option<ExtClient>,
}

/// 一次性整包回传内容（`AggregateEvent::Full`，仅测试夹具）。
pub struct AggregatePayload {
    pub(crate) items: Vec<crate::state::PanelItem>,
    pub(crate) sources: Vec<aggregator::SourceSummary>,
    /// 保活客户端：`(扩展清单 id, 客户端)`（warm；frozen 读桩无客户端；M9 含内置 in-process）。
    pub(crate) processes: Vec<(String, ExtClient)>,
    /// 已扫描扩展（含 manifest frozen/entry），供桩复热 spawn（M3）。
    pub(crate) exts: Vec<LoadedExtension>,
    /// S-05 信任判定表（`id → 判定`）：设置页审批 UI 与面板页脚提示的数据源。
    pub(crate) trust: HashMap<String, dd_host::trust::Assessment>,
    /// S-05 信任台账读取状态（`Corrupt` → 设置页提示）。
    pub(crate) ledger_state: dd_host::trust::LedgerState,
    /// R-20：解析失败的清单（路径, 原因）——扩展卡顶部警告行的数据源。
    pub(crate) skipped: Vec<(String, String)>,
    /// M9：内置 in-process 规格表（`id → ExtensionSpec`），供复热链路重建内置客户端。
    pub(crate) inproc_specs: HashMap<String, ExtensionSpec>,
}

impl AggregateEvent {
    /// bench / 外部测试钩子：构造「聚合已完成」整包（`Full`）。[`AggregatePayload`]
    /// 字段全为 `pub(crate)`，`benches/` 独立 crate 无法字面构造——本钩子为等价
    /// 入口（语义同快照夹具 `snapshot_app_with` 的预发 payload：`items` 即首帧
    /// 落地后的 Root 列表）。P2-4.5 `FuzzyMatcher` 放宽 `pub` 同款偏差记档；
    /// `#[doc(hidden)]` 不进公开文档面，产品代码不调用。
    #[doc(hidden)]
    pub fn full_fixture(items: Vec<crate::state::PanelItem>) -> Self {
        Self::Full(AggregatePayload {
            items,
            sources: Vec::new(),
            processes: Vec::new(),
            exts: Vec::new(),
            trust: Default::default(),
            ledger_state: dd_host::trust::LedgerState::Ok,
            skipped: Vec::new(),
            inproc_specs: Default::default(),
        })
    }
}

/// 后台线程执行首屏收集（不阻塞 UI，A12）：扫描 → 注入搜索引擎配置 →
/// M3 分流（frozen 读桩 / fresh spawn）→ **完成序逐扩展回传** → 收尾。
///
/// `engines_json` = `Settings::search_engines_env()`：spawn `dd-ext-websearch`
/// 前注入其进程环境（可配置搜索引擎，2026-09-05）；回传的 `exts` 已含注入，
/// 桩复热/后续 spawn 沿用同一环境。自定义直达命令（N1）自 C2 起由 UI 侧
/// 在每次列表重建时追加（`rebuild_root_from_arrived`），不再经聚合线程。
pub fn spawn_aggregation(
    tx: mpsc::Sender<AggregateEvent>,
    cache: Option<FrozenCache>,
    engines_json: String,
    disabled: Vec<String>,
    lang: crate::settings::Lang,
    ext_settings: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
) {
    thread::spawn(move || {
        // A2 拆分计时的"数据平面"：从 scan 起到聚合完成止（不含 GUI/字体加载）
        let agg_start = Instant::now();
        // M9：in-process 内置共享宿主进程，其文案取 `dd_ext::i18n` 的进程级生效
        // 语言——须在构造内置规格（`builtin_specs()` 内 `tr()`）**之前**设好，
        // 否则内置文案会停留在首次读到的语言（设置页切换语言后需重设）。
        dd_ext::i18n::set_lang(match lang {
            crate::settings::Lang::EnUs => dd_ext::i18n::Lang::EnUs,
            _ => dd_ext::i18n::Lang::ZhCn,
        });
        // note（来源备注）不再进页脚（用户决策 2026-09-04）：丢弃即可，
        // 异常细节已由 load_extension_sources 内部日志输出。
        // R-20：skipped（解析失败清单）改走扩展卡警告行（页脚 note 语义不变）。
        let aggregator::ExtensionSources {
            exts,
            inproc_specs,
            note: _note,
            skipped,
            trust,
            ledger_state,
        } = aggregator::load_extension_sources(lang);
        // ① Meta：扫描期即得——设置页扩展列表与信任表先于任何进程拉取就绪。
        let _ = tx.send(AggregateEvent::Meta(AggregateMeta {
            exts: exts.clone(),
            trust: trust.clone(),
            ledger_state,
            skipped: skipped.clone(),
            inproc_specs: inproc_specs.clone(),
        }));
        // M6 批次 6.3：停用扩展只从**聚合采集**中剔除——payload.exts 必须保留
        // 全集（self.pool.exts 驱动设置页「扩展管理」列表，过滤掉会让已停用扩展从
        // 列表消失、无法再从 UI 启用，真机反馈 2026-09-05）。
        //
        // S-05（2026-09-24）：同手法再加一道**信任门禁**——未获信任（待批准 /
        // 已阻止）的扩展同样只从采集集剔除，`exts` 仍保留以便设置页展示与批准。
        // 第二道门在 `spawn_and_initialize_with_info`（spawn 唯一入口，覆盖桩复热）。
        let mut active: Vec<LoadedExtension> = exts
            .iter()
            .filter(|e| !disabled.contains(&e.manifest.id))
            .filter(|e| aggregator::is_trusted(&trust, &e.manifest.id))
            .cloned()
            .collect();
        aggregator::inject_websearch_env(&mut active, &engines_json);
        // N2（2026-10-04）：用户可调扩展配置通用注入（entry.env 内存副本，
        // 变量名 `DD_EXT_CFG_<KEY>`；S-10 键名守卫见 inject_ext_settings）。
        aggregator::inject_ext_settings(&mut active, &ext_settings);
        // M9 内置 websearch 为 in-process（无子进程，进程环境变量对它无效）：
        // 引擎配置经内存通道直接注入扩展（2026-09-12 修复——此前配置被静默
        // 忽略、面板永远显示内置全表）。独立 exe / spawn 路径仍走 entry.env。
        dd_ext::builtins::websearch::set_configured_engines_json(Some(engines_json.clone()));
        // N2：内置 apps 同为 in-process——用户屏蔽名单经内存通道直接注入
        //（websearch 同款先例；`None` = 未配置/已清空，扩展侧零过滤）。
        dd_ext::builtins::apps::set_configured_blocklist(
            ext_settings
                .get("com.ddrun.apps")
                .and_then(|m| m.get("blocklist"))
                .cloned(),
        );
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
        // ② Ext×N：完成序逐个回传（快扩展先到先渲染——C2 的核心）。
        aggregator::collect_top_level_streaming(
            &active,
            &inproc_specs,
            cache.as_ref(),
            |item, proc| {
                let _ = tx.send(AggregateEvent::Ext(ExtArrival { item, proc }));
            },
        );
        // ③ Done：收尾计时（agg_ms 语义不变：scan 起 → 全部完成止）。
        let agg_ms = agg_start.elapsed().as_millis() as u64;
        let _ = tx.send(AggregateEvent::Done { agg_ms });
    });
}

impl PaletteApp {
    // ── 后台结果轮询 ─────────────────────────────────────────

    /// 首屏聚合事件：逐条落地（一帧内可到多条——先收齐再应用，避免借用交叉）。
    pub(crate) fn poll_aggregate(&mut self) {
        let Some(rx) = &self.pool.aggregate_rx else {
            return;
        };
        let mut events = Vec::new();
        let disconnected = loop {
            match rx.try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Empty) => break false,
                Err(TryRecvError::Disconnected) => break true,
            }
        };
        for event in events {
            match event {
                AggregateEvent::Meta(meta) => self.apply_aggregate_meta(meta),
                AggregateEvent::Ext(arrival) => self.apply_aggregate_ext(arrival),
                AggregateEvent::Done { agg_ms } => self.finish_aggregation(agg_ms),
                AggregateEvent::Full(payload) => self.apply_aggregate_full(payload),
            }
        }
        if disconnected {
            // 线程异常终止兜底（无 Done）：至少停掉加载态，别让面板永久转圈。
            self.pool.aggregating = false;
            self.pool.aggregate_rx = None;
        }
    }

    /// ① Meta：全集元数据落地（设置页扩展列表 / 信任表 / 规格表即刻可用）。
    fn apply_aggregate_meta(&mut self, meta: AggregateMeta) {
        self.pool.arrived.clear(); // 新一轮开始（旧轮结果由 Root 列表持有）
        self.pool.exts = meta.exts;
        self.pool.trust = meta.trust;
        self.pool.ledger_state = meta.ledger_state;
        self.pool.skipped_manifests = meta.skipped;
        self.pool.inproc_specs = meta.inproc_specs;
        // S-05：首次出现「待批准」时给**一次** toast（不打断、不重复）。
        // 为何需要：面板页脚只在"无选中项"时显示该提示，而实际使用时通常
        // 总有选中项；设置页卡片头是"处理现场"，但用户未必会打开设置页。
        if !self.pool.pending_notified {
            let n = aggregator::pending_count(&self.pool.trust);
            if n > 0 {
                self.pool.pending_notified = true;
                let msg = self.tr("toast.ext_pending").replace("{n}", &n.to_string());
                self.show_toast(msg, Some(6_000));
            }
        }
    }

    /// ② Ext：并入已到达集合并**立即重建 Root 列表**（首项落地即可交互）。
    fn apply_aggregate_ext(&mut self, arrival: ExtArrival) {
        let first = self.pool.arrived.is_empty();
        if first {
            // 本轮首个结果：作废旧轮进程（等价旧「payload 整体替换」，提前到
            // 首个结果到达——零项期之前旧列表/旧进程照常服务，无空窗）。
            self.pool.processes.clear();
        }
        if let Some(proc) = arrival.proc {
            self.pool
                .processes
                .push((arrival.item.id().to_string(), proc));
        }
        self.pool.arrived.push(arrival.item);
        self.rebuild_root_from_arrived();
        // A2「数据就绪」= 首个结果可交互的时刻（幂等；无扩展到达时 Done 兜底）
        if first {
            self.pool.cold.mark_first_interactive();
        }
    }

    /// 以「已到达集合」重跑纯函数 flatten 重建 Root 列表（C2：不做增量合并——
    /// 扩展数个位数，全量重算微秒级且排序天然稳定）。选中项按 (ext_id, id)
    /// 身份恢复，列表增长不打断键盘/鼠标选中（验收「增量追加、选中不跳」）。
    fn rebuild_root_from_arrived(&mut self) {
        let lang = self.view.lang_effective;
        let (mut items, sources) = aggregator::flatten(&self.pool.arrived, lang);
        // N1（2026-10-04）：自定义直达命令追加为宿主虚拟条目（不经扩展，无信任面）。
        items.extend(aggregator::custom_command_items(
            &self.draft.settings.custom_commands,
            lang,
        ));
        self.pool.sources = sources;

        let query = self.view.stack.current().list.query().to_owned();
        let selected = self
            .view
            .stack
            .current()
            .list
            .selected_item()
            .map(|it| (it.ext_id.clone(), it.id.clone()));
        let mut root = PageState::root(items);
        root.list.set_query(query);
        // 首屏视图（设置项）：默认功能 / 全部——真机反馈 2026-09-04
        root.list.set_empty_view(
            if self.draft.settings.open_view == crate::settings::OpenView::All {
                crate::state::EmptyQueryView::All
            } else {
                crate::state::EmptyQueryView::WithoutApps
            },
        );
        // 「搜索应用」关（2026-09-12）：根页排除「应用」类项
        //（空查询首屏与关键词匹配均排除；默认开 = 不过滤）
        root.list.set_apps_hidden(!self.draft.settings.search_apps);
        // 「搜索 Steam 游戏」关（2026-10-07）：根页排除 Steam 游戏项
        //（`steam` 机器标签判据；默认关 = 隐藏）
        root.list
            .set_steam_hidden(!self.draft.settings.search_steam_games);
        if let Some((ext_id, id)) = selected {
            root.list.select_by_identity(&ext_id, &id);
        }
        *self.view.stack.root_mut() = root;
        // §5.1：Root 列表整包替换会丢排序快照，重建后重新注入（空表 = None no-op）。
        self.sync_usage_index();
    }

    /// ③ Done：收尾——停加载态、LRU 计入、A2 计时日志。
    fn finish_aggregation(&mut self, agg_ms: u64) {
        self.pool.aggregating = false;
        self.pool.aggregate_rx = None;
        self.pool.arrived.clear(); // Root 列表已持有全量项，释放双份内存
                                   // M3：cold-start 保活进程计入 LRU（超出容量即驱逐，一般场景不会触发）
        let mut victims = Vec::new();
        for (id, _) in &self.pool.processes {
            if let Some(v) = self.pool.lru.access(id) {
                if v != *id {
                    victims.push(v);
                }
            }
        }
        for v in victims {
            self.evict_warm(&v);
        }
        // A6/A2 可观察日志：桩/warm 分流 + 冷启动耗时
        for s in &self.pool.sources {
            match &s.status {
                SourceStatus::Stub { commands } => {
                    log::debug!(
                        "[dd-gui] 冷启动：{} 读桩 {} 命令（frozen，未拉起进程 A6）",
                        s.name,
                        commands
                    );
                }
                SourceStatus::Warm { commands } => {
                    log::info!("[dd-gui] 冷启动：{} warm（{} 命令）", s.name, commands);
                }
                SourceStatus::Failed { .. } => {}
            }
        }
        self.pool.cold.mark_first_interactive();
        if let Some(total_ms) = self.pool.cold.duration_ms() {
            // A2 拆分日志：total = 进程启动→首屏就绪；agg = 数据平面（scan+collect+flatten）。
            // 差额 = GUI 初始化 + wgpu + 字体加载（msyh.ttc 数十 MB），R2 要求记录瓶颈而非调目标。
            let gui_init_ms = total_ms.saturating_sub(agg_ms);
            log::debug!(
                "[dd-gui] 冷启动完成：{total_ms} ms（A2 目标 <200ms：数据就绪 {} ms + GUI 初始化/字体加载 ~{gui_init_ms} ms，记录实测不调目标）",
                agg_ms
            );
        }
    }

    /// 测试夹具专用（`AggregateEvent::Full`）：一次性整包落地，语义与 C2 前
    /// 的 `poll_aggregate` 单 payload 分支逐字一致（快照金图不走 flatten）。
    fn apply_aggregate_full(&mut self, payload: AggregatePayload) {
        let query = self.view.stack.current().list.query().to_owned();
        let mut root = PageState::root(payload.items);
        root.list.set_query(query);
        root.list.set_empty_view(
            if self.draft.settings.open_view == crate::settings::OpenView::All {
                crate::state::EmptyQueryView::All
            } else {
                crate::state::EmptyQueryView::WithoutApps
            },
        );
        root.list.set_apps_hidden(!self.draft.settings.search_apps);
        root.list
            .set_steam_hidden(!self.draft.settings.search_steam_games);
        *self.view.stack.root_mut() = root;
        // §5.1：同上——整包替换后重新注入排序快照。
        self.sync_usage_index();
        self.pool.sources = payload.sources;
        self.pool.processes = payload.processes;
        self.pool.exts = payload.exts;
        self.pool.trust = payload.trust;
        self.pool.ledger_state = payload.ledger_state;
        self.pool.skipped_manifests = payload.skipped;
        self.pool.inproc_specs = payload.inproc_specs;
        self.pool.aggregating = false;
        self.pool.aggregate_rx = None;
        let mut victims = Vec::new();
        for (id, _) in &self.pool.processes {
            if let Some(v) = self.pool.lru.access(id) {
                if v != *id {
                    victims.push(v);
                }
            }
        }
        for v in victims {
            self.evict_warm(&v);
        }
        self.pool.cold.mark_first_interactive();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::make_app;
    use dd_protocol::model::{CommandItem, CommandRef};

    fn ext_items(id: &str, titles: &[&str]) -> ExtItems {
        ExtItems::Stub {
            id: id.to_string(),
            name: id.to_string(),
            items: titles
                .iter()
                .map(|t| CommandItem {
                    id: format!("{id}.{t}"),
                    title: (*t).to_string(),
                    subtitle: None,
                    icon: None,
                    section: None,
                    tags: None,
                    details: None,
                    text_to_suggest: None,
                    more_commands: None,
                    command: CommandRef::Invoke,
                })
                .collect(),
        }
    }

    fn arrival(id: &str, titles: &[&str]) -> ExtArrival {
        ExtArrival {
            item: ext_items(id, titles),
            proc: None,
        }
    }

    fn empty_meta() -> AggregateMeta {
        AggregateMeta {
            exts: vec![],
            trust: HashMap::new(),
            ledger_state: dd_host::trust::LedgerState::Ok,
            skipped: vec![],
            inproc_specs: HashMap::new(),
        }
    }

    /// C2 验收口径（结构化）：快扩展到达即进列表——聚合未完成（aggregating
    /// 仍真、Done 未到）时 Root 列表已有可交互项；Done 后列表含全部扩展项。
    #[test]
    fn streaming_first_arrival_renders_before_done() {
        let mut app = make_app();
        app.pool.aggregate_rx = None; // 直接驱动事件方法（不走通道）
        app.pool.aggregating = true;

        app.apply_aggregate_meta(empty_meta());
        assert!(app.pool.aggregating, "Meta 段不结束加载态");
        assert!(
            app.view.stack.root().list.visible_count() == 0,
            "零项期列表为空（绘制层展示加载骨架）"
        );

        app.apply_aggregate_ext(arrival("fast", &["A1"]));
        assert!(
            app.view.stack.root().list.visible_count() > 0,
            "首个结果到达即重建列表（不等慢扩展/Done）"
        );
        assert!(
            app.pool.aggregating,
            "Done 前加载态保持（fallback 门禁语义不变）"
        );

        app.apply_aggregate_ext(arrival("slow", &["B1", "B2"]));
        let n = app.view.stack.root().list.visible_count();
        assert_eq!(n, 3, "后续到达并入（全量重算 flatten），实际 {n}");

        app.finish_aggregation(42);
        assert!(!app.pool.aggregating, "Done 停加载态");
        assert!(
            app.pool.arrived.is_empty(),
            "Done 后释放已到达集（防双份内存）"
        );
        assert_eq!(
            app.view.stack.root().list.visible_count(),
            3,
            "列表保持全量"
        );
    }

    /// C2 验收口径：慢扩展到达后列表**增量追加而非重排**——已选中项按
    /// (ext_id, id) 身份恢复，选中不跳。
    #[test]
    fn streaming_arrival_preserves_selection_by_identity() {
        let mut app = make_app();
        app.pool.aggregate_rx = None;
        app.apply_aggregate_meta(empty_meta());
        app.apply_aggregate_ext(arrival("fast", &["A1", "A2", "A3"]));
        let root = app.view.stack.root_mut();
        root.list.move_down();
        root.list.move_down(); // 选中 A3（可见序 2）
        let selected_before = app
            .view
            .stack
            .root()
            .list
            .selected_item()
            .map(|it| (it.ext_id.clone(), it.id.clone()))
            .expect("已有选中");
        assert_eq!(selected_before.1, "fast.A3");

        // 慢扩展到达：即使其项排到前面，选中身份不丢
        app.apply_aggregate_ext(arrival("slow", &["B1"]));
        let selected_after = app
            .view
            .stack
            .root()
            .list
            .selected_item()
            .map(|it| (it.ext_id.clone(), it.id.clone()));
        assert_eq!(
            selected_after,
            Some(selected_before),
            "增量到达后选中项按身份恢复（不跳）"
        );
    }

    /// 旧轮进程在**新轮首个结果**到达时作废（等价旧「payload 整体替换」，
    /// 零项期之前旧进程照常服务——无空窗）。
    #[test]
    fn streaming_first_arrival_replaces_old_processes() {
        let mut app = make_app();
        app.pool.aggregate_rx = None;
        app.pool
            .processes
            .push(("old".to_string(), crate::test_support::dying_client("old")));
        app.apply_aggregate_meta(empty_meta());
        assert_eq!(app.pool.processes.len(), 1, "Meta 不动作废（旧进程服务中）");
        app.apply_aggregate_ext(arrival("new", &["N1"]));
        assert_eq!(app.pool.processes.len(), 0, "首个结果到达即替换旧轮进程");
    }
}
