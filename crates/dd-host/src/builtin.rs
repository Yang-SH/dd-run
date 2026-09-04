//! 内置扩展注册表与内存自注册（P4 `ensure_builtins`）。
//!
//! 契约来源：
//! - [`docs/manifest-schema.md`](../../docs/manifest-schema.md) §10：内置扩展
//!   **同样走清单注册**（与第三方无特判代码）；MVP 无安装器，故由宿主在启动时
//!   **内存注册**——把 `exe_dir` 目录里的 `dd-ext-*.exe` 直接构造为
//!   [`LoadedExtension`]，等效于"安装器写好了清单、扫描恰好扫到"。
//!   `exe_dir` 的来源见 [`ensure_builtins`] 文档：开发期为宿主 exe 同目录；
//!   单文件分发为内嵌扩展物化目录。
//! - 5 个内置扩展的元数据（id / name / frozen / capabilities）必须与
//!   `crates/dd-ext/src/bin/*.rs` 各自的 `spec()` 保持一致（宿主编排侧登记，
//!   扩展自述侧为准——握手 `initialize` 后宿主会再次拿到真实 `ProviderInfo`）。
//!
//! 注册规则：
//! - **内存构造，零文件写入**（不落 `extensions.d`，不产生清单文件）；
//! - 只注册**指定目录存在**的 exe（未构建 / 被移除的扩展静默跳过，不视为错误）；
//!   `exe_dir` 由宿主决定——开发期通常是「宿主 exe 同目录」，打包后的单文件分发
//!   则是「内嵌扩展物化目录」（`dd-gui::embedded::materialize`，见
//!   `crates/dd-gui/src/embedded.rs`）；
//! - `version` 取宿主包版本（内置扩展随宿主分发，宿主升级即桩缓存自然失效）。

use std::path::Path;

use crate::manifest::{from_builtin, LoadedExtension};

/// 单个内置扩展的注册描述。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinSpec {
    /// 可执行文件名（**不带**平台后缀，如 `dd-ext-calc`；注册时按平台补 `.exe`）。
    pub exe: &'static str,
    pub id: &'static str,
    pub name: &'static str,
    /// 扩展**自述**的顶层命令是否内容不变（与扩展 `spec().frozen` 对齐，防漂移）。
    /// 注意：这与"宿主是否落桩"**不是一回事**——宿主缓存策略见
    /// [`BuiltinSpec::host_frozen`]（设计文档 §6.3：含兜底能力者一律视为 fresh）。
    pub frozen: bool,
    /// 是否提供兜底命令模板（协议 §6.2 `fallback_commands` 结果非空）。
    pub has_fallback: bool,
    /// 扩展声明需要的 `host/*` 方法（协议 §7.4 能力前置白名单）。
    pub capabilities: &'static [&'static str],
}

impl BuiltinSpec {
    /// 宿主缓存策略的 `frozen`（写进 `manifest.frozen`，决定 M3 落桩/读桩）：
    /// **含兜底能力者一律视为 fresh**（§6.3）——不落桩、冷启动拉起进程，
    /// 否则无进程可调 `fallback_commands`。
    pub fn host_frozen(&self) -> bool {
        self.frozen && !self.has_fallback
    }

    /// 设计文档 §6.3 的 fresh 判定：`!host_frozen`。
    pub fn is_fresh(&self) -> bool {
        !self.host_frozen()
    }
}

/// MVP 5 个内置扩展（与 `crates/dd-ext/src/bin/*.rs` 的 `spec()` 对齐：
/// `frozen` = 扩展自述；宿主落桩策略由 [`BuiltinSpec::host_frozen`] 派生）。
pub const BUILTINS: &[BuiltinSpec] = &[
    BuiltinSpec {
        exe: "dd-ext-apps",
        id: "com.ddrun.apps",
        name: "Apps",
        // 应用列表随安装/卸载变化 → fresh
        frozen: false,
        has_fallback: false,
        capabilities: &[],
    },
    BuiltinSpec {
        exe: "dd-ext-calc",
        id: "com.ddrun.calc",
        name: "Calculator",
        // 自述：顶层固定可缓存；但含兜底能力 → host_frozen=false（§6.3 fresh）
        frozen: true,
        has_fallback: true,
        capabilities: &["host/set_clipboard"],
    },
    BuiltinSpec {
        exe: "dd-ext-system",
        id: "com.ddrun.system",
        name: "System",
        // 无兜底、顶层固定 → host_frozen=true（可落磁盘桩，A6）
        frozen: true,
        has_fallback: false,
        capabilities: &[],
    },
    BuiltinSpec {
        exe: "dd-ext-websearch",
        id: "com.ddrun.websearch",
        name: "Web Search",
        // 含兜底能力 → host_frozen=false（§6.3 fresh）
        frozen: true,
        has_fallback: true,
        capabilities: &["host/open_url"],
    },
    BuiltinSpec {
        exe: "dd-ext-shell",
        id: "com.ddrun.shell",
        name: "Shell",
        // 含兜底能力 → host_frozen=false（§6.3 fresh）
        frozen: true,
        has_fallback: true,
        capabilities: &[],
    },
];

