//! §5.2（dd-run-改进分析.md）：检查更新——读 GitHub Releases 的 `latest`，
//! 与当前版本比较；**不做自动安装**（便携版静默替换 exe 风险大于收益，原案口径）。
//!
//! HTTP 方案（本批决策记档）：**不引 HTTP crate**（运行时面 lock 新增 + 二进制
//! 体积税），走系统 `curl`（Windows 10 1803+ 自带 `System32\curl.exe`，POSIX
//! 普遍可用；Git Bash 亦有）。`--max-time` 由调用方给 3 s（验收「点击后 3 s
//! 内显示」由 curl 超时硬约束承接）。GitHub API 要求 User-Agent；匿名限额
//! 60 次/时/IP，手动检查频度远低于此。
//!
//! 版本比较口径：取 `tag_name` 剥离前导 `v`，按 `MAJOR.MINOR.PATCH` 三段
//! 数值比较——**预发布后缀（如 `-beta`）被忽略**（`v0.3.0-beta` 视为 0.3.0，
//! 会提示更新；近似记档，语义化预发布排序不在 v1 范围）。解析失败（API 出错
//! 返回非 JSON / 字段缺失）→ `Failed`，UI 提示而非静默。

use serde_json::Value;

/// 当前产品版本（dd-gui = 产品版本，workspace 继承）。
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 本仓库 latest release 的 GitHub API 端点。
pub const RELEASES_API: &str = "https://api.github.com/repos/Yang-SH/dd-run/releases/latest";

/// 检查结果（纯数据，UI 层转 toast 文案）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    /// 已是最新（远端 ≤ 当前）。
    UpToDate,
    /// 有新版本（剥 `v` 后的语义版本字符串，如 `"0.3.0"`）。
    Available { version: String },
    /// 检查失败（网络 / 超时 / 响应异常），附可读原因。
    Failed(String),
    /// O17：下载在途（线程首条进度消息；UI 置进行态，通道保持待终态）。
    Downloading,
    /// O17：下载完成且 SHA256 校验通过（zip 路径，供「打开所在目录」定位）。
    Downloaded { path: String },
    /// O17：下载或校验失败（残留 zip/SUMS 已清理），附可读原因。
    VerifyFailed(String),
}

/// 从 GitHub releases/latest 响应体提取 `tag_name`（缺失/非法 JSON → None）。
pub fn parse_tag_name(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    v.get("tag_name")?.as_str().map(str::to_string)
}

/// 剥离 tag 的前导 `v`/`V`（`v0.2.2` → `0.2.2`）。
pub fn normalize_tag(tag: &str) -> &str {
    tag.strip_prefix('v')
        .or_else(|| tag.strip_prefix('V'))
        .unwrap_or(tag)
}

/// 三段数值版本解析（`0.2.1` → (0,2,1)；预发布后缀截断；失败 → None）。
fn parse_triple(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split(['-', '+']).next().unwrap_or(version);
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next()?.parse().ok()?;
    let patch = it.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// 比较：远端 tag 与当前版本谁新（解析失败的一侧 → `Failed`）。
pub fn classify(current: &str, tag: &str) -> UpdateStatus {
    let Some(remote) = parse_triple(normalize_tag(tag)) else {
        return UpdateStatus::Failed(format!("远端版本号无法解析：{tag}"));
    };
    let Some(local) = parse_triple(current) else {
        return UpdateStatus::Failed(format!("本地版本号无法解析：{current}"));
    };
    if remote > local {
        UpdateStatus::Available {
            version: normalize_tag(tag).to_string(),
        }
    } else {
        UpdateStatus::UpToDate
    }
}

/// 经系统 `curl` 拉取 latest release 的 `tag_name`（网络/超时/解析失败 → Err）。
pub fn fetch_latest_tag(timeout_secs: u64) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args([
            "-fsSL", // fail 静默 + 重定向跟随；-S 把错误送到 stderr
            "--max-time",
            &timeout_secs.to_string(),
            "-A",
            &format!("dd-run/{}", CURRENT_VERSION),
            RELEASES_API,
        ])
        .output()
        .map_err(|e| format!("无法启动 curl（{e}）"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "curl 退出码 {:?}{}",
            output.status.code(),
            detail
                .trim()
                .is_empty()
                .then(String::new)
                .unwrap_or_else(|| format!("：{}", detail.trim()))
        ));
    }
    parse_tag_name(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| "响应缺少 tag_name".to_string())
}

