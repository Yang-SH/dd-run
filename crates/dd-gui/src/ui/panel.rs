//! 主面板：搜索栏 + 列表 + 页脚。

use crate::app::PaletteApp;
use crate::text::footer_action_text;
use crate::text::nested_search_placeholder;
use crate::ui::row::draw_item_row;
use crate::ui::settings_view::draw_ext_chip;
use crate::ui::states::draw_empty_state;
use crate::ui::states::draw_loading_state;
use crate::ui::widgets::draw_back_btn;
use crate::ui::widgets::draw_settings_gear;
use crate::ui::widgets::keycap_width;
use crate::ui::widgets::keys_width;
use crate::ui::widgets::paint_keycap;
use crate::ui::widgets::paint_keys_at;
use crate::ui::widgets::text_width;
use dd_gui::state::PanelItem;
use dd_gui::theme;
use eframe::egui;

/// 搜索栏前缀搜索图标（Segoe MDL2 "Search" U+E721；设计稿 01 searchbar glyph）。
pub(crate) const SEARCH_GLYPH: char = '\u{E721}';

impl PaletteApp {
    // ── 渲染 ─────────────────────────────────────────────────

    pub(crate) fn draw_panel(&mut self, ui: &mut egui::Ui) {
        // 底部固定栏：源状态 + 键位提示（始终贴窗口底，不受中央列表高度影响；
        // 解决 M3 实测"列表长时把页脚挤出 460px 窗口"——之前把它们放进
        // CentralPanel 内的 ScrollArea 之后，长列表时整个 footer 块被推到
        // 视口下方。Panel::bottom 是 egui 0.36 处理 chrome vs content 的标准做法）。
        // 这里对 self 做不可变再借用，闭包结束后即可变借用给下面的 CentralPanel。
        // 批次 4.2：设置页**也**渲染全局页脚——`draw_status_footer` 内按 `is_settings`
        // 早返回到「左说明文案 + 右 Esc 返回」分支（设计稿 §08 line 1223-1226 + D15 line 971）。
        let mut open_settings = false;
        let self_ref: &Self = &*self;
        let p = theme::Palette::of(ui.visuals().dark_mode);
        // v4.7 D31（真机反馈修订）：材质生效时页脚同样透出系统材质——否则底部
        // 一条不透明带把材质面板割裂；全关/回退时保持 --panel-2 不透明。
        let footer_fill = if self.backdrop_active {
            egui::Color32::TRANSPARENT
        } else {
            p.panel_2
        };
        // 批次 4.0：页脚最左齿轮按钮的点击旗标（闭包内置位，面板结束后消费）。
        let footer =
            egui::containers::Panel::bottom("status_footer")
                // 关掉内建分隔线：它用 `widgets.noninteractive.bg_stroke`（默认灰），
                // 颜色与 `--border` 不一致，下面按设计稿手绘 1px `--border` 顶边。
                .show_separator_line(false)
                // CSS `.panel-footer`：background `--panel-2` + padding 8px 16px。
                // `Panel` 默认 frame 是 `Frame::side_top_panel`：fill 取 `--panel`
                // （页脚与列表区同色，失去区隔）且 margin 仅 (8,2)。
                .frame(egui::Frame::default().fill(footer_fill).inner_margin(
                    egui::Margin::symmetric(theme::FOOTER_PAD_X as i8, theme::FOOTER_PAD_Y as i8),
                ))
                .show(ui, |ui| {
                    open_settings = self_ref.draw_status_footer(ui);
                });
        // 顶部 1px `--border`（CSS `.panel-footer` border-top）；贴面板外框上沿，
        // 因此用返回的 response.rect（= 面板外矩形），而非闭包内被 margin 收缩过的 max_rect。
        ui.painter().hline(
            footer.response.rect.x_range(),
            footer.response.rect.top(),
            egui::Stroke::new(1.0, p.border),
        );
        if open_settings {
            self.open_settings();
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(ui.style().visuals.panel_fill)
                    // 设计稿 00.1 顶行 padding（v4）：8px 12px 4px —— 顶部 8 +
                    // 搜索栏 40 + 下方 4 = 52px；底部 8 由 `.results` padding-bottom 承担。
                    .inner_margin(egui::Margin {
                        left: 12,
                        right: 12,
                        top: 8,
                        bottom: 8,
                    }),
            )
            .show(ui, |ui| {
                // ── 设置页（批次 4.0）：独立视图，不走 searchbar/列表链路 ──
                if self.stack.current().is_settings {
                    self.draw_settings(ui);
                    return;
                }
                // ── 统一顶行（设计稿 §07.1 v4，C 组批次 C1）──────────────
                // Root = 搜索框占满；嵌套页 = 返回按钮 28×28 + 搜索框 flex:1，
                // 页标题进 placeholder、ext_id 徽标移页脚右端（D2）。
                // 三页顶行完全同构（同 40px 高），切换零位移（验收 A1/A5）。
                // 旧实现（嵌套页独立标题行 + "[Esc] 返回" 文本）已废弃。
                let page = self.stack.current();
                let is_nested = page.page_id.is_some() && !page.is_settings;
                let page_title = if is_nested {
                    page.title.clone()
                } else {
                    String::new()
                };
                let mut go_back_clicked = false;
                let (row_rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), theme::SEARCHBAR_H),
                    egui::Sense::hover(),
                );
                let mut search_rect = row_rect;
                if is_nested {
                    // 返回按钮：28×28 子区垂直居中（与设置页顶行同构，
                    // `draw_back_btn` 恰好填满子区）
                    let back_rect = egui::Rect::from_min_size(
                        egui::pos2(row_rect.min.x, row_rect.center().y - 14.0),
                        egui::vec2(28.0, 28.0),
                    );
                    let mut back_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(back_rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Min)),
                    );
                    go_back_clicked = draw_back_btn(&mut back_ui, &p);
                    search_rect.min.x = back_rect.right() + 8.0;
                }
                let mut query = self.stack.current().list.query().to_owned();
                let placeholder = if is_nested {
                    nested_search_placeholder(&page_title)
                } else {
                    "搜索命令…".to_string()
                };
                let search_ui = &mut ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(search_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                let resp = draw_searchbar(search_ui, &mut query, &placeholder);
                {
                    let page = self.stack.current_mut();
                    page.list.set_query(query);
                    if self.want_focus {
                        resp.request_focus();
                        self.want_focus = false;
                    }
                }
                if go_back_clicked {
                    self.stack.go_back();
                }
                // M4 宿主 fallback：查询变化后同步兜底展示/拉取（页面借用已释放）
                self.sync_fallback();
                ui.add_space(4.0);

                // ── 列表区 ───────────────────────────────────
                if self.aggregating {
                    // C 组批次 C2（§07.2）：首屏聚合加载与子页拉取共用同一
                    // Loading 组件（Spinner 22px accent + 3 条骨架行），替换
                    // 旧纯文本占位「正在加载扩展…」（2026-09-05 真机核对不一致）。
                    // 动画驱动：egui 按需重绘，加载期间 ~30fps 轮询重绘。
                    let dark = ui.visuals().dark_mode;
                    let time = ui.ctx().time();
                    draw_loading_state(ui, &p, dark, time);
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(33));
                } else {
                    self.draw_list(ui);
                }
            });
    }

    /// 底部固定栏内容：**设置按钮**（最左常驻，§6.1）+ **上下文动作提示**
    /// （批次 4.1，§6.3）+ 键位图例。
    ///
    /// 布局（§6.3 + C6 + **D15 v4.3 修订**）：
    /// - **严格单行**（`FOOTER_PAD_Y 8 + KEYCAP_H + FOOTER_PAD_Y 8`；v4.10 D35：
    ///   KEYCAP_H 16→20 ⇒ 页脚总高 32→36，D8 原值 32）：
    ///   `allocate_exact_size` 锁一行 + 手动锚定行中心线（混合高度内容下
    ///   egui 自动居中逐项漂移，真机 2026-09-04 修复），**禁止 `horizontal_wrapped`**。
    /// - **有选中项**：左侧 = 齿轮 + 选中项默认动作文本（[`footer_action_text`]
    ///   实时推导，C7）；**无选中 / 空态**：左侧仅齿轮；拉取中显示「正在加载…」
    ///   （§07 loading mockup）。
    /// - **右侧键位图例恒定完整**（v4.11 修订：Enter 执行 / Esc 返回·隐藏，
    ///   移除 ↑↓ 选择；不再随选中态退化为单键帽）；嵌套页右端追加 `ext_id` 徽标
    ///   （§07.1，C 组批次 C1）。
    /// - **源健康诊断整体移除**（v4.3 修订，2026-09-04 用户决策）：stub/err
    ///   状态点与聚合 note 不再进页脚（原 C8"仅异常时显示"规格废止）。
    /// - 批次 4.0：齿轮在最左（Root/子页/空态恒定，设置页除外，§6.1）；
    ///   返回值 = 本帧齿轮是否被点击（调用方在面板闭包结束后 `open_settings`）。
    pub(crate) fn draw_status_footer(&self, ui: &mut egui::Ui) -> bool {
        let p = theme::Palette::of(ui.visuals().dark_mode);

        // §08 设计稿 D15（line 595、line 971、line 1449）：设置页**不渲染齿轮**，
        // 左 = "修改主题立即生效并持久化" 说明文案，右 = "Esc 返回" 键位。
        // 此分支早返回 false（设置页无齿轮点击）。
        //
        // 对齐策略（真机 2026-09-04 修复"底栏不在一条线上"）：egui 自动垂直居中
        // （`Align::Center`）把每个控件居中于**剩余**空间，混合高度内容（24px 齿轮 /
        // 16px 键帽 / 文本）会逐项漂移。因此本函数所有元素改为**手动锚定行中心线
        // `cy`**：键帽/文本用 `paint_keycap` / `painter.text` 显式定位，齿轮用
        // 24×24 精确 max_rect 子区（恰好填满，无漂移），动作文本用 16px 高子区
        // （首控件居中无漂移）。
        if self.stack.current().is_settings {
            let total_w = ui.available_width();
            let (row, _) =
                ui.allocate_exact_size(egui::vec2(total_w, theme::KEYCAP_H), egui::Sense::hover());
            let cy = row.center().y;
            // 左：说明文案（与齿轮同 x 起点——Panel 内边距已含 16px，不再额外缩进）
            ui.painter().text(
                egui::pos2(row.min.x, cy),
                egui::Align2::LEFT_CENTER,
                "设置修改自动保存；搜索引擎更改返回首屏后生效",
                egui::FontId::proportional(theme::FOOTER_FONT),
                p.text3,
            );
            // 右：`返回 [Esc]`——v4.10 D35 组内顺序 = 说明在前、键帽在后；
            // Esc 键帽贴右缘，「返回」右对齐其左 KEYCAP_DESC_GAP 处。
            let esc_w = keycap_width(ui, "Esc");
            let esc_left = row.right() - esc_w;
            let krect = egui::Rect::from_min_size(
                egui::pos2(esc_left, cy - theme::KEYCAP_H / 2.0),
                egui::vec2(esc_w, theme::KEYCAP_H),
            );
            paint_keycap(ui, krect, "Esc", &p);
            ui.painter().text(
                egui::pos2(esc_left - theme::KEYCAP_DESC_GAP, cy),
                egui::Align2::RIGHT_CENTER,
                "返回",
                egui::FontId::proportional(theme::FOOTER_FONT),
                p.text2,
            );
            return false;
        }

        // C7：动作文本随选中项实时变化（含 fallback 模式——filtered() 覆盖兜底集）。
        // 用户决策（2026-09-04）：页脚不再显示源状态诊断（stub 状态点 / failed
        // 报错 / 聚合 note）——左块 = 齿轮 + 上下文动作，右块 = 完整键位图例
        // （Enter 执行 / Esc 返回·隐藏，v4.11 移除 ↑↓ 选择）。
        // C 组批次 C1（§07.1）：嵌套页页脚右端常驻 ext_id 徽标——键位图例
        // 整体左移 chip 宽 + 间距让位。
        let page = self.stack.current();
        let ext_chip_w = if page.page_id.is_some() && !page.is_settings {
            text_width(ui, &page.ext_id, egui::FontId::monospace(10.0)) + 16.0
        } else {
            0.0
        };
        // C7：动作文本随选中项实时变化（含 fallback 模式——filtered() 覆盖兜底集）。
        // §07 loading mockup：拉取中（无选中项）页脚左侧显示「正在加载…」提示。
        let action = page
            .list
            .selected_item()
            .map(footer_action_text)
            .or_else(|| page.is_loading.then(|| "正在加载…".to_string()));
        let keys_w = keys_width(ui)
            + if ext_chip_w > 0.0 {
                theme::FOOTER_GAP + ext_chip_w
            } else {
                0.0
            };
        let total_w = ui.available_width();

        // 严格单行：分配 KEYCAP_H 高的行矩形，所有元素锚定中心线 cy
        let (row, _) =
            ui.allocate_exact_size(egui::vec2(total_w, theme::KEYCAP_H), egui::Sense::hover());
        let cy = row.center().y;
        let split_x = row.right() - keys_w;

        // 左 1：齿轮（24×24 子区恰好填满，中心 = cy）
        let gear_rect = egui::Rect::from_min_size(
            egui::pos2(row.min.x, cy - theme::GEAR_SIZE / 2.0),
            egui::vec2(theme::GEAR_SIZE, theme::GEAR_SIZE),
        );
        let mut gear_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(gear_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Min)),
        );
        let clicked = draw_settings_gear(&mut gear_ui, &p);

        // 左 2：上下文动作文本（16px 高子区，首控件垂直居中无漂移；`Label::truncate`
        // 让超宽文本在左块边界处省略号截断，不溢出盖到右侧键位区）。
        if let Some(text) = &action {
            let text_rect = egui::Rect::from_min_max(
                egui::pos2(gear_rect.right() + theme::FOOTER_GAP, cy - 8.0),
                egui::pos2(split_x - theme::FOOTER_GAP, cy + 8.0),
            );
            if text_rect.width() > 24.0 {
                let mut text_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(text_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                // 动作文本用 text2（较键位图例的 text3 略强调一级）
                text_ui.add(
                    egui::Label::new(
                        egui::RichText::new(text)
                            .size(theme::FOOTER_FONT)
                            .color(p.text2),
                    )
                    .truncate(),
                );
            }
        }

        // 右：完整键位图例，全部手绘锚定 cy（Enter 执行 / Esc 返回·隐藏）
        paint_keys_at(ui, egui::pos2(split_x, cy), &p);
        // 嵌套页：ext_id 徽标贴右缘垂直居中（draw_ext_chip 恰好填满其子区）
        if ext_chip_w > 0.0 {
            let chip_rect = egui::Rect::from_min_size(
                egui::pos2(row.right() - ext_chip_w, cy - 8.0),
                egui::vec2(ext_chip_w, 16.0),
            );
            let mut chip_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(chip_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Min)),
            );
            draw_ext_chip(&mut chip_ui, &self.stack.current().ext_id, &p);
        }
        clicked
    }
}

