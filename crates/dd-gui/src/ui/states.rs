//! 列表空态/加载态绘制与辅助色计算。

use crate::ui::panel::SEARCH_GLYPH;
use crate::ui::settings_view::accent_soft;
use dd_gui::theme;
use eframe::egui;

/// egui 0.36 中 `weak_text_color` 是 `Option<Color32>`，取不到时退回文本色。
pub(crate) fn weak_text_color(ui: &egui::Ui) -> egui::Color32 {
    ui.visuals()
        .weak_text_color
        .unwrap_or_else(|| ui.visuals().text_color())
}

/// 单个列表项行（设计稿 01 ueli 式布局）：**图标 20px 列 + 名称 + 描述徽标
/// （名称右侧）+ Tag chips（贴右）**；40px 高（[`theme::ROW_H`]，D8）。
///
/// 行态（设计稿 CSS `.row`）：
/// - hover → [`theme::Palette::row_hover`] 填充；
/// - 选中 → [`theme::Palette::row_selected`] 填充 + 左侧 3px accent 指示条
///   （top/bottom 8px、圆角 2）；
/// - subtitle 从第二行改为**描述徽标**（badge 底、圆角 4、text-2 11px、限宽 220）；
/// - tags 为 pill chip（chip 底、text-3 10.5px）。
///
/// 返回整行可交互响应（hover + click 感知），供 `draw_list` 做
/// 「悬停高亮 / 单击执行」与「选中项滚入可视区」。
/// `icon` = [`PaletteApp::resolve_icons`] 预解析结果（`None` 项 = 空列占位对齐）。
///
/// 行背景在 Frame 之前按**预算行矩形**判定 hover（`ui.rect_contains_pointer`），
/// 避免依赖交互响应回读的帧延迟；预算矩形与实际分配矩形一致的前提是行前无
/// 其它布局消耗（ScrollArea 内逐行调用，成立）。
/// 纯空态（设计稿 02 屏 `.empty`）：搜索图标 glyph 30px/text-3 + 标题
/// 16px/600/text（base400）+ 可选描述 12px/text-3（caption1），居中、间距 10px。
pub(crate) fn draw_empty_state(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    title: &str,
    desc: Option<&str>,
) {
    ui.vertical_centered(|ui| {
        ui.add_space(34.0);
        ui.label(
            egui::RichText::new(SEARCH_GLYPH.to_string())
                .size(32.0)
                .color(p.text3),
        );
        ui.add_space(10.0);
        ui.label(egui::RichText::new(title).size(16.0).color(p.text));
        if let Some(desc) = desc {
            ui.add_space(2.0);
            ui.label(egui::RichText::new(desc).size(12.0).color(p.text3));
        }
    });
}

// ── Loading 态（设计稿 §07.2，C 组批次 C2）────────────────────────────

/// 骨架行三段宽度占行宽比例（行 `idx` 决定、确定性）。设计稿 §07 mockup：
/// 三行 name 34%（CSS 默认）/26%/30%，desc 20%（CSS 默认）/20%/14%。
pub(crate) fn skeleton_fractions(idx: usize) -> (f32, f32) {
    const NAME: [f32; 3] = [0.34, 0.26, 0.30];
    const DESC: [f32; 3] = [0.20, 0.20, 0.14];
    (NAME[idx % 3], DESC[idx % 3])
}

