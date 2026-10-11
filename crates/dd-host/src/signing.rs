//! 发行方扩展签名（**O14/F12 批次一**，2026-10-11）。
//!
//! 完整选型与三态语义见 [`docs/extension-signing-plan.md`](../../../docs/extension-signing-plan.md)
//! §4–§5。要点：
//!
//! - **目标**：闭合 trust 台账「非防篡改」定性与 R-12 升级重钉的静默信任窗口——
//!   信任锚从「应用分发包本身」升级为「发行方私钥」。
//! - **签名对象（D3 捆绑锚）**：`清单字节 SHA256` + `entry.command 目标 exe SHA256`
//!   的域分隔捆绑串（见 [`bundle_message`]）——单签名同时锚定清单与二进制，
//!   exe 单独替换无法通过验签；锚结构与 R-12 双哈希同构，判定链平滑扩展。
//! - **载体（D2-A）**：清单旁挂 `<清单文件名>.sig`（detached，minisign 风格文本），
//!   **清单字节零改动**（schema v1.0 不动）。
//! - **三态降级（D5）**：`Valid` → 免同意 `AutoTrusted`；`Invalid` → fail-closed
//!   `Pending` + 告警位（比现状严）；`Absent` → 落入既有判定（不比现状差）。
//!
//! ## 批次一范围（本模块现状）
//!
//! - 验签机器全量就位（解析 / 捆绑 / ed25519 验签 / 三态整合 + 单测）；
//! - [`PUBLISHER_PUBLIC_KEY_HEX`] 为**占位公钥**：openssl 现场生成、**种子即弃**
//!   （未写入任何文件/CI）——当前没有任何 `.sig` 能通过内置公钥验签，属预期
//!   安全姿态（fail-closed）；批次二以 Release 工作流 secret 持有真种子、轮换
//!   本常量并补端到端用例（含内置公钥全链路腿）；
//! - 非 Windows：哈希设施未实现（trust.rs「已声明的缺口」），捆绑锚无从计算
//!   → [`check_signature`] 恒 [`SigCheck::Absent`]，现行 fail-open 行为不变
//!   （D2 缺口的签名侧收口依赖哈希跨平台化，随 O6；ed25519 验签本身已跨平台）。

use crate::manifest::LoadedExtension;
use crate::trust::sha256_file;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::path::{Path, PathBuf};

/// 发行方公钥（Ed25519 公钥，32 字节小写 hex）。
///
/// **批次一占位**：由 openssl 现场生成、种子即弃（未落任何存储）——当前不存在
/// 能签出有效 `.sig` 的私钥，任何 `.sig` 都会落入 `Invalid`（fail-closed）。
/// 批次二以发行方真钥轮换本常量（私钥入 Release 工作流 secret，见选型稿 §4-D4）。
pub const PUBLISHER_PUBLIC_KEY_HEX: &str =
    "298f5123ef025ce61d13ce367bfcf7c5050e287a06b8f4d90dc85d97974438a1";

/// 签名检查三态（O14 选型稿 §4-D5）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigCheck {
    /// `.sig` 存在且验签通过、双哈希一致 → 免同意 `AutoTrusted`。
    Valid,
    /// `.sig` 存在但不可用（解析失败 / 验签失败 / 双哈希不符）→ fail-closed
    /// `Pending` + 告警位。携带机器可读原因（日志用，非契约文案）。
    Invalid(&'static str),
    /// `.sig` 不存在（或非 Windows 哈希设施缺失）→ 落入既有判定，零变化。
    Absent,
}

/// 清单旁挂签名文件路径：同目录、清单文件名追加 `.sig`
/// （`com.ddrun.filesearch.json` → `com.ddrun.filesearch.json.sig`）。
pub fn sig_path_for(manifest_path: &Path) -> PathBuf {
    let mut name = manifest_path
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_default();
    name.push(".sig");
    manifest_path.with_file_name(name)
}

