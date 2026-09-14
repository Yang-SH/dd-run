//! 列表行绘制。

use crate::ui::icons::draw_icon_cell;
use crate::ui::icons::IconView;
use crate::ui::widgets::text_width;
use dd_gui::state::PanelItem;
use dd_gui::theme;
use eframe::egui;

/// 文件结果右列（所在文件夹路径）宽度上限 = 可用宽 × 45%。
/// 取上限而非定值：窄面板下不挤光标题，宽面板下不无限伸展。
const FILE_PATH_W_FRACTION: f32 = 0.45;
/// 文件结果右列路径宽度下限（px）：极窄面板下仍给路径留可读宽度。
const FILE_PATH_MIN_W: f32 = 120.0;
/// 类型标签宽度上限（px，B5 既有口径 90 不变）。
const CAT_LABEL_MAX_W: f32 = 90.0;
/// 中列标题与右列文本之间的间隙（px，与 B5 的 `cat_w + 12.0` 同口径）。
const TRAIL_GAP: f32 = 12.0;

pub(crate) fn draw_item_row(
    ui: &mut egui::Ui,
    item: &PanelItem,
    selected: bool,
    icon: Option<&IconView>,
    hover_enabled: bool,
    fills: theme::RowFills,
    m: theme::ListMetrics,
) -> egui::Response {
    let p = theme::Palette::of(ui.visuals().dark_mode);

    // 预算行矩形（本帧指针测试 → 同帧 hover 填充，无帧延迟）
    let row_rect =
        egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), m.row_h));
    // v4.17a：`hover_enabled=false` = 鼠标自面板唤起后尚未动过（指针停在唤起
    // 前的残留位置）→ 不画 hover，避免与"第一项 keyboard 选中"抢视觉。
    let hovered_now = hover_enabled && ui.rect_contains_pointer(row_rect);
    let fill = if selected {
        // 选中行：玻璃填充 + 左侧 accent 竖条（材质档 = 比 hover 重一档的
        // 玻璃，回退档 = 实色 row_selected；与 hover 同为半透明/同族色，鼠标
        // 扫动时选择逐行移动是平滑过渡而非「不透明块跳变」——P2 v5 治闪烁）。
        // selected 行即使被 hover 也不画 hover（fill 分支已优先 selected）——
        // 避免键盘选中被鼠标覆盖（设计稿 §6.1 视觉一致性）。
        fills.selected
    } else if hovered_now {
        // 非选中行 hover：材质感知玻璃（P2 v5，`theme::row_fills`——
        // 云母加权 / 亚克力减重 / 回退实色）。
        fills.hover
    } else {
        egui::Color32::TRANSPARENT
    };

    let framed = egui::Frame::default()
        .fill(fill)
        .corner_radius(theme::ROW_RADIUS)
        // CSS `.row` padding（v4）：8px 10px 8px 8px（4px ramp，上下 8 → 行高 40）。
        // 注意 egui 0.36 `Margin` 字段为 i8。
        .inner_margin(egui::Margin {
            left: 8,
            right: 10,
            top: 8,
            bottom: 8,
        })
        .show(ui, |ui| {
            // 内容区固定 行高 - 16 = 24px 高（垂直居中图标与文字；上下 margin 8 不随档变）
            ui.set_min_height(m.row_h - 16.0);
            // B5：行内两段式——**标题占满中列 + 右列文本贴右**。右列文本按结果
            // 类型分派：文件结果 = 所在文件夹路径（uid 2026-09-14 修订，替代对
            // 该结果恒为「命令」的类别标签），其余 = 类别标签。副标题（应用 =
            // exe/lnk 路径）仍不在行内渲染，转整行 hover tooltip（信息不丢，
            // 用户决策 2026-09-12）。item_spacing.x 清零，间距全部显式控制，
            // 右列文本的贴右位置才能精确预算。
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.horizontal(|ui| {
                // 图标列（m.icon_cell；无图标/url 渲染弱色占位 glyph 保持各行对齐——I1）
                draw_icon_cell(ui, icon, m);
                ui.add_space(theme::LIST_ICON_GAP); // CSS `.row` gap 12px
                let font_title = theme::semibold(m.title_pt); // B1：行名 500 字重（设计稿 `.name`）
                let font_cat = egui::FontId::proportional(m.cat_pt);
                // ── 右列文本（uid 2026-09-14，B5 修订：文件地址常显）──────────
                // 文件结果：右列显示**所在文件夹路径**，替代来源类别标签。理由：
                // ① B5 把路径挪进 hover tooltip 后，文件搜索结果「看不到文件在哪」
                //    （真机反馈）；② 文件结果的类别标签恒为「命令」
                //    （`category_label_for` 对 `com.ddrun.search` 走 `cat.command`
                //    兜底），对该结果无信息量；③ 行高仍取 `m.row_h`（D8 40px /
                //    密度档同构），**不动几何契约**——两行方案已被
                //    `docs/icons-typography-plan.md` §5.3 明确不采纳。
                // 非文件结果：维持类别标签（「应用 / 网页 / 系统」等）不变。
                let file_item = is_file_item(item);
                let trail_full: Option<String> = if file_item {
                    // 只显示父目录——文件名已在标题，右列重复无意义；中段省略
                    // 后首段保留盘符、尾段保留最靠近文件的父目录，定位信息最大化。
                    Some(file_location(&item.subtitle).to_string())
                } else {
                    item.result_category.clone()
                };
                // 右列宽：路径上限 = 可用宽 × 45%（下限 120px，窄面板下不退化
                // 成空串）；类别标签沿用 90px 截断。
                let trail_cap = if file_item {
                    (ui.available_width() * FILE_PATH_W_FRACTION).max(FILE_PATH_MIN_W)
                } else {
                    CAT_LABEL_MAX_W
                };
                let trail_w = trail_full
                    .as_deref()
                    .map(|s| text_width(ui, s, font_cat.clone()).min(trail_cap))
                    .unwrap_or(0.0);
                // 标题可用宽 = 剩余宽 − 右列文本宽 − 12px 间隙。
                let title_avail = (ui.available_width()
                    - if trail_w > 0.0 { trail_w + TRAIL_GAP } else { 0.0 })
                .max(1.0);
                // 标题：占满中列，过长截断。**左对齐贴图标**——`add_sized` 默认
                // centered_and_justified 会把短标题居中（真机反馈 2026-09-12），
                // 改为 allocate_exact_size 预留中列矩形 + child Ui
                // left_to_right 布局（与页脚/引擎行同一惯用法）。
                let (title_box, _) = ui.allocate_exact_size(
                    egui::vec2(title_avail, m.title_pt + 2.0),
                    egui::Sense::hover(),
                );
                let mut title_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(title_box)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                title_ui.add(
                    egui::Label::new(egui::RichText::new(&item.title).size(m.title_pt)).truncate(),
                );
                // 右列：文件结果 = 所在文件夹路径（**中段省略**，无论文件名多长
                // 都可见）；其余 = 类别标签。贴右最右。
                if let Some(trail) = trail_full {
                    if trail_w > 0.0 {
                        let disp = if file_item {
                            middle_ellipsis(ui, &trail, &font_cat, trail_w)
                        } else {
                            trail
                        };
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_sized(
                                egui::vec2(trail_w, m.title_pt + 2.0),
                                egui::Label::new(
                                    egui::RichText::new(disp).size(m.cat_pt).color(p.text3),
                                )
                                .truncate(),
                            );
                        });
                    }
                }
                // 标题自然宽度超出可用宽 = 被截断（供行 tooltip 判定；
                // egui 0.36 无 Response::truncated()，用量宽比较）
                text_width(ui, &item.title, font_title) > title_avail
            })
            .inner
        });
    let frame_resp = framed.response;
    let title_truncated = framed.inner;

    // 选中指示条：行左缘 3px accent（`.row.selected::before`：left 0 / top-bottom 8 / 圆角 2）。
    // 画在 Frame 背景之后（x 0..3 区域无内容，不与文字重叠）。
    if selected {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(frame_resp.rect.left(), frame_resp.rect.top() + 8.0),
                egui::pos2(
                    frame_resp.rect.left() + theme::ACCENT_BAR_W,
                    frame_resp.rect.bottom() - 8.0,
                ),
            ),
            egui::CornerRadius::same(2),
            p.accent_stroke, // B2：线状小元素用 accent_stroke（暗色 brand[100]）
        );
    }

    let hit_resp = ui
        .interact(
            frame_resp.rect,
            ui.id().with(("hit", &item.id)),
            egui::Sense::click().union(egui::Sense::hover()),
        )
        // 行不是文本输入区：Label 悬停默认 I 形（Text）光标，整行改回箭头
        //（真机反馈 2026-09-12）。
        .on_hover_cursor(egui::CursorIcon::Default);
    // B5：信息不丢——整行 hover tooltip。标题被截断时给全文；副标题
    // （应用 = exe/lnk 路径）行内已不渲染，在此展示。两者都没有则不挂。
    // B7 修订：tooltip 改几何判定（on_hover_text 依赖的 hovered() 链路失效）。
    // uid 2026-09-14：文件结果的行内右列是**中段省略后的所在文件夹**，
    // tooltip 恒给**完整路径**（含文件名）——地址信息在任何面板宽度下都不丢。
    let tip = if is_file_item(item) {
        item.subtitle.clone()
    } else {
        match (title_truncated, item.subtitle.is_empty()) {
            (true, false) => format!("{}\n{}", item.title, item.subtitle),
            (true, true) => item.title.clone(),
            (false, false) => item.subtitle.clone(),
            (false, true) => String::new(),
        }
    };
    if tip.is_empty() {
        hit_resp
    } else {
        if ui.rect_contains_pointer(frame_resp.rect) {
            hit_resp.show_tooltip_text(tip);
        }
        hit_resp
    }
}

