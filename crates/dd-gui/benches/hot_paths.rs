//! §4.5（dd-run-改进分析.md）：首屏热路径 bench——模糊匹配 / 查询重算 / flatten 合并。
//!
//! 三个组对应分析文档验收「3 个 bench 有基线数据」：
//! - `fuzzy_score_2000`：[`FuzzyMatcher::score`] 一次击键的纯打分成本（2000 项 × 8 字符查询）；
//! - `set_query_2000`：[`PanelState::set_query`] 完整击键路径（构建 matcher + 逐项打分 +
//!   稳定排序重建可见表）——`recompute_visible` 是私有方法，公共入口多个
//!   （`set_query` / `set_empty_view` / `set_apps_hidden` / `set_steam_hidden` / `reset`），
//!   bench 取用户击键驱动的 `set_query`；
//! - `flatten_5ext`：[`dd_gui::aggregator::flatten`] 合并 5 扩展 × 100 命令（≈ 真实上限量级）。
//!
//! 基线比对（CI bench job / 本机）：`node tools/bench_compare.mjs <criterion目录> base head`。

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use dd_gui::aggregator::{flatten, ExtItems};
use dd_gui::fuzzy::FuzzyMatcher;
use dd_gui::settings::Lang;
use dd_gui::state::{PanelItem, PanelState};
use dd_protocol::model::{CommandItem, CommandRef};

/// 2000 项测试集：混合英文标题 / CJK 标题（走拼音索引路径）/ 副标题 / 标签，
/// 形态贴近真实聚合结果（应用路径副标题、网页 URL 副标题、分组名、tag chip）。
fn bench_items(n: usize) -> Vec<PanelItem> {
    (0..n)
        .map(|i| {
            let cjk = i % 10 == 0;
            let title = if cjk {
                format!("计算器 {i}")
            } else {
                format!("Application {:04}", i)
            };
            let subtitle = if i % 3 == 0 {
                format!("C:\\Program Files\\App{i}\\app{i}.exe")
            } else if i % 3 == 1 {
                format!("https://example.com/app/{i}")
            } else {
                String::new()
            };
            PanelItem {
                id: format!("item-{i}"),
                ext_id: format!("ext.{}", i % 5),
                title,
                subtitle,
                section: format!("Section {}", i % 7),
                icon: None,
                tags: if i % 4 == 0 {
                    vec!["steam".into(), format!("tag{i}")]
                } else {
                    Vec::new()
                },
                result_category: Some("应用".into()),
                pinyin: if cjk {
                    format!("jisuanqi{i} jsq{i}")
                } else {
                    String::new()
                },
                command: CommandRef::Invoke,
                more_commands: Vec::new(),
            }
        })
        .collect()
}

/// 5 扩展 × 100 命令（Root 列表真实上限量级：400 应用 + 30 文件结果）。
fn bench_per_ext() -> Vec<ExtItems> {
    (0..5)
        .map(|e| ExtItems::Ready {
            id: format!("ext.{e}"),
            name: format!("Extension {e}"),
            items: (0..100)
                .map(|i| CommandItem {
                    id: format!("cmd-{e}-{i}"),
                    title: if i % 5 == 0 {
                        format!("设置 {e} {i}")
                    } else {
                        format!("Command {e} {i:03}")
                    },
                    subtitle: Some(format!("subtitle {e}-{i}")),
                    icon: None,
                    section: Some(format!("Section {}", i % 7)),
                    tags: None,
                    details: None,
                    text_to_suggest: None,
                    more_commands: None,
                    command: CommandRef::Invoke,
                })
                .collect(),
        })
        .collect()
}

fn bench_fuzzy_score(c: &mut Criterion) {
    let items = bench_items(2000);
    let query = "settings";
    c.bench_function("fuzzy_score_2000", |b| {
        b.iter(|| {
            let mut fm = FuzzyMatcher::new(query);
            let mut hits = 0usize;
            for item in &items {
                hits += fm.score(item).is_some() as usize;
            }
            black_box(hits)
        })
    });
}

fn bench_set_query(c: &mut Criterion) {
    let items = bench_items(2000);
    let mut state = PanelState::new(items);
    // 两个真实查询交替：`set_query` 对同查询不重算，交替保证每次迭代都走
    // 完整 recompute_visible（matcher 构建 + 打分 + 稳定排序）。
    let queries = ["settings", "settingsx"];
    let mut turn = 0usize;
    c.bench_function("set_query_2000", |b| {
        b.iter(|| {
            let q = queries[turn % 2];
            turn += 1;
            black_box(state.set_query(q))
        })
    });
}

fn bench_flatten(c: &mut Criterion) {
    let per_ext = bench_per_ext();
    c.bench_function("flatten_5ext", |b| {
        b.iter(|| {
            let (items, sources) = flatten(black_box(&per_ext), Lang::EnUs);
            black_box((items.len(), sources.len()))
        })
    });
}

criterion_group!(hot_paths, bench_fuzzy_score, bench_set_query, bench_flatten);
criterion_main!(hot_paths);
