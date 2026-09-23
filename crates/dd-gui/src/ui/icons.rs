//! 图标：IconView 解析、纹理缓存、glyph/path 绘制。

use crate::app::PaletteApp;
use dd_gui::state::PanelItem;
use dd_gui::theme;
use dd_protocol::model::IconKind;
use eframe::egui;
use std::collections::HashMap;

/// 图标格边长/glyph 字号按密度档运行时传入（`theme::ListMetrics`；标准档
/// 锚点 = `theme::LIST_ICON_CELL` 24 / `LIST_GLYPH_PT` 20，原真机反馈
/// 2026-09-12 从 20×20 放大到 24×24）。F2 前为本模块常量，随 F2 上移。
/// path 图标解码失败时的占位 glyph（Segoe MDL2 "Page" U+E7C3；
/// 设计稿 04：加载失败回落占位 glyph）。
pub(crate) const PLACEHOLDER_GLYPH: char = '\u{E7C3}';

/// 列表行图标的**已解析**渲染形态（M5 批次 2）。
///
/// 在 `ScrollArea` 闭包之外统一解析（需要 `&mut self` 写纹理缓存与 `ctx`），
/// 闭包内只读借用本表渲染，避免借用冲突。
pub(crate) enum IconView {
    /// 无图标 / url（本态暂缓）：弱色占位 glyph（I1：与解码失败占位区分层级
    /// ——「本来就没有」用 text4 极弱色，「加载失败」走 Glyph 态 text2 色），
    /// 占满图标格保持各行对齐。
    Empty,
    /// glyph 码位文本（§8.6 glyph 值本身；path 解码失败回落占位 glyph 也走此态）。
    Glyph { text: String },
    /// path 本地文件解码成功后的纹理；`dark` = 图标本体偏暗（暗色主题下
    /// 需垫浅色圆角底，否则黑 glyph 贴暗背景不可见——真机反馈 ChatGPT 图标）。
    Texture {
        tex: egui::TextureHandle,
        dark: bool,
    },
}

/// 判断图标是否"本体偏暗"：不透明像素（α≥32）的 **max(r,g,b) 均值** < 90。
/// 全透明图（无有效像素）不算暗。纯函数，可无窗口单测。
/// 用 max 通道而非感知亮度：暗底可见性取决于最亮通道——饱和红（AMD，
/// max=224、感知亮度仅 ~60）在暗底上清晰可见，不该垫底；黑/深灰 glyph
/// （ChatGPT，max≈32）不可见，该垫底。
pub(crate) fn icon_is_dark(img: &egui::ColorImage) -> bool {
    let mut sum = 0u32;
    let mut n = 0u32;
    for px in &img.pixels {
        // ColorImage 存 unmultiplied RGBA
        let [r, g, b, a] = px.to_array();
        if a < 32 {
            continue;
        }
        sum += r.max(g).max(b) as u32;
        n += 1;
    }
    if n == 0 {
        return false;
    }
    (sum / n) < 90
}

/// path 图标读盘的**文件大小上限**（S-04，2026-09-23）。
///
/// 图标本体是 24–48 px 的小图，正常 PNG/ICO 远小于 64 KB；512 KB 已是极大宽松值。
/// 为什么需要：`icon.value` 完全由扩展响应决定（§8.6），加固前直接 `fs::read`
/// 整文件读入 → 不受信扩展给一个 10 GB 路径即可让宿主 OOM
/// （见 `docs/security-audit-2026-09-23.md` §4.3）。
const MAX_ICON_BYTES: u64 = 512 * 1024;

/// 图标解码的**尺寸上限**（宽/高各自，S-04）：超过即拒绝，不再分配像素缓冲。
///
/// 为什么需要：`image` 的默认 `Limits` 只限制总分配（512 MiB）、**不限尺寸**——
/// 一张 8000×8000 的 PNG（256 MB RGBA）会被完整解出，对图标格（≤ 24 px）纯属浪费
/// 且构成解压炸弹面。512 覆盖 256/512 px 高清图标，留足余量。
const MAX_ICON_DIM: u32 = 512;

