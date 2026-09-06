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

use dd_protocol::model::{CommandRef, Icon};

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
    /// 图标（§8.6 三态 glyph/path/url；`None` = 无图标，渲染空列对齐）。
    /// M5 UI 批次 2：`aggregator::to_panel_item` 从 `CommandItem.icon` 透传，
    /// 渲染层统一解析为 20px 图标列。
    pub icon: Option<Icon>,
    /// 标签（§4.4 `Tags`，渲染为 chip）。
    pub tags: Vec<String>,
    /// 结果类别显示标签（设计文档 §6.2：按 `ext_id` 推导，如「应用/命令/设置/网页」）。
    /// 协议层无此字段（`CommandItem` 无 kind），由 GUI 聚合器本地填充。
    pub result_category: Option<String>,
    /// 拼音匹配索引（M6 批次 6.1，L4）：`title` 汉字部分生成的
    /// 「全拼 + 首字母」混合串（如 计算器 → `"jisuanqi jsq"`），供模糊匹配
    /// 命中拼音输入；非汉字字符跳过，纯英文标题为空串。宿主侧预计算
    /// （协议 v1.0 冻结零新增），由 [`pinyin_haystack`] 生成。
    pub pinyin: String,
    /// 选中这一项会发生什么（§8.2：直接执行 / 进入嵌套页）。
    pub command: CommandRef,
}

/// 生成拼音匹配索引：逐字取无声调全拼拼接，再附首字母缩写（M6 批次 6.1）。
///
/// 例：`"计算器"` → `"jisuanqi jsq"`；无汉字（纯英文/数字）→ 空串（原字符
/// 本就是 title 匹配字段，无需重复）。多音字取 pinyin crate 的默认读音。
pub(crate) fn pinyin_haystack(s: &str) -> String {
    use pinyin::ToPinyin;
    let mut full = String::new();
    let mut initials = String::new();
    let mut any = false;
    for p in s.to_pinyin().flatten() {
        any = true;
        full.push_str(p.plain());
        if let Some(c) = p.plain().chars().next() {
            initials.push(c);
        }
    }
    if any {
        full.push(' ');
        full.push_str(&initials);
        full
    } else {
        String::new()
    }
}

impl PanelItem {
    /// 仅凭标题构造（id 缺省与标题相同，命令缺省为直接执行，无扩展来源）。
    pub fn new(title: impl Into<String>) -> Self {
        let title = title.into();
        let pinyin = pinyin_haystack(&title);
        Self {
            id: title.clone(),
            ext_id: String::new(),
            title,
            subtitle: String::new(),
            section: String::new(),
            icon: None,
            tags: Vec::new(),
            result_category: None,
            pinyin,
            command: CommandRef::Invoke,
        }
    }
}

/// 空查询（打开面板首屏）的显示范围（设置页「打开面板时」配置项，
/// 真机反馈 2026-09-04：默认不铺全部应用，只显示默认功能）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmptyQueryView {
    /// 全部项（旧行为）。
    #[default]
    All,
    /// 隐藏「应用」类项（`result_category == "应用"`，来自 Apps 扩展的
    /// 应用本体/lnk 项）；输入查询时应用仍参与模糊匹配。
    WithoutApps,
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
/// M4 P5（A3/D3-A）：`visible` 是**查询变化时一次性计算**的可见索引表
/// （`items` 下标，按 nucleo 得分降序、同分保原序——稳定排序）。
/// `filtered()` 等全部可见性查询都走这张表：既保证"查询字符串 ↔ 可见列表
/// ↔ 选中索引"三者一致（SSOT：查询变则重算），又避免每帧多次重跑匹配
/// （`draw_panel` 每帧调 `set_query`，`filtered()` 每帧被多次消费）。
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
    /// 可见索引表（items 下标；按得分降序、同分保原序）。查询变化时重算。
    visible: Vec<usize>,
    /// 当前查询无匹配时的兜底展示集（空 = 无兜底项可用）。
    fallback: Vec<PanelItem>,
    selected: Selected,
    /// 空查询首屏显示范围（设置项；仅影响空查询分支，见 [`Self::recompute_visible`]）。
    empty_view: EmptyQueryView,
}