impl PaletteApp {
    /// 当前页的列表渲染（Loading / 空态 / 按 `section` 分组）。
    ///
    /// 选中项通过 `scroll_to_me(None)` 滚入可视区（仅键盘选中时跟随，`scroll_follow`；
    /// 鼠标选中不滚动，避免内容在静止指针下位移造成错位）；鼠标 hover 高亮、单击执行
    /// （与 `Enter` 等价）。回写规则（修复鼠标/键盘选择互相干扰）：
    /// - `clicked`：选中并直接执行，不受 hover 冲突规则影响；
    /// - `hovered`：**仅当鼠标指针本帧真正移动过**（`hover_pos` 与上帧不同）且悬停行
    ///   与基准不同才接管选中——静止不动的鼠标不抢占键盘（Tab/↓/↑）选中，修复
    ///   「一直按 ↑ 滚到顶部时，内容从静止鼠标下方滑过把选中抢回鼠标所在行」。
    pub(crate) fn draw_list(&mut self, ui: &mut egui::Ui) {
        // M5 批次 3：本帧主题色板（组件色统一经 Palette，不写裸色值）
        let p = theme::Palette::of(ui.visuals().dark_mode);
        // 本帧鼠标指针屏幕坐标（用于区分「鼠标真的动了」vs「内容在静止鼠标下滚动」）。
        let current_hover_pos = ui.input(|i| i.pointer.hover_pos());

        // 先把需要的状态拷贝出来（释放对 `self` 的不可变借用），
        // 以便循环结束后可写回 hover/click 结果，避免借用冲突。
        let (is_loading, empty, selected, items, query_empty) = {
            let page = self.stack.current();
            (
                page.is_loading,
                page.empty.clone(),
                page.list.selected_index(),
                page.list
                    .filtered()
                    .map(|(i, it)| (i, it.clone()))
                    .collect::<Vec<_>>(),
                page.list.query().is_empty(),
            )
        };

        if is_loading {
            // C 组批次 C2（§07.2）：Spinner + 3 条骨架行替换纯文本；
            // 动画驱动：egui 按需重绘，加载期间 ~30fps 轮询重绘。
            let dark = ui.visuals().dark_mode;
            let time = ui.ctx().time();
            draw_loading_state(ui, &p, dark, time);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
            return;
        }
        if let Some(empty) = empty {
            draw_empty_state(ui, &p, &empty, None);
            return;
        }
        if items.is_empty() {
            if query_empty {
                // 设计稿 02 屏纯空态：图标 + 标题 + 描述
                draw_empty_state(ui, &p, "未发现命令", Some("检查扩展清单或扩展运行状态"));
            } else {
                draw_empty_state(ui, &p, "未找到匹配的命令", Some("试试其他关键词。"));
            }
            return;
        }

        // 按 section 分组（用拷贝出的 items，不借用 self）
        let mut groups: Vec<(String, Vec<(usize, PanelItem)>)> = Vec::new();
        for (idx, item) in &items {
            match groups.iter_mut().find(|(s, _)| s == &item.section) {
                Some((_, list)) => list.push((*idx, item.clone())),
                None => groups.push((item.section.clone(), vec![(*idx, item.clone())])),
            }
        }

        // M5 批次 2：ScrollArea 闭包外预解析图标（闭包内只读借用，避免借用冲突）
        let icon_views = self.resolve_icons(ui.ctx(), &items);

        let mut hovered: Option<usize> = None;
        let mut clicked: Option<usize> = None;
        // v4.4（D19）：右键行 → 打开菜单（锚点 = 右键点）；选中行矩形仅在
        // 键盘触发（Shift+F10）时用于锚定行底边左缘（D20）。
        let mut right_clicked: Option<(usize, egui::Pos2)> = None;
        let mut selected_rect: Option<egui::Rect> = None;
        // 本帧行矩形存档（菜单开着时右键另一行的命中依据，见 `reopen_ctx_menu_at`）。
        let mut row_rects: Vec<(usize, egui::Rect)> = Vec::new();
        let scroll_follow = self.scroll_follow;
        egui::ScrollArea::vertical().show(ui, |ui| {
            // `.results` 容器：padding 6px 6px 8px（CSS 简写 = 上 6 / 左右 6 /
            // 下 8；行相对搜索栏再内收 6px、列表底部留 8px。egui Frame
            // inner_margin 承担——与顶行 padding-bottom 4 合计 10px，同设计稿）。
            egui::Frame::default()
                .inner_margin(egui::Margin {
                    left: 6,
                    right: 6,
                    top: 6,
                    bottom: 8,
                })
                .show(ui, |ui| {
                    for (section, group_items) in &groups {
                        if !section.is_empty() {
                            // 分组标题（CSS `.section-label` v4/D14：caption1Strong
                            // 12/16·600、text-3、上方留白 12、下方 4；组间留白无分隔线）
                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.label(
                                    egui::RichText::new(section)
                                        .size(12.0)
                                        .strong()
                                        .color(p.text3),
                                );
                            });
                            ui.add_space(4.0);
                        } else {
                            ui.add_space(4.0);
                        }
                        for (idx, item) in group_items {
                            let resp = draw_item_row(
                                ui,
                                item,
                                Some(*idx) == selected,
                                icon_views.get(idx),
                            );
                            row_rects.push((*idx, resp.rect));
                            if Some(*idx) == selected {
                                if scroll_follow {
                                    // 选中项滚入可视区（仅键盘选中时跟随；鼠标选中不滚动，
                                    // 避免内容在静止指针下移动造成高亮与光标错位）
                                    resp.scroll_to_me(None);
                                }
                                // v4.4：记录选中行矩形（Shift+F10 菜单锚定用）
                                selected_rect = Some(resp.rect);
                            }
                            if resp.hovered() {
                                hovered = Some(*idx);
                            }
                            if resp.clicked() {
                                clicked = Some(*idx);
                            }
                            if resp.secondary_clicked() {
                                if let Some(pos) = resp.interact_pointer_pos() {
                                    right_clicked = Some((*idx, pos));
                                }
                            }
                        }
                    }
                });
        });
        self.ctx_row_rects = row_rects;

        // 回写鼠标结果：
        // - clicked：选中并直接执行（与 Enter 等价），不受 hover 冲突规则影响；
        // - hovered：**仅当鼠标指针本帧真正移动过**（`current_hover_pos` ≠ 上一帧），
        //   且悬停行与基准不同，才接管选中——静止的鼠标不抢占键盘（Tab/↓/↑）选中，
        //   修复「一直按 ↑ 滚到顶部时，内容从静止鼠标下滚过把选中抢回鼠标所在行」。
        let pointer_moved = self.last_pointer_pos != current_hover_pos;
        let last_hovered = self.last_hovered_index;
        if let Some(idx) = clicked {
            self.stack.current_mut().list.set_selected(idx);
            self.confirm_selected();
            self.last_hovered_index = clicked; // 点击位置即新的 hover 基准
            self.scroll_follow = false; // 鼠标驱动选中：不滚动跟随
        } else if pointer_moved && hovered != last_hovered {
            if let Some(idx) = hovered {
                if self.stack.current_mut().list.set_selected(idx) {
                    // 选中确实变化：滚随关闭 + **强制下一帧重绘**。
                    // egui 按需重绘——选中在帧末回写、高亮下一帧才绘制，
                    // 鼠标停下后没有新输入事件就不会再有下一帧，
                    // 高亮会「卡」在旧行直到再动鼠标。
                    self.scroll_follow = false;
                    ui.ctx().request_repaint();
                }
            }
            self.last_hovered_index = hovered;
        }
        // 更新指针坐标基准（无论是否移动都记录，供下一帧比较）。
        self.last_pointer_pos = current_hover_pos;

        // ── v4.4 右键菜单触发（D19）───────────────────────────────
        // 右键行：置选中（与键盘选中视觉一致）+ 打开菜单（锚点 = 右键点 + 2,2）。
        if let Some((idx, pos)) = right_clicked {
            if let Some((_, item)) = items.iter().find(|(i, _)| *i == idx) {
                let item = item.clone();
                self.stack.current_mut().list.set_selected(idx);
                self.last_hovered_index = Some(idx);
                self.scroll_follow = false;
                self.open_ctx_menu(
                    idx,
                    &item,
                    pos + egui::vec2(theme::CTX_ANCHOR_OFFSET, theme::CTX_ANCHOR_OFFSET),
                );
                ui.ctx().request_repaint();
            }
        }
        // 键盘触发（Shift+F10）：锚定选中行底边左缘（D20）。
        if self.want_ctx_menu_for_selected {
            self.want_ctx_menu_for_selected = false;
            let selected_now = self.stack.current().list.selected_index();
            if let (Some(sel), Some(rect)) = (selected_now, selected_rect) {
                if let Some((_, item)) = items.iter().find(|(i, _)| *i == sel) {
                    let item = item.clone();
                    let anchor = egui::pos2(rect.left(), rect.bottom());
                    self.open_ctx_menu(sel, &item, anchor);
                    ui.ctx().request_repaint();
                }
            }
        }
    }
}

