//! 嵌套页：`get_items` 发起（warm/复热）+ `poll_page` 结果落地。

use crate::app::e2e::E2eSample;
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
use std::time::Instant;

/// 纯决策：`get_items` 落地结果是否**已过期**（不具权威性）。
///
/// 判据 = 本次请求携带的查询文本（`req_search`）与页内**当前** query 不一致，
/// 说明请求在飞期间用户又输入了。
///
/// - 页内框为空属合法初始态（`GoToPage` 等无查询进页路径）→ **不过期**，
///   照常落地以保留扩展的初始过滤结果（与 v3.3 既有语义一致）；
/// - 其余不一致 = 过期：旧查询（尤其 0 命中）结果不得落地，否则会把
///   「该页暂无内容」画上屏（真机 2026-09-14：文件搜索输入后 ~1s 误显空态）。
///
/// 抽成纯函数便于单测（无宿主状态）。
pub(crate) fn landing_is_stale(current_query: &str, req_search: Option<&str>) -> bool {
    !current_query.is_empty() && Some(current_query) != req_search
}

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
                            let e2e_landed_at = Instant::now(); // E2E：落地时刻（尽早捕获）
                            if let Some(p) = proc {
                                self.store_warm_process(ext_id.clone(), p);
                            }
                            if stub_reheat {
                                self.mark_source_warm(&ext_id);
                                log::debug!(
                                    "[dd-gui] 桩复热成功：ext={ext_id} 转 warm（LRU 保活）"
                                );
                            }
                            let items_raw = res.items;
                            let is_loading = res.is_loading;
                            let items: Vec<PanelItem> = items_raw
                                .iter()
                                .map(|cmd| aggregator::to_panel_item(cmd, &ext_id, "", lang))
                                .collect();
                            log::debug!(
                                "[dd-gui] get_items 成功：page={page_id} items={}",
                                items.len()
                            );
                            // 过期判定（真机 2026-09-14：文件搜索输入后 ~1s 误显
                            // 「该页暂无内容」）：落地结果对应的查询与页内当前 query
                            // 不一致 → 旧查询结果，不具权威性。旧查询 0 命中若照常
                            // 落地会把空态画上屏；此处丢弃、保持 in-flight 语义
                            // （列表空 → 骨架 / 非空 → 保留旧结果），并重武装去抖
                            // 按最新 query 补拉。非过期结果走下方正常落地。
                            let landing_query = self.stack.current().list.query().to_owned();
                            if landing_is_stale(&landing_query, req_search.as_deref()) {
                                log::debug!(
                                    "[dd-gui] get_items 结果过期：req={req_search:?} cur={landing_query:?} → 不落地 + 重武装补拉"
                                );
                                self.stack.current_mut().begin_refetch(Instant::now());
                                self.rearm_page_query_debounce();
                            } else {
                                // E2E：非过期落地 → 先取计时起点（self 借用须早于 page 借用）
                                let e2e_times =
                                    self.e2e_input_at.take().zip(self.e2e_dispatch_at.take());
                                let e2e_items = items.len(); // E2E：快照（下方 PanelState::new 会移走 items）
                                let page = self.stack.current_mut();
                                page.clear_refetch(); // 落地：撤销延迟骨架标记
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
                                // E2E：建样本（page 借用至此已结束）——本帧 draw_panel
                                // 完成时由 `e2e_report` 结算输出。
                                if let Some((input_at, dispatch_at)) = e2e_times {
                                    self.e2e_pending = Some(E2eSample {
                                        page_id: page_id.clone(),
                                        query_chars: prev_query.chars().count(),
                                        items: e2e_items,
                                        input_at,
                                        dispatch_at,
                                        landed_at: e2e_landed_at,
                                    });
                                }
                                // 嵌套页落地后 query 已由 draw_searchbar 写回列表
                                // （panel.rs），这里无需额外处理。
                                // 过期补偿已前移至落地前的 `landing_is_stale` 分支；
                                // 此处为非过期结果，正常落地（query 保留 + drill 回填见上）。
                            }
                        }
                        Err(e) => {
                            if stub_reheat {
                                // 复热失败：不保活新进程、扩展保持 stub（A6 回退）
                                log::warn!("[dd-gui] 桩复热失败：ext={ext_id}，回退 stub：{e}");
                            } else if let Some(mut p) = proc {
                                if p.has_exited() {
                                    // A8：进程在 get_items 期间崩溃——丢弃死进程，回落 stub
                                    let detail = p.failure_detail();
                                    log::debug!(
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
                            log::warn!("[dd-gui] get_items 失败：page={page_id}：{e}");
                            self.e2e_dispatch_at = None; // E2E：失败不计样本，input 保留供重拉续测
                            let page = self.stack.current_mut();
                            page.clear_refetch(); // 落地（失败）：撤销延迟骨架标记
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
                            // 失败落地同款过期补偿：请求期间用户又输入了 →
                            // 重武装去抖按最新 query 补拉（与 Ok 分支对称；
                            // 否则补拉期间输入的字符会被失败态吞掉不再触发）。
                            let cur_query = self.stack.current().list.query().to_owned();
                            if !cur_query.is_empty() && Some(cur_query) != req_search {
                                self.schedule_page_query_debounce();
                            }
                        }
                    }
                } else {
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
                    log::debug!("[dd-gui] get_items 结果作废：已离开 page={page_id}");
                    self.e2e_input_at = None; // E2E：已离页，测量作废
                    self.e2e_dispatch_at = None;
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.page_rx = None,
        }
    }
}

