//! 批次 B（refactor-structure-2026-10-21.md §4.2）：列表合并与条目渲染。
//! 自 `aggregator.rs` 原块（765–897 行）逐字节搬入、零逻辑改动；
//! 外部路径经 `aggregator/mod.rs` 的 `pub use` 桥保持不变。

use crate::settings::{CustomCommand, CustomCommandKind, Lang};
use dd_protocol::model::CommandItem;

use super::model::{
    ExtItems, SourceStatus, SourceSummary, CUSTOM_ITEM_PREFIX, DIRECT_ITEM_PREFIX,
    HOST_CUSTOM_EXT_ID,
};
use crate::state::PanelItem;
use crate::text;

/// 把各扩展结果合并为可渲染列表 + 源状态汇总（纯函数，可单测）。
/// `lang` 传入为渲染时的生效语言（v4.14 D40：类别徽标随 GUI 语言切换），
/// `lang_dirty` 离开设置页触发的重聚合会随之更新徽标。
pub fn flatten(per_ext: &[ExtItems], lang: Lang) -> (Vec<PanelItem>, Vec<SourceSummary>) {
    let mut items = Vec::new();
    let mut sources = Vec::with_capacity(per_ext.len());
    for ext in per_ext {
        match ext {
            ExtItems::Ready {
                id,
                name,
                items: cmds,
            } => {
                sources.push(SourceSummary {
                    id: id.clone(),
                    name: name.clone(),
                    status: SourceStatus::Warm {
                        commands: cmds.len(),
                    },
                });
                for cmd in cmds {
                    items.push(to_panel_item(cmd, id, name, lang));
                }
            }
            ExtItems::Stub {
                id,
                name,
                items: cmds,
            } => {
                sources.push(SourceSummary {
                    id: id.clone(),
                    name: name.clone(),
                    status: SourceStatus::Stub {
                        commands: cmds.len(),
                    },
                });
                for cmd in cmds {
                    items.push(to_panel_item(cmd, id, name, lang));
                }
            }
            ExtItems::Failed { id, name, error } => {
                sources.push(SourceSummary {
                    id: id.clone(),
                    name: name.clone(),
                    status: SourceStatus::Failed {
                        error: error.clone(),
                    },
                });
            }
        }
    }
    (items, sources)
}

/// [`CommandItem`] → [`PanelItem`]；`section` 缺省时用扩展名兜底分组。
///
/// `ext_id` 记录命令来源扩展（`invoke` / `get_items` 时定位子进程）。
/// `icon` 从 `CommandItem.icon`（§8.6 三态）**透传**——渲染层决定如何显示；
/// 列表/嵌套页/fallback 模板均经此函数，保证全链路图标一致（M5 UI 批次 2）。
///
/// M5 批次 3.9：按 `ext_id` 去 `com.ddrun.` 前缀推导通用类别标签（设计文档 §6.2），
/// 内置扩展映射为「应用/命令/设置/网页」，第三方回退「命令」。
/// 语言按当前生效语言解析（v4.14 D40，与 GUI 其他徽标/页脚口径一致）——
/// `lang_dirty` 离开设置页触发的重聚合会随之更新类别标签。
pub fn to_panel_item(
    cmd: &CommandItem,
    ext_id: &str,
    fallback_section: &str,
    lang: Lang,
) -> PanelItem {
    PanelItem {
        id: cmd.id.clone(),
        ext_id: ext_id.to_string(),
        title: cmd.title.clone(),
        subtitle: cmd.subtitle.clone().unwrap_or_default(),
        section: cmd
            .section
            .clone()
            .unwrap_or_else(|| fallback_section.to_string()),
        icon: cmd.icon.clone(),
        tags: cmd.tags.clone().unwrap_or_default(),
        result_category: Some(category_label_for(ext_id, lang).to_string()),
        // M6 批次 6.1（L4）：预计算拼音匹配索引（全拼+首字母），协议层零改动
        pinyin: crate::state::pinyin_haystack(&cmd.title),
        command: cmd.command.clone(),
        // v3.3 P1：`more_commands` 透传（§8.1 上下文菜单动作，右键菜单渲染 +
        // `sender=context_menu` 回调 invoke）
        more_commands: cmd.more_commands.clone().unwrap_or_default(),
    }
}

