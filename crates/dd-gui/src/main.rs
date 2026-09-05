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

// 发布版（release）不弹命令行控制台（GUI 程序，日志随 stderr 丢弃）；
// debug / 测试构建保留控制台，便于看 eprintln 探针与冷启动计时。
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//!
//! 2026-09-05 分层重构（方案 1，docs/refactor-layering-plan.md）：实现已拆分至
//! dd_gui::{app, ui, platform, text}（业务/绘制/系统副作用/纯函数），
//! 本文件仅保留进程入口 main()。

use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;

use eframe::egui;

use dd_gui::app::{spawn_aggregation, PaletteApp, APP_H, APP_W, OFFSCREEN_X, OFFSCREEN_Y};
use dd_gui::hotkey::HotkeyThread;
use dd_gui::platform::setup_cjk_fonts;
use dd_gui::theme;
use dd_gui::tray::TrayThread;
use dd_host::cache::{ColdStartTimer, FrozenCache};
use dd_host::manifest;

fn main() -> eframe::Result {
    // A2 冷启动计时起点：**进程进入 main 即开始**，覆盖 eframe/wgpu 窗口创建 +
    // 字体加载（msyh.ttc ~19.7MB + seguisym 2.5MB）+ 聚合全过程。
    // 之前放在 setup 闭包内、且位于 setup_cjk_fonts 之后，把最重的字体加载整段漏掉了
    // （实测只剩 4~6 ms，测的几乎什么都不是）。
    let mut cold = ColdStartTimer::new();
    cold.mark_spawn_start();
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([APP_W, APP_H])
        // v4.7 D31：透明视觉恒创建——材质生效时 DWM 系统材质从面板透明底后
        // 透出；未生效（Win10 / 22621 以下回退）时面板不透明填充完整覆盖，
        // 视觉与既往一致。
        .with_transparent(true)
        // 启动静默双保险之二（其一为 with_visible(false)）：初始位置屏幕外 +
        // 初始不活跃。屏幕外位置保证 eframe 首帧强制 set_visible(true) 时
        // 用户看不到任何窗口；with_active(false) 防止强制显示瞬间夺取焦点
        // （焦点要等用户热键唤起时才给，设计稿 03 验收行）。
        .with_position([OFFSCREEN_X, OFFSCREEN_Y])
        .with_active(false)
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
            // M5 批次 3：注册亮/暗双视觉 + 主题偏好（批次 4.0 起偏好来自
            // 持久化配置，缺省跟随系统；须在任何绘制前应用，egui 会按
            // 系统亮暗自动在两套 Style 间 re-resolve）。
            let settings = dd_gui::settings::Settings::load();
            // v4.7 D31：启动先按不透明注册（HWND 未捕获、材质成败未知）；
            // 首个 ui 帧捕获 HWND 后由 refresh_backdrop 按结果切换透明性。
            theme::apply(&cc.egui_ctx, theme::theme_preference(settings.theme), false);
            // 初始隐藏双保险：`with_visible(false)` 之外显式发 `Visible(false)`，
            // 规避 eframe/egui 0.36 在 Windows 上 `with_visible` 偶发不生效的情况。
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(false));
            let hotkey = HotkeyThread::spawn(cc.egui_ctx.clone());
            // 系统托盘（设计稿 10C：常驻图标 + 左键 toggle + 右键原生菜单 +
            // 唯一退出入口；失败线程内降级，不影响面板）。click_flag = Toggle
            // 点击在途旗标（修复失焦隐藏与 Toggle 的「闪黑又展示」竞态）。
            let tray_click_flag = Arc::new(AtomicBool::new(false));
            let tray = TrayThread::spawn(cc.egui_ctx.clone(), Arc::clone(&tray_click_flag));
            // M3 磁盘桩缓存（读桩不拉起进程 A6；目录 = 数据根目录/cache）
            // PaletteApp 持有一份副本：设置页搜索引擎变更触发重聚合时复用。
            let cache = manifest::cache_dir().map(FrozenCache::new);
            // M3 A2 冷启动计时起点：进程就绪即开始（聚合线程随后启动）
            let mut cold = ColdStartTimer::new();
            cold.mark_spawn_start();
            // 可配置搜索引擎（2026-09-05）：spawn 前把引擎表注入 websearch
            // 扩展环境（DD_WEBSEARCH_ENGINES），面板「网络搜索」按配置渲染。
            let engines_env = settings.search_engines_env();
            let (agg_tx, agg_rx) = mpsc::channel();
            spawn_aggregation(agg_tx, cache.clone(), engines_env);
            Ok(Box::new(PaletteApp::new(
                hotkey.events,
                tray.events,
                tray_click_flag,
                agg_rx,
                cold,
                cache,
                settings,
            )))
        }),
    )
}
