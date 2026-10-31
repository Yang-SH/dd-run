//! 批次 B（refactor-structure-2026-10-21.md §4.2）：扩展目录扫描与信任归并。
//! 自 `aggregator.rs` 原块（219–500 行）逐字节搬入、零逻辑改动；
//! 外部路径经 `aggregator/mod.rs` 的 `pub use` 桥保持不变。

use std::collections::HashMap;
use std::path::PathBuf;

use crate::settings::Lang;
use crate::text;
use dd_ext::ExtensionSpec;
use dd_host::builtin::merge_builtins;
use dd_host::manifest::{self, LoadedExtension, ScanOptions};
use dd_host::trust::{self, Assessment, ExtOrigin, LedgerState};

use super::model::HOST_VERSION;

/// [`load_extension_sources`] 的返回（S-05 起由 3 元组改为具名结构：字段增至 5 个，
/// 元组已不可读）。
pub struct ExtensionSources {
    /// 全部已扫描扩展（内置 + 磁盘）。**含**未获信任者——设置页要展示它们并提供
    /// 「允许 / 阻止」；只进入 spawn 集合的是经 [`is_trusted`] 过滤后的子集。
    pub exts: Vec<LoadedExtension>,
    /// 内置 in-process 规格表（`id → ExtensionSpec`），仅内置 6 个。
    pub inproc_specs: HashMap<String, ExtensionSpec>,
    /// 异常备注（仅目录不可读等异常时非空）。
    pub note: String,
    /// R-20：解析失败的清单（路径, 原因）——设置页扩展卡顶部警告行的数据源
    /// （仅含 `SkipReason::is_error()` 的条目；`OtherPlatform` 类**设计上静默**
    /// 的跳过不进此表）。此前该信息被整体丢弃，扩展「无声消失」无从排查。
    pub skipped: Vec<(String, String)>,
    /// `id → 信任判定`（S-05）：设置页据此渲染来源标识与「待批准 / 已阻止」。
    pub trust: HashMap<String, Assessment>,
    /// 信任台账读取状态（`Corrupt` 时设置页须给出可操作提示，而非静默失效）。
    pub ledger_state: LedgerState,
}

/// 该扩展是否获准拉起。**fail-closed**：判定表缺项一律视为未获准
/// （正常情况下每个已扫描扩展都在表内；缺项意味着上游漏判，此时宁可拦下）。
pub fn is_trusted(trust: &HashMap<String, Assessment>, id: &str) -> bool {
    trust.get(id).map(|a| a.is_trusted()).unwrap_or(false)
}

/// 待用户批准的扩展数（面板页脚提示用；`Blocked` 是用户已定的决策，不再提醒）。
pub fn pending_count(trust: &HashMap<String, Assessment>) -> usize {
    trust.values().filter(|a| a.is_pending()).count()
}

/// 从清单目录推断扩展来源（S-05）。
///
/// **内置不走此函数**：其 `command` 是名义路径（`dd-ext-*.exe`）、从不 spawn，
/// 来源在 [`load_extension_sources`] 内由 `specs` 命中直接判为 [`ExtOrigin::Builtin`]。
///
/// 判定 = 清单目录是否等于「宿主 exe 同目录的 `extensions.d`」。两侧都做
/// canonicalize 以容忍 `..` / 符号链接等写法差异；任一侧不可解析时回落为字面比较。
pub fn origin_of(ext: &LoadedExtension) -> ExtOrigin {
    match sidecar_extensions_dir() {
        Some(d) if same_dir(&ext.dir, &d) => ExtOrigin::Sidecar,
        _ => ExtOrigin::UserDir,
    }
}

