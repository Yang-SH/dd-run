//! N14 子项（2026-11-22）：`?` 键位速查页——PageStack 宿主静态页（实施计划
//! 决策点默认 B：复用嵌套页形态 [`PageState::nested`] + 自带 passthrough，
//! 零新渲染路径、零协议面；Esc/Backspace 返回沿用页面栈既有行为）。
//!
//! 触发：根页搜索框为空时按 `?`（page_keys 消费 Text 事件防上屏；非空框 =
//! 普通字符不触发；IME 组词期 R-22 守卫）。条目 = 宿主虚拟行（id `help:<n>`，
//! invoke 无动作——纯展示行，dispatch 侧早退）。键表 zh/en 双语静态条目；
//! **一致性断言**：行键位 ⊆ [`CONSUMED_KEYS`]（page_keys 消费清单口径），
//! 防键表收录了未消费键位的漂移；N12 落地批同步补录 Ctrl+Backspace / Right。

use crate::aggregator::HOST_CUSTOM_EXT_ID;
use crate::navigation::PageState;
use crate::settings::Lang;
use crate::state::PanelItem;
use crate::text;
use dd_protocol::model::CommandRef;

/// 速查页 page_id（幂等判定与 Esc 返回语义共用）。
pub(crate) const HELP_PAGE_ID: &str = "help";

/// page_keys 消费清单口径（handle_keys 文档注释同源；键位增量随落地批登记）。
/// 一致性单测锁定速查页行 ⊆ 本表（仅测试编译消费——非 test 构建免 dead-code）。
#[cfg_attr(not(test), allow(dead_code))]
const CONSUMED_KEYS: &[&str] = &[
    "Escape",
    "↑↓",
    "Enter",
    "Tab / Shift+Tab",
    "Home / End",
    "PgUp / PgDn",
    "Backspace",
    "Ctrl+,",
    "Ctrl+F",
    "Shift+F10",
    "Ctrl+1..9",
    "Ctrl+Enter",
    "Ctrl+↑/↓",
];

/// 速查页行（键位 + 双语说明 + 分组键）。
struct HelpRow {
    keys: &'static str,
    zh: &'static str,
    en: &'static str,
    group_key: &'static str,
}

const HELP_ROWS: &[HelpRow] = &[
    // ── 通用 ──
    HelpRow {
        keys: "Escape",
        zh: "关闭面板（根页）· 返回上级（嵌套页）",
        en: "Close panel (root) · go back (nested)",
        group_key: "help.group.basics",
    },
    HelpRow {
        keys: "Ctrl+,",
        zh: "打开设置",
        en: "Open settings",
        group_key: "help.group.basics",
    },
    HelpRow {
        keys: "Ctrl+F",
        zh: "一键进入文件搜索",
        en: "Open file search",
        group_key: "help.group.basics",
    },
    // ── 列表 ──
    HelpRow {
        keys: "↑↓",
        zh: "列表导航（移动选中）",
        en: "Move selection",
        group_key: "help.group.list",
    },
    HelpRow {
        keys: "Tab / Shift+Tab",
        zh: "列表导航（环绕）",
        en: "Move selection (wrap)",
        group_key: "help.group.list",
    },
    HelpRow {
        keys: "Home / End",
        zh: "跳到列表首 / 尾",
        en: "Jump to first / last",
        group_key: "help.group.list",
    },
    HelpRow {
        keys: "PgUp / PgDn",
        zh: "列表翻页",
        en: "Page up / down",
        group_key: "help.group.list",
    },
    HelpRow {
        keys: "Ctrl+1..9",
        zh: "直开可见列表第 N 行",
        en: "Activate visible row N",
        group_key: "help.group.list",
    },
    // ── 搜索框 ──
    HelpRow {
        keys: "Backspace",
        zh: "输入框删字 · 退格返回（可配置）",
        en: "Delete char · go back (configurable)",
        group_key: "help.group.search",
    },
    HelpRow {
        keys: "Ctrl+↑/↓",
        zh: "搜索历史回填（↑更旧 / ↓更新）",
        en: "Fill from search history (Up = older / Down = newer)",
        group_key: "help.group.search",
    },
    // ── 结果行 ──
    HelpRow {
        keys: "Enter",
        zh: "激活选中项",
        en: "Activate selected",
        group_key: "help.group.results",
    },
    HelpRow {
        keys: "Ctrl+Enter",
        zh: "打开所在位置 / 复制文本或链接",
        en: "Reveal location / copy text or link",
        group_key: "help.group.results",
    },
    HelpRow {
        keys: "Shift+F10",
        zh: "打开选中项右键菜单",
        en: "Open context menu for selected",
        group_key: "help.group.results",
    },
];

/// 速查页静态条目（宿主虚拟：id = `help:<序号>`，invoke 无动作——纯展示行；
/// title = 键位、subtitle = 说明[双语]；拼音索引随宿主管线生成）。
fn help_items(lang: Lang) -> Vec<PanelItem> {
    let desc = |r: &HelpRow| match lang {
        Lang::ZhCn => r.zh,
        _ => r.en,
    };
    HELP_ROWS
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let title = r.keys.to_string();
            PanelItem {
                id: format!("help:{i}"),
                ext_id: HOST_CUSTOM_EXT_ID.to_string(),
                pinyin: crate::state::pinyin_haystack(&title),
                title,
                subtitle: desc(r).to_string(),
                section: text::t(lang, r.group_key).to_string(),
                icon: None,
                tags: Vec::new(),
                result_category: None,
                command: CommandRef::Invoke,
                more_commands: Vec::new(),
            }
        })
        .collect()
}

impl crate::app::PaletteApp {
    /// N14 子项：打开 `?` 键位速查页（根页空框 `?` 触发；已在速查页时幂等
    /// 不重推——同 open_settings 口径）。
    pub(crate) fn open_help_page(&mut self) {
        if self.view.stack.current().page_id.as_deref() == Some(HELP_PAGE_ID) {
            return;
        }
        let lang = self.view.lang_effective;
        log::debug!("[dd-gui] 打开键位速查页（PageStack 推页）");
        self.view.stack.push(PageState::nested(
            HELP_PAGE_ID,
            text::t(lang, "help.title"),
            HOST_CUSTOM_EXT_ID,
            help_items(lang),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n14_help_rows_shape_and_consumed_keys_consistency() {
        let items = help_items(Lang::ZhCn);
        assert_eq!(items.len(), HELP_ROWS.len());
        // 一致性断言：行键位 ⊆ page_keys 消费清单口径（防键表漂移）
        for r in HELP_ROWS {
            assert!(
                CONSUMED_KEYS.contains(&r.keys),
                "速查页键位 {} 未在 page_keys 消费清单登记",
                r.keys
            );
        }
        // 条目形态：id 唯一、宿主虚拟、Invoke（dispatch 侧 help: 早退无动作）
        let mut ids: Vec<_> = items.iter().map(|i| i.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), items.len(), "id 唯一");
        assert!(items.iter().all(|i| i.ext_id == HOST_CUSTOM_EXT_ID));
        assert!(items
            .iter()
            .all(|i| matches!(i.command, CommandRef::Invoke)));
        assert!(items.iter().all(|i| !i.subtitle.is_empty()));
        assert!(items.iter().all(|i| !i.section.is_empty()));
        // 分组键齐全（zh 渲染非原键名）
        assert!(items.iter().all(|i| !i.section.starts_with("help.")));
    }
}
