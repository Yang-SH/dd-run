//! 批次 B（refactor-structure-2026-10-21.md §4.2）：扩展 spawn 与首屏收集。
//! 自 `aggregator.rs` 原块（502–763 行）逐字节搬入、零逻辑改动
//! （`collect_top_level_streaming` 为方案撰写后 C2 新增，按主题归入本文件）；
//! 外部路径经 `aggregator/mod.rs` 的 `pub use` 桥保持不变。

use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;

use dd_ext::ExtensionSpec;
use dd_host::cache::{FrozenCache, FrozenSnapshot};
use dd_host::manifest::LoadedExtension;
use dd_host::process::ExtensionProcess;
use dd_host::trust::{self, Assessment, ExtOrigin, Trust};
use dd_protocol::messages::InitializeResult;
use dd_protocol::model::CommandItem;

use super::model::{CollectResult, ExtItems, HOST_VERSION, PROTOCOL_VERSION};
use super::scan::origin_of;
use crate::ext_client::ExtClient;

/// 单个扩展线程的原始结果（携带客户端，跨线程回传）。
pub(super) enum ExtOutcome {
    Ready {
        proc: Box<ExtClient>,
        id: String,
        name: String,
        items: Vec<CommandItem>,
    },
    /// 磁盘桩命中：无进程（A6）。
    Stub {
        id: String,
        name: String,
        items: Vec<CommandItem>,
    },
    Failed {
        id: String,
        name: String,
        error: String,
    },
}

/// 流式收集首屏（C2，dd-run-改进分析.md §4.1）：每扩展一个线程，**完成序**
/// 回调——快扩展先回先渲染，不被 spawn 序里靠前的慢扩展阻塞（旧
/// [`collect_top_level`] 按 spawn 序 `join`，任一慢扩展会扣住其后全部结果）。
///
/// 线程 panic 兜底与旧实现一致：转 `Failed`（id = unknown），不让缺失一条
/// 结果错位整个流。回调在 scoped thread 全部结束后返回（父线程循环 recv）。
pub fn collect_top_level_streaming(
    exts: &[LoadedExtension],
    specs: &HashMap<String, ExtensionSpec>,
    cache: Option<&FrozenCache>,
    mut on_result: impl FnMut(ExtItems, Option<ExtClient>),
) {
    thread::scope(|scope| {
        let (tx, rx) = mpsc::channel::<ExtOutcome>();
        for ext in exts {
            let ext = ext.clone();
            let tx = tx.clone();
            scope.spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    load_one(ext, specs, cache)
                }))
                .unwrap_or_else(|_| ExtOutcome::Failed {
                    id: "unknown".to_string(),
                    name: "扩展线程".to_string(),
                    error: "拉取线程 panic".to_string(),
                });
                let _ = tx.send(outcome);
            });
        }
        drop(tx);
        while let Ok(outcome) = rx.recv() {
            match outcome {
                ExtOutcome::Ready {
                    proc,
                    id,
                    name,
                    items,
                } => {
                    on_result(ExtItems::Ready { id, name, items }, Some(*proc));
                }
                ExtOutcome::Stub { id, name, items } => {
                    on_result(ExtItems::Stub { id, name, items }, None);
                }
                ExtOutcome::Failed { id, name, error } => {
                    on_result(ExtItems::Failed { id, name, error }, None);
                }
            }
        }
    });
}

/// 并行收集首屏：每扩展一个线程，客户端对象线程独占，join 回传。
///
/// M3 分流（见模块文档，**仅子进程**）：frozen + 磁盘桩命中 → [`ExtOutcome::Stub`]
/// （不 spawn）；frozen 无桩（首启）→ spawn 拉取并落盘；fresh → spawn 拉取不落盘。
/// M9：内置扩展（`specs` 命中者）走 in-process，**无 spawn / 无桩**（纯函数调用恒瞬时）。
/// `cache` 用 scoped thread 共享只读借用（`FrozenCache` 仅含目录路径，无内部状态）。
///
/// C2 起生产链路走 [`collect_top_level_streaming`]（完成序）；本函数保留作
/// 批量语义兼容（一次性收集全部，`processes` 与 `per_ext` 的 Ready 顺序一一对应）。
pub fn collect_top_level(
    exts: &[LoadedExtension],
    specs: &HashMap<String, ExtensionSpec>,
    cache: Option<&FrozenCache>,
) -> CollectResult {
    let mut processes: Vec<ExtClient> = Vec::new();
    let mut per_ext = Vec::with_capacity(exts.len());
    collect_top_level_streaming(exts, specs, cache, |item, proc| {
        if let Some(proc) = proc {
            processes.push(proc);
        }
        per_ext.push(item);
    });
    CollectResult { processes, per_ext }
}

