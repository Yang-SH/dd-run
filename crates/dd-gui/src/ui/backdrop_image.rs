//! 背景图层（T9，2026-10-05）：本地图片 → 解码 → 纹理缓存 → 面板底层绘制。
//!
//! 一期互斥语义（settings-personalization-plan §3.2 P2）：设了背景图 = 该图
//! 即面板背景，材质 / 着色 / 边框链路暂停生效（`refresh_backdrop` 按
//! `Backdrop::None` 走回退路径），清除后恢复。**不做**实时模糊 / 亮度滑杆。
//!
//! 缓存策略（icon_cache 同款口径 + mtime 门控）：缓存键 = (路径, mtime)，
//! 键不变沿用旧纹理；读盘 / 解码失败 → **负缓存**（同键不重试，避免逐帧
//! 磁盘 IO），文件被替换（mtime 变化）或路径变更后自动重试；解码失败时
//! 调用方回落「无图」——面板底保持回退实色（`refresh_backdrop` 已按
//! `Backdrop::None` 注册），视觉与关闭背景图一致。

use crate::settings::BgImageFit;
use eframe::egui;
use std::time::SystemTime;

/// 解码限幅：宽/高上限（S-04 同款收紧口径——默认 `Limits` 只限分配不限
/// 尺寸）。超出部分交给缩略，不直接拒载（壁纸常见 4K 内）。
const MAX_BG_DIM: u32 = 8192;
/// 解码限幅：总像素分配上限（8192² × 4B ≈ 268 MB 的防御上限）。
const MAX_BG_ALLOC: u64 = 268_435_456;
/// 纹理最长边上限：超过则等比缩略（egui 纹理常驻显存，2048 最长边
/// ≈ 2048×1152×4 ≈ 9.4 MB，面板显示尺寸 ≤1.3K，缩略无损观感）。
const TEX_MAX_SIDE: u32 = 2048;

/// 背景图缓存键：路径 + 文件 mtime（缺文件 → mtime `None`，同样构成键——
/// 文件出现 / 消失都会变键触发重载）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct BackdropKey {
    path: String,
    mtime: Option<SystemTime>,
}

/// 背景图纹理缓存（挂在 [`crate::app::PaletteApp`] 上，随面板生命周期存活）。
#[derive(Default)]
pub(crate) struct BackdropImageCache {
    key: Option<BackdropKey>,
    /// `Some(Ok)` = 现键纹理就绪；`Some(Err)` = 现键读盘 / 解码失败（负缓存）。
    state: Option<Result<egui::TextureHandle, ()>>,
}

impl BackdropImageCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 主入口：按当前设置路径取背景纹理。`None` / 空串 / 失败 → `None`
    /// （调用方回落无图绘制）。键未变时零 IO 直接命中缓存。
    pub(crate) fn texture(
        &mut self,
        ctx: &egui::Context,
        path: Option<&str>,
    ) -> Option<&egui::TextureHandle> {
        let path = path.map(str::trim).filter(|p| !p.is_empty());
        let Some(path) = path else {
            // 路径清除：释放纹理（显存回收），下次设图重新加载
            if self.key.is_some() || self.state.is_some() {
                self.key = None;
                self.state = None;
            }
            return None;
        };
        let key = BackdropKey {
            path: path.to_string(),
            mtime: mtime_of(path),
        };
        if self.key.as_ref() == Some(&key) {
            if let Some(Ok(tex)) = &self.state {
                return Some(tex);
            }
            return None; // 负缓存命中：同键已失败，不重试
        }
        // 键变化（首次 / 路径改 / 文件被替换）：重载
        self.key = Some(key);
        self.state = None;
        let loaded = std::fs::read(path)
            .ok()
            .and_then(|bytes| decode_backdrop_image(&bytes))
            .map(|img| ctx.load_texture("backdrop_image", img, egui::TextureOptions::LINEAR));
        self.state = Some(loaded.ok_or(()));
        match &self.state {
            Some(Ok(tex)) => Some(tex),
            _ => None,
        }
    }

    /// 当前设置路径是否处于「加载失败」态（设置页错误行展示用；路径为空
    /// 或未失败 → false）。
    pub(crate) fn load_failed(&self, path: Option<&str>) -> bool {
        let Some(path) = path.map(str::trim).filter(|p| !p.is_empty()) else {
            return false;
        };
        matches!(
            (&self.key, &self.state),
            (Some(k), Some(Err(()))) if k.path == path
        )
    }
}

