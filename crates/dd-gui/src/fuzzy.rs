//! M4 P5（A3 / 决策 D3-A）：nucleo 模糊匹配封装（纯逻辑，可独立单测）。
//!
//! 匹配语义（P5 用户确认）：
//! - 查询按空白拆分为多个 atom（AND 组合，fzf 风格）；
//! - 大小写**不敏感**（`CaseMatching::Ignore`——保持 P5 前 contains 的语义）；
//! - Unicode 规范化开启（`Normalization::Smart`，重音等价）；
//! - 每个字段（title / subtitle / section / tags）**独立**打分，取最高分为
//!   该项得分——任一字段子序列命中即视为匹配（避免跨字段拼接产生的
//!   「查询横跨 title 尾 + subtitle 头」的伪命中）；
//! - 空白查询视为「全部可见」（与 P5 前 `q.is_empty() → true` 语义一致）。
//!
//! 拼音匹配**不在 P5 范围**（nucleo 原生不支持，需额外转换 crate，
//! 用户决策本轮不做、留作后续独立项）。

use crate::state::PanelItem;
use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::{Config, Matcher, Utf32Str};

/// 一次查询的匹配器（`PanelState::set_query` 时构建，重算可见列表用）。
pub(crate) struct FuzzyMatcher {
    pattern: Pattern,
    matcher: Matcher,
    /// [`Utf32Str::new`] 的复用缓冲（避免每字段每次分配）。
    buf: Vec<char>,
    /// 空白查询（trim 后为空）→ 全部可见。
    blank: bool,
}

impl FuzzyMatcher {
    pub(crate) fn new(query: &str) -> Self {
        Self {
            pattern: Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart),
            matcher: Matcher::new(Config::DEFAULT),
            buf: Vec::new(),
            blank: query.trim().is_empty(),
        }
    }

    /// 该项得分（各字段最高分；空白查询 → `Some(0)`；无一命中 → `None`）。
    /// 匹配判定 = `score(...).is_some()`（任一字段子序列命中）。
    pub(crate) fn score(&mut self, item: &PanelItem) -> Option<u32> {
        if self.blank {
            return Some(0);
        }
        let mut best = self.field_score(&item.title);
        for hay in [&item.subtitle, &item.section] {
            best = best.max(self.field_score(hay));
        }
        for tag in &item.tags {
            best = best.max(self.field_score(tag));
        }
        best
    }

    /// 单字段打分（空字段不参与——空串对非空 query 恒不命中）。
    fn field_score(&mut self, hay: &str) -> Option<u32> {
        if hay.is_empty() {
            return None;
        }
        let h = Utf32Str::new(hay, &mut self.buf);
        self.pattern.score(h, &mut self.matcher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_protocol::model::CommandRef;

    fn item(title: &str, subtitle: &str, section: &str, tags: &[&str]) -> PanelItem {
        PanelItem {
            id: title.to_string(),
            ext_id: "ext.t".to_string(),
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            section: section.to_string(),
            icon: None,
            tags: tags.iter().map(|s| s.to_string()).collect(),
            result_category: None,
            command: CommandRef::Invoke,
        }
    }

    #[test]
    fn subsequence_matches_without_contiguity() {
        let mut fm = FuzzyMatcher::new("opst");
        assert!(fm.score(&item("Open Settings", "", "", &[])).is_some());
        assert!(fm.score(&item("Copy Path", "", "", &[])).is_none());
    }

    #[test]
    fn case_insensitive_and_multi_field() {
        // 大小写不敏感（CaseMatching::Ignore）
        let mut fm = FuzzyMatcher::new("OPEN");
        assert!(fm.score(&item("Open Settings", "", "", &[])).is_some());
        // subtitle / section / tag 任一字段命中
        let mut fm2 = FuzzyMatcher::new("brws");
        assert!(fm2.score(&item("X", "Browse files", "", &[])).is_some());
        let mut fm3 = FuzzyMatcher::new("cfg");
        assert!(fm3.score(&item("X", "", "", &["config"])).is_some());
    }

    #[test]
    fn blank_query_matches_everything() {
        let mut fm = FuzzyMatcher::new("   ");
        assert!(fm.score(&item("anything", "", "", &[])).is_some());
        assert_eq!(fm.score(&item("anything", "", "", &[])), Some(0));
    }

    #[test]
    fn score_prefers_tighter_match() {
        let mut fm = FuzzyMatcher::new("open");
        // 前缀命中（o 在首位）应比中间命中得分高
        let prefix = fm.score(&item("Open Settings", "", "", &[])).unwrap();
        let middle = fm.score(&item("Reopen File", "", "", &[])).unwrap();
        assert!(prefix > middle, "前缀 {prefix} 应 > 中间 {middle}");
    }
}