/// spawn → `initialize`（握手+版本协商）。供首屏拉取与 **GUI 桩复热链路**复用。
pub fn spawn_and_initialize(
    ext: &LoadedExtension,
) -> Result<ExtensionProcess, dd_host::error::LoadError> {
    spawn_and_initialize_with_info(ext).map(|(proc, _)| proc)
}

/// 同 [`spawn_and_initialize`]，但一并返回握手结果（`ProviderInfo`）。
///
/// 用途：宿主需据 `provider.has_fallback` 决定是否落 frozen 桩
/// （设计文档 §6.3：含兜底能力者一律视为 fresh，不落桩）。
pub fn spawn_and_initialize_with_info(
    ext: &LoadedExtension,
) -> Result<(ExtensionProcess, InitializeResult), dd_host::error::LoadError> {
    spawn_and_initialize_with(ext, assess_for_spawn)
}

/// R-12 路由判定：该扩展是否走 [`trust::assess_sidecar`] 分支（随包首方 sidecar）。
pub(super) fn uses_sidecar_assessment(ext: &LoadedExtension, origin: ExtOrigin) -> bool {
    origin == ExtOrigin::Sidecar && trust::is_first_party_id(&ext.manifest.id)
}

/// 生产信任判定（R-12）：随包首方 sidecar 走 `assess_sidecar` 首跑钉扎重验
/// （会读/写真实 trust.json），其余扩展走台账判定。
fn assess_for_spawn(ext: &LoadedExtension, origin: ExtOrigin) -> Assessment {
    if uses_sidecar_assessment(ext, origin) {
        trust::assess_sidecar(ext, HOST_VERSION)
    } else {
        trust::assess(ext, origin, &trust::TrustLedger::load())
    }
}

/// 同 [`spawn_and_initialize_with_info`]，但信任判定可注入（单测密闭化——
/// 不触真实 trust.json，见 `spawn_gate_passes_first_party_sidecar`）。
pub(crate) fn spawn_and_initialize_with(
    ext: &LoadedExtension,
    assess: impl FnOnce(&LoadedExtension, ExtOrigin) -> Assessment,
) -> Result<(ExtensionProcess, InitializeResult), dd_host::error::LoadError> {
    // S-05 信任门禁（**第二道，且是所有子进程 spawn 的唯一入口**）：未获信任者一律
    // 不拉起。第一道在 `load_extension_sources` 的 `active` 过滤（决定"谁进集合"）；
    // 此处兜底覆盖桩复热等一切路径——即使上游漏判也拦得住。
    //
    // R-12：随包首方 sidecar 在此**每次 spawn 重验双哈希**（首跑钉扎语义，
    // 见 `trust::assess_sidecar`）；成本为两次数 MB 内的读盘，仅在 spawn 时发生。
    let origin = origin_of(ext);
    let a = assess(ext, origin);
    if !a.is_trusted() {
        let label = match a.trust {
            Trust::Pending => "待批准",
            Trust::Blocked => "已被阻止",
            Trust::AutoTrusted => unreachable!("is_trusted() 为真时不会进入此分支"),
        };
        return Err(dd_host::error::LoadError::Untrusted {
            id: ext.manifest.id.clone(),
            label,
        });
    }
    // 失败信息一律带**可操作线索**：spawn 失败附被尝试的命令路径（PATH / 路径类
    // 问题一眼可见），握手失败附扩展 stderr 末行（根因通常就在那里）。
    // 2026-09-10 真机反馈驱动：此前只报「spawn 失败：os error 2」，排查只能靠猜。
    // （C3：文案经 `LoadError::Display` 输出，与拼接版逐字一致。）
    let mut spawned =
        ExtensionProcess::spawn(ext).map_err(|e| dd_host::error::LoadError::Spawn {
            reason: e.to_string(),
            command: ext.command.display().to_string(),
        })?;
    let init = match spawned.initialize(PROTOCOL_VERSION, HOST_VERSION) {
        Ok(init) => init,
        Err(e) => {
            return Err(dd_host::error::LoadError::Initialize {
                reason: e.to_string(),
                detail: detail_suffix(spawned.failure_detail()),
            })
        }
    };
    Ok((spawned, init))
}

