//! 右键菜单：纯逻辑映射、开合与激活（设计稿 10B，D18/D19）。

use crate::app::PaletteApp;
use crate::platform::reveal_in_folder;
use crate::platform::run_as_admin;
use crate::text::default_action_glyph;
use crate::text::footer_action_text;
use crate::text::path_like;
use crate::text::url_like;
use crate::text::CTX_GLYPH_ADMIN;
use crate::text::CTX_GLYPH_COPY;
use crate::text::CTX_GLYPH_FOLDER;
use crate::text::CTX_GLYPH_LINK;
use dd_gui::settings::Lang;
use dd_gui::state::PanelItem;
use dd_gui::theme;
use eframe::egui;

// ── 右键菜单（设计稿 10B，v4.4）─────────────────────────────────────────

/// 右键菜单动作（10B.2 映射表；D18：GUI 层静态硬编码，协议零改动）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CtxAction {
    /// 默认动作 = 页脚/Enter 同款：按 `CommandRef` 分派（invoke / 进入页）。
    Default,
    /// 以管理员身份运行（UAC 提权，`ShellExecuteW` verb=runas）。
    RunAsAdmin { path: String },
    /// 在资源管理器中定位宿主目录（`explorer /select,<path>`）。
    RevealInFolder { path: String },
    /// 复制路径 / 链接到剪贴板。
    CopyText { text: String },
}

/// 右键菜单项（10B.1：图标 glyph 16 + 名称 body1 14 + 快捷键 caption1/fg3）。
#[derive(Clone)]
pub(crate) struct CtxEntry {
    pub(crate) glyph: char,
    pub(crate) label: String,
    pub(crate) shortcut: &'static str,
    pub(crate) action: CtxAction,
}

/// 菜单行：动作项或分组分隔线（10B.1：1px stroke2、垂直 margin 4、内缩 8）。
#[derive(Clone)]
pub(crate) enum CtxRow {
    Entry(CtxEntry),
    Separator,
}

/// 打开中的右键菜单状态。
pub(crate) struct CtxMenuState {
    /// 锚定项在**可见列表**中的索引（默认动作执行前置为选中）。
    pub(crate) visible_idx: usize,
    /// 锚定项 id（激活前校验索引未因列表刷新漂移——防陈旧）。
    pub(crate) item_id: String,
    /// 菜单内容（打开时按 D18 映射固化，列表刷新不重算）。
    pub(crate) rows: Vec<CtxRow>,
    /// 锚点（窗口坐标）：指针 = 右键点 + (2,2)；键盘 = 选中行底边左缘。
    pub(crate) anchor: egui::Pos2,
    /// 键盘/悬停焦点（Entry 行下标，不含 Separator）。
    pub(crate) focus: usize,
}

impl PaletteApp {
    // ── 右键菜单（设计稿 10B，v4.4）─────────────────────────────────

    /// 打开右键菜单：按 D18 映射固化菜单内容，焦点落在第一项（默认动作）。
    pub(crate) fn open_ctx_menu(
        &mut self,
        visible_idx: usize,
        item: &PanelItem,
        anchor: egui::Pos2,
    ) {
        let rows = context_menu_rows(self.lang_effective, item);
        eprintln!(
            "[dd-gui] 右键菜单：item={} category={:?}（{} 项）",
            item.id,
            item.result_category,
            rows.iter()
                .filter(|r| matches!(r, CtxRow::Entry(_)))
                .count()
        );
        self.ctx_menu = Some(CtxMenuState {
            visible_idx,
            item_id: item.id.clone(),
            rows,
            anchor,
            focus: 0,
        });
    }

    /// 菜单开着时右键另一行（D19 修正）：捕获层吞掉了该行的
    /// `secondary_clicked`，这里按本帧行矩形（[`Self::ctx_row_rects`]）命中，
    /// 置选中并就地重开菜单；未命中任何行（列表空白处）则仅关闭。
    pub(crate) fn reopen_ctx_menu_at(&mut self, ctx: &egui::Context, pos: egui::Pos2) {
        let hit = self
            .ctx_row_rects
            .iter()
            .find(|(_, r)| r.contains(pos))
            .map(|(i, _)| *i);
        let found = hit.and_then(|idx| {
            self.stack
                .current()
                .list
                .filtered()
                .find(|(j, _)| *j == idx)
                .map(|(_, it)| (idx, it.clone()))
        });
        let Some((idx, item)) = found else {
            return;
        };
        self.stack.current_mut().list.set_selected(idx);
        self.last_hovered_index = Some(idx);
        self.scroll_follow = false;
        self.open_ctx_menu(
            idx,
            &item,
            pos + egui::vec2(theme::CTX_ANCHOR_OFFSET, theme::CTX_ANCHOR_OFFSET),
        );
        ctx.request_repaint();
    }

