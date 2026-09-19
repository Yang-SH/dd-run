//! 零依赖 PNG 编码器（RGBA8、单帧、非隔行）——E2：绕开 `image[png]` 依赖。
//!
//! **为什么自写**（2026-09-19，E2）：图标管线的 PNG 编码原先交 `image` 的
//! `PngEncoder`，实测会把整条 PNG 编码链（含 `fdeflate` 及其依赖）链入
//! `dd-ext-search.exe`；探针（临时短路 `hicon_to_png` 后重编）实测 **−96,768 B**，
//! 即体积主因。本模块用 std 实现所需最小子集（写、不读），把依赖降为 **0**：
//! - 交付约束是**宿主单文件** `dd-run.exe`，sidecar 体积预算是既有验收档
//!   `A-IC-06`（≤ 64 KB 增量）；本模块是让该档回到预算内的手段。
//! - 只实现**编码**。解码（宿主看图标）仍由 `dd-gui` 的 `image` 负责，格式面
//!   无需自行实现。
//!
//! **正确性怎么保证**：`image` 保留为 **dev-dependency**，单测用它解码本模块
//! 的产物并**逐字节比对像素**（见 `mod tests`）——即「自产 → 权威解码器回读」，
//! 而不是自证。交付产物不含 `image`（dev-dependency 不参与 release 构建）。
//!
//! **实现面**（均为 PNG/zlib 规范强制要求，非自创格式）：
//! - 结构：`PNG 魔数 + IHDR + IDAT + IEND`，每块带 CRC-32（反射多项式 `0xEDB88320`）；
//! - `IDAT` 载荷 = zlib 流（`0x78 0x01` 头 + deflate + Adler-32）；
//! - deflate 用**固定 Huffman 块**（`BTYPE=01`，单个 `BFINAL=1` 块）+ 贪心 LZ77
//!   （窗口 32 KiB、最短匹配 3、最长 258、哈希链最多 16 次探测）；
//! - 行滤波在 `None` / `Sub` / `Up` 三档中**取产物最小者**（图标类图像 `Up` 通常最优；
//!   逐行滤波类型的混用是规范允许的）。
//!
//! **不做的事**（刻意简单）：动态 Huffman、多块流、`Paeth`/`Average` 滤波、隔行、
//! 调色板/灰度/16 位。图标是 32–48 px 的 RGBA 小图，上述任一都换不来可感知收益。

/// PNG 魔数（8 字节，PNG 规范 §5.2）。与 [`crate::shell_icon::is_png`] 的判据同源。
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// LZ77 参数。
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const WINDOW: usize = 32_768;
const HASH_BITS: u32 = 15;
const MAX_CHAIN: usize = 16;

/// 长度码 257–285 的基值与额外位数（RFC 1951 §3.2.5）。
const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

/// 距离码 0–29 的基值与额外位数（RFC 1951 §3.2.5）。
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// 行滤波类型（PNG 规范 §9）。
const FILTER_NONE: u8 = 0;
const FILTER_SUB: u8 = 1;
const FILTER_UP: u8 = 2;

/// RGBA8 像素缓冲区 → 完整 PNG 字节流（`None` = 入参非法，调用方回落）。
///
/// `rgba` 必须是 `width * height * 4` 字节的**逐行 top-down** RGBA 数据
/// （与 `shell_icon` 的 GDI 抽取口径一致；通道序为 R,G,B,A）。
///
/// 失败仅一种情形：尺寸为 0 或缓冲区长度与尺寸不符——即**编程错误**，不是运行时
/// 环境问题。编码本身不返回 `Result`（无 IO、无外部依赖）。
pub fn encode_rgba(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() != w * h * 4 {
        return None;
    }

    // 三档滤波各编一遍，取产物最小者（图标小图，成本可忽略）。
    let mut best: Option<(Vec<u8>, u8)> = None;
    for filter in [FILTER_NONE, FILTER_SUB, FILTER_UP] {
        let raw = filtered_scanlines(w, h, rgba, filter);
        let z = zlib_deflate(&raw);
        if best.as_ref().is_none_or(|(prev, _)| z.len() < prev.len()) {
            best = Some((z, filter));
        }
    }
    let (idat, _filter) = best?;

    let mut out = Vec::with_capacity(idat.len() + 64);
    out.extend_from_slice(&PNG_MAGIC);

    // IHDR：宽/高 + 位深 8 + 颜色类型 6(RGBA) + 压缩 0 + 滤波 0 + 非隔行。
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&width.to_be_bytes());
    ihdr[4..8].copy_from_slice(&height.to_be_bytes());
    ihdr[8] = 8;
    ihdr[9] = 6;
    write_chunk(&mut out, b"IHDR", &ihdr);
    write_chunk(&mut out, b"IDAT", &idat);
    write_chunk(&mut out, b"IEND", &[]);
    Some(out)
}