/// 捆绑消息（D3）：域分隔串防跨锚重放——哈希只是 hex 文本，无域前缀时
/// 「把 exe 哈希当清单哈希」的错位签名也能过验。
pub fn bundle_message(manifest_sha_hex: &str, exe_sha_hex: &str) -> Vec<u8> {
    format!("dd-run-ext-sig-v1\nmanifest={manifest_sha_hex}\nexe={exe_sha_hex}\n").into_bytes()
}

/// 解析 `.sig` 文本（选型稿 §5 草案）：
///
/// ```text
/// untrusted comment: dd-run extension signature v1
/// <base64(64 字节 Ed25519 签名)>
/// trusted comment: manifest=<hex64> exe=<hex64>
/// ```
///
/// trusted-comment 中的双哈希与实际计算值**逐字节比对**——即使签名本身可验，
/// 锚不符仍判 `Invalid`（防 trusted-comment 被调包后签名与旧锚组合的重放）。
fn parse_sig(text: &str) -> Result<(Signature, String, String), &'static str> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() != 3 || !lines[0].starts_with("untrusted comment:") {
        return Err("格式不符（应为 3 行：untrusted / 签名 / trusted）");
    }
    let sig_b64 = lines[1].trim();
    if sig_b64.len() != 88 {
        // 64 字节 Ed25519 签名的标准 base64 长度（86 数据字符 + 2 个 `=` 填充）
        return Err("签名字段长度不符（应为 88 字符 base64）");
    }
    let sig_bytes = base64_decode_64(sig_b64).ok_or("签名字段 base64 解码失败")?;
    let sig = Signature::from_slice(&sig_bytes).map_err(|_| "签名长度非法")?;
    let trusted = lines[2]
        .strip_prefix("trusted comment: manifest=")
        .ok_or("trusted comment 缺失或前缀不符")?;
    let (m, x) = trusted
        .split_once(" exe=")
        .ok_or("trusted comment 缺 exe 锚")?;
    if m.len() != 64
        || x.len() != 64
        || !m.bytes().all(|b| b.is_ascii_hexdigit())
        || !x.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("trusted comment 双哈希格式不符");
    }
    Ok((sig, m.to_ascii_lowercase(), x.to_ascii_lowercase()))
}

/// 标准 base64 解码（64 字节定长）。**刻意手写**：仅此一处使用，避免为 88 字符
/// 引入 base64 crate（依赖克制，O3 口径）。
fn base64_decode_64(s: &str) -> Option<[u8; 64]> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let s = s.trim_end_matches('=');
    if s.len() != 86 {
        return None;
    }
    let mut out = [0u8; 64];
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    let mut oi = 0;
    for ch in s.bytes() {
        let v = TABLE.iter().position(|&t| t == ch)? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out[oi] = ((buf >> bits) & 0xFF) as u8;
            oi += 1;
        }
    }
    (oi == 64).then_some(out)
}

/// 内置公钥解析（防御式：常量损坏 → `None`，调用方视作 `Absent` 并记日志）。
pub fn built_in_verifying_key() -> Option<VerifyingKey> {
    let mut key = [0u8; 32];
    let hex = PUBLISHER_PUBLIC_KEY_HEX;
    if hex.len() != 64 {
        return None;
    }
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let hi = (chunk[0] as char).to_digit(16)? as u8;
        let lo = (chunk[1] as char).to_digit(16)? as u8;
        key[i] = (hi << 4) | lo;
    }
    VerifyingKey::from_bytes(&key).ok()
}

