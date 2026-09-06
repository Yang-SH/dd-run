//! 通用绘制原语：键帽、键位组、齿轮、返回键等。

use dd_gui::theme;
use eframe::egui;

/// 页脚键位组：若干**键帽** + 说明文本（设计稿 01 `.panel-footer .keys`）。
pub(crate) struct KeyGroup {
    pub(crate) caps: &'static [&'static str],
    pub(crate) desc: &'static str,
}

/// `<b>↑↓</b> 选择` ｜ `<b>Enter</b> 执行` ｜ `<b>Esc</b> 返回·隐藏`
/// （v4.7 修订，真机反馈 2026-09-05：↑↓ 合并**单枚**键帽——两枚窄键帽各自
/// 四边描边在行内形成「|||」碎裂感；文案统一用 D15 文字规格的「返回·隐藏」）
pub(crate) const KEY_GROUPS: [KeyGroup; 3] = [
    KeyGroup {
        caps: &["↑↓"],
        desc: "选择",
    },
    KeyGroup {
        caps: &["Enter"],
        desc: "执行",
    },
    KeyGroup {
        caps: &["Esc"],
        desc: "返回·隐藏",
    },
];

/// 文本单行宽度（不改布局，只量尺寸）。
pub(crate) fn text_width(ui: &egui::Ui, text: &str, font_id: egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), font_id, egui::Color32::TRANSPARENT)
        .size()
        .x
}

/// 单个键帽宽度 = 文本宽 + 左右各 6px padding + 左右各 1px 描边。
pub(crate) fn keycap_width(ui: &egui::Ui, cap: &str) -> f32 {
    text_width(ui, cap, egui::FontId::monospace(theme::KEYCAP_FONT))
        + 2.0 * theme::KEYCAP_PAD_X
        + 2.0
}

/// 键位组宽度 = 各键帽 + 帽间 2px 间隙 + 4px + 说明文本宽。
pub(crate) fn key_group_width(ui: &egui::Ui, group: &KeyGroup) -> f32 {
    let caps: f32 = group.caps.iter().map(|c| keycap_width(ui, c)).sum();
    let gaps = theme::KEYCAP_GAP * group.caps.len().saturating_sub(1) as f32;
    caps + gaps
        + 4.0
        + text_width(
            ui,
            group.desc,
            egui::FontId::proportional(theme::FOOTER_FONT),
        )
}

/// 键位区总宽（组间 `FOOTER_GAP` 14px）。
pub(crate) fn keys_width(ui: &egui::Ui) -> f32 {
    let sum: f32 = KEY_GROUPS.iter().map(|g| key_group_width(ui, g)).sum();
    sum + theme::FOOTER_GAP * (KEY_GROUPS.len() - 1) as f32
}

/// 页脚最左**设置按钮**（设计稿 §6.1 line 970）：齿轮 `\u{E713}`、16px、
/// 颜色恒为 `--text-2`（fg2 = 暗 #d6d6d6 / 亮 #424242，**hover 不变色**）、
/// hover 加 `bg1Hover` 圆角背景、24×24 热区。返回是否被点击。
///
/// glyph 经图标字体渲染（`SegoeIcons.ttf` → `segmdl2.ttf` 已在字体回退链，
/// 与列表行图标同链路）；键盘可达经 `Ctrl+,` 快捷键（批次 4.0 决策）。
pub(crate) fn draw_settings_gear(ui: &mut egui::Ui, p: &theme::Palette) -> bool {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(theme::GEAR_SIZE, theme::GEAR_SIZE),
        egui::Sense::click(),
    );
    // hover 反馈按设计稿 §6.1 line 970："hover 不变色、仅加深背景 = bg1Hover"——
    // 圆角 4 圆角矩形底，圆心不变，glyph 始终 text2。
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(4), p.row_hover);
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        theme::GEAR_GLYPH,
        egui::FontId::proportional(theme::GEAR_FONT),
        p.text2,
    );
    resp.clicked()
}

