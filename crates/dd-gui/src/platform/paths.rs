//! `file://` URL 解析：纯函数，无 IO（便于单测）。

/// 拆分 `file://` URL 为 `(authority, path)`。
///
/// - `file:///<path>` → `(None, path)`（空 authority，本机路径）
/// - `file://<host>/<path>` → `(Some(host), path)`（UNC 网络共享，v3.3 P2 §9.6 支持）
/// - 非 `file://` 协议、或 `file://<host>` 后无路径 → None
///
/// 注：host 不做 percent-decode（`file://` 的 authority 极少出现非 ASCII/IPv6
/// 字面量，Everything 索引结果亦不会产生），已作为已知边界记档。
fn split_file_url(url: &str) -> Option<(Option<&str>, &str)> {
    let rest = url.strip_prefix("file://")?;
    match rest.strip_prefix('/') {
        Some(p) => Some((None, p)),
        None => {
            let slash = rest.find('/')?;
            Some((Some(&rest[..slash]), &rest[slash + 1..]))
        }
    }
}

/// `(authority, path)` → Windows 路径：`/` → `\`；有 host 时拼 UNC `\\host\share\…`。
fn to_windows_path(host: Option<&str>, path: &str) -> String {
    let p = path.replace('/', "\\");
    match host {
        Some(h) => format!("\\\\{h}\\{p}"),
        None => p,
    }
}

/// 去掉 `?query` / `#fragment`（Windows 文件名不允许 `?`，`#` 则合法，故仅作
/// 次要候选——见 `file_url_candidates` 的排序）。
fn strip_query_fragment(s: &str) -> &str {
    match s.find(['?', '#']) {
        Some(i) => &s[..i],
        None => s,
    }
}

/// 最小 `%XX` → byte 解码（**按字节处理**，避免把 UTF-8 多字节序列拆成多个
/// Latin-1 codepoint）。非法/不完整转义按字面保留。
fn percent_decode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_byte(bytes[i + 1]), hex_byte(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// `file://` URL → **按可能性排序**的本地路径候选（纯函数，无 IO）。
///
/// 排序（v3.3 P2 修复 §9.6 缺陷 1/2：`%` `#` `?` 未处理）：
/// 1. 原样保留（`%XX` 按字面）——覆盖「文件名真的含 `%`」，如 `report%20final.txt`；
/// 2. 去 `?query`/`#fragment`——覆盖 URL 语义分量（`?` 在 Windows 文件名非法）；
/// 3. 上述两者的 percent-decode 版——覆盖标准编码 URL，如 `%20` → 空格。
///
/// 伴随宿主的存在性优选（`resolve_file_url_to_path`）后，两种来源互不干扰：
/// 字面路径存在即命中自身，不存在才落到解码解释。
pub(crate) fn file_url_candidates(url: &str) -> Vec<String> {
    let (host, path) = match split_file_url(url) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let mut variants: Vec<&str> = vec![path];
    let trimmed = strip_query_fragment(path);
    if trimmed != path {
        variants.push(trimmed);
    }

    let mut out: Vec<String> = Vec::with_capacity(4);
    for v in &variants {
        push_unique(&mut out, to_windows_path(host, v));
    }
    for v in &variants {
        push_unique(&mut out, to_windows_path(host, &to_decoded_str(v)));
    }
    out
}

fn to_decoded_str(s: &str) -> String {
    String::from_utf8_lossy(&percent_decode(s.as_bytes())).into_owned()
}

fn push_unique(out: &mut Vec<String>, s: String) {
    if !out.contains(&s) {
        out.push(s);
    }
}

/// `file://` URL → 最可能存在 Windows 路径（v3.3 P2：`file_url_candidates` +
/// 存在性优选）。
///
/// IO 策略：**UNC 路径不做 `exists()`**（离线 SMB 共享会让 `Path::exists()`
/// 阻塞到网络超时），直接返回由 `ShellExecuteW` 处理；本地路径按候选顺序取
/// 第一个存在者；全部不存在时回退首选（原样保留版），便于上层报错。
pub(crate) fn resolve_file_url_to_path(url: &str) -> Option<String> {
    let mut cands = file_url_candidates(url);
    if let Some(hit) = cands
        .iter()
        .position(|c| !is_unc_path(c) && std::path::Path::new(c).exists())
    {
        return Some(cands.swap_remove(hit));
    }
    cands.into_iter().next()
}

fn is_unc_path(p: &str) -> bool {
    p.starts_with("\\\\")
}

fn hex_byte(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
