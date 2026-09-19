//! 共享图标管线（Windows Shell 图标 → PNG → 落盘缓存）。
//!
//! **为什么独立成模块**（2026-09-19）：同一套 HICON/HBITMAP → PNG 管线原本只存在于
//! [`crate::builtins::apps`]，文件搜索扩展需要「按文件类型显示真实图标」，若复制一份
//! 会产生双份维护——该管线内含 4 处真机坑（掩码型图标 alpha、掩码行 DWORD 对齐、
//! `GetDIBits` 负高 top-down、缓存 PNG 魔数自愈），漂移风险高。故上移为共享模块：
//! - [`crate::builtins::apps`]：应用图标（48px，`IShellItemImageFactory` 优先，
//!   `SHGetFileInfoW` 32px 回落）；
//! - `dd-ext-search`（bin）：文件类型图标（32px，`SHGetFileInfoW`，见
//!   [`file_type_icon_png`]）。
//!
//! 对外可见性为 `pub`：`bin/*.rs` 是**独立 crate**（同一 package 的 bin target），
//! 无法访问 lib 的 `pub(crate)` 项。除 dd-ext 自身的 bin 外不预期有其他使用者。
//!
//! 缓存布局：`<数据目录>/dd-run/cache/<prefix>-icons/<prefix>-<hash16>-<size>.png`
//! （与 `dd_host::manifest::cache_dir()` 同源推导，见 [`cache_dir`]）。

use std::path::{Path, PathBuf};

/// PNG 魔数（含前 8 字节）；缓存自愈判据见 [`is_png`]。
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// 图标缓存基目录：`%APPDATA%\dd-run\cache\<sub>\`
/// （`sub` = `apps-icons` / `file-icons` …；与 `dd-host::manifest::cache_dir()` 同源）。
pub fn cache_dir(sub: &str) -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("dd-run").join("cache").join(sub))
}

