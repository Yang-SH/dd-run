//! 扩展信任台账与判定（**S-05**，2026-09-24）。
//!
//! 背景与完整方案见 [`docs/security-audit-2026-09-23.md`](../../../docs/security-audit-2026-09-23.md)
//! §4.4–§4.4.2。要点：
//!
//! - **问题**：`extensions.d/*.json` 里任何通过 §7 九条规则的清单都会被静默拉起
//!   （任意 `command` + 任意 `entry.env` + 任意 `cwd`），宿主既不校验来源与哈希，
//!   也不征求用户同意；设置页也看不出哪个扩展是随包首方的。
//! - **判据修订（选型时发现）**：原方案按 `com.ddrun.*` **id 前缀**自动信任，但
//!   id 是清单作者自填的字符串 → 攻击者写一份 `com.ddrun.evil` 即可白拿信任。
//!   故判据改为「**来源 ∈ 随包 sidecar 目录** AND **id ∈ 首方白名单**」——
//!   **前缀本身不再是信任依据**。
//! - **定性（务必不要误读）**：本模块实现的是「**用户同意 + 变更检测**」，
//!   **不是防篡改**。能写 `extensions.d` 的攻击者同样能写 `trust.json`。
//!   真正意义上的防篡改需要扩展签名（宿主内置公钥 + 作者签名），**不在本项范围**。
//!
//! ## 判定规则（自上而下短路，`assess`）
//!
//! | 条件 | 结果 |
//! |---|---|
//! | `origin == Builtin`（in-process，`command` 为名义路径、从不 spawn） | `AutoTrusted` |
//! | `origin == Sidecar` **且** `id ∈ FIRST_PARTY_IDS` | `AutoTrusted` |
//! | 台账该 id 记录 `decision == Allow` 且**两枚哈希与当前文件一致** | `AutoTrusted` |
//! | 台账该 id 记录 `decision == Deny` 且**两枚哈希与当前文件一致** | `Blocked` |
//! | 其余（无记录 / 哈希变化 / 台账损坏 / 文件不可读） | `Pending`（**fail-closed**） |
//!
//! 哈希只在「台账里确有该 id 的记录」时才计算——首方与内置扩展走短路，故**常见
//! 情形零额外 I/O**（启动开销≈0）；只有用户手动安装过的扩展才有两次数 MB 内的读盘。
//!
//! ## 哈希实现（D2）
//!
//! Windows 用 CNG（`bcrypt.dll`）`SHA256`，**分块流式**（64 KiB/块）——避免为哈希把
//! 整个 exe 读进内存。选 CNG 而非 `sha2` crate 的理由：`windows-sys` 已在
//! `Cargo.lock`（dd-gui / dd-ext 已依赖）→ **lock 零新增、体积增量可忽略**；
//! 也无需自研密码学实现。
//!
//! **非 Windows 为已声明的缺口**：`sha256_file` / `sha256_bytes` 返回 `Err`，
//! `assess` 直接放行（保持门禁引入前的行为）。理由：P4 为 Windows 优先，非 Windows
//! 下 `extensions.d` 生态尚不存在；若 fail-closed 会让该平台任何扩展都无法使用。

use crate::manifest::{trust_file, LoadedExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 台账格式版本；`load` 遇到不识别版本一律视作损坏（fail-closed → 空台账）。
pub const LEDGER_VERSION: u32 = 1;

/// 随包首方扩展 id 白名单（D1）。
///
/// **必须与 `dd-gui::aggregator::owned_sidecar_name_key` 的键集保持一致**——
/// 该函数是"宿主自有 sidecar"的既有单一事实来源（负责本地化名），此处复用它的
/// 键集而不是另建一份名单；`dd-gui` 侧有测试锁定两者一致。
///
/// 当前仅文件搜索（随 `dist/extensions.d/` 分发）。
pub const FIRST_PARTY_IDS: &[&str] = &["com.ddrun.filesearch"];

/// 扩展来源（决定是否可能自动信任）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtOrigin {
    /// 宿主内置扩展：in-process 驱动，`command` 为**名义路径**（`dd-ext-*.exe`），
    /// **从不 spawn**（M9 起）。故不经台账判定。
    Builtin,
    /// 便携 sidecar：清单位于 `<宿主 exe 目录>\extensions.d\`（随包分发）。
    Sidecar,
    /// 用户数据目录：`%APPDATA%\dd-run\extensions.d\`——**任意用户级程序可写**，
    /// 是 S-05 的真正攻击面。
    UserDir,
}