impl PanelState {
    pub fn new(items: Vec<PanelItem>) -> Self {
        Self::with_empty_view(items, EmptyQueryView::All)
    }

    /// 指定空查询首屏视图构造（宿主按设置项调用）。
    pub fn with_empty_view(items: Vec<PanelItem>, empty_view: EmptyQueryView) -> Self {
        let mut s = Self {
            items,
            query: String::new(),
            visible: Vec::new(),
            fallback: Vec::new(),
            selected: Selected::None,
            empty_view,
        };
        s.recompute_visible();
        s.reset_selection();
        s
    }

    /// 切换空查询首屏视图（设置页变更时调用）；空查询下立即重算可见表。
    pub fn set_empty_view(&mut self, view: EmptyQueryView) {
        if self.empty_view == view {
            return;
        }
        self.empty_view = view;
        if self.query.is_empty() {
            self.recompute_visible();
            self.clamp_selection();
        }
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

    /// 设置查询文本；查询变化时重算可见索引表（模糊打分 + 排序），
    /// 选中索引自动夹紧（可能变成 None）。
    ///
    /// 性能（A3）：`draw_panel` **每帧**以当前输入框文本调用本方法，
    /// 查询未变化时直接早退——匹配/排序成本只发生在真正按键的帧。
    pub fn set_query(&mut self, q: impl Into<String>) {
        let q = q.into();
        if q == self.query {
            return;
        }
        self.query = q;
        self.recompute_visible();
        self.clamp_selection();
    }

    /// 当前查询下可见项个数（fallback 模式时 = fallback 长度）。
    pub fn visible_count(&self) -> usize {
        self.filtered().count()
    }

    /// 可见项迭代器（**fallback 模式下不二次过滤**——项已按查询渲染好）：
    /// `(可见下标, 原始项)`，供渲染与选中高亮使用。
    ///
    /// M4 P5（A3/D3-A）：可见序 = nucleo 得分降序（同分保原序，稳定排序），
    /// 由 [`Self::recompute_visible`] 在查询变化时一次性算好。
    pub fn filtered(&self) -> Box<dyn Iterator<Item = (usize, &PanelItem)> + '_> {
        if self.is_fallback_mode() {
            return Box::new(self.fallback.iter().enumerate());
        }
        Box::new(
            self.visible
                .iter()
                .enumerate()
                .map(move |(i, &idx)| (i, &self.items[idx])),
        )
    }

    /// 是否处于 fallback 展示模式：查询非空、常规项全部不匹配、且有兜底项。
    pub fn is_fallback_mode(&self) -> bool {
        !self.query.is_empty() && !self.fallback.is_empty() && self.visible.is_empty()
    }

    /// 常规过滤（不含 fallback）是否有匹配项——宿主据此决定是否触发
    /// `fallback_commands` 拉取（有匹配则不拉，§6.2"搜索无匹配时"）。
    pub fn has_regular_match(&self) -> bool {
        self.query.is_empty() || !self.visible.is_empty()
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
        self.recompute_visible();
        self.reset_selection();
    }

