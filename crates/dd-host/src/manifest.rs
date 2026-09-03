//! 扩展清单的加载、校验与扫描。
//!
//! 契约来源：[`docs/manifest-schema.md`](../../docs/manifest-schema.md)：
//! §2 文件位置与加载顺序、§3 字段表、§4 路径展开、§7 九条校验规则。
//!
//! 设计约束：
//! - **未知字段一律忽略**（§3 脚注与 §9）——由 serde 默认行为保证，不用
//!   `deny_unknown_fields`；
//! - **任一校验失败只跳过该扩展**，不影响其他扩展、不崩溃（§7 引言）。

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 宿主当前支持的清单格式版本（§3：v1.0 阶段恒为 `"1.0"`）。
pub const SCHEMA_VERSION: &str = "1.0";

/// 宿主可提供的 `host/*` 方法全集（§7 校验规则 9 的白名单，取值见协议 §1.3）。
pub const HOST_CAPABILITIES: [&str; 3] =
    ["host/show_status", "host/set_clipboard", "host/open_url"];

/// §3 清单文件（18 字段）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: String,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub entry: Entry,
    /// 顶层命令是否可缓存；缺省 `true`（§3）
    #[serde(default = "default_true")]
    pub frozen: bool,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_host_version: Option<String>,
}

fn default_true() -> bool {
    true
}

/// §3 `entry`：启动配置（`command` 为必填，其余可选）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// §7 校验失败原因，与九条规则一一对应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// 规则 1：不是合法 JSON
    ParseError(String),
    /// 规则 2：`schema_version` 不被宿主支持
    UnsupportedSchema { found: String, supported: String },
    /// 规则 3：五个必填字段缺失、为空或类型错误
    MissingField(String),
    /// 规则 4：`version`（或 `min_host_version`）不是合法 semver
    InvalidVersion(String),
    /// 规则 5：`platforms` 不含当前平台——**非错误**，静默跳过
    OtherPlatform { current: String },
    /// 规则 6：宿主版本低于 `min_host_version`
    HostTooOld { required: String, actual: String },
    /// 规则 7：`id` 在已加载集合中重复
    DuplicateId { id: String },
    /// 规则 8：展开后的 `entry.command` 不存在
    EntryNotExecutable(PathBuf),
    /// 规则 9：`capabilities` 含未知方法名
    UnknownCapability(String),
}

impl SkipReason {
    /// 规则 5 的"不含当前平台"是静默跳过（非错误），其余均为错误。
    pub fn is_error(&self) -> bool {
        !matches!(self, SkipReason::OtherPlatform { .. })
    }
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParseError(msg) => write!(f, "不是合法 JSON：{msg}"),
            Self::UnsupportedSchema { found, supported } => {
                write!(f, "不支持的清单格式 `{found}`（宿主支持 `{supported}`）")
            }
            Self::MissingField(msg) => write!(f, "必填字段缺失/为空/类型错误：{msg}"),
            Self::InvalidVersion(v) => {
                write!(f, "版本号不是合法 semver（MAJOR.MINOR.PATCH）：`{v}`")
            }
            Self::OtherPlatform { current } => {
                write!(f, "不支持当前平台 `{current}`（静默跳过，非错误）")
            }
            Self::HostTooOld { required, actual } => {
                write!(f, "宿主版本 {actual} 低于扩展要求的 {required}")
            }
            Self::DuplicateId { id } => write!(f, "扩展 id 重复：`{id}`"),
            Self::EntryNotExecutable(p) => write!(f, "entry.command 不存在：{}", p.display()),
            Self::UnknownCapability(name) => write!(f, "未知的 host/* 方法名：`{name}`"),
        }
    }
}

/// 通过全部校验、可被宿主加载的扩展。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedExtension {
    pub manifest: Manifest,
    /// 清单文件路径
    pub path: PathBuf,
    /// 清单所在目录，即路径展开中的 `${EXT_DIR}`
    pub dir: PathBuf,
    /// 展开并解析后的可执行文件（Windows 上可能补出 `.exe`）
    pub command: PathBuf,
    /// 展开后的子进程工作目录（缺省为清单所在目录）
    pub cwd: PathBuf,
}

