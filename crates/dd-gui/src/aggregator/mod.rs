//! 首屏聚合层：扫描扩展 → 并行拉取 → 合并为可渲染的 [`PanelItem`] 列表。
//!
//! 对齐 [`docs/protocol.md`](../../docs/protocol.md) §5（握手）/ §6.1（顶层命令）
//! 与 [`docs/implementation.md`](../../docs/implementation.md) M1「首屏聚合」任务：
//! - **并行**：每扩展一个线程（进程对象线程独占），互不阻塞（A12 能力调用不阻塞 UI）；
//! - **错误隔离**：单个扩展失败只记入 [`SourceSummary`]，不影响其他扩展与整体渲染；
//! - **内置扩展常驻**（M4 P4 → M9 in-process）：`dd-ext-apps/calc/system/websearch/shell`
//!   由宿主**内存自注册**（manifest-schema §10：内置不通过清单文件注册 →
//!   宿主启动时直接内存构造 `LoadedExtension`）。**M9 起内置扩展以 in-process 方式运行**
//!   （宿主进程内直接调 `dd_ext::serve_line`，见 [`crate::ext_client`]），注册不再
//!   依赖磁盘 `dd-ext-*.exe`（[`dd_host::builtin::builtin_registrations`]），也
//!   不再物化内嵌 exe；第三方 / sidecar 仍为子进程，扩展目录中的清单与其**并存**，
//!   同 id 以内置优先。
//! - **扩展清单扫描双位置**（M7 批次 7.5）：用户数据目录 `extensions.d/`（manifest-schema
//!   §2 主位置）+ **宿主 exe 同目录 `extensions.d/` 便携 sidecar**（免安装 zip「解压即用」）；
//!   两处按 id 去重——用户目录优先覆盖分发版，sidecar 独有追加，内置仍最优先。
//! - **信任门禁**（S-05，2026-09-24）：扫描到的每个扩展经 [`dd_host::trust::assess`] 判定
//!   （来源 + 首方白名单 + 台账哈希），**只有 `AutoTrusted` 才进入 spawn 集合**——与既有的
//!   「停用集」同一手法：`exts` 保留全集（设置页要展示并允许用户批准），`active` 只留放行者。
//!   第二道门在 [`spawn_and_initialize_with_info`]（**所有子进程 spawn 的唯一入口**，含桩复热），
//!   故不存在旁路。完整方案见 `docs/security-audit-2026-09-23.md` §4.4.1。
//!
//! M3 缓存与懒加载（见 [`docs/implementation.md`](../../docs/implementation.md) §M3）：
//! - **frozen + 磁盘桩命中** → [`ExtItems::Stub`]：**不拉起进程**（A6），首屏读桩渲染；
//! - **frozen 无桩**（首次运行）→ 照常 spawn 拉取并**落盘**（下次冷启动读桩）；
//! - **fresh**（`frozen=false`）→ spawn 拉取，**不落盘**；
//! - M4 宿主 fallback 轮补充：握手后 `provider.has_fallback == true`（设计文档 §6.3
//!   "含兜底能力者一律视为 fresh"）→ **即使清单标 frozen 也不落盘**，并清除历史桩，
//!   保证进程恒 warm 可响应 `fallback_commands`；
//! - 源状态三态：Warm（进程活）/ Stub（仅桩）/ Failed（失败），供页脚展示与 A6 观察。

mod env;
mod items;
mod model;
mod scan;
mod spawn;

