//! dd-gui：dd-run 宿主面板 UI。
//!
//! 分层：
//! - [`state`]：面板状态机纯逻辑（不依赖 egui，可单测）——R1 尖峰的
//!   "逻辑自动化"部分；
//! - [`hotkey`]：Windows 全局热键（`RegisterHotKey`，独立线程消息循环）；
//! - [`aggregator`]：首屏聚合（扫描扩展 → 并行拉取 `top_level_commands`
//!   → 合并为可渲染项），错误隔离 + 示例扩展兜底；
//! - [`navigation`]：页面栈（Root / 嵌套页，`GoBack`/`GoHome` 导航，A5）；
//! - [`result`]：8 种 `CommandResultKind` 裁决（A4）+ `Confirm` 挂起重发
//!   + `invoke` 参数构造；
//! - `main`（bin）：eframe/egui 窗口骨架（无边框、置顶、失焦隐藏、
//!   初始隐藏），FilterBox + 分组列表 + 页脚键位提示，对应设计稿界面 01。

pub mod aggregator;
pub mod hotkey;
pub mod navigation;
pub mod result;
pub mod state;