/// 是否文件搜索结果（驱动右列由「类别标签」改显「所在文件夹路径」）。
///
/// 判据 = `tags` 含 `"files"`（`dd-ext/src/bin/search.rs::to_command_item`
/// 下发，协议零改动）**且** `subtitle` 形如绝对路径。后者是必需的：文件搜索
/// 扩展的入口项 `files.search` 与 `files.search.query` 同样带 `"files"` 标签，
/// 但副标题是说明文案（「输入关键词搜索本地文件」）而非路径，不能误判。
fn is_file_item(item: &PanelItem) -> bool {
    item.tags.iter().any(|t| t == "files") && looks_like_path(&item.subtitle)
}

/// 粗糙的绝对路径判别：`C:\` / `C:/` 盘符绝对路径，或 `\\` UNC 前缀。
/// 只用于区分「路径副标题」与「说明文案副标题」，不追求完备。
fn looks_like_path(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() >= 3 && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')) || s.starts_with("\\\\")
}

/// 取路径的父目录部分（`C:\a\b\f.txt` → `C:\a\b`；`C:\f.txt` → `C:\`）。
/// 无可辨父目录（无分隔符、或分隔符在首位）时原样返回。
fn file_location(path: &str) -> &str {
    let Some(idx) = path.rfind(['\\', '/']) else {
        return path;
    };
    if idx == 0 {
        return path; // "\file"：无父目录可展示
    }
    let head = &path[..idx];
    // 裸盘符补回分隔符："C:" → "C:\"（idx 处必为分隔符，切片安全）
    if head.ends_with(':') {
        &path[..idx + 1]
    } else {
        head
    }
}

