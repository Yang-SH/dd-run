//! O12（2026-10-11）拆分自 `app/keys.rs`（O12 巨型文件拆分批次）。
//! 纪律（refactor-layering-plan 同款）：搬运单位 = 完整定义块（含文档注释），
//! 函数体一字不改；仅按编译器指示将跨子模块项 `pub(super)` 化。
//! 本文件承载：键盘导航分发（handle_keys / capture_vk）与查询清理（clear_current_query）。

use super::*;

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
        // ⚠️ Backspace **不在此处消费**（2026-09-23 修复）：它默认是文本编辑键
        // （FilterBox 的 TextEdit 删字）。仅当下方「退格返回」分支确认要返回时才
        // `consume_key`，其余情况一律留给输入框。此前在此无条件消费 Backspace，
        // 导致默认关（`backspace_go_back=false`）时输入框也收不到退格——
        // 文件搜索页无法删除已输入内容（B2 回归）。
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
        // R-22（2026-09-30）：IME 组词期回车守卫——组词中回车 = 「上屏」手势，
        // 上屏 Commit 与 Enter 键事件常**同帧到达**；不设防会把选中项直接执行
        // （危险命令有确认门，普通命令没有）。本帧收到过组词系事件（Preedit
        // 组词中 / Commit 上屏）则跳过一次激活：该 Enter 已被上方消费（吞掉，
        // 文本经 Commit 事件入框、与键事件无关），下一帧恢复正常。IME 开启但
        // 非组词（英文直输）无 Ime 事件，不受影响。
        let ime_composing = ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::Ime(egui::ImeEvent::Preedit { .. } | egui::ImeEvent::Commit(_))
                )
            })
        });
        let enter = enter && !ime_composing;
        // 批次 4.0：Ctrl+, 打开设置（§6.1 快捷键；设置入口的键盘可达手段
        // ——Tab 保持列表导航语义不变，见 implementation.md 批次 4.0 决策）。
        // 在确认对话框分支之后处理：对话框活跃时该键被吞掉不穿透。
        let ctrl_comma = ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Comma));
        // v0.1.4（2026-09-19）：`Ctrl+F` 一键进入文件搜索页（方案
        // `docs/search-file-ctrl-f-icons-plan.md` §3）。键位冲突已取证：`Ctrl+F`
        // 此前全仓（含 egui 0.36 输入框内建绑定）无占用——面板内键位仅
        // Esc / ↑↓ / Tab / Shift+Tab / Enter / `Ctrl+,` / Shift+F10（§2.2）。
        // 与 `Ctrl+,` 同层：确认对话框 / 右键菜单分支已先行 return，按键不穿透。
        let ctrl_f = ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F));
        // v4.4（D19）：Shift+F10 对选中行打开右键菜单（egui 0.36 键表无 Menu
        // 键，Shift+F10 为 Windows 菜单键的等价惯例——偏离记档：Menu 键不可达）。
        let shift_f10 = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F10));
        // F7（2026-10-10）：Home/End/PgUp/PgDn 列表导航（长列表便利性，对齐
        // CmdPal / PowerToys Run）。与 ↑↓ 同层消费（NONE 修饰）。取舍声明：
        // 搜索框聚焦时 Home/End 被列表消费（handle_keys 先于 TextEdit 处理
        // 输入）——单行输入框内光标跳行首/行尾让位列表导航，与 CmdPal 行为
        // 一致；未来若需保留编辑光标语义可改 Ctrl+Home/End，本轮不做。
        let (home, end, page_up, page_down) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::Home),
                i.consume_key(egui::Modifiers::NONE, egui::Key::End),
                i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown),
            )
        });

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
            // B1（2026-09-20）：Esc 行为三档（设置 `esc_behavior`），默认档 =
            // 既有行为（非 Root 返回并聚焦回落后页面、Root 隐藏）。决策为纯函数，
            // 副作用在此执行。
            let is_root = self.stack.current().page_id.is_none();
            let query_empty = self.stack.current().list.query().is_empty();
            match self.settings.esc_behavior.decide(is_root, query_empty) {
                dd_gui::settings::EscAction::ClearSearch => self.clear_current_query(ctx),
                dd_gui::settings::EscAction::GoBack => {
                    if self.go_back_focused().is_none() {
                        self.hide(ctx);
                    }
                }
                dd_gui::settings::EscAction::Hide => self.hide(ctx),
            }
            return;
        }
        // B2（2026-09-20）：退格键返回（默认关）。仅在「开关开 + 嵌套页 + 搜索框为空」
        // 时**消费 Backspace** 并返回；其余情况一律不消费——退格留给输入框删字。
        // 修复（2026-09-23）：消费动作必须收敛在此判定内（而非函数开头无条件消费），
        // 否则默认关时输入框收不到退格，无法删除已输入内容。
        if self.settings.backspace_go_back {
            let is_root = self.stack.current().page_id.is_none();
            let query_empty = self.stack.current().list.query().is_empty();
            let go_back = !is_root
                && query_empty
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace));
            if go_back {
                self.go_back_focused();
                return;
            }
        }
        if down || tab {
            self.stack.current_mut().list.move_down();
            self.scroll_follow = true; // 键盘选中：恢复滚动跟随
        }
        if up || shift_tab {
            self.stack.current_mut().list.move_up();
            self.scroll_follow = true;
        }
        // F7：Home/End/PgUp/PgDn 与 ↑↓ 同分支分发（scroll_follow 同步置位）
        if home {
            self.stack.current_mut().list.move_home();
            self.scroll_follow = true;
        }
        if end {
            self.stack.current_mut().list.move_end();
            self.scroll_follow = true;
        }
        if page_down {
            self.stack.current_mut().list.move_page_down();
            self.scroll_follow = true;
        }
        if page_up {
            self.stack.current_mut().list.move_page_up();
            self.scroll_follow = true;
        }
        if enter {
            self.confirm_selected();
        }
        if ctrl_comma {
            self.open_settings();
        }
        if ctrl_f {
            self.open_file_search_from_panel();
        }
    }

    /// 热键捕获候选键 → 虚拟键码（M6 批次 6.3：字母/数字 = ASCII，
    /// F1–F12 = 0x70–0x7B，Space = 0x20；其余键不作为热键主键）。
    pub(super) fn capture_vk(key: egui::Key) -> Option<u32> {
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

    /// B1：Esc「先清除搜索内容」档——清空当前页搜索框。嵌套页的过滤由扩展侧
    /// 完成，清空后按空查询去抖重拉（根页走本地过滤，清空即恢复全量）。
    fn clear_current_query(&mut self, ctx: &egui::Context) {
        let is_nested = self.stack.current().page_id.is_some();
        self.stack.current_mut().list.set_query(String::new());
        if is_nested {
            self.rearm_page_query_debounce();
        }
        ctx.request_repaint();
    }
}