/// 图标解码的**总分配上限**（S-04）：512×512×4 ≈ 1 MiB，限 16 MiB 足够容纳
/// 解码中间态，同时把单张图的分配面收在两个数量级以内。
const MAX_ICON_ALLOC: u64 = 16 * 1024 * 1024;

/// 读图标文件：**先看元数据后读内容**，超限/非普通文件直接拒绝（S-04）。
///
/// 与 `fs::read` 的差别在于：拒绝发生在**读盘之前**，故超大文件不产生任何分配。
fn read_icon_limited(path: &str) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_ICON_BYTES {
        return None;
    }
    std::fs::read(path).ok()
}

/// 解码 PNG/ICO 字节 → egui 颜色纹理数据（§8.6 path 图标）。
/// 独立函数便于无窗口单测（不依赖 egui Context）。
/// 失败返回 `None`——调用方回落占位 glyph（设计稿 04）。
///
/// S-04：解码前**显式收紧 `image::Limits`**（宽/高 ≤ [`MAX_ICON_DIM`]、
/// 总分配 ≤ [`MAX_ICON_ALLOC`]）——默认 `Limits` 只限分配不限尺寸，超大尺寸图仍会
/// 被完整解出。
pub(crate) fn decode_icon_image(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_ICON_DIM);
    limits.max_image_height = Some(MAX_ICON_DIM);
    limits.max_alloc = Some(MAX_ICON_ALLOC);
    reader.limits(limits);

    let img = reader.decode().ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        img.as_raw(),
    ))
}

impl PaletteApp {
    /// 预解析一批可见项的图标为渲染形态（M5 批次 2）。
    ///
    /// 在 `ScrollArea` 闭包**外**调用（可 `&mut self` 写纹理缓存）：
    /// - `glyph` → 原文直接渲染（图标字体已在 `setup_cjk_fonts` 装入字形回退链）；
    /// - `path` → 查 [`Self::icon_cache`]；未命中则读盘 + [`decode_icon_image`] +
    ///   `ctx.load_texture` 入缓存；读盘/解码失败 → 占位 glyph（[`PLACEHOLDER_GLYPH`]）；
    /// - `url` → 留接口暂缓（M5 决策：不做网络下载），弱色占位 glyph（I1）；
    /// - 无 icon → 弱色占位 glyph（I1 修订设计稿 04：原「空列」改为占位符，
    ///   对齐不变、观感不再像「图标缺失」）。
    pub(crate) fn resolve_icons(
        &mut self,
        ctx: &egui::Context,
        items: &[(usize, PanelItem)],
    ) -> HashMap<usize, IconView> {
        let mut views = HashMap::new();
        for (idx, item) in items {
            let Some(icon) = &item.icon else {
                continue;
            };
            let view = match icon.kind {
                IconKind::Glyph => IconView::Glyph {
                    text: icon.value.clone(),
                },
                IconKind::Path => {
                    if let Some((tex, dark)) = self.icon_cache.get(&icon.value) {
                        IconView::Texture {
                            tex: tex.clone(),
                            dark: *dark,
                        }
                    } else if self.icon_failed.contains(&icon.value) {
                        // 负缓存命中：读盘/解码已失败过，直接回落占位 glyph，
                        // 不再每帧重试（避免失败 eprintln 刷屏）。
                        IconView::Glyph {
                            text: PLACEHOLDER_GLYPH.to_string(),
                        }
                    } else {
                        match read_icon_limited(&icon.value)
                            .and_then(|bytes| decode_icon_image(&bytes))
                        {
                            Some(img) => {
                                let dark = icon_is_dark(&img);
                                let name = format!("dd-path-icon://{}", icon.value);
                                let tex = ctx.load_texture(name, img, egui::TextureOptions::LINEAR);
                                let view = IconView::Texture {
                                    tex: tex.clone(),
                                    dark,
                                };
                                self.icon_cache.insert(icon.value.clone(), (tex, dark));
                                view
                            }
                            None => {
                                self.icon_failed.insert(icon.value.clone());
                                log::debug!(
                                    "[dd-gui] path 图标读盘/解码失败：{}（回落占位 glyph，本次会话不再重试）",
                                    icon.value
                                );
                                IconView::Glyph {
                                    text: PLACEHOLDER_GLYPH.to_string(),
                                }
                            }
                        }
                    }
                }
                // url：M5 决策"留接口暂缓"——不做网络下载/缓存，本态渲染空列
                IconKind::Url => IconView::Empty,
            };
            views.insert(*idx, view);
        }
        views
    }
}

