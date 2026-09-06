//! 嵌套页：`get_items` 发起（warm/复热）+ `poll_page` 结果落地。

use crate::app::pool::get_items_on;
use crate::app::PaletteApp;
use dd_gui::aggregator;
use dd_gui::navigation::PageState;
use dd_gui::state::PanelItem;
use dd_gui::state::PanelState;
use dd_host::manifest::LoadedExtension;
use dd_host::process::ExtensionProcess;
use dd_protocol::messages::GetItemsResult;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;

/// 后台 `get_items` 的结果。
pub(crate) struct PageOutcome {
    pub(crate) ext_id: String,
    /// 同 [`InvokeOutcome::proc`]。
    pub(crate) proc: Option<ExtensionProcess>,
    pub(crate) page_id: String,
    pub(crate) result: Result<GetItemsResult, String>,
    /// 本次是否由**桩复热**发起：失败时不归还进程、回退 stub。
    pub(crate) stub_reheat: bool,
}

impl PaletteApp {
    /// `get_items` 结果：归还/取舍进程（M3 按是否桩复热）→ 更新对应页（页已退栈则作废）。
    pub(crate) fn poll_page(&mut self) {
        let lang = self.lang_effective; // 预捕获：current_mut() 借用期内不能调 self.tr
        let Some(rx) = &self.page_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(outcome) => {
                let PageOutcome {
                    ext_id,
                    mut proc,
                    page_id,
                    result,
                    stub_reheat,
                } = outcome;
                self.inflight.remove(&ext_id);
                self.page_rx = None;

                if self.stack.current().page_id.as_deref() == Some(page_id.as_str()) {
                    match result {
                        Ok(res) => {
                            if let Some(p) = proc {
                                self.store_warm_process(ext_id.clone(), p);
                            }
                            if stub_reheat {
                                self.mark_source_warm(&ext_id);
                                eprintln!("[dd-gui] 桩复热成功：ext={ext_id} 转 warm（LRU 保活）");
                            }
                            let items_raw = res.items;
                            let is_loading = res.is_loading;
                            let items: Vec<PanelItem> = items_raw
                                .iter()
                                .map(|cmd| aggregator::to_panel_item(cmd, &ext_id, "", lang))
                                .collect();
                            eprintln!(
                                "[dd-gui] get_items 成功：page={page_id} items={}",
                                items.len()
                            );
                            let page = self.stack.current_mut();
                            page.is_loading = false;
                            page.empty = if items.is_empty() && !is_loading {
                                Some(crate::text::t(lang, "page.empty").to_string())
                            } else {
                                None
                            };
                            page.is_loading = is_loading;
                            page.list = PanelState::new(items);
                        }
                        Err(e) => {
                            if stub_reheat {
                                // 复热失败：不保活新进程、扩展保持 stub（A6 回退）
                                eprintln!("[dd-gui] 桩复热失败：ext={ext_id}，回退 stub：{e}");
                            } else if let Some(mut p) = proc {
                                if p.has_exited() {
                                    // A8：进程在 get_items 期间崩溃——丢弃死进程，回落 stub
                                    eprintln!(
                                        "[dd-gui] get_items 失败：ext={ext_id} 进程已退出（A8 崩溃恢复），丢弃死进程回落 stub：{e}"
                                    );
                                    self.drop_source_to_stub(&ext_id);
                                    self.record_crash(&ext_id);
                                } else {
                                    self.store_warm_process(ext_id.clone(), p);
                                }
                            }
                            eprintln!("[dd-gui] get_items 失败：page={page_id}：{e}");
                            let page = self.stack.current_mut();
                            page.is_loading = false;
                            page.empty =
                                Some(crate::text::t(lang, "page.fetch_fail").replace("{e}", &e));
                            page.list = PanelState::new(Vec::new());
                        }
                    }
                } else {
                    // 用户已离开来源页：成功（或 warm 失败但进程存活）仍归还进程——
                    // 它是扩展资产；复热失败或进程已死则不保活（A8：回落 stub）。
                    let proc_alive = proc.as_mut().map(|p| !p.has_exited()).unwrap_or(false);
                    if result.is_ok() || (proc_alive && !stub_reheat) {
                        if let Some(p) = proc {
                            self.store_warm_process(ext_id.clone(), p);
                        }
                    } else if proc.is_some() {
                        // 进程已死或复热失败：丢弃（drop 即强杀/清理），回落 stub
                        self.drop_source_to_stub(&ext_id);
                    }
                    eprintln!("[dd-gui] get_items 结果作废：已离开 page={page_id}");
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.page_rx = None,
        }
    }
}

impl PaletteApp {
    /// 进入嵌套页（页面入栈 + loading），并按 warm/桩选择取数路径。
    ///
    /// `command_id` = 被点击的 `Page` 命令 id（桩复热时按协议 §6.4 先 `get_command` 校验）；
    /// `GoToPage` 动作无对应命令点击，传 `None`。
    pub(crate) fn open_page(
        &mut self,
        ext_id: &str,
        page_id: &str,
        search: Option<String>,
        command_id: Option<String>,
    ) {
        self.stack
            .push(PageState::nested(page_id, page_id, ext_id, Vec::new()));
        self.stack.current_mut().is_loading = true;
        self.dispatch_fetch_page(ext_id, page_id, search, command_id);
    }

