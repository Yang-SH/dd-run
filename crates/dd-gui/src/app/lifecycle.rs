//! 窗口生命周期：显示/隐藏/失焦、热键与托盘事件轮询。

use crate::app::PaletteApp;
use dd_gui::hotkey::HotkeyEvent;
use dd_gui::tray::TrayEvent;
use eframe::egui;
use std::sync::atomic::Ordering;
use std::time::Instant;

impl PaletteApp {
    // ── 窗口可见性 ───────────────────────────────────────────

    pub(crate) fn show(&mut self, ctx: &egui::Context) {
        self.visible = true;
        self.paint_hide_frame = false; // 显示时清掉可能残留的隐藏帧标记
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
        // 隐藏当帧继续绘制真实内容一次，避免 present 纯色空帧的「闪黑」
        // （见 `paint_hide_frame` 字段注释）。
        self.paint_hide_frame = true;
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
        while let Ok(ev) = self.hotkey.events.try_recv() {
            match ev {
                HotkeyEvent::Toggle => {
                    if self.visible {
                        self.hide(ctx);
                    } else {
                        self.show(ctx);
                    }
                }
                HotkeyEvent::ReRegistered(ok) => {
                    // M6 批次 6.3：重注册结果。成功 → 提示并清回滚备份；失败
                    //（组合键被占用）→ 还原设置为旧键 + 命令线程回滚旧热键。
                    if ok {
                        self.hotkey_prev = None;
                        self.show_toast("全局热键已更新", None);
                    } else if let Some(old) = self.hotkey_prev.take() {
                        self.settings.hotkey_mods = old.0;
                        self.settings.hotkey_vk = old.1;
                        self.settings.save();
                        self.hotkey.re_register(old.0, old.1);
                        self.show_toast("新热键注册失败（可能被占用），已恢复原热键", None);
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
                // Toggle 事件消费即复位「点击在途」旗标（与 tray.rs 置位严格成对）。
                TrayEvent::Toggle => {
                    self.tray_click_flag.store(false, Ordering::Relaxed);
                    if self.visible {
                        self.hide(ctx);
                    } else if self.hidden_by_recent_focus_loss() {
                        // 刚因失焦隐藏（本次托盘点击在鼠标按下瞬间夺焦，早于
                        // WM_LBUTTONUP 的 Toggle 到达）：隐藏意图已由失焦路径
                        // 完成，维持隐藏——否则 hide→show = 「闪黑又展示」。
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
            // 托盘 Toggle 点击在途：本次失焦由用户点击托盘引起，隐藏交给
            // poll_tray 的 Toggle 完成一次干净 hide——否则失焦先 hide、Toggle
            // 再 show = 「闪黑又展示」竞态（真机 2026-09-05 反馈，10C D23）。
            if self.tray_click_flag.load(Ordering::Relaxed) {
                return;
            }
            self.last_focus_loss_hide = Some(Instant::now());
            self.hide(ctx);
        }
    }

    /// 面板是否刚因失焦自动隐藏（<300ms）。
    ///
    /// 托盘 Toggle 的兜底判据：点击托盘时任务栏在鼠标**按下**瞬间夺焦
    /// （失焦隐藏可能先于 WM_LBUTTONUP → Toggle 到达主线程），此时隐藏意图
    /// 已由失焦路径完成，Toggle 不应再 show（见 poll_tray）。
    fn hidden_by_recent_focus_loss(&self) -> bool {
        self.last_focus_loss_hide
            .is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(300))
    }
}