/// 追加诊断后缀：`（诊断：退出码 1；stderr: ...）`；无诊断则空串。
pub(super) fn detail_suffix(detail: Option<String>) -> String {
    detail.map(|d| format!("（诊断：{d}）")).unwrap_or_default()
}

/// 一个扩展的完整链路：M3 分流（仅子进程）后 open → initialize → top_level_commands（+落盘）。
///
/// M9：内置扩展（`specs` 命中）走 in-process——不 spawn、不读/落磁盘桩（调用为纯
/// 函数，恒"瞬时可用"，`SourceStatus::Warm`），顶层命令拉到即 Ready。
pub(super) fn load_one(
    ext: LoadedExtension,
    specs: &HashMap<String, ExtensionSpec>,
    cache: Option<&FrozenCache>,
) -> ExtOutcome {
    let id = ext.manifest.id.clone();
    let name = ext.manifest.name.clone();
    let version = ext.manifest.version.clone();
    let in_process = specs.contains_key(&id);

    // M3（仅子进程）：frozen 且磁盘桩命中（键 = id + version，`FrozenCache::load`
    // 已按当前 version 精确定位）→ **不拉起进程**（A6），首屏直接渲染桩。
    if !in_process && ext.manifest.frozen {
        if let Some(snap) = cache.and_then(|c| c.load(&id, &version)) {
            return ExtOutcome::Stub {
                id,
                name,
                items: snap.commands,
            };
        }
    }

    // 无桩（frozen 首启）/ fresh / 内置 in-process：open → initialize → top_level_commands。
    let (mut client, init) = match crate::ext_client::open(specs.get(&id).cloned(), Some(&ext)) {
        Ok(pair) => pair,
        Err(e) => {
            return ExtOutcome::Failed {
                id,
                name,
                error: e.to_string(),
            }
        }
    };
    // §6.3：含兜底能力者一律视为 fresh——不落桩；若历史桩存在则清除，
    // 避免下次冷启动读桩（无进程 → fallback_commands 拉不到）。
    let has_fallback = init.provider.has_fallback;
    match client.top_level_commands() {
        Ok(items) => {
            // M9：内置 in-process 不参与磁盘桩缓存（无 spawn 成本，读桩反而多一次
            // 文件读取且可能拿到旧文案）；仅子进程走 M3 落桩/清桩。
            if !in_process {
                if ext.manifest.frozen && !has_fallback {
                    // M3：frozen 成功拉取 → 落盘桩（下次冷启动读桩不拉起）。
                    // 先清同 id 的旧版本桩，避免旧文件残留；落盘失败不致命（本次仍 warm
                    // 服务，仅下次冷启动退化为再拉一次）。
                    if let Some(c) = cache {
                        c.invalidate_if_version_changed(&id, &version);
                        let snap = FrozenSnapshot {
                            ext_id: id.clone(),
                            version: version.clone(),
                            commands: items.clone(),
                        };
                        let _ = c.save(&snap);
                    }
                } else if has_fallback {
                    // fresh（含兜底）：确保磁盘上没有它的桩文件
                    if let Some(c) = cache {
                        let _ = c.remove(&id);
                    }
                }
            }
            ExtOutcome::Ready {
                proc: Box::new(client),
                id,
                name,
                items,
            }
        }
        Err(e) => ExtOutcome::Failed {
            id,
            name,
            error: format!(
                "top_level_commands 失败：{e}{}",
                detail_suffix(client.failure_detail())
            ),
        },
    }
}