/// 按 `filter` 生成扫描线数据（每行 = 1 字节滤波类型 + 该行滤波后像素）。
fn filtered_scanlines(w: usize, h: usize, rgba: &[u8], filter: u8) -> Vec<u8> {
    let stride = w * 4;
    let mut out = Vec::with_capacity(h * (1 + stride));
    for y in 0..h {
        let row = &rgba[y * stride..(y + 1) * stride];
        // `Up` 对首行无前行可用 → 按规范回落 `None`。
        let kind = if filter == FILTER_UP && y == 0 {
            FILTER_NONE
        } else {
            filter
        };
        out.push(kind);
        match kind {
            FILTER_SUB => {
                for (x, &b) in row.iter().enumerate() {
                    let left = if x >= 4 { row[x - 4] } else { 0 };
                    out.push(b.wrapping_sub(left));
                }
            }
            FILTER_UP => {
                let prev = &rgba[(y - 1) * stride..y * stride];
                out.extend(row.iter().zip(prev).map(|(cur, up)| cur.wrapping_sub(*up)));
            }
            _ => out.extend_from_slice(row),
        }
    }
    out
}

// ────────────────────────────────────────────────────────────────
// zlib / deflate（固定 Huffman + 贪心 LZ77）
// ────────────────────────────────────────────────────────────────

/// `data` → zlib 流（2 字节头 + 固定 Huffman 单块 deflate + Adler-32）。
fn zlib_deflate(data: &[u8]) -> Vec<u8> {
    let mut bw = BitWriter::default();
    // zlib 头：CM=8(CINFO=7 → 32 KiB 窗口)、FLEVEL=0(最快)、FCHECK 使 (0x78<<8|0x01)%31==0。
    bw.write_bits(0x78, 8);
    bw.write_bits(0x01, 8);
    bw.write_bits(1, 1); // BFINAL：单块
    bw.write_bits(1, 2); // BTYPE=01：固定 Huffman

    let mut head = vec![usize::MAX; 1 << HASH_BITS];
    let mut prev = vec![usize::MAX; data.len()];
    let mut i = 0;
    while i < data.len() {
        let (mut best_len, mut best_dist) = (0usize, 0usize);
        if i + MIN_MATCH <= data.len() {
            let hash = hash3(&data[i..i + MIN_MATCH]);
            let mut cand = head[hash];
            let mut probes = 0;
            while cand != usize::MAX && probes < MAX_CHAIN {
                let dist = i - cand;
                if dist > WINDOW {
                    break;
                }
                let max = MAX_MATCH.min(data.len() - i);
                let mut len = 0;
                while len < max && data[cand + len] == data[i + len] {
                    len += 1;
                }
                if len > best_len {
                    best_len = len;
                    best_dist = dist;
                    if len >= max {
                        break;
                    }
                }
                cand = prev[cand];
                probes += 1;
            }
            prev[i] = head[hash];
            head[hash] = i;
        }

        if best_len >= MIN_MATCH {
            write_match(&mut bw, best_len, best_dist);
            // 匹配内部各位置同样入哈希表（否则匹配后的区域无法被后续位置引用）。
            for (j, slot) in prev.iter_mut().enumerate().skip(i + 1).take(best_len - 1) {
                if j + MIN_MATCH <= data.len() {
                    let hash = hash3(&data[j..j + MIN_MATCH]);
                    *slot = head[hash];
                    head[hash] = j;
                }
            }
            i += best_len;
        } else {
            write_symbol(&mut bw, u32::from(data[i]));
            i += 1;
        }
    }
    write_symbol(&mut bw, 256); // 块结束符

    let mut out = bw.finish();
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// 三字节滚动哈希 → `[0, 1 << HASH_BITS)`。
fn hash3(bytes: &[u8]) -> usize {
    let v = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
    (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
}

/// 长度 → `LEN_BASE` 下标（`3..=258`）。
fn len_index(len: usize) -> usize {
    LEN_BASE
        .windows(2)
        .position(|w| (len as u16) < w[1])
        .unwrap_or(LEN_BASE.len() - 1)
}

/// 距离 → `DIST_BASE` 下标（`1..=32768`）。
fn dist_index(dist: usize) -> usize {
    DIST_BASE
        .windows(2)
        .position(|w| (dist as u16) < w[1])
        .unwrap_or(DIST_BASE.len() - 1)
}

/// 写一个「长度 + 距离」匹配对。
fn write_match(bw: &mut BitWriter, len: usize, dist: usize) {
    let li = len_index(len);
    write_symbol(bw, 257 + li as u32);
    let extra = LEN_EXTRA[li];
    if extra > 0 {
        bw.write_bits(len as u32 - u32::from(LEN_BASE[li]), u32::from(extra));
    }
    let di = dist_index(dist);
    bw.write_huff(di as u32, 5); // 距离码：固定 5 位，值即符号
    let extra = DIST_EXTRA[di];
    if extra > 0 {
        bw.write_bits(dist as u32 - u32::from(DIST_BASE[di]), u32::from(extra));
    }
}

/// 写一个 Huffman 符号（字面量字节，或 `256` 块结束、`257..=285` 长度码）。
fn write_symbol(bw: &mut BitWriter, symbol: u32) {
    match symbol {
        0..=143 => bw.write_huff(0x30 + symbol, 8),
        144..=255 => bw.write_huff(0x190 + symbol - 144, 9),
        256..=279 => bw.write_huff(symbol - 256, 7),
        // 280..=287：本编码器只产出 280..=285（长度码），286/287 为规范保留值。
        _ => bw.write_huff(0xC0 + symbol - 280, 8),
    }
}

/// deflate 位流写入器：**LSB 优先**打包（RFC 1951 §3.1.1）。
#[derive(Default)]
struct BitWriter {
    out: Vec<u8>,
    buf: u32,
    bits: u32,
}

impl BitWriter {
    /// 写入 `n` 位原始值（额外位、块头等；低位在前）。
    fn write_bits(&mut self, value: u32, n: u32) {
        debug_assert!((1..=16).contains(&n), "位宽越界：{n}");
        self.buf |= (value & ((1u32 << n) - 1)) << self.bits;
        self.bits += n;
        while self.bits >= 8 {
            self.out.push((self.buf & 0xFF) as u8);
            self.buf >>= 8;
            self.bits -= 8;
        }
    }

    /// 写入一个 Huffman 码：码字**高位在前**（规范），故先按位反转再走 LSB 打包。
    fn write_huff(&mut self, code: u32, len: u32) {
        let mut reversed = 0u32;
        for i in 0..len {
            reversed |= ((code >> i) & 1) << (len - 1 - i);
        }
        self.write_bits(reversed, len);
    }

    /// 补齐末字节（补 0）并取走结果。
    fn finish(mut self) -> Vec<u8> {
        if self.bits > 0 {
            self.out.push((self.buf & 0xFF) as u8);
        }
        self.out
    }
}

// ────────────────────────────────────────────────────────────────
// 校验和与块封装
// ────────────────────────────────────────────────────────────────

/// PNG 块封装：长度 + 类型 + 数据 + CRC-32(类型 + 数据)。
fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = Crc32::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finish().to_be_bytes());
}