    /// 重算可见索引表（M4 P5/A3-D3-A）：
    /// - 空白查询 → 全部项、原顺序（等价 P5 前的"空查询显示全部"）；
    /// - 非空查询 → nucleo 逐项打分（[`crate::fuzzy`]：多字段取最高），
    ///   未命中剔除，按得分**降序稳定排序**（同分保原序）。
    ///
    /// 性能要点：仅在本方法内跑匹配（查询变化时 / `reset` 时各一次），
    /// `filtered()` 等每帧多次的只读路径只走索引表（A3 埋点见计时日志）。
    fn recompute_visible(&mut self) {
        let start = std::time::Instant::now(); // A3 埋点：一次重算 = 一次按键的过滤成本
        let n = self.items.len();
        if self.query.trim().is_empty() {
            // 空查询：按首屏视图过滤（WithoutApps = 隐藏「应用」类项，
            // 默认功能视图——应用仍可通过输入查询命中）
            self.visible = (0..n)
                .filter(|&i| {
                    self.empty_view != EmptyQueryView::WithoutApps
                        || self.items[i].result_category.as_deref() != Some("应用")
                })
                .collect();
            return;
        }
        let mut fm = crate::fuzzy::FuzzyMatcher::new(&self.query);
        let mut scored: Vec<(usize, u32)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, it)| fm.score(it).map(|s| (i, s)))
            .collect();
        // `sort_by_key` 为稳定排序：同分项保持原始顺序（切片 2 行为测试守卫）
        scored.sort_by_key(|&(_, s)| std::cmp::Reverse(s));
        self.visible = scored.into_iter().map(|(i, _)| i).collect();
        eprintln!(
            "[dd-gui] A3 过滤：{n} 项 → {} 命中，query {} 字符，耗时 {} µs",
            self.visible.len(),
            self.query.chars().count(),
            start.elapsed().as_micros()
        );
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
                icon: None,
                tags: vec!["config".into()],
                result_category: None,
                pinyin: String::new(),
                command: CommandRef::Invoke,
            },
            PanelItem {
                id: "file".into(),
                ext_id: "ext.a".into(),
                title: "Open File".into(),
                subtitle: "Browse files".into(),
                section: "Files".into(),
                icon: None,
                tags: vec!["browse".into()],
                result_category: None,
                pinyin: String::new(),
                command: CommandRef::Invoke,
            },
            PanelItem {
                id: "copy".into(),
                ext_id: "ext.a".into(),
                title: "Copy Path".into(),
                subtitle: "Copy current path".into(),
                section: "Files".into(),
                icon: None,
                tags: vec!["clipboard".into()],
                result_category: None,
                pinyin: String::new(),
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

    /// 真机反馈（2026-09-04）：首屏默认不铺全部应用（WithoutApps 视图），
    /// 「应用」类项在空查询下隐藏、查询时仍参与匹配。
    #[test]
    fn empty_view_without_apps_hides_apps_on_empty_query_only() {
        let mut app = PanelItem::new("7-Zip File Manager");
        app.result_category = Some("应用".into());
        let mut calc = PanelItem::new("= 表达式");
        calc.result_category = Some("命令".into());
        let items = vec![app, calc];

        // WithoutApps：空查询只见默认功能
        let mut s = PanelState::with_empty_view(items.clone(), EmptyQueryView::WithoutApps);
        assert_eq!(s.visible_count(), 1, "空查询应隐藏「应用」项");
        assert_eq!(
            s.selected_item().map(|i| i.title.as_str()),
            Some("= 表达式")
        );

        // 非空查询：应用照常参与模糊匹配
        s.set_query("7-zip");
        assert_eq!(s.visible_count(), 1);
        assert_eq!(
            s.selected_item().map(|i| i.title.as_str()),
            Some("7-Zip File Manager")
        );

        // All（旧行为）：空查询显示全部
        let s = PanelState::with_empty_view(items.clone(), EmptyQueryView::All);
        assert_eq!(s.visible_count(), 2);

        // 运行中切换视图：立即生效并夹紧选中
        let mut s = PanelState::with_empty_view(items, EmptyQueryView::All);
        assert_eq!(s.visible_count(), 2);
        s.set_empty_view(EmptyQueryView::WithoutApps);
        assert_eq!(s.visible_count(), 1, "切换后空查询立即重算");
    }

    #[test]
    fn query_filters_case_insensitively() {
        let mut s = PanelState::new(sample_items());
        // P5 模糊语义："open" 对 "Copy Path" 的 subtitle「Copy current path」
        // 构成子序列（o…p…e…n）→ 3 项命中（旧 contains 为 2 项）。
        s.set_query("open");
        assert_eq!(s.visible_count(), 3);
        s.set_query("OPEN");
        assert_eq!(s.visible_count(), 3);
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

    /// M4 P5（切片 2）：按分数重排——弱匹配（中间命中）在前、强匹配（前缀
    /// 命中）在后时，排序后强匹配应排到前面。
    #[test]
    fn query_orders_matches_by_score() {
        let items = vec![
            PanelItem::new("Reopen File"),   // 原序 0：中间命中（弱）
            PanelItem::new("Open Settings"), // 原序 1：前缀命中（强）
        ];
        let mut s = PanelState::new(items);
        s.set_query("open");
        let titles: Vec<&str> = s.filtered().map(|(_, it)| it.title.as_str()).collect();
        assert_eq!(
            titles,
            vec!["Open Settings", "Reopen File"],
            "前缀命中应排在中间命中之前"
        );
        assert_eq!(
            s.selected_item().map(|i| i.title.as_str()),
            Some("Open Settings"),
            "选中应跟随重排落在最强匹配上"
        );
    }

    /// M4 P5（切片 2）：同分保持原始顺序（稳定排序）。
    #[test]
    fn query_keeps_original_order_on_tie() {
        let items = vec![PanelItem::new("Open A"), PanelItem::new("Open B")];
        let mut s = PanelState::new(items);
        s.set_query("open"); // 两项同为前缀命中 → 同分
        let titles: Vec<&str> = s.filtered().map(|(_, it)| it.title.as_str()).collect();
        assert_eq!(titles, vec!["Open A", "Open B"], "同分应保持原始顺序");
    }

    /// M4 P5（切片 2）：重排后 ↑↓ 导航仍按**可见列表位置**工作（环绕/confirm）。
    #[test]
    fn navigation_follows_score_order() {
        let items = vec![
            PanelItem::new("Reopen File"),   // 排序后可见位置 1
            PanelItem::new("Open Settings"), // 排序后可见位置 0
        ];
        let mut s = PanelState::new(items);
        s.set_query("open");
        assert_eq!(s.selected_index(), Some(0), "重排后选中第 0 项 = 最强匹配");
        s.move_down();
        assert_eq!(
            s.confirm().map(|i| i.title.as_str()),
            Some("Reopen File"),
            "可见位置 1 = Reopen File"
        );
    }

    /// M4 P5（A3 实测，**不调目标**）：大列表一次重算（打分+排序）的耗时记录。
    /// 规模取真实上限的裕量（MAX_APPS=400 → 2000 项）；断言为宽松病态上限
    /// （debug 构建），实测值打印供 m4-record.md 记录；未达标则记录实测与瓶颈。
    #[test]
    fn filter_latency_on_large_list_is_recorded() {
        let items: Vec<PanelItem> = (0..2_000)
            .map(|i| PanelItem::new(format!("Command {i:04} settings page browse")))
            .collect();
        let mut s = PanelState::new(items);
        let start = std::time::Instant::now();
        s.set_query("sett");
        let elapsed = start.elapsed();
        println!(
            "[A3] 2000 项模糊过滤一次重算实测：{} µs（{} ms）",
            elapsed.as_micros(),
            elapsed.as_millis()
        );
        assert!(
            elapsed.as_millis() < 100,
            "病态回归：一次重算 {} ms（应远低于 16ms/帧 目标量级）",
            elapsed.as_millis()
        );
        assert_eq!(
            s.visible_count(),
            2_000,
            "构造的项全部应命中（含 sett 子序列）"
        );
    }

    /// M4 P5（A3/D3-A）：模糊子序列匹配——query 字符按顺序命中即可，
    /// 无需连续子串（`"opst"` ⊂ "Open **S**ettings" 的 o…p…s…t）。
    /// 两个用例都刻意选「子序列但**非**子串」，旧 contains 实现下必然失败。
    #[test]
    fn query_matches_fuzzy_subsequence() {
        let mut s = PanelState::new(sample_items());
        s.set_query("opst"); // o-p-s…t ∈ "Open Settings"
        assert_eq!(s.visible_count(), 1, "仅 Open Settings 子序列命中");
        assert_eq!(
            s.selected_item().map(|i| i.title.as_str()),
            Some("Open Settings")
        );
        s.set_query("cpy"); // c-p-y ∈ "Copy Path"（非子串）
        assert_eq!(s.visible_count(), 1);
        assert_eq!(
            s.selected_item().map(|i| i.title.as_str()),
            Some("Copy Path")
        );
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
            icon: None,
            tags: Vec::new(),
            result_category: Some("命令".to_string()),
            pinyin: String::new(),
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
        // P5 模糊语义："open" 命中 3 项（含 Copy Path 的 subtitle 子序列）
        s.set_query("open");
        s.set_fallback(vec![fallback_item("calc.eval.query", "= open")]);
        assert!(!s.is_fallback_mode(), "常规有匹配时不进入 fallback");
        assert_eq!(s.visible_count(), 3);

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
