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
        // M6 批次 6.3：热键捕获模式优先拦截全部按键（Esc 取消；组合键生效）。
        if self.hotkey_capturing {
            self.handle_hotkey_capture(ctx);
            return;
        }
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

    /// 热键捕获候选键 → 虚拟键码（M6 批次 6.3：字母/数字 = ASCII，
    /// F1–F12 = 0x70–0x7B，Space = 0x20；其余键不作为热键主键）。
    fn capture_vk(key: egui::Key) -> Option<u32> {
        use egui::Key;
        Some(match key {
            Key::A => 0x41,
            Key::B => 0x42,
            Key::C => 0x43,
            Key::D => 0x44,
            Key::E => 0x45,
            Key::F => 0x46,
            Key::G => 0x47,
            Key::H => 0x48,
            Key::I => 0x49,
            Key::J => 0x4A,
            Key::K => 0x4B,
            Key::L => 0x4C,
            Key::M => 0x4D,
            Key::N => 0x4E,
            Key::O => 0x4F,
            Key::P => 0x50,
            Key::Q => 0x51,
            Key::R => 0x52,
            Key::S => 0x53,
            Key::T => 0x54,
            Key::U => 0x55,
            Key::V => 0x56,
            Key::W => 0x57,
            Key::X => 0x58,
            Key::Y => 0x59,
            Key::Z => 0x5A,
            Key::Num0 => 0x30,
            Key::Num1 => 0x31,
            Key::Num2 => 0x32,
            Key::Num3 => 0x33,
            Key::Num4 => 0x34,
            Key::Num5 => 0x35,
            Key::Num6 => 0x36,
            Key::Num7 => 0x37,
            Key::Num8 => 0x38,
            Key::Num9 => 0x39,
            Key::F1 => 0x70,
            Key::F2 => 0x71,
            Key::F3 => 0x72,
            Key::F4 => 0x73,
            Key::F5 => 0x74,
            Key::F6 => 0x75,
            Key::F7 => 0x76,
            Key::F8 => 0x77,
            Key::F9 => 0x78,
            Key::F10 => 0x79,
            Key::F11 => 0x7A,
            Key::F12 => 0x7B,
            Key::Space => 0x20,
            _ => return None,
        })
    }

    /// 热键捕获（M6 批次 6.3）：捕获模式下的按键裁决——Esc 取消；带 Ctrl/Alt
    /// 修饰的候选键生效（纯 Shift/无修饰忽略，避免误捕获单键）；其余键吞掉。
    ///
    /// **逐事件检查**（v4.7 真机反馈修订）：组合键的按下/释放常在同一事件批次
    /// 内完成，`i.modifiers` 快照已是释放后的状态（合成注入必现、极快击键
    /// 同样可能）——改读 `Event::Key` 自带的按下时刻 modifiers，精确可靠。
    fn handle_hotkey_capture(&mut self, ctx: &egui::Context) {
        let events: Vec<egui::Event> = ctx.input(|i| i.events.clone());
        for ev in &events {
            let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = ev
            else {
                continue;
            };
            if *key == egui::Key::Escape {
                self.hotkey_capturing = false;
                return;
            }
            let Some(vk) = Self::capture_vk(*key) else {
                continue;
            };
            let mods = ((modifiers.ctrl as u32) << 1)
                | (modifiers.alt as u32)
                | ((modifiers.shift as u32) << 2);
            if mods & 0b0011 == 0 {
                continue; // 需含 Ctrl/Alt：忽略本次按键，继续等待
            }
            self.hotkey_prev = Some((self.settings.hotkey_mods, self.settings.hotkey_vk));
            self.settings.hotkey_mods = mods;
            self.settings.hotkey_vk = vk;
            self.settings.save();
            self.hotkey.re_register(mods, vk);
            self.hotkey_capturing = false;
            return;
        }
    }

    /// 设置页「更改热键」：进入捕获模式（下一组合键生效，Esc 取消）。
    pub(crate) fn start_hotkey_capture(&mut self) {
        self.hotkey_capturing = true;
    }

    /// 设置页「恢复默认热键」（M6 批次 6.3）：捕获 UI 不支持 Win 修饰
    ///（egui 在 Windows 不暴露 Win 键 modifiers），默认组合经此按钮一键还原。
    pub(crate) fn apply_hotkey_default(&mut self) {
        self.hotkey_prev = Some((self.settings.hotkey_mods, self.settings.hotkey_vk));
        self.settings.hotkey_mods = dd_gui::settings::HOTKEY_MODS_DEFAULT;
        self.settings.hotkey_vk = dd_gui::settings::HOTKEY_VK_DEFAULT;
        self.settings.save();
        self.hotkey.re_register(
            dd_gui::settings::HOTKEY_MODS_DEFAULT,
            dd_gui::settings::HOTKEY_VK_DEFAULT,
        );
    }

    /// 设置页开机自启开关（M6 批次 6.3）：注册表先落，成功才持久化；
    /// 失败 Toast 错误并保持原状态。
    pub(crate) fn apply_autostart(&mut self, on: bool) {
        if self.settings.autostart == on {
            return;
        }
        match crate::platform::set_autostart(on) {
            Ok(()) => {
                self.settings.autostart = on;
                self.settings.save();
            }
            Err(e) => self.show_toast(format!("开机自启设置失败：{e}"), None),
        }
    }

    /// 设置页扩展启停（M6 批次 6.3）：更新停用表 + 落盘 + 置脏标记
    /// （离开设置页时重聚合，见 mod.rs 收口点）；幂等调用无副作用。
    pub(crate) fn apply_extension_enabled(&mut self, id: &str, enabled: bool) {
        let mut next = self.settings.disabled_extensions.clone();
        if enabled {
            next.retain(|x| x != id);
        } else if !next.iter().any(|x| x == id) {
            next.push(id.to_string());
        }
        if next == self.settings.disabled_extensions {
            return; // 幂等
        }
        self.settings.disabled_extensions = next;
        self.settings.save();
        self.exts_dirty = true;
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
        // v4.7 D31：材质生效时同步 DWM 明暗染色（跟随新主题；best-effort）
        if self.backdrop_active {
            if let Some(hwnd) = self.hwnd {
                let dark = ctx.theme() == egui::Theme::Dark;
                crate::platform::set_immersive_dark(hwnd, dark);
            }
        }
        self.settings.save();
    }

    /// 设置页材质开关（v4.7 D30/D31）：更新设置 → 落盘 → 立即应用。
    /// 两开关互斥·后开优先由单值 `backdrop` 派生（开关状态 = 与该值比较），
    /// 开关行点击语义：已开 → 关（None）；未开 → 开（该项）。
    pub(crate) fn apply_backdrop(
        &mut self,
        ctx: &egui::Context,
        backdrop: dd_gui::settings::Backdrop,
    ) {
        if self.settings.backdrop == backdrop {
            return;
        }
        eprintln!("[dd-gui] 窗口材质：{} → 立即生效并保存", backdrop.label());
        self.settings.backdrop = backdrop;
        self.settings.save();
        self.refresh_backdrop(ctx);
    }

    /// 按当前设置应用 DWM 材质（v4.7 D31）。成功 → 面板背景透明化（亮暗两套
    /// Style 同步注册）+ 明暗染色跟随主题；失败（Win10 / 22621 以下）→ 保持
    /// 不透明（platform 层已记日志，回退不阻断）。HWND 未捕获（首帧前）时
    /// 跳过——`ui()` 捕获后会再调用一次。
    ///
    /// **切换防闪（v4.7 真机反馈）**：透明化方向 DWM 先行——材质先在当前
    /// （尚不透明）帧后面就位，下一帧透明面板呈现时即有材质可透出；不透明化
    /// 方向（切到「无材质」）**不能立即清 DWM**——DWM 属性即时生效而 egui 要
    /// 下一帧才画出不透明面板，间隙内桌面穿透一闪。改为置
    /// `backdrop_clear_countdown`，由 `ui()` 末尾在不透明帧呈现之后倒计时清材质。
    pub(crate) fn refresh_backdrop(&mut self, ctx: &egui::Context) {
        let Some(hwnd) = self.hwnd else {
            return;
        };
        // ── 不透明化方向（backdrop = None）：先绘制不透明，后清材质 ──
        if self.settings.backdrop == dd_gui::settings::Backdrop::None {
            if self.backdrop_active {
                self.backdrop_active = false;
                theme::apply_panel_transparency(ctx, false);
                // 倒计时 3 帧：点击帧（旧透明视觉）→ 第 1 个不透明帧绘制并呈现
                // → 第 2 个不透明帧呈现后清 DWM 材质。全程无透明帧暴露窗口。
                self.backdrop_clear_countdown = 3;
            }
            return;
        }
        // ── 透明化方向（云母 / 亚克力）：DWM 先行，再切透明视觉 ──
        let kind = match self.settings.backdrop {
            dd_gui::settings::Backdrop::None => crate::platform::SystemBackdrop::None,
            dd_gui::settings::Backdrop::Mica => crate::platform::SystemBackdrop::Mica,
            dd_gui::settings::Backdrop::Acrylic => crate::platform::SystemBackdrop::Acrylic,
        };
        let ok = crate::platform::apply_system_backdrop(hwnd, kind);
        let active = ok;
        if active {
            let dark = ctx.theme() == egui::Theme::Dark;
            crate::platform::set_immersive_dark(hwnd, dark);
        }
        if active != self.backdrop_active {
            self.backdrop_active = active;
            theme::apply_panel_transparency(ctx, active);
        }
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
        crate::app::spawn_aggregation(
            tx,
            self.cache.clone(),
            self.settings.search_engines_env(),
            self.settings.disabled_extensions.clone(),
        );
        self.aggregate_rx = Some(rx);
        self.aggregating = true;
    }
}
