//! 窗口生命周期：显示/隐藏/失焦、热键与托盘事件轮询。

use crate::app::PaletteApp;
use crate::app::{root_panel_size, settings_panel_size, APP_H, APP_W, SIZE_EPSILON};
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
        // v4.12 D37：先取光标屏工作区（逻辑点）驱动本屏自适应尺寸 clamp；
        // 单帧内指针不动，与下方居中取同一屏（见 `platform::cursor_work_area`）。
        let work = self.cursor_work_area(ctx);
        self.last_work_area = work;
        // v4.12 D37（推翻 v4.10 D36④「唤起即回默认尺寸」半条）：唤起尺寸 =
        // 基准 650×420 或记忆值（`settings.panel_size`，拉伸落盘）按目标屏
        // 工作区 clamp；设置页打开态隐藏后再唤起（Hide 保留状态）仍按
        // 设置页有效尺寸。与 `ui()` 的 settings_sized diff 收口同口径：
        // 先同步旗标防重复发送。
        let want_settings = self.stack.current().is_settings;
        self.settings_sized = want_settings;
        let (w, h) = if want_settings {
            settings_panel_size(self.last_work_area, self.settings.panel_size)
        } else {
            root_panel_size(self.last_work_area, self.settings.panel_size)
        };
        self.shown_size = Some((w, h));
        // 按**目标尺寸**居中（不再读 stale `inner_rect`）：重开设置页后回到根页
        // 时，用根页尺寸定位而非残留的设置页大尺寸，消除位置跳动（issue：
        // 「切换设置项后关闭重开面板位置变化」）。
        self.center_on_cursor(ctx, (w, h));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    pub(crate) fn hide(&mut self, ctx: &egui::Context) {
        // v4.12 D37：隐藏前落盘本显示周期的拉伸尺寸（best-effort，见
        // `persist_panel_size`）——此时窗口尺寸即本周期最终尺寸。
        self.persist_panel_size(ctx);
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

    /// v4.12 D37 ②：把当前窗口尺寸落盘为记忆值（`settings.panel_size`）。
    ///
    /// 判定口径：仅当当前尺寸**偏离 `show()` 时程序设定的 `shown_size`**
    /// （> [`SIZE_EPSILON`]）才落盘——该偏离只可能来自用户 8 向缩放（D36），
    /// 小屏 clamp 的程序尺寸不会被误存为用户记忆。当前尺寸等于基准
    /// `APP_W/APP_H` 时存 `None`（「从未拉伸」语义：用户缩回基准后，日后
    /// 基准调整不残留旧记录）。
    ///
    /// 仅根页/子页态落盘；设置页态的窗口尺寸属设置页，不入根页记忆。
    /// 落盘时机 = `hide()` / 托盘退出——拖拽缩放期间不逐帧写盘（防高频
    /// IO；取舍记档 D37：进程被强杀时丢最后一次拉伸）。
    pub(crate) fn persist_panel_size(&mut self, ctx: &egui::Context) {
        if self.stack.current().is_settings {
            return;
        }
        let Some(shown) = self.shown_size else {
            return; // 本进程尚未 show() 过：无参照，不落盘
        };
        let cur = ctx.input(|i| i.viewport().inner_rect.map(|r| (r.width(), r.height())));
        let Some((w, h)) = cur else {
            return; // inner_rect 不可得（无头/早期帧）：跳过
        };
        if (w - shown.0).abs() <= SIZE_EPSILON && (h - shown.1).abs() <= SIZE_EPSILON {
            return; // 未偏离程序设定值 = 用户未拉伸
        }
        let rounded = (w.round().max(1.0) as u32, h.round().max(1.0) as u32);
        let baseline = (APP_W.round() as u32, APP_H.round() as u32);
        let next = if rounded == baseline {
            None
        } else {
            Some(rounded)
        };
        if next != self.settings.panel_size {
            eprintln!(
                "[dd-gui] 面板尺寸落盘：{w:.0}×{h:.0}（记忆 {}）",
                {
                    match next {
                        Some((sw, sh)) => format!("{sw}×{sh}"),
                        None => "清除".to_string(),
                    }
                }
            );
            self.settings.panel_size = next;
            self.settings.save();
        }
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
                    // v4.12 D37：退出前兜底落盘拉伸尺寸（面板可见时直接退出
                    // 不经 hide()——hide 落盘会漏掉这一路径）。
                    self.persist_panel_size(ctx);
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