pub(crate) fn draw_searchbar(
    ui: &mut egui::Ui,
    query: &mut String,
    placeholder: &str,
) -> egui::Response {
    let p = theme::Palette::of(ui.visuals().dark_mode);

    // 1) 在父 Ui 里**先**预留 40px 高的精确矩形（Fluent Input large，D8；这一步
    //    决定了 searchbar 真实占位高度，后面 child Ui 只能在这 40px 里横排，
    //    不会再把父 Ui 撑高）。
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), theme::SEARCHBAR_H),
        egui::Sense::hover(),
    );

    // 2) filled-darker 外观（D17）：只画 bg3 凹陷填充、圆角 4，**无边框**——
    //    Fluent 三种外观互斥，不再保留 v2 的「1px 全边框 + 2px 底边」混合形态。
    let radius = egui::CornerRadius::same(theme::SEARCHBAR_RADIUS);
    ui.painter().rect_filled(rect, radius, p.input_fill);

    // 3) 在 40px 矩形内开 child Ui：左右各留 12px padding（设计稿 searchbar
    //    padding 0 12px），内容用 left_to_right + Align::Center 垂直居中。
    let content_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 12.0, rect.top()),
        egui::pos2(rect.right() - 12.0, rect.bottom()),
    );
    let mut content_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );

    // 4) glyph 前缀（设计稿 01：text-2 16px；与输入框间距 = `.searchbar` gap 8px）
    content_ui.label(
        egui::RichText::new(SEARCH_GLYPH.to_string())
            .size(16.0)
            .color(p.text2),
    );
    content_ui.add_space(8.0);

    // 5) TextEdit：在 child_ui（限定 40px 高度）里扩展 INFINITY 宽度撑满，
    //    高度由 font(14px, body1) 自定 ~20px，居中显示。
    let resp = content_ui.add(
        egui::TextEdit::singleline(query)
            .frame(egui::Frame::new()) // 空 Frame：无底无边框，外观由外层 Frame 承担
            .hint_text(egui::RichText::new(placeholder).color(p.text3))
            .desired_width(f32::INFINITY)
            .font(egui::FontId::proportional(14.0)),
    );

    // 5.5) 中文输入法候选框位置修正：手动报告 IME 光标区域。
    // egui TextEdit 内部也会设置 ime，但实测 Microsoft Pinyin 候选窗会漂到屏幕左上角；
    // 在聚焦时显式覆盖为搜索框响应矩形，强制 winit 更新 set_ime_cursor_area。
    if resp.has_focus() {
        let text_w = text_width(
            &content_ui,
            query.as_str(),
            egui::FontId::proportional(14.0),
        );
        let cursor_x = (resp.rect.min.x + text_w).min(resp.rect.right() - 2.0);
        let cursor_y = resp.rect.center().y;
        let cursor_rect =
            egui::Rect::from_min_size(egui::pos2(cursor_x, cursor_y - 8.0), egui::vec2(1.0, 16.0));
        ui.ctx().output_mut(|o| {
            o.ime = Some(egui::output::IMEOutput {
                purpose: egui::IMEPurpose::Normal,
                rect: resp.rect,
                cursor_rect,
                should_interrupt_composition: false,
            });
        });
    }

    // 6) 聚焦指示（filled-darker）：仅聚焦时底部 2px accent 下划线；
    //    未聚焦无任何描边（Fluent 外观规范，与旧 border-strong 底边区分）。
    if resp.has_focus() {
        let bottom_bar = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.bottom() - 2.0),
            egui::pos2(rect.right(), rect.bottom()),
        );
        ui.painter()
            .rect_filled(bottom_bar, egui::CornerRadius::same(2), p.accent);
    }

    resp
}
