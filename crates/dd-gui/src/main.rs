//! dd-gui 宿主面板（bin 入口）：egui 窗口骨架 + 页面栈渲染 + 命令执行链路。
//!
//! M1–M3 职责（见 [`docs/implementation.md`](../../docs/implementation.md)）：
//! 1. **键盘焦点**：FilterBox 有焦点时，`↑↓/Tab/Enter/Esc` 仍能经
//!    `ctx.input_mut(|i| i.consume_key(...))` 在应用层可靠拦截（A11）；
//! 2. **窗口行为**：无边框、置顶、初始隐藏、失焦自动隐藏、热键唤起（A1）；
//! 3. **Root View（界面 01）**：FilterBox + 分组列表（Section）+ tags chip + 页脚键位提示；
//!    数据来自首屏聚合（扫描扩展 → 并行拉取 `top_level_commands`），失败扩展不阻塞整体；
//! 4. **页面栈**（M2）：`CommandRef::Page` 命令进入嵌套页（后台 `get_items`），
//!    `Esc` 非 Root 先返回、Root 再隐藏；`GoBack`/`GoHome` 由结果类型驱动（A5）；
//! 5. **命令执行**（M2）：Enter → 后台 `invoke` → 8 种 `CommandResultKind` 裁决为
//!    宿主动作（关闭/隐藏/回首页/返回/保持/跳页/Toast/确认，A4）；`Confirm` 弹确认框，
//!    确认后带 `context.confirmed = true` 重发（协议 §8.3 注）；
//! 6. **列表刷新**（M2）：`items_changed` 通知 → 100ms 合并 → 全量重拉 `get_items`
//!    （协议 §6.3 + 验收 A9：协议层不做增量推送）；
//! 7. **缓存与懒加载**（M3）：冷启动按 `frozen` 分流——磁盘桩命中直接渲染、**不拉起进程**
//!    （A6，桩缓存由 `dd-host::cache::FrozenCache` 落盘）；点击桩项走**复热链路**
//!    （spawn → initialize → `get_command` → 执行，协议 §6.4）；warm 进程经
//!    `LruWarmSet` 保活、超容驱逐回落 stub（A7）；`ColdStartTimer` 埋点供 A2 实测。
//!
//! eframe 0.36 的 `App` trait 为 `ui()` + `logic()` 两个回调：
//! - `logic`：窗口**隐藏时也会被调用**（经 `request_repaint` 唤醒）→ 热键与失焦；
//! - `ui`：窗口可见/需重绘时调用 → 后台结果轮询、键盘导航与绘制。

use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;

use dd_gui::aggregator::{self, SourceStatus};
use dd_gui::hotkey::{HotkeyEvent, HotkeyThread};
use dd_gui::navigation::{PageStack, PageState};
use dd_gui::result::{self, HostAction, PendingConfirm};
use dd_gui::state::{PanelItem, PanelState};
use dd_host::cache::{ColdStartTimer, FrozenCache, LruWarmSet};
use dd_host::manifest::{self, LoadedExtension};
use dd_host::process::{ExtensionProcess, TIMEOUT_GET_ITEMS, TIMEOUT_INVOKE};
use dd_protocol::messages::{GetItemsParams, GetItemsResult, InvokeParams};
use dd_protocol::model::{CommandRef, CommandResult};

/// §6.3 + A9：`items_changed` 通知的合并窗口（窗口内多次通知只重拉一次）。
const REFRESH_WINDOW: Duration = Duration::from_millis(100);
/// Toast 默认显示时长（扩展未指定 `duration_ms` 时）。
const TOAST_DEFAULT_MS: u64 = 2_000;
/// M3 LRU 保活容量（设计文档 §6.3"最近 N 个"；超出则 close+释放、命令回落 stub，A7）。
const LRU_WARM_CAPACITY: usize = 8;

/// 首屏聚合的后台线程回传内容。
struct AggregatePayload {
    items: Vec<PanelItem>,
    sources: Vec<aggregator::SourceSummary>,
    /// 保活进程：`(扩展清单 id, 进程)`（仅 warm；frozen 读桩无进程）。
    processes: Vec<(String, ExtensionProcess)>,
    /// 已扫描扩展（含 manifest frozen/entry），供桩复热 spawn（M3）。
    exts: Vec<LoadedExtension>,
    /// 来源备注（兜底/异常提示，空串表示正常扫描）。
    note: String,
    /// 聚合线程内从"开始 scan"到"完成 collect+flatten"耗时（ms）。
    /// 与 [`PaletteApp::cold`] 的"进程启动→首屏就绪"总耗时对照，便于 A2 瓶颈定位
    /// （implementation.md R2：未达标记录实测与瓶颈，不调目标）。
    agg_ms: u64,
}

/// 后台 `invoke` 的结果（进程随结果归还主线程）。
struct InvokeOutcome {
    ext_id: String,
    /// `Some` = 进程对象（成功或链路内错误都归还，由 poll 按 `stub_reheat` 决定取舍）；
    /// `None` = 复热 spawn 本身失败（无进程可归还）。
    proc: Option<ExtensionProcess>,
    result: Result<CommandResult, String>,
    /// 本次是否由**桩复热**发起（spawn 的新进程）：失败时不归还进程、回退 stub。
    stub_reheat: bool,
}

/// 后台 `get_items` 的结果。
struct PageOutcome {
    ext_id: String,
    /// 同 [`InvokeOutcome::proc`]。
    proc: Option<ExtensionProcess>,
    page_id: String,
    result: Result<GetItemsResult, String>,
    /// 本次是否由**桩复热**发起：失败时不归还进程、回退 stub。
    stub_reheat: bool,
}

/// Toast 提示条（过期即清除）。
struct ToastState {
    message: String,
    expires: Instant,
}

/// 待用户二次确认的对话框。
struct ConfirmDialog {
    /// 发起该命令的扩展（确认后据此重发 `invoke`）。
    ext_id: String,
    title: String,
    description: String,
    confirm_label: String,
    is_critical: bool,
    /// 确认后重发 `invoke` 所需的原始请求。
    pending: PendingConfirm,
}

/// `items_changed` 的合并刷新调度。
struct RefreshState {
    page_id: String,
    ready_at: Instant,
}

fn main() -> eframe::Result {
    // A2 冷启动计时起点：**进程进入 main 即开始**，覆盖 eframe/wgpu 窗口创建 +
    // 字体加载（msyh.ttc ~19.7MB + seguisym 2.5MB）+ 聚合全过程。
    // 之前放在 setup 闭包内、且位于 setup_cjk_fonts 之后，把最重的字体加载整段漏掉了
    // （实测只剩 4~6 ms，测的几乎什么都不是）。
    let mut cold = ColdStartTimer::new();
    cold.mark_spawn_start();
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([560.0, 460.0])
        .with_decorations(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_resizable(false)
        .with_visible(false); // 初始隐藏，热键唤起
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "dd-run",
        options,
        Box::new(|cc| {
            setup_cjk_fonts(&cc.egui_ctx);
            // 初始隐藏双保险：`with_visible(false)` 之外显式发 `Visible(false)`，
            // 规避 eframe/egui 0.36 在 Windows 上 `with_visible` 偶发不生效的情况。
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(false));
            let hotkey = HotkeyThread::spawn(cc.egui_ctx.clone());
            // M3 磁盘桩缓存（读桩不拉起进程 A6；目录 = 数据根目录/cache）
            let cache = manifest::cache_dir().map(FrozenCache::new);
            // M3 A2 冷启动计时起点：进程就绪即开始（聚合线程随后启动）
            let mut cold = ColdStartTimer::new();
            cold.mark_spawn_start();
            let (agg_tx, agg_rx) = mpsc::channel();
            spawn_aggregation(agg_tx, cache);
            Ok(Box::new(PaletteApp::new(hotkey.events, agg_rx, cold)))
        }),
    )
}

