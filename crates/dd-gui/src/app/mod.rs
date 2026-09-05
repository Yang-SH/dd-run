//! PaletteApp：宿主面板的业务编排中枢（状态容器 + eframe 调度入口）。
//!
//! 拆分自原 main.rs（docs/refactor-layering-plan.md 方案 1）：方法体逐字未改，
//! 仅按职责物理分文件——业务编排在 app/，绘制在 [`crate::ui`]，系统副作用在
//! [`crate::platform`]，纯函数在 [`crate::text`]。

pub(crate) mod aggregate;
pub(crate) mod ctx_menu;
pub(crate) mod fallback_flow;
pub(crate) mod health;
pub(crate) mod host_actions;
pub(crate) mod invoke;
pub(crate) mod keys;
pub(crate) mod lifecycle;
pub(crate) mod page;
pub(crate) mod pool;
pub(crate) mod refresh;
pub(crate) mod toast;

/// bin 入口（main.rs）需要的两个跨 crate 符号（facade 再导出）。
pub use aggregate::{spawn_aggregation, AggregatePayload};

use crate::app::ctx_menu::CtxMenuState;
use crate::app::fallback_flow::FallbackFetchOutcome;
use crate::app::invoke::InvokeOutcome;
use crate::app::page::PageOutcome;
use crate::app::pool::LRU_WARM_CAPACITY;
use crate::app::refresh::RefreshState;
use crate::app::toast::ConfirmDialog;
use crate::app::toast::ToastState;
use dd_gui::aggregator;
use dd_gui::hotkey::HotkeyEvent;
use dd_gui::navigation::PageStack;
use dd_gui::navigation::PageState;
use dd_gui::robustness::CrashGuard;
use dd_gui::tray::TrayEvent;
use dd_host::cache::ColdStartTimer;
use dd_host::cache::LruWarmSet;
use dd_host::manifest::LoadedExtension;
use dd_host::process::ExtensionProcess;
use dd_protocol::messages::InvokeParams;
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::time::Instant;

/// 面板逻辑尺寸（设计稿 v2：宽 560；高沿用现行 460）。`show()` 居中定位用。
pub const APP_W: f32 = 560.0;

pub const APP_H: f32 = 460.0;

/// 设置页窗口尺寸：设置卡片内容（主题三选 + 首屏视图 + 快捷键/自启占位 +
/// 页脚）在 460px 高度下截断（真机反馈），设置页允许与启动页尺寸不一致。
pub(crate) const SETTINGS_W: f32 = 560.0;

pub(crate) const SETTINGS_H: f32 = 640.0;

/// 启动期窗口的屏幕外坐标（远离所有显示器的负象限）——实现"物理不可见"：
/// eframe 0.36 首帧渲染后**无条件** `set_visible(true)`（egui PR #2279"画完才
/// 显示"），`with_visible(false)` 拦不住；把窗口放屏幕外，即使被强制显示也
/// 不可能被用户看到，彻底消除启动黑框/跳动毛刺（设计稿 03 验收）。
pub const OFFSCREEN_X: f32 = -20_000.0;

pub const OFFSCREEN_Y: f32 = -20_000.0;