/// 稳定哈希缓存键（任意标识串）→ 16-hex 文件名片段。
///
/// 用 `DefaultHasher`：跨运行稳定（不随机化种子），足以避免文件名非法字符问题；
/// 碰撞概率对本用途（几十到几千个键）可忽略，且碰撞最坏后果是图标串用（非正确性缺陷）。
pub fn cache_key(key: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// 缓存文件名：`<prefix>-<hash16>-<size>.png`（纯函数，单测锚点）。
pub fn cache_file_name(prefix: &str, key: &str, size: u32) -> String {
    format!("{prefix}-{}-{size}.png", cache_key(key))
}

/// 字节是否为合法 PNG（魔数校验，纯函数，单测锚点）。
///
/// 自愈判据：落盘文件若**不是**合法 PNG（上次写入因 IO/中断失败留下的残片），
/// 视为未命中 → 上层重抽覆盖。
pub fn is_png(bytes: &[u8]) -> bool {
    bytes.len() >= PNG_MAGIC.len() && bytes[..PNG_MAGIC.len()] == PNG_MAGIC
}

/// 若 cache 已有该 key 的 PNG（且内容合法），返回该路径（不重抽）。
pub fn cached_png(dir: &Path, prefix: &str, key: &str, size: u32) -> Option<PathBuf> {
    let p = dir.join(cache_file_name(prefix, key, size));
    if !p.is_file() {
        return None;
    }
    match std::fs::read(&p) {
        Ok(head) if is_png(&head) => Some(p),
        _ => None,
    }
}

/// 把 PNG 字节写入 cache 目录并返回路径；已存在则不重写（幂等）。
///
/// 目录不存在时创建；任何 IO 失败 → `None`（调用方自行回落，不 panic）。
pub fn store_png(dir: &Path, prefix: &str, key: &str, size: u32, png: &[u8]) -> Option<PathBuf> {
    if !is_png(png) {
        return None;
    }
    std::fs::create_dir_all(dir).ok()?;
    let out = dir.join(cache_file_name(prefix, key, size));
    if !out.exists() {
        std::fs::write(&out, png).ok()?;
    }
    Some(out)
}

/// 文件/目录类型图标入口（文件搜索用）：按缓存键取 32×32 Shell 图标 PNG 路径。
///
/// `key` = 缓存键（扩展名 / 真实路径，见 `dd-ext-search` 的 `icon_cache_key_for`）；
/// `sample` = 用于取图的实际路径（同键的任一真实文件即可）。
///
/// 失败（无 `APPDATA`、`SHGetFileInfoW` 失败、编码失败、写盘失败）→ `None`，
/// 调用方回落既有类别 glyph（零退化）。
pub fn file_type_icon_png(key: &str, sample: &Path, size: u32) -> Option<PathBuf> {
    let dir = cache_dir(FILE_ICON_CACHE_SUB)?;
    if let Some(p) = cached_png(&dir, FILE_ICON_CACHE_PREFIX, key, size) {
        return Some(p);
    }
    let png = extract_png(sample)?;
    store_png(&dir, FILE_ICON_CACHE_PREFIX, key, size, &png)
}

/// 平台抽取入口（非 Windows 恒 `None`——文件搜索的传输通道本身也仅 Windows 可用）。
#[cfg(windows)]
fn extract_png(sample: &Path) -> Option<Vec<u8>> {
    // SAFETY：`shfileinfo_png` 自带安全契约，见其文档。
    unsafe { shfileinfo_png(sample) }
}

/// 非 Windows 桩：无 Shell 图标概念，恒 `None`（调用方回落类别 glyph）。
#[cfg(not(windows))]
fn extract_png(_sample: &Path) -> Option<Vec<u8>> {
    None
}

/// 文件图标缓存子目录名（与 apps 图标缓存并列）。
pub const FILE_ICON_CACHE_SUB: &str = "file-icons";
/// 文件图标缓存文件名前缀。
pub const FILE_ICON_CACHE_PREFIX: &str = "file";
/// 文件搜索图标边长（px）：`SHGFI_LARGEICON` 档 = 32×32；宿主图标格 24 逻辑点，
/// 32px 源在 100%–150% 缩放下可接受（更大档位见 `shell_icon` 文档「后续可升级」）。
pub const FILE_ICON_SIZE: u32 = 32;

// ────────────────────────────────────────────────────────────────
// Windows Shell / GDI 抽取（原 `builtins::apps` 实现，逻辑未改动）
// ────────────────────────────────────────────────────────────────

/// 回退链路：`SHGetFileInfoW` 取 32×32 HICON → PNG bytes（不落盘，调用方负责）。
///
/// 传**真实路径**（调用方保证）：`SHGFI_LARGEICON` 档对 `.exe`/`.lnk` 会给出该程序
/// 自身的图标（与资源管理器一致），对普通文件给出该类型的关联图标。
///
/// # Safety
///
/// 仅调用 Shell / GDI 的 Win32 API，**不接收裸句柄**、不做指针解引用；返回的
/// `HICON` 在本函数内 `DestroyIcon` 释放，不泄漏给调用方。故调用方无需额外不变量，
/// 只需注意它是一个会做磁盘/注册表查询的**阻塞**调用（勿在持有锁时调用）。
#[cfg(windows)]
pub unsafe fn shfileinfo_png(path: &Path) -> Option<Vec<u8>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;

    let path_wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut fi: SHFILEINFOW = std::mem::zeroed();
    let hr = SHGetFileInfoW(
        path_wide.as_ptr(),
        0,
        &mut fi,
        std::mem::size_of::<SHFILEINFOW>() as u32,
        SHGFI_ICON | SHGFI_LARGEICON,
    );
    if hr == 0 || fi.hIcon.is_null() {
        return None;
    }
    let png = hicon_to_png(fi.hIcon);
    DestroyIcon(fi.hIcon);
    png
}