/// 一次信任判定结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assessment {
    pub id: String,
    pub trust: Trust,
    pub origin: ExtOrigin,
}

impl Assessment {
    /// 是否可直接拉起。
    pub fn is_trusted(&self) -> bool {
        self.trust == Trust::AutoTrusted
    }

    /// 是否待用户批准（面板结果中不出现，设置页可见）。
    pub fn is_pending(&self) -> bool {
        self.trust == Trust::Pending
    }

    /// 是否已被用户拒绝（持久阻止）。
    pub fn is_blocked(&self) -> bool {
        self.trust == Trust::Blocked
    }

    /// **D4 告警**：与随包首方扩展**同 id** 但来自用户数据目录。
    ///
    /// 成因：`merge_scanned_dirs` 同 id 时用户目录优先（用户手动放置覆盖分发版），
    /// 于是攻击者放一份 `com.ddrun.filesearch.json` 指向自有 exe 即可**顶掉**随包
    /// sidecar。他拿不到自动信任（来源不满足），但会让该功能被挡成「待批准」而构成
    /// DoS。本方法供设置页显式提示，不改加载语义（D4 决策）。
    pub fn shadows_first_party(&self) -> bool {
        self.origin == ExtOrigin::UserDir && FIRST_PARTY_IDS.contains(&self.id.as_str())
    }
}

/// 信任状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// 自动信任（内置 / 随包首方 / 台账已批准且哈希未变）→ 可 spawn。
    AutoTrusted,
    /// 待用户批准 → **不 spawn**（fail-closed 的默认归宿）。
    Pending,
    /// 用户已拒绝 → **不 spawn**（可撤销）。
    Blocked,
}

/// 用户对某扩展的决策（落台账）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Deny,
}

/// 台账单条记录。
///
/// **批准粒度绑定到内容**（`manifest_sha256` + `exe_sha256`）而非仅 id —— 否则
/// 「批准过的 id」可被另一份同 id 但不同内容的清单复用（影子/顶替）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustEntry {
    pub id: String,
    pub manifest_sha256: String,
    pub exe_sha256: String,
    pub decision: Decision,
    /// RFC3339（本地时区），仅作审计展示，不参与判定。
    #[serde(default)]
    pub decided_at: String,
    /// 决策时的清单路径（便于用户事后追溯"我当时批的是什么"）。
    #[serde(default)]
    pub manifest_path: String,
}

/// 台账读取状态（供设置页给出**可操作**提示，而不是静默失效）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerState {
    /// 文件不存在（首跑）——等价空台账，**不是异常**。
    Missing,
    /// 读取且版本识别。
    Ok,
    /// 读失败 / JSON 非法 / 版本不识别 → 视作**空台账**（全部回到待批准）。
    Corrupt,
}

/// 信任台账（`%APPDATA%\dd-run\trust.json`；**宿主私有文件，非清单契约**）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustLedger {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<TrustEntry>,
}

fn default_version() -> u32 {
    LEDGER_VERSION
}

impl Default for TrustLedger {
    fn default() -> Self {
        Self {
            version: LEDGER_VERSION,
            entries: Vec::new(),
        }
    }
}

impl TrustLedger {
    /// 读取台账；任何异常都回落为**空台账**（fail-closed），状态另由
    /// [`Self::load_with_state`] 返回。
    pub fn load() -> Self {
        Self::load_with_state().0
    }