/// O17：本仓库 latest release 资产直链前缀（`SHA256SUMS.txt` 由 release.yml
/// 随 Release 附着，全资产覆盖——校验源为发行链既有产物，无新链路）。
pub const RELEASES_DOWNLOAD_BASE: &str =
    "https://github.com/Yang-SH/dd-run/releases/latest/download";

/// O17：SHA256SUMS 资产名（release.yml `sha256sum dd-run-*` 产物）。
pub const SHA256SUMS_ASSET: &str = "SHA256SUMS.txt";

/// O17：便携 zip 资产名（实测口径：`tools/portable_zip.ps1` 的
/// `dist/dd-run-<ver>-portable.zip` + release.yml `files: dist/dd-run-*-portable.zip`；
/// 版本 = 剥 `v` 后的语义版本，与 [`classify`] 的 `Available.version` 同源）。
pub fn portable_zip_asset(version: &str) -> String {
    format!("dd-run-{version}-portable.zip")
}

/// O17：资产直链 URL。
pub fn release_asset_url(asset: &str) -> String {
    format!("{RELEASES_DOWNLOAD_BASE}/{asset}")
}

/// O17：SHA-256 纯函数实现（FIPS 180-4）。决策点「纯函数 vs sha2 crate」按
/// 实施计划默认定案：依赖克制判据（零新增 crate / Cargo.lock 零变更），
/// 正确性由下方 NIST 标准向量单测锁定（空串 / "abc" / 56 字节 / 百万 a）。
const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// 计算数据的 SHA-256，输出小写十六进制（64 字符）。
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    // `as_chunks` = 编译期定长切块（MSRV 1.95 已稳定），clippy 1.98 的
    // `chunks_exact_to_as_chunks` 门禁口径；块内 4 字节直接解为 u32。
    for block in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word) in block.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for (i, k) in SHA256_K.iter().enumerate() {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*k)
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = String::with_capacity(64);
    for word in h {
        out.push_str(&format!("{word:08x}"));
    }
    out
}

/// O17：解析 SHA256SUMS 文本，取 `filename` 行的哈希（小写十六进制）。
/// 形态容错：sha256sum 标准两空格分隔与二进制模式「空格 + 星号」（`<hex>  <name>`
/// / `<hex> *<name>`）；CRLF 行尾；哈希大小写不敏感；非数据行跳过。
pub fn parse_sums(text: &str, filename: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        // 最短合法行 = 64 hex + 1 分隔符 + 1 文件名字符
        if line.len() < 66 {
            continue;
        }
        let (hex, rest) = line.split_at(64);
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let rest = rest.trim_start();
        let name = rest.strip_prefix('*').unwrap_or(rest);
        if name.trim_end() == filename {
            return Some(hex.to_ascii_lowercase());
        }
    }
    None
}

/// O17：校验文件哈希与 SHA256SUMS 行一致（缺行 / 不匹配 → Err 可读原因）。
pub fn verify_sums(sums_text: &str, filename: &str, actual_hex: &str) -> Result<(), String> {
    let expected = parse_sums(sums_text, filename)
        .ok_or_else(|| format!("SHA256SUMS 中未找到 {filename} 对应行"))?;
    let actual = actual_hex.to_ascii_lowercase();
    if expected != actual {
        return Err(format!(
            "哈希不匹配（SUMS 行 {}…，实得 {}…）",
            &expected[..12],
            &actual[..12]
        ));
    }
    Ok(())
}

