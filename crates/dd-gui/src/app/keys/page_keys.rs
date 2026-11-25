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
        if self.hotkeys.hotkey_capturing {
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
        // N14（2026-11-21）：`Ctrl+1..9` 直开可见列表第 N 行 + `Ctrl+Enter`
        // 定位/复制。与 Enter 同层消费（确认对话框 / 右键菜单分支已先行
        // return，按键不穿透）；数字守卫复用 R-22 IME 组词口径（组词期
        // Ctrl+数字 不误触——数字上屏是 IME 常态）。键位冲突已取证：page_keys
        // 既有消费清单（Escape/↑↓/Enter/Tab/Shift+Tab/Ctrl+,/Ctrl+F/
        // Shift+F10/Home/End/PgUp/PgDn/Backspace）无 Ctrl+数字与 Ctrl+Enter。
        let ctrl_enter = ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter));
        let ctrl_digit: Option<u8> = (!ime_composing)
            .then(|| {
                ctx.input_mut(|i| {
                    const DIGITS: [(egui::Key, u8); 9] = [
                        (egui::Key::Num1, 1),
                        (egui::Key::Num2, 2),
                        (egui::Key::Num3, 3),
                        (egui::Key::Num4, 4),
                        (egui::Key::Num5, 5),
                        (egui::Key::Num6, 6),
                        (egui::Key::Num7, 7),
                        (egui::Key::Num8, 8),
                        (egui::Key::Num9, 9),
                    ];
                    for (key, n) in DIGITS {
                        if i.consume_key(egui::Modifiers::CTRL, key) {
                            return Some(n);
                        }
                    }
                    None
                })
            })
            .flatten();

        // 确认对话框活跃时：Enter=确认、Esc=取消，其余键不穿透到列表
        if self.overlay.confirm.is_some() {
            if esc {
                self.overlay.confirm = None;
            } else if enter {
                let dialog = self.overlay.confirm.take().expect("对话框存在");
                let params = dialog.pending.confirmed_params();
                self.dispatch_invoke(&dialog.ext_id, params);
            }
            return;
        }

        // 右键菜单打开时：键盘上下文移交菜单（10B.1 键盘行）——↑↓ 移动焦点、
        // Enter 激活、Esc 关闭并回到列表；Tab/Ctrl+, 一并吞掉不穿透（D19 冻结）。
        if self.overlay.ctx_menu.is_some() {
            if esc {
                self.overlay.ctx_menu = None;
            } else if enter {
                self.activate_ctx_menu(ctx);
            } else if down || tab {
                let n = ctx_entry_count(self.overlay.ctx_menu.as_ref());
                if n > 0 {
                    let state = self.overlay.ctx_menu.as_mut().expect("菜单存在");
                    state.focus = (state.focus + 1) % n;
                }
            } else if up || shift_tab {
                let n = ctx_entry_count(self.overlay.ctx_menu.as_ref());
                if n > 0 {
                    let state = self.overlay.ctx_menu.as_mut().expect("菜单存在");
                    state.focus = (state.focus + n - 1) % n;
                }
            }
            return;
        }

        // v4.4（D19）：Shift+F10 = 对选中行打开菜单。行矩形只在绘制期可得，
        // 这里仅置旗标，`draw_list` 绘制选中行后消费落位（锚定行底边左缘）。
        if shift_f10 {
            self.overlay.want_ctx_menu_for_selected = true;
            ctx.request_repaint();
        }

        if esc {
            // B1（2026-09-20）：Esc 行为三档（设置 `esc_behavior`），默认档 =
            // 既有行为（非 Root 返回并聚焦回落后页面、Root 隐藏）。决策为纯函数，
            // 副作用在此执行。
            let is_root = self.view.stack.current().page_id.is_none();
            let query_empty = self.view.stack.current().list.query().is_empty();
            match self
                .draft
                .settings
                .esc_behavior
                .decide(is_root, query_empty)
            {
                crate::settings::EscAction::ClearSearch => self.clear_current_query(ctx),
                crate::settings::EscAction::GoBack => {
                    if self.go_back_focused().is_none() {
                        self.hide(ctx);
                    }
                }
                crate::settings::EscAction::Hide => self.hide(ctx),
            }
            return;
        }
        // B2（2026-09-20）：退格键返回（默认关）。仅在「开关开 + 嵌套页 + 搜索框为空」
        // 时**消费 Backspace** 并返回；其余情况一律不消费——退格留给输入框删字。
        // 修复（2026-09-23）：消费动作必须收敛在此判定内（而非函数开头无条件消费），
        // 否则默认关时输入框收不到退格，无法删除已输入内容。
        if self.draft.settings.backspace_go_back {
            let is_root = self.view.stack.current().page_id.is_none();
            let query_empty = self.view.stack.current().list.query().is_empty();
            let go_back = !is_root
                && query_empty
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace));
            if go_back {
                self.go_back_focused();
                return;
            }
        }
        if down || tab {
            self.view.stack.current_mut().list.move_down();
            self.window.scroll_follow = true; // 键盘选中：恢复滚动跟随
        }
        if up || shift_tab {
            self.view.stack.current_mut().list.move_up();
            self.window.scroll_follow = true;
        }
        // F7：Home/End/PgUp/PgDn 与 ↑↓ 同分支分发（scroll_follow 同步置位）
        if home {
            self.view.stack.current_mut().list.move_home();
            self.window.scroll_follow = true;
        }
        if end {
            self.view.stack.current_mut().list.move_end();
            self.window.scroll_follow = true;
        }
        if page_down {
            self.view.stack.current_mut().list.move_page_down();
            self.window.scroll_follow = true;
        }
        if page_up {
            self.view.stack.current_mut().list.move_page_up();
            self.window.scroll_follow = true;
        }
        if enter {
            // N12（2026-11-22）：文件搜索页目录行 Enter = 进入目录浏览（先于
            // invoke 分派拦截；文件行 Enter 打开不变，「打开目录」保留于右键
            // reveal——search-file.md v3.14 权威口径）。
            if self.try_enter_directory() {
                ctx.request_repaint();
            } else {
                self.confirm_selected();
            }
        }
        // N14：Ctrl+Enter = 选中项定位/复制（右键同款动作的键盘可达路径）。
        if ctrl_enter && !ime_composing {
            self.ctrl_enter_selected(ctx);
        }
        // N14：Ctrl+1..9 = 置选中第 N 行（1 基）并激活；越界 no-op。
        if let Some(n) = ctrl_digit {
            let idx = n as usize - 1;
            let in_bounds = idx < self.view.stack.current().list.visible_count();
            if in_bounds {
                self.view.stack.current_mut().list.set_selected(idx);
                self.window.scroll_follow = true;
                self.confirm_selected();
            }
        }
        if ctrl_comma {
            self.open_settings();
        }
        if ctrl_f {
            self.open_file_search_from_panel();
        }
        // N20（2026-11-22）：Ctrl+↑/↓ 搜索历史回填（仅根页；空框或处于历史
        // 回填态才劫持——框内有手输内容 = 常规编辑，不劫持；IME 组词期同守卫
        // 跳过）。↑ = 更旧（i+1）、↓ = 更新（i-1，越过最新回空框）。
        // 实现注记（安全侧偏离记档）：方案 §4.15 的「光标在行首」判定需
        // egui `TextEditState`（0.36 未公开导出该类型）——以「回填态游标匹配」
        // 等价承载：回填后连续 ↑/↓ 不受影响；框内手输内容一律不劫持；差异 =
        // 手置光标行首（非回填态）不触发回填。
        let (ctrl_up, ctrl_down) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowDown),
            )
        });
        if (ctrl_up || ctrl_down) && !ime_composing {
            let current = self.view.stack.current();
            let is_root = current.page_id.is_none() && !current.is_settings;
            if is_root {
                let query = current.list.query().to_owned();
                // 游标失效判定：内容 ≠ 游标所指历史项（手输改写）→ 复位，
                // 下一次 ↑ 从最新开始。
                if let Some(i) = self.view.history_nav {
                    if self.view.search_history.get(i).map(String::as_str) != Some(query.as_str()) {
                        self.view.history_nav = None;
                    }
                }
                // 可回填态：空框起步，或正处历史回填态（内容 = 游标所指项）。
                let nav_live = match self.view.history_nav {
                    Some(i) => {
                        self.view.search_history.get(i).map(String::as_str) == Some(query.as_str())
                    }
                    None => query.is_empty(),
                };
                if nav_live && !self.view.search_history.is_empty() {
                    let n = self.view.search_history.len();
                    let new_nav = if ctrl_up {
                        Some(match self.view.history_nav {
                            None => 0,
                            Some(i) => (i + 1).min(n - 1),
                        })
                    } else {
                        self.view.history_nav.map(|i| i.saturating_sub(1))
                    };
                    self.view.history_nav = new_nav;
                    let filled = new_nav
                        .and_then(|i| self.view.search_history.get(i).cloned())
                        .unwrap_or_default();
                    self.view.stack.current_mut().list.set_query(&filled);
                    ctx.request_repaint();
                }
            }
        }
        // N14 子项（2026-11-22）：`?` 键位速查页——根页空框触发（消费 Text 事件
        // 防上屏；非空框 = 普通字符不触发；IME 组词期守卫；已在速查页不重推）。
        if !ime_composing {
            let current = self.view.stack.current();
            let is_root = current.page_id.is_none() && !current.is_settings;
            if is_root && current.list.query().is_empty() {
                let question_typed = ctx.input_mut(|i| {
                    let idx = i
                        .events
                        .iter()
                        .position(|e| matches!(e, egui::Event::Text(t) if t == "?"));
                    idx.map(|k| i.events.remove(k)).is_some()
                });
                if question_typed {
                    self.open_help_page();
                }
            }
        }
        // N12（2026-11-22）：文件搜索页目录浏览键（search-file.md v3.14）——
        // Ctrl+Backspace = 搜索框删除最后一段路径（盘符根 / UNC share 根 /
        // 退无可退 → 清空查询）；Right = 选中行名追加进搜索框（目录自动补
        // `\`）。仅文件搜索页生效；Right 仅在路径浏览形态消费（自由文本态
        // 保留输入框光标移动）；Tab 保持列表导航、Backspace 单键保持输入编辑
        // （批次 4.0 决策不动）。
        let on_files_page = {
            let cur = self.view.stack.current();
            // 2026-11-22 真机腿修正：id 曾写成不存在的 `com.ddrun.search` →
            // 门控恒假、N12 宿主键位全部失效；改用 `app` 模块既有常量（单一来源）。
            cur.page_id.as_deref() == Some(crate::app::FILE_SEARCH_PAGE_ID)
                && cur.ext_id == crate::app::FILE_SEARCH_EXT_ID
        };
        if on_files_page
            && !ime_composing
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Backspace))
        {
            let backed = path_back(self.view.stack.current().list.query());
            self.view.stack.current_mut().list.set_query(&backed);
            self.rearm_page_query_debounce();
            ctx.request_repaint();
        }
        let browsing = on_files_page && {
            let q = self.view.stack.current().list.query();
            q.ends_with('\\') || q.ends_with('/')
        };
        let right = browsing
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight));
        if right && !ime_composing {
            let selected = self.view.stack.current().list.confirm().cloned();
            if let Some(item) = selected {
                let mut q = self.view.stack.current().list.query().to_owned();
                if !q.ends_with('\\') && !q.ends_with('/') {
                    q.push('\\');
                }
                q.push_str(&item.title);
                if item.tags.iter().any(|t| t == "dir") {
                    q.push('\\');
                }
                self.view.stack.current_mut().list.set_query(&q);
                self.rearm_page_query_debounce();
                ctx.request_repaint();
            }
        }
    }

    /// N12：文件搜索页目录行 Enter = 进入目录浏览。命中（files.results 页 +
    /// 选中行带 `dir` 标签）→ 搜索框写入路径（补尾 `\`）+ 页查询去抖重拉 →
    /// true；未命中 → false（调用方走常规 confirm 激活——文件行打开不变）。
    pub(crate) fn try_enter_directory(&mut self) -> bool {
        let cur = self.view.stack.current();
        let on_files_page = cur.page_id.as_deref() == Some(crate::app::FILE_SEARCH_PAGE_ID)
            && cur.ext_id == crate::app::FILE_SEARCH_EXT_ID;
        if !on_files_page {
            return false;
        }
        let Some(item) = cur.list.confirm().cloned() else {
            return false;
        };
        if !item.tags.iter().any(|t| t == "dir") {
            return false;
        }
        let path = format!("{}\\", item.subtitle.trim_end_matches(['\\', '/']));
        self.view.stack.current_mut().list.set_query(&path);
        self.rearm_page_query_debounce();
        log::debug!("[dd-gui] N12 进入目录：{path}");
        true
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
        let is_nested = self.view.stack.current().page_id.is_some();
        self.view.stack.current_mut().list.set_query(String::new());
        if is_nested {
            self.rearm_page_query_debounce();
        }
        ctx.request_repaint();
    }
}

