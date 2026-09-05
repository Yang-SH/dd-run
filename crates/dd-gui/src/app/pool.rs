//! warm 进程池：LRU 保活、进程取还、桩回落与复热标记。

use crate::app::PaletteApp;
use dd_gui::aggregator::SourceStatus;
use dd_host::manifest::LoadedExtension;
use dd_host::process::ExtensionProcess;
use dd_host::process::TIMEOUT_GET_ITEMS;
use dd_protocol::messages::{GetItemsParams, GetItemsResult, InvokeParams};
use dd_protocol::model::CommandResult;
use std::thread;

/// M3 LRU 保活容量（设计文档 §6.3"最近 N 个"；超出则 close+释放、命令回落 stub，A7）。
pub(crate) const LRU_WARM_CAPACITY: usize = 8;

impl PaletteApp {
    /// 按清单 id 找已扫描扩展（复热 spawn 用）。
    pub(crate) fn find_ext(&self, ext_id: &str) -> Option<&LoadedExtension> {
        self.exts.iter().find(|e| e.manifest.id == ext_id)
    }
}

impl PaletteApp {
    /// 进程归还入口（M3）：写回 warm 集 + LRU 触达；超容驱逐最久未用者（A7）。
    pub(crate) fn store_warm_process(&mut self, ext_id: String, proc: ExtensionProcess) {
        self.processes.push((ext_id.clone(), proc));
        if let Some(victim) = self.lru.access(&ext_id) {
            if victim != ext_id {
                self.evict_warm(&victim);
            }
        }
    }

    /// LRU 驱逐（A7）：close + 终止进程、从保活集移除、源状态回落 stub。
    /// 优雅 close 走后台线程（≤1s+1s 超时），避免卡 UI；失败强杀由 Drop 兜底。
    pub(crate) fn evict_warm(&mut self, victim: &str) {
        if let Some(idx) = self.processes.iter().position(|(id, _)| id == victim) {
            let (_, proc) = self.processes.remove(idx);
            thread::spawn(move || {
                let _ = proc.close();
            });
            eprintln!("[dd-gui] LRU 驱逐：{victim}（warm 超容量，close+释放，命令回落 stub）");
        }
        self.lru.remove(victim);
        if let Some(s) = self.sources.iter_mut().find(|s| s.id == victim) {
            if !s.status.is_failed() {
                let n = match &s.status {
                    SourceStatus::Warm { commands } | SourceStatus::Stub { commands } => *commands,
                    SourceStatus::Failed { .. } => 0,
                };
                s.status = SourceStatus::Stub { commands: n };
            }
        }
    }

    /// A8：进程已退出（崩溃或正常退出）时把该扩展从保活集移除、LRU 清出、源状态回落 stub。
    /// 进程对象本身由调用方 drop（此处仅清理宿主侧簿记，供下次点击走复热 spawn）。
    pub(crate) fn drop_source_to_stub(&mut self, ext_id: &str) {
        self.processes.retain(|(pid, _)| pid != ext_id);
        self.lru.remove(ext_id);
        if let Some(s) = self.sources.iter_mut().find(|s| s.id == ext_id) {
            if !s.status.is_failed() {
                let n = match &s.status {
                    SourceStatus::Warm { commands } | SourceStatus::Stub { commands } => *commands,
                    SourceStatus::Failed { .. } => 0,
                };
                s.status = SourceStatus::Stub { commands: n };
            }
        }
    }

    /// 源状态转 warm（桩复热成功 / cold start warm 时调用；Failed→Warm 同理恢复）。
    pub(crate) fn mark_source_warm(&mut self, ext_id: &str) {
        if let Some(s) = self.sources.iter_mut().find(|s| s.id == ext_id) {
            if s.status.is_stub() || s.status.is_failed() {
                let n = match &s.status {
                    SourceStatus::Warm { commands } | SourceStatus::Stub { commands } => *commands,
                    SourceStatus::Failed { .. } => 0,
                };
                s.status = SourceStatus::Warm { commands: n };
            }
        }
        // M4/§11：warm = 成功恢复 → 清零连续崩溃计数（解除熔断）
        self.reset_crash(ext_id);
    }
}

/// 在给定进程上执行一次 `invoke`（协议 §6.5），返回 `CommandResult` 本体。
///
/// 委托 [`ExtensionProcess::invoke`]（M4 P4：协议方法封装上提至 dd-host，
/// 序列化/信封解析只写一份）。`call` 已解开 JSON-RPC 信封，内层 `result`
/// 即 §8.3 `CommandResult` 本体——不能按 `InvokeResult` 再包一层（M2 修复记录）。
pub(crate) fn invoke_on(
    proc: &mut ExtensionProcess,
    params: &InvokeParams,
) -> Result<CommandResult, String> {
    proc.invoke(params).map_err(|e| e.to_string())
}

/// 在给定进程上全量拉取一页（协议 §6.3 `get_items`）。
pub(crate) fn get_items_on(
    proc: &mut ExtensionProcess,
    page_id: &str,
    search: Option<String>,
) -> Result<GetItemsResult, String> {
    let params = GetItemsParams {
        page_id: page_id.to_string(),
        search_text: search,
    };
    serde_json::to_value(&params)
        .map_err(|e| format!("参数序列化失败：{e}"))
        .and_then(|v| {
            proc.call("get_items", v, TIMEOUT_GET_ITEMS)
                .map_err(|e| e.to_string())
        })
        .and_then(|v| {
            serde_json::from_value::<GetItemsResult>(v).map_err(|e| format!("响应解析失败：{e}"))
        })
}