fn mtime_of(path: &str) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// 解码本地图片字节 → egui 颜色纹理数据。独立函数便于无窗口单测（不依赖
/// egui Context）。失败返回 `None`（调用方负缓存 + 回落无图）。
///
/// S-04 同款口径：解码前显式收紧 `image::Limits`；超 [`TEX_MAX_SIDE`] 等
/// 比缩略（壁纸 4K 常态，直接上纹理浪费显存）。
pub(crate) fn decode_backdrop_image(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_BG_DIM);
    limits.max_image_height = Some(MAX_BG_DIM);
    limits.max_alloc = Some(MAX_BG_ALLOC);
    reader.limits(limits);

    let img = reader.decode().ok()?;
    let img = if img.width().max(img.height()) > TEX_MAX_SIDE {
        img.thumbnail(TEX_MAX_SIDE, TEX_MAX_SIDE)
    } else {
        img
    };
    let img = img.to_rgba8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        img.as_raw(),
    ))
}

/// 绘制期 UV 窗口（纯函数，可单测）：按适应方式从源图取映射到目标矩形的
/// 归一化 UV 区间 `[min, max]`。
/// - `Stretch`：整图拉伸（UV 全域）。
/// - `Fill`（cover）：保持宽高比铺满、居中裁剪——源比目标「更宽」时水平裁，
///   更「高」时垂直裁（resize 只变目标尺寸，UV 随帧重算，**无需重解码**）。
pub(crate) fn backdrop_uv(fit: BgImageFit, src: (f32, f32), dst: (f32, f32)) -> [[f32; 2]; 2] {
    let full = [[0.0, 0.0], [1.0, 1.0]];
    let (sw, sh) = src;
    let (dw, dh) = dst;
    if sw <= 0.0 || sh <= 0.0 || dw <= 0.0 || dh <= 0.0 {
        return full;
    }
    match fit {
        BgImageFit::Stretch => full,
        BgImageFit::Fill => {
            let src_ar = sw / sh;
            let dst_ar = dw / dh;
            if src_ar > dst_ar {
                // 源更宽 → 裁水平：可见宽 = dst_ar × 源高
                let vis_w = (dst_ar / src_ar).min(1.0);
                let u0 = (1.0 - vis_w) / 2.0;
                [[u0, 0.0], [u0 + vis_w, 1.0]]
            } else {
                // 源更高（或同比）→ 裁垂直：可见高 = 源宽 / dst_ar
                let vis_h = (src_ar / dst_ar).min(1.0);
                let v0 = (1.0 - vis_h) / 2.0;
                [[0.0, v0], [1.0, v0 + vis_h]]
            }
        }
    }
}

