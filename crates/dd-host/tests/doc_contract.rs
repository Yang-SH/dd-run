//! doc 契约测试（B2②，dd-run-改进分析.md §3.3）：把契约文档的「实现现状」
//! 注从散文变成断言——**测试名即锚点**，实现漂移时测试红，提示同步文档。
//!
//! 覆盖范围与口径（与 docs/INDEX.md 登记的 24 处现状注对照）：
//! - ✅「已实现」类 → 正向断言行为；
//! - ⚠️「未实现/有偏差」类 → 断言**当前确实没有该行为**（有人补实现即红，
//!   提醒同步文档）；
//! - 断言不到的（渲染侧状态、纯 UI 分支）标 `no-assert` 并写原因，见各注
//!   附近的本文件内清单。
//!
//! B2② 分布（第二批收尾后）：本文件覆盖 **dd-host / dd-protocol 可达**的
//! 6 条（manifest 跳过上报、UTF-8 无效帧、超时常量、semver 三段口径、
//! 可执行补全、内置 frozen 降级）；`dd-protocol` / `dd-ext` / `dd-gui` 侧
//! 现状注分别由各自 `doc_contract`（测试）文件承接——dd-gui 侧（toast 忽略
//! state / frozen 门禁读清单 / 通知消费式取走等，其中 2 条就近放
//! `aggregator.rs` 测试模块）见 `crates/dd-gui/src/doc_contract_tests.rs`。
//! 本文件共 **7 个测试** = 上述 6 条 B2② 锚 + 1 条 C5 deprecated 锚
//! （`manifest_icon_is_stored_unexpanded`，见下方 no-assert 台账首条）。
//!
//! `no-assert` 台账（dd-host 侧）：
//! - manifest-schema.md §5「`icon` 无消费者」：**C5 收口后改为 deprecated 锚**
//!   ——可断言的半段是「宿主不展开」（`${EXT_DIR}` 占位符原样保留，区别于
//!   `entry.command`/`entry.cwd` 的展开），见
//!   [`manifest_icon_is_stored_unexpanded`]；「全仓无读取方」的负向结构性
//!   断言仍无运行时载体（启用消费时同步 manifest-schema.md §5 deprecated 段）；
//! - protocol.md §10「宿主不读取 `result.timeouts`」：消费侧行为，无断言
//!   载体（补实现时同步 protocol.md §10 表与 §5.1 注）。

use std::path::PathBuf;

/// 进程级唯一临时目录（与 `dd-host` 既有测试同口径）。
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "dd-host-doc-contract-{tag}-{}-{}",
        std::process::id(),
        std::time::Instant::now().elapsed().as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// 契约锚：docs/manifest-schema.md「目录异常处置」现状注（B2① 校正后）。
