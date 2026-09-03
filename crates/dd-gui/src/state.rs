//! 面板纯逻辑层（不依赖 egui，可独立单测）。
//!
//! `PanelState` 承载单页列表的状态机：过滤查询、选中索引、可见性切换。
//! 键盘语义对齐设计文档 §4.3：
//! - `↑`/`↓` 或 `Tab`/`Shift+Tab`：在过滤后的列表项间移动
//!   （`move_up` / `move_down`）；
//! - `Enter`：`confirm` 返回当前选中项（默认命令/进入页）；
//! - `Esc`：关闭面板或返回上一级（由 [`crate::navigation`] 页面栈裁决）。
//!
//! 本节是 M1–M2 的"逻辑自动化"部分：所有过滤/选中/循环语义
//! 都在这里单测覆盖，egui 层只做渲染与按键转发。

use dd_protocol::model::CommandRef;

/// 一个可展示的列表项（对应设计文档 §4.4 `IListItem` 的核心字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelItem {
    /// 命令 id（`invoke` / `get_command` 的入参，§8.1 `CommandItem.id`）。
    pub id: String,
    /// 命令所属扩展的清单 id（`invoke` / `get_items` 时用于定位子进程）。
    pub ext_id: String,
    pub title: String,
    pub subtitle: String,
    /// 所属分组名（§4.4 `Section`）。
    pub section: String,
    /// 标签（§4.4 `Tags`，渲染为 chip）。
    pub tags: Vec<String>,
    /// 选中这一项会发生什么（§8.2：直接执行 / 进入嵌套页）。
    pub command: CommandRef,
}

impl PanelItem {
    /// 仅凭标题构造（id 缺省与标题相同，命令缺省为直接执行，无扩展来源）。
    pub fn new(title: impl Into<String>) -> Self {
        let title = title.into();
        Self {
            id: title.clone(),
            ext_id: String::new(),
            title,
            subtitle: String::new(),
            section: String::new(),
            tags: Vec::new(),
            command: CommandRef::Invoke,
        }
    }
}

/// 过滤后列表中当前选中的索引。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selected {
    /// 过滤结果为空，无选中项。
    None,
    /// 选中过滤后列表的第 `idx` 项（`0 ≤ idx < visible_count`）。
    Some(usize),
}

/// Root View 面板状态机。
///
/// 不持有过滤后的缓存列表，而是实时按 `query` 从 `items` 过滤，
/// 保证"查询字符串 ↔ 可见列表 ↔ 选中索引"三者永远一致（SSOT）。
///
/// M4 宿主 fallback 轮：`fallback` 是"当前查询无匹配时的兜底展示集"
/// （宿主按 §6.2 从扩展 `fallback_commands` 模板渲染得到，见 [`crate::fallback`]）。
/// 显示规则：查询**非空**且常规 `items` **全部不匹配**时，可见列表切换为
/// `fallback`（原样展示，不再二次过滤——它们已按当前查询渲染好）；
/// 其余情况（空查询 / 常规有匹配）与 M1–M3 一致，fallback 不参与。
#[derive(Debug, Clone, PartialEq)]
pub struct PanelState {
    items: Vec<PanelItem>,
    query: String,
    /// 当前查询无匹配时的兜底展示集（空 = 无兜底项可用）。
    fallback: Vec<PanelItem>,
    selected: Selected,
}

impl PanelState {
    pub fn new(items: Vec<PanelItem>) -> Self {
        let mut s = Self {
            items,
            query: String::new(),
            fallback: Vec::new(),
            selected: Selected::None,
        };
        s.reset_selection();
        s
    }

    pub fn items(&self) -> &[PanelItem] {
        &self.items
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// 设置当前查询的兜底展示集（宿主在拉取模板并渲染后调用）。
    /// 空查询时忽略（兜底仅在查询非空场景参与显示）。
    pub fn set_fallback(&mut self, items: Vec<PanelItem>) {
        if !self.query.is_empty() {
            self.fallback = items;
            self.clamp_selection();
        }
    }

    /// 清空兜底集（如进入嵌套页 / 回根视图时重置）。
    pub fn clear_fallback(&mut self) {
        self.fallback.clear();
        self.clamp_selection();
    }

    /// 设置查询文本；查询变化时选中索引自动夹紧（可能变成 None）。
    pub fn set_query(&mut self, q: impl Into<String>) {
        self.query = q.into();
        self.clamp_selection();
    }

    /// 当前查询下可见项个数（fallback 模式时 = fallback 长度）。
    pub fn visible_count(&self) -> usize {
        self.filtered().count()
    }

    /// 可见项迭代器（**fallback 模式下不二次过滤**——项已按查询渲染好）：
    /// `(可见下标, 原始项)`，供渲染与选中高亮使用。
    pub fn filtered(&self) -> Box<dyn Iterator<Item = (usize, &PanelItem)> + '_> {
        if self.is_fallback_mode() {
            return Box::new(self.fallback.iter().enumerate());
        }
        Box::new(
            self.items
                .iter()
                .filter(|it| Self::is_visible(it, &self.query))
                .enumerate(),
        )
    }