/// 面板底层绘制：图（按不透明度）+ 着色叠层（按强度叠 `tint_color`）。
/// 调用点 = `draw_panel` 帧首（页脚 / CentralPanel 的 Frame 填充在背景图
/// 生效时置透明，让本图层透出；行 / 卡片自带不透明底，可读性不受影响）。
pub(crate) fn paint_backdrop_image(
    ui: &egui::Ui,
    rect: egui::Rect,
    tex: &egui::TextureHandle,
    fit: BgImageFit,
    opacity_pct: u8,
    tint_pct: u8,
    tint_color: egui::Color32,
) {
    let painter = ui.painter_at(rect);
    let size = tex.size_vec2();
    let [[u0, v0], [u1, v1]] = backdrop_uv(fit, (size.x, size.y), (rect.width(), rect.height()));
    let opacity = f32::from(opacity_pct.clamp(0, 100)) / 100.0;
    painter.image(
        tex.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(u1, v1)),
        egui::Color32::WHITE.gamma_multiply(opacity),
    );
    let tint = f32::from(tint_pct.clamp(0, 100)) / 100.0;
    if tint > 0.0 {
        painter.rect_filled(rect, 0.0, tint_color.gamma_multiply(tint));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::BgImageFit;

    fn uv(fit: BgImageFit, sw: f32, sh: f32, dw: f32, dh: f32) -> [[f32; 2]; 2] {
        backdrop_uv(fit, (sw, sh), (dw, dh))
    }

    #[test]
    fn t9_uv_stretch_is_full_and_degenerate_falls_back_full() {
        assert_eq!(
            uv(BgImageFit::Stretch, 1920.0, 1080.0, 800.0, 600.0),
            [[0.0, 0.0], [1.0, 1.0]],
            "Stretch = UV 全域"
        );
        assert_eq!(
            uv(BgImageFit::Fill, 0.0, 100.0, 800.0, 600.0),
            [[0.0, 0.0], [1.0, 1.0]],
            "源宽 0（防御）→ 全域"
        );
        assert_eq!(
            uv(BgImageFit::Fill, 100.0, 100.0, 0.0, 0.0),
            [[0.0, 0.0], [1.0, 1.0]],
            "目标尺寸 0（防御）→ 全域"
        );
    }

    #[test]
    fn t9_uv_fill_crops_horizontally_when_source_wider() {
        // 源 16:9、目标 4:3 → 源更宽 → 水平居中裁：可见宽 = (4/3)/(16/9) = 0.75
        let [[u0, v0], [u1, v1]] = uv(BgImageFit::Fill, 1920.0, 1080.0, 800.0, 600.0);
        assert_eq!(v0, 0.0);
        assert_eq!(v1, 1.0, "垂直不裁");
        assert!((u0 - 0.125).abs() < 1e-6, "u0 居中裁 12.5%，得 {u0}");
        assert!((u1 - 0.875).abs() < 1e-6, "u1 得 {u1}");
        // resize 方向回归：目标变宽（16:9 同比）→ 无裁剪
        let [[u0, v0], [u1, v1]] = uv(BgImageFit::Fill, 1920.0, 1080.0, 1280.0, 720.0);
        assert!(
            (u0 - v0).abs() < 1e-6 && (u1 - 1.0).abs() < 1e-6 && (v1 - 1.0).abs() < 1e-6,
            "同比 Fill = 无裁剪"
        );
    }

    #[test]
    fn t9_uv_fill_crops_vertically_when_source_taller() {
        // 源 4:3、目标 16:9 → 源更高 → 垂直居中裁：可见高 = (4/3)/(16/9) = 0.75
        let [[u0, v0], [u1, v1]] = uv(BgImageFit::Fill, 800.0, 600.0, 1280.0, 720.0);
        assert_eq!(u0, 0.0);
        assert_eq!(u1, 1.0, "水平不裁");
        assert!((v0 - 0.125).abs() < 1e-6, "v0 居中裁 12.5%，得 {v0}");
        assert!((v1 - 0.875).abs() < 1e-6, "v1 得 {v1}");
    }

    /// 解码健壮性：垃圾字节 / 截断 PNG → None；合法小 PNG → 尺寸正确且
    /// 超限缩略生效。PNG 由 `image` 编码器现做（零 fixture 依赖）。
    #[test]
    fn t9_decode_tolerates_garbage_and_thumbnails_oversize() {
        assert!(decode_backdrop_image(b"not an image").is_none());
        assert!(decode_backdrop_image(&[]).is_none());
        // 合法 PNG（4×3 红块）
        let small = image::DynamicImage::new_rgb8(4, 3);
        let mut buf = std::io::Cursor::new(Vec::new());
        small
            .write_to(&mut buf, image::ImageFormat::Png)
            .expect("png encode");
        let decoded = decode_backdrop_image(buf.get_ref()).expect("合法 PNG 可解码");
        assert_eq!(decoded.size, [4, 3]);
        // 超限缩略：4096×4096 → 最长边 ≤ TEX_MAX_SIDE
        let big = image::DynamicImage::new_rgb8(4096, 4096);
        let mut buf = std::io::Cursor::new(Vec::new());
        big.write_to(&mut buf, image::ImageFormat::Png)
            .expect("png encode");
        let thumb = decode_backdrop_image(buf.get_ref()).expect("超限 PNG 缩略后解码");
        assert!(
            thumb.size[0].max(thumb.size[1]) <= TEX_MAX_SIDE as usize,
            "缩略后最长边 ≤ {TEX_MAX_SIDE}，得 {:?}",
            thumb.size
        );
    }
}
