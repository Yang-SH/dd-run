//! 右键菜单：纯逻辑映射、开合与激活（设计稿 10B，D18/D19）。

use crate::app::PaletteApp;
use crate::platform::reveal_in_folder;
use crate::platform::run_as_admin;
use crate::settings::Lang;
use crate::state::PanelItem;
use crate::text::default_action_glyph;
use crate::text::footer_action_text;
use crate::text::path_like;
use crate::text::url_like;
use crate::text::CTX_GLYPH_ADMIN;
use crate::text::CTX_GLYPH_COPY;
use crate::text::CTX_GLYPH_DELETE;
use crate::text::CTX_GLYPH_FOLDER;
use crate::text::CTX_GLYPH_LINK;
use crate::text::CTX_GLYPH_SAVE;
use crate::theme;
use dd_protocol::model::CommandItem;
use eframe::egui;

// ── 右键菜单（设计稿 10B，v4.4）─────────────────────────────────────────

/// 右键菜单动作（10B.2 映射表；D18：GUI 层静态硬编码，协议零改动）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CtxAction {
    /// 默认动作 = 页脚/Enter 同款：按 `CommandRef` 分派（invoke / 进入页）。
    Default,
    /// 以管理员身份运行（UAC 提权，`ShellExecuteW` verb=runas）。
    RunAsAdmin { path: String },
    /// 在资源管理器中定位宿主目录（`explorer /select,<path>`）。
    RevealInFolder { path: String },
    /// 复制路径 / 链接到剪贴板。
    CopyText { text: String },
    /// 扩展声明的 `more_commands` 项（§8.1；v3.3 P1）：以
    /// `sender=context_menu` 回调该扩展的 `invoke`，副作用与结果由扩展裁决。
    Extension { command_id: String },
    /// N1（2026-10-04）：删除自定义直达命令（宿主侧配置操作：移除 + 落盘 +
    /// 重聚合脏标记；下一次重聚合后条目消失）。
    DeleteCustomCommand { keyword: String },
    /// N13（2026-11-21）：屏蔽此应用（宿主侧配置操作：显示名追加进 apps
    /// 用户屏蔽名单（N2 通道）+ 落盘 + 重聚合脏标记，输出层过滤即时生效）。
    HideApp { title: String },
    /// N19（2026-11-21）：一键转自定义命令（宿主侧预填：设置卡输入缓冲写入
    /// title/target/kind，关键词留空由用户填写；不静默写配置）。
    SaveAsCommand {
        title: String,
        target: String,
        kind: crate::settings::CustomCommandKind,
    },
    /// N11（2026-11-22）：置顶/取消置顶（宿主侧配置操作：pinned_keys 幂等切换
    /// + 落盘 + 重聚合脏标记；键策略两族见 `aggregator::pin_key_for`）。
    TogglePin { key: String },
    /// N18：复制片段（宿主侧：按关键词查片段 → 剪贴板队列）。
    CopySnippet { keyword: String },
    /// N18：删除此片段（宿主侧配置操作：移除 + 落盘 + 重聚合脏标记）。
    DeleteSnippet { keyword: String },
}

/// 右键菜单项（10B.1：图标 glyph 16 + 名称 body1 14 + 快捷键 caption1/fg3）。
#[derive(Clone)]
pub(crate) struct CtxEntry {
    pub(crate) glyph: char,
    pub(crate) label: String,
    pub(crate) shortcut: &'static str,
    pub(crate) action: CtxAction,
}

/// 菜单行：动作项或分组分隔线（10B.1：1px stroke2、垂直 margin 4、内缩 8）。
#[derive(Clone)]
pub(crate) enum CtxRow {
    Entry(CtxEntry),
    Separator,
}

/// 打开中的右键菜单状态。
pub(crate) struct CtxMenuState {
    /// 锚定项在**可见列表**中的索引（默认动作执行前置为选中）。
    pub(crate) visible_idx: usize,
    /// 锚定项 id（激活前校验索引未因列表刷新漂移——防陈旧）。
    pub(crate) item_id: String,
    /// 菜单内容（打开时按 D18 映射固化，列表刷新不重算）。
    pub(crate) rows: Vec<CtxRow>,
    /// 锚点（窗口坐标）：指针 = 右键点 + (2,2)；键盘 = 选中行底边左缘。
    pub(crate) anchor: egui::Pos2,
    /// 键盘/悬停焦点（Entry 行下标，不含 Separator）。
    pub(crate) focus: usize,
}

impl PaletteApp {
    // ── 右键菜单（设计稿 10B，v4.4）─────────────────────────────────