/// 设置页顶行返回按钮（设计稿 §07.1 line 1121 + §08.1）：28×28 热区、
/// ChevronLeft `\u{E72B}` 16px、颜色恒为 `--text-2`、hover 加 `bg1Hover`
/// 圆角 4 背景。返回是否被点击。
///
/// 与 `draw_settings_gear` 同链路（统一字体与 token 取色）。
pub(crate) fn draw_back_btn(ui: &mut egui::Ui, p: &theme::Palette) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect.shrink(2.0), egui::CornerRadius::same(4), p.row_hover);
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        '\u{E72B}',
        egui::FontId::proportional(16.0),
        p.text2,
    );
    resp.clicked()
}

// ── 现有页脚键位区 ────────────────────────────────────────────────

/// 在 `(x, cy)` 起点向右**手绘**完整键位图例（[`KEY_GROUPS`]），所有键帽与
/// 说明文本统一垂直锚定 `cy`——egui 自动居中在混合高度内容下逐项漂移
/// （真机 2026-09-04 修复"底栏不在一条线上"），页脚键位区弃用光标布局。
///
/// 宽度推进与 [`keys_width`] / [`key_group_width`] 的口径完全一致
/// （键帽间 `KEYCAP_GAP`、键帽与说明间 4px、组间 `FOOTER_GAP`），
/// 保证 `split_x = row.right() - keys_width()` 后恰好从 `split_x` 起排。
pub(crate) fn paint_keys_at(ui: &mut egui::Ui, origin: egui::Pos2, p: &theme::Palette) {
    let font = egui::FontId::proportional(theme::FOOTER_FONT);
    let mut x = origin.x;
    for (i, group) in KEY_GROUPS.iter().enumerate() {
        if i > 0 {
            x += theme::FOOTER_GAP;
        }
        for (j, cap) in group.caps.iter().enumerate() {
            if j > 0 {
                x += theme::KEYCAP_GAP;
            }
            let kw = keycap_width(ui, cap);
            let krect = egui::Rect::from_min_size(
                egui::pos2(x, origin.y - theme::KEYCAP_H / 2.0),
                egui::vec2(kw, theme::KEYCAP_H),
            );
            paint_keycap(ui, krect, cap, p);
            x += kw;
        }
        x += 4.0;
        ui.painter().text(
            egui::pos2(x, origin.y),
            egui::Align2::LEFT_CENTER,
            group.desc,
            font.clone(),
            p.text3,
        );
        x += text_width(ui, group.desc, font.clone());
    }
}

/// 单个键帽（设计稿 `.panel-footer b`）：chip 底 + 1px 描边 + 圆角 4 +
/// monospace 10px + 左右 6px padding。
///
/// v4.7 修订（真机反馈两轮）：
/// ① 描边 `--border` → **`--border-strong`**——浅色主题下 #e0e0e0 键帽贴
/// #fafafa 页脚几乎不可见，提一档后键帽轮廓清晰（与设计稿 CSS 同步修订）；
/// ② **下边线统一 1px**——原设计的 2px 下边条（keycap 立体感）在 16px 高的
/// 小键帽上明显重于三边 1px 描边（用户反馈"下划线比其他方向粗"），扁平化
/// 为四边等宽；原"下边 2px 画入边框盒内部"的 0.5px 悬空线修复随之失效删除。
///
/// 绘制核心 [`paint_keycap`] 接收**显式矩形**：egui `Frame` 不支持按边设不同
/// 描边宽度只能手画，且页脚混合高度内容需手动锚定中心线（见 `paint_keys_at`），
/// 故不再提供光标布局版包装。
pub(crate) fn paint_keycap(ui: &mut egui::Ui, rect: egui::Rect, cap: &str, p: &theme::Palette) {
    let r = rect.shrink(0.5); // 给 1px 描边留位置
    ui.painter()
        .rect_filled(r, egui::CornerRadius::same(4), p.chip_bg);
    ui.painter().rect_stroke(
        r,
        egui::CornerRadius::same(4),
        egui::Stroke::new(1.0, p.border_strong),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        cap,
        egui::FontId::monospace(theme::KEYCAP_FONT),
        p.text2,
    );
}