/// 被跳过的清单文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedExtension {
    pub path: PathBuf,
    pub reason: SkipReason,
}

/// 一次扫描的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanOutcome {
    pub loaded: Vec<LoadedExtension>,
    pub skipped: Vec<SkippedExtension>,
    /// 扫描目录本身不可读。**非致命**——等同"该目录下没有扩展"。
    ///
    /// 契约：`io::ErrorKind::NotFound` **不写**——目录不存在（首跑场景）
    /// 视作空目录；其余错误（权限拒绝、损坏链接、磁盘 I/O 等）才写。
    /// 调用方据此把"目录不存在"与"目录出错"在 UI 上区分开。
    pub dir_error: Option<String>,
}

/// 扫描选项（把平台、宿主版本、home 目录显式化，便于测试）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOptions {
    /// `"windows"` / `"macos"` / `"linux"`
    pub platform: String,
    /// 宿主版本（semver），用于规则 6
    pub host_version: String,
    /// `~` 的展开基准
    pub home: PathBuf,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            platform: current_platform().to_string(),
            host_version: env!("CARGO_PKG_VERSION").to_string(),
            home: home_dir().unwrap_or_default(),
        }
    }
}

/// 当前平台标识（协议 §5.1 的 `host.platform` 与清单 `platforms` 同口径）。
pub fn current_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// home 目录。标准库无稳定 API，按平台读环境变量。
pub fn home_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// dd-run 数据根目录（三平台对称）：
/// Windows `%APPDATA%\dd-run`、macOS `~/Library/Application Support/dd-run`、
/// Linux `$XDG_CONFIG_HOME/dd-run`（缺省 `~/.config/dd-run`）。
fn dd_run_dir() -> Option<PathBuf> {
    let home = home_dir()?;
    if cfg!(windows) {
        let appdata = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"));
        Some(appdata.join("dd-run"))
    } else if cfg!(target_os = "macos") {
        Some(
            home.join("Library")
                .join("Application Support")
                .join("dd-run"),
        )
    } else {
        let xdg = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        Some(xdg.join("dd-run"))
    }
}

/// §2 三平台扩展目录 = 数据根目录下 `extensions.d`。
pub fn extensions_dir() -> Option<PathBuf> {
    dd_run_dir().map(|d| d.join("extensions.d"))
}

/// M3 磁盘桩缓存目录 = 数据根目录下 `cache`（`FrozenCache` 落盘位置，
/// 键 = 扩展 id + version，见 [`crate::cache`]）。
pub fn cache_dir() -> Option<PathBuf> {
    dd_run_dir().map(|d| d.join("cache"))
}

/// M5 批次 4.0：宿主本地配置文件 = 数据根目录下 `config.json`
/// （GUI 主题偏好等用户设置，见 `dd-gui::settings`）。
pub fn config_file() -> Option<PathBuf> {
    dd_run_dir().map(|d| d.join("config.json"))
}

/// §4 路径展开：`${EXT_DIR}` → 清单目录，`~` → home，相对路径 → 相对清单目录，
/// 绝对路径与 Windows 盘符路径原样返回。
pub fn expand_path(raw: &str, ext_dir: &Path, home: &Path) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("${EXT_DIR}") {
        let rest = rest.trim_start_matches(['/', '\\']);
        return if rest.is_empty() {
            ext_dir.to_path_buf()
        } else {
            ext_dir.join(rest)
        };
    }
    if raw == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        return home.join(rest);
    }
    if is_relative(raw) {
        ext_dir.join(raw)
    } else {
        PathBuf::from(raw)
    }
}

/// 相对路径判定：不以 `/` 或 `\` 开头，且不是 Windows 盘符路径（`C:\…` / `C:/…`）。
fn is_relative(raw: &str) -> bool {
    if raw.is_empty() {
        return true;
    }
    if raw.starts_with('/') || raw.starts_with('\\') {
        return false;
    }
    let mut chars = raw.chars();
    let is_drive = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && matches!(chars.next(), Some(':'));
    !is_drive
}