/// CRC-32（PNG 规范 §5.5：反射多项式 `0xEDB88320`，初值/末值均取反）。
struct Crc32(u32);

impl Crc32 {
    fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u32::from(b);
            for _ in 0..8 {
                let mask = (self.0 & 1).wrapping_neg();
                self.0 = (self.0 >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
    }

    fn finish(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}

/// Adler-32（RFC 1950 §9）：zlib 流尾校验和。
fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用 `image`（dev-dependency）解码——**独立权威解码器**，非自证。
    fn decode(png: &[u8]) -> (usize, usize, Vec<u8>) {
        let img = image::load_from_memory(png).expect("自产 PNG 必须能被 image 解码");
        let rgba = img.to_rgba8();
        (
            rgba.width() as usize,
            rgba.height() as usize,
            rgba.into_raw(),
        )
    }

    fn roundtrip(w: usize, h: usize, pixels: Vec<u8>) {
        let png = encode_rgba(w as u32, h as u32, &pixels).expect("编码成功");
        let (dw, dh, back) = decode(&png);
        assert_eq!((dw, dh), (w, h), "尺寸往返一致");
        assert_eq!(back, pixels, "像素逐字节往返一致");
    }

    /// 确定性 PRNG（避免为测试引入 rand 依赖）。
    fn lcg(seed: u32) -> impl FnMut() -> u8 {
        let mut state = seed;
        move || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        }
    }