/// 后台线程执行首屏收集（不阻塞 UI，A12）：扫描 → M3 分流（frozen 读桩 / fresh spawn）→ 合并 → 回传。
fn spawn_aggregation(tx: mpsc::Sender<AggregatePayload>, cache: Option<FrozenCache>) {
    thread::spawn(move || {
        // A2 拆分计时的"数据平面"：从 scan 起到聚合完成止（不含 GUI/字体加载）
        let agg_start = Instant::now();
        let (exts, note) = aggregator::load_extension_sources();
        let result = aggregator::collect_top_level(&exts, cache.as_ref());
        let (items, sources) = aggregator::flatten(&result.per_ext);

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
            note,
            agg_ms,
        });
    });
}

/// 加载本地字体栈：CJK 主字（msyh / SimHei / Deng）+ Segoe UI Symbol 符号后援。
///
/// msyh.ttc 覆盖 CJK 与 ✓/✗（Dingbats 区），但**缺** Geometric Shapes 的 ◌ (U+25CC)
/// ——M3 桩态页脚会渲染成方框。seguisym.ttf（Win 7+ 必装）补 Geometric Shapes /
/// Misc Symbols，把 ◌/○/· 等符号路由到它去渲染。
fn setup_cjk_fonts(ctx: &egui::Context) {
    let cjk_candidates = [
        // 优先 msyh.ttc（YaHei，Win7+ 必装且完整含 U+2713 ✓ 与 CJK）
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\Deng.ttf",
    ];
    let sym_candidate = r"C:\Windows\Fonts\seguisym.ttf";

    let mut fonts = egui::FontDefinitions::default();
    let mut any_loaded = false;

    if let Some(path) = cjk_candidates
        .into_iter()
        .find(|p| std::path::Path::new(p).is_file())
    {
        match std::fs::read(path) {
            Ok(bytes) => {
                fonts.font_data.insert(
                    "cjk".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push("cjk".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("cjk".to_owned());
                any_loaded = true;
            }
            Err(e) => eprintln!("[dd-gui] 读 CJK 字体 {path} 失败：{e}"),
        }
    }
    if let Ok(bytes) = std::fs::read(sym_candidate) {
        // 符号后援：append 在 cjk 之后，egui 字形回退按字体族顺序查找，
        // cjk 缺的 Geometric Shapes/Misc Symbols 落到 seguisym。
        fonts.font_data.insert(
            "sym".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .push("sym".to_owned());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("sym".to_owned());
        any_loaded = true;
    } else {
        eprintln!("[dd-gui] 未找到 {sym_candidate}（符号字体）；M3 桩态 ◌ 等符号可能仍显示为方块");
    }

    if !any_loaded {
        eprintln!("[dd-gui] 未找到任何 CJK 字体，中文可能显示为方块");
        return;
    }
    ctx.set_fonts(fonts);
}

struct PaletteApp {
    /// 页面栈：栈底为 Root（首屏聚合），其上为嵌套页。
    stack: PageStack,
    /// 热键事件接收端。
    events: Receiver<HotkeyEvent>,
    /// 窗口是否可见。
    visible: bool,
    /// 下次显示时请求 FilterBox 获得焦点。
    want_focus: bool,
    /// 是否已经取得过窗口焦点（用于失焦隐藏的防误触）。
    ever_focused: bool,
    /// 启动后是否已完成窗口居中（`monitor_size` 首次可读时执行一次）。
    recentered: bool,
    /// 首屏聚合结果接收端（聚合完成前为 `Some`）。
    aggregate_rx: Option<Receiver<AggregatePayload>>,
    /// 聚合是否仍在进行（列表区显示加载态）。
    aggregating: bool,
    /// 扩展源状态（页脚展示）。
    sources: Vec<aggregator::SourceSummary>,
    /// 聚合来源备注。
    note: String,
    /// 保活进程：`(扩展 id, 进程)`。发起请求时 take、结果归还，
    /// 保证同一进程同一时刻最多 1 个 in-flight 请求（协议 §4 串行化）。
    processes: Vec<(String, ExtensionProcess)>,
    /// 已扫描扩展（含 manifest frozen/entry），供桩复热 spawn（M3）。
    exts: Vec<LoadedExtension>,
    /// M3 LRU 保活集（容量 [`LRU_WARM_CAPACITY`]）：超容驱逐 → close + 命令回落 stub（A7）。
    lru: LruWarmSet,
    /// M3 冷启动计时（A2 实测：`spawn_start` → 首屏数据就绪）。
    cold: ColdStartTimer,
    /// 有请求在途（进程被 take / 桩复热线程未回）的扩展 id——防止同扩展二次并发。
    inflight: HashSet<String>,
    /// 最近一次 `invoke` 的命令 id（`Confirm` 重发时用）。
    last_command_id: Option<String>,
    /// 后台 `invoke` 结果接收端。
    invoke_rx: Option<Receiver<InvokeOutcome>>,
    /// 最近一次发起的 `invoke` 完整参数（Confirm 重发时沿用其 sender/context，
    /// 仅补 `confirmed=true`，见 `result::pending_confirm_for` 与协议 §8.3）。
    last_invoke: Option<InvokeParams>,
    /// 后台 `get_items` 结果接收端。
    page_rx: Option<Receiver<PageOutcome>>,
    /// Toast 提示条。
    toast: Option<ToastState>,
    /// 待确认的二次确认对话框。
    confirm: Option<ConfirmDialog>,
    /// `items_changed` 合并刷新调度。
    refresh: Option<RefreshState>,
    /// 鼠标上一帧悬停的行索引。仅当本帧悬停行与它**不同**时才接管选中，
    /// 静止不动的鼠标不再每帧抢占键盘（Tab/↓）选中——修复鼠标/键盘选择互相干扰。
    last_hovered_index: Option<usize>,
    /// 鼠标上一帧的指针屏幕坐标。**仅当本帧指针位置与它不同（鼠标真正移动）
    /// 时才允许 hover 接管选中**——区分「键盘滚动使内容从静止鼠标下方滑过」
    /// 与「鼠标主动移到别的行」，修复「一直按 ↑ 滚到顶部时被鼠标抢回鼠标所在行」。
    last_pointer_pos: Option<egui::Pos2>,
    /// 选中项是否需要滚动跟随（`scroll_to_me`）。键盘导航（↑↓/Tab）置 true；
    /// 鼠标 hover/点击驱动选中置 false——鼠标滑到边缘半可见行时若仍滚动，
    /// 内容会在静止的指针下移动、高亮与光标错位一格（"不跟手"）。
    scroll_follow: bool,
    /// 下次 `show()` 是否复位查询与选中：用户主动隐藏（Esc/热键/失焦）为 true
    /// （M1 清单第 10 项）；扩展 `Hide`（保留状态，协议 §8.3）置 false；
    /// 扩展 `Dismiss`（关闭）已清空状态，保持 true 无碍。
    reset_on_show: bool,
}

impl PaletteApp {
    fn new(
        events: Receiver<HotkeyEvent>,
        aggregate_rx: Receiver<AggregatePayload>,
        cold: ColdStartTimer,
    ) -> Self {
        Self {
            stack: PageStack::new(PageState::root(Vec::new())),
            events,
            visible: false,
            want_focus: true,
            ever_focused: false,
            recentered: false,
            aggregate_rx: Some(aggregate_rx),
            aggregating: true,
            sources: Vec::new(),
            note: String::new(),
            processes: Vec::new(),
            exts: Vec::new(),
            lru: LruWarmSet::new(LRU_WARM_CAPACITY),
            cold,
            inflight: HashSet::new(),
            last_command_id: None,
            invoke_rx: None,
            page_rx: None,
            last_invoke: None,
            toast: None,
            confirm: None,
            refresh: None,
            last_hovered_index: None,
            last_pointer_pos: None,
            scroll_follow: true, // 键盘是主输入：初始允许滚动跟随
            reset_on_show: true,
        }
    }

    // ── 窗口可见性 ───────────────────────────────────────────

    fn show(&mut self, ctx: &egui::Context) {
        self.visible = true;
        self.ever_focused = false;
        self.want_focus = true;
        // 复位语义（协议 §8.3 Hide/Dismiss 区分）：
        // - 用户主动隐藏（Esc/热键/失焦）→ 复位（M1 §4 清单第 10 项）；
        // - 扩展 `Hide` → 保留状态不复位（再次唤起仍在当前页/查询）；
        // - 扩展 `Dismiss` → 已在 dismiss() 清空，复位为空操作。
        if self.reset_on_show {
            self.stack.current_mut().list.reset();
        }
        self.refresh_health();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn hide(&mut self, ctx: &egui::Context) {
        self.visible = false;
        self.want_focus = false;
        self.reset_on_show = true; // 用户主动隐藏：默认下次唤起复位
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    /// 扩展请求 `Dismiss`（协议 §8.3：关闭面板）：清空页面栈回 Root 再隐藏，
    /// 下次唤起回到首页——与 `Hide`（保留状态）形成可观察区别。
    fn dismiss(&mut self, ctx: &egui::Context) {
        eprintln!("[dd-gui] Dismiss：清空页面栈回 Root 后隐藏");
        self.stack.go_home();
        self.stack.root_mut().list.reset();
        self.hide(ctx);
    }

    /// 扩展请求 `Hide`（协议 §8.3：隐藏但不关闭、保留状态）：
    /// 下次唤起不复位查询与选中，仍回到调用时的页面栈位置。
    fn hide_keep_state(&mut self, ctx: &egui::Context) {
        eprintln!("[dd-gui] Hide：保留状态隐藏（下次唤起不复位）");
        self.hide(ctx);
        self.reset_on_show = false;
    }

    fn poll_hotkey(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.events.try_recv() {
            match ev {
                HotkeyEvent::Toggle => {
                    if self.visible {
                        self.hide(ctx);
                    } else {
                        self.show(ctx);
                    }
                }
            }
        }
    }

    /// 启动后首次把窗口居中（仅执行一次）。
    /// egui 0.36 的 `ViewportCommand::center_on_screen` 会在 viewport 信息
    /// 尚未就绪时返回 `None`，因此每帧轮询直到 Some 即可（无需手算
    /// `monitor_size` / 窗口尺寸 / DPI 换算——由 `center_on_screen` 内部处理）。
    fn recenter_if_needed(&mut self, ctx: &egui::Context) {
        if self.recentered {
            return;
        }
        if let Some(cmd) = egui::ViewportCommand::center_on_screen(ctx) {
            ctx.send_viewport_cmd(cmd);
            self.recentered = true;
        }
    }

    /// 失焦自动隐藏（设计文档 §4.3 / 界面 01）。
    fn handle_focus_loss(&mut self, ctx: &egui::Context) {
        if !self.visible {
            return;
        }
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(false);
        if focused {
            self.ever_focused = true;
        }
        if self.ever_focused && !focused {
            self.hide(ctx);
        }
    }

    // ── 后台结果轮询 ─────────────────────────────────────────

    /// 首屏聚合结果：替换 Root 列表（保留用户已输入的查询）。
    fn poll_aggregate(&mut self) {
        let Some(rx) = &self.aggregate_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(payload) => {
                let query = self.stack.current().list.query().to_owned();
                let mut root = PageState::root(payload.items);
                root.list.set_query(query);
                *self.stack.root_mut() = root;
                self.sources = payload.sources;
                self.processes = payload.processes;
                self.exts = payload.exts;
                self.note = payload.note;
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

    /// `invoke` 结果：归还/取舍进程（M3 按是否桩复热）→ 裁决 8 种 Kind → 应用动作。
    fn poll_invoke(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.invoke_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(outcome) => {
                let InvokeOutcome {
                    ext_id,
                    proc,
                    result,
                    stub_reheat,
                } = outcome;
                self.inflight.remove(&ext_id);
                self.invoke_rx = None;
                match result {
                    Ok(command_result) => {
                        if let Some(p) = proc {
                            self.store_warm_process(ext_id.clone(), p);
                        }
                        if stub_reheat {
                            self.mark_source_warm(&ext_id);
                            eprintln!("[dd-gui] 桩复热成功：ext={ext_id} 转 warm（LRU 保活）");
                        }
                        let action = result::resolve(&command_result);
                        eprintln!("[dd-gui] invoke 成功：{command_result:?} → 动作 {action:?}");
                        self.apply_action(ctx, action, &ext_id);
                    }
                    Err(e) => {
                        if stub_reheat {
                            // 复热失败：新进程不归还（drop 即强杀），扩展保持 stub（A6 回退）
                            eprintln!("[dd-gui] 桩复热失败：ext={ext_id}，回退 stub：{e}");
                        } else if let Some(p) = proc {
                            // warm 请求失败：进程归还（超时/错误一般可恢复）
                            self.store_warm_process(ext_id.clone(), p);
                        }
                        eprintln!("[dd-gui] invoke 失败：{e}");
                        self.show_toast(format!("命令执行失败：{e}"), Some(3_000));
                    }
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.invoke_rx = None,
        }
    }

    /// `get_items` 结果：归还/取舍进程（M3 按是否桩复热）→ 更新对应页（页已退栈则作废）。
    fn poll_page(&mut self) {
        let Some(rx) = &self.page_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(outcome) => {
                let PageOutcome {
                    ext_id,
                    proc,
                    page_id,
                    result,
                    stub_reheat,
                } = outcome;
                self.inflight.remove(&ext_id);
                self.page_rx = None;

                if self.stack.current().page_id.as_deref() == Some(page_id.as_str()) {
                    match result {
                        Ok(res) => {
                            if let Some(p) = proc {
                                self.store_warm_process(ext_id.clone(), p);
                            }
                            if stub_reheat {
                                self.mark_source_warm(&ext_id);
                                eprintln!("[dd-gui] 桩复热成功：ext={ext_id} 转 warm（LRU 保活）");
                            }
                            let items_raw = res.items;
                            let is_loading = res.is_loading;
                            let items: Vec<PanelItem> = items_raw
                                .iter()
                                .map(|cmd| aggregator::to_panel_item(cmd, &ext_id, ""))
                                .collect();
                            eprintln!(
                                "[dd-gui] get_items 成功：page={page_id} items={}",
                                items.len()
                            );
                            let page = self.stack.current_mut();
                            page.is_loading = false;
                            page.empty = if items.is_empty() && !is_loading {
                                Some("该页暂无内容".to_string())
                            } else {
                                None
                            };
                            page.is_loading = is_loading;
                            page.list = PanelState::new(items);
                        }
                        Err(e) => {
                            if stub_reheat {
                                // 复热失败：不保活新进程、扩展保持 stub（A6 回退）
                                eprintln!("[dd-gui] 桩复热失败：ext={ext_id}，回退 stub：{e}");
                            } else if let Some(p) = proc {
                                self.store_warm_process(ext_id.clone(), p);
                            }
                            eprintln!("[dd-gui] get_items 失败：page={page_id}：{e}");
                            let page = self.stack.current_mut();
                            page.is_loading = false;
                            page.empty = Some(format!("拉取失败：{e}"));
                            page.list = PanelState::new(Vec::new());
                        }
                    }
                } else {
                    // 用户已离开来源页：成功（或 warm 失败）仍归还进程——它是扩展资产；
                    // 复热失败则不保活。
                    if result.is_ok() || !stub_reheat {
                        if let Some(p) = proc {
                            self.store_warm_process(ext_id.clone(), p);
                        }
                    }
                    eprintln!("[dd-gui] get_items 结果作废：已离开 page={page_id}");
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.page_rx = None,
        }
    }

    /// `items_changed` 通知轮询：命中当前页则进入 100ms 合并窗口。
    fn poll_notifications(&mut self) {
        if self.processes.is_empty() {
            return;
        }
        let current = self.stack.current().page_id.clone();
        let mut hit: Option<String> = None;
        let mut top_changed = false;
        // 注意：循环期间只借用 `self.processes`，不调用 `self.show_toast`
        // （后者可变借用整个 `self`，会与此处冲突）。
        for (_, proc) in self.processes.iter_mut() {
            for changed in proc.poll_notifications() {
                match changed {
                    // `None` = 顶层命令变了（当前仅提示，见 m2-record.md §5）
                    None => top_changed = true,
                    Some(pid) if Some(pid.as_str()) == current.as_deref() => hit = Some(pid),
                    Some(_) => {}
                }
            }
        }
        if top_changed {
            eprintln!("[dd-gui] 收到顶层 items_changed（Root 重聚合属遗留，仅提示）");
            self.show_toast("扩展命令已更新", Some(1_500));
        }
        if let Some(pid) = hit {
            eprintln!(
                "[dd-gui] 收到 items_changed page={pid} → {}ms 后全量重拉",
                REFRESH_WINDOW.as_millis()
            );
            if self.refresh.is_none() {
                self.refresh = Some(RefreshState {
                    page_id: pid,
                    ready_at: Instant::now() + REFRESH_WINDOW,
                });
            }
        }
    }

    /// 合并窗口到期：重拉当前页（**全量**，协议层无增量推送）。
    fn tick_refresh(&mut self) {
        let Some(refresh) = &self.refresh else {
            return;
        };
        if Instant::now() < refresh.ready_at {
            return;
        }
        let page_id = refresh.page_id.clone();
        self.refresh = None;

        let page = self.stack.current();
        // 用户可能已离开通知来源页（如已 GoBack）→ 目标页非当前页时丢弃，
        // 避免拉取一个不可见的页（结果也只会被 poll_page 作废）。
        if page.page_id.as_deref() != Some(page_id.as_str()) {
            eprintln!("[dd-gui] items_changed 刷新作废：已离开 page={page_id}");
            return;
        }
        let (ext_id, query) = (page.ext_id.clone(), page.list.query().to_owned());
        if ext_id.is_empty() {
            return;
        }
        let search = (!query.is_empty()).then_some(query);
        // M3：warm 直发 / 进程被驱逐则走复热；`command_id=None`（刷新非命令点击）
        self.dispatch_fetch_page(&ext_id, &page_id, search, None);
    }

    /// 崩溃检测骨架：已退出的进程，其扩展源状态标记为失败。
    fn refresh_health(&mut self) {
        if self.processes.is_empty() {
            return;
        }
        let exited: Vec<String> = self
            .processes
            .iter_mut()
            .filter_map(|(id, p)| p.has_exited().then(|| id.clone()))
            .collect();
        if exited.is_empty() {
            return;
        }
        for id in &exited {
            // 已退出进程从保活集移除（下次点击走桩复热、重新 spawn）；
            // 崩溃恢复（重试/连续崩溃保护）属 M4（A8）。
            self.processes.retain(|(pid, _)| pid != id);
            self.lru.remove(id);
            eprintln!("[dd-gui] 扩展进程已退出：{id}（移除保活，点击命令将重新拉起）");
            if let Some(s) = self.sources.iter_mut().find(|s| s.id == *id) {
                if !s.status.is_failed() {
                    s.status = SourceStatus::Failed {
                        error: "扩展进程已退出".to_string(),
                    };
                }
            }
        }
    }

    // ── 命令执行 ─────────────────────────────────────────────

    /// Enter/单击：按 `CommandRef` 分派（执行 / 进入页）。
    /// M3：扩展进程 warm → 直接执行；未 warm（frozen 桩）→ 复热后执行（A6）。
    fn confirm_selected(&mut self) {
        let Some(item) = self.stack.current().list.confirm().cloned() else {
            return;
        };
        let query = self.stack.current().list.query().to_owned();
        match &item.command {
            CommandRef::Invoke => {
                let params = result::invoke_params(&item.id, &query);
                self.dispatch_invoke(&item.ext_id, params);
            }
            CommandRef::Page { page_id } => {
                let page_id = page_id.clone();
                let search = (!query.is_empty()).then_some(query);
                self.open_page(&item.ext_id, &page_id, search, Some(item.id.clone()));
            }
        }
    }

    /// 按清单 id 找已扫描扩展（复热 spawn 用）。
    fn find_ext(&self, ext_id: &str) -> Option<&LoadedExtension> {
        self.exts.iter().find(|e| e.manifest.id == ext_id)
    }

    /// `invoke` 分派：warm 进程在 → 直接后台执行；不在 → 桩复热（A6）。
    fn dispatch_invoke(&mut self, ext_id: &str, params: InvokeParams) {
        if self.invoke_rx.is_some() || self.inflight.contains(ext_id) {
            eprintln!("[dd-gui] invoke 失败：ext={ext_id} 上一请求仍在处理");
            self.show_toast("扩展进程不可用（可能正在处理上一个请求）", Some(2_000));
            return;
        }
        if self.processes.iter().any(|(id, _)| id == ext_id) {
            self.start_invoke(ext_id, params);
        } else if let Some(ext) = self.find_ext(ext_id).cloned() {
            self.start_invoke_reheat(&ext, params); // 桩复热
        } else {
            eprintln!("[dd-gui] invoke 失败：ext={ext_id} 无扩展信息");
            self.show_toast("扩展信息缺失，无法执行", Some(2_000));
        }
    }

    /// 后台 `invoke`（warm：take 进程 → 线程调用 → 结果经 channel 归还）。
    fn start_invoke(&mut self, ext_id: &str, params: InvokeParams) {
        self.last_command_id = Some(params.id.clone());
        self.last_invoke = Some(params.clone()); // Confirm 重发沿用
        eprintln!("[dd-gui] invoke 发起：ext={ext_id} cmd={}", params.id);
        let Some(idx) = self.processes.iter().position(|(id, _)| id == ext_id) else {
            eprintln!("[dd-gui] invoke 失败：ext={ext_id} 进程不可用（可能 in-flight）");
            self.show_toast("扩展进程不可用（可能正在处理上一个请求）", Some(2_000));
            return;
        };
        let (_, mut proc) = self.processes.remove(idx);
        self.inflight.insert(ext_id.to_string());
        let ext_id = ext_id.to_string();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = invoke_on(&mut proc, &params);
            let _ = tx.send(InvokeOutcome {
                ext_id,
                proc: Some(proc),
                result,
                stub_reheat: false,
            });
        });
        self.invoke_rx = Some(rx);
    }

    /// 桩复热 + `invoke`（A6 / 协议 §6.4）：spawn → initialize → `get_command(id)` → invoke。
    /// 复热失败（spawn/握手/命令失效/超时）→ 不保活新进程、扩展保持 stub 并报错。
    fn start_invoke_reheat(&mut self, ext: &LoadedExtension, params: InvokeParams) {
        self.last_command_id = Some(params.id.clone());
        self.last_invoke = Some(params.clone());
        eprintln!(
            "[dd-gui] 桩复热：ext={} cmd={}（spawn→initialize→get_command→invoke）",
            ext.manifest.id, params.id
        );
        self.inflight.insert(ext.manifest.id.clone());
        let ext = ext.clone();
        let ext_id = ext.manifest.id.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut proc = match aggregator::spawn_and_initialize(&ext) {
                Ok(p) => p,
                Err(e) => {
                    // spawn/握手失败：无进程可归还，直接报错
                    let _ = tx.send(InvokeOutcome {
                        ext_id,
                        proc: None,
                        result: Err(e),
                        stub_reheat: true,
                    });
                    return;
                }
            };
            let result: Result<CommandResult, String> =
                match proc.get_command(&params.id).map_err(|e| e.to_string()) {
                    // §6.4：取回真实命令后再执行
                    Ok(Some(_)) => invoke_on(&mut proc, &params),
                    Ok(None) => {
                        Err("命令已失效：扩展未找到该命令（get_command 返回 null）".to_string())
                    }
                    Err(e) => Err(e),
                };
            let _ = tx.send(InvokeOutcome {
                ext_id,
                proc: Some(proc),
                result,
                stub_reheat: true,
            });
        });
        self.invoke_rx = Some(rx);
    }

    /// 进入嵌套页（页面入栈 + loading），并按 warm/桩选择取数路径。
    ///
    /// `command_id` = 被点击的 `Page` 命令 id（桩复热时按协议 §6.4 先 `get_command` 校验）；
    /// `GoToPage` 动作无对应命令点击，传 `None`。
    fn open_page(
        &mut self,
        ext_id: &str,
        page_id: &str,
        search: Option<String>,
        command_id: Option<String>,
    ) {
        self.stack
            .push(PageState::nested(page_id, page_id, ext_id, Vec::new()));
        self.stack.current_mut().is_loading = true;
        self.dispatch_fetch_page(ext_id, page_id, search, command_id);
    }

    /// `get_items` 分派：warm → take 直发；不在 → 桩复热后拉取（A6）。
    fn dispatch_fetch_page(
        &mut self,
        ext_id: &str,
        page_id: &str,
        search: Option<String>,
        command_id: Option<String>,
    ) {
        if self.page_rx.is_some() || self.inflight.contains(ext_id) {
            eprintln!("[dd-gui] get_items 失败：ext={ext_id} 上一请求仍在处理");
            let page = self.stack.current_mut();
            page.is_loading = false;
            page.empty = Some("扩展进程不可用（可能正在处理上一个请求）".to_string());
            return;
        }
        if self.processes.iter().any(|(id, _)| id == ext_id) {
            self.fetch_page_warm(ext_id, page_id, search);
        } else if let Some(ext) = self.find_ext(ext_id).cloned() {
            self.fetch_page_reheat(&ext, page_id, search, command_id);
        } else {
            let page = self.stack.current_mut();
            page.is_loading = false;
            page.empty = Some("扩展信息缺失，无法打开页面".to_string());
        }
    }

    /// 后台 `get_items`（warm：take 进程 → 线程调用 → 结果经 channel 归还）。
    fn fetch_page_warm(&mut self, ext_id: &str, page_id: &str, search: Option<String>) {
        let Some(idx) = self.processes.iter().position(|(id, _)| id == ext_id) else {
            eprintln!("[dd-gui] get_items 失败：ext={ext_id} 进程不可用（可能 in-flight）");
            let page = self.stack.current_mut();
            page.is_loading = false;
            page.empty = Some("扩展进程不可用".to_string());
            return;
        };
        let (_, mut proc) = self.processes.remove(idx);
        self.inflight.insert(ext_id.to_string());
        let ext_id = ext_id.to_string();
        let page_id = page_id.to_string();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = get_items_on(&mut proc, &page_id, search);
            let _ = tx.send(PageOutcome {
                ext_id,
                proc: Some(proc),
                page_id,
                result,
                stub_reheat: false,
            });
        });
        self.page_rx = Some(rx);
    }

    /// 桩复热 + `get_items`（A6 / 协议 §6.4）：spawn → initialize →（`get_command` 校验）→ get_items。
    /// 复热失败 → 不保活新进程、扩展保持 stub 并报错。
    fn fetch_page_reheat(
        &mut self,
        ext: &LoadedExtension,
        page_id: &str,
        search: Option<String>,
        command_id: Option<String>,
    ) {
        eprintln!(
            "[dd-gui] 桩复热：ext={} page={page_id}（spawn→initialize→get_command→get_items）",
            ext.manifest.id
        );
        self.inflight.insert(ext.manifest.id.clone());
        let ext = ext.clone();
        let ext_id = ext.manifest.id.clone();
        let page_id = page_id.to_string();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut proc = match aggregator::spawn_and_initialize(&ext) {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx.send(PageOutcome {
                        ext_id,
                        proc: None,
                        page_id,
                        result: Err(e),
                        stub_reheat: true,
                    });
                    return;
                }
            };
            // 协议 §6.4：被点击的 Page 命令先 `get_command` 校验桩是否仍有效
            let result: Result<GetItemsResult, String> = match &command_id {
                Some(cid) => match proc.get_command(cid).map_err(|e| e.to_string()) {
                    Ok(Some(_)) => get_items_on(&mut proc, &page_id, search),
                    Ok(None) => {
                        Err("命令已失效：扩展未找到该命令（get_command 返回 null）".to_string())
                    }
                    Err(e) => Err(e),
                },
                None => get_items_on(&mut proc, &page_id, search),
            };
            let _ = tx.send(PageOutcome {
                ext_id,
                proc: Some(proc),
                page_id,
                result,
                stub_reheat: true,
            });
        });
        self.page_rx = Some(rx);
    }

    /// 进程归还入口（M3）：写回 warm 集 + LRU 触达；超容驱逐最久未用者（A7）。
    fn store_warm_process(&mut self, ext_id: String, proc: ExtensionProcess) {
        self.processes.push((ext_id.clone(), proc));
        if let Some(victim) = self.lru.access(&ext_id) {
            if victim != ext_id {
                self.evict_warm(&victim);
            }
        }
    }

    /// LRU 驱逐（A7）：close + 终止进程、从保活集移除、源状态回落 stub。
    /// 优雅 close 走后台线程（≤1s+1s 超时），避免卡 UI；失败强杀由 Drop 兜底。
    fn evict_warm(&mut self, victim: &str) {
        if let Some(idx) = self.processes.iter().position(|(id, _)| id == victim) {
            let (_, proc) = self.processes.remove(idx);
            thread::spawn(move || {
                let _ = proc.close();
            });
            eprintln!("[dd-gui] LRU 驱逐：{victim}（warm 超容量，close+释放，命令回落 stub）");
        }
        self.lru.remove(victim);
        if let Some(s) = self.sources.iter_mut().find(|s| s.id == victim) {
            if !s.status.is_failed() {
                let n = match &s.status {
                    SourceStatus::Warm { commands } | SourceStatus::Stub { commands } => *commands,
                    SourceStatus::Failed { .. } => 0,
                };
                s.status = SourceStatus::Stub { commands: n };
            }
        }
    }

    /// 源状态转 warm（桩复热成功 / cold start warm 时调用；Failed→Warm 同理恢复）。
    fn mark_source_warm(&mut self, ext_id: &str) {
        if let Some(s) = self.sources.iter_mut().find(|s| s.id == ext_id) {
            if s.status.is_stub() || s.status.is_failed() {
                let n = match &s.status {
                    SourceStatus::Warm { commands } | SourceStatus::Stub { commands } => *commands,
                    SourceStatus::Failed { .. } => 0,
                };
                s.status = SourceStatus::Warm { commands: n };
            }
        }
    }

    /// 应用 8 种 Kind 裁决出的宿主动作（A4）。
    fn apply_action(&mut self, ctx: &egui::Context, action: HostAction, ext_id: &str) {
        match action {
            HostAction::Dismiss => self.dismiss(ctx),
            HostAction::Hide => self.hide_keep_state(ctx),
            HostAction::GoHome => self.stack.go_home(),
            HostAction::GoBack => {
                self.stack.go_back();
            }
            HostAction::KeepOpen => {}
            HostAction::GoToPage { page_id } => {
                let ext_id = ext_id.to_string();
                self.open_page(&ext_id, &page_id, None, None);
            }
            HostAction::ShowToast {
                message,
                duration_ms,
            } => self.show_toast(message, duration_ms),
            HostAction::Confirm {
                title,
                description,
                confirm_label,
                is_critical,
            } => {
                // §8.3 注：确认后宿主带 `context.confirmed = true` 重新 invoke。
                // 沿用原 invoke 的 sender/context（`pending_confirm_for` 保证
                // 不丢失搜索词/选中项，仅补 confirmed=true）。
                let command_id = self.last_command_id.clone().unwrap_or_default();
                let pending = result::pending_confirm_for(&command_id, self.last_invoke.as_ref());
                self.confirm = Some(ConfirmDialog {
                    ext_id: ext_id.to_string(),
                    title,
                    description,
                    confirm_label,
                    is_critical,
                    pending,
                });
            }
        }
    }

    fn show_toast(&mut self, message: impl Into<String>, duration_ms: Option<u64>) {
        let ms = duration_ms.unwrap_or(TOAST_DEFAULT_MS);
        self.toast = Some(ToastState {
            message: message.into(),
            expires: Instant::now() + Duration::from_millis(ms),
        });
    }

    // ── 键盘 ─────────────────────────────────────────────────

    /// 应用层拦截导航键（`consume_key` 移除事件，FilterBox 的 TextEdit 收不到
    /// → 输入光标不动）。设计文档 §4.3：`↑/↓` **或** `Tab/Shift+Tab` 移动、
    /// `Enter` 执行、`Esc` 关闭或返回上一级。
    fn handle_keys(&mut self, ctx: &egui::Context) {
        let (esc, down, up, enter, tab, shift_tab) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Tab),
                i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab),
            )
        });

        // 确认对话框活跃时：Enter=确认、Esc=取消，其余键不穿透到列表
        if self.confirm.is_some() {
            if esc {
                self.confirm = None;
            } else if enter {
                let dialog = self.confirm.take().expect("对话框存在");
                let params = dialog.pending.confirmed_params();
                self.dispatch_invoke(&dialog.ext_id, params);
            }
            return;
        }

        if esc {
            // 非 Root 先返回上一级，Root 再隐藏（§4.3）
            if self.stack.go_back().is_none() {
                self.hide(ctx);
            }
            return;
        }
        if down || tab {
            self.stack.current_mut().list.move_down();
            self.scroll_follow = true; // 键盘选中：恢复滚动跟随
        }
        if up || shift_tab {
            self.stack.current_mut().list.move_up();
            self.scroll_follow = true;
        }
        if enter {
            self.confirm_selected();
        }
    }

    // ── 渲染 ─────────────────────────────────────────────────

    fn draw_panel(&mut self, ui: &mut egui::Ui) {
        // 底部固定栏：源状态 + 键位提示（始终贴窗口底，不受中央列表高度影响；
        // 解决 M3 实测"列表长时把页脚挤出 460px 窗口"——之前把它们放进
        // CentralPanel 内的 ScrollArea 之后，长列表时整个 footer 块被推到
        // 视口下方。Panel::bottom 是 egui 0.36 处理 chrome vs content 的标准做法）。
        // 这里对 self 做不可变再借用，闭包结束后即可变借用给下面的 CentralPanel。
        let self_ref: &Self = &*self;
        egui::containers::Panel::bottom("status_footer").show(ui, |ui| {
            self_ref.draw_status_footer(ui);
        });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(ui.style().visuals.panel_fill)
                    .inner_margin(egui::Margin::symmetric(12, 10)),
            )
            .show(ui, |ui| {
                // ── 嵌套页标题栏（含返回提示） ───────────────
                let (page_id, page_title) = {
                    let page = self.stack.current();
                    (page.page_id.clone(), page.title.clone())
                };
                if let Some(pid) = &page_id {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(if page_title.is_empty() {
                                pid.clone()
                            } else {
                                page_title.clone()
                            })
                            .size(12.0)
                            .color(weak_text_color(ui)),
                        );
                        ui.label(
                            egui::RichText::new("[Esc] 返回")
                                .size(11.0)
                                .color(weak_text_color(ui)),
                        );
                    });
                    ui.add_space(4.0);
                }

                // ── FilterBox ────────────────────────────────
                let mut query = self.stack.current().list.query().to_owned();
                let filter = egui::TextEdit::singleline(&mut query)
                    .hint_text("搜索命令…")
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Body);
                let resp = ui.add(filter);
                let page = self.stack.current_mut();
                page.list.set_query(query);
                if self.want_focus {
                    resp.request_focus();
                    self.want_focus = false;
                }
                ui.add_space(6.0);

                // ── 列表区 ───────────────────────────────────
                if self.aggregating {
                    // 用 vertical_centered 而非 centered_and_justified：后者会撑满
                    // 列表区（与 BottomPanel 分离后这里本身已是剩余高度，但
                    // centered_and_justified 仍会顶到列表区上下沿视觉上难看）。
                    ui.vertical_centered(|ui| {
                        ui.weak("正在加载扩展…");
                    });
                } else {
                    self.draw_list(ui);
                }
            });
    }

    /// 底部固定栏内容：扩展源状态（含兜底/异常 note）+ 键位提示。
    fn draw_status_footer(&self, ui: &mut egui::Ui) {
        // 源状态 + 兜底/异常 note（横向自动换行，长 note 不会撑爆宽度）
        if !self.sources.is_empty() || !self.note.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for s in &self.sources {
                    match &s.status {
                        // M3 三态：Warm（进程活）/ Stub（仅磁盘桩）/ Failed
                        SourceStatus::Warm { commands } => {
                            ui.label(
                                egui::RichText::new(format!("✓ {}（{} 命令）", s.name, commands))
                                    .size(11.0)
                                    .color(weak_text_color(ui)),
                            );
                        }
                        SourceStatus::Stub { commands } => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "◌ {}（{} 命令·桩）",
                                    s.name, commands
                                ))
                                .size(11.0)
                                .color(weak_text_color(ui)),
                            );
                        }
                        SourceStatus::Failed { error } => {
                            ui.label(
                                egui::RichText::new(format!("✗ {}：{error}", s.name))
                                    .size(11.0)
                                    .color(ui.visuals().error_fg_color),
                            );
                        }
                    }
                    ui.add_space(10.0);
                }
                if !self.note.is_empty() {
                    ui.label(
                        egui::RichText::new(&self.note)
                            .size(11.0)
                            .color(weak_text_color(ui)),
                    );
                }
            });
        }
        ui.separator();
        // 键位提示（始终展示）
        ui.horizontal(|ui| {
            for (key, desc) in [
                ("↑↓/Tab", "移动"),
                ("Enter", "执行"),
                ("Esc", "返回/关闭"),
                ("Win+Alt+Space", "唤起/隐藏"),
            ] {
                ui.label(
                    egui::RichText::new(format!("[{key}] {desc}"))
                        .size(11.0)
                        .color(weak_text_color(ui)),
                );
                ui.add_space(8.0);
            }
        });
    }

    /// 当前页的列表渲染（Loading / 空态 / 按 `section` 分组）。
    ///
    /// 选中项通过 `scroll_to_me(None)` 滚入可视区（仅键盘选中时跟随，`scroll_follow`；
    /// 鼠标选中不滚动，避免内容在静止指针下位移造成错位）；鼠标 hover 高亮、单击执行
    /// （与 `Enter` 等价）。回写规则（修复鼠标/键盘选择互相干扰）：
    /// - `clicked`：选中并直接执行，不受 hover 冲突规则影响；
    /// - `hovered`：**仅当鼠标指针本帧真正移动过**（`hover_pos` 与上帧不同）且悬停行
    ///   与基准不同才接管选中——静止不动的鼠标不抢占键盘（Tab/↓/↑）选中，修复
    ///   「一直按 ↑ 滚到顶部时，内容从静止鼠标下方滑过把选中抢回鼠标所在行」。
    fn draw_list(&mut self, ui: &mut egui::Ui) {
        // 本帧鼠标指针屏幕坐标（用于区分「鼠标真的动了」vs「内容在静止鼠标下滚动」）。
        let current_hover_pos = ui.input(|i| i.pointer.hover_pos());

        // 先把需要的状态拷贝出来（释放对 `self` 的不可变借用），
        // 以便循环结束后可写回 hover/click 结果，避免借用冲突。
        let (is_loading, empty, selected, items, query_empty) = {
            let page = self.stack.current();
            (
                page.is_loading,
                page.empty.clone(),
                page.list.selected_index(),
                page.list
                    .filtered()
                    .map(|(i, it)| (i, it.clone()))
                    .collect::<Vec<_>>(),
                page.list.query().is_empty(),
            )
        };

        if is_loading {
            // 紧凑居中：不撑满高度，避免把列表外的 sources/键位挤出窗口
            ui.vertical_centered(|ui| {
                ui.weak("正在加载…");
            });
            return;
        }
        if let Some(empty) = empty {
            ui.vertical_centered(|ui| {
                ui.weak(empty);
            });
            return;
        }
        if items.is_empty() {
            ui.vertical_centered(|ui| {
                if query_empty {
                    ui.weak("未发现命令（检查扩展清单或扩展运行状态）");
                } else {
                    ui.weak("没有匹配项");
                }
            });
            return;
        }

        // 按 section 分组（用拷贝出的 items，不借用 self）
        let mut groups: Vec<(String, Vec<(usize, PanelItem)>)> = Vec::new();
        for (idx, item) in &items {
            match groups.iter_mut().find(|(s, _)| s == &item.section) {
                Some((_, list)) => list.push((*idx, item.clone())),
                None => groups.push((item.section.clone(), vec![(*idx, item.clone())])),
            }
        }

        let mut hovered: Option<usize> = None;
        let mut clicked: Option<usize> = None;
        let scroll_follow = self.scroll_follow;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (section, group_items) in &groups {
                ui.add_space(4.0);
                if !section.is_empty() {
                    ui.label(
                        egui::RichText::new(section)
                            .size(12.0)
                            .color(weak_text_color(ui)),
                    );
                    ui.add_space(2.0);
                }
                for (idx, item) in group_items {
                    let resp = draw_item_row(ui, item, Some(*idx) == selected);
                    if Some(*idx) == selected && scroll_follow {
                        // 选中项滚入可视区（仅键盘选中时跟随；鼠标选中不滚动，
                        // 避免内容在静止指针下移动造成高亮与光标错位）
                        resp.scroll_to_me(None);
                    }
                    if resp.hovered() {
                        hovered = Some(*idx);
                    }
                    if resp.clicked() {
                        clicked = Some(*idx);
                    }
                }
            }
        });

        // 回写鼠标结果：
        // - clicked：选中并直接执行（与 Enter 等价），不受 hover 冲突规则影响；
        // - hovered：**仅当鼠标指针本帧真正移动过**（`current_hover_pos` ≠ 上一帧），
        //   且悬停行与基准不同，才接管选中——静止的鼠标不抢占键盘（Tab/↓/↑）选中，
        //   修复「一直按 ↑ 滚到顶部时，内容从静止鼠标下滚过把选中抢回鼠标所在行」。
        let pointer_moved = self.last_pointer_pos != current_hover_pos;
        let last_hovered = self.last_hovered_index;
        if let Some(idx) = clicked {
            self.stack.current_mut().list.set_selected(idx);
            self.confirm_selected();
            self.last_hovered_index = clicked; // 点击位置即新的 hover 基准
            self.scroll_follow = false; // 鼠标驱动选中：不滚动跟随
        } else if pointer_moved && hovered != last_hovered {
            if let Some(idx) = hovered {
                if self.stack.current_mut().list.set_selected(idx) {
                    // 选中确实变化：滚随关闭 + **强制下一帧重绘**。
                    // egui 按需重绘——选中在帧末回写、高亮下一帧才绘制，
                    // 鼠标停下后没有新输入事件就不会再有下一帧，
                    // 高亮会「卡」在旧行直到再动鼠标。
                    self.scroll_follow = false;
                    ui.ctx().request_repaint();
                }
            }
            self.last_hovered_index = hovered;
        }
        // 更新指针坐标基准（无论是否移动都记录，供下一帧比较）。
        self.last_pointer_pos = current_hover_pos;
    }

    /// Toast 提示条（悬浮于面板底部居中）。
    /// 必须画在独立 `Area`（ctx 层）：若在 `CentralPanel` 之后追加到根 `Ui`，
    /// 布局会落到面板矩形之外被裁剪——真机表现为「Toast 永远不显示」
    /// （M2 真机反馈 #1/#5/#6/#8 的共同根因）。
    fn draw_toast(&self, ctx: &egui::Context) {
        let Some(toast) = &self.toast else {
            return;
        };
        egui::Area::new(egui::Id::new("dd-gui-toast"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -48.0])
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(ui.visuals().extreme_bg_color)
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(&toast.message).size(12.0));
                    });
            });
    }

    /// 二次确认对话框（居中浮层，Enter 确认 / Esc 取消；鼠标点击 确认 同样
    /// 触发 invoke）。渲染只读借用 `self.confirm`，点击后再取走并真正发起请求，
    /// 避免闭包内同时持有 `self` 的不可变与可变借用。
    fn draw_confirm(&mut self, ctx: &egui::Context) {
        let dialog = match self.confirm.as_ref() {
            Some(d) => d,
            None => return,
        };
        let title = dialog.title.clone();
        let description = dialog.description.clone();
        let confirm_label = dialog.confirm_label.clone();
        let is_critical = dialog.is_critical;

        let mut confirmed = false;
        let mut cancelled = false;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(&description);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let label = if confirm_label.is_empty() {
                        "确认".to_string()
                    } else {
                        confirm_label
                    };
                    let color = if is_critical {
                        ui.visuals().error_fg_color
                    } else {
                        ui.visuals().text_color()
                    };
                    if ui
                        .add(egui::Button::new(egui::RichText::new(label).color(color)))
                        .clicked()
                    {
                        confirmed = true;
                    }
                    if ui.button("取消").clicked() {
                        cancelled = true;
                    }
                });
                ui.label(
                    egui::RichText::new("[Enter] 确认   [Esc] 取消")
                        .size(11.0)
                        .color(weak_text_color(ui)),
                );
            });

        if confirmed {
            // 取走对话框并真正重发 invoke（带 context.confirmed = true）
            let taken = self.confirm.take().expect("对话框仍应在位");
            let params = taken.pending.confirmed_params();
            self.dispatch_invoke(&taken.ext_id, params);
        } else if cancelled {
            self.confirm = None;
        }
    }
}

