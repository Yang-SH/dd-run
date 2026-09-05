//! Toast 浮层绘制。

use crate::app::PaletteApp;
use crate::ui::widgets::text_width;
use dd_gui::theme;
use eframe::egui;

impl PaletteApp {
    /// Toast 提示条（悬浮于面板底部居中）。
    /// 必须画在独立 `Area`（ctx 层）：若在 `CentralPanel` 之后追加到根 `Ui`，
    /// 布局会落到面板矩形之外被裁剪——真机表现为「Toast 永远不显示」
    /// （M2 真机反馈 #1/#5/#6/#8 的共同根因）。
    ///
    /// C 组批次 C3（§09）：意图图标（success/error/info 语义色，16px）+
    /// caption1 12/16 单行文本。同一时刻至多 1 条（单槽语义不变）。
    pub(crate) fn draw_toast(&self, ctx: &egui::Context) {
        let Some(toast) = &self.toast else {
            return;
        };
        egui::Area::new(egui::Id::new("dd-gui-toast"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -48.0])
            .show(ctx, |ui| {
                // 锚定 Area 初始可用宽度为 0：短消息（`= 2`）看不出异常，长消息
                // （shell 输出摘要）会被按 ~1 字符换行成竖条（真机反馈）。先按
                // 12px 字体实测单行宽度（+意图图标 16px + 间隙 6px），限宽
                // [250, 420]（§9.1 最小宽 250px）再渲染——长消息在 420px 内
                // 换行，整体仍由 CENTER_BOTTOM 锚点水平居中。
                let w =
                    (16.0 + 6.0 + text_width(ui, &toast.message, egui::FontId::proportional(12.0)))
                        .clamp(250.0, 420.0);
                ui.set_max_width(w);
                // 设计稿 09：elevated card 表面 —— card 底 + 1px stroke2 描边 +
                // 圆角 8 + shadow16（D10）；padding 8px 12px；caption1 12/16。
                let p = theme::Palette::of(ui.visuals().dark_mode);
                egui::Frame::default()
                    .fill(p.card)
                    .stroke(egui::Stroke::new(1.0, p.border))
                    .corner_radius(8.0)
                    .shadow(theme::toast_shadow(ui.visuals().dark_mode))
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            ui.label(
                                egui::RichText::new(toast.kind.glyph().to_string())
                                    .size(16.0)
                                    .color(toast.kind.color(&p)),
                            );
                            ui.label(egui::RichText::new(&toast.message).size(12.0).color(p.text));
                        });
                    });
            });
    }
}
