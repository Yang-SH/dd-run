//! 绘制层：入参 `(&mut Ui, theme::Palette, 数据)` → 像素。不含业务状态变更。
//!
//! 拆分自原 main.rs（docs/refactor-layering-plan.md 方案 1），方法体逐字未改。

pub(crate) mod chrome;
pub(crate) mod confirm;
pub(crate) mod context_menu;
pub(crate) mod icons;
pub(crate) mod panel;
pub(crate) mod row;
pub(crate) mod settings_view;
pub(crate) mod states;
pub(crate) mod toast;
pub(crate) mod widgets;