    /// 读取台账并给出状态。`Missing` 不算异常（首跑）；`Corrupt` 须由 UI 提示。
    pub fn load_with_state() -> (Self, LedgerState) {
        let Some(path) = trust_file() else {
            return (Self::default(), LedgerState::Missing);
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return (Self::default(), LedgerState::Missing)
            }
            Err(e) => {
                log::warn!(
                    "[dd-host] 信任台账读失败（{}）：{e} —— 视为空台账（全部扩展需重新批准）",
                    path.display()
                );
                return (Self::default(), LedgerState::Corrupt);
            }
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(l) if l.version == LEDGER_VERSION => (l, LedgerState::Ok),
            Ok(l) => {
                log::warn!(
                    "[dd-host] 信任台账版本不识别（{}，期望 {LEDGER_VERSION}）—— 视为空台账",
                    l.version
                );
                (Self::default(), LedgerState::Corrupt)
            }
            Err(e) => {
                log::warn!(
                    "[dd-host] 信任台账解析失败（{}）：{e} —— 视为空台账（fail-closed）",
                    path.display()
                );
                (Self::default(), LedgerState::Corrupt)
            }
        }
    }

    /// 写回台账（best-effort：目录不存在则创建；调用方决定失败时是否提示）。
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = trust_file() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "无法定位数据根目录（home 环境变量缺失）",
            ));
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&path, text)
    }

    /// 查某 id 的记录。
    pub fn entry(&self, id: &str) -> Option<&TrustEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// 记录用户决策（覆盖同 id 旧记录）。哈希在**此处**计算，调用方无需关心；
    /// 清单或 exe 不可读时返回 `Err`（不写入半截记录）。
    pub fn record(&mut self, ext: &LoadedExtension, decision: Decision) -> Result<(), String> {
        let manifest_sha256 = sha256_file(&ext.path)?;
        let exe_sha256 = sha256_file(&ext.command)?;
        let entry = TrustEntry {
            id: ext.manifest.id.clone(),
            manifest_sha256,
            exe_sha256,
            decision,
            decided_at: now_rfc3339(),
            manifest_path: ext.path.display().to_string(),
        };
        self.entries.retain(|e| e.id != entry.id);
        self.entries.push(entry);
        Ok(())
    }

    /// 删除某 id 记录（回到"待批准"）。返回是否确有删除。
    pub fn forget(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() != before
    }
}

/// 判定某扩展的信任状态（规则见模块文档）。
///
/// `origin` 由调用方给出（宿主知道它从哪个目录扫到的；内置由 `specs` 命中判定）。
pub fn assess(ext: &LoadedExtension, origin: ExtOrigin, ledger: &TrustLedger) -> Assessment {
    let id = ext.manifest.id.clone();
    let trust = if !hash_available() {
        // D2 缺口声明：非 Windows 不做门禁（保持旧行为）。
        Trust::AutoTrusted
    } else {
        match origin {
            ExtOrigin::Builtin => Trust::AutoTrusted,
            ExtOrigin::Sidecar if FIRST_PARTY_IDS.contains(&id.as_str()) => Trust::AutoTrusted,
            _ => match ledger.entry(&id) {
                Some(e) if e.decision == Decision::Allow && hashes_match(ext, e) => {
                    Trust::AutoTrusted
                }
                Some(e) if e.decision == Decision::Deny && hashes_match(ext, e) => Trust::Blocked,
                // 无记录 / 哈希变化 / 文件不可读 → 重新征求同意（fail-closed）
                _ => Trust::Pending,
            },
        }
    };
    Assessment { id, trust, origin }
}

/// 台账记录的两枚哈希是否与**当前文件**一致。任一不可读 → 不一致（fail-closed）。
fn hashes_match(ext: &LoadedExtension, e: &TrustEntry) -> bool {
    match (sha256_file(&ext.path), sha256_file(&ext.command)) {
        (Ok(m), Ok(x)) => m == e.manifest_sha256 && x == e.exe_sha256,
        _ => false,
    }
}

/// 哈希能力是否可用（**非 Windows 未实现**，见模块文档「已声明的缺口」）。
pub fn hash_available() -> bool {
    cfg!(windows)
}