/// O17：便捷校验——zip 字节 → sha256 → 与 SUMS 行比对。
pub fn verify_zip_bytes(sums_text: &str, asset: &str, zip_bytes: &[u8]) -> Result<(), String> {
    verify_sums(sums_text, asset, &sha256_hex(zip_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tag_name_extracts_field() {
        assert_eq!(
            parse_tag_name(r#"{"tag_name":"v0.3.0","name":"x"}"#).as_deref(),
            Some("v0.3.0")
        );
        assert_eq!(parse_tag_name(r#"{"name":"x"}"#), None, "缺 tag_name");
        assert_eq!(parse_tag_name("not json"), None, "非法 JSON");
        assert_eq!(parse_tag_name(r#"{"tag_name":123}"#), None, "类型损坏");
    }

    #[test]
    fn classify_compares_triples_and_ignores_prerelease_suffix() {
        assert_eq!(
            classify("0.2.1", "v0.2.2"),
            UpdateStatus::Available {
                version: "0.2.2".into()
            }
        );
        assert_eq!(classify("0.2.1", "v0.2.1"), UpdateStatus::UpToDate);
        assert_eq!(classify("0.3.0", "v0.2.9"), UpdateStatus::UpToDate);
        assert_eq!(
            classify("0.2.1", "v1.0.0"),
            UpdateStatus::Available {
                version: "1.0.0".into()
            },
            "远端主版本更高 → Available"
        );
        assert_eq!(
            classify("0.2.1", "v0.3.0-beta"),
            UpdateStatus::Available {
                version: "0.3.0-beta".into()
            },
            "比较忽略预发布后缀（0.3.0-beta > 0.2.1）；展示保留完整 tag（近似记档）"
        );
        assert!(
            matches!(classify("0.2.1", "weird"), UpdateStatus::Failed(_)),
            "远端版本号无法解析 → Failed"
        );
    }

    #[test]
    fn normalize_strips_leading_v() {
        assert_eq!(normalize_tag("v0.2.2"), "0.2.2");
        assert_eq!(normalize_tag("0.2.2"), "0.2.2");
        assert_eq!(normalize_tag("V1.0"), "1.0");
    }

    // ── O17：SHA-256 与 SHA256SUMS 校验 ──────────────────────────────────────

    #[test]
    fn sha256_nist_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "NIST 空串向量"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "NIST \"abc\" 向量"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            "NIST 56 字节向量（跨块填充边界）"
        );
    }

    #[test]
    fn sha256_nist_million_a() {
        let data = vec![b'a'; 1_000_000];
        assert_eq!(
            sha256_hex(&data),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
            "NIST 百万 a 向量（多块累加）"
        );
    }

    #[test]
    fn asset_urls_match_release_chain() {
        let v = "0.3.0";
        assert_eq!(portable_zip_asset(v), "dd-run-0.3.0-portable.zip");
        assert_eq!(
            release_asset_url(&portable_zip_asset(v)),
            format!("{RELEASES_DOWNLOAD_BASE}/dd-run-0.3.0-portable.zip")
        );
        assert_eq!(
            release_asset_url(SHA256SUMS_ASSET),
            format!("{RELEASES_DOWNLOAD_BASE}/SHA256SUMS.txt")
        );
    }

    #[test]
    fn parse_sums_handles_sha256sum_forms() {
        let zip = "dd-run-0.3.0-portable.zip";
        let hex = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        // 两空格 + CRLF + 二进制星号 + 大写哈希行 + 垃圾行混合
        let text = format!("{hex}  {zip}\r\n{hex}  other.zip\r\n{hex} *{zip}\r\njunk\r\nshort");
        assert_eq!(
            parse_sums(&text, zip).as_deref(),
            Some(hex),
            "标准两空格行优先命中"
        );
        assert_eq!(parse_sums(&text, "other.zip").as_deref(), Some(hex));
        assert_eq!(parse_sums(&text, "missing.zip"), None, "缺行 → None");
        assert_eq!(parse_sums("junk", zip), None, "非数据行跳过");
    }

    #[test]
    fn verify_sums_ok_mismatch_and_missing() {
        let zip = "dd-run-0.3.0-portable.zip";
        let hex = sha256_hex(b"abc");
        let sums = format!("{hex}  {zip}\n");
        assert_eq!(verify_zip_bytes(&sums, zip, b"abc"), Ok(()), "一致 → Ok");
        assert!(
            verify_zip_bytes(&sums, zip, b"abd").is_err(),
            "篡改字节 → 哈希不匹配"
        );
        assert!(
            verify_zip_bytes(&sums, "other.zip", b"abc").is_err(),
            "SUMS 缺行 → 失败"
        );
        assert!(
            verify_sums(&sums, zip, &"ABCD".repeat(16)).is_err(),
            "哈希不等 → 失败"
        );
    }
}
