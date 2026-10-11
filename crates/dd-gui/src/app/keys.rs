//! 键盘导航与设置页动作入口。

// O12（2026-10-11）：巨型文件拆分——键盘导航（navigation）/ 热键捕获与预设
// （hotkey_capture）/ 设置页动作入口（settings_actions）拆入子模块；本文件
// 保留模块接线与单测。纪律：搬运单位 = 完整定义块（含文档注释），函数体
// 一字不改；仅将跨子模块项 `pub(super)` 化（编译器指示的最小可见性改动）。

pub(crate) use crate::app::ctx_menu::ctx_entry_count;
pub(crate) use crate::app::PaletteApp;
pub(crate) use crate::ui::settings_view::SettingsCategory;
pub(crate) use dd_gui::navigation::PageState;
pub(crate) use dd_gui::theme;
pub(crate) use dd_host::trust::{Decision, TrustLedger};
pub(crate) use eframe::egui;
pub(crate) use std::sync::mpsc;
pub(crate) use std::time::Instant;

mod hotkey_capture;
mod navigation;
mod settings_actions;

#[cfg(test)]
mod tests {
    use super::*;

    /// R-21：系统常用/保留组合黑名单——方案四组合（Alt+Space / Ctrl+Esc /
    /// Ctrl+Shift+Esc / Alt+F4）逐一命中；非名单组合（含默认热键 Ctrl+Space
    /// 与 Win+D 类）不误报（回归：非黑名单组合捕获流程零新增警示）。
    #[test]
    fn r21_hotkey_blacklist_matches() {
        // 正例：方案黑名单四组合（修饰键掩码：0x1=Alt 0x2=Ctrl 0x4=Shift 0x8=Win）。
        assert!(PaletteApp::is_system_reserved_combo(0x1, 0x20), "Alt+Space");
        assert!(PaletteApp::is_system_reserved_combo(0x2, 0x1B), "Ctrl+Esc");
        assert!(
            PaletteApp::is_system_reserved_combo(0x6, 0x1B),
            "Ctrl+Shift+Esc"
        );
        assert!(PaletteApp::is_system_reserved_combo(0x1, 0x73), "Alt+F4");
        // 负例：Win+D（方案指名的非名单组合）。
        assert!(!PaletteApp::is_system_reserved_combo(0x8, 0x44), "Win+D");
        // 负例：默认热键 Ctrl+Space（捕获默认路径不得出现警示）。
        assert!(
            !PaletteApp::is_system_reserved_combo(
                dd_gui::settings::HOTKEY_MODS_DEFAULT,
                dd_gui::settings::HOTKEY_VK_DEFAULT
            ),
            "Ctrl+Space 是默认热键，不应命中黑名单"
        );
        // 负例：修饰键子集不误报（Shift+Esc / Ctrl+Alt+P / Alt+）。
        assert!(
            !PaletteApp::is_system_reserved_combo(0x4, 0x1B),
            "Shift+Esc"
        );
        assert!(
            !PaletteApp::is_system_reserved_combo(0x3, 0x50),
            "Ctrl+Alt+P"
        );
        assert!(!PaletteApp::is_system_reserved_combo(0x1, 0x45), "Alt+E");
    }

    /// N4：warm 保活容量的消费点——① 默认配置构造 → LRU 容量 = 8（= 原
    /// `LRU_WARM_CAPACITY` 常量，M1–M4 基线口径不变）；② `warm_capacity = 2`
    /// 的配置构造 → LRU 容量跟随 = 2。驱逐行为本体（缩容队尾优先 / 扩容不
    /// 追补）由 dd-host `lru_set_capacity_*` 单测锚定，此处不重复。
    #[test]
    fn n4_warm_capacity_consumed_at_construction() {
        let app = crate::test_support::make_app();
        assert_eq!(app.lru.capacity(), 8, "默认配置 → LRU 容量 8");
        let settings = dd_gui::settings::Settings {
            warm_capacity: 2,
            ..dd_gui::settings::Settings::default()
        };
        let app = crate::test_support::make_app_with_settings(settings);
        assert_eq!(app.lru.capacity(), 2, "构造点读配置容量");
    }

    /// R-22：IME 组词期回车守卫——① 同帧 Ime(Preedit/Commit) + Enter → 不激活
    /// 且 Enter 被吞；② 无 Ime 帧 Enter → 正常激活（负例，回归锚）；③ 下一帧恢复。
    #[test]
    fn r22_ime_frame_skips_activate() {
        // 夹具：Root 页一个 Page 命令项——激活 = 推入嵌套页（栈深 1 → 2），
        // 纯状态断言、不依赖扩展进程。
        let make_app_with_item = || {
            let mut app = crate::test_support::make_app();
            let item = crate::test_support::item_with(
                "com.example.a",
                dd_protocol::model::CommandRef::Page {
                    page_id: "sub".to_string(),
                },
            );
            *app.stack.root_mut() = dd_gui::navigation::PageState::root(vec![item]);
            app.stack.current_mut().list.move_down(); // 选中唯一项
            app
        };
        // ① 同帧 Preedit + Enter：不激活（栈深不变）。
        let mut app = make_app_with_item();
        let ctx = crate::test_support::ctx();
        ctx.input_mut(|i| {
            i.events.push(egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "ipconfig".to_string(),
                active_range_chars: None,
            }));
        });
        press_enter(&ctx);
        app.handle_keys(&ctx);
        assert_eq!(app.stack.depth(), 1, "组词帧回车不得激活选中项");
        assert!(
            !key_enter_in_queue(&ctx),
            "该 Enter 应被吞掉（不留给输入框重复上屏）"
        );

        // ①' 同帧 Commit + Enter：同口径。
        let mut app = make_app_with_item();
        let ctx = crate::test_support::ctx();
        ctx.input_mut(|i| {
            i.events.push(egui::Event::Ime(egui::ImeEvent::Commit(
                "ipconfig".to_string(),
            )));
        });
        press_enter(&ctx);
        app.handle_keys(&ctx);
        assert_eq!(app.stack.depth(), 1, "上屏帧回车不得激活选中项");

        // ② 无 Ime 帧 Enter：正常激活（回归锚——非 IME 环境行为不变）。
        let mut app = make_app_with_item();
        let ctx = crate::test_support::ctx();
        press_enter(&ctx);
        app.handle_keys(&ctx);
        assert_eq!(app.stack.depth(), 2, "无组词帧回车应正常激活（推入嵌套页）");

        // ③ 下一帧恢复：组词帧吞掉一次后，后续无 Ime 帧 Enter 正常激活。
        // （headless 队列无真实帧边界——手动清空事件模拟「下一帧」，生产中
        // egui begin_pass 会整帧重建事件表。）
        let mut app = make_app_with_item();
        let ctx = crate::test_support::ctx();
        ctx.input_mut(|i| {
            i.events
                .push(egui::Event::Ime(egui::ImeEvent::Commit("上".to_string())));
        });
        press_enter(&ctx);
        app.handle_keys(&ctx);
        ctx.input_mut(|i| i.events.clear()); // 帧边界
        press_enter(&ctx);
        app.handle_keys(&ctx);
        assert_eq!(app.stack.depth(), 2, "下一帧恢复正常激活");
    }

    /// headless 注入 Enter 按下事件（同 `press_key`）。
    fn press_enter(ctx: &egui::Context) {
        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
    }

    /// 该帧事件队列是否仍含未消费的 Enter 按下事件。
    fn key_enter_in_queue(ctx: &egui::Context) -> bool {
        ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::Key {
                        key: egui::Key::Enter,
                        pressed: true,
                        ..
                    }
                )
            })
        })
    }
}
