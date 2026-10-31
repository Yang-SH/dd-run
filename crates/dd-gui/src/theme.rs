//! UI 视觉主题层：设计稿 v4 token → egui `Style`/`Visuals`。
//!
//! 契约来源：`cmdpal-ui-mockups.html`（v4，Fluent 2 token 驱动）05「alias token
//! 映射表」+ 05.1 字号 ramp + CSS 组件派生值（searchbar / Tag / 键帽 / 页脚状态点）。
//! 亮暗两套视觉分别注册到 egui（`set_visuals_of(Theme::Dark|Light)`），主题偏好
//! `System` 跟随系统；无系统主题信息时 egui 回落暗色（设计稿 05 note「默认暗色」）。
//!
//! 语义：本模块是**唯一 token 源**——绘制层不写裸色值（`Visuals` 覆盖不了的
//! 场景：行 hover/选中填充、左侧 accent 指示条、Tag/键帽底、搜索框聚焦下划线、
//! 页脚状态点等，统一经 [`Palette`] 取色）。

// 本 crate 未直接依赖 `egui`（bin 层经 `eframe::egui` re-export 使用）；
// lib 层统一走同一路径，避免重复声明 egui 依赖。
//
// 批次 C（refactor-structure-2026-10-21.md §4.3）目录化：tokens / palette / backdrop /
// visuals 四子模块按主题切分，公开项经 `pub use` 桥再导出，调用方的既有路径保持
// 不变（批次 E 起 lib 内自引用统一写 `crate::`）；tests 原位保留在本文件。
mod backdrop;
mod palette;
mod tokens;
mod visuals;