    /// 激活焦点项（Enter / 单击）：默认动作 = Enter 同款（先置选中再按
    /// `CommandRef` 分派）；其余动作宿主侧直接执行，结果经 Toast/eprintln 反馈。
    /// 激活同时关闭菜单（单例浮层语义）。
    pub(crate) fn activate_ctx_menu(&mut self, ctx: &egui::Context) {
        let Some(state) = self.ctx_menu.take() else {
            return;
        };
        let Some(entry) = state
            .rows
            .iter()
            .filter_map(|r| match r {
                CtxRow::Entry(e) => Some(e),
                CtxRow::Separator => None,
            })
            .nth(state.focus)
            .cloned()
        else {
            return;
        };
        match &entry.action {
            CtxAction::Default => {
                // 防陈旧：列表可能已刷新（items_changed / fallback 重算）导致
                // 索引漂移——激活前校验，失配则丢弃（菜单已随 take 关闭）。
                let current_matches = self
                    .stack
                    .current()
                    .list
                    .filtered()
                    .any(|(i, it)| i == state.visible_idx && it.id == state.item_id);
                if !current_matches {
                    eprintln!(
                        "[dd-gui] 右键菜单：列表已刷新，丢弃陈旧激活（item={}）",
                        state.item_id
                    );
                    return;
                }
                self.stack
                    .current_mut()
                    .list
                    .set_selected(state.visible_idx);
                self.scroll_follow = false;
                self.confirm_selected();
            }
            CtxAction::RunAsAdmin { path } => match run_as_admin(path) {
                Ok(()) => eprintln!("[dd-gui] 以管理员身份运行请求已发起：{path}"),
                Err(e) => self.show_error_toast(self.tr("ctx.admin_fail").replace("{e}", &e)),
            },
            CtxAction::RevealInFolder { path } => match reveal_in_folder(path) {
                Ok(()) => eprintln!("[dd-gui] 已在资源管理器中定位：{path}"),
                Err(e) => self.show_error_toast(self.tr("ctx.locate_fail").replace("{e}", &e)),
            },
            CtxAction::CopyText { text } => {
                ctx.copy_text(text.clone());
                self.show_toast(self.tr("ctx.copied"), None);
            }
        }
    }
}

// ── 右键菜单：纯逻辑映射与平台动作（可单测，设计稿 10B / v4.4）─────────

/// 菜单中动作项计数（键盘焦点循环用，Separator 不计）。
pub(crate) fn ctx_entry_count(state: Option<&CtxMenuState>) -> usize {
    state
        .map(|s| {
            s.rows
                .iter()
                .filter(|r| matches!(r, CtxRow::Entry(_)))
                .count()
        })
        .unwrap_or(0)
}

