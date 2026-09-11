//! 嵌套页：`get_items` 发起（warm/复热）+ `poll_page` 结果落地。

use crate::app::pool::get_items_on;
use crate::app::PaletteApp;
use crate::ext_client::ExtClient;
use dd_gui::aggregator;
use dd_gui::navigation::PageState;
use dd_gui::state::PanelItem;
use dd_gui::state::PanelState;
use dd_host::manifest::LoadedExtension;
use dd_protocol::messages::GetItemsResult;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;

/// 后台 `get_items` 的结果。
pub(crate) struct PageOutcome {
    pub(crate) ext_id: String,
    /// 同 `InvokeOutcome::proc`（客户端随结果归还主线程）。
    pub(crate) proc: Option<ExtClient>,
    pub(crate) page_id: String,
    /// 发起本次 `get_items` 时携带的 search 文本：落地时与页内当前 query 比对，
    /// 不一致说明「请求期间用户又输入了」→ 本次结果已过期，重新武装去抖补拉。
    pub(crate) search: Option<String>,
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
                    search: req_search,
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
                            // 文件搜索自动进页：落地时回填搜索框查询文本（否则被清空）
                            let drill_armed = self.file_drill_armed;
                            let drill_q = self.file_drill.clone();
                            let page = self.stack.current_mut();
                            page.is_loading = false;
                            page.empty = if items.is_empty() && !is_loading {
                                Some(crate::text::t(lang, "page.empty").to_string())
                            } else {
                                None
                            };
                            page.is_loading = is_loading;
                            // v3.3：落地**保留**页内 query（旧实现整表重建会把搜索框清空，
                            // 用户在 loading 期间打的字全部丢失）；items 重建但 query 保留。
                            let prev_query = page.list.query().to_owned();
                            page.list = PanelState::new(items);
                            page.list.set_passthrough(); // 嵌套页：扩展已过滤/排序，宿主不再二次过滤
                            if !prev_query.is_empty() {
                                page.list.set_query(prev_query.clone());
                            }
                            // 备用：普通嵌套页（非文件搜索 drill）落地后 query 已由
                            // draw_searchbar 写回列表（panel.rs），这里无需额外处理。
                            if drill_armed {
                                self.file_drill_armed = false;
                                if page_id == crate::app::FILE_SEARCH_PAGE_ID {
                                    if let Some(root_q) = &drill_q {
                                        if let Some(rest) =
                                            root_q.strip_prefix(crate::app::FILE_SEARCH_PREFIX)
                                        {
                                            let q = rest.trim_start().to_string();
                                            if !q.is_empty() {
                                                page.list.set_query(q);
                                            }
                                        }
                                    }
                                }
                            }
                            // v3.3 过期补偿：请求期间用户又输入了 → 本次结果是旧查询的，重新武装去抖
                            //（200ms 静默后按最新 query 补拉）。仅在页内**已有输入**且与请求
                            // search 不一致时触发——页内框为空属合法初始态（open_page 会把
                            // Root 查询作为 search 传入而页内框初始为空，如点「在文件中搜索」），
                            // 不补拉以保留扩展的初始过滤结果。set_query 幂等 + 去抖刷新式调度，
                            // 不构成循环；直到「query 稳定 ∧ 结果与 query 对应」才收敛。
                            let cur_query = self.stack.current().list.query().to_owned();
                            if !cur_query.is_empty() && Some(cur_query) != req_search {
                                self.schedule_page_query_debounce();
                            }
                        }
                        Err(e) => {
                            if stub_reheat {
                                // 复热失败：不保活新进程、扩展保持 stub（A6 回退）
                                eprintln!("[dd-gui] 桩复热失败：ext={ext_id}，回退 stub：{e}");
                            } else if let Some(mut p) = proc {
                                if p.has_exited() {
                                    // A8：进程在 get_items 期间崩溃——丢弃死进程，回落 stub
                                    let detail = p.failure_detail();
                                    eprintln!(
                                        "[dd-gui] get_items 失败：ext={ext_id} 进程已退出（A8 崩溃恢复），丢弃死进程回落 stub：{e}{}",
                                        detail
                                            .as_deref()
                                            .map(|d| format!("；诊断：{d}"))
                                            .unwrap_or_default()
                                    );
                                    self.drop_source_to_stub(&ext_id);
                                    self.record_crash(&ext_id, detail.as_deref());
                                } else {
                                    self.store_warm_process(ext_id.clone(), p);
                                }
                            }
                            eprintln!("[dd-gui] get_items 失败：page={page_id}：{e}");
                            let page = self.stack.current_mut();
                            // v3.3：失败落地同样保留页内 query（不吞掉 loading 期间输入）
                            let prev_query = page.list.query().to_owned();
                            page.is_loading = false;
                            page.empty =
                                Some(crate::text::t(lang, "page.fetch_fail").replace("{e}", &e));
                            page.list = PanelState::new(Vec::new());
                            page.list.set_passthrough();
                            if !prev_query.is_empty() {
                                page.list.set_query(prev_query);
                            }
                        }
                    }
                } else {
                    // 用户已离开来源页：清除文件搜索回填标记，避免后续落地误回填旧查询
                    self.file_drill_armed = false;
                    // 成功（或 warm 失败但进程存活）仍归还进程——
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
        // 进嵌套页即聚焦搜索框（截图反馈：进入文件搜索后输入框无光标 → 无法直接键入）
        self.want_focus = true;
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
            // L5：兜底模板 id 不在顶层命令注册表内，§6.4 校验必失败 → 等价
            // GoToPage 的「无对应命令点击」语义（传 None 即跳过校验）。
            let command_id =
                command_id.filter(|cid| !self.fallback_store.contains_template(ext_id, cid));
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
        let search_req = search.clone(); // 供落地时比对（v3.3 过期补偿）
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = get_items_on(&mut proc, &page_id, search);
            let _ = tx.send(PageOutcome {
                ext_id,
                proc: Some(proc),
                page_id,
                search: search_req,
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
        // M9：内置扩展按 in-process 重建（spec 直调），其余 spawn 子进程。
        let spec = self.inproc_specs.get(&ext_id).cloned();
        let (tx, rx) = mpsc::channel();
        let lang = self.lang_effective; // 线程闭包无法借 self：按值捕获生效语言
        thread::spawn(move || {
            let mut proc = match crate::ext_client::open(spec, Some(&ext)) {
                Ok((p, _init)) => p,
                Err(e) => {
                    let _ = tx.send(PageOutcome {
                        ext_id,
                        proc: None,
                        page_id,
                        search,
                        result: Err(e),
                        stub_reheat: true,
                    });
                    return;
                }
            };
            // 协议 §6.4：被点击的 Page 命令先 `get_command` 校验桩是否仍有效
            let result: Result<GetItemsResult, String> = match &command_id {
                Some(cid) => match proc.get_command(cid).map_err(|e| e.to_string()) {
                    Ok(Some(_)) => get_items_on(&mut proc, &page_id, search.clone()),
                    Ok(None) => Err(crate::text::t(lang, "page.cmd_stale").to_string()),
                    Err(e) => Err(e),
                },
                None => get_items_on(&mut proc, &page_id, search.clone()),
            };
            let _ = tx.send(PageOutcome {
                ext_id,
                proc: Some(proc),
                page_id,
                search,
                result,
                stub_reheat: true,
            });
        });
        self.page_rx = Some(rx);
    }
}
