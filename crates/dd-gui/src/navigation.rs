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
use std::time::{Duration, Instant};

/// 嵌套页补拉「延迟骨架」窗口（真机 2026-09-14：输入后删一个字「页面闪一下」）。
///
/// 快速补拉（Everything 温热 IPC ~50ms）若在发起瞬间就切骨架屏，骨架只显示
/// 几帧随即被结果替换 → 视觉上「闪一下」。因此补拉发起时**不立即**切骨架，
/// 而是记录到期时刻；到期仍未落地才切。慢补拉（冷启动 sidecar ~1s）照常显示骨架。
pub const PAGE_LOADING_DELAY: Duration = Duration::from_millis(250);

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
    /// 「延迟骨架」到期时刻（[`Self::begin_refetch`] 在列表为空时设置）：
    /// 到期仍未落地 → 渲染骨架；快速补拉在到期前落地 → 全程无骨架闪烁。
    /// `None` = 无延迟骨架（首拉由 `is_loading` 承担 / 有旧结果保留展示）。
    pub skeleton_after: Option<Instant>,
    /// M5 批次 4.0：是否为**宿主设置页**（GUI 本地页，非扩展嵌套页）。
    /// 渲染层据此切换到设置视图（不走 searchbar/列表/兜底链路），
    /// 数据仍由 `list`（空列表）承载以复用键盘状态机。
    pub is_settings: bool,
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
            skeleton_after: None,
            is_settings: false,
        }
    }

    /// 嵌套页（`get_items` 拉取后的列表，`ext_id` 为内容来源扩展）。
    pub fn nested(
        page_id: impl Into<String>,
        title: impl Into<String>,
        ext_id: impl Into<String>,
        items: Vec<PanelItem>,
    ) -> Self {
        let list = PanelState::new(items);
        let mut list = list;
        list.set_passthrough(); // 嵌套页“直通”模式：扩展已过滤/排序，宿主不再二次过滤
        Self {
            page_id: Some(page_id.into()),
            ext_id: ext_id.into(),
            title: title.into(),
            list,
            is_loading: false,
            empty: None,
            skeleton_after: None,
            is_settings: false,
        }
    }

    /// 页内补拉发起前的状态预备（两轮真机反馈合并语义）。
    ///
    /// - **有旧结果** → 保持展示（stale-while-revalidate），不切骨架；在飞指示由
    ///   搜索框右端 spinner 承担（一轮 2026-09-14：边打边搜不闪骨架）；
    /// - **列表为空** → **不立即**切骨架，而是记录延迟到期时刻（[`PAGE_LOADING_DELAY`]）
    ///   ——快速补拉在到期前落地则全程无骨架闪烁（二轮 2026-09-14：输入后删一个字
    ///   「页面闪一下」）；慢补拉到期才显示骨架；
    /// - **不清 `empty`** → 列表为空时清空态会瞬时渲染出另一种空态（`empty.no_match`）
    ///   同样闪烁；过期空态交由落地统一替换。
    pub fn begin_refetch(&mut self, now: Instant) {
        self.skeleton_after = if self.list.visible_count() == 0 {
            Some(now + PAGE_LOADING_DELAY)
        } else {
            None
        };
    }

    /// 延迟骨架是否已到期（渲染层用：`is_loading || skeleton_due(now)` 决定画骨架）。
    pub fn skeleton_due(&self, now: Instant) -> bool {
        self.skeleton_after.is_some_and(|deadline| now >= deadline)
    }

    /// 落地时清除延迟骨架标记（无论成功 / 失败）。
    pub fn clear_refetch(&mut self) {
        self.skeleton_after = None;
    }

    /// M5 批次 4.0：宿主设置页（`page_id` 用 GUI 保留标记
    /// [`crate::settings::SETTINGS_PAGE_ID`]，列表恒为空）。
    pub fn settings() -> Self {
        Self {
            page_id: Some(crate::settings::SETTINGS_PAGE_ID.to_string()),
            ext_id: String::new(),
            title: "设置".to_string(),
            list: PanelState::new(Vec::new()),
            is_loading: false,
            empty: None,
            skeleton_after: None,
            is_settings: true,
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

    /// 补拉预备：列表为空 → **不**立即切骨架，而是延迟到期
    ///（二轮 2026-09-14：快速补拉立即切骨架只闪几帧 → 延迟化）。
    #[test]
    fn begin_refetch_empty_list_defers_skeleton() {
        let mut page = PageState::nested("p1", "文件搜索", "com.ddrun.filesearch", vec![]);
        let t0 = Instant::now();
        page.begin_refetch(t0);
        assert!(!page.is_loading, "不立即切骨架（保持原展示）");
        assert!(!page.skeleton_due(t0), "刚发起：延迟未到 → 不画骨架");
        assert!(
            !page.skeleton_due(t0 + PAGE_LOADING_DELAY - Duration::from_millis(1)),
            "差 1ms 仍未到期"
        );
        assert!(
            page.skeleton_due(t0 + PAGE_LOADING_DELAY),
            "到期 → 画骨架（慢补拉）"
        );
    }

    /// 补拉预备：**不清 `empty`**（清空会瞬时渲染出另一种空态 → 闪烁）。
    #[test]
    fn begin_refetch_keeps_empty_until_landing() {
        let mut page = PageState::nested("p1", "文件搜索", "com.ddrun.filesearch", vec![]);
        page.empty = Some("该页暂无内容".to_string());
        page.begin_refetch(Instant::now());
        assert_eq!(
            page.empty.as_deref(),
            Some("该页暂无内容"),
            "空态文案交由落地替换，不在发起时清空"
        );
    }

    /// 补拉预备：有旧结果 → 保留展示（stale-while-revalidate），不闪骨架。
    #[test]
    fn begin_refetch_keeps_stale_results_without_skeleton() {
        let mut page = PageState::nested(
            "p1",
            "文件搜索",
            "com.ddrun.filesearch",
            vec![item("f1"), item("f2")],
        );
        let t0 = Instant::now();
        page.begin_refetch(t0);
        assert!(!page.is_loading, "有旧结果 → 不进骨架，继续展示");
        assert!(page.skeleton_after.is_none(), "有旧结果 → 无延迟骨架");
        assert!(!page.skeleton_due(t0 + PAGE_LOADING_DELAY));
    }

    /// 落地清除延迟骨架标记（无论成功 / 失败）。
    #[test]
    fn clear_refetch_drops_deferred_skeleton() {
        let mut page = PageState::nested("p1", "文件搜索", "com.ddrun.filesearch", vec![]);
        page.begin_refetch(Instant::now());
        assert!(page.skeleton_after.is_some());
        page.clear_refetch();
        assert!(page.skeleton_after.is_none());
        assert!(!page.skeleton_due(Instant::now() + PAGE_LOADING_DELAY * 4));
    }

    /// 补拉预备：首拉已在 Loading（`open_page`）→ 不改动 `is_loading`，不回退。
    #[test]
    fn begin_refetch_keeps_existing_loading_state() {
        let mut page = PageState::nested("p1", "文件搜索", "com.ddrun.filesearch", vec![]);
        page.is_loading = true;
        page.begin_refetch(Instant::now());
        assert!(page.is_loading, "首拉 Loading 不被预备逻辑改动");
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

    #[test]
    fn settings_page_pushes_and_pops_like_nested_page() {
        // M5 批次 4.0：设置页复用页面栈——Esc（go_back）返回原页面
        let mut stack = root();
        stack.push(PageState::nested("p1", "One", "ext.a", vec![item("x")]));
        stack.push(PageState::settings());
        assert_eq!(stack.depth(), 3);
        assert!(stack.current().is_settings, "栈顶为设置页");
        assert_eq!(stack.current().page_id.as_deref(), Some("__settings__"));
        assert_eq!(stack.current().list.visible_count(), 0, "设置页列表恒为空");

        let popped = stack.go_back().expect("设置页可返回");
        assert!(popped.is_settings);
        assert!(!stack.current().is_settings);
        assert_eq!(
            stack.current().page_id.as_deref(),
            Some("p1"),
            "回到原嵌套页"
        );
    }

    #[test]
    fn settings_page_never_confused_with_nested_page() {
        // 设置页不是扩展嵌套页：is_settings 标记与 page_id 双重区分
        let nested = PageState::nested("p1", "One", "ext.a", vec![]);
        assert!(!nested.is_settings);
        let settings = PageState::settings();
        assert!(settings.is_settings);
        assert_ne!(settings, nested);
    }
}
