//! 键盘导航与设置页动作入口。

use crate::app::ctx_menu::ctx_entry_count;
use crate::app::PaletteApp;
use crate::ui::settings_view::SettingsCategory;
use dd_gui::navigation::PageState;
use dd_gui::theme;
use eframe::egui;
use std::sync::mpsc;

impl PaletteApp {
    // ── 键盘 ─────────────────────────────────────────────────

    /// 应用层拦截导航键（`consume_key` 移除事件，FilterBox 的 TextEdit 收不到
    /// → 输入光标不动）。设计文档 §4.3：`↑/↓` **或** `Tab/Shift+Tab` 移动、
    /// `Enter` 执行、`Esc` 关闭或返回上一级。
    pub(crate) fn handle_keys(&mut self, ctx: &egui::Context) {
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
        // 批次 4.0：Ctrl+, 打开设置（§6.1 快捷键；设置入口的键盘可达手段
        // ——Tab 保持列表导航语义不变，见 implementation.md 批次 4.0 决策）。
        // 在确认对话框分支之后处理：对话框活跃时该键被吞掉不穿透。
        let ctrl_comma = ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Comma));
        // v4.4（D19）：Shift+F10 对选中行打开右键菜单（egui 0.36 键表无 Menu
        // 键，Shift+F10 为 Windows 菜单键的等价惯例——偏离记档：Menu 键不可达）。
        let shift_f10 = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F10));

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

        // 右键菜单打开时：键盘上下文移交菜单（10B.1 键盘行）——↑↓ 移动焦点、
        // Enter 激活、Esc 关闭并回到列表；Tab/Ctrl+, 一并吞掉不穿透（D19 冻结）。
        if self.ctx_menu.is_some() {
            if esc {
                self.ctx_menu = None;
            } else if enter {
                self.activate_ctx_menu(ctx);
            } else if down || tab {
                let n = ctx_entry_count(self.ctx_menu.as_ref());
                if n > 0 {
                    let state = self.ctx_menu.as_mut().expect("菜单存在");
                    state.focus = (state.focus + 1) % n;
                }
            } else if up || shift_tab {
                let n = ctx_entry_count(self.ctx_menu.as_ref());
                if n > 0 {
                    let state = self.ctx_menu.as_mut().expect("菜单存在");
                    state.focus = (state.focus + n - 1) % n;
                }
            }
            return;
        }

        // v4.4（D19）：Shift+F10 = 对选中行打开菜单。行矩形只在绘制期可得，
        // 这里仅置旗标，`draw_list` 绘制选中行后消费落位（锚定行底边左缘）。
        if shift_f10 {
            self.want_ctx_menu_for_selected = true;
            ctx.request_repaint();
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
        if ctrl_comma {
            self.open_settings();
        }
    }

    /// 打开设置页（批次 4.0）：推入页面栈（复用嵌套页语义，Esc 返回）。
    /// 已在设置页时幂等（不重复推栈）。每次进入重置左栏栏目到首栏「外观」
    /// （§08 v4.6 B5：栏目为纯视图状态，与 go_home 复位语义一致）。
    pub(crate) fn open_settings(&mut self) {
        if self.stack.current().is_settings {
            return;
        }
        eprintln!("[dd-gui] 打开设置页（PageStack 推页）");
        self.settings_category = SettingsCategory::default();
        self.stack.push(PageState::settings());
    }

    /// 设置页改选主题（批次 4.0）：立即生效 + 持久化（best-effort）。
    pub(crate) fn apply_theme_pref(
        &mut self,
        ctx: &egui::Context,
        pref: dd_gui::settings::ThemePref,
    ) {
        if self.settings.theme == pref {
            return;
        }
        eprintln!("[dd-gui] 主题偏好变更：{} → 立即生效并保存", pref.label());
        self.settings.theme = pref;
        ctx.set_theme(theme::theme_preference(pref));
        self.settings.save();
    }

    /// 设置页改选「打开面板时显示」：立即生效（重算 root 首屏可见表）+ 持久化。
    pub(crate) fn apply_open_view(&mut self, ctx: &egui::Context, show_all: bool) {
        let view = if show_all {
            dd_gui::settings::OpenView::All
        } else {
            dd_gui::settings::OpenView::Default
        };
        if self.settings.open_view == view {
            return;
        }
        eprintln!("[dd-gui] 首屏视图变更：{}", view.label());
        self.settings.open_view = view;
        self.stack.root_mut().list.set_empty_view(if show_all {
            dd_gui::state::EmptyQueryView::All
        } else {
            dd_gui::state::EmptyQueryView::WithoutApps
        });
        self.settings.save();
        ctx.request_repaint();
    }

    /// 设置页搜索引擎变更（勾选预设/添加/删除自定义，2026-09-05）：
    /// 立即持久化 + 置脏标记；**离开设置页时**由 `ui()` 的 size-diff 收口点
    /// 消费并全量重聚合（websearch 进程须以新环境变量重启才能生效）。
    pub(crate) fn apply_search_engines(&mut self, ctx: &egui::Context) {
        eprintln!(
            "[dd-gui] 搜索引擎配置变更：{} 个引擎已保存（离开设置页后重新聚合生效）",
            self.settings.search_engines.len()
        );
        self.engines_dirty = true;
        self.settings.save();
        ctx.request_repaint();
    }

    /// 搜索引擎配置变更后的全量重聚合：重走 scan → 注入引擎环境 → collect
    /// → 替换 Root 列表（复用首屏聚合的全部既有机制，含进程替换与 LRU）。
    pub(crate) fn restart_aggregation(&mut self) {
        eprintln!("[dd-gui] 搜索引擎配置变更 → 重新聚合首屏");
        let (tx, rx) = mpsc::channel();
        crate::app::spawn_aggregation(tx, self.cache.clone(), self.settings.search_engines_env());
        self.aggregate_rx = Some(rx);
        self.aggregating = true;
    }
}
