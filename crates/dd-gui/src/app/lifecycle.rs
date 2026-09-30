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
        log::debug!("[dd-gui] show()：visible→false→true，重置状态并唤起");
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
        let row_h = dd_gui::theme::ListMetrics::of(self.settings.density).row_h;
        let (w, h) = if want_settings {
            settings_panel_size(self.last_work_area, self.settings.panel_size, row_h)
        } else {
            root_panel_size(self.last_work_area, self.settings.panel_size, row_h)
        };
        self.shown_size = Some((w, h));
        // 按**目标尺寸**居中（不再读 stale `inner_rect`）：重开设置页后回到根页
        // 时，用根页尺寸定位而非残留的设置页大尺寸，消除位置跳动（issue：
        // 「切换设置项后关闭重开面板位置变化」）。
        self.center_on_cursor(ctx, (w, h));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        // v4.17a 真机修复：唤起时重置指针基准 + hover 基准。
        // 面板隐藏期间指针可能已移到别处，若沿用上次遗留坐标，首帧
        // `last_pointer_pos != current_hover_pos` 会被判成"鼠标移动过"→ hover
        // 行接管选中，表现为「输入文字时鼠标在面板中，默认选中了鼠标所在项」
        // 而非第一项。置 `None` = 首帧只采样不判活动（见 `draw_list` 的
        // `pointer_moved` 匹配分支）。
        self.last_pointer_pos = None;
        self.last_hovered_index = None;
        // v4.17a：面板唤起后默认隐藏鼠标（静止 → 隐藏，鼠标一动即恢复，
        // 由 `MouseHideScope::apply` 每帧按闲置时长判定）。take() 兜底：
        // 若守卫意外残留（旧 show 未配对 hide），先释放再构造新的。
        if let Some(stale) = self.mouse_hide.take() {
            stale.release();
        }
        self.mouse_hide = Some(crate::platform::MouseHideScope::new());
    }

    pub(crate) fn hide(&mut self, ctx: &egui::Context) {
        // v4.16 真机修复（拖拽后面板空白）：原生拖拽/缩放模态循环在途时，
        // `Visible(false)`（ShowWindow SW_HIDE）会被 Windows 静默忽略 →
        // 应用态 visible=false 而窗口仍可见，ui() 早返回只呈现透明帧 =
        // 纯材质空白底（真机截图形态）。故循环在途时推迟 hide，由
        // `chrome_begin` 在循环结束后补执行。
        if self.native_resize {
            self.hide_pending = true;
            log::debug!("[dd-gui] hide()：原生模态循环在途 → 推迟至循环结束后执行");
            return;
        }
        log::debug!("[dd-gui] hide()：visible→false（Visible(false) 排队）");
        self.panel_open.store(false, Ordering::Relaxed);
        self.hide_desync_since = None;
        // v4.17 亚克力体验优化：面板关闭时 take 鼠标守卫并**立即恢复默认光标**。
        // 主动恢复是因为面板隐藏后 ui() 早返回（`paint_hide_frame` 之后不再
        // 调 apply），单靠下一帧 apply() 兜底不够稳。模态循环在途的 hide() 已
        // early return（守卫继续 hold），chrome_begin 补执行本函数时再 take。
        if self.mouse_hide.take().is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::Default);
        }
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
        // 热键捕获是模态交互，不跨隐藏周期存活（真机 bug 2026-09-29：捕获期
        // 按 Win 弹开始菜单 → 失焦 hide → 再唤起仍卡在捕获态）。`capture_hook`
        // 置 None 同时卸载 LL 钩子（守卫 Drop）——失焦隐藏是钩子的逃生出口。
        self.end_hotkey_capture();
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
            log::debug!(
                "[dd-gui] 面板尺寸落盘：{w:.0}×{h:.0}（记忆 {}）",
                {
                    match next {
                        Some((sw, sh)) => format!("{sw}×{sh}"),
                        None => "清除".to_string(),
                    }
                }
            );
            self.settings.panel_size = next;
            self.save_settings_with_feedback();
        }
    }

    /// 扩展请求 `Dismiss`（协议 §8.3：关闭面板）：清空页面栈回 Root 再隐藏，
    /// 下次唤起回到首页——与 `Hide`（保留状态）形成可观察区别。
    pub(crate) fn dismiss(&mut self, ctx: &egui::Context) {
        log::debug!("[dd-gui] Dismiss：清空页面栈回 Root 后隐藏");
        self.stack.go_home();
        self.stack.root_mut().list.reset();
        self.hide(ctx);
    }

    /// 扩展请求 `Hide`（协议 §8.3：隐藏但不关闭、保留状态）：
    /// 下次唤起不复位查询与选中，仍回到调用时的页面栈位置。
    pub(crate) fn hide_keep_state(&mut self, ctx: &egui::Context) {
        log::debug!("[dd-gui] Hide：保留状态隐藏（下次唤起不复位）");
        self.hide(ctx);
        self.reset_on_show = false;
    }

    pub(crate) fn poll_hotkey(&mut self, ctx: &egui::Context) {
        // 确认超时兜底：正常回发毫秒级；线程死亡（Died 分支也会清）或消息
        // 丢失时不至于永久「应用中」。超时 = 视同失败（设置未写，无需回滚）。
        if let Some(c) = &self.hotkey_confirm {
            if c.started.elapsed() > super::HOTKEY_CONFIRM_TIMEOUT {
                log::warn!("[dd-gui] 热键改绑确认超时（seq={}）——视同失败", c.seq);
                self.hotkey_confirm = None;
                self.hotkey_apply_failed = true;
            }
        }
        while let Ok(ev) = self.hotkey.events.try_recv() {
            match ev {
                HotkeyEvent::Toggle => {
                    // 捕获期守卫（2026-09-30 真机）：回落（egui）捕获模式下按
                    // 键不被系统级吞掉，按下**当前已注册**的组合键仍会送达
                    // WM_HOTKEY → 面板即时切换隐藏 → hide() 复位捕获态 → 对
                    // 话框销毁（「按键后界面隐藏，不能正常进行设置」）。钩子
                    // 模式下全键盘被吞、WM_HOTKEY 不会产生，本守卫对钩子模
                    // 式零行为变化。捕获结束后新热键照常切换。
                    if self.hotkey_capturing {
                        log::debug!("[dd-gui] 捕获期忽略热键 Toggle（回落捕获下旧热键仍存活）");
                        continue;
                    }
                    if self.visible {
                        self.hide(ctx);
                    } else {
                        self.show(ctx);
                    }
                }
                HotkeyEvent::ReRegistered {
                    seq,
                    ok,
                    rolled_back,
                } => {
                    // seq 配对（2026-09-30 协议化）：事件只对「同 seq 的在途
                    // 确认」生效；过期/未知事件一律忽略（旧实现按到达顺序
                    // 盲配 + 单槽快照，重试场景错位——真机「冲突第二次假
                    // 成功」根因，本分支为根治后的唯一裁决点）。
                    if seq == dd_gui::hotkey::HOTKEY_SEQ_STARTUP {
                        // 启动注册结果（线程 spawn 失败 / 启动注册失败）。
                        // R-15：置「未注册」状态位（设置页徽标）+ 错误 toast。
                        if !ok {
                            self.hotkey_unregistered = true;
                            self.show_error_toast(
                                crate::text::t(self.lang_effective, "toast.hotkey_unregistered")
                                    .to_string(),
                            );
                        }
                        continue;
                    }
                    let Some(c) = self.hotkey_confirm.filter(|c| c.seq == seq) else {
                        log::debug!(
                            "[dd-gui] 忽略过期/未知热键事件（seq={seq}，ok={ok}，rolled_back={rolled_back}）"
                        );
                        continue;
                    };
                    if ok {
                        // 注册成功 → 此时才写设置（唯一写点）+ 成功 toast +
                        // 复位「未注册」位 + 退出捕获（对话框随 capture 态关闭）。
                        self.settings.hotkey_mods = c.mods;
                        self.settings.hotkey_vk = c.vk;
                        self.save_settings_with_feedback();
                        self.hotkey_confirm = None;
                        self.hotkey_unregistered = false;
                        let combo = format!(
                            "{}+{}",
                            dd_gui::settings::hotkey_mods_label(c.mods),
                            dd_gui::settings::hotkey_vk_label(c.vk),
                        );
                        // R-18：toast 入 i18n + 带新组合名 + 点明「按它可显隐
                        // 面板」（按新热键面板隐藏是 Toggle 设计行为非 bug）。
                        self.show_toast(
                            crate::text::t(self.lang_effective, "toast.hotkey_updated")
                                .replace("{combo}", &combo),
                            None,
                        );
                        self.end_hotkey_capture();
                    } else {
                        // 注册失败（设置未动）。rolled_back=true = 线程已自动
                        // 回滚旧键：对话框开着 → 行内红色占用提示（候选保留
                        // 可重试）；已关 → toast 兜底（「恢复默认」路径）。
                        // rolled_back=false = 旧键回滚**也**失败（旧键刚被第
                        // 三方抢占等）——当前实际**无生效热键**，除改绑失败外
                        // 还须置 R-15「未注册」位（徽标可见），否则设置页声
                        // 称旧键生效而实际为空（核查识别的死角 #2）。
                        self.hotkey_confirm = None;
                        self.hotkey_apply_failed = true;
                        if !rolled_back {
                            self.hotkey_unregistered = true;
                        }
                        if !self.hotkey_capturing {
                            self.show_toast(
                                crate::text::t(self.lang_effective, "toast.hotkey_failed")
                                    .to_string(),
                                None,
                            );
                        }
                    }
                }
                HotkeyEvent::Died => {
                    // R-15：热键线程异常死亡（GetMessageW 返回 -1）——置「未
                    // 注册」状态位 + **恒有**错误 toast（对话框开着时行内占
                    // 用提示由 apply_failed 承担，但 toast 兜底「对话框已关
                    // / 面板隐藏」路径——核查识别的盲点 #3：确认在途时原先
                    // 完全无感知）。在途确认一并作废。
                    self.hotkey_unregistered = true;
                    if self.hotkey_confirm.take().is_some() {
                        self.hotkey_apply_failed = true;
                    }
                    self.show_error_toast(
                        crate::text::t(self.lang_effective, "toast.hotkey_unregistered")
                            .to_string(),
                    );
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
                    log::info!("[dd-gui] 托盘菜单：退出（结束进程）");
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
            // 捕获期守卫（2026-09-30 真机）：回落捕获模式下按 Win / Alt+Space
            // 会拉起开始菜单/系统菜单抢焦点 → 失焦自动隐藏 → hide() 复位捕获
            // 态、对话框销毁。捕获是模态操作，焦点抢占不应销毁它——捕获期
            // 不自动隐藏（结束后恢复标准失焦语义；点击面板控件本身会带回焦
            // 点，正常路径无残留影响）。
            if self.hotkey_capturing {
                return;
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ctx, make_app, make_app_with};
    use std::sync::mpsc;
    use std::time::Instant;

    /// R-15 验收单测（plan §5 R-15；2026-09-30 seq 协议后重写）：
    /// ① 启动注册失败（`ReRegistered{seq:0, false}`）→ 置「未注册」位 + 错误 toast；
    /// ② 热键线程死亡（`Died`）→ 置同一状态位；
    /// ③ 改绑确认成功（`ReRegistered{seq:1, true}`）→ 此时才写设置 + 复位（徽标消失）。
    #[test]
    fn r15_hotkey_failed_state_flags() {
        // ① 启动注册失败：seq=0 走启动分支。
        let (tx, rx) = mpsc::channel();
        let mut app = make_app_with(rx);
        assert!(!app.hotkey_unregistered);
        tx.send(HotkeyEvent::ReRegistered {
            seq: 0,
            ok: false,
            rolled_back: false,
        })
        .unwrap();
        app.poll_hotkey(&ctx());
        assert!(app.hotkey_unregistered, "启动注册失败应置「未注册」位");
        assert!(app.toast.is_some(), "启动注册失败应出现错误 toast");

        // ② 热键线程死亡：同一状态位（无在途确认 → toast 提示口径）。
        tx.send(HotkeyEvent::Died).unwrap();
        app.poll_hotkey(&ctx());
        assert!(app.hotkey_unregistered, "线程死亡应置同一「未注册」位");

        // ③ 改绑确认成功：设置在此刻才写入（此前保持旧值），徽标复位。
        app.apply_captured_hotkey(0b0010, 0x20); // 候选 Ctrl+Space（seq=1）
        assert!(app.hotkey_confirm.is_some());
        tx.send(HotkeyEvent::ReRegistered {
            seq: 1,
            ok: true,
            rolled_back: true,
        })
        .unwrap();
        app.poll_hotkey(&ctx());
        assert_eq!(app.settings.hotkey_mods, 0b0010, "确认成功才写设置");
        assert_eq!(app.settings.hotkey_vk, 0x20);
        assert!(app.hotkey_confirm.is_none(), "确认完成应清在途态");
        assert!(!app.hotkey_unregistered, "重注册成功应复位「未注册」位");
    }

    /// seq 配对回归（真机「冲突只拦截一次、第二次假成功」根治锚定，2026-09-30）：
    /// ① 过期/未知 seq 事件一律忽略（旧实现盲配的等价场景）；
    /// ② 在途确认收到**不匹配 seq** 的失败事件 → 忽略，设置不动、确认仍在途；
    /// ③ 匹配失败 → 行内失败态，设置不动（永不回滚写盘）；
    /// ④ 失败后重试同款候选 → 正常成功写设置（重试路径不腐蚀状态机）。
    #[test]
    fn hotkey_confirm_seq_pairing_rejects_stale_and_mismatched() {
        let (tx, rx) = mpsc::channel();
        let mut app = make_app_with(rx);
        let (old_mods, old_vk) = (app.settings.hotkey_mods, app.settings.hotkey_vk);

        // ① 未知 seq 的「假成功」：无在途确认 → 忽略，无任何状态变化。
        tx.send(HotkeyEvent::ReRegistered {
            seq: 9,
            ok: true,
            rolled_back: true,
        })
        .unwrap();
        app.poll_hotkey(&ctx());
        assert_eq!(app.settings.hotkey_mods, old_mods);
        assert!(app.hotkey_confirm.is_none());

        // ② 在途 seq=1，先到不匹配的 seq=2 失败事件（旧实现此处错位清快照）。
        app.apply_captured_hotkey(0b0010, 0x20);
        tx.send(HotkeyEvent::ReRegistered {
            seq: 2,
            ok: false,
            rolled_back: true,
        })
        .unwrap();
        app.poll_hotkey(&ctx());
        assert!(app.hotkey_confirm.is_some(), "不匹配事件不得清在途确认");
        assert!(!app.hotkey_apply_failed, "不匹配事件不得置失败态");
        assert_eq!(app.settings.hotkey_mods, old_mods, "设置不得提前变动");

        // ③ 匹配的失败事件：行内失败态（对话框开与否由 capturing 决定），
        //    设置不动——冲突组合永不落盘。
        tx.send(HotkeyEvent::ReRegistered {
            seq: 1,
            ok: false,
            rolled_back: true,
        })
        .unwrap();
        app.poll_hotkey(&ctx());
        assert!(app.hotkey_confirm.is_none());
        assert!(app.hotkey_apply_failed, "匹配失败应置行内失败态");
        assert_eq!(app.settings.hotkey_mods, old_mods, "失败后设置保持旧值");
        assert_eq!(app.settings.hotkey_vk, old_vk);

        // ④ 重试（发号新 seq）→ 成功 → 写设置。
        app.apply_captured_hotkey(0b0010, 0x20);
        let retry_seq = app.hotkey_seq;
        tx.send(HotkeyEvent::ReRegistered {
            seq: retry_seq,
            ok: true,
            rolled_back: true,
        })
        .unwrap();
        app.poll_hotkey(&ctx());
        assert_eq!(app.settings.hotkey_mods, 0b0010, "重试成功应写设置");
        assert!(!app.hotkey_apply_failed);
    }

    /// 确认超时兜底：结果 2s 未回发（线程死亡且 Died 丢失等）→ 视同失败，
    /// 设置不动、失败态可见，可重新发起。
    #[test]
    fn hotkey_confirm_timeout_fails_without_touching_settings() {
        let (_tx, rx) = mpsc::channel();
        let mut app = make_app_with(rx);
        let old_mods = app.settings.hotkey_mods;
        app.apply_captured_hotkey(0b0010, 0x20);
        // 回拨发起时刻，绕过真实 2s 等待。
        let confirm = app.hotkey_confirm.as_mut().unwrap();
        confirm.started = Instant::now() - super::super::HOTKEY_CONFIRM_TIMEOUT;
        app.poll_hotkey(&ctx());
        assert!(app.hotkey_confirm.is_none(), "超时应清在途确认");
        assert!(app.hotkey_apply_failed, "超时应置失败态");
        assert_eq!(app.settings.hotkey_mods, old_mods, "超时不得写设置");
    }

    /// 双失败死角（2026-09-30 核查 #2）：改绑失败**且**旧键回滚也失败
    ///（`rolled_back=false`）→ 除改绑失败态外还须置 R-15「未注册」位——
    /// 此时实际无任何生效热键，设置页不得声称旧键仍生效。对照：回滚成功
    ///（`rolled_back=true`）不得误置「未注册」位。
    #[test]
    fn hotkey_failed_rollback_status_drives_unregistered_flag() {
        // 回滚也失败：改绑失败 + 未注册双置位，设置保持旧值。
        let (tx, rx) = mpsc::channel();
        let mut app = make_app_with(rx);
        let old_mods = app.settings.hotkey_mods;
        app.apply_captured_hotkey(0b0010, 0x20);
        tx.send(HotkeyEvent::ReRegistered {
            seq: 1,
            ok: false,
            rolled_back: false,
        })
        .unwrap();
        app.poll_hotkey(&ctx());
        assert!(app.hotkey_apply_failed, "改绑失败应置行内失败态");
        assert!(
            app.hotkey_unregistered,
            "回滚也失败应置「未注册」位（当前无生效热键）"
        );
        assert_eq!(app.settings.hotkey_mods, old_mods, "设置保持旧值不变形");

        // 对照：回滚成功（常规冲突场景）不得误置「未注册」位。
        let (tx, rx) = mpsc::channel();
        let mut app = make_app_with(rx);
        app.apply_captured_hotkey(0b0010, 0x20);
        tx.send(HotkeyEvent::ReRegistered {
            seq: 1,
            ok: false,
            rolled_back: true,
        })
        .unwrap();
        app.poll_hotkey(&ctx());
        assert!(app.hotkey_apply_failed);
        assert!(
            !app.hotkey_unregistered,
            "旧键回滚成功 ≠ 无热键，不得置「未注册」位"
        );
    }

    /// Died 盲点（2026-09-30 核查 #3）：确认在途且对话框已关（如「恢复默认」
    /// 路径）时热键线程死亡 → 除作废在途确认 + 失败态外**恒有**错误 toast
    ///（原实现此场景完全无感知）。
    #[test]
    fn hotkey_died_with_pending_confirm_still_toasts() {
        let (tx, rx) = mpsc::channel();
        let mut app = make_app_with(rx);
        app.apply_captured_hotkey(0b0010, 0x20); // 捕获未开启 = 对话框已关
        assert!(app.hotkey_confirm.is_some());
        assert!(app.toast.is_none());
        tx.send(HotkeyEvent::Died).unwrap();
        app.poll_hotkey(&ctx());
        assert!(app.hotkey_confirm.is_none(), "线程死亡应作废在途确认");
        assert!(app.hotkey_apply_failed, "在途确认应转行内失败态");
        assert!(app.hotkey_unregistered, "线程死亡应置「未注册」位");
        assert!(app.toast.is_some(), "对话框已关场景 toast 必须兜底");
    }

    /// 捕获期守卫（2026-09-30 真机「按键后界面隐藏」）：回落捕获模式下按
    /// 当前已注册组合键送达 Toggle → 捕获期必须忽略，不得切换面板；对照：
    /// 非捕获态 Toggle 照常切换。
    #[test]
    fn toggle_ignored_while_hotkey_capturing() {
        let (tx, rx) = mpsc::channel();
        let mut app = make_app_with(rx);
        app.visible = true;
        app.hotkey_capturing = true; // 对话框打开中（回落模式等价场景）
        tx.send(HotkeyEvent::Toggle).unwrap();
        app.poll_hotkey(&ctx());
        assert!(app.visible, "捕获期 Toggle 不得隐藏面板");
        assert!(app.hotkey_capturing, "捕获态必须存活（对话框不被销毁）");

        // 对照：非捕获态 Toggle 正常切换（隐藏）。
        app.hotkey_capturing = false;
        tx.send(HotkeyEvent::Toggle).unwrap();
        app.poll_hotkey(&ctx());
        assert!(!app.visible, "非捕获态 Toggle 应照常隐藏面板");
    }

    /// 捕获期守卫（2026-09-30 真机）：回落捕获下 Win/Alt+Space 拉起系统 UI
    /// 抢焦点 → 失焦自动隐藏必须豁免捕获期，不得销毁捕获态；对照：非捕获
    /// 期失焦照常隐藏。headless ctx 的 viewport().focused 恒 None = 未聚焦。
    #[test]
    fn focus_loss_hidden_while_hotkey_capturing() {
        let mut app = make_app();
        app.visible = true;
        app.ever_focused = true;
        app.hotkey_capturing = true;
        app.handle_focus_loss(&ctx());
        assert!(app.visible, "捕获期失焦不得自动隐藏面板");
        assert!(app.hotkey_capturing, "捕获态必须存活");

        // 对照：非捕获期失焦照常隐藏。
        app.hotkey_capturing = false;
        app.handle_focus_loss(&ctx());
        assert!(!app.visible, "非捕获期失焦应照常隐藏面板");
    }

    /// B 方案小修（2026-09-29）：捕获态不跨隐藏周期存活——`hide()` 复位捕获
    /// 标志并清钩子字段（真机 bug：捕获期按 Win → 开始菜单抢焦点 → 失焦
    /// hide → 再唤起仍卡在捕获提示态；hide 同时是 LL 钩子的逃生出口）。
    /// 注：不调用 `start_hotkey_capture`（会在 Windows 装真钩子），直接置场。
    #[test]
    fn hide_resets_hotkey_capture_state() {
        let mut app = make_app();
        app.hotkey_capturing = true;
        app.capture_started = Some(Instant::now());
        app.hide(&ctx());
        assert!(!app.hotkey_capturing, "hide 应复位捕获标志");
        assert!(
            app.capture_hook.is_none() && app.capture_rx.is_none() && app.capture_started.is_none(),
            "hide 应清空钩子守卫 / 事件端 / 超时基准"
        );
    }
}