pub use backdrop::{
    backdrop_config, card_fill, panel_tint_cap, panel_tint_with_opacity, row_fills,
    BackdropStyleConfig, RowFillProfile, RowFills, CARD_GLASS_ALPHA_DARK, CARD_GLASS_ALPHA_LIGHT,
    TINT_CAP_ACRYLIC, TINT_CAP_MICA,
};
pub use palette::{
    dialog_shadow, menu_shadow, overlay, toast_shadow, Palette, CTX_ANCHOR_OFFSET, CTX_ICON,
    CTX_ITEM_GAP, CTX_ITEM_H, CTX_ITEM_PAD_X, CTX_MENU_MARGIN, CTX_MENU_MIN_W, CTX_MENU_PAD,
    CTX_SEP_H, CTX_SEP_INSET,
};
pub use tokens::{
    semibold, semibold_title, ListMetrics, ACCENT_BAR_W, DOT_GAP, DOT_SIZE, FOOTER_FONT,
    FOOTER_GAP, FOOTER_PAD_X, FOOTER_PAD_Y, GEAR_FONT, GEAR_GLYPH, GEAR_SIZE, KEYCAP_DESC_GAP,
    KEYCAP_FONT, KEYCAP_GAP, KEYCAP_H, KEYCAP_PAD_X, KEYCAP_RADIUS, LIST_CAT_PT, LIST_GLYPH_PT,
    LIST_ICON_CELL, LIST_ICON_GAP, LIST_TITLE_PT, ROW_H, ROW_RADIUS, SEARCHBAR_H, SEARCHBAR_RADIUS,
    SEMIBOLD_FAMILY,
};
pub use visuals::{apply, apply_panel_tint, theme_preference, tint_color, visuals, Colorization};
#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Color32, CornerRadius, ThemePreference};

    use super::visuals::{mix_panel, rgb, tint_mix};

    /// 05 表 parity 守卫（v4 Fluent 2 alias token）：十六进制值必须与设计稿
    /// 一致（防手滑改色）。
    #[test]
    fn dark_palette_matches_design_tokens() {
        let p = Palette::dark();
        assert_eq!(p.panel, rgb(0x29_29_29), "--panel 暗 = bg1 grey[16]");
        assert_eq!(p.panel_2, rgb(0x1f_1f_1f), "--panel-2 暗 = bg2 grey[12]");
        assert_eq!(
            p.border_strong,
            rgb(0x66_66_66),
            "--border-strong 暗 = stroke1 grey[40]"
        );
        assert_eq!(p.border, rgb(0x52_52_52), "--border 暗 = stroke2 grey[32]");
        assert_eq!(p.text, rgb(0xff_ff_ff), "--text 暗 = fg1");
        assert_eq!(p.text2, rgb(0xd6_d6_d6), "--text-2 暗 = fg2 grey[84]");
        assert_eq!(p.text3, rgb(0xad_ad_ad), "--text-3 暗 = fg3 grey[68]");
        assert_eq!(p.text4, rgb(0x99_99_99), "--text-4 暗 = fg4 grey[60]");
        assert_eq!(
            p.text_disabled,
            rgb(0x5c_5c_5c),
            "--text-disabled 暗 = fgDisabled grey[36]"
        );
        assert_eq!(
            p.card,
            rgb(0x33_33_33),
            "--card 暗 = cardBackground grey[20]"
        );
        assert_eq!(
            p.card_hover,
            rgb(0x3d_3d_3d),
            "--card-hover 暗 = cardHover grey[24]（卡面 grey[20] 同族更亮一档）"
        );
        assert_eq!(
            p.accent,
            rgb(0x11_5e_a3),
            "--accent 暗 = brand[70]（colorBrandBackground）"
        );
        assert_eq!(
            p.accent_hover,
            rgb(0x28_86_de),
            "--accent-hover 暗 = brand[80]（colorBrandBackgroundHover）"
        );
        assert_eq!(
            p.accent_stroke,
            rgb(0x47_9e_f5),
            "--accent-stroke 暗 = brand[100]（colorBrandStroke1）"
        );
        assert_eq!(
            p.control_hover,
            rgb(0x3d_3d_3d),
            "--control-hover 暗 = row_hover（bg1Hover）"
        );
        assert_eq!(p.success, rgb(0x54_b0_54), "--success 暗 = green[tint30]");
        assert_eq!(p.danger, rgb(0xdc_62_6d), "--danger 暗 = cranberry[tint30]");
        assert_eq!(
            p.row_hover,
            rgb(0x3d_3d_3d),
            "--row-hover 暗 = bg1Hover grey[24]"
        );
        assert_eq!(
            p.row_selected,
            rgb(0x38_38_38),
            "--row-selected 暗 = bg1Selected grey[22]"
        );
        assert_eq!(
            p.row_pressed,
            rgb(0x1f_1f_1f),
            "--row-pressed 暗 = bg1Pressed grey[12]"
        );
        assert_eq!(
            p.input_fill,
            rgb(0x14_14_14),
            "--input-fill 暗 = bg3 grey[8]"
        );
        assert_eq!(p.chip_bg, rgb(0x33_33_33), "--chip-bg 暗 = card grey[20]");
    }

    #[test]
    fn light_palette_matches_design_tokens() {
        let p = Palette::light();
        assert_eq!(p.panel, rgb(0xff_ff_ff), "--panel 亮 = bg1 white");
        assert_eq!(p.panel_2, rgb(0xfa_fa_fa), "--panel-2 亮 = bg2 grey[98]");
        assert_eq!(
            p.border_strong,
            rgb(0xd1_d1_d1),
            "--border-strong 亮 = stroke1 grey[82]"
        );
        assert_eq!(p.border, rgb(0xe0_e0_e0), "--border 亮 = stroke2 grey[88]");
        assert_eq!(p.text, rgb(0x24_24_24), "--text 亮 = fg1 grey[14]");
        assert_eq!(p.text2, rgb(0x42_42_42), "--text-2 亮 = fg2 grey[26]");
        assert_eq!(p.text3, rgb(0x61_61_61), "--text-3 亮 = fg3 grey[38]");
        assert_eq!(p.text4, rgb(0x70_70_70), "--text-4 亮 = fg4 grey[44]");
        assert_eq!(
            p.text_disabled,
            rgb(0xbd_bd_bd),
            "--text-disabled 亮 = fgDisabled grey[74]"
        );
        assert_eq!(
            p.card,
            rgb(0xfa_fa_fa),
            "--card 亮 = cardBackground grey[98]"
        );
        assert_eq!(
            p.card_hover,
            rgb(0xf5_f5_f5),
            "--card-hover 亮 = cardHover grey[96]（卡面 grey[98] 同族更灰一档）"
        );
        assert_eq!(p.accent, rgb(0x0f_6c_bd), "--accent 亮 = brand[80]");
        assert_eq!(
            p.accent_hover,
            rgb(0x11_5e_a3),
            "--accent-hover 亮 = brand[70]（colorBrandBackgroundHover）"
        );
        assert_eq!(
            p.accent_stroke,
            rgb(0x0f_6c_bd),
            "--accent-stroke 亮 = brand[80]（colorBrandStroke1，= 原 --accent 值）"
        );
        assert_eq!(
            p.control_hover,
            rgb(0xf0_f0_f0),
            "--control-hover 亮 = grey[94]（可辨且区别于 selected #ebebeb）"
        );
        assert_eq!(p.success, rgb(0x0e_70_0e), "--success 亮 = green[shade10]");
        assert_eq!(
            p.danger,
            rgb(0xb1_0e_1c),
            "--danger 亮 = cranberry[shade10]"
        );
        assert_eq!(
            p.row_hover,
            rgb(0xf5_f5_f5),
            "--row-hover 亮 = bg1Hover grey[96]"
        );
        assert_eq!(
            p.row_selected,
            rgb(0xeb_eb_eb),
            "--row-selected 亮 = bg1Selected grey[92]"
        );
        assert_eq!(
            p.row_pressed,
            rgb(0xe0_e0_e0),
            "--row-pressed 亮 = bg1Pressed grey[88]"
        );
        assert_eq!(
            p.input_fill,
            rgb(0xf5_f5_f5),
            "--input-fill 亮 = bg3 grey[96]"
        );
        assert_eq!(p.chip_bg, rgb(0xf5_f5_f5), "--chip-bg 亮 = bg3 grey[96]");
    }

    /// 页脚底必须与面板底不同（设计稿 `--panel-2` ≠ `--panel`），
    /// 否则页脚与列表区糊成一片、失去底部区隔。
    #[test]
    fn footer_fill_differs_from_panel_in_both_themes() {
        for dark in [true, false] {
            let p = Palette::of(dark);
            assert_ne!(
                p.panel_2,
                p.panel,
                "{} 主题：页脚底 --panel-2 应区别于面板底 --panel",
                if dark { "暗" } else { "亮" }
            );
        }
    }

    /// v4 D11：暗色行态改为 Fluent 实色（bg1Hover/bg1Selected），
    /// 不再是 v2/v3 的半透明白叠加（实色在滚动内容上不会透出下层文字）。
    #[test]
    fn dark_row_states_are_solid() {
        let p = Palette::dark();
        assert_eq!(p.row_hover.a(), 255, "暗色 hover 为实色");
        assert_eq!(p.row_selected.a(), 255, "暗色 selected 为实色");
        assert_ne!(p.row_hover, p.row_selected, "hover 与 selected 可区分");
    }

    /// 行 hover 玻璃色（亚克力下结果列表用）：保留 row_hover 色相，仅 alpha=80，
    /// RGB 与 row_hover 一致仅 alpha 不同（其他用 row_hover 的场景不受牵连）。
    #[test]
    fn row_hover_glass_is_translucent_and_keeps_hue() {
        for dark in [true, false] {
            let p = Palette::of(dark);
            // 期望值直接用 from_rgba_unmultiplied 构造（与 glass helper 同实现路径），
            // 全字段相等比较避免 Color32 内部预乘语义差异
            let (v, name) = if dark {
                (0x3d_3d_3d, "暗")
            } else {
                (0xf5_f5_f5, "亮")
            };
            let expected = Color32::from_rgba_unmultiplied(
                ((v >> 16) & 0xff) as u8,
                ((v >> 8) & 0xff) as u8,
                (v & 0xff) as u8,
                80,
            );
            assert_eq!(
                p.row_hover_glass,
                expected,
                "{} 主题：glass = rgb({:02x},{:02x},{:02x},80)",
                name,
                (v >> 16) & 0xff,
                (v >> 8) & 0xff,
                v & 0xff
            );
            assert_eq!(
                p.row_hover,
                rgb(v),
                "{} 主题：row_hover = rgb({:02x},{:02x},{:02x}) 实色",
                name,
                (v >> 16) & 0xff,
                (v >> 8) & 0xff,
                v & 0xff
            );
            assert_eq!(
                p.row_hover_glass.a(),
                80,
                "{} 主题：glass alpha=80（亚克力下通透）",
                name
            );
        }
    }

    /// 卡片表面（Toast/设置卡/按钮底）与面板底可区分（暗色 card grey[20]
    /// 比面板 grey[16] 亮一档；亮色 card grey[98] 比面板 white 灰一档）。
    #[test]
    fn card_differs_from_panel_in_both_themes() {
        for dark in [true, false] {
            let p = Palette::of(dark);
            assert_ne!(
                p.card,
                p.panel,
                "{} 主题：card 应区别于面板底",
                if dark { "暗" } else { "亮" }
            );
        }
    }

    #[test]
    fn visuals_reflect_palette_of_theme() {
        for dark in [true, false] {
            let v = visuals(dark, None, Colorization::default());
            let p = Palette::of(dark);
            assert_eq!(v.dark_mode, dark, "dark_mode 标志随主题");
            assert_eq!(v.panel_fill, p.panel, "panel_fill = --panel");
            assert_eq!(v.override_text_color, Some(p.text), "主文本 = --text");
            assert_eq!(v.weak_text_color, Some(p.text2), "weak 文本 = --text-2");
            assert_eq!(v.hyperlink_color, p.accent, "链接 = --accent");
            assert_eq!(v.error_fg_color, p.danger, "错误色 = --danger");
            assert_eq!(
                v.window_corner_radius,
                CornerRadius::same(8),
                "窗口圆角 8px（几何 note）"
            );
        }
    }

    /// v4.7 D31 + M1（2026-09-13）+ P2 v3/v5：`None` = 不透明面板底；材质档 =
    /// 浓淡基色半透明层（只调 alpha，RGB 不变；cap 按材质两主题同档），其余
    /// 字段两路径一致，亮暗两套相同。
    #[test]
    fn panel_tint_visuals_only_affect_panel_fill() {
        use crate::settings::Backdrop;
        for dark in [true, false] {
            let p = Palette::of(dark);
            let opaque = visuals(dark, None, Colorization::default());
            assert_eq!(opaque.panel_fill, p.panel, "回退路径（无材质）= 实色面板底");
            for backdrop in [Backdrop::Mica, Backdrop::Acrylic] {
                let a = panel_tint_cap(dark, backdrop).expect("材质档必有浓淡层");
                let tinted = visuals(dark, Some(a), Colorization::default());
                // P2 v3：浓淡基色 = tint_color（面板色 × 系统强调色混合，DeskBox
                // 配方）；非 Windows/取色失败时 = p.panel（本断言经 tint_color
                // 取期望值，两平台口径一致）。
                let base = tint_color(dark, Colorization::default());
                assert_eq!(
                    tinted.panel_fill,
                    Color32::from_rgba_unmultiplied(
                        base.r(),
                        base.g(),
                        base.b(),
                        (a * 255.0).round() as u8
                    ),
                    "材质档 panel_fill = 浓淡基色 × alpha（RGB 不变）"
                );
                // 其余表面不透明层级不变（行/卡片经 Palette 取用，不在 Visuals 内）
                assert_eq!(tinted.extreme_bg_color, opaque.extreme_bg_color);
                assert_eq!(tinted.window_fill, opaque.window_fill);
                assert_eq!(tinted.dark_mode, opaque.dark_mode);
            }
        }
        // v5：cap 按材质两主题同档（亮色低档已废——带色基色无白漆问题）
        assert_eq!(
            panel_tint_cap(false, Backdrop::Mica),
            panel_tint_cap(true, Backdrop::Mica),
            "云母 cap 两主题一致"
        );
        assert_eq!(
            panel_tint_cap(false, Backdrop::Acrylic),
            panel_tint_cap(true, Backdrop::Acrylic),
            "亚克力 cap 两主题一致"
        );
    }

    /// P2 v2/v5（2026-09-13 真机反馈迭代）：不透明度直控式换算——默认 40% 档
    /// = 真机对比调定推荐观感（云母 0.30 / 亚克力 0.40，两主题同档）；0% =
    /// 纯材质；100% = cap；随 pct 单调不减（越界 clamp）；无材质恒 None。
    #[test]
    fn panel_tint_with_opacity_direct_control() {
        use crate::settings::Backdrop;
        // 默认 40% = 推荐观感锚点（±1e-6：1.0×40/100 存在浮点舍入）
        let anchors = [
            (true, Backdrop::Mica, 0.30_f32),
            (true, Backdrop::Acrylic, 0.40_f32),
            (false, Backdrop::Mica, 0.30_f32),
            (false, Backdrop::Acrylic, 0.40_f32),
        ];
        for (dark, backdrop, anchor) in anchors {
            let a = panel_tint_with_opacity(dark, backdrop, 40).expect("材质档必有浓淡层");
            assert!(
                (a - anchor).abs() < 1e-6,
                "默认 40% 必须复现推荐观感锚点 {anchor}（实际 {a}，dark={dark} {backdrop:?}）"
            );
        }
        // 0% = 纯材质（浓淡层全透）；100% = cap 上限（拉满即面板最实）
        assert_eq!(panel_tint_with_opacity(true, Backdrop::Mica, 0), Some(0.0));
        assert_eq!(
            panel_tint_with_opacity(true, Backdrop::Acrylic, 100),
            panel_tint_cap(true, Backdrop::Acrylic)
        );
        assert_eq!(
            panel_tint_with_opacity(false, Backdrop::Mica, 100),
            panel_tint_cap(false, Backdrop::Mica)
        );
        // 单调不减；超界 u8（200 > 100）视同 100
        let mut prev = -1.0_f32;
        for pct in [0u8, 10, 25, 40, 60, 80, 100, 200] {
            let a = panel_tint_with_opacity(true, Backdrop::Mica, pct).unwrap();
            assert!(a >= prev, "alpha 随 pct 单调不减（pct={pct}）");
            prev = a;
        }
        assert_eq!(
            panel_tint_with_opacity(true, Backdrop::Mica, 200),
            panel_tint_cap(true, Backdrop::Mica),
            "pct > 100 兜底视同 100"
        );
        // cap 排序（材质语义：亚克力拉满可全实、云母拉满保留采色语义）——本条
        // 即真机反馈「调透明度不明显」的修复锚点
        const { assert!(TINT_CAP_MICA >= 0.75 && TINT_CAP_ACRYLIC > TINT_CAP_MICA) };
        // 无材质恒 None（滑杆在该档置灰，换算层同口径）
        assert_eq!(panel_tint_with_opacity(true, Backdrop::None, 50), None);
    }

    /// P2 v5（2026-09-13 真机反馈）：行填充材质适配——hover 分档（云母加权、
    /// 亚克力减重）；selected 同走玻璃（比 hover 重一档，消除扫动时「不透明块
    /// 跳变」的闪烁）；回退 = row_hover/row_selected 实色不变。
    #[test]
    fn row_fills_are_material_aware() {
        use crate::settings::Backdrop;
        let p_light = Palette::of(false);
        let p_dark = Palette::of(true);
        // 回退路径 = row_hover / row_selected 实色（既有观感不变）
        for backdrop in [Backdrop::None, Backdrop::Mica, Backdrop::Acrylic] {
            let f = row_fills(true, backdrop, false);
            assert_eq!(f.hover, p_dark.row_hover);
            assert_eq!(f.selected, p_dark.row_selected);
            let f = row_fills(false, backdrop, false);
            assert_eq!(f.hover, p_light.row_hover);
            assert_eq!(f.selected, p_light.row_selected);
        }
        let f = row_fills(true, Backdrop::None, true);
        assert_eq!(f.hover, p_dark.row_hover);
        assert_eq!(f.selected, p_dark.row_selected);
        // 材质档 = 半透明玻璃（alpha < 255），且 selected 比 hover 重一档
        let lm = row_fills(false, Backdrop::Mica, true);
        let la = row_fills(false, Backdrop::Acrylic, true);
        let dm = row_fills(true, Backdrop::Mica, true);
        let da = row_fills(true, Backdrop::Acrylic, true);
        for f in [&lm, &la, &dm, &da] {
            assert!(
                f.hover.a() < 255 && f.selected.a() < 255,
                "材质档必须全玻璃"
            );
            assert!(
                f.selected.a() > f.hover.a(),
                "selected 必须重于 hover（平滑过渡）"
            );
        }
        // 云母档强于亚克力档
        assert!(
            lm.hover.a() > la.hover.a(),
            "亮色：云母 hover > 亚克力 hover"
        );
        assert!(
            dm.hover.a() > da.hover.a(),
            "暗色：云母 hover > 亚克力 hover"
        );
        // 定值锚点（亮·云母 黑 7%/10%、暗·云母 白 7.8%/10.2%、亚克力减半）
        assert_eq!(lm.hover, Color32::from_rgba_unmultiplied(0, 0, 0, 18));
        assert_eq!(lm.selected, Color32::from_rgba_unmultiplied(0, 0, 0, 26));
        assert_eq!(dm.hover, Color32::from_rgba_unmultiplied(255, 255, 255, 20));
        assert_eq!(
            dm.selected,
            Color32::from_rgba_unmultiplied(255, 255, 255, 26)
        );
        assert_eq!(la.hover, Color32::from_rgba_unmultiplied(0, 0, 0, 9));
        assert_eq!(la.selected, Color32::from_rgba_unmultiplied(0, 0, 0, 15));
        assert_eq!(da.hover, Color32::from_rgba_unmultiplied(255, 255, 255, 10));
        assert_eq!(
            da.selected,
            Color32::from_rgba_unmultiplied(255, 255, 255, 15)
        );
    }

    /// 设置卡填充（2026-09-13）：非玻璃 = 实色 card；玻璃 = card 色 × alpha
    /// （预乘存储，材质从卡面透出），亮暗两主题一致。
    #[test]
    fn card_fill_glass_is_translucent_only_when_asked() {
        for dark in [true, false] {
            let p = Palette::of(dark);
            assert_eq!(card_fill(dark, false), p.card, "回退路径 = 实色卡");
            let expected_a = if dark {
                CARD_GLASS_ALPHA_DARK
            } else {
                CARD_GLASS_ALPHA_LIGHT
            };
            assert_eq!(
                card_fill(dark, true),
                Color32::from_rgba_unmultiplied(p.card.r(), p.card.g(), p.card.b(), expected_a),
                "玻璃卡 = card 色 × alpha"
            );
            assert!(expected_a < 255, "玻璃卡必须降 alpha（材质透出）");
        }
    }

    #[test]
    fn light_and_dark_palettes_differ_in_accent() {
        // 两主题 accent 不同（Fluent 品牌色阶）；同值说明写错表了
        assert_ne!(Palette::dark().accent, Palette::light().accent);
    }

    /// D10 阴影分级：Toast shadow16 的 key 层不透明度暗 28% / 亮 14%。
    #[test]
    fn toast_shadow_follows_elevation_opacities() {
        assert_eq!(
            toast_shadow(true).color.a(),
            (0.28f32 * 255.0).round() as u8
        );
        assert_eq!(
            toast_shadow(false).color.a(),
            (0.14f32 * 255.0).round() as u8
        );
    }

    /// C 组批次 C3（§10.1）：遮罩 blackAlpha[50]（暗）/ [40]（亮）。
    #[test]
    fn overlay_matches_fluent_black_alpha() {
        assert_eq!(
            overlay(true),
            Color32::from_black_alpha(128),
            "暗 = blackAlpha[50]"
        );
        assert_eq!(
            overlay(false),
            Color32::from_black_alpha(102),
            "亮 = blackAlpha[40]"
        );
    }

    /// C 组批次 C3（§10.1）：对话框 shadow64——offset (0,32) blur 64，
    /// key 层不透明度暗 28% / 亮 24%。
    #[test]
    fn dialog_shadow_follows_elevation_opacities() {
        let s = dialog_shadow(true);
        assert_eq!(s.offset, [0, 32]);
        assert_eq!(s.blur, 64);
        assert_eq!(s.color.a(), (0.28f32 * 255.0).round() as u8);
        assert_eq!(
            dialog_shadow(false).color.a(),
            (0.24f32 * 255.0).round() as u8
        );
    }

    /// v4.4（10B.1）：右键菜单 shadow8——offset (0,4) blur 8，key 层
    /// 不透明度暗 28% / 亮 14%（与官方 elevation 低层 ramp 一致）。
    #[test]
    fn menu_shadow_follows_elevation_opacities() {
        let s = menu_shadow(true);
        assert_eq!(s.offset, [0, 4]);
        assert_eq!(s.blur, 8);
        assert_eq!(s.color.a(), (0.28f32 * 255.0).round() as u8);
        assert_eq!(
            menu_shadow(false).color.a(),
            (0.14f32 * 255.0).round() as u8
        );
    }

    /// v4.4（10B.1）几何常量：min-width 200 / 项高 32 / 容器 padding 4 /
    /// 分隔线 1+4+4=9 / 面板内夹紧边距 8（D20）。
    #[test]
    fn context_menu_geometry_matches_design_10b() {
        assert_eq!(CTX_MENU_MIN_W, 200.0, "容器 min-width 200");
        assert_eq!(CTX_ITEM_H, 32.0, "菜单项高 32");
        assert_eq!(CTX_MENU_PAD, 4.0, "容器 padding 4");
        assert_eq!(CTX_ITEM_PAD_X, 10.0, "菜单项 padding 0 10");
        assert_eq!(CTX_ICON, 16.0, "图标 16px");
        assert_eq!(CTX_SEP_H, 9.0, "分隔线 = 1px + 上下 4px margin");
        assert_eq!(CTX_SEP_INSET, 8.0, "分隔线水平内缩 8");
        assert_eq!(CTX_MENU_MARGIN, 8.0, "面板内夹紧边距 8（D20）");
        assert_eq!(CTX_ANCHOR_OFFSET, 2.0, "指针锚点偏移 2,2（D20）");
    }

    /// 几何常量与设计稿一致（D8：搜索栏 40 / 行 40；v4.10 D35：页脚 36）。
    #[test]
    fn geometry_matches_design_v4() {
        assert_eq!(ROW_H, 40.0, "行高 40（D8）");
        assert_eq!(SEARCHBAR_H, 40.0, "搜索栏高 40（Fluent Input large）");
        assert_eq!(
            FOOTER_PAD_Y * 2.0 + KEYCAP_H,
            36.0,
            "页脚总高 = 8 + 20 + 8 = 36（v4.10 D35）"
        );
        assert_eq!(FOOTER_FONT, 12.0, "页脚字号 = caption1 base200");
        assert_eq!(KEYCAP_FONT, 12.0, "键帽字号 = caption1（v4.10 D35）");
        assert_eq!(KEYCAP_RADIUS, 5, "键帽圆角 5（v4.10 D35）");
        assert_eq!(KEYCAP_DESC_GAP, 6.0, "帽-文距 6（v4.10 D35）");
        assert_eq!(KEYCAP_PAD_X, 8.0, "键帽左右 padding 8（v4.10 D35）");
    }

    #[test]
    fn theme_preference_maps_all_three_choices() {
        use crate::settings::ThemePref;
        // 批次 4.0：设置页三选 → egui 偏好一一对应
        assert_eq!(theme_preference(ThemePref::System), ThemePreference::System);
        assert_eq!(theme_preference(ThemePref::Light), ThemePreference::Light);
        assert_eq!(theme_preference(ThemePref::Dark), ThemePreference::Dark);
    }

    /// F2 parity：密度标准档硬性等于排印/几何常量锚点（紧凑/宽松按
    /// ±1 字号、±4px 行高同节奏派生），改常量漏改映射在此被拦下。
    #[test]
    fn list_metrics_standard_matches_constants() {
        use crate::settings::ListDensity;
        let m = ListMetrics::of(ListDensity::Standard);
        assert_eq!(m.row_h, ROW_H, "标准档行高 = ROW_H");
        assert_eq!(m.title_pt, LIST_TITLE_PT, "标准档行名字号");
        assert_eq!(m.cat_pt, LIST_CAT_PT, "标准档标签字号");
        assert_eq!(m.icon_cell, LIST_ICON_CELL, "标准档图标格");
        assert_eq!(m.glyph_pt, LIST_GLYPH_PT, "标准档 glyph 字号");
        // 非标准档为独立取值，且节奏与标准档对齐（行高 ±4）
        assert_eq!(
            ListMetrics::of(ListDensity::Compact).row_h,
            ROW_H - 4.0,
            "紧凑 = 标准 − 4"
        );
        assert_eq!(
            ListMetrics::of(ListDensity::Relaxed).row_h,
            ROW_H + 4.0,
            "宽松 = 标准 + 4"
        );
        assert_eq!(ListMetrics::of(ListDensity::Compact).icon_cell, 20.0);
        assert_eq!(ListMetrics::of(ListDensity::Relaxed).icon_cell, 28.0);
    }

    /// T6（2026-09-20）：着色三档的纯函数口径（混合比 + 混合结果）。
    #[test]
    fn colorization_mix_and_tint_rules() {
        use crate::settings::{ColorizationMode as M, CUSTOM_TINT_DEFAULT};
        let cz = |mode: M, intensity: u8| Colorization {
            mode,
            custom: CUSTOM_TINT_DEFAULT,
            intensity,
        };
        // 混合比：暗 0.08 / 亮 0.30；强度只作用于自定义档
        assert!((tint_mix(true, cz(M::SystemAccent, 10)) - 0.08).abs() < 1e-6);
        assert!((tint_mix(false, cz(M::SystemAccent, 100)) - 0.30).abs() < 1e-6);
        assert!((tint_mix(true, cz(M::Custom, 50)) - 0.04).abs() < 1e-6);
        assert!((tint_mix(false, cz(M::Custom, 100)) - 0.30).abs() < 1e-6);
        assert_eq!(tint_mix(true, cz(M::Custom, 0)), 0.0, "强度 0 = 无混合");
        assert!(
            (tint_mix(true, cz(M::Custom, 200)) - 0.08).abs() < 1e-6,
            "越界按 100"
        );
        // 混合结果：mix=0 恒等于面板色；同色混合不变；纯红混合后 R 升 / B 降
        let p = Palette::of(true);
        assert_eq!(mix_panel(p.panel, Color32::RED, 0.0), p.panel);
        assert_eq!(mix_panel(p.panel, p.panel, 0.5), p.panel);
        let red = mix_panel(p.panel, Color32::RED, 1.0);
        assert_eq!(red, Color32::RED);
        // 档位行为：None → 纯面板色；Custom 强度 0 → 纯面板色
        assert_eq!(tint_color(true, cz(M::None, 100)), p.panel);
        assert_eq!(tint_color(true, cz(M::Custom, 0)), p.panel);
        // Custom 满强度 = 按 0.08 朝自定义色插值（可与纯函数对照）
        let custom_full = cz(M::Custom, 100);
        assert_eq!(
            tint_color(false, custom_full),
            mix_panel(
                Palette::of(false).panel,
                Color32::from_rgb(
                    CUSTOM_TINT_DEFAULT[0],
                    CUSTOM_TINT_DEFAULT[1],
                    CUSTOM_TINT_DEFAULT[2]
                ),
                0.30
            )
        );
    }

    /// M1（2026-09-20）：材质注册表覆盖全部档位，且与浓淡层换算一致。
    #[test]
    fn backdrop_registry_covers_all_variants() {
        use crate::settings::Backdrop;
        for b in Backdrop::ALL {
            let cfg = backdrop_config(b);
            assert_eq!(
                cfg.tint_cap,
                panel_tint_cap(false, b),
                "{}：panel_tint_cap 必须委托注册表",
                b.label()
            );
            assert_eq!(
                cfg.tint_cap.is_some(),
                !matches!(b, Backdrop::None),
                "{}：仅无材质无浓淡层",
                b.label()
            );
        }
        // 云母与云母 Alt 同档；亚克力上限更高（拉满即全实色）
        assert_eq!(
            backdrop_config(Backdrop::MicaAlt).tint_cap,
            Some(TINT_CAP_MICA)
        );
        assert_eq!(
            backdrop_config(Backdrop::Acrylic).tint_cap,
            Some(TINT_CAP_ACRYLIC)
        );
        assert!(!backdrop_config(Backdrop::None).supports_opacity);
        assert!(Backdrop::ALL.len() == 4, "四档：无/云母/云母 Alt/亚克力");
        // 云母 Alt 与云母同档（cap + 行填充档一致）
        assert_eq!(
            backdrop_config(Backdrop::MicaAlt).row_fill,
            backdrop_config(Backdrop::Mica).row_fill
        );
        assert_eq!(
            backdrop_config(Backdrop::MicaAlt).row_fill,
            RowFillProfile::Weighted
        );
        assert_eq!(
            backdrop_config(Backdrop::Acrylic).row_fill,
            RowFillProfile::Light
        );
    }
}