/// 解析可执行文件：§7 规则 8 只做**存在性检查**（可执行性留给首次 spawn 失败时判定，
/// 见 §7 脚注）。Windows 上按 `PATHEXT` 习惯补 `.exe`（cargo 产物带该后缀）。
pub fn resolve_executable(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    if cfg!(windows) {
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            for ext in [".exe", ".cmd", ".bat"] {
                let mut candidate = path.to_path_buf();
                candidate.set_file_name(format!("{name}{ext}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// 解析 `MAJOR.MINOR.PATCH` 三段纯数字版本号。
pub fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let mut parts = s.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// 加载并校验单个清单文件（§7 规则 1–6、8、9；规则 7 的 id 唯一性在 [`scan_dir`] 层做）。
pub fn load_manifest(path: &Path, opts: &ScanOptions) -> Result<LoadedExtension, SkipReason> {
    // 规则 1：文件可读且是合法 JSON
    let text = fs::read_to_string(path).map_err(|e| SkipReason::ParseError(e.to_string()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| SkipReason::ParseError(e.to_string()))?;

    // 规则 2：schema_version 存在且宿主支持
    let schema = value
        .get("schema_version")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if schema != SCHEMA_VERSION {
        return Err(SkipReason::UnsupportedSchema {
            found: schema.to_string(),
            supported: SCHEMA_VERSION.to_string(),
        });
    }

    // 规则 3：结构、字段类型（未知字段忽略）
    let manifest: Manifest =
        serde_json::from_value(value).map_err(|e| SkipReason::MissingField(e.to_string()))?;
    for (field, value) in [
        ("id", &manifest.id),
        ("name", &manifest.name),
        ("version", &manifest.version),
        ("entry.command", &manifest.entry.command),
    ] {
        if value.trim().is_empty() {
            return Err(SkipReason::MissingField(format!("`{field}` 为空")));
        }
    }

    // 规则 4：version 为合法 semver
    if parse_semver(&manifest.version).is_none() {
        return Err(SkipReason::InvalidVersion(manifest.version.clone()));
    }

    let dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    // 规则 5：platforms 未声明，或包含当前平台（不含则静默跳过）
    if let Some(platforms) = &manifest.platforms {
        if !platforms.iter().any(|p| p == &opts.platform) {
            return Err(SkipReason::OtherPlatform {
                current: opts.platform.clone(),
            });
        }
    }

    // 规则 6：min_host_version 未声明，或宿主版本 ≥ 该值
    if let Some(required) = &manifest.min_host_version {
        let required_v =
            parse_semver(required).ok_or_else(|| SkipReason::InvalidVersion(required.clone()))?;
        let actual_v = parse_semver(&opts.host_version).ok_or_else(|| {
            SkipReason::InvalidVersion(format!("宿主版本号非法：{}", opts.host_version))
        })?;
        if actual_v < required_v {
            return Err(SkipReason::HostTooOld {
                required: required.clone(),
                actual: opts.host_version.clone(),
            });
        }
    }

    // 规则 9：capabilities 不含未知方法名
    for cap in &manifest.capabilities {
        if !HOST_CAPABILITIES.contains(&cap.as_str()) {
            return Err(SkipReason::UnknownCapability(cap.clone()));
        }
    }

    // 规则 8：展开后的 entry.command 存在（可执行性留给 spawn 阶段）
    let command = resolve_executable(&expand_path(&manifest.entry.command, &dir, &opts.home))
        .ok_or_else(|| {
            SkipReason::EntryNotExecutable(expand_path(&manifest.entry.command, &dir, &opts.home))
        })?;

    let cwd = match &manifest.entry.cwd {
        Some(raw) => expand_path(raw, &dir, &opts.home),
        None => dir.clone(),
    };

    Ok(LoadedExtension {
        manifest,
        path: path.to_path_buf(),
        dir,
        command,
        cwd,
    })
}

/// 由可执行文件**直接构造**一份内存清单（不经磁盘扫描）。
///
/// 用途：宿主自检在示例清单的产物尚未就位时兜底（M0 `--roundtrip`）。
/// 它绕过 §7 校验——调用方必须在输出中说明这一点，避免把"内置兜底"
/// 误读为"扫描发现"。
pub fn from_executable(command: PathBuf, id: &str, name: &str) -> LoadedExtension {
    from_command(
        command,
        id,
        name,
        "0.0.0",
        true,
        &[],
        "内置兜底清单（非磁盘扫描产物）",
    )
}

/// 由可执行文件直接构造**内置扩展**的内存清单（P4 `ensure_builtins` 用）。
///
/// 与 [`from_executable`] 的区别：`frozen` / `capabilities` / `version` 由
/// 调用方（宿主内置注册表）显式给定——Apps 是 fresh（`frozen=false`，
/// 应用列表随安装/卸载变化，不落磁盘桩），Calc/System/WebSearch/Shell 可缓存。
/// `version` 参与磁盘桩缓存键（M3 `FrozenCache`），内置扩展升级时须同步。
pub fn from_builtin(
    command: PathBuf,
    id: &str,
    name: &str,
    frozen: bool,
    capabilities: &[&str],
    version: &str,
) -> LoadedExtension {
    from_command(
        command,
        id,
        name,
        version,
        frozen,
        capabilities,
        "内置扩展（宿主内存注册，非磁盘扫描产物）",
    )
}

/// [`from_executable`] / [`from_builtin`] 共用的构造体。
#[allow(clippy::too_many_arguments)]
fn from_command(
    command: PathBuf,
    id: &str,
    name: &str,
    version: &str,
    frozen: bool,
    capabilities: &[&str],
    description: &str,
) -> LoadedExtension {
    let dir = command
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    LoadedExtension {
        manifest: Manifest {
            schema_version: SCHEMA_VERSION.to_string(),
            id: id.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            description: description.to_string(),
            author: String::new(),
            license: String::new(),
            homepage: String::new(),
            icon: None,
            entry: Entry {
                command: command.to_string_lossy().to_string(),
                args: Vec::new(),
                env: BTreeMap::new(),
                cwd: None,
            },
            frozen,
            capabilities: capabilities.iter().map(|s| (*s).to_string()).collect(),
            platforms: None,
            min_host_version: None,
        },
        path: dir.join(format!("{id}.json")),
        dir: dir.clone(),
        command,
        cwd: dir,
    }
}

/// §2 扫描一个扩展目录：`*.json`、**不递归子目录**、按文件名字典序；
/// 逐条套用 §7 九条规则，失败的跳过。
///
/// 目录不存在（`io::ErrorKind::NotFound`，首跑场景）视作空目录——
/// 等同"该目录里没有扩展"，**不写** [`ScanOutcome::dir_error`]。
pub fn scan_dir(dir: &Path, opts: &ScanOptions) -> ScanOutcome {
    let mut outcome = ScanOutcome::default();

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // 首跑场景：扩展目录尚未创建，等同"该目录下没有扩展"，
            // 不污染 dir_error，让宿主 UI 无需为"不存在"显示异常 note。
            return outcome;
        }
        Err(e) => {
            outcome.dir_error = Some(format!("{}: {e}", dir.display()));
            return outcome;
        }
    };

    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    files.sort();

    let mut seen: HashSet<String> = HashSet::new();
    for file in files {
        match load_manifest(&file, opts) {
            Ok(ext) => {
                // 规则 7：id 唯一（后加载者跳过）
                if !seen.insert(ext.manifest.id.clone()) {
                    outcome.skipped.push(SkippedExtension {
                        path: file,
                        reason: SkipReason::DuplicateId {
                            id: ext.manifest.id.clone(),
                        },
                    });
                } else {
                    outcome.loaded.push(ext);
                }
            }
            Err(reason) => outcome
                .skipped
                .push(SkippedExtension { path: file, reason }),
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用临时目录（避免引入 tempfile 依赖）。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("系统时间早于 UNIX 纪元")
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("dd-run-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&dir).expect("创建临时目录");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, name: &str, content: &str) -> PathBuf {
            let path = self.0.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("创建父目录");
            }
            fs::write(&path, content).expect("写入文件");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn opts(platform: &str, host_version: &str) -> ScanOptions {
        ScanOptions {
            platform: platform.to_string(),
            host_version: host_version.to_string(),
            home: PathBuf::from("/home/tester"),
        }
    }

    /// 最小清单（§5），command 指向临时目录里的假可执行文件。
    fn minimal_manifest(id: &str, command: &str) -> String {
        format!(
            r#"{{"schema_version":"1.0","id":"{id}","name":"Sample","version":"1.0.0","entry":{{"command":"{command}"}}}}"#
        )
    }

    #[test]
    fn expands_ext_dir_token() {
        let dir = Path::new("/ext");
        let home = Path::new("/home/tester");
        assert_eq!(
            expand_path("${EXT_DIR}/bin/dd-run-calc", dir, home),
            PathBuf::from("/ext/bin/dd-run-calc")
        );
        assert_eq!(expand_path("${EXT_DIR}", dir, home), PathBuf::from("/ext"));
    }

    #[test]
    fn expands_tilde_to_home() {
        let dir = Path::new("/ext");
        let home = Path::new("/home/tester");
        assert_eq!(
            expand_path("~/tools/x", dir, home),
            PathBuf::from("/home/tester/tools/x")
        );
        assert_eq!(expand_path("~", dir, home), PathBuf::from("/home/tester"));
    }

    #[test]
    fn expands_relative_to_manifest_dir_and_keeps_absolute() {
        let dir = Path::new("/ext");
        let home = Path::new("/home/tester");
        assert_eq!(expand_path("bin/x", dir, home), PathBuf::from("/ext/bin/x"));
        assert_eq!(
            expand_path("/usr/local/bin/x", dir, home),
            PathBuf::from("/usr/local/bin/x")
        );
    }

    #[cfg(windows)]
    #[test]
    fn keeps_windows_drive_path_absolute() {
        let dir = Path::new("/ext");
        let home = Path::new("/home/tester");
        assert_eq!(
            expand_path("C:\\tools\\x", dir, home),
            PathBuf::from("C:\\tools\\x")
        );
    }

    #[test]
    fn semver_requires_three_numeric_parts() {
        assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("1.2"), None);
        assert_eq!(parse_semver("1.2.3.4"), None);
        assert_eq!(parse_semver("1.2.x"), None);
    }

    #[test]
    fn minimal_manifest_loads() {
        let tmp = TempDir::new("minimal");
        tmp.write("bin/ext", "#!/bin/sh\n");
        let path = tmp.write("a.json", &minimal_manifest("com.example.a", "bin/ext"));

        let ext = load_manifest(&path, &opts(current_platform(), "0.1.0")).expect("最小清单应通过");
        assert_eq!(ext.manifest.id, "com.example.a");
        assert!(ext.manifest.frozen, "frozen 缺省应为 true（§3）");
        assert_eq!(ext.cwd, tmp.path(), "cwd 缺省为清单所在目录");
        assert_eq!(ext.command, tmp.path().join("bin").join("ext"));
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let tmp = TempDir::new("unknown");
        tmp.write("bin/ext", "");
        let json = r#"{"schema_version":"1.0","id":"com.example.a","name":"A","version":"1.0.0",
            "entry":{"command":"bin/ext"},"future_field":{"anything":true}}"#;
        let path = tmp.write("a.json", json);

        load_manifest(&path, &opts(current_platform(), "0.1.0")).expect("未知字段应被忽略（§9）");
    }

    #[test]
    fn rule1_rejects_invalid_json() {
        let tmp = TempDir::new("rule1");
        let path = tmp.write("a.json", "{not json");

        let err = load_manifest(&path, &opts(current_platform(), "0.1.0")).unwrap_err();
        assert!(matches!(err, SkipReason::ParseError(_)), "got {err:?}");
    }

    #[test]
    fn rule2_rejects_unsupported_schema() {
        let tmp = TempDir::new("rule2");
        let path = tmp.write("a.json", r#"{"schema_version":"2.0","id":"a","name":"A","version":"1.0.0","entry":{"command":"x"}}"#);

        let err = load_manifest(&path, &opts(current_platform(), "0.1.0")).unwrap_err();
        assert_eq!(
            err,
            SkipReason::UnsupportedSchema {
                found: "2.0".into(),
                supported: "1.0".into()
            }
        );
    }

    #[test]
    fn rule3_rejects_missing_required_field() {
        let tmp = TempDir::new("rule3");
        // 缺 version
        let path = tmp.write(
            "a.json",
            r#"{"schema_version":"1.0","id":"a","name":"A","entry":{"command":"x"}}"#,
        );

        let err = load_manifest(&path, &opts(current_platform(), "0.1.0")).unwrap_err();
        assert!(matches!(err, SkipReason::MissingField(_)), "got {err:?}");
    }

    #[test]
    fn rule4_rejects_invalid_version() {
        let tmp = TempDir::new("rule4");
        let json = r#"{"schema_version":"1.0","id":"a","name":"A","version":"1.0","entry":{"command":"x"}}"#;
        let path = tmp.write("a.json", json);

        let err = load_manifest(&path, &opts(current_platform(), "0.1.0")).unwrap_err();
        assert_eq!(err, SkipReason::InvalidVersion("1.0".into()));
    }

    #[test]
    fn rule5_skips_other_platform_silently() {
        let tmp = TempDir::new("rule5");
        let json = r#"{"schema_version":"1.0","id":"a","name":"A","version":"1.0.0",
            "entry":{"command":"x"},"platforms":["macos"]}"#;
        let path = tmp.write("a.json", json);

        let err = load_manifest(&path, &opts("linux", "0.1.0")).unwrap_err();
        assert_eq!(
            err,
            SkipReason::OtherPlatform {
                current: "linux".into()
            }
        );
        assert!(!err.is_error(), "规则 5 是静默跳过，非错误（§7）");

        let tmp2 = TempDir::new("rule5b");
        tmp2.write("bin/ext", "");
        let json2 = r#"{"schema_version":"1.0","id":"a","name":"A","version":"1.0.0",
            "entry":{"command":"bin/ext"},"platforms":["macos","linux"]}"#;
        let path2 = tmp2.write("a.json", json2);
        assert!(load_manifest(&path2, &opts("linux", "0.1.0")).is_ok());
    }

    #[test]
    fn rule6_rejects_host_too_old() {
        let tmp = TempDir::new("rule6");
        tmp.write("bin/ext", "");
        let json = r#"{"schema_version":"1.0","id":"a","name":"A","version":"1.0.0",
            "entry":{"command":"bin/ext"},"min_host_version":"0.2.0"}"#;
        let path = tmp.write("a.json", json);

        let err = load_manifest(&path, &opts(current_platform(), "0.1.0")).unwrap_err();
        assert_eq!(
            err,
            SkipReason::HostTooOld {
                required: "0.2.0".into(),
                actual: "0.1.0".into()
            }
        );

        let ok = load_manifest(&path, &opts(current_platform(), "0.2.0"));
        assert!(ok.is_ok(), "宿主版本达标时应通过");
    }

    #[test]
    fn rule7_rejects_duplicate_id_and_keeps_first() {
        let tmp = TempDir::new("rule7");
        tmp.write("bin/ext", "");
        tmp.write("a.json", &minimal_manifest("com.example.a", "bin/ext"));
        tmp.write("b.json", &minimal_manifest("com.example.a", "bin/ext"));

        let outcome = scan_dir(tmp.path(), &opts(current_platform(), "0.1.0"));
        assert_eq!(outcome.loaded.len(), 1);
        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(
            outcome.skipped[0].reason,
            SkipReason::DuplicateId {
                id: "com.example.a".into()
            }
        );
        assert!(outcome.skipped[0].path.ends_with("b.json"));
    }

    #[test]
    fn rule8_rejects_missing_entry_command() {
        let tmp = TempDir::new("rule8");
        let path = tmp.write("a.json", &minimal_manifest("com.example.a", "bin/nope"));

        let err = load_manifest(&path, &opts(current_platform(), "0.1.0")).unwrap_err();
        let expected = tmp.path().join("bin").join("nope");
        assert_eq!(err, SkipReason::EntryNotExecutable(expected));
    }

    #[test]
    fn rule9_rejects_unknown_capability() {
        let tmp = TempDir::new("rule9");
        tmp.write("bin/ext", "");
        let json = r#"{"schema_version":"1.0","id":"a","name":"A","version":"1.0.0",
            "entry":{"command":"bin/ext"},"capabilities":["host/show_status","host/launch_nuke"]}"#;
        let path = tmp.write("a.json", json);

        let err = load_manifest(&path, &opts(current_platform(), "0.1.0")).unwrap_err();
        assert_eq!(
            err,
            SkipReason::UnknownCapability("host/launch_nuke".into())
        );
    }

    #[test]
    fn scan_is_sorted_and_non_recursive_and_json_only() {
        let tmp = TempDir::new("scan");
        tmp.write("bin/ext", "");
        tmp.write("b.json", &minimal_manifest("com.example.b", "bin/ext"));
        tmp.write("a.json", &minimal_manifest("com.example.a", "bin/ext"));
        tmp.write(
            "nested/c.json",
            &minimal_manifest("com.example.c", "bin/ext"),
        );
        tmp.write("readme.txt", "not a manifest");

        let outcome = scan_dir(tmp.path(), &opts(current_platform(), "0.1.0"));
        let ids: Vec<&str> = outcome
            .loaded
            .iter()
            .map(|e| e.manifest.id.as_str())
            .collect();
        // 字典序：a.json 先于 b.json；子目录与非 json 文件不参与
        assert_eq!(ids, vec!["com.example.a", "com.example.b"]);
        assert!(outcome.skipped.is_empty());
        assert!(outcome.dir_error.is_none());
    }

    #[test]
    fn scan_treats_missing_dir_as_empty() {
        // 首跑契约：扩展目录尚未创建 = "该目录下没有扩展"，不写 dir_error。
        // 旧断言曾是 `dir_error.is_some()`，与新契约冲突（仅非 NotFound 才写）。
        let tmp = TempDir::new("missing");
        let missing = tmp.path().join("nope");

        let outcome = scan_dir(&missing, &ScanOptions::default());
        assert!(
            outcome.loaded.is_empty(),
            "目录不存在应视作空（无人可加载）"
        );
        assert!(outcome.skipped.is_empty(), "目录不存在不应触发任何 skip");
        assert!(
            outcome.dir_error.is_none(),
            "目录不存在（NotFound）≠错误；权限拒绝等其它错误才写 dir_error"
        );
    }

    #[test]
    fn scan_reports_non_not_found_dir_error() {
        // 锁住契约"仅非 NotFound 写 dir_error"——这里我们制造一个**路径合法但
        // 不可读**的场景（链向一个文件而非目录），让 Windows/Linux 都返回
        // `ErrorKind::Other` 而非 `NotFound`。
        let tmp = TempDir::new("nonread");
        let file_path = tmp.path().join("not-a-dir.json");
        std::fs::write(&file_path, "{}").expect("写一个真文件");

        let outcome = scan_dir(&file_path, &ScanOptions::default());
        assert!(outcome.loaded.is_empty());
        assert!(
            outcome.dir_error.is_some(),
            "非 NotFound 错误（如把文件当目录读）必须写 dir_error"
        );
        assert!(
            outcome
                .dir_error
                .as_deref()
                .unwrap()
                .contains("not-a-dir.json"),
            "dir_error 应包含具体路径以便诊断"
        );
    }
}