/// HICON / HBITMAP → PNG bytes。实际尺寸由位图决定（GetImage BIGGERSIZEOK 可能给出
/// 大于请求的图）。失败 None（**不** DestroyIcon：调用方负责）。
///
/// 入参句柄类型实测不统一：`IShellItemImageFactory::GetImage` 在部分系统/项返回
/// HICON、另一部分返回 32bpp HBITMAP——两者都可交给本函数（`GetIconInfo` 成功走
/// HICON 链路含 AND 掩码；失败则由调用方改用 [`bitmap_to_png`]）。
///
/// alpha 生成策略（修复旧实现"强制 alpha=255"导致的黑角/锯齿）：
/// - 色位图含真实 per-pixel alpha → 原样保留；
/// - alpha 全 0（掩码型图标）→ 读 `hbmMask` 上半部 AND 掩码生成 alpha
///   （掩码位 1 = 透明）；掩码读取失败 → 整图 alpha=255 兜底。
///
/// # Safety
///
/// `hicon` 必须是调用方持有的**有效且未释放**的图标/位图句柄（`HICON` 或
/// `IShellItemImageFactory::GetImage` 返回的 32bpp `HBITMAP`——实测两种都可能出现，
/// 两者均可被 `GetIconInfo` 解析，解析失败由调用方改走 [`bitmap_to_png`]）。
/// 本函数**不**释放该句柄，所有权仍归调用方（勿重复释放、勿与本函数并发使用同句柄）。
#[cfg(windows)]
pub unsafe fn hicon_to_png(hicon: *mut core::ffi::c_void) -> Option<Vec<u8>> {
    use image::ImageEncoder;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, BITMAPINFO, BITMAPINFOHEADER,
        DIB_RGB_COLORS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};

    let mut ii: ICONINFO = std::mem::zeroed();
    if GetIconInfo(hicon, &mut ii) == 0 {
        return None;
    }
    let hbm_color = ii.hbmColor;
    let hbm_mask = ii.hbmMask;
    if hbm_color.is_null() {
        if !hbm_mask.is_null() {
            DeleteObject(hbm_mask);
        }
        return None;
    }

    let hdc = CreateCompatibleDC(std::ptr::null_mut());
    if hdc.is_null() {
        DeleteObject(hbm_color);
        if !hbm_mask.is_null() {
            DeleteObject(hbm_mask);
        }
        return None;
    }

    // ① 查询色位图实际尺寸（lpvBits=NULL 的 GetDIBits 会回填 bmiHeader）
    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    let ok = GetDIBits(
        hdc,
        hbm_color,
        0,
        0,
        std::ptr::null_mut(),
        &mut bmi,
        DIB_RGB_COLORS,
    );
    let w = bmi.bmiHeader.biWidth;
    let h = bmi.bmiHeader.biHeight.unsigned_abs() as i32;
    if ok == 0 || w <= 0 || h <= 0 || w > 512 || h > 512 {
        DeleteObject(hbm_color);
        if !hbm_mask.is_null() {
            DeleteObject(hbm_mask);
        }
        DeleteDC(hdc);
        return None;
    }

    // ② 32bpp top-down 读色位图
    let stride = w as usize * 4;
    let mut buf: Vec<u8> = vec![0; stride * h as usize];
    bmi.bmiHeader.biHeight = -h; // 负高 = top-down
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = 0; // BI_RGB
    let n = GetDIBits(
        hdc,
        hbm_color,
        0,
        h as u32,
        buf.as_mut_ptr() as *mut _,
        &mut bmi,
        DIB_RGB_COLORS,
    );

    // ③ 掩码型图标：读 AND 掩码上半部（1bpp，行 DWORD 对齐；位 1 = 透明）。
    //    必须在 DeleteObject(hbm_mask) 之前读。
    let mask = if !hbm_mask.is_null() {
        read_mask_bits(hdc, hbm_mask, w as usize, h as usize)
    } else {
        None
    };

    DeleteObject(hbm_color);
    if !hbm_mask.is_null() {
        DeleteObject(hbm_mask);
    }
    DeleteDC(hdc);
    if n == 0 {
        return None;
    }

    // BGRA → RGBA
    for px in buf.as_chunks_mut::<4>().0 {
        px.swap(0, 2); // B↔R
    }
    // alpha：有真实 per-pixel alpha 就保留；全 0 → 掩码生成 / 兜底不透明
    let has_alpha = buf.as_chunks::<4>().0.iter().any(|p| p[3] != 0);
    if !has_alpha {
        match mask {
            Some((mask_bits, mask_stride)) => {
                for (y, row) in buf.chunks_exact_mut(stride).enumerate() {
                    for (x, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                        let byte = mask_bits[y * mask_stride + x / 8];
                        // AND 掩码位 0 = 不透明
                        let opaque = (byte >> (7 - (x % 8))) & 1 == 0;
                        px[3] = if opaque { 0xff } else { 0 };
                    }
                }
            }
            None => {
                for px in buf.as_chunks_mut::<4>().0 {
                    px[3] = 0xff;
                }
            }
        }
    }

    let img = image::RgbaImage::from_raw(w as u32, h as u32, buf)?;
    let mut out = Vec::with_capacity(16 * 1024);
    let encoder = image::codecs::png::PngEncoder::new(&mut out);
    encoder
        .write_image(
            img.as_raw(),
            w as u32,
            h as u32,
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(out)
}