impl PaletteApp {
    /// 返回上一级（页面栈出栈）并聚焦回落后页面的搜索框（真机 2026-09-12：
    /// 从文件搜索页返回后输入框失焦，无法直接键入——进页时 `open_page` 置位
    /// 的 `want_focus` 只在落地时消费一次，返回路径需重新置位）。
    /// 返回 `None` = 已在 Root（调用方决定隐藏，§4.3），不置位。
    pub(crate) fn go_back_focused(&mut self) -> Option<dd_gui::navigation::PageState> {
        let popped = self.stack.go_back();
        if popped.is_some() {
            self.want_focus = true;
        }
        popped
    }

    /// 进入嵌套页（页面入栈 + loading），并按 warm/桩选择取数路径。
    ///
    /// `command_id` = 被点击的 `Page` 命令 id（桩复热时按协议 §6.4 先 `get_command` 校验）；
    /// `GoToPage` 动作无对应命令点击，传 `None`。
    ///
    /// `title` = **页标题的来源**，进搜索框 placeholder（设计稿 §07.1 D2）：
    /// - 点击入口项进页 → 传**被点击项标题**（人类可读）；
    /// - 无入口项的场景（如 `Ctrl+F` 直达）→ 传宿主本地化文案；
    /// - 扩展 `GoToPage` 效果 → `None` → placeholder 回落「筛选命令…」。
    ///
    /// ⚠️ 此前这里直接把 `page_id` 当标题，导致 placeholder 显示「在「files.results」中筛选…」
    /// （原始 id 泄漏给用户）；协议侧 `PageInfo` 定为**不传递**（`protocol.md` §8.5），
    /// 故标题一律由宿主侧信息提供。
    pub(crate) fn open_page(
        &mut self,
        ext_id: &str,
        page_id: &str,
        search: Option<String>,
        command_id: Option<String>,
        title: Option<String>,
    ) {
        let title = title.unwrap_or_default();
        self.stack
            .push(PageState::nested(page_id, title, ext_id, Vec::new()));
        self.stack.current_mut().is_loading = true;
        // 进嵌套页即聚焦搜索框（截图反馈：进入文件搜索后输入框无光标 → 无法直接键入）
        self.want_focus = true;
        // 根页查询回填页内搜索框（真机 2026-09-12：输入「测试」后点「文件搜索」，
        // 落地页输入框为空——根页查询虽已作为 search 传给 get_items，但页内框
        // 不见 query，用户须重打一遍）。与 `Ctrl+F` 直达进页的回填语义一致；
        // 落地后 poll_page 的 v3.3 保留逻辑沿用此值（cur_query == req_search
        // → 不触发过期补偿重拉）。
        if let Some(q) = search.as_deref() {
            if !q.is_empty() {
                self.stack.current_mut().list.set_query(q.to_string());
                self.e2e_input_at = Some(Instant::now()); // E2E：带词进页（Ctrl+F / 入口项）= 感知起点
            }
        }
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
                                        // 补拉状态预备（真机 2026-09-14 两轮反馈）：凡发起 get_items 先做预备
                                        // ——有旧结果 → 保留展示（stale-while-revalidate）；列表为空 → **延迟**切
                                        // 骨架（快速补拉不闪骨架，慢补拉才显示）；不清空态文案（交由落地统一替换）。
        self.stack.current_mut().begin_refetch(Instant::now());
        if self.page_rx.is_some() || self.inflight.contains(ext_id) {
            log::warn!("[dd-gui] get_items 失败：ext={ext_id} 上一请求仍在处理");
            let page = self.stack.current_mut();
            page.clear_refetch(); // 未真正发起 → 撤销延迟骨架（否则 250ms 后误显示）
            page.is_loading = false;
            page.empty = Some(crate::text::t(lang, "toast.ext_busy").to_string());
            return;
        }
        if self.is_crash_tripped(ext_id) {
            log::warn!("[dd-gui] get_items 拒绝：ext={ext_id} 暂时不可用（连续崩溃熔断）");
            let page = self.stack.current_mut();
            page.clear_refetch(); // 未真正发起 → 撤销延迟骨架
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
            page.clear_refetch(); // 未真正发起 → 撤销延迟骨架
            page.is_loading = false;
            page.empty = Some(crate::text::t(lang, "page.ext_missing").to_string());
        }
    }