    /// 打开右键菜单：按 D18 映射固化菜单内容，焦点落在第一项（默认动作）。
    pub(crate) fn open_ctx_menu(
        &mut self,
        visible_idx: usize,
        item: &PanelItem,
        anchor: egui::Pos2,
    ) {
        // N11：置顶态与可钉判定——仅根页条目（嵌套页/设置页不可钉）；direct:
        // 查询态条目无持久语义，不钉。键族见 `aggregator::pin_key_for`。
        let current = self.view.stack.current();
        let pin_state = if current.page_id.is_none()
            && !current.is_settings
            && !item.id.starts_with(crate::aggregator::DIRECT_ITEM_PREFIX)
        {
            let key = crate::aggregator::pin_key_for(item);
            Some(self.draft.settings.pinned_keys.contains(&key))
        } else {
            None
        };
        let rows = context_menu_rows_with_pin(self.view.lang_effective, item, pin_state);
        log::debug!(
            "[dd-gui] 右键菜单：item={} category={:?}（{} 项）",
            item.id,
            item.result_category,
            rows.iter()
                .filter(|r| matches!(r, CtxRow::Entry(_)))
                .count()
        );
        self.overlay.ctx_menu = Some(CtxMenuState {
            visible_idx,
            item_id: item.id.clone(),
            rows,
            anchor,
            focus: 0,
        });
    }

    /// 菜单开着时右键另一行（D19 修正）：捕获层吞掉了该行的
    /// `secondary_clicked`，这里按本帧行矩形（[`Self::ctx_row_rects`]）命中，
    /// 置选中并就地重开菜单；未命中任何行（列表空白处）则仅关闭。
    pub(crate) fn reopen_ctx_menu_at(&mut self, ctx: &egui::Context, pos: egui::Pos2) {
        let hit = self
            .overlay
            .ctx_row_rects
            .iter()
            .find(|(_, r)| r.contains(pos))
            .map(|(i, _)| *i);
        let found = hit.and_then(|idx| {
            self.view
                .stack
                .current()
                .list
                .filtered()
                .find(|(j, _)| *j == idx)
                .map(|(_, it)| (idx, it.clone()))
        });
        let Some((idx, item)) = found else {
            return;
        };
        self.view.stack.current_mut().list.set_selected(idx);
        self.window.last_hovered_index = Some(idx);
        self.window.scroll_follow = false;
        self.open_ctx_menu(
            idx,
            &item,
            pos + egui::vec2(theme::CTX_ANCHOR_OFFSET, theme::CTX_ANCHOR_OFFSET),
        );
        ctx.request_repaint();
    }

