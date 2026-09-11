//! 命令执行：`invoke` 发起/复热/轮询 + Confirm 重发。

use crate::app::pool::invoke_on;
use crate::app::toast::ToastKind;
use crate::app::PaletteApp;
use crate::ext_client::ExtClient;
use dd_gui::result;
use dd_host::manifest::LoadedExtension;
use dd_protocol::messages::InvokeParams;
use dd_protocol::model::{CommandRef, CommandResult};
use eframe::egui;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;

/// 后台 `invoke` 的结果（客户端随结果归还主线程）。
pub(crate) struct InvokeOutcome {
    pub(crate) ext_id: String,
    /// `Some` = 客户端对象（成功或链路内错误都归还，由 poll 按 `stub_reheat` 决定取舍）；
    /// `None` = 复热打开本身失败（无客户端可归还）。
    pub(crate) proc: Option<ExtClient>,
    pub(crate) result: Result<CommandResult, String>,
    /// 本次是否由**桩复热**发起（spawn 的新进程）：失败时不归还进程、回退 stub。
    pub(crate) stub_reheat: bool,
}

impl PaletteApp {
    /// `invoke` 结果：归还/取舍进程（M3 按是否桩复热）→ 裁决 8 种 Kind → 应用动作。
    pub(crate) fn poll_invoke(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.invoke_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(outcome) => {
                let InvokeOutcome {
                    ext_id,
                    proc,
                    result,
                    stub_reheat,
                } = outcome;
                self.inflight.remove(&ext_id);
                self.invoke_rx = None;
                match result {
                    Ok(command_result) => {
                        if let Some(p) = proc {
                            self.store_warm_process(ext_id.clone(), p);
                        }
                        if stub_reheat {
                            self.mark_source_warm(&ext_id);
                            eprintln!("[dd-gui] 桩复热成功：ext={ext_id} 转 warm（LRU 保活）");
                        }
                        let action = result::resolve(&command_result);
                        eprintln!("[dd-gui] invoke 成功：{command_result:?} → 动作 {action:?}");
                        self.apply_action(ctx, action, &ext_id);
                    }
                    Err(e) => {
                        if stub_reheat {
                            // 复热失败：新进程不归还（drop 即强杀），扩展保持 stub（A6 回退）
                            eprintln!("[dd-gui] 桩复热失败：ext={ext_id}，回退 stub：{e}");
                        } else if let Some(mut p) = proc {
                            if p.has_exited() {
                                // A8：进程在调用期间崩溃——丢弃死进程，命令回落 stub
                                // （下次点击走复热 spawn），宿主继续运行；连续计数进熔断（§11）。
                                let detail = p.failure_detail();
                                eprintln!(
                                    "[dd-gui] invoke 失败：ext={ext_id} 进程已退出（A8 崩溃恢复），丢弃死进程回落 stub：{e}{}",
                                    detail
                                        .as_deref()
                                        .map(|d| format!("；诊断：{d}"))
                                        .unwrap_or_default()
                                );
                                self.drop_source_to_stub(&ext_id);
                                self.record_crash(&ext_id, detail.as_deref());
                            } else {
                                // warm 请求失败但进程还活着：进程归还（超时/错误一般可恢复）
                                self.store_warm_process(ext_id.clone(), p);
                            }
                        }
                        eprintln!("[dd-gui] invoke 失败：{e}");
                        self.show_error_toast(self.tr("toast.invoke_fail").replace("{e}", &e));
                    }
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.invoke_rx = None,
        }
    }
}

impl PaletteApp {
    // ── 命令执行 ─────────────────────────────────────────────

    /// Enter/单击：按 `CommandRef` 分派（执行 / 进入页）。
    /// M3：扩展进程 warm → 直接执行；未 warm（frozen 桩）→ 复热后执行（A6）。
    pub(crate) fn confirm_selected(&mut self) {
        let Some(item) = self.stack.current().list.confirm().cloned() else {
            return;
        };
        let query = self.stack.current().list.query().to_owned();
        match &item.command {
            CommandRef::Invoke => {
                let params = result::invoke_params(&item.id, &query);
                self.dispatch_invoke(&item.ext_id, params);
            }
            CommandRef::Page { page_id } => {
                let page_id = page_id.clone();
                let search = (!query.is_empty()).then_some(query);
                self.open_page(&item.ext_id, &page_id, search, Some(item.id.clone()));
            }
        }
    }
}

impl PaletteApp {
    /// `invoke` 分派：warm 进程在 → 直接后台执行；不在 → 桩复热（A6）。
    /// M4：熔断（连续崩溃，§11）的扩展**不再尝试 spawn**，等重启/手动重试。
    pub(crate) fn dispatch_invoke(&mut self, ext_id: &str, params: InvokeParams) {
        if self.invoke_rx.is_some() || self.inflight.contains(ext_id) {
            eprintln!("[dd-gui] invoke 失败：ext={ext_id} 上一请求仍在处理");
            self.show_toast_kind(ToastKind::Error, self.tr("toast.ext_busy"), Some(2_000));
            return;
        }
        if self.is_crash_tripped(ext_id) {
            eprintln!("[dd-gui] invoke 拒绝：ext={ext_id} 暂时不可用（连续崩溃熔断）");
            self.show_toast_kind(
                ToastKind::Error,
                self.tr("toast.ext_unavailable").replace("{id}", ext_id),
                Some(2_500),
            );
            return;
        }
        if self.processes.iter().any(|(id, _)| id == ext_id) {
            self.start_invoke(ext_id, params);
        } else if let Some(ext) = self.find_ext(ext_id).cloned() {
            // L5：兜底模板 id 不在扩展顶层命令注册表内 → §6.4 校验对其必然失败，
            // 模板项跳过校验直接 invoke（常规命令保持 §6.4 语义不变）。
            let skip_lookup = self.fallback_store.contains_template(ext_id, &params.id);
            self.start_invoke_reheat(&ext, params, skip_lookup); // 桩复热
        } else {
            eprintln!("[dd-gui] invoke 失败：ext={ext_id} 无扩展信息");
            self.show_error_toast(self.tr("toast.ext_missing"));
        }
    }