    /// 是否处于 fallback 展示模式：查询非空、常规项全部不匹配、且有兜底项。
    pub fn is_fallback_mode(&self) -> bool {
        if self.query.is_empty() || self.fallback.is_empty() {
            return false;
        }
        !self
            .items
            .iter()
            .any(|it| Self::is_visible(it, &self.query))
    }

    /// 常规过滤（不含 fallback）是否有匹配项——宿主据此决定是否触发
    /// `fallback_commands` 拉取（有匹配则不拉，§6.2"搜索无匹配时"）。
    pub fn has_regular_match(&self) -> bool {
        self.query.is_empty()
            || self
                .items
                .iter()
                .any(|it| Self::is_visible(it, &self.query))
    }

    pub fn selected(&self) -> Selected {
        self.selected
    }

    /// 选中项的下标（过滤后列表中的位置），无选中时返回 None。
    pub fn selected_index(&self) -> Option<usize> {
        match self.selected {
            Selected::Some(i) => Some(i),
            Selected::None => None,
        }
    }

    /// 当前选中的列表项（原始 `items` 中的引用）。
    pub fn selected_item(&self) -> Option<&PanelItem> {
        self.selected_index()
            .and_then(|i| self.filtered().nth(i).map(|(_, it)| it))
    }

    /// `↓`：下移一个；已在末尾时回到开头（环绕）。空列表无操作。
    pub fn move_down(&mut self) {
        let n = self.visible_count();
        if n == 0 {
            self.selected = Selected::None;
            return;
        }
        self.selected = match self.selected {
            Selected::None => Selected::Some(0),
            Selected::Some(i) if i + 1 < n => Selected::Some(i + 1),
            Selected::Some(_) => Selected::Some(0), // 环绕
        };
    }

    /// `↑`：上移一个；已在开头时回到末尾（环绕）。空列表无操作。
    pub fn move_up(&mut self) {
        let n = self.visible_count();
        if n == 0 {
            self.selected = Selected::None;
            return;
        }
        self.selected = match self.selected {
            Selected::None => Selected::Some(n - 1),
            Selected::Some(0) => Selected::Some(n - 1), // 环绕
            Selected::Some(i) => Selected::Some(i - 1),
        };
    }

    /// `Enter`：返回当前选中项（若存在）。
    pub fn confirm(&self) -> Option<&PanelItem> {
        self.selected_item()
    }

    /// 鼠标点击/悬停：把选中项定位到过滤后列表的第 `idx` 项。
    /// 越界（如 `idx ≥ visible_count`）则忽略，不改变当前选中（防御性）。
    /// 返回选中是否**真的变化**（调用方据此决定是否强制重绘——
    /// egui 按需重绘模型下，false 时的重绘是纯浪费）。
    pub fn set_selected(&mut self, idx: usize) -> bool {
        if idx < self.visible_count() && self.selected_index() != Some(idx) {
            self.selected = Selected::Some(idx);
            true
        } else {
            false
        }
    }

    /// 查询与选中回落到初始态（面板重新唤起时调用）。
    pub fn reset(&mut self) {
        self.query.clear();
        self.fallback.clear();
        self.reset_selection();
    }

    fn reset_selection(&mut self) {
        self.selected = if self.visible_count() > 0 {
            Selected::Some(0)
        } else {
            Selected::None
        };
    }

    /// 选中索引夹紧到 [0, visible_count)，越界则归零；空列表置 None。
    fn clamp_selection(&mut self) {
        let n = self.visible_count();
        self.selected = match self.selected {
            Selected::None if n > 0 => Selected::Some(0),
            Selected::Some(i) if i >= n && n > 0 => Selected::Some(0),
            Selected::Some(_) if n == 0 => Selected::None,
            other => other,
        };
    }

