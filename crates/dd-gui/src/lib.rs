//! dd-gui：dd-run 宿主面板 UI。
//!
//! 分层：
//! - [`state`]：面板状态机纯逻辑（不依赖 egui，可单测）——R1 尖峰的
//!   "逻辑自动化"部分；
//! - [`hotkey`]：Windows 全局热键（`RegisterHotKey`，独立线程消息循环）；
//! - [`tray`]：Windows 系统托盘（设计稿 10C：左键 toggle / 右键原生菜单 /
//!   唯一退出入口，独立线程隐藏窗口 + `Shell_NotifyIconW`）；
//! - [`aggregator`]：首屏聚合（扫描扩展 → 并行拉取 `top_level_commands`
//!   → 合并为可渲染项），错误隔离 + 示例扩展兜底；
//! - [`navigation`]：页面栈（Root / 嵌套页，`GoBack`/`GoHome` 导航，A5）；
//! - [`result`]：8 种 `CommandResultKind` 裁决（A4）+ `Confirm` 挂起重发
//!   + `invoke` 参数构造；
//! - [`fallback`]：M4 兜底命令模板缓存与渲染（协议 §6.2：`{query}` 占位符
//!   替换；每扩展只拉一次，全局无匹配时展示）；
//! - [`fuzzy`]：M4 P5 nucleo 模糊匹配封装（D3-A：子序列打分，多字段取最高，
//!   大小写不敏感；拼音留作后续独立项）；
//! - [`robustness`]：M4 崩溃保护状态机（协议 §11：连续崩溃 N 次 → 熔断"暂时
//!   不可用"，恢复后清零；验收 A8）；
//! - [`theme`]：M5 批次 3 ueli 皮肤 token 源（设计稿 05 表 → 亮/暗双 `Visuals`；
//!   绘制层经 [`theme::Palette`] 取组件色，本模块是唯一 token 源）；
//! - [`embedded`]：M6 单文件分发——把 5 个内置扩展 exe 内嵌进宿主（build.rs +
//!   `assets/embed/`），运行时物化到缓存目录供 spawn（进程隔离不变）；
//! - [`app`]：业务编排（PaletteApp 状态机、进程池、协议时序；拆分自 main.rs）；
//! - [`ui`]：绘制层（面板/列表/设置页/浮层；拆分自 main.rs）；
//! - [`platform`]：系统副作用（字体加载、窗口定位、提权/资源管理器动作）；
//! - [`text`]：纯文本函数（路径/URL 形态、glyph 映射，零 egui 依赖）；
//! - `main`（bin）：eframe/egui 窗口骨架（无边框、置顶、失焦隐藏、
//!   初始隐藏），FilterBox + 分组列表 + 页脚键位提示，对应设计稿界面 01。

// ── 2026-09-05 分层重构（docs/refactor-layering-plan.md 方案 1）──────────
// main.rs 的实现拆入以下四个模块；`extern crate self` 使 lib 内部代码可以
// 沿用 `dd_gui::` 全路径（与拆分前逐字一致，零行为影响）。
extern crate self as dd_gui;

pub mod aggregator;
pub mod app;
pub mod embedded;
pub mod fallback;
pub mod fuzzy;
pub mod hotkey;
pub mod navigation;
pub mod platform;
pub mod result;
pub mod robustness;
pub mod settings;
pub mod state;
pub mod text;
pub mod theme;
pub mod tray;
pub mod ui;

#[cfg(test)]
pub(crate) mod test_support;
