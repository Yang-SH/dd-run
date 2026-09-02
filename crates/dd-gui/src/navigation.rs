//! 页面栈（纯逻辑，不依赖 egui，可单测）。
//!
//! 对齐设计文档 §4.3 步骤 5 与 implementation.md M2「页面栈」任务：
//! - 栈底恒为 **Root 视图**（首屏聚合页，`page_id = None`）；
//! - 选中 `CommandRef::Page` 命令 → [`PageStack::push`] 进入嵌套页
//!   （§6.3 `get_items` 的 `page_id`）；
//! - 返回导航**由命令执行结果驱动**：`GoBack` 弹栈（已在 Root 时返回 `None`，
//!   由 UI 层决定关闭面板）、`GoHome` 清空到只剩 Root。
//!
//! 每页的列表状态机复用 [`PanelState`]（过滤/选中/夹紧语义一致）。

use crate::state::{PanelItem, PanelState};

/// 页面栈中的一页。
#[derive(Debug, Clone, PartialEq)]
pub struct PageState {
    /// `None` = Root 视图；`Some` = 嵌套页 id（§6.3 `get_items` 的 `page_id`）。
    pub page_id: Option<String>,
    /// 该页内容所属扩展的清单 id（**Root 为空**——它是多扩展聚合结果）；
    /// 嵌套页的刷新（`items_changed` → `get_items`）据此定位子进程。
    pub ext_id: String,
    /// 页标题（嵌套页来自 `PageInfo.title`）。
    pub title: String,
    /// 该页的列表状态机（过滤/选中/环绕/夹紧）。
    pub list: PanelState,
    /// 是否正在拉取（Loading 态，设计稿界面 11）。
    pub is_loading: bool,
    /// 空态提示文案（`list` 为空且 `is_loading == false` 时展示，界面 10）。
    pub empty: Option<String>,
}

impl PageState {
    /// Root 视图页（首屏聚合结果，不隶属于单个扩展）。
    pub fn root(items: Vec<PanelItem>) -> Self {
        Self {
            page_id: None,
            ext_id: String::new(),
            title: String::new(),
            list: PanelState::new(items),
            is_loading: false,
            empty: None,
        }
    }

    /// 嵌套页（`get_items` 拉取后的列表，`ext_id` 为内容来源扩展）。
    pub fn nested(
        page_id: impl Into<String>,
        title: impl Into<String>,
        ext_id: impl Into<String>,
        items: Vec<PanelItem>,
    ) -> Self {
        Self {
            page_id: Some(page_id.into()),
            ext_id: ext_id.into(),
            title: title.into(),
            list: PanelState::new(items),
            is_loading: false,
            empty: None,
        }
    }
}

/// 页面栈：栈底恒为 Root。
#[derive(Debug, Clone)]
pub struct PageStack {
    pages: Vec<PageState>,
}

impl PageStack {
    /// 以 Root 页初始化（`depth == 1`）。
    pub fn new(root: PageState) -> Self {
        Self { pages: vec![root] }
    }

    /// 当前栈深（1 = 在 Root）。
    pub fn depth(&self) -> usize {
        self.pages.len()
    }

    /// 是否在 Root 视图。
    pub fn at_root(&self) -> bool {
        self.pages.len() == 1
    }

    /// 当前页引用。
    pub fn current(&self) -> &PageState {
        self.pages.last().expect("栈底恒为 Root，永不为空")
    }

    /// 当前页可变引用。
    pub fn current_mut(&mut self) -> &mut PageState {
        self.pages.last_mut().expect("栈底恒为 Root，永不为空")
    }

    /// Root 页可变引用（聚合结果到达时替换列表用）。
    pub fn root_mut(&mut self) -> &mut PageState {
        self.pages.first_mut().expect("栈底恒为 Root，永不为空")
    }

    /// 进入嵌套页（`CommandRef::Page` 命中时）。
    pub fn push(&mut self, page: PageState) {
        self.pages.push(page);
    }

    /// `GoBack`：返回上一级。已在 Root 时返回 `None`（由 UI 层决定关闭面板）。
    pub fn go_back(&mut self) -> Option<PageState> {
        if self.pages.len() <= 1 {
            return None;
        }
        self.pages.pop()
    }

    /// `GoHome`：回根视图（清空到只剩 Root）。
    pub fn go_home(&mut self) {
        self.pages.truncate(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> PanelItem {
        PanelItem::new(id)
    }

    fn root() -> PageStack {
        PageStack::new(PageState::root(vec![item("r1"), item("r2")]))
    }

    #[test]
    fn root_starts_at_depth_one() {
        let stack = root();
        assert_eq!(stack.depth(), 1);
        assert!(stack.at_root());
        assert_eq!(stack.current().page_id, None);
    }

    #[test]
    fn push_enters_nested_page_and_back_returns() {
        let mut stack = root();
        stack.push(PageState::nested(
            "p1",
            "Page One",
            "ext.a",
            vec![item("p1a")],
        ));
        assert_eq!(stack.depth(), 2);
        assert!(!stack.at_root());
        assert_eq!(stack.current().page_id.as_deref(), Some("p1"));
        assert_eq!(stack.current().title, "Page One");
        assert_eq!(stack.current().ext_id, "ext.a", "嵌套页记录来源扩展");

        let popped = stack.go_back().expect("嵌套页可返回");
        assert_eq!(popped.page_id.as_deref(), Some("p1"));
        assert_eq!(stack.depth(), 1);
        assert!(stack.at_root());
    }

    #[test]
    fn go_back_on_root_returns_none() {
        let mut stack = root();
        assert_eq!(stack.go_back(), None, "Root 不可再返回，关闭由 UI 决定");
        assert_eq!(stack.depth(), 1);
    }

    #[test]
    fn go_home_clears_all_nested_pages() {
        let mut stack = root();
        stack.push(PageState::nested("p1", "One", "ext.a", vec![]));
        stack.push(PageState::nested("p2", "Two", "ext.a", vec![]));
        stack.go_home();
        assert_eq!(stack.depth(), 1);
        assert!(stack.at_root());
        assert_eq!(stack.current().page_id, None);
    }

    #[test]
    fn root_mut_replaces_items_keeping_stack() {
        let mut stack = root();
        stack.push(PageState::nested("p1", "One", "ext.a", vec![]));
        let root = stack.root_mut();
        root.list = PanelState::new(vec![item("new")]);
        assert_eq!(stack.depth(), 2, "替换 Root 列表不影响嵌套页");
        assert_eq!(stack.root_mut().list.visible_count(), 1);
    }
}