pub struct PaletteApp {
    /// 页面栈：栈底为 Root（首屏聚合），其上为嵌套页。
    pub(crate) stack: PageStack,
    /// 热键事件接收端。
    pub(crate) events: Receiver<HotkeyEvent>,
    /// 托盘事件接收端（设计稿 10C：Toggle / OpenSettings / Exit）。
    pub(crate) tray_events: Receiver<TrayEvent>,
    /// 窗口是否可见。
    pub(crate) visible: bool,
    /// 下次显示时请求 FilterBox 获得焦点。
    pub(crate) want_focus: bool,
    /// 是否已经取得过窗口焦点（用于失焦隐藏的防误触）。
    pub(crate) ever_focused: bool,
    /// 首屏聚合结果接收端（聚合完成前为 `Some`）。
    pub(crate) aggregate_rx: Option<Receiver<AggregatePayload>>,
    /// 聚合是否仍在进行（列表区显示加载态）。
    pub(crate) aggregating: bool,
    /// 扩展源状态（健康检查/LRU 复热逻辑用；**页脚不再展示**，用户决策 2026-09-04）。
    pub(crate) sources: Vec<aggregator::SourceSummary>,
    /// 保活进程：`(扩展 id, 进程)`。发起请求时 take、结果归还，
    /// 保证同一进程同一时刻最多 1 个 in-flight 请求（协议 §4 串行化）。
    pub(crate) processes: Vec<(String, ExtensionProcess)>,
    /// 已扫描扩展（含 manifest frozen/entry），供桩复热 spawn（M3）。
    pub(crate) exts: Vec<LoadedExtension>,
    /// M3 LRU 保活集（容量 [`LRU_WARM_CAPACITY`]）：超容驱逐 → close + 命令回落 stub（A7）。
    pub(crate) lru: LruWarmSet,
    /// M3 冷启动计时（A2 实测：`spawn_start` → 首屏数据就绪）。
    pub(crate) cold: ColdStartTimer,
    /// 有请求在途（进程被 take / 桩复热线程未回）的扩展 id——防止同扩展二次并发。
    pub(crate) inflight: HashSet<String>,
    /// 最近一次 `invoke` 的命令 id（`Confirm` 重发时用）。
    pub(crate) last_command_id: Option<String>,
    /// 后台 `invoke` 结果接收端。
    pub(crate) invoke_rx: Option<Receiver<InvokeOutcome>>,
    /// 最近一次发起的 `invoke` 完整参数（Confirm 重发时沿用其 sender/context，
    /// 仅补 `confirmed=true`，见 `result::pending_confirm_for` 与协议 §8.3）。
    pub(crate) last_invoke: Option<InvokeParams>,
    /// 后台 `get_items` 结果接收端。
    pub(crate) page_rx: Option<Receiver<PageOutcome>>,
    /// Toast 提示条。
    pub(crate) toast: Option<ToastState>,
    /// 待确认的二次确认对话框。
    pub(crate) confirm: Option<ConfirmDialog>,
    /// `items_changed` 合并刷新调度。
    pub(crate) refresh: Option<RefreshState>,
    /// 鼠标上一帧悬停的行索引。仅当本帧悬停行与它**不同**时才接管选中，
    /// 静止不动的鼠标不再每帧抢占键盘（Tab/↓）选中——修复鼠标/键盘选择互相干扰。
    pub(crate) last_hovered_index: Option<usize>,
    /// 鼠标上一帧的指针屏幕坐标。**仅当本帧指针位置与它不同（鼠标真正移动）
    /// 时才允许 hover 接管选中**——区分「键盘滚动使内容从静止鼠标下方滑过」
    /// 与「鼠标主动移到别的行」，修复「一直按 ↑ 滚到顶部时被鼠标抢回鼠标所在行」。
    pub(crate) last_pointer_pos: Option<egui::Pos2>,
    /// 选中项是否需要滚动跟随（`scroll_to_me`）。键盘导航（↑↓/Tab）置 true；
    /// 鼠标 hover/点击驱动选中置 false——鼠标滑到边缘半可见行时若仍滚动，
    /// 内容会在静止的指针下移动、高亮与光标错位一格（"不跟手"）。
    pub(crate) scroll_follow: bool,
    /// 下次 `show()` 是否复位查询与选中：用户主动隐藏（Esc/热键/失焦）为 true
    /// （M1 清单第 10 项）；扩展 `Hide`（保留状态，协议 §8.3）置 false；
    /// 扩展 `Dismiss`（关闭）已清空状态，保持 true 无碍。
    pub(crate) reset_on_show: bool,
    /// M4：连续崩溃保护状态机（协议 §11）——键 = 扩展清单 id。
    pub(crate) crash_guards: HashMap<String, CrashGuard>,
    /// M4 宿主 fallback 轮：兜底模板缓存与渲染状态（协议 §6.2）。
    pub(crate) fallback_store: dd_gui::fallback::FallbackStore,
    /// 后台 `fallback_commands` 拉取结果接收端（`Some` = 有拉取在途）。
    pub(crate) fallback_rx: Option<Receiver<FallbackFetchOutcome>>,
    /// path 图标纹理缓存：路径 → TextureHandle（每路径只读盘+解码一次，
    /// 避免列表每次重绘都重复 I/O 与解码——设计稿 04"按路径缓存 textureId"）。
    pub(crate) icon_cache: HashMap<String, (egui::TextureHandle, bool)>,
    /// Path 图标读盘/解码失败的负缓存（路径集）：失败只报一次 + 回落占位 glyph，
    /// 否则每帧重读盘 → eprintln 刷屏（真机 2026-09-05 反馈）。
    pub(crate) icon_failed: HashSet<String>,
    /// M5 批次 4.0：宿主本地设置（当前仅主题偏好；启动加载、设置页改选即存）。
    pub(crate) settings: dd_gui::settings::Settings,
    /// 当前窗口是否已按设置页尺寸调整（帧间 diff，仅在进/出设置页时发
    /// `InnerSize`，避免每帧塞 ViewportCommand）。
    pub(crate) settings_sized: bool,
    /// 打开中的右键菜单（设计稿 10B，v4.4；`None` = 关闭）。
    pub(crate) ctx_menu: Option<CtxMenuState>,
    /// 键盘触发旗标：Shift+F10 请求对选中行开菜单（D19）。行矩形只在绘制期
    /// 可得，`handle_keys` 置位、`draw_list` 在绘制选中行后消费落位。
    pub(crate) want_ctx_menu_for_selected: bool,
    /// 本帧各列表行矩形（`draw_list` 每帧重建）：菜单开着时右键另一行，全屏
    /// 捕获层会吞掉该行的 `secondary_clicked`（D19 修正），据此命中行就地重开。
    pub(crate) ctx_row_rects: Vec<(usize, egui::Rect)>,
}

