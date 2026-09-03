//! 兜底命令模板的纯逻辑层（M4 宿主 fallback 轮，不依赖 egui，可单测）。
//!
//! 职责（对齐 [`docs/protocol.md`](../../docs/protocol.md) §6.2 与
//! [`docs/implementation.md`](../../docs/implementation.md) §M4 A10 批次 2）：
//! - **模板缓存**：`fallback_commands` 返回的是含 `{query}` 占位符的**静态模板**
//!   （与具体输入无关），每扩展**只拉一次**并缓存；后续输入只做本地渲染替换，
//!   不重复 RPC（宿主 fallback 轮决策 3）。
//! - **状态机**：`Unknown → Fetching → Ready | Exhausted`，防并发重拉、防对
//!   "已确认无兜底能力"（空结果/失败）的扩展反复拉取。
//! - **渲染**：按当前查询把模板 `title` 中的 `{query}` 替换为真实搜索词，
//!   产出可渲染的 [`PanelItem`]（复用 [`aggregator::to_panel_item`] 的字段映射）。
//!
//! 使用方（main.rs）流程：查询变化且全局过滤为空 → 对 warm 且 [`Self::wants`]
//! 的扩展后台拉模板 → [`Self::store`] 回填 → [`Self::render`] 出列表展示；
//! Enter 走既有 `confirm_selected`（`invoke_params` 已带 `context.query`）。

use dd_host::process::ExtensionProcess;
use dd_protocol::messages::error_codes;
use dd_protocol::model::CommandItem;
use std::time::Duration;

use crate::aggregator;
use crate::state::PanelItem;

/// `fallback_commands` 的协议超时（协议 §10：2000 ms）。
pub const FALLBACK_TIMEOUT: Duration = Duration::from_millis(2_000);

/// 单个扩展的兜底状态。
#[derive(Debug, Clone, PartialEq, Eq)]
enum ExtState {
    /// 模板已取回且**非空**（具备兜底能力），含扩展显示名与模板列表。
    Ready {
        name: String,
        templates: Vec<CommandItem>,
    },
    /// 已确认**无兜底能力**（空结果或拉取失败）——本面板会话内不再拉取。
    Exhausted,
}

/// 兜底模板存储与渲染（纯逻辑，主线程独占，无锁）。
#[derive(Debug, Clone, Default)]
pub struct FallbackStore {
    /// 扩展清单 id → 状态。**Vec 保序**：渲染顺序 = 登记顺序（稳定、可预测），
    /// 不用 HashMap（迭代序不定会让 fallback 项每帧乱序跳动）。
    exts: Vec<(String, ExtState)>,
    /// 正在拉取中的扩展 id（防并发重复拉取同一扩展）。
    fetching: std::collections::HashSet<String>,
}

impl FallbackStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 该扩展是否**需要**拉取模板：未登记 且 不在拉取中。
    pub fn wants(&self, ext_id: &str) -> bool {
        !self.fetching.contains(ext_id) && self.state(ext_id).is_none()
    }

    /// 标记开始拉取（幂等：已在拉取中则忽略）。
    pub fn begin_fetch(&mut self, ext_id: &str) {
        if self.state(ext_id).is_none() {
            self.fetching.insert(ext_id.to_string());
        }
    }

    /// 回填拉取结果。`templates` 非空 → `Ready`（具备兜底能力）；空 → `Exhausted`
    /// （协议 §6.2：**空数组表示无兜底能力**，宿主以此判定 fresh/不再拉取）。
    pub fn store(&mut self, ext_id: &str, name: &str, templates: Vec<CommandItem>) {
        self.fetching.remove(ext_id);
        // 已在 Exhausted/Ready 的直接覆盖为最新结果（幂等回填）
        let state = if templates.is_empty() {
            ExtState::Exhausted
        } else {
            ExtState::Ready {
                name: name.to_string(),
                templates,
            }
        };
        match self.exts.iter_mut().find(|(id, _)| id == ext_id) {
            Some((_, s)) => *s = state,
            None => self.exts.push((ext_id.to_string(), state)),
        }
    }

    /// 拉取失败（超时/进程退出/协议错误）→ 视为无兜底（本会话不重试），
    /// 避免每次无匹配都触发对坏扩展的 RPC。
    pub fn store_failure(&mut self, ext_id: &str) {
        self.store(ext_id, "", Vec::new());
    }

    /// 该扩展当前状态（None = 未登记，即 Unknown）。
    fn state(&self, ext_id: &str) -> Option<&ExtState> {
        self.exts
            .iter()
            .find(|(id, _)| id == ext_id)
            .map(|(_, s)| s)
    }

    /// 当前处于 Ready 的扩展是否为空（无可渲染的兜底模板）。
    pub fn is_empty(&self) -> bool {
        !self
            .exts
            .iter()
            .any(|(_, s)| matches!(s, ExtState::Ready { .. }))
    }

    /// 某扩展已缓存的模板条数（0 = 未 Ready，诊断日志用）。
    pub fn template_count(&self, ext_id: &str) -> usize {
        match self.state(ext_id) {
            Some(ExtState::Ready { templates, .. }) => templates.len(),
            _ => 0,
        }
    }

    /// 按当前查询渲染全部 Ready 模板为 [`PanelItem`]（`title` 中 `{query}` →
    /// 真实搜索词）。空查询不调用（调用方保证 query 非空时才展示兜底）。
    pub fn render(&self, query: &str) -> Vec<PanelItem> {
        let mut out = Vec::new();
        for (ext_id, state) in &self.exts {
            if let ExtState::Ready { name, templates } = state {
                for tmpl in templates {
                    let mut item = aggregator::to_panel_item(tmpl, ext_id, name);
                    item.title = render_title(&item.title, query);
                    out.push(item);
                }
            }
        }
        out
    }
}