    /// 后台 `get_items`（warm：take 进程 → 线程调用 → 结果经 channel 归还）。
    pub(crate) fn fetch_page_warm(&mut self, ext_id: &str, page_id: &str, search: Option<String>) {
        let lang = self.lang_effective;
        let Some(idx) = self.processes.iter().position(|(id, _)| id == ext_id) else {
            log::warn!("[dd-gui] get_items 失败：ext={ext_id} 进程不可用（可能 in-flight）");
            let page = self.stack.current_mut();
            page.clear_refetch(); // 未真正发起 → 撤销延迟骨架
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
        log::debug!(
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

#[cfg(test)]
mod tests {
    use super::landing_is_stale;

    /// 请求期间用户又输入（当前 query ≠ 请求查询）→ 过期，不得落地。
    /// 真机 2026-09-14：文件搜索输入后 ~1s 误显「该页暂无内容」即此路径。
    #[test]
    fn stale_when_current_query_differs_from_request() {
        assert!(landing_is_stale("报告", Some("报")));
        assert!(landing_is_stale("abc", Some("ab")));
    }

    /// 请求查询与页内当前一致 → 非过期，正常落地。
    #[test]
    fn not_stale_when_queries_match() {
        assert!(!landing_is_stale("报告", Some("报告")));
    }

    /// 页内框为空属合法初始态（GoToPage 无查询进页）→ 非过期，
    /// 保留扩展初始过滤结果（既有语义不回退）。
    #[test]
    fn not_stale_when_current_query_empty() {
        assert!(!landing_is_stale("", Some("anything")));
        assert!(!landing_is_stale("", None));
    }

    /// 请求无查询（search=None）但页内已有输入 → 过期（用户进页后立即输入）。
    #[test]
    fn stale_when_request_had_no_search_but_page_has_query() {
        assert!(landing_is_stale("报告", None));
    }
}