/// 路径中段省略（纯函数，可单测）：保留首尾、中间折叠 `…`。
/// `max_chars` 为可容纳字符上限（含省略号）；≤1 或原串更短时原样返回。
fn middle_ellipsis_chars(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if len <= max_chars || max_chars <= 1 {
        return text.to_string();
    }
    let keep = max_chars - 1; // 预留 1 个字符给省略号
    let head = keep / 2;
    let tail = keep - head;
    let mut s = String::with_capacity(max_chars + 1);
    for &c in &chars[..head] {
        s.push(c);
    }
    s.push('\u{2026}'); // …
    for &c in &chars[len - tail..] {
        s.push(c);
    }
    s
}

/// 宽度自适应中段省略：按平均字宽估算可容纳字符数，再实测收敛，保证不溢出
/// `max_w`。依赖字体测量（`text_width`），故需 `&egui::Ui`；调用点的
/// [`egui::Label::truncate`] 作为兜底（极端窄宽仍可能尾省略）。
fn middle_ellipsis(ui: &egui::Ui, text: &str, font: &egui::FontId, max_w: f32) -> String {
    if text.is_empty() || max_w <= 0.0 {
        return String::new();
    }
    let full_w = text_width(ui, text, font.clone());
    if full_w <= max_w {
        return text.to_string();
    }
    let n = text.chars().count();
    let avg = (full_w / n as f32).max(1.0);
    let mut max_chars = ((max_w / avg) as usize).max(6).min(n);
    let mut out = middle_ellipsis_chars(text, max_chars);
    // 平均字宽估计偏差时再缩减，直到不溢出（上限 16 次防呆）
    let mut guard = 0;
    while text_width(ui, &out, font.clone()) > max_w && max_chars > 6 && guard < 16 {
        max_chars = max_chars.saturating_sub(2).max(6);
        out = middle_ellipsis_chars(text, max_chars);
        guard += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_path_accepts_drive_and_unc() {
        assert!(looks_like_path("C:\\Users\\me\\a.txt"));
        assert!(looks_like_path("D:/work/a.txt"));
        assert!(looks_like_path("\\\\server\\share\\a.txt"));
        assert!(!looks_like_path("输入关键词搜索本地文件"));
        assert!(!looks_like_path(""), "空串不是路径");
        assert!(!looks_like_path("C:relative.txt"), "无分隔符的非绝对路径");
    }

    #[test]
    fn is_file_item_requires_files_tag_and_path_subtitle() {
        // 有路径副标题但无 files 标签（普通应用项）→ 不是文件结果
        let mut app = PanelItem::new("Visual Studio Code");
        app.subtitle = "C:\\Program Files\\Code\\Code.exe".into();
        assert!(!is_file_item(&app), "无 files 标签 → 不按文件结果渲染");

        // 有 files 标签但副标题是说明文案（入口项 files.search）→ 不能误判
        let mut entry = PanelItem::new("文件搜索");
        entry.tags.push("files".into());
        entry.subtitle = "输入关键词搜索本地文件（或输入 f 后空格直接进入）".into();
        assert!(!is_file_item(&entry), "说明文案副标题 → 不按文件结果渲染");

        // 两者齐备（真实文件结果 files.open.*）→ 文件结果
        let mut hit = PanelItem::new("report-2026.pdf");
        hit.tags.push("files".into());
        hit.subtitle = "C:\\Users\\me\\Documents\\report-2026.pdf".into();
        assert!(is_file_item(&hit), "files 标签 + 路径副标题 → 文件结果");
    }

    #[test]
    fn file_location_strips_file_name() {
        assert_eq!(
            file_location("C:\\Users\\me\\Documents\\report-2026.pdf"),
            "C:\\Users\\me\\Documents"
        );
        assert_eq!(file_location("C:\\f.txt"), "C:\\", "裸盘符补回分隔符");
        assert_eq!(
            file_location("\\\\server\\share\\f.txt"),
            "\\\\server\\share",
            "UNC 保留服务器与共享名"
        );
        assert_eq!(file_location("no-separator"), "no-separator", "无分隔符原样返回");
    }

    #[test]
    fn middle_ellipsis_chars_keeps_head_and_tail() {
        // "abcdef" 截断到 5 字符（含 1 省略号）→ 头 2 + … + 尾 2
        assert_eq!(middle_ellipsis_chars("abcdef", 5), "ab…ef");
        // 原串更短 / 上限 ≤1：原样返回
        assert_eq!(middle_ellipsis_chars("abc", 10), "abc");
        assert_eq!(middle_ellipsis_chars("abc", 1), "abc");
        // 盘符长路径：首段保留盘符与 Users、尾段保留最靠近文件的父目录
        let long = "C:\\Users\\Administrator\\Documents\\Projects\\very-long-folder-name";
        let out = middle_ellipsis_chars(long, 24);
        assert!(out.starts_with("C:\\Users"), "head 应保留盘符：|{out}|");
        assert!(
            out.ends_with("folder-name"),
            "tail 应保留最靠近文件的父目录：|{out}|"
        );
        assert!(out.contains('\u{2026}'), "应含中段省略号");
        assert_eq!(out.chars().count(), 24, "总长度应为 24（含 1 省略号）");
    }
}