    /// 过滤：大小写不敏感的子串匹配（title + subtitle + tags + section 任一命中）。
    fn is_visible(item: &PanelItem, q: &str) -> bool {
        if q.is_empty() {
            return true;
        }
        let q = q.to_lowercase();
        let hay = format!(
            "{} {} {} {}",
            item.title,
            item.subtitle,
            item.section,
            item.tags.join(" ")
        )
        .to_lowercase();
        hay.contains(&q)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_items() -> Vec<PanelItem> {
        vec![
            PanelItem {
                id: "settings".into(),
                ext_id: "ext.a".into(),
                title: "Open Settings".into(),
                subtitle: "Open the settings page".into(),
                section: "System".into(),
                tags: vec!["config".into()],
                command: CommandRef::Invoke,
            },
            PanelItem {
                id: "file".into(),
                ext_id: "ext.a".into(),
                title: "Open File".into(),
                subtitle: "Browse files".into(),
                section: "Files".into(),
                tags: vec!["browse".into()],
                command: CommandRef::Invoke,
            },
            PanelItem {
                id: "copy".into(),
                ext_id: "ext.a".into(),
                title: "Copy Path".into(),
                subtitle: "Copy current path".into(),
                section: "Files".into(),
                tags: vec!["clipboard".into()],
                command: CommandRef::Invoke,
            },
        ]
    }

    #[test]
    fn empty_query_shows_all_and_selects_first() {
        let s = PanelState::new(sample_items());
        assert_eq!(s.visible_count(), 3);
        assert_eq!(s.selected_index(), Some(0));
        assert_eq!(
            s.selected_item().map(|i| i.title.as_str()),
            Some("Open Settings")
        );
    }

    #[test]
    fn query_filters_case_insensitively() {
        let mut s = PanelState::new(sample_items());
        s.set_query("open");
        assert_eq!(s.visible_count(), 2); // Open Settings / Open File
        s.set_query("OPEN");
        assert_eq!(s.visible_count(), 2);
        s.set_query("copy");
        assert_eq!(s.visible_count(), 1);
        assert_eq!(
            s.selected_item().map(|i| i.title.as_str()),
            Some("Copy Path")
        );
    }

    #[test]
    fn query_matches_subtitle_and_tags_and_section() {
        let mut s = PanelState::new(sample_items());
        s.set_query("clipboard"); // tag
        assert_eq!(s.visible_count(), 1);
        s.set_query("browse"); // subtitle
        assert_eq!(s.visible_count(), 1);
        s.set_query("files"); // section（且大小写不敏感：Files → files）
        assert_eq!(s.visible_count(), 2);
    }

    #[test]
    fn no_match_yields_none_selection() {
        let mut s = PanelState::new(sample_items());
        s.set_query("zzz-no-such-query");
        assert_eq!(s.visible_count(), 0);
        assert_eq!(s.selected(), Selected::None);
        assert_eq!(s.confirm(), None);
        // 空列表上移动键无操作
        s.move_down();
        s.move_up();
        assert_eq!(s.selected(), Selected::None);
    }

    #[test]
    fn move_down_wraps_around() {
        let mut s = PanelState::new(sample_items());
        s.move_down();
        assert_eq!(s.selected_index(), Some(1));
        s.move_down();
        assert_eq!(s.selected_index(), Some(2));
        s.move_down(); // 环绕回开头
        assert_eq!(s.selected_index(), Some(0));
    }

    #[test]
    fn move_up_wraps_around() {
        let mut s = PanelState::new(sample_items());
        s.move_up(); // 在开头向上 → 环绕到末尾
        assert_eq!(s.selected_index(), Some(2));
        s.move_up();
        assert_eq!(s.selected_index(), Some(1));
    }

    #[test]
    fn query_change_clamps_selection() {
        let mut s = PanelState::new(sample_items());
        s.set_query("open");
        s.move_down(); // 选中第 2 个（Open File）
        assert_eq!(s.selected_index(), Some(1));
        s.set_query("copy"); // 过滤后只剩 1 项 → 夹紧到 0
        assert_eq!(s.selected_index(), Some(0));
        assert_eq!(s.confirm().map(|i| i.title.as_str()), Some("Copy Path"));
        s.set_query(""); // 清空查询 → 重新选中第一项
        assert_eq!(s.selected_index(), Some(0));
    }

    #[test]
    fn confirm_returns_selected() {
        let mut s = PanelState::new(sample_items());
        assert_eq!(s.confirm().map(|i| i.title.as_str()), Some("Open Settings"));
        s.move_down();
        s.move_down();
        assert_eq!(s.confirm().map(|i| i.title.as_str()), Some("Copy Path"));
    }

    #[test]
    fn reset_clears_query_and_selection() {
        let mut s = PanelState::new(sample_items());
        s.set_query("copy");
        s.reset();
        assert_eq!(s.query(), "");
        assert_eq!(s.visible_count(), 3);
        assert_eq!(s.selected_index(), Some(0));
    }

    #[test]
    fn set_selected_positions_and_ignores_out_of_bounds() {
        let mut s = PanelState::new(sample_items()); // 3 项
        assert!(s.set_selected(2)); // 变化 → true
        assert_eq!(s.selected_index(), Some(2));
        assert!(!s.set_selected(99)); // 越界：忽略 → false
        assert_eq!(s.selected_index(), Some(2));
        assert!(s.set_selected(0));
        assert_eq!(s.selected_index(), Some(0));
        assert!(!s.set_selected(0)); // 同项重复设置：无变化 → false
        assert_eq!(s.selected_index(), Some(0));
    }

    fn fallback_item(id: &str, title: &str) -> PanelItem {
        PanelItem {
            id: id.to_string(),
            ext_id: "com.ddrun.calc".to_string(),
            title: title.to_string(),
            subtitle: String::new(),
            section: "计算".to_string(),
            tags: Vec::new(),
            command: CommandRef::Invoke,
        }
    }

    #[test]
    fn fallback_shows_only_when_query_nonempty_and_no_regular_match() {
        let mut s = PanelState::new(sample_items());

        // 空查询：fallback 不参与，全部常规项可见（此时 set_fallback 被忽略）
        s.set_fallback(vec![fallback_item("calc.eval.query", "= {query}")]);
        assert_eq!(s.query(), "");
        assert!(!s.is_fallback_mode());
        assert_eq!(s.visible_count(), 3);

        // 查询命中常规项：fallback 不参与（即使注入了 fallback 集）
        s.set_query("open");
        s.set_fallback(vec![fallback_item("calc.eval.query", "= open")]);
        assert!(!s.is_fallback_mode(), "常规有匹配时不进入 fallback");
        assert_eq!(s.visible_count(), 2);

        // 查询无匹配：注入渲染好的兜底项 → 进入 fallback 模式
        s.set_query("zzz-no-match");
        s.set_fallback(vec![fallback_item("calc.eval.query", "= zzz-no-match")]);
        assert!(s.is_fallback_mode());
        assert_eq!(s.visible_count(), 1);
        assert_eq!(
            s.selected_item().map(|i| i.id.as_str()),
            Some("calc.eval.query")
        );
        assert_eq!(s.selected_index(), Some(0));
        // 兜底项在 fallback 模式下可见 title 为渲染后文本
        assert_eq!(
            s.selected_item().map(|i| i.title.as_str()),
            Some("= zzz-no-match")
        );
    }

    #[test]
    fn set_fallback_updates_items_and_selection_clamps() {
        let mut s = PanelState::new(sample_items());
        s.set_query("zzz");
        // 先给 2 个兜底项，选中第二个
        s.set_fallback(vec![fallback_item("a", "A"), fallback_item("b", "B")]);
        s.move_down();
        assert_eq!(s.selected_index(), Some(1));
        // fallback 集缩小到 1 → 选中夹紧回 0
        s.set_fallback(vec![fallback_item("c", "C")]);
        assert_eq!(s.selected_index(), Some(0));
        assert_eq!(s.confirm().map(|i| i.id.as_str()), Some("c"));
    }

    #[test]
    fn set_fallback_ignored_when_query_empty() {
        let mut s = PanelState::new(sample_items());
        // 空查询时 set_fallback 不生效（fallback 仅在查询非空参与）
        s.set_fallback(vec![fallback_item("x", "X")]);
        assert!(!s.is_fallback_mode());
        assert_eq!(s.visible_count(), 3);
    }

    #[test]
    fn has_regular_match_distinguishes() {
        let mut s = PanelState::new(sample_items());
        s.set_fallback(vec![fallback_item("x", "X")]);
        assert!(s.has_regular_match(), "空查询视为有匹配（显示全部）");
        s.set_query("open");
        assert!(s.has_regular_match());
        s.set_query("zzz-no-match");
        assert!(!s.has_regular_match(), "常规项全不匹配 → 触发兜底拉取");
    }

    #[test]
    fn reset_clears_fallback() {
        let mut s = PanelState::new(sample_items());
        s.set_query("zzz");
        s.set_fallback(vec![fallback_item("x", "X")]);
        assert!(s.is_fallback_mode());
        s.reset();
        assert_eq!(s.query(), "");
        assert!(!s.is_fallback_mode());
        assert_eq!(s.visible_count(), 3);
    }

    #[test]
    fn clear_fallback_restores_regular_empty_state() {
        let mut s = PanelState::new(sample_items());
        s.set_query("zzz");
        s.set_fallback(vec![fallback_item("x", "X")]);
        assert_eq!(s.visible_count(), 1);
        s.clear_fallback();
        assert_eq!(s.visible_count(), 0, "清空兜底后回到常规空态");
        assert_eq!(s.selected(), Selected::None);
    }
}