/// 内置扩展的注册版本 = 宿主包版本（随宿主分发，宿主升级 → 磁盘桩缓存键变化
/// → 旧桩自然失效重拉，见 M3 `FrozenCache` 键 = id + version）。
fn builtin_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 在 `exe_dir` 中注册**存在**的内置扩展。
///
/// `exe_dir` 由宿主决定：开发期为宿主 `current_exe` 同目录（workspace 各 bin 同放
/// 一处）；打包后的单文件分发为内嵌扩展物化目录（`dd-gui::embedded::materialize`，
/// 见 `crates/dd-gui/src/embedded.rs`）。
///
/// 返回顺序与 [`BUILTINS`] 一致；找不到的 exe 静默跳过。这是纯内存构造，
/// 不触碰文件系统（除 `is_file` 探测外），可安全在单测中调用。
pub fn ensure_builtins(exe_dir: &Path) -> Vec<LoadedExtension> {
    BUILTINS
        .iter()
        .filter_map(|spec| {
            let exe = exe_name(spec.exe);
            let command = exe_dir.join(exe);
            command.is_file().then(|| {
                from_builtin(
                    command,
                    spec.id,
                    spec.name,
                    spec.host_frozen(), // 宿主缓存策略（含兜底者 fresh，§6.3）
                    spec.capabilities,
                    builtin_version(),
                )
            })
        })
        .collect()
}

/// 平台可执行文件名（Windows 补 `.exe`）。
fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