/// 两个目录是否指向同一位置（canonicalize 优先，失败回落字面比较）。
fn same_dir(a: &std::path::Path, b: &std::path::Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// 扫描扩展目录并**合并内置扩展**（M4 P4 → M9 in-process）。
///
/// 返回 [`ExtensionSources`]（S-05 起）：
/// - **扩展列表**：内置 6 个（**恒注册，不依赖磁盘 exe**；N3 起含 bookmarks）+ 扩展目录中的第三方
///   清单（同 id 以内置优先，`merge_builtins` 去重）。列表用于 UI 展示与
///   id/名称查询；内置项的 `command` 为名义路径，**不会被 spawn**。
/// - **规格表**：`id → ExtensionSpec`，仅内置 6 个。宿主据此以 in-process 方式
///   驱动内置扩展（[`crate::ext_client::ExtClient::open_builtin`]）。
/// - **信任判定表**：每个扩展一条（内置判为 `Builtin` → 自动信任）。
/// - **备注**：仅在异常时非空（目录不可读），供 UI 提示。M9 起内置恒可用，
///   不再有"找不到内置 exe"类备注。
pub fn load_extension_sources(lang: Lang) -> ExtensionSources {
    // M9：内置扩展恒注册（in-process，无需 exe / 无需物化内嵌 exe）。
    let builtins = dd_host::builtin::builtin_registrations();
    // 运行期规格（按生效语言构造；宿主已在聚合前经 `dd_ext::i18n::set_lang` 设语言）。
    let specs: HashMap<String, ExtensionSpec> = dd_ext::builtins::builtin_specs()
        .into_iter()
        .map(|s| (s.id.to_string(), s))
        .collect();

    let mut note = String::new();
    // R-20：解析失败清单（路径, 原因）随扫描收集（仅错误类跳过）。
    let mut skipped: Vec<(String, String)> = Vec::new();
    // 扫描结果**携带来源**（S-05）：去重前先记下，合并后仍能判定
    // （`merge_scanned_dirs` 同 id 保留用户目录项 → 该 id 的来源即 UserDir）。
    let scanned = merge_sidecar_scan(
        manifest::extensions_dir(),
        sidecar_extensions_dir(),
        &mut note,
        &mut skipped,
    );
    let origin_by_id: HashMap<String, ExtOrigin> = scanned
        .iter()
        .map(|(e, o)| (e.manifest.id.clone(), *o))
        .collect();
    let mut merged = merge_builtins(builtins, scanned.into_iter().map(|(e, _)| e).collect());
    // 显示名本地化：清单 `name` 是单串、无 i18n（schema v1.0 冻结），故宿主自有
    // 扩展的名称在注册后统一覆盖（内置取自述 display_name）。
    apply_owned_names(&mut merged, &specs, lang);
    if merged.is_empty() && note.is_empty() {
        note = "无可用扩展（内置与扩展目录均为空）".to_string();
    }

    // S-05：信任判定（台账读一次，逐扩展短路判定——首方与内置不做哈希，故常见
    // 情形零额外 I/O）。
    let (ledger, ledger_state) = trust::TrustLedger::load_with_state();
    if ledger_state == LedgerState::Corrupt {
        log::warn!(
            "[dd-gui] 信任台账损坏/版本不识别 → 视作空台账：所有用户安装的扩展回到「待批准」\
             （设置页可逐个批准；文件不会因此被删）"
        );
    }
    let mut trust_map = HashMap::with_capacity(merged.len());
    for ext in &merged {
        let origin = if specs.contains_key(&ext.manifest.id) {
            ExtOrigin::Builtin
        } else {
            origin_by_id
                .get(&ext.manifest.id)
                .copied()
                .unwrap_or(ExtOrigin::UserDir)
        };
        let a = if origin == ExtOrigin::Sidecar && trust::is_first_party_id(&ext.manifest.id) {
            // R-12：随包首方 sidecar 走首跑钉扎三态（不再白名单短路）——
            // 首钉 / 每次 spawn 重验双哈希 / 同版篡改 → Pending+告警 / 升版静默重钉
            trust::assess_sidecar(ext, HOST_VERSION)
        } else {
            trust::assess(ext, origin, &ledger)
        };
        if !a.is_trusted() {
            log::info!(
                "[dd-gui] 扩展 {} 未获信任（trust={:?}, origin={:?}）→ 不拉起；设置页「扩展」可批准",
                a.id,
                a.trust,
                a.origin
            );
        }
        if a.sidecar_tampered {
            log::warn!(
                "[dd-gui] 随包扩展 {} 与钉扎哈希不符（宿主版本未变）——疑似被替换，已拦下；设置页已告警",
                a.id
            );
        }
        if a.shadows_first_party() {
            log::warn!(
                "[dd-gui] 扩展 {} 与随包扩展同 id 但来自用户数据目录（会顶掉随包版本）——设置页已告警",
                a.id
            );
        }
        trust_map.insert(a.id.clone(), a);
    }

    ExtensionSources {
        exts: merged,
        inproc_specs: specs,
        note,
        skipped,
        trust: trust_map,
        ledger_state,
    }
}

/// 覆盖**宿主自有扩展**的显示名（本地化）。
///
/// - **内置 6 个**：用扩展自述的 `display_name`（`dd_ext::builtins::builtin_specs()`
///   已按生效语言构造）——宿主不重复维护名称（单一事实来源）。
/// - **随包 sidecar**（非内置，如文件搜索 `com.ddrun.filesearch`）：清单 `name`
///   无 i18n 字段（manifest-schema v1.0 冻结），故对宿主自有 id 用宿主文案表覆盖。
/// - **第三方清单名按作者提供原样**——宿主不臆测翻译。
pub(super) fn apply_owned_names(
    exts: &mut [LoadedExtension],
    specs: &HashMap<String, ExtensionSpec>,
    lang: Lang,
) {
    for ext in exts {
        if let Some(spec) = specs.get(&ext.manifest.id) {
            ext.manifest.name = spec.display_name.to_string();
        } else if let Some(key) = owned_sidecar_name_key(&ext.manifest.id) {
            ext.manifest.name = text::t(lang, key).to_string();
        }
    }
}

/// 宿主自有 sidecar（随包分发、非内置）的本地化名文案键。
pub(super) fn owned_sidecar_name_key(id: &str) -> Option<&'static str> {
    match id {
        "com.ddrun.filesearch" => Some("ext.name.filesearch"),
        _ => None,
    }
}