/// 32bpp GDI HBITMAP → PNG bytes（直接 GetDIBits；alpha 保留，全零回退不透明）。
///
/// # Safety
///
/// `hbm` 必须是调用方持有的**有效未释放** GDI 位图句柄；本函数只读取其像素，
/// **不**释放句柄（所有权归调用方），亦不得与同句柄的其他 GDI 操作并发。
#[cfg(windows)]
pub unsafe fn bitmap_to_png(hbm: windows_sys::Win32::Graphics::Gdi::HBITMAP) -> Option<Vec<u8>> {
    use image::ImageEncoder;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, GetDIBits, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
    };

    let hdc = CreateCompatibleDC(std::ptr::null_mut());
    if hdc.is_null() {
        return None;
    }
    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    let ok = GetDIBits(
        hdc,
        hbm,
        0,
        0,
        std::ptr::null_mut(),
        &mut bmi,
        DIB_RGB_COLORS,
    );
    let w = bmi.bmiHeader.biWidth;
    let h = bmi.bmiHeader.biHeight.unsigned_abs() as i32;
    if ok == 0 || w <= 0 || h <= 0 || w > 512 || h > 512 {
        DeleteDC(hdc);
        return None;
    }
    let stride = w as usize * 4;
    let mut buf: Vec<u8> = vec![0; stride * h as usize];
    bmi.bmiHeader.biHeight = -h; // top-down
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = 0; // BI_RGB
    let n = GetDIBits(
        hdc,
        hbm,
        0,
        h as u32,
        buf.as_mut_ptr() as *mut _,
        &mut bmi,
        DIB_RGB_COLORS,
    );
    DeleteDC(hdc);
    if n == 0 {
        return None;
    }
    for px in buf.as_chunks_mut::<4>().0 {
        px.swap(0, 2); // B↔R
    }
    let has_alpha = buf.as_chunks::<4>().0.iter().any(|p| p[3] != 0);
    if !has_alpha {
        for px in buf.as_chunks_mut::<4>().0 {
            px[3] = 0xff;
        }
    }
    let img = image::RgbaImage::from_raw(w as u32, h as u32, buf)?;
    let mut out = Vec::with_capacity(16 * 1024);
    let encoder = image::codecs::png::PngEncoder::new(&mut out);
    encoder
        .write_image(
            img.as_raw(),
            w as u32,
            h as u32,
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(out)
}