/// 把 [`ensure_builtins`] 找到的扩展与磁盘扫描结果合并：
/// 内置扩展**排在前**，同 id 的磁盘清单不重复注册（内置优先，避免双份进程）。
pub fn merge_builtins(
    builtins: Vec<LoadedExtension>,
    scanned: Vec<LoadedExtension>,
) -> Vec<LoadedExtension> {
    let mut merged = builtins;
    for ext in scanned {
        if !merged.iter().any(|b| b.manifest.id == ext.manifest.id) {
            merged.push(ext);
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// 临时目录（避免引入 tempfile 依赖，与 roundtrip.rs 同款）。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut dir = std::env::temp_dir();
            dir.push(format!("dd-host-builtin-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("创建临时目录");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn touch(path: &Path) {
        fs::write(path, b"").expect("写入空文件");
    }

    #[test]
    fn registry_matches_dd_ext_specs() {
        // 与 crates/dd-ext/src/bin/*.rs 的 spec() 逐字段对齐（防漂移哨兵）
        let ids: Vec<&str> = BUILTINS.iter().map(|s| s.id).collect();
        assert_eq!(
            ids,
            vec![
                "com.ddrun.apps",
                "com.ddrun.calc",
                "com.ddrun.system",
                "com.ddrun.websearch",
                "com.ddrun.shell"
            ]
        );
        // 扩展自述 frozen 与 crates/dd-ext 各 bin 的 spec().frozen 对齐
        //（防漂移哨兵）：Apps fresh、其余 4 个自述"顶层固定可缓存"。
        let frozen_map: Vec<(&str, bool)> = BUILTINS.iter().map(|s| (s.id, s.frozen)).collect();
        assert_eq!(
            frozen_map,
            vec![
                ("com.ddrun.apps", false),
                ("com.ddrun.calc", true),
                ("com.ddrun.system", true),
                ("com.ddrun.websearch", true),
                ("com.ddrun.shell", true),
            ]
        );
        // 兜底能力与 crates/dd-ext 各 bin 的 spec().has_fallback 对齐
        let fallback_ids: Vec<&str> = BUILTINS
            .iter()
            .filter(|s| s.has_fallback)
            .map(|s| s.id)
            .collect();
        assert_eq!(
            fallback_ids,
            vec!["com.ddrun.calc", "com.ddrun.websearch", "com.ddrun.shell"]
        );
        // 宿主缓存策略 host_frozen：含兜底 → false（fresh，§6.3）；无兜底按自述。
        let host_frozen_map: Vec<(&str, bool)> =
            BUILTINS.iter().map(|s| (s.id, s.host_frozen())).collect();
        assert_eq!(
            host_frozen_map,
            vec![
                ("com.ddrun.apps", false),
                ("com.ddrun.calc", false),      // 含兜底 → 不落桩（fresh）
                ("com.ddrun.system", true),     // 无兜底 → 可落桩（A6）
                ("com.ddrun.websearch", false), // 含兜底 → fresh
                ("com.ddrun.shell", false),     // 含兜底 → fresh
            ]
        );
        // §6.3：含兜底能力者一律 fresh
        assert!(
            BUILTINS
                .iter()
                .filter(|s| s.has_fallback)
                .all(|s| s.is_fresh()),
            "含兜底能力的扩展必须 fresh（否则无进程可调 fallback_commands）"
        );
        // 能力声明 ⊆ 宿主白名单（manifest.rs HOST_CAPABILITIES）
        for spec in BUILTINS {
            for cap in spec.capabilities {
                assert!(
                    crate::manifest::HOST_CAPABILITIES.contains(cap),
                    "{} 声明未知能力 {cap}",
                    spec.id
                );
            }
        }
    }

    #[test]
    fn ensure_builtins_registers_only_existing_exes() {
        let tmp = TempDir::new("only-existing");
        // 只放 calc + shell 两个 exe
        let calc = tmp.0.join(exe_name("dd-ext-calc"));
        let shell = tmp.0.join(exe_name("dd-ext-shell"));
        touch(&calc);
        touch(&shell);

        let exts = ensure_builtins(&tmp.0);
        assert_eq!(exts.len(), 2);
        assert_eq!(exts[0].manifest.id, "com.ddrun.calc");
        assert_eq!(exts[1].manifest.id, "com.ddrun.shell");
        // 元数据透传：calc 含兜底 → fresh（不落桩），capabilities 与注册表一致
        assert!(
            !exts[0].manifest.frozen,
            "calc 含兜底能力 → fresh，不落 frozen 桩"
        );
        assert_eq!(exts[0].manifest.capabilities, vec!["host/set_clipboard"]);
        assert_eq!(exts[0].manifest.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(exts[0].command, calc, "command 指向真实 exe");
    }

    #[test]
    fn ensure_builtins_empty_dir_yields_empty() {
        let tmp = TempDir::new("empty");
        let exts = ensure_builtins(&tmp.0);
        assert!(exts.is_empty(), "目录无 exe → 不注册任何内置扩展");
    }

    #[test]
    fn merge_builtins_dedupes_scanned_same_id() {
        let tmp = TempDir::new("merge");
        let exe = tmp.0.join(exe_name("dd-ext-calc"));
        touch(&exe);
        let builtins = ensure_builtins(&tmp.0);
        assert_eq!(builtins.len(), 1);

        // 磁盘扫描到同 id（模拟用户手写 com.ddrun.calc.json）→ 内置优先，不重复
        let dup = from_builtin(
            exe.clone(),
            "com.ddrun.calc",
            "Calculator (user)",
            true,
            &[],
            "9.9.9",
        );
        let merged = merge_builtins(builtins.clone(), vec![dup.clone()]);
        assert_eq!(merged.len(), 1, "同 id 磁盘清单被去重");
        assert_eq!(merged[0].manifest.name, "Calculator", "内置优先");

        // 磁盘上的第三方扩展（不同 id）照常并入
        let third = from_builtin(
            tmp.0.join("third.exe"),
            "com.third.party",
            "Third Party",
            true,
            &[],
            "1.0.0",
        );
        let merged = merge_builtins(builtins, vec![third]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1].manifest.id, "com.third.party");
    }
}