/// 三态签名检查（D5）。`Absent` = 无 `.sig` 或非 Windows（哈希设施缺失）；
/// 其余任何不合法情形一律 `Invalid`（fail-closed），不吞错误。
pub fn check_signature(ext: &LoadedExtension, vk: &VerifyingKey) -> SigCheck {
    if !crate::trust::hash_available() {
        // 非 Windows：捆绑锚的 SHA-256 未实现（trust.rs 已声明缺口），无法构成
        // 验签前提 → 与无 `.sig` 同等回落（现行 fail-open 行为保持不变）。
        return SigCheck::Absent;
    }
    let sig_path = sig_path_for(&ext.path);
    if !sig_path.is_file() {
        return SigCheck::Absent;
    }
    let text = match std::fs::read_to_string(&sig_path) {
        Ok(t) => t,
        Err(_) => return SigCheck::Invalid("sig 读取失败"),
    };
    let (sig, m_hex, x_hex) = match parse_sig(&text) {
        Ok(v) => v,
        Err(e) => return SigCheck::Invalid(e),
    };
    // 双哈希：清单读字节（清单小，直接读），exe 流式（可达数十 MB，复用既有设施）。
    let manifest_bytes = match std::fs::read(&ext.path) {
        Ok(b) => b,
        Err(_) => return SigCheck::Invalid("清单读取失败"),
    };
    let m_now = match crate::trust::sha256_bytes(&manifest_bytes) {
        Ok(h) => h,
        Err(_) => return SigCheck::Invalid("清单哈希失败"),
    };
    let x_now = match sha256_file(&ext.command) {
        Ok(h) => h,
        Err(_) => return SigCheck::Invalid("exe 哈希失败"),
    };
    if m_now != m_hex || x_now != x_hex {
        return SigCheck::Invalid("双哈希与 trusted comment 不符（清单或 exe 已变更）");
    }
    // verify_strict：额外拒绝弱公钥与非规范 R——发行方单源签发，零兼容负担
    match vk.verify_strict(&bundle_message(&m_now, &x_now), &sig) {
        Ok(()) => SigCheck::Valid,
        Err(_) => SigCheck::Invalid("ed25519 验签失败"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// 测试专用密钥对（**非**内置占位公钥——内置钥种子已弃，任何测试都不可能
    /// 用它签出 Valid；三态 Valid 腿经注入 `vk` 覆盖）。
    fn test_signing_key() -> SigningKey {
        // 固定 32 字节种子：测试可复现，与发行方钥无关
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn write_fixtures(dir: &Path) -> (PathBuf, PathBuf, Vec<u8>, Vec<u8>) {
        std::fs::create_dir_all(dir).expect("创建临时目录");
        let manifest = dir.join("com.example.x.json");
        let exe = dir.join("x.exe");
        let m_bytes = br#"{"schema_version":"1.0","id":"com.example.x"}"#.to_vec();
        let x_bytes = b"MZ fake exe bytes".to_vec();
        std::fs::write(&manifest, &m_bytes).expect("写清单");
        std::fs::write(&exe, &x_bytes).expect("写 exe");
        (manifest, exe, m_bytes, x_bytes)
    }

    fn make_ext(dir: &Path) -> LoadedExtension {
        use crate::manifest::{Entry, Manifest};
        use std::collections::BTreeMap;
        let manifest = dir.join("com.example.x.json");
        let exe = dir.join("x.exe");
        LoadedExtension {
            manifest: Manifest {
                schema_version: "1.0".to_string(),
                id: "com.example.x".to_string(),
                name: "Test".to_string(),
                version: "0.1.0".to_string(),
                description: String::new(),
                author: String::new(),
                license: String::new(),
                homepage: String::new(),
                icon: None,
                entry: Entry {
                    command: "${EXT_DIR}/x.exe".to_string(),
                    args: Vec::new(),
                    env: BTreeMap::new(),
                    cwd: None,
                },
                frozen: false,
                capabilities: Vec::new(),
                platforms: None,
                min_host_version: None,
            },
            path: manifest,
            dir: dir.to_path_buf(),
            command: exe,
            cwd: dir.to_path_buf(),
        }
    }

    fn sign_and_write(dir: &Path, sk: &SigningKey, m_hex: &str, x_hex: &str) -> PathBuf {
        let sig = sk.sign(&bundle_message(m_hex, x_hex));
        let sig_path = dir.join("com.example.x.json.sig");
        let body = format!(
            "untrusted comment: dd-run extension signature v1\n{}\ntrusted comment: manifest={m_hex} exe={x_hex}\n",
            encode_base64(&sig.to_bytes())
        );
        std::fs::write(&sig_path, body).expect("写 .sig");
        sig_path
    }

    pub(crate) fn encode_base64(b: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in b.chunks(3) {
            let mut buf = [0u8; 3];
            buf[..chunk.len()].copy_from_slice(chunk);
            out.push(TABLE[(buf[0] >> 2) as usize] as char);
            out.push(TABLE[((buf[0] & 0x03) << 4 | buf[1] >> 4) as usize] as char);
            out.push(if chunk.len() > 1 {
                TABLE[((buf[1] & 0x0F) << 2 | buf[2] >> 6) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                TABLE[(buf[2] & 0x3F) as usize] as char
            } else {
                '='
            });
        }
        out
    }

    #[test]
    fn built_in_key_parses() {
        // 占位公钥常量必须可解析（批次二轮换时防手误）
        assert!(built_in_verifying_key().is_some(), "内置公钥常量应可解析");
    }

    #[test]
    fn no_sig_file_is_absent() {
        let dir = std::env::temp_dir().join(format!("dd-sig-{}", std::process::id()));
        let (manifest, _, _, _) = write_fixtures(&dir);
        let ext = make_ext(&dir);
        let _ = manifest;
        let vk = test_signing_key().verifying_key();
        assert_eq!(check_signature(&ext, &vk), SigCheck::Absent);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn valid_sig_verifies() {
        let dir = std::env::temp_dir().join(format!("dd-sig-v-{}", std::process::id()));
        let (_, _, m, x) = write_fixtures(&dir);
        let ext = make_ext(&dir);
        let sk = test_signing_key();
        let m_hex = crate::trust::sha256_bytes(&m).expect("清单哈希");
        let x_hex = crate::trust::sha256_bytes(&x).expect("exe 哈希");
        let _ = sign_and_write(&dir, &sk, &m_hex, &x_hex);
        let vk = sk.verifying_key();
        let r = check_signature(&ext, &vk);
        if r != SigCheck::Valid {
            panic!("VALID 腿失败: {r:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tampered_manifest_fails() {
        let dir = std::env::temp_dir().join(format!("dd-sig-t-{}", std::process::id()));
        let (_, _, m, x) = write_fixtures(&dir);
        let ext = make_ext(&dir);
        let sk = test_signing_key();
        let m_hex = crate::trust::sha256_bytes(&m).expect("清单哈希");
        let x_hex = crate::trust::sha256_bytes(&x).expect("exe 哈希");
        let _ = sign_and_write(&dir, &sk, &m_hex, &x_hex);
        // 篡改清单一字节（验收判据 2：fail-closed）
        std::fs::write(
            &ext.path,
            b"{\"schema_version\":\"1.0\",\"id\":\"com.example.y\"}",
        )
        .expect("篡改清单");
        let vk = sk.verifying_key();
        assert!(
            matches!(check_signature(&ext, &vk), SigCheck::Invalid(_)),
            "篡改一字节应判 Invalid"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrong_key_fails() {
        let dir = std::env::temp_dir().join(format!("dd-sig-w-{}", std::process::id()));
        let (_, _, m, x) = write_fixtures(&dir);
        let ext = make_ext(&dir);
        let sk = test_signing_key();
        let m_hex = crate::trust::sha256_bytes(&m).expect("清单哈希");
        let x_hex = crate::trust::sha256_bytes(&x).expect("exe 哈希");
        let _ = sign_and_write(&dir, &sk, &m_hex, &x_hex);
        // 用另一把钥匙验
        let vk = SigningKey::from_bytes(&[9u8; 32]).verifying_key();
        assert!(
            matches!(check_signature(&ext, &vk), SigCheck::Invalid(_)),
            "错误公钥应判 Invalid"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_sig_fails() {
        let dir = std::env::temp_dir().join(format!("dd-sig-m-{}", std::process::id()));
        let (_, _, _, _) = write_fixtures(&dir);
        let ext = make_ext(&dir);
        std::fs::write(dir.join("com.example.x.json.sig"), "garbage").expect("写坏 .sig");
        let vk = test_signing_key().verifying_key();
        assert!(
            matches!(check_signature(&ext, &vk), SigCheck::Invalid(_)),
            "坏格式 .sig 应判 Invalid（fail-closed）"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