/// 右键菜单内容映射（10B.2，D18：GUI 层静态硬编码、协议零改动）。
///
/// 第一项恒为默认动作（= 页脚 / Enter 同款，[`footer_action_text`]），
/// 其后按 `result_category` 追加二级动作，分组间用分隔线（10B.1）。
/// 二级动作按**数据可用性**门控（偏离记档：设计稿映射为理想形态，实现以
/// 项内实际可得的路径/URL 数据为准）：
/// - 路径型动作（提权 / 定位 / 复制路径）要求 subtitle 为绝对路径（盘符/UNC）；
/// - 「复制链接」要求 subtitle 为 http(s) URL；
///
/// 数据不可得时该项**不出现**（而非禁用）——不渲染无法兑现的动作。
/// 菜单标签按 `lang` 走 i18n 表（v4.13 D38）；`result_category` 的中文值是
/// 协议侧数据（比对用），不翻译。
pub(crate) fn context_menu_rows(lang: Lang, item: &PanelItem) -> Vec<CtxRow> {
    let mut rows = vec![CtxRow::Entry(CtxEntry {
        glyph: default_action_glyph(item),
        label: footer_action_text(lang, item),
        shortcut: "↵ Enter",
        action: CtxAction::Default,
    })];
    match item.result_category.as_deref() {
        Some("应用") | Some("文件") => {
            let Some(path) = path_like(&item.subtitle) else {
                return rows;
            };
            if item.result_category.as_deref() == Some("应用") {
                rows.push(CtxRow::Entry(CtxEntry {
                    glyph: CTX_GLYPH_ADMIN,
                    label: crate::text::t(lang, "ctx.admin").to_string(),
                    shortcut: "Ctrl+↵",
                    action: CtxAction::RunAsAdmin { path: path.clone() },
                }));
            }
            rows.push(CtxRow::Entry(CtxEntry {
                glyph: CTX_GLYPH_FOLDER,
                label: crate::text::t(lang, "ctx.locate").to_string(),
                shortcut: "",
                action: CtxAction::RevealInFolder { path: path.clone() },
            }));
            rows.push(CtxRow::Separator);
            rows.push(CtxRow::Entry(CtxEntry {
                glyph: CTX_GLYPH_COPY,
                label: crate::text::t(lang, "ctx.copy_path").to_string(),
                shortcut: "Ctrl+Shift+C",
                action: CtxAction::CopyText { text: path },
            }));
        }
        Some("网页") => {
            if let Some(url) = url_like(&item.subtitle) {
                rows.push(CtxRow::Separator);
                rows.push(CtxRow::Entry(CtxEntry {
                    glyph: CTX_GLYPH_LINK,
                    label: crate::text::t(lang, "ctx.copy_link").to_string(),
                    shortcut: "Ctrl+Shift+C",
                    action: CtxAction::CopyText { text: url },
                }));
            }
        }
        // 命令 / 设置 / 未知类别：仅默认动作（10B.2）
        _ => {}
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── v4.4 10B：右键菜单映射（D18）与数据门控 ────────────────────

    /// 构造指定类别 + subtitle 的列表项。
    fn ctx_item(category: &str, subtitle: &str) -> PanelItem {
        PanelItem {
            result_category: Some(category.to_string()),
            subtitle: subtitle.to_string(),
            ext_id: "com.ddrun.apps".to_string(),
            ..PanelItem::new("app.1")
        }
    }

    /// 菜单行中的动作标签序列（跳过分隔线）。
    fn entry_labels(rows: &[CtxRow]) -> Vec<&str> {
        rows.iter()
            .filter_map(|r| match r {
                CtxRow::Entry(e) => Some(e.label.as_str()),
                CtxRow::Separator => None,
            })
            .collect()
    }

    #[test]
    fn ctx_menu_app_with_path_gets_full_mapping() {
        // 10B.2 应用：打开 / 以管理员身份运行 / 打开所在位置 / ─ / 复制路径
        let item = ctx_item("应用", r"C:\WINDOWS\system32\AgentService.exe");
        let rows = context_menu_rows(Lang::ZhCn, &item);
        assert_eq!(
            entry_labels(&rows),
            vec!["打开应用", "以管理员身份运行", "打开所在位置", "复制路径"]
        );
        // 分隔线恰在「复制路径」前（默认/高频组与复制组之间，10B.0 解剖图）
        assert!(
            matches!(rows[rows.len() - 2], CtxRow::Separator),
            "分隔线在复制路径之前"
        );
        // 第一项 = 默认动作，快捷键 ↵ Enter（10B.1 默认动作行）
        match &rows[0] {
            CtxRow::Entry(e) => {
                assert_eq!(e.shortcut, "↵ Enter");
                assert_eq!(e.action, CtxAction::Default);
            }
            CtxRow::Separator => panic!("首项必须是动作"),
        }
    }

    #[test]
    fn ctx_menu_gates_secondary_actions_on_data_availability() {
        // subtitle 无路径形态（如无目标路径的项）→ 仅默认动作，不渲染无法兑现的项
        let item = ctx_item("应用", "");
        assert_eq!(
            entry_labels(&context_menu_rows(Lang::ZhCn, &item)),
            vec!["打开应用"]
        );
        let item = ctx_item("应用", "快捷方式");
        assert_eq!(
            entry_labels(&context_menu_rows(Lang::ZhCn, &item)),
            vec!["打开应用"]
        );
    }

    #[test]
    fn ctx_menu_file_category_has_no_admin_entry() {
        // 10B.2 文件：无管理员运行项（非可执行语义）
        let item = ctx_item("文件", r"G:\AI\dd-run\Cargo.toml");
        assert_eq!(
            entry_labels(&context_menu_rows(Lang::ZhCn, &item)),
            vec!["打开应用", "打开所在位置", "复制路径"]
        );
    }

    #[test]
    fn ctx_menu_web_url_gets_copy_link_hint_text_does_not() {
        // 10B.2 网页：URL 形态 subtitle → 复制链接；提示文案（现行 websearch
        // 顶层项）不含 URL → 仅默认动作（门控规则，偏离记档见函数 doc）
        let mut item = ctx_item("网页", "https://example.com/search?q=dd");
        item.ext_id = "com.ddrun.websearch".to_string();
        let rows = context_menu_rows(Lang::ZhCn, &item);
        assert_eq!(entry_labels(&rows), vec!["打开网页", "复制链接"]);
        assert!(matches!(rows[rows.len() - 2], CtxRow::Separator));
        let mut item = ctx_item("网页", "输入关键词后在浏览器中搜索");
        item.ext_id = "com.ddrun.websearch".to_string();
        assert_eq!(
            entry_labels(&context_menu_rows(Lang::ZhCn, &item)),
            vec!["打开网页"]
        );
    }

    #[test]
    fn ctx_menu_command_and_settings_default_only() {
        // 10B.2 命令 / 设置：仅默认动作，菜单保持单项
        for category in ["命令", "设置"] {
            let item = ctx_item(category, "任意副标题");
            assert_eq!(
                entry_labels(&context_menu_rows(Lang::ZhCn, &item)),
                vec!["打开应用"],
                "{category} 仅默认动作"
            );
        }
    }

    #[test]
    fn ctx_menu_focus_count_skips_separators() {
        // 键盘焦点循环只在动作项上进行（Separator 不占焦点位）
        let item = ctx_item("应用", r"C:\x\y.exe");
        let rows = context_menu_rows(Lang::ZhCn, &item);
        let state = CtxMenuState {
            visible_idx: 0,
            item_id: "app.1".to_string(),
            rows,
            anchor: egui::Pos2::ZERO,
            focus: 0,
        };
        assert_eq!(ctx_entry_count(Some(&state)), 4, "4 个动作项 + 1 条分隔线");
        assert_eq!(ctx_entry_count(None), 0, "菜单关闭 = 无焦点项");
    }
}