    /// `get_items` 分派：warm → take 直发；不在 → 桩复热后拉取（A6）。
    /// M4：熔断（连续崩溃，§11）的扩展**不再尝试 spawn**，等重启/手动重试。
    pub(crate) fn dispatch_fetch_page(
        &mut self,
        ext_id: &str,
        page_id: &str,
        search: Option<String>,
        command_id: Option<String>,
    ) {
        let lang = self.lang_effective; // 预捕获：current_mut() 借用期内不能调 self.tr
        if self.page_rx.is_some() || self.inflight.contains(ext_id) {
            eprintln!("[dd-gui] get_items 失败：ext={ext_id} 上一请求仍在处理");
            let page = self.stack.current_mut();
            page.is_loading = false;
            page.empty = Some(crate::text::t(lang, "toast.ext_busy").to_string());
            return;
        }
        if self.is_crash_tripped(ext_id) {
            eprintln!("[dd-gui] get_items 拒绝：ext={ext_id} 暂时不可用（连续崩溃熔断）");
            let page = self.stack.current_mut();
            page.is_loading = false;
            page.empty =
                Some(crate::text::t(lang, "page.ext_unavailable_restart").replace("{id}", ext_id));
            return;
        }
        if self.processes.iter().any(|(id, _)| id == ext_id) {
            self.fetch_page_warm(ext_id, page_id, search);
        } else if let Some(ext) = self.find_ext(ext_id).cloned() {
            self.fetch_page_reheat(&ext, page_id, search, command_id);
        } else {
            let page = self.stack.current_mut();
            page.is_loading = false;
            page.empty = Some(crate::text::t(lang, "page.ext_missing").to_string());
        }
    }

    /// 后台 `get_items`（warm：take 进程 → 线程调用 → 结果经 channel 归还）。
    pub(crate) fn fetch_page_warm(&mut self, ext_id: &str, page_id: &str, search: Option<String>) {
        let lang = self.lang_effective;
        let Some(idx) = self.processes.iter().position(|(id, _)| id == ext_id) else {
            eprintln!("[dd-gui] get_items 失败：ext={ext_id} 进程不可用（可能 in-flight）");
            let page = self.stack.current_mut();
            page.is_loading = false;
            page.empty = Some(crate::text::t(lang, "toast.ext_busy").to_string());
            return;
        };
        let (_, mut proc) = self.processes.remove(idx);
        self.inflight.insert(ext_id.to_string());
        let ext_id = ext_id.to_string();
        let page_id = page_id.to_string();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = get_items_on(&mut proc, &page_id, search);
            let _ = tx.send(PageOutcome {
                ext_id,
                proc: Some(proc),
                page_id,
                result,
                stub_reheat: false,
            });
        });
        self.page_rx = Some(rx);
    }

    /// 桩复热 + `get_items`（A6 / 协议 §6.4）：spawn → initialize →（`get_command` 校验）→ get_items。
    /// 复热失败 → 不保活新进程、扩展保持 stub 并报错。
    pub(crate) fn fetch_page_reheat(
        &mut self,
        ext: &LoadedExtension,
        page_id: &str,
        search: Option<String>,
        command_id: Option<String>,
    ) {
        eprintln!(
            "[dd-gui] 桩复热：ext={} page={page_id}（spawn→initialize→get_command→get_items）",
            ext.manifest.id
        );
        self.inflight.insert(ext.manifest.id.clone());
        let ext = ext.clone();
        let ext_id = ext.manifest.id.clone();
        let page_id = page_id.to_string();
        let (tx, rx) = mpsc::channel();
        let lang = self.lang_effective; // 线程闭包无法借 self：按值捕获生效语言
        thread::spawn(move || {
            let mut proc = match aggregator::spawn_and_initialize(&ext) {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx.send(PageOutcome {
                        ext_id,
                        proc: None,
                        page_id,
                        result: Err(e),
                        stub_reheat: true,
                    });
                    return;
                }
            };
            // 协议 §6.4：被点击的 Page 命令先 `get_command` 校验桩是否仍有效
            let result: Result<GetItemsResult, String> = match &command_id {
                Some(cid) => match proc.get_command(cid).map_err(|e| e.to_string()) {
                    Ok(Some(_)) => get_items_on(&mut proc, &page_id, search),
                    Ok(None) => Err(crate::text::t(lang, "page.cmd_stale").to_string()),
                    Err(e) => Err(e),
                },
                None => get_items_on(&mut proc, &page_id, search),
            };
            let _ = tx.send(PageOutcome {
                ext_id,
                proc: Some(proc),
                page_id,
                result,
                stub_reheat: true,
            });
        });
        self.page_rx = Some(rx);
    }
}
