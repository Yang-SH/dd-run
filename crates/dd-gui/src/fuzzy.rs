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
///
/// `pub`（§4.5）：纯逻辑零副作用，与 [`crate::aggregator::flatten`] 同理
/// 对外可构造——criterion bench（`benches/hot_paths.rs`）与单测直达。
pub struct FuzzyMatcher {
    pattern: Pattern,
    matcher: Matcher,
    /// [`Utf32Str::new`] 的复用缓冲（避免每字段每次分配）。
    buf: Vec<char>,
    /// 空白查询（trim 后为空）→ 全部可见。
    blank: bool,
}

impl FuzzyMatcher {
    pub fn new(query: &str) -> Self {
        Self {
            pattern: Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart),
            matcher: Matcher::new(Config::DEFAULT),
            buf: Vec::new(),
            blank: query.trim().is_empty(),
        }
    }

    /// 该项得分（**字段分层** × 层内 nucleo 最高分；空白查询 → `Some(0)`；
    /// 无一命中 → `None`）。匹配判定 = `score(...).is_some()`。
    ///
    /// 返回值 = `(层 << 32) | nucleo 分` —— **层不同绝不互比**：
    /// - **层 1（标题层）**：`title` 及其派生拼音索引 `pinyin`；
    /// - **层 0（附属层）**：`subtitle` / `section` / `tags`。
    ///
    /// M6 批次 6.1（L4）：`item.pinyin`（全拼 + 首字母混合串）作为独立字段
    /// 参与打分——输入 `jsq` / `jisuanqi` 均可命中「计算器」；它属**标题层**
    /// （是 title 的派生表示，命中即等价于命中标题）。
    ///
    /// 分层由来（2026-09-19，真机反馈「搜 `steam` 时 Steam 不在第一个」）：
    /// 此前所有字段混在同一个池里取最高分，而 nucleo 对**前缀连续匹配不区分
    /// haystack 长度** —— 实测 `Steam`（标题精确，140 分）与
    /// `steam://rungameid/588650`（Steam 游戏副标题，140 分）**完全同分**，
    /// `recompute_visible` 的稳定排序遂回落到扩展侧字母序，把标题精确匹配的
    /// Steam 挤到第 5。分层后标题命中恒先于副标题命中，同层内仍按匹配质量排。
    pub fn score(&mut self, item: &PanelItem) -> Option<u64> {
        if self.blank {
            return Some(0);
        }
        let mut primary = self.field_score(&item.title);
        primary = primary.max(self.field_score(&item.pinyin));
        let mut secondary = self.field_score(&item.subtitle);
        secondary = secondary.max(self.field_score(&item.section));
        for tag in &item.tags {
            secondary = secondary.max(self.field_score(tag));
        }
        let (tier, score) = match (primary, secondary) {
            (Some(s), _) => (1u64, s),
            (None, Some(s)) => (0u64, s),
            (None, None) => return None,
        };
        Some((tier << 32) | u64::from(score))
    }

    /// 单字段打分（空字段不参与——空串对非空 query 恒不命中）。
    fn field_score(&mut self, hay: &str) -> Option<u32> {
        if hay.is_empty() {
            return None;
        }
        let h = Utf32Str::new(hay, &mut self.buf);
        self.pattern.score(h, &mut self.matcher)
    }

    /// N6：标题高亮**字符下标**（升序去重；空 = 无高亮）——二遍计算，评分
    /// 热路径（[`Self::score`]）不经过本函数。
    ///
    /// 取序：① title 直接命中（nucleo `Pattern::indices`——多 atom 索引逐
    /// atom 追加不排序，`sort_unstable + dedup` 收口）；② title 未中而拼音
    /// 索引命中 → 命中下标经 [`crate::state::pinyin_spans`] 映射回源汉字；
    /// ③ 附属层命中（title 与拼音均未中，如 `brws` 命中 subtitle）→ 空
    /// （不高亮标题）。空白查询 → 空。
    pub fn title_highlight(&mut self, item: &PanelItem) -> Vec<usize> {
        if self.blank {
            return Vec::new();
        }
        if let Some(mut idx) = self.field_indices(&item.title) {
            idx.sort_unstable();
            idx.dedup();
            return idx.into_iter().map(|i| i as usize).collect();
        }
        if !item.pinyin.is_empty() {
            if let Some(mut idx) = self.field_indices(&item.pinyin) {
                idx.sort_unstable();
                idx.dedup();
                let spans = crate::state::pinyin_spans(&item.title);
                let mut out: Vec<usize> = idx
                    .into_iter()
                    .filter_map(|i| spans.get(i as usize).copied())
                    .filter(|&t| t != crate::state::PINYIN_SPAN_GAP)
                    .collect();
                out.sort_unstable();
                out.dedup();
                return out;
            }
        }
        Vec::new()
    }

    /// 单字段命中下标（空字段不参与；无命中 → `None`，局部追加的索引随
    /// 每次调用的独立缓冲一起丢弃）。
    fn field_indices(&mut self, hay: &str) -> Option<Vec<u32>> {
        if hay.is_empty() {
            return None;
        }
        let h = Utf32Str::new(hay, &mut self.buf);
        let mut idx = Vec::new();
        self.pattern.indices(h, &mut self.matcher, &mut idx)?;
        Some(idx)
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
            pinyin: crate::state::pinyin_haystack(title),
            command: CommandRef::Invoke,
            more_commands: Vec::new(),
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

    #[test]
    fn pinyin_full_and_initials_match() {
        // M6 批次 6.1（L4）：拼音索引参与匹配——全拼与首字母缩写均可命中
        let calc = item("计算器", "", "计算", &[]);
        assert_eq!(calc.pinyin, "jisuanqi jsq", "拼音索引 = 全拼 + 首字母");
        let mut fm = FuzzyMatcher::new("jsq");
        assert!(fm.score(&calc).is_some(), "首字母缩写 jsq 命中");
        let mut fm2 = FuzzyMatcher::new("jisuanqi");
        assert!(fm2.score(&calc).is_some(), "全拼 jisuanqi 命中");
        // 非拼音输入仍走原字段；纯英文标题拼音索引为空串
        let en = item("Open Settings", "", "", &[]);
        assert_eq!(en.pinyin, "", "纯英文标题无拼音索引");
        let mut fm3 = FuzzyMatcher::new("jsq");
        assert!(fm3.score(&en).is_none(), "jsq 不命中英文项");
    }

    /// 2026-09-19（真机反馈「搜 `steam` 时 Steam 不在第一个」）：字段分层 ——
    /// 标题命中（含拼音索引）**恒优先**于副标题/标签命中，即便二者 nucleo 分相同。
    ///
    /// 实测复现：`Pattern::parse("steam", Ignore, Smart)` 下
    /// `Steam`（标题）与 `steam://rungameid/588650`（Steam 游戏副标题）**同为 140**。
    #[test]
    fn title_hit_outranks_subtitle_hit() {
        let mut fm = FuzzyMatcher::new("steam");
        let steam = item(
            "Steam",
            "G:\\Program Files (x86)\\Steam\\Steam.exe",
            "应用",
            &[],
        );
        let game = item("Dead Cells", "steam://rungameid/588650", "应用", &[]);
        let a = fm.score(&steam).expect("Steam 应命中标题");
        let b = fm.score(&game).expect("Dead Cells 应经副标题命中");
        assert!(a > b, "标题命中 {a} 应 > 副标题命中 {b}");
        // 同层（标题层）内仍可比：Steam Support Center 亦为标题命中，二者高位一致
        let support = item(
            "Steam Support Center",
            "http://support.steampowered.com/",
            "应用",
            &[],
        );
        assert_eq!(
            fm.score(&support).expect("应命中") >> 32,
            a >> 32,
            "两条标题命中应同属标题层"
        );
    }

    // ── N6：标题高亮 ────────────────────────────────────────────────

    /// N6：title 直接命中 → 命中字符下标（升序去重、界内）。
    #[test]
    fn highlight_title_direct_match() {
        let mut fm = FuzzyMatcher::new("opst");
        let idx = fm.title_highlight(&item("Open Settings", "", "", &[]));
        assert!(!idx.is_empty());
        assert_eq!(idx.first(), Some(&0), "o 首字符命中");
        assert!(idx.windows(2).all(|w| w[0] < w[1]), "升序去重");
        let n = "Open Settings".chars().count();
        assert!(idx.iter().all(|&i| i < n), "界内");
    }

    /// N6：空白查询恒空；附属层命中（title 与拼音均未中）→ 标题不高亮。
    #[test]
    fn highlight_blank_and_secondary_miss_are_empty() {
        let mut fm = FuzzyMatcher::new("   ");
        assert!(
            fm.title_highlight(&item("Open Settings", "", "", &[]))
                .is_empty(),
            "空白查询无高亮"
        );
        let mut fm2 = FuzzyMatcher::new("brws");
        assert!(
            fm2.title_highlight(&item("X", "Browse files", "", &[]))
                .is_empty(),
            "副标题命中不高亮标题"
        );
    }

    /// N6：拼音命中 → 经 spans 映射回源汉字（全拼连续命中 = 全部三汉字；
    /// 首字母命中非空且界内——nucleo 可能择全拼散点或首字母连续段，两者
    /// 映射后均落源汉字界内）。
    #[test]
    fn highlight_pinyin_maps_back_to_hanzi() {
        let mut fm = FuzzyMatcher::new("jisuanqi");
        let idx = fm.title_highlight(&item("计算器", "", "", &[]));
        assert_eq!(idx, vec![0, 1, 2], "全拼连续命中 → 三汉字全高亮");
        let mut fm2 = FuzzyMatcher::new("jsq");
        let idx2 = fm2.title_highlight(&item("计算器", "", "", &[]));
        assert!(!idx2.is_empty(), "首字母命中非空");
        assert!(idx2.iter().all(|&i| i < 3), "映射回源汉字界内");
        assert!(idx2.windows(2).all(|w| w[0] < w[1]), "升序去重");
    }
}