/// 便携 sidecar 扩展目录：宿主 exe 同目录的 `extensions.d/`（M7 批次 7.5）。
///
/// 免安装分发的 zip 布局为 `dd-run-<ver>.exe + extensions.d/`（file-search
/// sidecar 随包携带），解压后与 exe 的相对位置不变——扫描此目录即可
/// 「解压即用」，无需先把清单拷入用户数据目录。
/// 开发期 / 内嵌物化目录下无此子目录：`scan_dir` 对不存在目录视作空（非错误）。
pub fn sidecar_extensions_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("extensions.d")))
}

/// 合并用户目录与便携 sidecar 两处扫描结果：**同 id 保留 `first`**（用户目录
/// 优先——用户手动放置的版本覆盖分发自带版本），`second` 其余按原序追加。
/// 与内置扩展的去重（内置最优先）由 [`merge_builtins`] 负责。
///
/// S-05：元素携带来源（[`ExtOrigin`]）——同 id 保留的是用户目录项，故该 id 的来源
/// 即 `UserDir`，与「是否可能自动信任」的判定天然一致（D4：不改本语义）。
pub(super) fn merge_scanned_dirs(
    mut first: Vec<(LoadedExtension, ExtOrigin)>,
    second: Vec<(LoadedExtension, ExtOrigin)>,
) -> Vec<(LoadedExtension, ExtOrigin)> {
    for item in second {
        if !first
            .iter()
            .any(|(e, _)| e.manifest.id == item.0.manifest.id)
        {
            first.push(item);
        }
    }
    first
}

/// 扫描用户数据目录 `extensions.d` + 便携 sidecar，异常记入 `note`；
/// R-20：解析失败的清单（路径, 原因，仅 `is_error()` 条目）同步收集进
/// `skipped`——设置页扩展卡顶部警告行的数据源。
/// 返回值携带每个扩展的[来源][ExtOrigin]（S-05）。`sidecar_dir` 参数化便于
/// 单测注入（生产传 [`sidecar_extensions_dir`]）。
pub(super) fn merge_sidecar_scan(
    user_dir: Option<PathBuf>,
    sidecar_dir: Option<PathBuf>,
    note: &mut String,
    skipped: &mut Vec<(String, String)>,
) -> Vec<(LoadedExtension, ExtOrigin)> {
    let collect_skipped = |outcome: &manifest::ScanOutcome, skipped: &mut Vec<(String, String)>| {
        for s in &outcome.skipped {
            if s.reason.is_error() {
                skipped.push((s.path.display().to_string(), s.reason.to_string()));
            }
        }
    };
    let mut scanned: Vec<(LoadedExtension, ExtOrigin)> = match user_dir {
        Some(d) => {
            let outcome = manifest::scan_dir(&d, &ScanOptions::default());
            if let Some(err) = &outcome.dir_error {
                push_note(note, &format!("扩展目录不可读：{err}"));
            }
            collect_skipped(&outcome, skipped);
            outcome
                .loaded
                .into_iter()
                .map(|e| (e, ExtOrigin::UserDir))
                .collect()
        }
        None => {
            push_note(note, "无法定位扩展目录（home 环境变量缺失）");
            Vec::new()
        }
    };

    // 便携 sidecar（M7 批次 7.5）：同 id 用户目录优先。
    if let Some(dir) = sidecar_dir {
        let outcome = manifest::scan_dir(&dir, &ScanOptions::default());
        if let Some(err) = &outcome.dir_error {
            push_note(note, &format!("扩展目录不可读：{err}"));
        }
        collect_skipped(&outcome, skipped);
        scanned = merge_scanned_dirs(
            scanned,
            outcome
                .loaded
                .into_iter()
                .map(|e| (e, ExtOrigin::Sidecar))
                .collect(),
        );
    }
    scanned
}

fn push_note(note: &mut String, msg: &str) {
    if !note.is_empty() {
        note.push('；');
    }
    note.push_str(msg);
}