    /// 后台 `invoke`（warm：take 进程 → 线程调用 → 结果经 channel 归还）。
    pub(crate) fn start_invoke(&mut self, ext_id: &str, params: InvokeParams) {
        self.last_command_id = Some(params.id.clone());
        self.last_invoke = Some(params.clone()); // Confirm 重发沿用
        eprintln!("[dd-gui] invoke 发起：ext={ext_id} cmd={}", params.id);
        let Some(idx) = self.processes.iter().position(|(id, _)| id == ext_id) else {
            eprintln!("[dd-gui] invoke 失败：ext={ext_id} 进程不可用（可能 in-flight）");
            self.show_error_toast(self.tr("toast.ext_busy"));
            return;
        };
        let (_, mut proc) = self.processes.remove(idx);
        self.inflight.insert(ext_id.to_string());
        let ext_id = ext_id.to_string();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = invoke_on(&mut proc, &params);
            let _ = tx.send(InvokeOutcome {
                ext_id,
                proc: Some(proc),
                result,
                stub_reheat: false,
            });
        });
        self.invoke_rx = Some(rx);
    }

    /// 桩复热 + `invoke`（A6 / 协议 §6.4）：spawn → initialize → `get_command(id)` → invoke。
    /// 复热失败（spawn/握手/命令失效/超时）→ 不保活新进程、扩展保持 stub 并报错。
    ///
    /// `skip_lookup` = 目标 id 是**兜底模板**（`FallbackStore::contains_template`）：
    /// 模板不在顶层命令注册表内，§6.4 校验必然回 `null`，故跳过校验直接 invoke —— L5 修复
    /// （否则扩展被 LRU 驱逐成 stub 后，点兜底项报「命令已失效」且 invoke 从未发出）。
    pub(crate) fn start_invoke_reheat(
        &mut self,
        ext: &LoadedExtension,
        params: InvokeParams,
        skip_lookup: bool,
    ) {
        self.last_command_id = Some(params.id.clone());
        self.last_invoke = Some(params.clone());
        eprintln!(
            "[dd-gui] 桩复热：ext={} cmd={}（spawn→initialize→{}→invoke）",
            ext.manifest.id,
            params.id,
            if skip_lookup {
                "跳过 get_command（兜底模板）"
            } else {
                "get_command"
            }
        );
        self.inflight.insert(ext.manifest.id.clone());
        let ext = ext.clone();
        let ext_id = ext.manifest.id.clone();
        // M9：内置扩展按 in-process 重建（spec 直调），其余 spawn 子进程。
        let spec = self.inproc_specs.get(&ext_id).cloned();
        let (tx, rx) = mpsc::channel();
        let lang = self.lang_effective; // 线程闭包无法借 self：按值捕获生效语言
        thread::spawn(move || {
            let mut proc = match crate::ext_client::open(spec, Some(&ext)) {
                Ok((p, _init)) => p,
                Err(e) => {
                    // spawn/握手失败：无进程可归还，直接报错
                    let _ = tx.send(InvokeOutcome {
                        ext_id,
                        proc: None,
                        result: Err(e),
                        stub_reheat: true,
                    });
                    return;
                }
            };
            let result: Result<CommandResult, String> = if skip_lookup {
                // L5：兜底模板不在顶层注册表内，§6.4 校验必失败 → 直接执行
                invoke_on(&mut proc, &params)
            } else {
                match proc.get_command(&params.id).map_err(|e| e.to_string()) {
                    // §6.4：取回真实命令后再执行
                    Ok(Some(_)) => invoke_on(&mut proc, &params),
                    Ok(None) => Err(crate::text::t(lang, "page.cmd_stale").to_string()),
                    Err(e) => Err(e),
                }
            };
            let _ = tx.send(InvokeOutcome {
                ext_id,
                proc: Some(proc),
                result,
                stub_reheat: true,
            });
        });
        self.invoke_rx = Some(rx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ctx, dying_client, make_app};
    use dd_gui::aggregator::{SourceStatus, SourceSummary};

    #[test]
    fn poll_invoke_on_dead_process_drops_to_stub_and_records_crash() {
        let mut app = make_app();
        let c = ctx();
        let ext_id = "com.example.dying";
        app.sources.push(SourceSummary {
            id: ext_id.to_string(),
            name: "Dying".to_string(),
            status: SourceStatus::Warm { commands: 3 },
        });
        // 保活集放一个死进程（供 drop_source_to_stub 移除断言）
        app.processes
            .push((ext_id.to_string(), dying_client(ext_id)));

        // 构造「调用期间进程崩溃」的 InvokeOutcome（proc 为另一个死进程）
        let (tx, rx) = mpsc::channel();
        let _ = tx.send(InvokeOutcome {
            ext_id: ext_id.to_string(),
            proc: Some(dying_client(ext_id)),
            result: Err("进程在调用期间崩溃".to_string()),
            stub_reheat: false,
        });
        app.invoke_rx = Some(rx);
        app.inflight.insert(ext_id.to_string());

        app.poll_invoke(&c);

        assert!(
            app.processes.iter().all(|(id, _)| id != ext_id),
            "死进程应从保活集移除（回落 stub）"
        );
        assert!(
            app.crash_guards
                .get(ext_id)
                .map(|g| g.consecutive())
                .unwrap_or(0)
                >= 1,
            "应记录一次崩溃"
        );
    }
}