impl PaletteApp {
    pub fn new(
        events: Receiver<HotkeyEvent>,
        tray_events: Receiver<TrayEvent>,
        aggregate_rx: Receiver<AggregatePayload>,
        cold: ColdStartTimer,
        settings: dd_gui::settings::Settings,
    ) -> Self {
        Self {
            stack: PageStack::new(PageState::root(Vec::new())),
            events,
            tray_events,
            visible: false,
            want_focus: true,
            ever_focused: false,
            aggregate_rx: Some(aggregate_rx),
            aggregating: true,
            sources: Vec::new(),
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
            crash_guards: HashMap::new(),
            fallback_store: dd_gui::fallback::FallbackStore::new(),
            fallback_rx: None,
            icon_cache: HashMap::new(),
            icon_failed: HashSet::new(),
            settings,
            settings_sized: false,
            ctx_menu: None,
            want_ctx_menu_for_selected: false,
            ctx_row_rects: Vec::new(),
        }
    }
}

impl eframe::App for PaletteApp {
    /// 窗口隐藏时也会被调用（热键线程 `request_repaint` 唤醒）。
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 注意：启动期不再有 recenter 逐帧轮询（旧 `recenter_if_needed` 已删）。
        // 窗口初始在屏幕外物理不可见（OFFSCREEN_*），首次及每次唤起由
        // `show() → send_center_on_cursor` 一次性定位到光标屏——隐藏期间
        // 不发任何位置命令（设计稿 03 验收：窗口隐藏期间不发送位置命令）。
        self.poll_hotkey(ctx);
        self.poll_tray(ctx);
        self.handle_focus_loss(ctx);
        // A8：每帧健康检查——面板可见期间扩展崩溃也能及时移除（此前仅 show() 时查一次，
        // 面板一直开着时崩溃的进程会滞留到下次唤起才被清理）。
        self.refresh_health();
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
        self.poll_host_requests(); // M4 P2：host/* 副作用（Toast/剪贴板/开 URL）
        self.poll_fallback(&ctx); // M4 宿主 fallback：兜底模板拉取结果
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
        // 设置页与启动页窗口尺寸不同：按当前栈顶页帧间 diff 同步 `InnerSize`。
        // 进设置页放大、返回/出栈/清栈缩回——所有路径（Esc/Dismiss/show 复位）
        // 都经此处收口，不依赖各转换点逐一接线。
        let want_settings = self.stack.current().is_settings;
        if want_settings != self.settings_sized {
            self.settings_sized = want_settings;
            let (w, h) = if want_settings {
                (SETTINGS_W, SETTINGS_H)
            } else {
                (APP_W, APP_H)
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
        }
        self.draw_panel(ui);
        self.draw_toast(&ctx);
        self.draw_confirm(&ctx);
        self.draw_context_menu(&ctx, ui);
    }
}