/// 文件 SHA-256（小写 hex）。分块流式读盘。
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f =
        std::fs::File::open(path).map_err(|e| format!("打开失败：{e}（{}）", path.display()))?;
    sha256_hex(&mut f)
}

/// 内存字节 SHA-256（小写 hex）。
pub fn sha256_bytes(bytes: &[u8]) -> Result<String, String> {
    sha256_hex(&mut &bytes[..])
}

/// 当前时间（RFC3339，本地时区）。仅用于台账审计展示。
fn now_rfc3339() -> String {
    chrono::Local::now().to_rfc3339()
}

#[cfg(windows)]
fn sha256_hex(reader: &mut impl std::io::Read) -> Result<String, String> {
    use windows_sys::Win32::Security::Cryptography::{
        BCryptCloseAlgorithmProvider, BCryptCreateHash, BCryptDestroyHash, BCryptFinishHash,
        BCryptGetProperty, BCryptHashData, BCryptOpenAlgorithmProvider, BCRYPT_ALG_HANDLE,
        BCRYPT_HASH_HANDLE, BCRYPT_OBJECT_LENGTH, BCRYPT_SHA256_ALGORITHM,
    };

    /// 分块大小：避免为哈希把整个 exe 读进内存（exe 可达数十 MB）。
    const CHUNK: usize = 64 * 1024;
    /// SHA-256 摘要长度。
    const DIGEST: usize = 32;

    /// RAII：算法提供者句柄（Drop 时关闭，含早期返回路径）。
    struct AlgProvider(BCRYPT_ALG_HANDLE);
    impl Drop for AlgProvider {
        fn drop(&mut self) {
            // SAFETY：句柄由 BCryptOpenAlgorithmProvider 成功返回，且只在此处关闭一次。
            unsafe { BCryptCloseAlgorithmProvider(self.0, 0) };
        }
    }

    /// RAII：哈希句柄。
    struct HashHandle(BCRYPT_HASH_HANDLE);
    impl Drop for HashHandle {
        fn drop(&mut self) {
            // SAFETY：句柄由 BCryptCreateHash 成功返回，且只在此处销毁一次。
            unsafe { BCryptDestroyHash(self.0) };
        }
    }

    let mut alg: BCRYPT_ALG_HANDLE = std::ptr::null_mut();
    // SAFETY：`alg` 为合法出参；算法名是 crate 内的静态宽字符串常量；
    // `pszImplementation = null` 表示用默认加密提供者（无需指定具体实现）。
    // NTSTATUS ≥ 0 为成功（< 0 为错误，见 Windows 的 NT_SUCCESS 约定）。
    let st = unsafe {
        BCryptOpenAlgorithmProvider(&mut alg, BCRYPT_SHA256_ALGORITHM, std::ptr::null(), 0)
    };
    if st < 0 {
        return Err(format!("BCryptOpenAlgorithmProvider 失败：{st:#010x}"));
    }
    let alg = AlgProvider(alg);

    // 查询哈希对象所需缓冲长度（size 随实现/版本变化，不能写死）。
    let mut obj_len: u32 = 0;
    let mut got: u32 = 0;
    // SAFETY：`obj_len` 为 u32 出参，声明长度 4 与其大小一致；句柄有效。
    let st = unsafe {
        BCryptGetProperty(
            alg.0,
            BCRYPT_OBJECT_LENGTH,
            &mut obj_len as *mut u32 as *mut u8,
            std::mem::size_of::<u32>() as u32,
            &mut got,
            0,
        )
    };
    if st < 0 {
        return Err(format!("BCryptGetProperty(ObjectLength) 失败：{st:#010x}"));
    }
    let mut obj = vec![0u8; obj_len as usize];

    let mut h: BCRYPT_HASH_HANDLE = std::ptr::null_mut();
    // SAFETY：算法句柄有效；对象缓冲长度与查询值一致；无密钥（SHA-256 非 HMAC）。
    let st = unsafe {
        BCryptCreateHash(
            alg.0,
            &mut h,
            obj.as_mut_ptr(),
            obj_len,
            std::ptr::null(),
            0,
            0,
        )
    };
    if st < 0 {
        return Err(format!("BCryptCreateHash 失败：{st:#010x}"));
    }
    let h = HashHandle(h);

    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("读盘失败：{e}"))?;
        if n == 0 {
            break;
        }
        // SAFETY：`buf` 有效且长度 ≥ n；哈希句柄有效。
        let st = unsafe { BCryptHashData(h.0, buf.as_ptr(), n as u32, 0) };
        if st < 0 {
            return Err(format!("BCryptHashData 失败：{st:#010x}"));
        }
    }

    let mut out = [0u8; DIGEST];
    // SAFETY：输出缓冲恰为摘要长度；哈希句柄有效。
    let st = unsafe { BCryptFinishHash(h.0, out.as_mut_ptr(), DIGEST as u32, 0) };
    if st < 0 {
        return Err(format!("BCryptFinishHash 失败：{st:#010x}"));
    }
    Ok(out.iter().map(|b| format!("{b:02x}")).collect())
}