    fn image_of(w: usize, h: usize, mut f: impl FnMut(usize, usize) -> [u8; 4]) -> Vec<u8> {
        let mut v = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&f(x, y));
            }
        }
        v
    }

    #[test]
    fn roundtrip_flat_48x48() {
        let px = image_of(48, 48, |_, _| [30, 120, 200, 255]);
        roundtrip(48, 48, px);
    }

    #[test]
    fn roundtrip_gradient_32x32() {
        let px = image_of(32, 32, |x, y| {
            [(x * 8) as u8, (y * 8) as u8, ((x + y) * 4) as u8, 255]
        });
        roundtrip(32, 32, px);
    }

    #[test]
    fn roundtrip_icon_like_with_transparency() {
        // 圆形「图标」：中心不透明、四角全透明 + 边缘半透明。
        let px = image_of(32, 32, |x, y| {
            let (dx, dy) = (x as i32 - 16, y as i32 - 16);
            let d = dx * dx + dy * dy;
            let a = if d < 120 {
                255
            } else if d < 240 {
                128
            } else {
                0
            };
            [(x * 7) as u8, (y * 5) as u8, 200, a]
        });
        roundtrip(32, 32, px);
    }

    #[test]
    fn roundtrip_noise_27x13_is_incompressible_but_valid() {
        // 非整字节对齐的行宽 + 随机像素：走「几乎全是字面量」的分支。
        let mut gen = lcg(0x1234_5678);
        let px = image_of(27, 13, |_, _| [gen(), gen(), gen(), gen()]);
        roundtrip(27, 13, px);
    }

    #[test]
    fn roundtrip_single_pixel_and_tall_thin() {
        roundtrip(1, 1, vec![1, 2, 3, 4]);
        roundtrip(1, 300, image_of(1, 300, |_, y| [y as u8, 0, 0, 255]));
    }

    #[test]
    fn flat_image_is_actually_compressed() {
        // 平坦图像：`Up` 滤波后除首行外几乎全 0 → 产物应远小于未压缩扫描线（4,128 B）。
        let px = image_of(32, 32, |_, _| [0, 0, 0, 0]);
        let png = encode_rgba(32, 32, &px).expect("编码成功");
        let raw = 32 * (1 + 32 * 4);
        assert!(
            png.len() < raw / 8,
            "平坦图像应被显著压缩：png={} raw={raw}",
            png.len()
        );
    }

    #[test]
    fn structure_is_well_formed() {
        let px = image_of(4, 2, |x, y| [x as u8, y as u8, 9, 255]);
        let png = encode_rgba(4, 2, &px).expect("编码成功");
        assert_eq!(&png[..8], &PNG_MAGIC, "PNG 魔数");
        // 逐块走一遍：长度 + 类型 + 数据 + CRC，且 CRC 自洽（用独立实现复算）。
        let mut off = 8;
        let mut kinds: Vec<String> = Vec::new();
        while off < png.len() {
            let len = u32::from_be_bytes(png[off..off + 4].try_into().unwrap()) as usize;
            let kind = &png[off + 4..off + 8];
            let data = &png[off + 8..off + 8 + len];
            let stored = u32::from_be_bytes(png[off + 8 + len..off + 12 + len].try_into().unwrap());
            let mut crc = Crc32::new();
            crc.update(kind);
            crc.update(data);
            assert_eq!(stored, crc.finish(), "块 CRC 自洽：{:?}", kind);
            kinds.push(String::from_utf8_lossy(kind).into_owned());
            if kind == b"IHDR" {
                assert_eq!(&data[..8], &[0, 0, 0, 4, 0, 0, 0, 2], "IHDR 宽高 = 4×2");
                assert_eq!(&data[8..], &[8, 6, 0, 0, 0], "位深 8 / RGBA / 非隔行");
            }
            off += 12 + len;
        }
        assert_eq!(kinds, vec!["IHDR", "IDAT", "IEND"], "块序列");
        assert_eq!(off, png.len(), "无尾随字节");
    }

    #[test]
    fn rejects_mismatched_dimensions() {
        assert!(encode_rgba(0, 1, &[]).is_none(), "宽 0");
        assert!(encode_rgba(1, 0, &[]).is_none(), "高 0");
        assert!(encode_rgba(2, 2, &[0; 15]).is_none(), "缓冲区长度不符");
        assert!(encode_rgba(2, 2, &[0; 17]).is_none(), "缓冲区长度不符");
        assert!(encode_rgba(2, 2, &[0; 16]).is_some(), "长度正确则成功");
    }

    #[test]
    fn length_and_distance_index_tables_cover_full_range() {
        assert_eq!(len_index(3), 0);
        assert_eq!(len_index(4), 1);
        assert_eq!(len_index(10), 7);
        assert_eq!(len_index(11), 8);
        assert_eq!(len_index(257), 27);
        assert_eq!(len_index(258), 28, "258 = 长度码 285（无额外位）");
        assert_eq!(dist_index(1), 0);
        assert_eq!(dist_index(2), 1);
        assert_eq!(dist_index(4), 3);
        assert_eq!(dist_index(5), 4);
        assert_eq!(dist_index(32768), 29, "最大距离");
    }

    #[test]
    fn checksums_match_known_vectors() {
        assert_eq!(adler32(b""), 1, "空输入");
        assert_eq!(adler32(b"abc"), 0x024D_0127, "RFC 1950 示例值");
        // CRC-32："123456789" 的标准值 0xCBF43926（PNG/zip 同一多项式）。
        let mut crc = Crc32::new();
        crc.update(b"123456789");
        assert_eq!(crc.finish(), 0xCBF4_3926);
    }
}
