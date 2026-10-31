//! 批次 B（refactor-structure-2026-10-21.md §4.2）：列表合并与条目渲染。
//! 自 `aggregator.rs` 原块（765–897 行）逐字节搬入、零逻辑改动；
//! 外部路径经 `aggregator/mod.rs` 的 `pub use` 桥保持不变。

use crate::settings::{CustomCommand, Lang};
use dd_protocol::model::CommandItem;

use super::model::{ExtItems, SourceStatus, SourceSummary, CUSTOM_ITEM_PREFIX, HOST_CUSTOM_EXT_ID};
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
        .map(|c| PanelItem {
            id: format!("{CUSTOM_ITEM_PREFIX}{}", c.keyword),
            ext_id: HOST_CUSTOM_EXT_ID.to_string(),
            title: c.title.clone(),
            subtitle: c.target.clone(),
            section: section.clone(),
            icon: None,
            tags: vec![c.keyword.clone()],
            result_category: Some(section.clone()),
            pinyin: crate::state::pinyin_haystack(&c.title),
            command: dd_protocol::model::CommandRef::Invoke,
            more_commands: Vec::new(),
        })
        .collect()
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