    /// 激活焦点项（Enter / 单击）：默认动作 = Enter 同款（先置选中再按
    /// `CommandRef` 分派）；其余动作宿主侧直接执行，结果经 Toast/eprintln 反馈。
    /// 激活同时关闭菜单（单例浮层语义）。
    pub(crate) fn activate_ctx_menu(&mut self, ctx: &egui::Context) {
        let Some(state) = self.overlay.ctx_menu.take() else {
            return;
        };
        let Some(entry) = state
            .rows
            .iter()
            .filter_map(|r| match r {
                CtxRow::Entry(e) => Some(e),
                CtxRow::Separator => None,
            })
            .nth(state.focus)
            .cloned()
        else {
            return;
        };
        match &entry.action {
            CtxAction::Default => {
                // 防陈旧：列表可能已刷新（items_changed / fallback 重算）导致
                // 索引漂移——激活前校验，失配则丢弃（菜单已随 take 关闭）。
                let current_matches = self
                    .view
                    .stack
                    .current()
                    .list
                    .filtered()
                    .any(|(i, it)| i == state.visible_idx && it.id == state.item_id);
                if !current_matches {
                    log::debug!(
                        "[dd-gui] 右键菜单：列表已刷新，丢弃陈旧激活（item={}）",
                        state.item_id
                    );
                    return;
                }
                self.view
                    .stack
                    .current_mut()
                    .list
                    .set_selected(state.visible_idx);
                self.window.scroll_follow = false;
                self.confirm_selected();
            }
            CtxAction::RunAsAdmin { path } => match run_as_admin(path) {
                Ok(()) => log::debug!("[dd-gui] 以管理员身份运行请求已发起：{path}"),
                Err(e) => self.show_error_toast(self.tr("ctx.admin_fail").replace("{e}", &e)),
            },
            CtxAction::RevealInFolder { path } => match reveal_in_folder(path) {
                Ok(()) => log::debug!("[dd-gui] 已在资源管理器中定位：{path}"),
                Err(e) => self.show_error_toast(self.tr("ctx.locate_fail").replace("{e}", &e)),
            },
            CtxAction::CopyText { text } => {
                ctx.copy_text(text.clone());
                self.show_toast(self.tr("ctx.copied"), None);
            }
            CtxAction::DeleteCustomCommand { keyword } => {
                // N1：宿主侧配置操作——移除 + 落盘 + 重聚合脏标记（条目在
                // 下一次重聚合后从首屏消失；菜单已随 take 关闭）。
                self.delete_custom_command(keyword);
                ctx.request_repaint();
            }
            CtxAction::HideApp { title } => {
                // N13：宿主侧配置操作（N2 通道）——追加 + 落盘 + 重聚合脏标记
                // （下一次重聚合后该应用从首屏消失；菜单已随 take 关闭）。
                self.hide_app_in_blocklist(title);
                ctx.request_repaint();
            }
            CtxAction::SaveAsCommand {
                title,
                target,
                kind,
            } => {
                // N19：宿主侧预填（设置卡输入缓冲）+ 打开设置页——用户补关键词
                // 后手动「添加」，不静默写配置。
                self.prefill_custom_command(title, target, *kind);
                self.open_settings();
                ctx.request_repaint();
            }
            CtxAction::TogglePin { key } => {
                // N11：宿主侧配置操作——pinned_keys 幂等切换 + 落盘 + 重聚合
                // 脏标记（下一次重聚合后置顶分组即时生效；菜单已随 take 关闭）。
                self.toggle_pinned(key.clone());
                ctx.request_repaint();
            }
            CtxAction::CopySnippet { keyword } => {
                // N18：复制片段（R-07 队列同径；toast 由 poll 统一反馈）。
                self.copy_snippet(keyword);
            }
            CtxAction::DeleteSnippet { keyword } => {
                // N18：宿主侧配置操作——移除 + 落盘 + 重聚合脏标记。
                if self.draft.settings.remove_snippet(keyword) {
                    self.save_settings_with_feedback();
                    self.draft.engines_dirty = true;
                    self.show_toast(
                        self.tr("toast.snippet_deleted").replace("{kw}", keyword),
                        None,
                    );
                }
                ctx.request_repaint();
            }
            CtxAction::Extension { command_id } => {
                // 与 Default 相同的防陈旧校验（ext_id 取自当前列表项），
                // 以 `sender=context_menu` + `selected_item_id` 回调扩展 invoke。
                let Some(item) = self
                    .view
                    .stack
                    .current()
                    .list
                    .filtered()
                    .find(|(i, it)| *i == state.visible_idx && it.id == state.item_id)
                    .map(|(_, it)| it.clone())
                else {
                    log::debug!(
                        "[dd-gui] 右键菜单：列表已刷新，丢弃陈旧激活（item={}）",
                        state.item_id
                    );
                    return;
                };
                let query = self.view.stack.current().list.query().to_owned();
                let params =
                    crate::result::context_menu_invoke_params(command_id, &state.item_id, &query);
                log::debug!(
                    "[dd-gui] 右键菜单：扩展动作 {}（ext={}）",
                    command_id,
                    item.ext_id
                );
                self.dispatch_invoke(&item.ext_id, params);
            }
        }
    }
}

// ── 右键菜单：纯逻辑映射与平台动作（可单测，设计稿 10B / v4.4）─────────

/// 菜单中动作项计数（键盘焦点循环用，Separator 不计）。
pub(crate) fn ctx_entry_count(state: Option<&CtxMenuState>) -> usize {
    state
        .map(|s| {
            s.rows
                .iter()
                .filter(|r| matches!(r, CtxRow::Entry(_)))
                .count()
        })
        .unwrap_or(0)
}

/// 从 `CommandItem.icon` 解析菜单字形（仅认 Glyph 首字符，其余回退通用动作字形）。
fn glyph_from_command(cmd: &CommandItem) -> char {
    match &cmd.icon {
        Some(dd_protocol::model::Icon {
            kind: dd_protocol::model::IconKind::Glyph,
            value,
        }) => value.chars().next().unwrap_or(crate::text::CTX_GLYPH_PLAY),
        _ => crate::text::CTX_GLYPH_PLAY,
    }
}