/// 在给定进程上执行一次 `invoke`（协议 §6.5），返回 `CommandResult` 本体。
///
/// `call` 已解开 JSON-RPC 信封，返回的内层 `result` 即 §8.3 `CommandResult` 本体——
/// 直接解析；若按 `InvokeResult`（要求 `result` 字段）再包一层，任何成功 invoke
/// 都会报「响应解析失败：missing field `result`」（M2 修复记录）。
fn invoke_on(proc: &mut ExtensionProcess, params: &InvokeParams) -> Result<CommandResult, String> {
    serde_json::to_value(params)
        .map_err(|e| format!("参数序列化失败：{e}"))
        .and_then(|v| {
            proc.call("invoke", v, TIMEOUT_INVOKE)
                .map_err(|e| e.to_string())
        })
        .and_then(|v| {
            serde_json::from_value::<CommandResult>(v).map_err(|e| format!("响应解析失败：{e}"))
        })
}

/// 在给定进程上全量拉取一页（协议 §6.3 `get_items`）。
fn get_items_on(
    proc: &mut ExtensionProcess,
    page_id: &str,
    search: Option<String>,
) -> Result<GetItemsResult, String> {
    let params = GetItemsParams {
        page_id: page_id.to_string(),
        search_text: search,
    };
    serde_json::to_value(&params)
        .map_err(|e| format!("参数序列化失败：{e}"))
        .and_then(|v| {
            proc.call("get_items", v, TIMEOUT_GET_ITEMS)
                .map_err(|e| e.to_string())
        })
        .and_then(|v| {
            serde_json::from_value::<GetItemsResult>(v).map_err(|e| format!("响应解析失败：{e}"))
        })
}

