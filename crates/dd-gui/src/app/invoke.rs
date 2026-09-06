//! 命令执行：`invoke` 发起/复热/轮询 + Confirm 重发。

use crate::app::pool::invoke_on;
use crate::app::toast::ToastKind;
use crate::app::PaletteApp;
use dd_gui::aggregator;
use dd_gui::result;
use dd_host::manifest::LoadedExtension;
use dd_host::process::ExtensionProcess;
use dd_protocol::messages::InvokeParams;
use dd_protocol::model::{CommandRef, CommandResult};
use eframe::egui;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;

/// 后台 `invoke` 的结果（进程随结果归还主线程）。
pub(crate) struct InvokeOutcome {
    pub(crate) ext_id: String,
    /// `Some` = 进程对象（成功或链路内错误都归还，由 poll 按 `stub_reheat` 决定取舍）；
    /// `None` = 复热 spawn 本身失败（无进程可归还）。
    pub(crate) proc: Option<ExtensionProcess>,
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
                                eprintln!(
                                    "[dd-gui] invoke 失败：ext={ext_id} 进程已退出（A8 崩溃恢复），丢弃死进程回落 stub：{e}"
                                );
                                self.drop_source_to_stub(&ext_id);
                                self.record_crash(&ext_id);
                            } else {
                                // warm 请求失败但进程还活着：进程归还（超时/错误一般可恢复）
                                self.store_warm_process(ext_id.clone(), p);
                            }
                        }
                        eprintln!("[dd-gui] invoke 失败：{e}");
                        self.show_error_toast(format!("命令执行失败：{e}"));
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
            self.show_toast_kind(
                ToastKind::Error,
                "扩展进程不可用（可能正在处理上一个请求）",
                Some(2_000),
            );
            return;
        }
        if self.is_crash_tripped(ext_id) {
            eprintln!("[dd-gui] invoke 拒绝：ext={ext_id} 暂时不可用（连续崩溃熔断）");
            self.show_toast_kind(
                ToastKind::Error,
                format!("扩展 {ext_id} 暂时不可用，可在设置→扩展管理点击重试"),
                Some(2_500),
            );
            return;
        }
        if self.processes.iter().any(|(id, _)| id == ext_id) {
            self.start_invoke(ext_id, params);
        } else if let Some(ext) = self.find_ext(ext_id).cloned() {
            self.start_invoke_reheat(&ext, params); // 桩复热
        } else {
            eprintln!("[dd-gui] invoke 失败：ext={ext_id} 无扩展信息");
            self.show_error_toast("扩展信息缺失，无法执行");
        }
    }

    /// 后台 `invoke`（warm：take 进程 → 线程调用 → 结果经 channel 归还）。
    pub(crate) fn start_invoke(&mut self, ext_id: &str, params: InvokeParams) {
        self.last_command_id = Some(params.id.clone());
        self.last_invoke = Some(params.clone()); // Confirm 重发沿用
        eprintln!("[dd-gui] invoke 发起：ext={ext_id} cmd={}", params.id);
        let Some(idx) = self.processes.iter().position(|(id, _)| id == ext_id) else {
            eprintln!("[dd-gui] invoke 失败：ext={ext_id} 进程不可用（可能 in-flight）");
            self.show_error_toast("扩展进程不可用（可能正在处理上一个请求）");
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
    pub(crate) fn start_invoke_reheat(&mut self, ext: &LoadedExtension, params: InvokeParams) {
        self.last_command_id = Some(params.id.clone());
        self.last_invoke = Some(params.clone());
        eprintln!(
            "[dd-gui] 桩复热：ext={} cmd={}（spawn→initialize→get_command→invoke）",
            ext.manifest.id, params.id
        );
        self.inflight.insert(ext.manifest.id.clone());
        let ext = ext.clone();
        let ext_id = ext.manifest.id.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut proc = match aggregator::spawn_and_initialize(&ext) {
                Ok(p) => p,
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
            let result: Result<CommandResult, String> =
                match proc.get_command(&params.id).map_err(|e| e.to_string()) {
                    // §6.4：取回真实命令后再执行
                    Ok(Some(_)) => invoke_on(&mut proc, &params),
                    Ok(None) => {
                        Err("命令已失效：扩展未找到该命令（get_command 返回 null）".to_string())
                    }
                    Err(e) => Err(e),
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
    use crate::test_support::{ctx, dying_process, make_app};
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
            .push((ext_id.to_string(), dying_process(ext_id)));

        // 构造「调用期间进程崩溃」的 InvokeOutcome（proc 为另一个死进程）
        let (tx, rx) = mpsc::channel();
        let _ = tx.send(InvokeOutcome {
            ext_id: ext_id.to_string(),
            proc: Some(dying_process(ext_id)),
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