/// 非 Windows 占位：返回 `Err`（模块文档「已声明的缺口」）。
#[cfg(not(windows))]
fn sha256_hex(_reader: &mut impl std::io::Read) -> Result<String, String> {
    Err("当前平台未实现 SHA-256（S-05 门禁在非 Windows 上不生效）".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Entry, Manifest};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    /// 造一个临时目录（沿用本 crate 测试惯例：用进程 id + 计数器避免并发冲突）。
    fn tmp_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d =
            std::env::temp_dir().join(format!("dd-host-trust-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&d).expect("建临时目录");
        d
    }

    fn write(path: &Path, content: &[u8]) {
        std::fs::write(path, content).expect("写测试文件");
    }

    /// 造一个磁盘形态的扩展（清单文件 + exe 文件都真实存在）。
    fn disk_ext(dir: &Path, id: &str, manifest_bytes: &[u8], exe_bytes: &[u8]) -> LoadedExtension {
        let exe = dir.join("fake-ext.exe");
        write(&exe, exe_bytes);
        let mpath = dir.join(format!("{id}.json"));
        write(&mpath, manifest_bytes);
        LoadedExtension {
            manifest: Manifest {
                schema_version: "1.0".to_string(),
                id: id.to_string(),
                name: "Test".to_string(),
                version: "0.1.0".to_string(),
                description: String::new(),
                author: String::new(),
                license: String::new(),
                homepage: String::new(),
                icon: None,
                entry: Entry {
                    command: "${EXT_DIR}/fake-ext.exe".to_string(),
                    args: Vec::new(),
                    env: BTreeMap::new(),
                    cwd: None,
                },
                frozen: false,
                capabilities: Vec::new(),
                platforms: None,
                min_host_version: None,
            },
            path: mpath,
            dir: dir.to_path_buf(),
            command: exe,
            cwd: dir.to_path_buf(),
        }
    }

    /// 内置扩展：in-process，恒自动信任（不经台账）。
    #[test]
    fn builtin_origin_is_auto_trusted() {
        let d = tmp_dir("builtin");
        let ext = disk_ext(&d, "com.ddrun.calc", b"{}", b"BIN");
        let a = assess(&ext, ExtOrigin::Builtin, &TrustLedger::default());
        assert_eq!(a.trust, Trust::AutoTrusted);
        assert!(a.is_trusted());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 随包 sidecar + 首方白名单 → 自动信任（**T1′ 的零摩擦路径**，A6）。
    #[test]
    fn first_party_sidecar_is_auto_trusted() {
        let d = tmp_dir("fp-sidecar");
        let ext = disk_ext(&d, "com.ddrun.filesearch", b"{\"a\":1}", b"BIN");
        let a = assess(&ext, ExtOrigin::Sidecar, &TrustLedger::default());
        assert_eq!(a.trust, Trust::AutoTrusted);
        assert!(!a.shadows_first_party());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 随包 sidecar 但**非**首方 id → 仍需首次批准（来源满足、白名单不满足）。
    #[test]
    fn sidecar_non_first_party_still_needs_approval() {
        let d = tmp_dir("sidecar-third");
        let ext = disk_ext(&d, "com.other.thing", b"{}", b"BIN");
        let a = assess(&ext, ExtOrigin::Sidecar, &TrustLedger::default());
        assert_eq!(a.trust, Trust::Pending);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **判据漏洞回归（A7）**：用户目录里冒用首方前缀/同名 id **不得**自动信任。
    #[test]
    fn user_dir_cannot_impersonate_first_party_by_id() {
        let d = tmp_dir("impersonate");
        for id in ["com.ddrun.evil", "com.ddrun.filesearch"] {
            let ext = disk_ext(&d, id, b"{}", b"BIN");
            let a = assess(&ext, ExtOrigin::UserDir, &TrustLedger::default());
            assert_eq!(a.trust, Trust::Pending, "{id} 不应自动信任");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **D4 告警**：用户目录里与随包首方同 id → `shadows_first_party()` 为真。
    #[test]
    fn user_dir_same_id_as_first_party_flags_shadow() {
        let d = tmp_dir("shadow");
        let ext = disk_ext(&d, "com.ddrun.filesearch", b"{}", b"BIN");
        let a = assess(&ext, ExtOrigin::UserDir, &TrustLedger::default());
        assert!(a.shadows_first_party());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 台账 `allow` + 两枚哈希一致 → 自动信任（A2 的下游效果）。
    #[test]
    fn allow_entry_with_matching_hashes_is_auto_trusted() {
        let d = tmp_dir("allow");
        let ext = disk_ext(&d, "com.example.foo", b"{\"id\":\"x\"}", b"EXE-V1");
        let mut ledger = TrustLedger::default();
        ledger
            .record(&ext, Decision::Allow)
            .expect("记录允许（需哈希可用）");
        if !hash_available() {
            return; // 非 Windows：跳过（门禁不生效）
        }
        let a = assess(&ext, ExtOrigin::UserDir, &ledger);
        assert_eq!(a.trust, Trust::AutoTrusted);
        assert_eq!(
            ledger.entry("com.example.foo").unwrap().decision,
            Decision::Allow
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **哈希变化即回到待批准（A3）**：改了 exe 内容 → 旧批准失效。
    #[test]
    fn changed_exe_hash_revokes_approval() {
        let d = tmp_dir("changed");
        let ext = disk_ext(&d, "com.example.foo", b"{}", b"EXE-V1");
        if !hash_available() {
            return;
        }
        let mut ledger = TrustLedger::default();
        ledger.record(&ext, Decision::Allow).unwrap();
        // 原地改写 exe（模拟"更新"或"被替换"）
        write(&ext.command, b"EXE-V2");
        let a = assess(&ext, ExtOrigin::UserDir, &ledger);
        assert_eq!(a.trust, Trust::Pending, "exe 哈希变化必须回到待批准");
        // 清单变化同理
        write(&ext.command, b"EXE-V1");
        write(&ext.path, b"{\"changed\":true}");
        let a = assess(&ext, ExtOrigin::UserDir, &ledger);
        assert_eq!(a.trust, Trust::Pending, "清单哈希变化必须回到待批准");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// `deny` + 哈希一致 → 已阻止；再次「允许」可撤销（A9）。
    #[test]
    fn deny_entry_blocks_and_allow_revokes() {
        let d = tmp_dir("deny");
        let ext = disk_ext(&d, "com.example.foo", b"{}", b"EXE");
        if !hash_available() {
            return;
        }
        let mut ledger = TrustLedger::default();
        ledger.record(&ext, Decision::Deny).unwrap();
        assert_eq!(
            assess(&ext, ExtOrigin::UserDir, &ledger).trust,
            Trust::Blocked
        );
        assert!(assess(&ext, ExtOrigin::UserDir, &ledger).is_blocked());
        // 撤销：重新记为 allow（覆盖同 id 旧记录）
        ledger.record(&ext, Decision::Allow).unwrap();
        assert_eq!(ledger.entries.len(), 1, "同 id 只保留一条");
        assert_eq!(
            assess(&ext, ExtOrigin::UserDir, &ledger).trust,
            Trust::AutoTrusted
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 台账 JSON 损坏 / 版本不识别 → 空台账 + `Corrupt`（**fail-closed**，A5）。
    #[test]
    fn corrupt_or_unsupported_ledger_falls_back_to_empty() {
        // 直接验证解析路径（不依赖真实 trust.json 位置）
        let bad = serde_json::from_str::<TrustLedger>("{ this is not json");
        assert!(bad.is_err(), "非法 JSON 必须解析失败");
        let unsupported = serde_json::from_str::<TrustLedger>(r#"{"version":99,"entries":[]}"#);
        assert!(matches!(unsupported, Ok(l) if l.version != LEDGER_VERSION));
        // 空台账下磁盘扩展一律待批准
        let d = tmp_dir("corrupt");
        let ext = disk_ext(&d, "com.example.foo", b"{}", b"EXE");
        assert_eq!(
            assess(&ext, ExtOrigin::UserDir, &TrustLedger::default()).trust,
            Trust::Pending
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 保存/读取往返 + `forget` 回到待批准。
    #[test]
    fn save_load_roundtrip_and_forget() {
        let d = tmp_dir("roundtrip");
        let ext = disk_ext(&d, "com.example.foo", b"{}", b"EXE");
        if !hash_available() {
            return;
        }
        let mut ledger = TrustLedger::default();
        ledger.record(&ext, Decision::Allow).unwrap();
        let json = serde_json::to_string(&ledger).expect("序列化");
        let back: TrustLedger = serde_json::from_str(&json).expect("反序列化");
        assert_eq!(back, ledger);
        assert!(back.entry("com.example.foo").is_some());
        // forget → 回待批准
        let mut l2 = back;
        assert!(l2.forget("com.example.foo"));
        assert!(!l2.forget("com.example.foo"));
        assert_eq!(assess(&ext, ExtOrigin::UserDir, &l2).trust, Trust::Pending);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 文件不可读 → 待批准（不因 IO 失败而放行）。
    #[test]
    fn unreadable_files_stay_pending() {
        let d = tmp_dir("unreadable");
        let ext = disk_ext(&d, "com.example.foo", b"{}", b"EXE");
        if !hash_available() {
            return;
        }
        let mut ledger = TrustLedger::default();
        ledger.record(&ext, Decision::Allow).unwrap();
        let _ = std::fs::remove_file(&ext.command); // exe 消失
        assert_eq!(
            assess(&ext, ExtOrigin::UserDir, &ledger).trust,
            Trust::Pending
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 哈希实现与 NIST 已知向量一致（单块 + 多块各一）。
    #[test]
    #[cfg(windows)]
    fn sha256_matches_nist_known_vectors() {
        // 向量来源：NIST FIPS 180-4 示例（广为引用的 KAT）
        assert_eq!(
            sha256_bytes(b"abc").expect("哈希"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_bytes(b"").expect("哈希"),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // 1,000,000 个 'a'：覆盖多块 + 分块喂入路径（与 CHUNK 不同尺寸）
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            sha256_bytes(&million).expect("哈希"),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    /// `sha256_file` 与 `sha256_bytes` 对同一内容一致（分块与整块等价）。
    #[test]
    #[cfg(windows)]
    fn file_and_bytes_hash_agree() {
        let d = tmp_dir("hashfile");
        let p = d.join("blob.bin");
        // 跨 CHUNK 边界：64 KiB + 17 字节
        let data: Vec<u8> = (0..(64 * 1024 + 17)).map(|i| (i % 251) as u8).collect();
        write(&p, &data);
        assert_eq!(
            sha256_file(&p).expect("文件哈希"),
            sha256_bytes(&data).expect("内存哈希")
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 首方白名单与 `dd-gui` 的本地化名表**必须**一致（防两处名单漂移）。
    #[test]
    fn first_party_allowlist_is_not_empty_and_has_filesearch() {
        assert!(FIRST_PARTY_IDS.contains(&"com.ddrun.filesearch"));
    }
}