/// N1（2026-10-04）：自定义直达命令配置 → 宿主虚拟条目（§3 惯例 ⑤）。
///
/// 每条配置一个首屏条目：`title` 进拼音索引（复用 M6 管线，`pinyin_haystack`）、
/// `keyword` 并入 `tags`（关键词即搜）、`target` 作副标题（与「应用」行显路径
/// 同口径）、分组与类别徽标均为「直达」（`cat.custom`）；`ext_id` = 保留 id
/// [`HOST_CUSTOM_EXT_ID`]，`command = Invoke`（激活时宿主内部分发执行，不走扩展）。
/// 纯函数，可单测。
pub fn custom_command_items(cmds: &[CustomCommand], lang: Lang) -> Vec<PanelItem> {
    let section = text::t(lang, "cat.custom").to_string();
    cmds.iter()
        .map(|c| {
            // N19：多关键词（逗号分隔）——主关键词 = 第一段（id 与右键删除
            // 语义锚定），全部段进 tags（任一段可命中匹配）。
            let segs: Vec<String> = c
                .keyword
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
            let primary = segs.first().cloned().unwrap_or_default();
            // N19：path 型失效检测——target 不存在 → 副标题前缀提示（条目量
            // 小，逐聚合 stat 成本 µs 级；invoke 侧同口径拒绝执行）。
            let missing =
                c.kind == CustomCommandKind::Path && !std::path::Path::new(&c.target).exists();
            let subtitle = if missing {
                format!("{} {}", text::t(lang, "n19.path_missing"), c.target)
            } else {
                c.target.clone()
            };
            PanelItem {
                id: format!("{CUSTOM_ITEM_PREFIX}{primary}"),
                ext_id: HOST_CUSTOM_EXT_ID.to_string(),
                title: c.title.clone(),
                subtitle,
                section: section.clone(),
                icon: None,
                tags: segs,
                result_category: Some(section.clone()),
                pinyin: crate::state::pinyin_haystack(&c.title),
                command: dd_protocol::model::CommandRef::Invoke,
                more_commands: Vec::new(),
            }
        })
        .collect()
}

/// N10（2026-11-21）：URL / 路径形态查询 → 宿主「打开」直达条目（聚合尾部
/// 追加，排在常规结果之后）。
///
/// 判定复用 `text::url_like`（http(s) 前缀）/ `text::path_like`（盘符/UNC
/// 形态）——**裸域名刻意不识别**（`2.5.1`/`file.pdf` 类误报面，altrun 方案
/// §4.5 边界；N1 直达命令与书签覆盖该需求）。执行经
/// `host_actions::run_custom_command` 的 `direct:` 分支：URL 走
/// `open_url_execute`（S-03 白名单天然生效），路径走 `platform::open_path`
/// 「双击等价」。`id = direct:<目标>` 内嵌目标（执行点重分类，无配置竞态）。
/// 纯函数，可单测。
pub fn url_path_direct_item(query: &str, lang: Lang) -> Option<PanelItem> {
    let target = query.trim();
    if target.is_empty() {
        return None;
    }
    let is_url = text::url_like(target).is_some();
    let is_path = text::path_like(target).is_some();
    if !is_url && !is_path {
        return None;
    }
    let section = text::t(lang, "cat.custom").to_string();
    let title = text::t(lang, "direct.open").replace("{}", target);
    Some(PanelItem {
        id: format!("{DIRECT_ITEM_PREFIX}{target}"),
        ext_id: HOST_CUSTOM_EXT_ID.to_string(),
        subtitle: target.to_string(),
        title,
        section: section.clone(),
        icon: None,
        tags: Vec::new(),
        result_category: Some(section),
        pinyin: crate::state::pinyin_haystack(target),
        command: dd_protocol::model::CommandRef::Invoke,
        more_commands: Vec::new(),
    })
}

/// 按扩展清单 id 推导结果类别显示标签（设计文档 §6.2 映射表）。
///
/// 内置扩展使用全限定 id，去 `com.ddrun.` 前缀后匹配；未知第三方统一回退「命令」。
/// 文案经 `text::t` 按生效语言解析（v4.14 D40）。
fn category_label_for(ext_id: &str, lang: Lang) -> &'static str {
    let short = ext_id.strip_prefix("com.ddrun.").unwrap_or(ext_id);
    let key = match short {
        "apps" => "cat.apps",
        "system" => "cat.system",
        "websearch" => "cat.websearch",
        "bookmarks" => "cat.bookmarks",
        // calc / shell / 第三方统一归为「命令」
        _ => "cat.command",
    };
    text::t(lang, key)
}