/// 渲染列表行图标单元格（边长 `m.icon_cell`，行首固定列，垂直居中）。
/// - `None` / [`IconView::Empty`]：弱色占位 glyph（I1：保持图标列对齐，观感
///   为「本来就没有」，与解码失败的 text2 占位区分层级）；
/// - [`IconView::Glyph`]：码位文本（图标字体渲染，I2：text2 次级色——与彩色
///   path 图标视觉重量对齐；已核 091c765 visuals.weak_text_color = text2，
///   本处显式取 token，行为等价）；
/// - [`IconView::Texture`]：纹理贴满图标格（UV 缩放，不裁剪、不变形）。
pub(crate) fn draw_icon_cell(ui: &mut egui::Ui, icon: Option<&IconView>, m: theme::ListMetrics) {
    let p = theme::Palette::of(ui.visuals().dark_mode);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(m.icon_cell, m.icon_cell), egui::Sense::hover());
    match icon {
        None | Some(IconView::Empty) => {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                PLACEHOLDER_GLYPH.to_string(),
                egui::FontId::proportional(m.glyph_pt),
                p.text4,
            );
        }
        Some(IconView::Glyph { text }) => {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(m.glyph_pt),
                p.text2,
            );
        }
        Some(IconView::Texture { tex, dark }) => {
            // 暗色主题 + 暗色本体图标：垫浅色圆角底（ueli/Start 菜单式白底 tile），
            // 否则黑 glyph 贴暗背景不可见（真机反馈 ChatGPT 图标）。亮色主题不需要
            // （浅底上深 glyph 本就清晰）。底比图标区外扩 2px、圆角 4，贴近 Fluent。
            if *dark && ui.visuals().dark_mode {
                let bg = egui::Rect::from_center_size(
                    rect.center(),
                    egui::vec2(m.icon_cell + 2.0, m.icon_cell + 2.0),
                );
                ui.painter().rect_filled(
                    bg,
                    egui::CornerRadius::same(4),
                    egui::Color32::from_rgb(0xf5, 0xf5, 0xf5),
                );
            }
            // uv = 全图 [0,1]×[0,1]；大于图标格的原图由渲染缩放，不做预缩放
            let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
            ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 暗色图标检测（真机反馈 2026-09-04：ChatGPT 黑 glyph 暗主题不可见） ──

    fn solid_image(r: u8, g: u8, b: u8, a: u8) -> egui::ColorImage {
        egui::ColorImage::from_rgba_unmultiplied(
            [4, 4],
            &[
                r, g, b, a, r, g, b, a, r, g, b, a, r, g, b, a, r, g, b, a, r, g, b, a, r, g, b, a,
                r, g, b, a, r, g, b, a, r, g, b, a, r, g, b, a, r, g, b, a, r, g, b, a, r, g, b, a,
                r, g, b, a, r, g, b, a,
            ],
        )
    }

    #[test]
    fn icon_darkness_detection() {
        assert!(
            icon_is_dark(&solid_image(0x20, 0x20, 0x20, 255)),
            "黑 glyph → 暗"
        );
        assert!(
            !icon_is_dark(&solid_image(0xe0, 0x10, 0x10, 255)),
            "红（AMD）→ 不暗"
        );
        assert!(
            !icon_is_dark(&solid_image(0xff, 0xff, 0xff, 255)),
            "白 → 不暗"
        );
        assert!(!icon_is_dark(&solid_image(0, 0, 0, 0)), "全透明 → 不算暗");
        // 半透明（α<32）像素不计入
        assert!(
            !icon_is_dark(&solid_image(0, 0, 0, 16)),
            "α=16 视为透明 → 不暗"
        );
    }

    // ── M5 UI 批次 2：path 图标解码（§8.6，纯字节层，无窗口依赖） ─────────

    /// 1×1 最小合法 PNG（RGBA 黑色不透明），python zlib/struct 生成。
    /// 注意：IDAT 解压后必须含完整扫描线（1 filter 字节 + 4 RGBA 字节 = 5 字节），
    /// 早期版本只写了 2 字节导致 png crate 报 `NoMoreImageData`。
    const PNG_1PX: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60,
        0x60, 0x60, 0xf8, 0x0f, 0x00, 0x01, 0x04, 0x01, 0x00, 0x5f, 0xe5, 0xc3, 0x4b, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn decode_icon_image_parses_minimal_png() {
        let img = decode_icon_image(PNG_1PX).expect("1×1 合法 PNG 应解码成功");
        assert_eq!(img.size, [1, 1]);
        assert_eq!(img.pixels.len(), 1);
    }

    #[test]
    fn decode_icon_image_rejects_garbage() {
        assert!(decode_icon_image(b"this is definitely not a png").is_none());
        assert!(decode_icon_image(&[]).is_none());
    }

    // ── S-04（2026-09-23）：读盘与解码双上限 ─────────────────────────

    /// 超限文件在读盘**之前**被拒（不产生分配）；缺失/目录同样拒绝。
    #[test]
    fn read_icon_limited_rejects_oversize_without_reading() {
        let dir = std::env::temp_dir().join(format!("dd-gui-icon-s04-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let small = dir.join("small.png");
        std::fs::write(&small, PNG_1PX).unwrap();
        assert!(
            read_icon_limited(&small.to_string_lossy()).is_some(),
            "小文件应放行"
        );

        // 超限文件：600 KB（> 512 KB 上限）——写入用 set_len 稀疏落盘，不实际占盘
        let big = dir.join("big.png");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(600 * 1024)
            .unwrap();
        assert!(
            read_icon_limited(&big.to_string_lossy()).is_none(),
            "超限文件应在读盘前被拒"
        );

        assert!(
            read_icon_limited(&dir.to_string_lossy()).is_none(),
            "目录不是图标文件"
        );
        assert!(
            read_icon_limited(&dir.join("nope.png").to_string_lossy()).is_none(),
            "不存在的路径 → None"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 512×512 合法 PNG 正常解码（尺寸上限的正向边界）。
    #[test]
    fn decode_icon_image_accepts_up_to_dimension_limit() {
        let side = MAX_ICON_DIM as usize;
        let rgba = vec![0x80u8; side * side * 4];
        let png =
            dd_ext::png::encode_rgba(MAX_ICON_DIM, MAX_ICON_DIM, &rgba).expect("编码器应成功");
        let img = decode_icon_image(&png).expect("512×512 应可解码");
        assert_eq!(img.size, [side, side]);
    }

    /// 超过尺寸上限的合法 PNG 被拒（**合法性无误，仅因超限**）——解压炸弹面收敛。
    #[test]
    fn decode_icon_image_rejects_over_dimension() {
        let side = (MAX_ICON_DIM + 88) as usize; // 600×600
        let rgba = vec![0u8; side * side * 4];
        let png = dd_ext::png::encode_rgba(side as u32, side as u32, &rgba).expect("编码器应成功");
        assert!(
            decode_icon_image(&png).is_none(),
            "{side}×{side} 超过 {MAX_ICON_DIM} 上限应被拒"
        );
    }
}
