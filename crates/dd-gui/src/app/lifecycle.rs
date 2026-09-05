//! 窗口生命周期：显示/隐藏/失焦、热键与托盘事件轮询。

use crate::app::PaletteApp;
use dd_gui::hotkey::HotkeyEvent;
use dd_gui::tray::TrayEvent;
use eframe::egui;

impl PaletteApp {
    // ── 窗口可见性 ───────────────────────────────────────────

    pub(crate) fn show(&mut self, ctx: &egui::Context) {
        self.visible = true;
        self.ever_focused = false;
        self.want_focus = true;
        // 复位语义（协议 §8.3 Hide/Dismiss 区分）：
        // - 用户主动隐藏（Esc/热键/失焦）→ 复位（M1 §4 清单第 10 项）；
        // - 扩展 `Hide` → 保留状态不复位（再次唤起仍在当前页/查询）；
        // - 扩展 `Dismiss` → 已在 dismiss() 清空，复位为空操作。
        if self.reset_on_show {
            // 嵌套页一并出栈回 Root：设置页打开时失焦/Esc 之外路径隐藏（热键
            // Toggle）后再次唤起，必须回到首屏而非停留在设置页（真机反馈）。
            self.stack.go_home();
            self.stack.root_mut().list.reset();
        }
        // 每次唤起都在**光标所在屏居中**（grill 决策 A1；PowerToys Run 行为）。
        // 不能复用 egui `center_on_screen`：它按窗口**当前所在 monitor** 居中，
        // 而启动期窗口被放到屏幕外（OFFSCREEN）→ 会取错屏居中到负象限。
        // 这里用 Win32 `GetCursorPos + MonitorFromPoint` 自算目标屏工作区。
        self.send_center_on_cursor(ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    pub(crate) fn hide(&mut self, ctx: &egui::Context) {
        self.visible = false;
        self.want_focus = false;
        self.reset_on_show = true; // 用户主动隐藏：默认下次唤起复位
                                   // 右键菜单随窗口隐藏一并关闭（浮层不跨隐藏周期存活）
        self.ctx_menu = None;
        self.want_ctx_menu_for_selected = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    /// 扩展请求 `Dismiss`（协议 §8.3：关闭面板）：清空页面栈回 Root 再隐藏，
    /// 下次唤起回到首页——与 `Hide`（保留状态）形成可观察区别。
    pub(crate) fn dismiss(&mut self, ctx: &egui::Context) {
        eprintln!("[dd-gui] Dismiss：清空页面栈回 Root 后隐藏");
        self.stack.go_home();
        self.stack.root_mut().list.reset();
        self.hide(ctx);
    }

    /// 扩展请求 `Hide`（协议 §8.3：隐藏但不关闭、保留状态）：
    /// 下次唤起不复位查询与选中，仍回到调用时的页面栈位置。
    pub(crate) fn hide_keep_state(&mut self, ctx: &egui::Context) {
        eprintln!("[dd-gui] Hide：保留状态隐藏（下次唤起不复位）");
        self.hide(ctx);
        self.reset_on_show = false;
    }

    pub(crate) fn poll_hotkey(&mut self, ctx: &egui::Context) {
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

    /// 托盘事件（设计稿 10C.2 菜单项 → 行为映射）。
    pub(crate) fn poll_tray(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.tray_events.try_recv() {
            match ev {
                // D23：左键 / 「显示/隐藏面板」= 与热键同一 toggle 入口。
                TrayEvent::Toggle => {
                    if self.visible {
                        self.hide(ctx);
                    } else {
                        self.show(ctx);
                    }
                }
                // 10C.2「设置」：显示面板并 in-place 切到设置视图（D3）；
                // 已可见时不复位（保留当前页面栈，直接推设置页，与 Ctrl+, 同款）。
                TrayEvent::OpenSettings => {
                    if !self.visible {
                        self.show(ctx);
                    }
                    self.open_settings();
                }
                // 10C.2「退出」：唯一显式退出入口；关闭窗口 → run_native 返回
                // → main 返回 → 进程结束（托盘图标由系统随进程死亡移除）。
                TrayEvent::Exit => {
                    eprintln!("[dd-gui] 托盘菜单：退出（结束进程）");
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
}

impl PaletteApp {
    /// 失焦自动隐藏（设计文档 §4.3 / 界面 01）。
    pub(crate) fn handle_focus_loss(&mut self, ctx: &egui::Context) {
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
}