/// 两色按 `s`（0..1）线性插值。
pub(crate) fn lerp_color(a: egui::Color32, b: egui::Color32, s: f32) -> egui::Color32 {
    let mix = |x: u8, y: u8| -> u8 {
        (x as f32 + (y as f32 - x as f32) * s.clamp(0.0, 1.0)).round() as u8
    };
    egui::Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// shimmer 颜色：bg3（input_fill）↔ bg2（panel_2）按 1.4s 周期平滑往返
/// （§07.2「底色 bg3 ↔ bg2」）。`time` 秒。
pub(crate) fn shimmer_color(p: &theme::Palette, time: f64) -> egui::Color32 {
    let t = (time % 1.4) / 1.4;
    let s = (0.5 - 0.5 * (std::f64::consts::TAU * t).cos()) as f32;
    lerp_color(p.input_fill, p.panel_2, s)
}

/// Loading 态（§07.2）：Spinner（22×22、环宽 2.5px、accent 旋转弧 +
/// accent-soft 底环、0.9s/圈）+ caption「正在加载…」+ 3 条骨架行
/// （行高 [`theme::ROW_H`]：图标块 20×20 圆角 6 + 名称条 12px 高 +
/// 描述条 10px 高右对齐，shimmer 底色）。加载完成行高一致 → 无布局跳动（A3）。
///
/// 占位设计：不改拉取时序与超时语义（`TIMEOUT_GET_ITEMS` 不变）。
/// 动画由调用方 `request_repaint_after` 驱动（egui 按需重绘）。
pub(crate) fn draw_loading_state(
    ui: &mut egui::Ui,
    lang: dd_gui::settings::Lang,
    p: &theme::Palette,
    dark: bool,
    time: f64,
) {
    // ── Spinner + 文案（`.loading`：居中、gap 12） ──
    ui.vertical_centered(|ui| {
        ui.add_space(26.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
        let center = rect.center();
        let radius = 11.0 - 2.5 / 2.0; // 环中径（外径 22/2 - 环宽一半）
                                       // 底环：accent-soft 全圆
        ui.painter()
            .circle_stroke(center, radius, egui::Stroke::new(2.5, accent_soft(dark, p)));
        // 旋转弧：accent，90° 扇段，0.9s/圈（CSS @keyframes spin）
        let a0 = (time % 0.9) / 0.9 * std::f64::consts::TAU;
        let arc = std::f64::consts::FRAC_PI_2;
        const SEGS: usize = 20;
        let pts: Vec<egui::Pos2> = (0..=SEGS)
            .map(|i| {
                let a = a0 + arc * i as f64 / SEGS as f64;
                egui::pos2(
                    center.x + (radius as f64 * a.cos()) as f32,
                    center.y + (radius as f64 * a.sin()) as f32,
                )
            })
            .collect();
        ui.painter()
            .add(egui::Shape::line(pts, egui::Stroke::new(2.5, p.accent)));
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(crate::text::t(lang, "panel.loading"))
                .size(theme::FOOTER_FONT)
                .color(p.text3),
        );
    });

    // ── 3 条骨架行（`.skel-row`：min-height 40、padding 8 10 8 8、gap 12） ──
    for idx in 0..3usize {
        let (name_frac, desc_frac) = skeleton_fractions(idx);
        let c = shimmer_color(p, time + idx as f64 * 0.18); // 各行相位微错开
        let (row, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), theme::ROW_H),
            egui::Sense::hover(),
        );
        let cy = row.center().y;
        // 图标块 20×20 圆角 6
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(row.min.x + 8.0, cy - 10.0),
            egui::vec2(20.0, 20.0),
        );
        ui.painter()
            .rect_filled(icon_rect, egui::CornerRadius::same(6), c);
        // 名称条 12px 高
        let name_rect = egui::Rect::from_min_size(
            egui::pos2(icon_rect.right() + 12.0, cy - 6.0),
            egui::vec2(row.width() * name_frac, 12.0),
        );
        ui.painter()
            .rect_filled(name_rect, egui::CornerRadius::same(3), c);
        // 描述条 10px 高、右对齐（margin-left auto + padding-right 10）
        let desc_rect = egui::Rect::from_min_max(
            egui::pos2(row.right() - 10.0 - row.width() * desc_frac, cy - 5.0),
            egui::pos2(row.right() - 10.0, cy + 5.0),
        );
        ui.painter()
            .rect_filled(desc_rect, egui::CornerRadius::same(3), c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── C 组批次 C2：Loading 骨架（§07.2，验收 A3） ────────────────

    #[test]
    fn skeleton_fractions_are_deterministic_and_in_design_range() {
        // 设计稿 §07 mockup：name 34/26/30、desc 20/20/14
        let expected = [(0.34, 0.20), (0.26, 0.20), (0.30, 0.14)];
        for idx in 0..6usize {
            let (n, d) = skeleton_fractions(idx);
            assert_eq!((n, d), expected[idx % 3], "行 {idx} 宽度确定且符合设计稿");
        }
    }

    #[test]
    fn shimmer_color_oscillates_between_bg3_and_bg2() {
        let p = theme::Palette::dark();
        assert_eq!(
            shimmer_color(&p, 0.0),
            p.input_fill,
            "t=0 → bg3（input_fill）"
        );
        assert_eq!(
            shimmer_color(&p, 0.7),
            p.panel_2,
            "t=半周期 → bg2（panel_2）"
        );
        let mid = shimmer_color(&p, 0.35);
        assert_ne!(mid, p.input_fill, "中间相位 ≠ 两端色");
        assert_ne!(mid, p.panel_2, "中间相位 ≠ 两端色");
    }
}