///
/// 现状注原文：单个清单读取失败的条目**跳过并上报**（`scan_dir` 收集
/// `outcome.skipped`，R-20 起设置页可见原因）——**不再是**「静默丢弃」。
///
/// 断言：坏 JSON 清单 → `skipped` 非空且 `SkipReason::ParseError`；同目录的
/// 好清单照常 `loaded`（跳过不中断扫描）。有人改回「静默丢弃」（跳过不入
/// `skipped`）此测试即红。
#[test]
fn manifest_skip_is_reported_not_silent() {
    let dir = temp_dir("skip-reported");
    std::fs::create_dir_all(&dir).unwrap();
    // 坏 JSON：解析必失败（规则 1）
    std::fs::write(dir.join("bad.json"), "{ not json").unwrap();
    // 好清单：最小合法 manifest（跳过坏的不影响它）；entry.command 指向
    // 目录内真实存在的文件（规则 5：command 存在性校验）。
    std::fs::write(dir.join("ok.exe"), b"MZ").unwrap();
    std::fs::write(
        dir.join("good.json"),
        r#"{"schema_version":"1.0","id":"com.ok.ext","name":"Ok","version":"1.0.0","description":"","author":"","license":"MIT","entry":{"command":"ok.exe","args":[]}}"#,
    )
    .unwrap();

    let opts = dd_host::manifest::ScanOptions::default();
    let outcome = dd_host::manifest::scan_dir(&dir, &opts);

    assert_eq!(outcome.loaded.len(), 1, "好清单应照常加载");
    assert_eq!(outcome.skipped.len(), 1, "坏清单必须**上报**（非静默丢弃）");
    assert!(
        matches!(
            outcome.skipped[0].reason,
            dd_host::manifest::SkipReason::ParseError(_)
        ),
        "坏 JSON → SkipReason::ParseError"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 契约锚：docs/protocol.md §2.2 规则 6 现状注。
///
/// 现状注原文：非 UTF-8 的行视同无效帧，实现产出 `Frame::InvalidUtf8`（无
/// 对应 JSON-RPC 错误码），不 panic。
///
/// 断言：`Decoder` 吃进非 UTF-8 行 → 产出 `Frame::InvalidUtf8`（而非 panic /
/// 错误逃逸）。
#[test]
fn invalid_utf8_line_yields_invalid_utf8_frame() {
    use dd_protocol::framing::{Decoder, Frame};

    let mut dec = Decoder::new(dd_protocol::framing::DEFAULT_MAX_MESSAGE_BYTES);
    let mut bytes = b"{\"jsonrpc\":\"2.0\",\"method\":\"x\"}\n".to_vec();
    bytes.extend_from_slice(&[0xFF, 0xFE, b'\n']); // 非 UTF-8 行
    let frames = dec.push(&bytes);
    assert!(
        frames.iter().any(|f| matches!(f, Frame::InvalidUtf8)),
        "非 UTF-8 行必须产出 Frame::InvalidUtf8"
    );
}

/// 契约锚：docs/protocol.md §10 超时常量现状注。
///
/// 现状注原文：「宿主可配置」未实现——`initialize` / `top_level_commands`
/// 超时为编译期常量（`TIMEOUT_INITIALIZE` = 5 s、`TIMEOUT_TOP_LEVEL_COMMANDS`
/// = 3 s），且 `result.timeouts` 宿主从不读取。
///
/// 断言（前半）：常量数值与文档表格一致。有人调常量即红，提示同步文档表。
/// 后半（「从不读取 result.timeouts」）= `no-assert`：需 invoke 全链路夹具
/// 捕获解析丢弃点，第二批随 B2 收尾补。
#[test]
fn timeout_constants_match_doc_table() {
    use dd_host::process::{TIMEOUT_INITIALIZE, TIMEOUT_TOP_LEVEL_COMMANDS};
    assert_eq!(TIMEOUT_INITIALIZE, std::time::Duration::from_millis(5_000));
    assert_eq!(
        TIMEOUT_TOP_LEVEL_COMMANDS,
        std::time::Duration::from_millis(3_000)
    );
}

/// 契约锚：docs/manifest-schema.md §5 `version` 行现状注。
///
/// 现状注原文：解析只收**三段纯数字**，prerelease/build 后缀（如 `1.0.0-beta`）
/// 判为非法。
///
/// 断言：`parse_semver` 三段纯数字通过；第四段 / prerelease / build 后缀 /
/// 空段全部 `None`。有人引入完整 semver（协议演进）时此测试红，提示同步
/// manifest-schema.md §5 表。
#[test]
fn manifest_version_accepts_three_numeric_segments_only() {
    use dd_host::manifest::parse_semver;
    assert_eq!(parse_semver("1.0.0"), Some((1, 0, 0)));
    assert_eq!(parse_semver("0.2.13"), Some((0, 2, 13)));
    for bad in [
        "1.0.0-beta",
        "1.0.0+build.7",
        "1.0.0.4",
        "1..0",
        "1.0",
        "x.y.z",
    ] {
        assert_eq!(parse_semver(bad), None, "非三段纯数字应判非法：{bad}");
    }
}

/// 契约锚：docs/manifest-schema.md §7 路径展开现状注（核对至 2026-10-11）。
///
/// 现状注原文：Windows 下 `command` 不带扩展名时按「原样路径 → `.exe`」补全，
/// 同名无扩展名文件**优先**；**不再补全 `.cmd`/`.bat`**（F4，BatBadBut 纵深
/// 防御）——裸名只剩 `.cmd`/`.bat` 的清单 fail-closed 跳过。
///
/// 断言：① 原样存在 → 原样命中（优先于 `.exe`）；② 仅 `.exe` 存在 → 补全
/// 命中（仅 Windows）；③ 仅 `.cmd` 存在 → `None`（不补全批处理）。
/// 有人恢复 `.cmd`/`.bat` 补全时此测试红，提示同步 manifest-schema.md §7。
#[test]
fn resolve_executable_prefers_exact_name_and_skips_cmd_bat() {
    use dd_host::manifest::resolve_executable;
    let dir = temp_dir("resolve-exe");
    std::fs::create_dir_all(&dir).unwrap();

    // ① 同名无扩展名 + .exe 并存 → 无扩展名者优先（原样路径第一优先）
    std::fs::write(dir.join("tool"), b"x").unwrap();
    std::fs::write(dir.join("tool.exe"), b"x").unwrap();
    let hit = resolve_executable(&dir.join("tool")).unwrap();
    assert_eq!(hit, dir.join("tool"), "原样路径优先于 .exe 补全");

    // ② 仅 .exe 存在 → Windows 下补全命中
    std::fs::write(dir.join("only.exe"), b"x").unwrap();
    let hit = resolve_executable(&dir.join("only"));
    if cfg!(windows) {
        assert_eq!(hit.unwrap(), dir.join("only.exe"), "仅 .exe 时补全命中");
    } else {
        assert!(hit.is_none(), "非 Windows 不做 .exe 补全");
    }

    // ③ 仅 .cmd / .bat 存在 → fail-closed（F4：不补全批处理）
    std::fs::write(dir.join("batch.cmd"), b"@echo off").unwrap();
    std::fs::write(dir.join("script.bat"), b"@echo off").unwrap();
    assert!(
        resolve_executable(&dir.join("batch")).is_none(),
        "不得补全 .cmd（BatBadBut，CVE-2024-24576）"
    );
    assert!(
        resolve_executable(&dir.join("script")).is_none(),
        "不得补全 .bat"
    );

    // ④ 什么都不存在 → None
    assert!(resolve_executable(&dir.join("ghost")).is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 契约锚：docs/manifest-schema.md §10 `frozen` 行现状注。
///
/// 现状注原文：内置注册写入 `host_frozen()`——含兜底命令者被宿主**主动降级
/// 为 `false`**，与 calc/websearch/shell 自述 `frozen=true` 相反。
///
/// 断言：自述 `frozen=true` 且 `has_fallback=true` 的三个内置（calc /
/// websearch / shell）`host_frozen()` 全为 `false`；无兜底的 system 保持
/// `true`。有人调整 §6.3 降级策略时此测试红，提示同步 manifest-schema.md
/// §10 表与 protocol.md §6.1 注。
#[test]
fn builtin_fallback_providers_are_degraded_to_fresh() {
    use dd_host::builtin::BUILTINS;
    let degraded: Vec<&str> = BUILTINS
        .iter()
        .filter(|s| s.frozen && s.has_fallback)
        .map(|s| s.id)
        .collect();
    assert_eq!(
        degraded,
        vec!["com.ddrun.calc", "com.ddrun.websearch", "com.ddrun.shell"],
        "文档口径：恰这三个自述 frozen=true 的兜底提供者被降级"
    );
    for spec in BUILTINS.iter() {
        assert_eq!(
            spec.host_frozen(),
            spec.frozen && !spec.has_fallback,
            "{} 的 host_frozen 必须遵循 §6.3 降级式",
            spec.id
        );
    }
    // 文档点名对照：system（无兜底、自述 true）不降级
    let system = BUILTINS
        .iter()
        .find(|s| s.id == "com.ddrun.system")
        .unwrap();
    assert!(system.frozen, "system 自述 frozen=true");
    assert!(!system.has_fallback, "system 无兜底");
    assert!(system.host_frozen(), "system 可落磁盘桩（A6）");
}

/// 契约锚：docs/manifest-schema.md §5 `icon` 行 deprecated 注（C5 收口）。
///
/// 现状注原文：v1.0 冻结面上保留解析兼容，但宿主**无任何消费者**——不展开
/// （`${EXT_DIR}` 等占位符原样保留）、不读取，UI 亦未使用。清单作者勿依赖。
///
/// 断言：清单 `icon` 带 `${EXT_DIR}` 占位符 → `manifest.icon` **原样保留**
///（不经 `expand_path`，与 `entry.command`/`entry.cwd` 的展开形成对照）。
/// 有人启用 icon 消费（扩展管理卡片展示）时此测试红，提示同步 §5 deprecated
/// 段与 §7 路径展开注。
#[test]
fn manifest_icon_is_stored_unexpanded() {
    let dir = temp_dir("icon-unexpanded");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("app.exe"), b"MZ").unwrap();
    std::fs::write(
        dir.join("m.json"),
        r#"{"schema_version":"1.0","id":"com.icon.ext","name":"I","version":"1.0.0","description":"","author":"","license":"MIT","icon":"${EXT_DIR}/icon.png","entry":{"command":"app.exe","args":[]}}"#,
    )
    .unwrap();

    let opts = dd_host::manifest::ScanOptions::default();
    let outcome = dd_host::manifest::scan_dir(&dir, &opts);
    assert_eq!(outcome.loaded.len(), 1, "清单应照常加载");
    let ext = &outcome.loaded[0];
    assert_eq!(
        ext.manifest.icon.as_deref(),
        Some("${EXT_DIR}/icon.png"),
        "icon 必须原样保留（deprecated：宿主不展开不读取）"
    );
    // 对照：entry.command 同占位符已被展开为实际可执行路径
    assert_ne!(
        ext.command,
        PathBuf::from("${EXT_DIR}/app.exe"),
        "entry.command 走展开（与 icon 的不展开形成对照）"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