// ── N12：路径回退（2026-11-22，search-file.md v3.14）────────────────────────
/// N12：路径回退纯函数——删最后一段路径段。尾分隔符先剥（"C:\a\" → 剥后
/// "C:\a" → 上级 "C:\"）；盘符根（"C:"）/ UNC share 根（"\\srv\share"）/
/// 无分隔符（自由文本）→ 清空查询（方案「退无可退则清空」口径）。可单测。
fn path_back(query: &str) -> String {
    let trimmed = query.trim_end_matches(['\\', '/']);
    if trimmed.is_empty() {
        return String::new();
    }
    let bytes = trimmed.as_bytes();
    let is_drive_root =
        bytes.len() == 2 && bytes[1] == b':' && (bytes[0] as char).is_ascii_alphabetic();
    let is_unc_share_root = trimmed.starts_with(r"\\") && trimmed.matches('\\').count() == 3;
    if is_drive_root || is_unc_share_root {
        return String::new();
    }
    match trimmed.rfind(['\\', '/']) {
        Some(idx) => trimmed[..=idx].to_string(),
        None => String::new(),
    }
}

#[cfg(test)]
mod n12_tests {
    use super::path_back;

    /// 段删除 / 尾分隔符 / 根边界（盘符 + UNC）/ 自由文本四类边界。
    #[test]
    fn n12_path_back_segments_and_boundaries() {
        assert_eq!(path_back(r"C:\a\b"), r"C:\a\");
        assert_eq!(path_back(r"C:\a"), r"C:\");
        assert_eq!(path_back(r"C:\a\"), r"C:\", "尾分隔符先剥再删段");
        assert_eq!(path_back(r"C:\"), "", "盘符根 → 清空");
        assert_eq!(path_back("C:"), "", "盘符 → 清空");
        assert_eq!(path_back(""), "", "空 → 原样");
        assert_eq!(path_back("abc"), "", "无分隔符（自由文本）→ 清空");
        assert_eq!(path_back(r"\\srv\share\d"), r"\\srv\share\");
        assert_eq!(path_back(r"\\srv\share"), "", "UNC share 根 → 清空");
    }
}