// 批次 B（refactor-structure-2026-10-21.md §4.2）目录化桥接：公开项经
// `pub use` 再导出，调用方的既有路径（含 benches/hot_paths.rs 的 bench 引用）
// 保持不变。
pub use env::{inject_ext_settings, inject_websearch_env};
pub use items::{custom_command_items, flatten, to_panel_item};
pub use model::{
    CollectResult, ExtItems, SourceStatus, SourceSummary, CUSTOM_ITEM_PREFIX, HOST_CUSTOM_EXT_ID,
    HOST_VERSION, PROTOCOL_VERSION,
};
pub use scan::{
    is_trusted, load_extension_sources, origin_of, pending_count, sidecar_extensions_dir,
    ExtensionSources,
};
pub use spawn::{
    collect_top_level, collect_top_level_streaming, spawn_and_initialize,
    spawn_and_initialize_with_info,
};
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, HashMap};
    use std::path::PathBuf;

    use crate::settings::{CustomCommand, Lang};
    use dd_ext::ExtensionSpec;
    use dd_host::cache::{FrozenCache, FrozenSnapshot};
    use dd_host::manifest::LoadedExtension;
    use dd_host::trust::{Assessment, ExtOrigin, Trust};
    use dd_protocol::model::CommandItem;

    use super::env::{ext_cfg_env_name, EXT_CFG_ENV_PREFIX};
    use super::scan::{
        apply_owned_names, merge_scanned_dirs, merge_sidecar_scan, owned_sidecar_name_key,
    };
    use super::spawn::{
        detail_suffix, load_one, spawn_and_initialize_with, uses_sidecar_assessment, ExtOutcome,
    };
    use dd_protocol::model::{CommandRef, Icon, IconKind};

    fn cmd(id: &str, title: &str, section: Option<&str>) -> CommandItem {
        CommandItem {
            id: id.to_string(),
            title: title.to_string(),
            subtitle: None,
            icon: None,
            section: section.map(|s| s.to_string()),
            tags: None,
            details: None,
            text_to_suggest: None,
            more_commands: None,
            command: CommandRef::Invoke,
        }
    }

    fn ready(id: &str, name: &str, items: Vec<CommandItem>) -> ExtItems {
        ExtItems::Ready {
            id: id.to_string(),
            name: name.to_string(),
            items,
        }
    }

    /// N1：自定义直达命令 → 宿主虚拟条目——id 前缀 `custom:`、ext_id = 保留
    /// `com.ddrun.host`、keyword 进 tags、target 作副标题、拼音索引随 title
    /// 生成、分组与类别徽标 =「直达」、command = Invoke（宿主内部分发）。
    #[test]
    fn custom_command_items_build_host_virtual_entries() {
        use crate::settings::CustomCommandKind;
        let cmds = vec![
            CustomCommand::new("GitHub", "GH", CustomCommandKind::Url, "https://github.com")
                .unwrap(),
            CustomCommand::new("周报", "rep", CustomCommandKind::Path, r"C:\doc\周报.docx")
                .unwrap(),
        ];
        let items = custom_command_items(&cmds, Lang::ZhCn);
        assert_eq!(items.len(), 2);
        let gh = &items[0];
        assert_eq!(gh.id, "custom:gh", "keyword 小写化进 id");
        assert_eq!(gh.ext_id, HOST_CUSTOM_EXT_ID);
        assert_eq!(gh.title, "GitHub");
        assert_eq!(gh.subtitle, "https://github.com");
        assert_eq!(gh.tags, vec!["gh".to_string()]);
        assert_eq!(gh.section, "直达");
        assert_eq!(gh.result_category.as_deref(), Some("直达"));
        assert_eq!(gh.command, CommandRef::Invoke);
        assert!(gh.more_commands.is_empty());
        // 拼音索引：中文标题生成全拼+首字母；纯英文标题为空串（原字符即匹配域）
        assert_eq!(items[1].pinyin, "zhoubao zb", "周报 → zhoubao zb");
        assert_eq!(gh.pinyin, "", "纯英文标题拼音索引为空");
    }

    #[test]
    fn to_panel_item_carries_more_commands() {
        // v3.3 P1：§8.1 more_commands 透传进 PanelItem（右键菜单渲染源）
        let mut c = cmd("files.open.1", "报告.txt", Some("文件"));
        c.more_commands = Some(vec![
            cmd("files.reveal.1", "显示所在目录", None),
            cmd("files.copy.1", "复制路径", None),
        ]);
        let panel = to_panel_item(&c, "com.ddrun.filesearch", "文件", Lang::ZhCn);
        assert_eq!(panel.more_commands.len(), 2);
        assert_eq!(panel.more_commands[0].id, "files.reveal.1");
        assert_eq!(panel.more_commands[1].id, "files.copy.1");
        // None → 空表（无 more_commands 的项菜单走静态映射）
        let plain = to_panel_item(&cmd("a", "b", None), "com.ddrun.calc", "", Lang::ZhCn);
        assert!(plain.more_commands.is_empty());
    }

    /// N2：`ext_cfg_env_name` 键名守卫——合法键大写化拼前缀；空/超长/含
    /// 非法字符拒绝。前缀 `DD_EXT_CFG_` 结构性避开 S-10 保护名单
    ///（`DD_EXT_CFG_PATH` ≠ `PATH`），结果名复查仅作纵深防御。
    #[test]
    fn n2_ext_cfg_env_name_guards_key_shape() {
        assert_eq!(
            ext_cfg_env_name("blocklist"),
            Some("DD_EXT_CFG_BLOCKLIST".to_string())
        );
        assert_eq!(
            ext_cfg_env_name("Path"),
            Some("DD_EXT_CFG_PATH".to_string())
        );
        assert!(ext_cfg_env_name("").is_none());
        assert!(ext_cfg_env_name("foo-bar").is_none(), "连字符拒绝");
        assert!(ext_cfg_env_name("空 格").is_none(), "非 ASCII 拒绝");
        assert!(ext_cfg_env_name(&"a".repeat(33)).is_none(), "超长拒绝");
        // 前缀不变式：无论键是什么，结果名都不可能等于任何受保护变量名
        // （受保护名单无 `DD_EXT_CFG_` 前缀项——结构性豁免的锚定断言）。
        assert!(dd_host::process::PROTECTED_ENV_KEYS
            .iter()
            .all(|p| !p.starts_with(EXT_CFG_ENV_PREFIX)));
    }

    /// N2：`inject_ext_settings` 注入——只触碰表内扩展；键映射正确；
    /// 非法键跳过且不误伤同扩展其他键。
    #[test]
    fn n2_inject_ext_settings_targets_listed_exts_only() {
        let mut exts = vec![
            loaded_ext("com.ddrun.apps", "a"),
            loaded_ext("com.ddrun.calc", "c"),
        ];
        let mut kv = BTreeMap::new();
        kv.insert("blocklist".to_string(), "卸载, update".to_string());
        kv.insert("bad-key".to_string(), "v".to_string());
        let mut cfg = BTreeMap::new();
        cfg.insert("com.ddrun.apps".to_string(), kv);
        cfg.insert("com.ddrun.not-scanned".to_string(), BTreeMap::new());
        inject_ext_settings(&mut exts, &cfg);
        let env = &exts[0].manifest.entry.env;
        assert_eq!(
            env.get("DD_EXT_CFG_BLOCKLIST").map(String::as_str),
            Some("卸载, update"),
            "合法键注入"
        );
        assert!(!env.contains_key("DD_EXT_CFG_BAD-KEY"), "非法键跳过");
        assert!(
            !exts[1]
                .manifest
                .entry
                .env
                .contains_key("DD_EXT_CFG_BLOCKLIST"),
            "表外扩展零改动"
        );
    }

    /// 最小可加载扩展夹具：`path` 携带 `tag` 以区分「用户目录 / sidecar」来源。
    fn loaded_ext(id: &str, tag: &str) -> LoadedExtension {
        use dd_host::manifest::{Entry, Manifest};
        LoadedExtension {
            manifest: Manifest {
                schema_version: "1.0".to_string(),
                id: id.to_string(),
                name: format!("Ext {id}"),
                version: "0.1.0".to_string(),
                description: String::new(),
                author: String::new(),
                license: String::new(),
                homepage: String::new(),
                icon: None,
                entry: Entry {
                    command: "ext.exe".to_string(),
                    args: Vec::new(),
                    env: Default::default(),
                    cwd: None,
                },
                frozen: true,
                capabilities: Vec::new(),
                platforms: None,
                min_host_version: None,
            },
            path: PathBuf::from(format!(r"{tag}\{id}.json")),
            dir: PathBuf::from(tag),
            command: PathBuf::from(format!(r"{tag}\ext.exe")),
            cwd: PathBuf::from(tag),
        }
    }

    /// R-20：清单解析失败的扩展不再无声消失——聚合结果 `skipped` 含
    /// （路径, 原因）；全合法目录 → `skipped` 为空（正负例）。
    #[test]
    fn r20_skipped_aggregated_with_reason() {
        let dir = std::env::temp_dir().join(format!(
            "dd-gui-r20-{}-{}",
            std::process::id(),
            std::time::Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // 正例：语法错误清单 → skipped 恰含（该路径, 非空原因）。
        std::fs::write(dir.join("broken.json"), r#"{"schema_version":"1.0","""#).unwrap();
        let mut note = String::new();
        let mut skipped: Vec<(String, String)> = Vec::new();
        let scanned = merge_sidecar_scan(Some(dir.clone()), None, &mut note, &mut skipped);
        assert!(scanned.is_empty(), "语法错误清单不得进入 loaded");
        assert_eq!(skipped.len(), 1, "skipped 应恰含一条（路径, 原因）");
        assert!(
            skipped[0].0.ends_with("broken.json"),
            "skipped 应含清单路径，实得：{}",
            skipped[0].0
        );
        assert!(!skipped[0].1.is_empty(), "原因不得为空");

        // 负例：**全合法目录**（移除损坏清单）补一条全合法清单（命令文件存在，
        // 过规则 8）→ skipped 为空。
        std::fs::remove_file(dir.join("broken.json")).unwrap();
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin").join("ext"), "").unwrap();
        std::fs::write(
            dir.join("ok.json"),
            r#"{"schema_version":"1.0","id":"com.example.ok","name":"OK","version":"1.0.0","entry":{"command":"bin/ext"}}"#,
        )
        .unwrap();
        let mut skipped2: Vec<(String, String)> = Vec::new();
        let scanned2 =
            merge_sidecar_scan(Some(dir.clone()), None, &mut String::new(), &mut skipped2);
        assert_eq!(scanned2.len(), 1, "合法清单应正常加载");
        assert!(
            skipped2.is_empty(),
            "全合法目录 skipped 必须为空，实得：{skipped2:?}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// M9 修复：宿主自有扩展的显示名被本地化覆盖——**内置**取扩展自述
    /// `display_name`（不依赖具体语言）；**随包 sidecar** 取宿主文案键；
    /// **第三方**保持清单原样（宿主不臆造翻译）。
    #[test]
    fn apply_owned_names_localizes_only_owned_extensions() {
        let specs: HashMap<String, dd_ext::ExtensionSpec> = dd_ext::builtins::builtin_specs()
            .into_iter()
            .map(|s| (s.id.to_string(), s))
            .collect();
        let mut exts = vec![
            loaded_ext("com.ddrun.calc", "builtin"),
            loaded_ext("com.ddrun.filesearch", "sidecar"),
            loaded_ext("com.example.thirdparty", "user"),
        ];
        apply_owned_names(&mut exts, &specs, Lang::ZhCn);

        // 内置：覆盖为规格 display_name（与语言无关的等价断言）
        let calc_name = specs.get("com.ddrun.calc").unwrap().display_name;
        assert_eq!(exts[0].manifest.name, calc_name);
        assert_ne!(
            exts[0].manifest.name, "Ext com.ddrun.calc",
            "内置名应被自述 display_name 覆盖"
        );
        // 随包 sidecar：宿主文案键（中文模式）
        assert_eq!(exts[1].manifest.name, "文件搜索");
        // 第三方：清单名原样
        assert_eq!(exts[2].manifest.name, "Ext com.example.thirdparty");
        // 英文模式（sidecar 名随语言切换）
        apply_owned_names(&mut exts, &specs, Lang::EnUs);
        assert_eq!(exts[1].manifest.name, "File Search");
    }

    /// 宿主自有 sidecar 的本地化名映射只覆盖随包扩展。
    #[test]
    fn owned_sidecar_name_key_maps_bundled_only() {
        assert_eq!(
            owned_sidecar_name_key("com.ddrun.filesearch"),
            Some("ext.name.filesearch")
        );
        assert_eq!(owned_sidecar_name_key("com.example.x"), None);
    }

    /// M9 B3：内置扩展（spec 命中）走 **in-process** —— 聚合产出 Ready、且保活集
    /// 中必须是 in-process 后端（**不 spawn 子进程**，与 D3 验收"代码路径验证"对齐）；
    /// 同时无磁盘桩（即使 `frozen=true` 也不读桩）。
    #[test]
    fn collect_top_level_builtin_uses_in_process_backend() {
        use dd_ext::ExtensionSpec;
        use dd_protocol::model::CommandRef;

        let spec = ExtensionSpec {
            id: "com.ddrun.fixture",
            display_name: "Fixture",
            description: "单测夹具",
            frozen: true,
            has_fallback: false,
            capabilities: &[],
            log_tag: "dd-ext-fixture",
            top_level: || {
                vec![CommandItem {
                    id: "fix.hello".into(),
                    title: "Hello".into(),
                    subtitle: None,
                    icon: None,
                    section: None,
                    tags: None,
                    details: None,
                    text_to_suggest: None,
                    more_commands: None,
                    command: CommandRef::Invoke,
                }]
            },
            fallback: None,
            invoke: |_| (dd_protocol::model::CommandResult::Dismiss, Vec::new()),
            pages: None,
        };
        let ext = dd_host::manifest::from_builtin(
            PathBuf::from("dd-ext-fixture.exe"), // 名义路径：不存在也无妨（不 spawn）
            "com.ddrun.fixture",
            "Fixture",
            true,
            &[],
            "0.1.1",
        );
        let mut specs = HashMap::new();
        specs.insert("com.ddrun.fixture".to_string(), spec);

        let result = collect_top_level(std::slice::from_ref(&ext), &specs, None);

        assert_eq!(result.per_ext.len(), 1);
        assert!(
            result.per_ext[0].is_ready(),
            "内置 in-process 应直接 Ready（无 spawn / 无桩），实际 {:?}",
            result.per_ext[0]
        );
        assert_eq!(result.processes.len(), 1);
        assert!(
            result.processes[0].is_in_process(),
            "内置扩展必须走 in-process 后端（不再 spawn 子进程）"
        );
    }

    /// 对照：`specs` 未命中（第三方/sidecar）仍走子进程——exe 不存在 → Failed
    /// （不 panic、不误判为 Ready）。
    ///
    /// S-05 起需同时通过信任门禁：此处用**随包首方**扩展（来源 = sidecar 目录 +
    /// 首方白名单 → 自动信任），以保证本用例仍在验证"非内置走子进程"本身，而不是
    /// 被信任门禁提前拦下（后者由 `spawn_gate_rejects_untrusted_extension` 覆盖）。
    #[test]
    fn collect_top_level_non_builtin_still_uses_subprocess() {
        let dir = sidecar_extensions_dir().expect("本机应能定位宿主 exe 目录");
        let mut ext = loaded_ext("com.ddrun.filesearch", "sidecar");
        ext.dir = dir;
        let specs: HashMap<String, ExtensionSpec> = HashMap::new();

        let result = collect_top_level(std::slice::from_ref(&ext), &specs, None);

        assert_eq!(result.per_ext.len(), 1);
        assert!(
            matches!(result.per_ext[0], ExtItems::Failed { .. }),
            "非内置、exe 不存在 → spawn 失败 → Failed，实际 {:?}",
            result.per_ext[0]
        );
        assert!(result.processes.is_empty(), "失败项无保活客户端");
    }

    #[test]
    fn merge_scanned_dirs_user_dir_wins_and_sidecar_appends() {
        // M7 批次 7.5：便携 sidecar 合并语义——同 id 用户目录优先（覆盖分发版）、
        // sidecar 独有扩展按原序追加；first 顺序保持。
        // S-05：元素携带来源，去重后来源应保留"胜出者"的来源。
        let user = vec![
            (loaded_ext("com.a", "user"), ExtOrigin::UserDir),
            (loaded_ext("com.b", "user"), ExtOrigin::UserDir),
        ];
        let sidecar = vec![
            (loaded_ext("com.b", "sidecar"), ExtOrigin::Sidecar),
            (loaded_ext("com.c", "sidecar"), ExtOrigin::Sidecar),
        ];
        let merged = merge_scanned_dirs(user, sidecar);
        let ids: Vec<&str> = merged.iter().map(|(e, _)| e.manifest.id.as_str()).collect();
        assert_eq!(
            ids,
            ["com.a", "com.b", "com.c"],
            "同 id 去重 + sidecar 新增按原序追加"
        );
        // com.b 保留的是用户目录那份（path 前缀区分来源）
        assert_eq!(
            merged[1].0.path,
            PathBuf::from(r"user\com.b.json"),
            "同 id 应保留用户目录版本（覆盖分发版）"
        );
        // S-05：同 id 撞车时来源 = 用户目录（→ 不获自动信任，且设置页告警）
        assert_eq!(
            merged[1].1,
            ExtOrigin::UserDir,
            "同 id 保留用户目录项 → 来源必须是 UserDir（否则会绕开 S-05 判据）"
        );
        assert_eq!(merged[2].1, ExtOrigin::Sidecar);
    }

    #[test]
    fn merge_scanned_dirs_empty_sidecar_keeps_user_order() {
        // sidecar 无此扩展（zip 未带 / 目录不存在）→ 用户目录原样保留。
        let user = vec![
            (loaded_ext("com.a", "user"), ExtOrigin::UserDir),
            (loaded_ext("com.b", "user"), ExtOrigin::UserDir),
        ];
        let merged = merge_scanned_dirs(user, Vec::new());
        let ids: Vec<&str> = merged.iter().map(|(e, _)| e.manifest.id.as_str()).collect();
        assert_eq!(ids, ["com.a", "com.b"]);
        assert!(merged.iter().all(|(_, o)| *o == ExtOrigin::UserDir));
    }

    // ── S-05（2026-09-24）：信任门禁的接线（策略本体见 dd-host::trust 单测）──

    /// **fail-closed**：判定表缺项一律视为未获准（宁可拦下，不放行）。
    #[test]
    fn is_trusted_is_fail_closed_for_missing_entry() {
        let trust: HashMap<String, Assessment> = HashMap::new();
        assert!(
            !is_trusted(&trust, "com.unknown"),
            "缺项必须判为未获信任（fail-closed）"
        );
    }

    /// 页脚计数只统计「待批准」——「已阻止」是用户已定的决策，不再提醒。
    #[test]
    fn pending_count_excludes_blocked_and_trusted() {
        let mut trust = HashMap::new();
        trust.insert(
            "a".to_string(),
            Assessment {
                id: "a".to_string(),
                trust: Trust::Pending,
                origin: ExtOrigin::UserDir,
                sidecar_tampered: false,
                sig_invalid: false,
            },
        );
        trust.insert(
            "b".to_string(),
            Assessment {
                id: "b".to_string(),
                trust: Trust::Blocked,
                origin: ExtOrigin::UserDir,
                sidecar_tampered: false,
                sig_invalid: false,
            },
        );
        trust.insert(
            "c".to_string(),
            Assessment {
                id: "c".to_string(),
                trust: Trust::AutoTrusted,
                origin: ExtOrigin::Sidecar,
                sidecar_tampered: false,
                sig_invalid: false,
            },
        );
        assert_eq!(pending_count(&trust), 1);
        assert!(is_trusted(&trust, "c"));
        assert!(!is_trusted(&trust, "a") && !is_trusted(&trust, "b"));
    }

    /// **spawn 唯一入口的门禁**：未获信任的扩展不得被拉起（即使绕过上游过滤）。
    #[test]
    fn spawn_gate_rejects_untrusted_extension() {
        // `loaded_ext` 造的是磁盘形态清单（path 不存在）→ 无台账记录 → Pending。
        let ext = loaded_ext("com.example.untrusted", "user");
        // `.map(|_| ())`：`ExtensionProcess` 未实现 Debug，`expect_err` 需要 Ok 侧 Debug。
        let err = spawn_and_initialize_with_info(&ext)
            .map(|_| ())
            .expect_err("未获信任必须被拦下");
        // C3：错误已结构化——按变体匹配（不再对 Display 文案做 contains 判型）。
        let dd_host::error::LoadError::Untrusted { id, label } = &err else {
            panic!("应为 Untrusted，实际 {err:?}");
        };
        assert_eq!(id, "com.example.untrusted", "错误应含扩展 id");
        assert_eq!(*label, "待批准", "台账无记录 → 待批准");
        // Display 文案契约（面向用户的设置页提示）仍逐字可查：
        assert!(err.to_string().contains("未获信任"));
    }

    /// 对照：**随包首方**扩展（来源 = sidecar 目录 + 白名单）自动信任，门禁放行，
    /// 于是错误信息回到"spawn 失败"这一真实原因（而非信任拒绝）——说明门禁没有
    /// 误伤零摩擦路径。
    ///
    /// 密闭化（2026-09-30）：信任判定注入固定 `AutoTrusted`——R-12 起生产判定
    /// 会重验双哈希（fail-closed），测试的假路径与本机真实 trust.json 的钉扎
    /// 状态耦合必红；门禁「尊重判定、放行后失败于真实原因」这一语义由注入覆盖，
    /// 生产判定的三态本身由 trust.rs 的 r12 可注入单测覆盖。
    #[test]
    fn spawn_gate_passes_first_party_sidecar() {
        let mut ext = loaded_ext("com.ddrun.filesearch", "sidecar");
        ext.dir = sidecar_extensions_dir().expect("本机应能定位宿主 exe 目录"); // 使其被判为 Sidecar 来源
        let err = spawn_and_initialize_with(&ext, |_ext, _origin| Assessment {
            id: ext.manifest.id.clone(),
            trust: Trust::AutoTrusted,
            origin: ExtOrigin::Sidecar,
            sidecar_tampered: false,
            sig_invalid: false,
        })
        .map(|_| ())
        .expect_err("exe 不存在 → 仍应失败");
        assert!(
            matches!(
                &err,
                dd_host::error::LoadError::Spawn { .. }
                    | dd_host::error::LoadError::Initialize { .. }
            ),
            "首方扩展应通过信任门禁、失败于真实原因（Spawn/Initialize），实际 {err:?}"
        );
        assert!(
            !matches!(&err, dd_host::error::LoadError::Untrusted { .. }),
            "首方 sidecar 不应被信任门禁拦下：{err:?}"
        );
    }

    /// 生产判定路由（R-12）：首方 sidecar 走 `assess_sidecar`，其余走台账判定。
    /// 仅测**路由选择**（判定函数不可达注入真实台账，三态见 trust.rs r12 单测），
    /// 经由注入桩记录走了哪条分支——不触真实 trust.json。
    #[test]
    fn spawn_gate_routes_first_party_sidecar_to_sidecar_assess() {
        let mut ext = loaded_ext("com.ddrun.filesearch", "sidecar");
        ext.dir = sidecar_extensions_dir().expect("本机应能定位宿主 exe 目录");
        let mut route = None;
        let _ = spawn_and_initialize_with(&ext, |ext, origin| {
            route = Some(uses_sidecar_assessment(ext, origin));
            Assessment {
                id: ext.manifest.id.clone(),
                trust: Trust::AutoTrusted,
                origin: ExtOrigin::Sidecar,
                sidecar_tampered: false,
                sig_invalid: false,
            }
        });
        assert_eq!(
            route,
            Some(true),
            "首方 sidecar 必须路由到 assess_sidecar 分支"
        );

        // 对照：非首方 id 即便在 sidecar 目录也不走 assess_sidecar 分支
        let mut other = loaded_ext("com.acme.thing", "sidecar");
        other.dir = ext.dir.clone();
        let mut route2 = None;
        let _ = spawn_and_initialize_with(&other, |ext, origin| {
            route2 = Some(uses_sidecar_assessment(ext, origin));
            Assessment {
                id: ext.manifest.id.clone(),
                trust: Trust::AutoTrusted,
                origin: ExtOrigin::UserDir,
                sidecar_tampered: false,
                sig_invalid: false,
            }
        });
        assert_eq!(
            route2,
            Some(false),
            "非首方 id 不应进入 assess_sidecar 路由分支"
        );
    }

    #[test]
    fn maps_command_item_fields_and_fallback_section() {
        let item = cmd("a.1", "Hello", None);
        let panel = to_panel_item(&item, "com.example.a", "ExtA", Lang::ZhCn);
        assert_eq!(panel.id, "a.1", "id 透传");
        assert_eq!(panel.ext_id, "com.example.a", "来源扩展 id 透传");
        assert_eq!(panel.title, "Hello");
        assert_eq!(panel.section, "ExtA", "section 缺省用扩展名兜底");
        assert_eq!(panel.command, CommandRef::Invoke, "command 透传");

        let with_section = cmd("a.2", "Bye", Some("系统"));
        let panel = to_panel_item(&with_section, "com.example.a", "ExtA", Lang::ZhCn);
        assert_eq!(panel.section, "系统", "扩展返回的 section 优先");
    }

    #[test]
    fn category_label_is_derived_from_ext_id() {
        // M5 批次 3.9：内置扩展按去前缀映射，第三方回退「命令」。
        // v4.14 D40：类别徽标按生效语言解析（zh/en 双口径都覆盖）。
        let zh_cases = [
            ("com.ddrun.apps", "应用"),
            ("com.ddrun.calc", "命令"),
            ("com.ddrun.system", "设置"),
            ("com.ddrun.websearch", "网页"),
            ("com.ddrun.shell", "命令"),
            ("com.example.unknown", "命令"),
        ];
        for (ext_id, expected) in zh_cases {
            let panel = to_panel_item(&cmd("x", "X", None), ext_id, "Sec", Lang::ZhCn);
            assert_eq!(
                panel.result_category.as_deref(),
                Some(expected),
                "{ext_id} zh 应映射为 {expected}"
            );
        }
        let en_cases = [
            ("com.ddrun.apps", "Apps"),
            ("com.ddrun.calc", "Command"),
            ("com.ddrun.system", "Settings"),
            ("com.ddrun.websearch", "Web"),
            ("com.ddrun.shell", "Command"),
            ("com.example.unknown", "Command"),
        ];
        for (ext_id, expected) in en_cases {
            let panel = to_panel_item(&cmd("x", "X", None), ext_id, "Sec", Lang::EnUs);
            assert_eq!(
                panel.result_category.as_deref(),
                Some(expected),
                "{ext_id} en 应映射为 {expected}"
            );
        }
    }

    #[test]
    fn icon_is_passed_through_from_command_item() {
        // M5 UI 批次 2：§8.6 icon 三态（glyph/path/url）都应透传到 PanelItem，
        // 由渲染层按态渲染——此断言锁住"宿主不再丢弃 icon"这一修复。
        let kinds = [IconKind::Glyph, IconKind::Path, IconKind::Url];
        for kind in kinds {
            let icon = Icon {
                kind,
                value: match kind {
                    IconKind::Glyph => "\u{E8C8}".to_string(),
                    IconKind::Path => r"C:\demo\icon.png".to_string(),
                    IconKind::Url => "https://example.com/icon.png".to_string(),
                },
            };
            let mut item = cmd("a.icon", "Iconed", None);
            item.icon = Some(icon.clone());
            let panel = to_panel_item(&item, "com.example.a", "ExtA", Lang::ZhCn);
            assert_eq!(panel.icon, Some(icon), "{kind:?} 应透传");
        }
        // 无 icon → None（渲染空列，不 panic）
        let plain = to_panel_item(
            &cmd("b", "NoIcon", None),
            "com.example.a",
            "ExtA",
            Lang::ZhCn,
        );
        assert_eq!(plain.icon, None);
    }

    #[test]
    fn flatten_merges_warm_stub_and_keeps_failed_isolated() {
        let per_ext = vec![
            ready(
                "com.example.a",
                "Ext A",
                vec![cmd("a.1", "A1", None), cmd("a.2", "A2", Some("系统"))],
            ),
            // M3：磁盘桩（frozen 冷启动，无进程）也并入列表，源状态为 Stub
            ExtItems::Stub {
                id: "com.example.b".to_string(),
                name: "Ext B".to_string(),
                items: vec![cmd("b.1", "B1", None)],
            },
            ExtItems::Failed {
                id: "com.example.c".to_string(),
                name: "Ext C".to_string(),
                error: "initialize 失败：超时".to_string(),
            },
            ready("com.example.d", "Ext D", vec![]),
        ];

        let (items, sources) = flatten(&per_ext, Lang::ZhCn);

        // 合并 2 条(warm) + 1 条(stub) = 3；C 的失败不影响 A/B/D
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].section, "Ext A");
        assert_eq!(items[0].ext_id, "com.example.a", "项带来源扩展 id");
        assert_eq!(items[1].section, "系统");
        assert_eq!(items[2].section, "Ext B", "桩项并入列表并带 section");

        assert_eq!(sources.len(), 4);
        assert_eq!(sources[0].status, SourceStatus::Warm { commands: 2 });
        assert_eq!(sources[1].status, SourceStatus::Stub { commands: 1 });
        assert!(sources[1].status.is_stub());
        assert_eq!(
            sources[2].status,
            SourceStatus::Failed {
                error: "initialize 失败：超时".to_string()
            }
        );
        assert!(sources[2].status.is_failed());
        assert_eq!(sources[3].status, SourceStatus::Warm { commands: 0 });
    }

    #[test]
    fn flatten_empty_input() {
        let (items, sources) = flatten(&[], Lang::ZhCn);
        assert!(items.is_empty());
        assert!(sources.is_empty());
    }

    /// 诊断后缀：有诊断才加括号，无诊断不留空壳（失败文案不得出现「（诊断：）」）。
    #[test]
    fn detail_suffix_only_when_present() {
        assert_eq!(detail_suffix(None), "");
        assert_eq!(
            detail_suffix(Some("退出码 1；stderr: boom".to_string())),
            "（诊断：退出码 1；stderr: boom）"
        );
    }

    // ── doc 契约锚点（B2②，dd-run-改进分析.md §3.3）──────────────────
    // `load_one` 为本模块私有函数，锚点测试就近放置（不提可见性）。

    /// 构造子进程型 LoadedExtension（command 不存在；frozen 可调）。
    fn ghost_ext(id: &str, frozen: bool) -> LoadedExtension {
        LoadedExtension {
            manifest: dd_host::manifest::Manifest {
                schema_version: "1.0".to_string(),
                id: id.to_string(),
                name: "Ghost".to_string(),
                version: "1.0.0".to_string(),
                description: String::new(),
                author: String::new(),
                license: String::new(),
                homepage: String::new(),
                icon: None,
                entry: dd_host::manifest::Entry {
                    command: "dd-run-no-such-exe-ghost".to_string(),
                    args: vec![],
                    env: Default::default(),
                    cwd: None,
                },
                frozen,
                capabilities: vec![],
                platforms: None,
                min_host_version: None,
            },
            path: PathBuf::from("ghost.json"),
            dir: PathBuf::from("."),
            command: PathBuf::from("dd-run-no-such-exe-ghost"),
            cwd: PathBuf::from("."),
        }
    }

    /// 契约锚：docs/protocol.md §6.1 现状注（核对至 2026-09-13）。
    ///
    /// 现状注原文：落盘门禁实际读取**清单**的 `frozen` 字段（`load_one`
    /// frozen 桩分流段），握手回传的 `provider.frozen` 当前不被消费。
    ///
    /// 断言：清单 `frozen=true` 且磁盘桩命中 → 直接 `Stub`（**不 spawn**——
    /// command 指向不存在的 exe，若走了 open 路径必失败）；清单 `frozen=false`
    /// 时**同样的桩被无视** → 走 open（失败，因为 exe 不存在）。
    /// 有人把门禁改读握手回传值时此测试红，提示同步 protocol.md §6.1 注。
    #[test]
    fn frozen_gate_reads_manifest_frozen_not_handshake() {
        let dir = std::env::temp_dir().join(format!(
            "dd-gui-doc-contract-frozen-{}-{}",
            std::process::id(),
            std::time::Instant::now().elapsed().as_nanos()
        ));
        let cache = FrozenCache::new(&dir);
        let snap = FrozenSnapshot {
            ext_id: "com.ghost.frozen".to_string(),
            version: "1.0.0".to_string(),
            commands: vec![cmd("stub.item", "桩项", None)],
        };
        cache.save(&snap).expect("落桩");

        let specs = HashMap::new();

        // 清单 frozen=true + 桩命中 → Stub（不 spawn）
        let outcome = load_one(ghost_ext("com.ghost.frozen", true), &specs, Some(&cache));
        match outcome {
            ExtOutcome::Stub { items, .. } => {
                assert_eq!(items.len(), 1, "frozen=true 读桩不拉起（桩内 1 条命令）")
            }
            _ => panic!("frozen=true 且桩命中必须读桩（Stub），实际走了 open 路径"),
        }

        // 清单 frozen=false → 同一桩被无视，走 open（信任门禁/spawn 均可能失败，
        // 但**不是 Stub**——证明读桩分流只看清单侧 frozen）
        let outcome = load_one(ghost_ext("com.ghost.frozen", false), &specs, Some(&cache));
        assert!(
            matches!(outcome, ExtOutcome::Failed { .. }),
            "frozen=false 时桩被无视（任何失败都证明没读桩），却产出了非 Failed"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 契约锚：docs/extensions.md §5 现状注（核对至 2026-09-13）前半。
    ///
    /// 现状注原文：spawn 失败只置 `SourceStatus::Failed`、**不进崩溃计数**
    /// （`load_one` 产出 `ExtOutcome::Failed` → `collect_top_level` 收集 →
    /// `flatten` 落 `SourceStatus::Failed`，全程不触崩溃计数）。
    ///
    /// 断言：注入信任放行（`spawn_and_initialize_with` 缝，与
    /// `spawn_gate_passes_first_party_sidecar` 同款）后，spawn 失败（exe 不存在）
    /// → `Err` 且错误带被尝试的命令路径（可操作线索）→ `flatten` 落
    /// `SourceStatus::Failed`。「不触崩溃计数」为结构性保证（崩溃计数写点仅在
    /// `record_crash`——巡检/invoke/page 三处运行期路径，聚合链路不可达）。
    /// 「连续 3 次 spawn 失败熔断」未实现 = `no-assert`（负向能力断言无载体，
    /// 补实现时同步 extensions.md §5 注）。
    #[test]
    fn spawn_failure_yields_failed_outcome() {
        let ext = ghost_ext("com.ghost.dead", false);
        let err = spawn_and_initialize_with(&ext, |_ext, _origin| Assessment {
            id: ext.manifest.id.clone(),
            trust: Trust::AutoTrusted,
            origin: ExtOrigin::UserDir,
            sidecar_tampered: false,
            sig_invalid: false,
        })
        .map(|_| ())
        .expect_err("exe 不存在 → spawn 失败");
        // C3：结构化匹配 + Display 文案契约（命令路径线索保留）。
        let dd_host::error::LoadError::Spawn { reason, command } = &err else {
            panic!("应为 Spawn，实际 {err:?}");
        };
        assert!(
            command.contains("dd-run-no-such-exe-ghost"),
            "失败必须带被尝试的命令路径（可操作线索），实际 {command}"
        );
        assert!(
            !reason.is_empty(),
            "spawn 根因应透传（os error / program not found）"
        );
        // flatten 落 Failed（extensions.md §5 注的链路口径）
        let per_ext = vec![ExtItems::Failed {
            id: "com.ghost.dead".to_string(),
            name: "Ghost".to_string(),
            error: err.to_string(),
        }];
        let (_, sources) = flatten(&per_ext, Lang::ZhCn);
        assert!(matches!(sources[0].status, SourceStatus::Failed { .. }));
    }
}