/// egui 0.36 中 `weak_text_color` 是 `Option<Color32>`，取不到时退回文本色。
fn weak_text_color(ui: &egui::Ui) -> egui::Color32 {
    ui.visuals()
        .weak_text_color
        .unwrap_or_else(|| ui.visuals().text_color())
}

/// 单个列表项行：标题 + 副标题 + tags chip；选中态高亮背景。
/// 返回整行可交互响应（hover + click 感知），供 `draw_list` 做
/// 「悬停高亮 / 单击执行」与「选中项滚入可视区」。
fn draw_item_row(ui: &mut egui::Ui, item: &PanelItem, selected: bool) -> egui::Response {
    let accent = ui.visuals().selection.bg_fill;
    let fill = if selected {
        accent
    } else {
        egui::Color32::TRANSPARENT
    };

    // Frame 仅带 hover 感知；显式用 `ui.interact` 注册 click，使整行可点击
    let frame_resp = egui::Frame::default()
        .fill(fill)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    let title = if selected {
                        egui::RichText::new(&item.title).strong()
                    } else {
                        egui::RichText::new(&item.title)
                    };
                    ui.label(title);
                    if !item.subtitle.is_empty() {
                        ui.label(
                            egui::RichText::new(&item.subtitle)
                                .size(12.0)
                                .color(weak_text_color(ui)),
                        );
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for tag in item.tags.iter().rev() {
                        egui::Frame::default()
                            .fill(ui.visuals().extreme_bg_color)
                            .corner_radius(8.0)
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(tag)
                                        .size(11.0)
                                        .color(weak_text_color(ui)),
                                );
                            });
                        ui.add_space(4.0);
                    }
                });
            });
        })
        .response;

    ui.interact(
        frame_resp.rect,
        ui.id().with(("hit", &item.id)),
        egui::Sense::click().union(egui::Sense::hover()),
    )
}