/// 模板 `title` 渲染：把全部 `{query}` 占位符替换为真实搜索词。
/// 协议 §6.2：占位符约定在 `title`；subtitle/其余字段不替换。
pub fn render_title(template: &str, query: &str) -> String {
    template.replace("{query}", query)
}

/// 在给定进程上拉取一次兜底模板（协议 §6.2）。供 main.rs 后台线程调用。
///
/// `Err` = 协议/超时/进程故障；`Ok(空)` = 该扩展无兜底能力（正常结果）。
pub fn fetch_fallback_commands(proc: &mut ExtensionProcess) -> Result<Vec<CommandItem>, String> {
    proc.fallback_commands()
        .map_err(|e| match e.as_rpc_error() {
            Some(rpc) if rpc.code == error_codes::EXTENSION_TIMEOUT => {
                "fallback_commands 超时".to_string()
            }
            Some(rpc) => format!("fallback_commands 错误：{}", rpc.message),
            None => format!("fallback_commands 失败：{e}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_protocol::model::CommandRef;

    fn template(id: &str, title: &str) -> CommandItem {
        CommandItem {
            id: id.to_string(),
            title: title.to_string(),
            subtitle: Some("固定副标题".to_string()),
            icon: None,
            section: Some("兜底".to_string()),
            tags: None,
            details: None,
            text_to_suggest: None,
            more_commands: None,
            command: CommandRef::Invoke,
        }
    }

    #[test]
    fn store_ready_and_render_replaces_placeholder() {
        let mut store = FallbackStore::new();
        assert!(store.is_empty(), "未登记时无兜底项");
        store.store(
            "com.ddrun.calc",
            "Calculator",
            vec![
                template("calc.eval.query", "= {query}"),
                template("calc.fmt.query", "格式化 {query} 再来一次 {query}"),
            ],
        );
        assert!(!store.is_empty());
        let items = store.render("1+1");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "= 1+1", "占位符替换为真实查询");
        assert_eq!(items[0].ext_id, "com.ddrun.calc");
        assert_eq!(items[0].section, "兜底", "模板 section 透传");
        assert_eq!(items[0].subtitle, "固定副标题", "subtitle 不参与替换");
        assert_eq!(
            items[1].title, "格式化 1+1 再来一次 1+1",
            "多处占位符全替换"
        );
    }

    #[test]
    fn empty_result_marks_exhausted_and_no_rerender() {
        let mut store = FallbackStore::new();
        store.store("com.ddrun.system", "System", Vec::new());
        assert!(store.is_empty(), "空结果 → 无兜底项");
        assert!(!store.wants("com.ddrun.system"), "已确认无兜底 → 不再拉取");
        assert!(store.render("x").is_empty());
    }

    #[test]
    fn wants_and_fetching_dedup() {
        let mut store = FallbackStore::new();
        assert!(store.wants("com.ddrun.calc"), "未拉取过 → wants");
        store.begin_fetch("com.ddrun.calc");
        assert!(!store.wants("com.ddrun.calc"), "拉取中 → 不再 wants");
        store.store(
            "com.ddrun.calc",
            "Calculator",
            vec![template("t", "= {query}")],
        );
        assert!(!store.wants("com.ddrun.calc"), "已 Ready → 不再 wants");
        // 再 store 同扩展（幂等覆盖）不产生重复项
        store.store(
            "com.ddrun.calc",
            "Calculator",
            vec![template("t2", "x {query}")],
        );
        assert_eq!(store.render("q").len(), 1, "同扩展覆盖而非追加");
    }

    #[test]
    fn failure_marks_exhausted() {
        let mut store = FallbackStore::new();
        store.begin_fetch("com.ddrun.shell");
        store.store_failure("com.ddrun.shell");
        assert!(store.is_empty());
        assert!(!store.wants("com.ddrun.shell"), "失败后本会话不重试");
    }

    #[test]
    fn render_order_follows_registration() {
        let mut store = FallbackStore::new();
        store.store(
            "com.ddrun.calc",
            "Calculator",
            vec![template("a", "= {query}")],
        );
        store.store(
            "com.ddrun.websearch",
            "Web Search",
            vec![template("b", "搜 {query}")],
        );
        store.store(
            "com.ddrun.shell",
            "Shell",
            vec![template("c", "跑 {query}")],
        );
        let items = store.render("hi");
        let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"], "渲染顺序 = 登记顺序");
    }

    #[test]
    fn render_title_replaces_all_occurrences() {
        assert_eq!(render_title("= {query}", "2+2"), "= 2+2");
        assert_eq!(render_title("a {query} b {query}", "x"), "a x b x");
        assert_eq!(render_title("无占位符", "x"), "无占位符");
        assert_eq!(render_title("", "x"), "");
    }

    #[test]
    fn wants_unknown_is_true_then_false_after_state() {
        let mut store = FallbackStore::new();
        assert!(store.wants("nope"));
        // 未 fetch 直接 store（异常路径也兜住）：空 → Exhausted
        store.store("nope", "X", Vec::new());
        assert!(!store.wants("nope"));
    }
}