/// 读 AND 掩码（1bpp）上半部 `h` 行，返回 (bits, stride)。失败 None。
///
/// ICONINFO 的 hbmMask 高度为色位图高度 ×2（上 AND 下 XOR）；图标 alpha 全 0
/// 时只需 AND 掩码。行按 DWORD 对齐（GetDIBits 规则）。
#[cfg(windows)]
unsafe fn read_mask_bits(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    hbm_mask: windows_sys::Win32::Graphics::Gdi::HBITMAP,
    w: usize,
    h: usize,
) -> Option<(Vec<u8>, usize)> {
    use windows_sys::Win32::Graphics::Gdi::{
        GetDIBits, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
    };

    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    let ok = GetDIBits(
        hdc,
        hbm_mask,
        0,
        0,
        std::ptr::null_mut(),
        &mut bmi,
        DIB_RGB_COLORS,
    );
    let mw = bmi.bmiHeader.biWidth as usize;
    let mh = bmi.bmiHeader.biHeight.unsigned_abs() as usize;
    if ok == 0 || mw < w || mh < h || mw > 512 {
        return None;
    }
    let stride = mw.div_ceil(32) * 4; // 掩码行宽：每 32 像素一 DWORD
    let mut buf: Vec<u8> = vec![0; stride * h]; // 只取上半部（AND 掩码）
    bmi.bmiHeader.biBitCount = 1;
    bmi.bmiHeader.biCompression = 0;
    let n = GetDIBits(
        hdc,
        hbm_mask,
        0,
        h as u32,
        buf.as_mut_ptr() as *mut _,
        &mut bmi,
        DIB_RGB_COLORS,
    );
    if n == 0 {
        None
    } else {
        Some((buf, stride))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1×1 最小合法 PNG（RGBA 黑色不透明），python zlib/struct 生成。
    /// 注意：IDAT 解压后必须含完整扫描线（1 filter 字节 + 4 RGBA 字节 = 5 字节）。
    const PNG_1PX: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60,
        0x60, 0x60, 0xf8, 0x0f, 0x00, 0x01, 0x04, 0x01, 0x00, 0x5f, 0xe5, 0xc3, 0x4b, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn png_magic_check() {
        assert!(is_png(PNG_1PX));
        assert!(!is_png(b"not a png"), "非 PNG 内容 → 视为未命中（自愈）");
        assert!(!is_png(&[]), "空文件 → 未命中");
        assert!(!is_png(&PNG_1PX[..7]), "不足 8 字节 → 未命中");
    }

    #[test]
    fn cache_key_is_stable_and_filename_safe() {
        let a = cache_key("ext:rs");
        assert_eq!(a, cache_key("ext:rs"), "同键同值（跨运行稳定）");
        assert_ne!(a, cache_key("ext:ts"), "不同键不同值");
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn cache_file_name_layout() {
        let n = cache_file_name("file", "ext:rs", 32);
        assert!(n.starts_with("file-"), "{n}");
        assert!(n.ends_with("-32.png"), "{n}");
        assert_eq!(
            n,
            format!("file-{}-32.png", cache_key("ext:rs")),
            "命名 = prefix-hash-size.png"
        );
    }

    #[test]
    fn store_png_rejects_non_png_and_is_idempotent() {
        let dir = std::env::temp_dir().join("dd-run-shellicon-test");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            store_png(&dir, "file", "k", 32, b"garbage").is_none(),
            "非 PNG 不落盘"
        );
        let p = store_png(&dir, "file", "k", 32, PNG_1PX).expect("合法 PNG 落盘");
        assert!(p.is_file());
        assert_eq!(
            cached_png(&dir, "file", "k", 32).as_deref(),
            Some(p.as_path()),
            "命中缓存返回同一路径"
        );
        // 自愈：内容被破坏后视为未命中
        std::fs::write(&p, b"broken").expect("写坏缓存");
        assert!(
            cached_png(&dir, "file", "k", 32).is_none(),
            "非法内容 → 未命中"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