/// 右键菜单内容映射（10B.2，D18：GUI 层静态硬编码、协议零改动）。
///
/// 第一项恒为默认动作（= 页脚 / Enter 同款，[`footer_action_text`]），
/// 其后按 `result_category` 追加二级动作，分组间用分隔线（10B.1）。
/// 二级动作按**数据可用性**门控（偏离记档：设计稿映射为理想形态，实现以
/// 项内实际可得的路径/URL 数据为准）：
/// - 路径型动作（提权 / 定位 / 复制路径）要求 subtitle 为绝对路径（盘符/UNC）；
/// - 「复制链接」要求 subtitle 为 http(s) URL；
///
/// 数据不可得时该项**不出现**（而非禁用）——不渲染无法兑现的动作。
/// 菜单标签按 `lang` 走 i18n 表（v4.13 D38）；`result_category` 的中文值是
/// 协议侧数据（比对用），不翻译。
/// 不可钉）时尾部追加分隔线 + 置顶/取消置顶项；本包装供单测使用。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn context_menu_rows(lang: Lang, item: &PanelItem) -> Vec<CtxRow> {
    context_menu_rows_with_pin(lang, item, None)
}

/// N11：带置顶态的菜单映射——`pin` = `Some(已置顶?)` 时追加「置顶/取消置顶」
/// 项（宿主侧配置动作；根页条目通用尾项）。`None` = 不可钉（嵌套页 / 设置页 /
/// direct: 查询态条目），不出现该行。
pub(crate) fn context_menu_rows_with_pin(
    lang: Lang,
    item: &PanelItem,
    pin: Option<bool>,
) -> Vec<CtxRow> {
    let pin_label = |pinned: bool| -> String {
        crate::text::t(lang, if pinned { "ctx.unpin" } else { "ctx.pin" }).to_string()
    };
    let pin_row = |pinned: bool| -> CtxRow {
        CtxRow::Entry(CtxEntry {
            glyph: crate::text::CTX_GLYPH_PIN,
            label: pin_label(pinned),
            shortcut: "",
            action: CtxAction::TogglePin {
                key: crate::aggregator::pin_key_for(item),
            },
        })
    };
    // N18（2026-11-22）：文本片段专属菜单——默认（= 复制，Enter 同径）+
    // 分隔线 + 复制 + 分隔线 + 删除此片段（宿主侧配置动作，
    // DeleteCustomCommand 同型）。
    if let Some(kw) = item.id.strip_prefix("snippet:") {
        return vec![
            CtxRow::Entry(CtxEntry {
                glyph: default_action_glyph(item),
                label: footer_action_text(lang, item),
                shortcut: "↵ Enter",
                action: CtxAction::Default,
            }),
            CtxRow::Separator,
            CtxRow::Entry(CtxEntry {
                glyph: CTX_GLYPH_COPY,
                label: crate::text::t(lang, "ctx.copy_text").to_string(),
                shortcut: "",
                action: CtxAction::CopySnippet {
                    keyword: kw.to_string(),
                },
            }),
            CtxRow::Separator,
            CtxRow::Entry(CtxEntry {
                glyph: CTX_GLYPH_DELETE,
                label: crate::text::t(lang, "set.snippet.delete").to_string(),
                shortcut: "",
                action: CtxAction::DeleteSnippet {
                    keyword: kw.to_string(),
                },
            }),
        ];
    }
    // N1（2026-10-04）：自定义直达命令专属菜单——默认动作 + 分隔线 + 删除
    // （spec「上下文菜单提供删除」）；不走静态类别映射（其 subtitle 是用户
    // 配置目标，不适用 管理员/定位 腿）。
    if item.ext_id == crate::aggregator::HOST_CUSTOM_EXT_ID {
        if let Some(keyword) = item.id.strip_prefix(crate::aggregator::CUSTOM_ITEM_PREFIX) {
            let mut rows = vec![
                CtxRow::Entry(CtxEntry {
                    glyph: default_action_glyph(item),
                    label: footer_action_text(lang, item),
                    shortcut: "↵ Enter",
                    action: CtxAction::Default,
                }),
                CtxRow::Separator,
                CtxRow::Entry(CtxEntry {
                    glyph: CTX_GLYPH_DELETE,
                    label: crate::text::t(lang, "ctx.delete_custom").to_string(),
                    shortcut: "",
                    action: CtxAction::DeleteCustomCommand {
                        keyword: keyword.to_string(),
                    },
                }),
            ];
            // N11：自定义命令行同享置顶（键 = 宿主稳定 id 族，usage_key）。
            if let Some(pinned) = pin {
                rows.push(CtxRow::Separator);
                rows.push(pin_row(pinned));
            }
            return rows;
        }
        // N10：URL/路径直达条目专属菜单——默认动作 + 分隔线 + 复制（链接/路径
        // 按目标形态重分类）+ N19 转存（真机核对轮 2026-11-22 补：直达行是
        // URL/路径查询的**实际承载行**，N19 的「URL 行转存」若只挂网页类别则
        // 实践中不可达）。`direct:<目标>` 内嵌目标，无删除语义。
        if let Some(target) = item.id.strip_prefix(crate::aggregator::DIRECT_ITEM_PREFIX) {
            let is_url = url_like(target).is_some();
            return vec![
                CtxRow::Entry(CtxEntry {
                    glyph: default_action_glyph(item),
                    label: footer_action_text(lang, item),
                    shortcut: "↵ Enter",
                    action: CtxAction::Default,
                }),
                CtxRow::Separator,
                CtxRow::Entry(CtxEntry {
                    glyph: if is_url {
                        CTX_GLYPH_LINK
                    } else {
                        CTX_GLYPH_COPY
                    },
                    label: crate::text::t(
                        lang,
                        if is_url {
                            "ctx.copy_link"
                        } else {
                            "ctx.copy_path"
                        },
                    )
                    .to_string(),
                    shortcut: "Ctrl+Shift+C",
                    action: CtxAction::CopyText {
                        text: target.to_string(),
                    },
                }),
                CtxRow::Separator,
                CtxRow::Entry(CtxEntry {
                    glyph: CTX_GLYPH_SAVE,
                    label: crate::text::t(lang, "ctx.save_as_command").to_string(),
                    shortcut: "",
                    action: CtxAction::SaveAsCommand {
                        title: target.to_string(),
                        target: target.to_string(),
                        kind: if is_url {
                            crate::settings::CustomCommandKind::Url
                        } else {
                            crate::settings::CustomCommandKind::Path
                        },
                    },
                }),
            ];
        }
    }
    let mut rows = vec![CtxRow::Entry(CtxEntry {
        glyph: default_action_glyph(item),
        label: footer_action_text(lang, item),
        shortcut: "↵ Enter",
        action: CtxAction::Default,
    })];
    // v3.3 P1：扩展自带 `more_commands`（§8.1）→ 以扩展动作渲染（分隔线隔开
    // 默认动作组），GUI 静态类别映射让位——扩展最清楚自己的二级动作；避免
    // 与静态动作重复（如文件搜索的 显示所在目录/复制路径）。
    if !item.more_commands.is_empty() {
        rows.push(CtxRow::Separator);
        for cmd in &item.more_commands {
            rows.push(CtxRow::Entry(CtxEntry {
                glyph: glyph_from_command(cmd),
                label: cmd.title.clone(),
                shortcut: "",
                action: CtxAction::Extension {
                    command_id: cmd.id.clone(),
                },
            }));
        }
        return rows;
    }
    // N19 真机核对轮修正（2026-11-22）：文件动作按**路径形态**兜底门控——文件
    // 搜索页行 `result_category` 缺省（sidecar 只设 section），`Some("文件")`
    // 臂在真实行型上不可达（与 Ctrl+Enter 的 Reveal 同族问题）。类别缺省但
    // 副标题为盘符/UNC 路径形态的行按「文件」处理；其余类别不变。
    let category = item
        .result_category
        .as_deref()
        .or_else(|| path_like(&item.subtitle).map(|_| "文件"));
    match category {
        Some("应用") => {
            // N13：屏蔽此应用**不依赖路径数据**（N2 屏蔽名单按显示名片段匹配
            // ——无路径 subtitle 的应用行同样可屏蔽），置菜单尾部独立分隔组；
            // 仅 apps 扩展的行提供（防御类别映射未来变更）。
            if let Some(path) = path_like(&item.subtitle) {
                rows.push(CtxRow::Entry(CtxEntry {
                    glyph: CTX_GLYPH_ADMIN,
                    label: crate::text::t(lang, "ctx.admin").to_string(),
                    shortcut: "Ctrl+↵",
                    action: CtxAction::RunAsAdmin { path: path.clone() },
                }));
                rows.push(CtxRow::Entry(CtxEntry {
                    glyph: CTX_GLYPH_FOLDER,
                    label: crate::text::t(lang, "ctx.locate").to_string(),
                    shortcut: "",
                    action: CtxAction::RevealInFolder { path: path.clone() },
                }));
                rows.push(CtxRow::Separator);
                rows.push(CtxRow::Entry(CtxEntry {
                    glyph: CTX_GLYPH_COPY,
                    label: crate::text::t(lang, "ctx.copy_path").to_string(),
                    shortcut: "Ctrl+Shift+C",
                    action: CtxAction::CopyText { text: path },
                }));
            }
            if item.ext_id == "com.ddrun.apps" {
                rows.push(CtxRow::Separator);
                rows.push(CtxRow::Entry(CtxEntry {
                    glyph: CTX_GLYPH_DELETE,
                    label: crate::text::t(lang, "ctx.hide_app").to_string(),
                    shortcut: "",
                    action: CtxAction::HideApp {
                        title: item.title.clone(),
                    },
                }));
            }
        }
        Some("文件") => {
            let Some(path) = path_like(&item.subtitle) else {
                return rows;
            };
            rows.push(CtxRow::Entry(CtxEntry {
                glyph: CTX_GLYPH_FOLDER,
                label: crate::text::t(lang, "ctx.locate").to_string(),
                shortcut: "",
                action: CtxAction::RevealInFolder { path: path.clone() },
            }));
            rows.push(CtxRow::Separator);
            rows.push(CtxRow::Entry(CtxEntry {
                glyph: CTX_GLYPH_COPY,
                label: crate::text::t(lang, "ctx.copy_path").to_string(),
                shortcut: "Ctrl+Shift+C",
                action: CtxAction::CopyText { text: path.clone() },
            }));
            // N19：文件结果 → 一键转 path 型自定义命令（预填设置卡，不静默写配置）
            rows.push(CtxRow::Separator);
            rows.push(CtxRow::Entry(CtxEntry {
                glyph: CTX_GLYPH_SAVE,
                label: crate::text::t(lang, "ctx.save_as_command").to_string(),
                shortcut: "",
                action: CtxAction::SaveAsCommand {
                    title: item.title.clone(),
                    target: path,
                    kind: crate::settings::CustomCommandKind::Path,
                },
            }));
        }
        Some("网页") => {
            if let Some(url) = url_like(&item.subtitle) {
                rows.push(CtxRow::Separator);
                rows.push(CtxRow::Entry(CtxEntry {
                    glyph: CTX_GLYPH_LINK,
                    label: crate::text::t(lang, "ctx.copy_link").to_string(),
                    shortcut: "Ctrl+Shift+C",
                    action: CtxAction::CopyText { text: url.clone() },
                }));
                // N19：URL 结果 → 一键转 url 型自定义命令（预填设置卡，不静默写配置）
                rows.push(CtxRow::Entry(CtxEntry {
                    glyph: CTX_GLYPH_SAVE,
                    label: crate::text::t(lang, "ctx.save_as_command").to_string(),
                    shortcut: "",
                    action: CtxAction::SaveAsCommand {
                        title: item.title.clone(),
                        target: url,
                        kind: crate::settings::CustomCommandKind::Url,
                    },
                }));
            }
        }
        // 命令 / 设置 / 未知类别：仅默认动作（10B.2）
        _ => {}
    }
    // N11：置顶/取消置顶（根页条目通用尾项；独立分隔组，方案 §4.6 右键动作）。
    if let Some(pinned) = pin {
        rows.push(CtxRow::Separator);
        rows.push(pin_row(pinned));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── v4.4 10B：右键菜单映射（D18）与数据门控 ────────────────────

    /// 构造指定类别 + subtitle 的列表项。
    fn ctx_item(category: &str, subtitle: &str) -> PanelItem {
        PanelItem {
            result_category: Some(category.to_string()),
            subtitle: subtitle.to_string(),
            ext_id: "com.ddrun.apps".to_string(),
            ..PanelItem::new("app.1")
        }
    }

    /// v3.3 P1 夹具：构造一条最小 `more_commands` 菜单项（§8.1）。
    fn more_cmd(id: &str, title: &str, glyph: char) -> CommandItem {
        CommandItem {
            id: id.to_string(),
            title: title.to_string(),
            subtitle: None,
            icon: Some(dd_protocol::model::Icon {
                kind: dd_protocol::model::IconKind::Glyph,
                value: glyph.to_string(),
            }),
            section: None,
            tags: None,
            details: None,
            text_to_suggest: None,
            more_commands: None,
            command: dd_protocol::model::CommandRef::Invoke,
        }
    }
    /// 菜单行中的动作标签序列（跳过分隔线）。
    fn entry_labels(rows: &[CtxRow]) -> Vec<&str> {
        rows.iter()
            .filter_map(|r| match r {
                CtxRow::Entry(e) => Some(e.label.as_str()),
                CtxRow::Separator => None,
            })
            .collect()
    }

    #[test]
    fn ctx_menu_app_with_path_gets_full_mapping() {
        // 10B.2 应用 + N13：打开 / 以管理员身份运行 / 打开所在位置 / ─ / 复制路径 / ─ / 屏蔽此应用
        let item = ctx_item("应用", r"C:\WINDOWS\system32\AgentService.exe");
        let rows = context_menu_rows(Lang::ZhCn, &item);
        assert_eq!(
            entry_labels(&rows),
            vec![
                "打开应用",
                "以管理员身份运行",
                "打开所在位置",
                "复制路径",
                "屏蔽此应用"
            ]
        );
        // 分隔线在「复制路径」前（默认/高频组与复制组之间，10B.0 解剖图）
        assert!(
            matches!(rows[rows.len() - 4], CtxRow::Separator),
            "分隔线在复制路径之前"
        );
        // 分隔线在「屏蔽此应用」前（N13 独立分隔组）
        assert!(
            matches!(rows[rows.len() - 2], CtxRow::Separator),
            "分隔线在屏蔽此应用之前"
        );
        // 第一项 = 默认动作，快捷键 ↵ Enter（10B.1 默认动作行）
        match &rows[0] {
            CtxRow::Entry(e) => {
                assert_eq!(e.shortcut, "↵ Enter");
                assert_eq!(e.action, CtxAction::Default);
            }
            CtxRow::Separator => panic!("首项必须是动作"),
        }
        // N13 载荷 = 显示名（屏蔽名单按显示名片段匹配）
        match &rows[rows.len() - 1] {
            CtxRow::Entry(e) => match &e.action {
                CtxAction::HideApp { title } => assert_eq!(title, "app.1"),
                other => panic!("应为 HideApp 动作，实际 {other:?}"),
            },
            CtxRow::Separator => panic!("末项应为动作"),
        }
    }

    #[test]
    fn ctx_menu_gates_secondary_actions_on_data_availability() {
        // subtitle 无路径形态（如无目标路径的项）→ 路径型动作不出现；
        // N13 屏蔽动作不依赖路径数据，仍然提供
        let item = ctx_item("应用", "");
        assert_eq!(
            entry_labels(&context_menu_rows(Lang::ZhCn, &item)),
            vec!["打开应用", "屏蔽此应用"]
        );
        let item = ctx_item("应用", "快捷方式");
        assert_eq!(
            entry_labels(&context_menu_rows(Lang::ZhCn, &item)),
            vec!["打开应用", "屏蔽此应用"]
        );
    }

    #[test]
    fn ctx_menu_hide_app_gated_on_apps_ext() {
        // N13 防御门：屏蔽动作只对 apps 扩展的行提供（写入目标是 apps
        // 屏蔽名单；类别映射未来变更时不误挂）
        let mut item = ctx_item("应用", r"C:\x\y.exe");
        item.ext_id = "com.ddrun.somefuture".to_string();
        assert_eq!(
            entry_labels(&context_menu_rows(Lang::ZhCn, &item)),
            vec!["执行", "以管理员身份运行", "打开所在位置", "复制路径"],
            "非 apps 扩展无屏蔽项（默认动作文案随 ext 映射回退「执行」）"
        );
    }

    #[test]
    fn ctx_menu_file_category_has_no_admin_entry() {
        // 10B.2 文件：无管理员运行项（非可执行语义）；N19 尾部 + 存为自定义命令
        let item = ctx_item("文件", r"G:\AI\dd-run\Cargo.toml");
        assert_eq!(
            entry_labels(&context_menu_rows(Lang::ZhCn, &item)),
            vec!["打开应用", "打开所在位置", "复制路径", "存为自定义命令"]
        );
        // SaveAsCommand 载荷：path 型 + 目标 = 副标题路径
        let rows = context_menu_rows(Lang::ZhCn, &item);
        match &rows[rows.len() - 1] {
            CtxRow::Entry(e) => match &e.action {
                CtxAction::SaveAsCommand {
                    title,
                    target,
                    kind,
                } => {
                    assert_eq!(title, "app.1");
                    assert_eq!(target, r"G:\AI\dd-run\Cargo.toml");
                    assert_eq!(*kind, crate::settings::CustomCommandKind::Path);
                }
                other => panic!("应为 SaveAsCommand，实际 {other:?}"),
            },
            CtxRow::Separator => panic!("末项应为动作"),
        }
    }

    #[test]
    fn ctx_menu_web_url_gets_copy_link_hint_text_does_not() {
        // 10B.2 网页：URL 形态 subtitle → 复制链接；提示文案（现行 websearch
        // 顶层项）不含 URL → 仅默认动作（门控规则，偏离记档见函数 doc）
        let mut item = ctx_item("网页", "https://example.com/search?q=dd");
        item.ext_id = "com.ddrun.websearch".to_string();
        let rows = context_menu_rows(Lang::ZhCn, &item);
        assert_eq!(
            entry_labels(&rows),
            vec!["打开网页", "复制链接", "存为自定义命令"]
        );
        assert!(matches!(rows[rows.len() - 3], CtxRow::Separator));
        let mut item = ctx_item("网页", "输入关键词后在浏览器中搜索");
        item.ext_id = "com.ddrun.websearch".to_string();
        assert_eq!(
            entry_labels(&context_menu_rows(Lang::ZhCn, &item)),
            vec!["打开网页"]
        );
    }

    #[test]
    fn ctx_menu_command_and_settings_default_only() {
        // 10B.2 命令 / 设置：仅默认动作，菜单保持单项
        for category in ["命令", "设置"] {
            let item = ctx_item(category, "任意副标题");
            assert_eq!(
                entry_labels(&context_menu_rows(Lang::ZhCn, &item)),
                vec!["打开应用"],
                "{category} 仅默认动作"
            );
        }
    }

    #[test]
    fn ctx_menu_focus_count_skips_separators() {
        // 键盘焦点循环只在动作项上进行（Separator 不占焦点位）
        let item = ctx_item("应用", r"C:\x\y.exe");
        let rows = context_menu_rows(Lang::ZhCn, &item);
        let state = CtxMenuState {
            visible_idx: 0,
            item_id: "app.1".to_string(),
            rows,
            anchor: egui::Pos2::ZERO,
            focus: 0,
        };
        assert_eq!(ctx_entry_count(Some(&state)), 5, "5 个动作项 + 2 条分隔线");
        assert_eq!(ctx_entry_count(None), 0, "菜单关闭 = 无焦点项");
    }

    // ── v3.3 P1：扩展 more_commands 渲染（§8.1）────────────────────

    #[test]
    fn ctx_menu_more_commands_override_static_mapping() {
        // 扩展自带 more_commands → 渲染扩展动作，静态类别映射让位（避免
        // 与 文件搜索 的 显示所在目录/复制路径 重复）
        let mut item = ctx_item("命令", r"G:\AI\dd-run\报告.txt");
        item.ext_id = "com.ddrun.filesearch".to_string();
        item.more_commands = vec![
            more_cmd("files.reveal.7", "显示所在目录", '\u{E8DA}'),
            more_cmd("files.copy.7", "复制路径", '\u{E8C8}'),
        ];
        let rows = context_menu_rows(Lang::ZhCn, &item);
        assert_eq!(
            entry_labels(&rows),
            vec!["打开文件", "显示所在目录", "复制路径"],
            "默认动作（filesearch 走 footer.invoke=打开文件）+ 扩展两动作，无静态类别动作"
        );
        // 分隔线隔开默认动作组；扩展项带 Extension 动作与命令 id
        assert!(matches!(rows[1], CtxRow::Separator));
        match &rows[2] {
            CtxRow::Entry(e) => match &e.action {
                CtxAction::Extension { command_id } => {
                    assert_eq!(command_id, "files.reveal.7");
                    assert_eq!(e.glyph, '\u{E8DA}', "字形取自扩展 Icon");
                }
                other => panic!("应为 Extension 动作，实际 {other:?}"),
            },
            CtxRow::Separator => panic!("第 3 行应为动作"),
        }
    }

    #[test]
    fn ctx_menu_more_commands_fallback_glyph_for_path_icons() {
        // 非 Glyph 图标（如 Path）→ 回退通用动作字形，不 panic
        let mut item = ctx_item("命令", "");
        item.more_commands = vec![more_cmd("x.do", "做点事", '\u{E721}')];
        item.more_commands[0].icon = Some(dd_protocol::model::Icon {
            kind: dd_protocol::model::IconKind::Path,
            value: "C:\\i.png".to_string(),
        });
        let rows = context_menu_rows(Lang::ZhCn, &item);
        match &rows[2] {
            CtxRow::Entry(e) => {
                assert_eq!(
                    e.glyph,
                    crate::text::CTX_GLYPH_PLAY,
                    "非 Glyph 回退通用字形"
                )
            }
            CtxRow::Separator => panic!("应为动作"),
        }
    }
}