impl eframe::App for PaletteApp {
    /// 窗口隐藏时也会被调用（热键线程 `request_repaint` 唤醒）。
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.recenter_if_needed(ctx);
        self.poll_hotkey(ctx);
        self.handle_focus_loss(ctx);
        // A2 计时的正确性：聚合结果**必须**在窗口隐藏时也安装并完成计时。
        // 此前 poll_aggregate 只在 ui() 里调用，而 ui() 在 !visible 时直接 return
        // ——面板初始隐藏 ⇒ 冷启动计时会一直等到用户按下热键才触发，
        // 实测值里混入了"用户何时按 Win+Alt+Space"的人肉延迟（1936–3214 ms 乱跳的真因）。
        self.poll_aggregate();
        // 聚合未完成时持续请求重绘以驱动本回调（隐藏窗口下否则可能不产生帧）。
        if self.aggregate_rx.is_some() {
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.visible {
            return;
        }
        let ctx = ui.ctx().clone();
        self.poll_invoke(&ctx);
        self.poll_page();
        self.poll_notifications();
        self.tick_refresh();

        // Toast 到期清除；未到期则预约重绘
        if let Some(toast) = &self.toast {
            let now = Instant::now();
            if now >= toast.expires {
                self.toast = None;
            } else {
                ctx.request_repaint_after(toast.expires - now);
            }
        }

        self.handle_keys(&ctx);
        self.draw_panel(ui);
        self.draw_toast(&ctx);
        self.draw_confirm(&ctx);
    }
}
