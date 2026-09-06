//! M4 宿主 fallback 轮：兜底模板拉取链路与渲染（协议 §6.2）。

use crate::app::PaletteApp;
use dd_host::process::ExtensionProcess;
use dd_protocol::model::CommandItem;
use eframe::egui;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;

/// 后台 `fallback_commands` 拉取的结果（M4 宿主 fallback 轮）。
pub(crate) struct FallbackFetchOutcome {
    pub(crate) ext_id: String,
    /// `Some` = warm 进程取走后归还；`None` = 桩复热 spawn 场景（拉完即 close，
    /// 不保活——兜底模板与进程生命周期解耦，见 [`PaletteApp::start_fallback_fetch`]）。
    pub(crate) proc: Option<ExtensionProcess>,
    /// 扩展显示名（渲染 section 兜底用；与模板一起存入 store）。
    pub(crate) name: String,
    pub(crate) result: Result<Vec<CommandItem>, String>,
}

impl PaletteApp {
    // ── M4 宿主 fallback（协议 §6.2：搜索无匹配时展示兜底命令） ──────────

    /// 查询同步点：Root 页且查询非空且**常规无匹配**时，用缓存的兜底模板
    /// 渲染出展示项并注入列表；有匹配/空查询/嵌套页则清空展示（模板缓存保留）。
    /// 每帧由 `draw_panel` 调用（模板渲染开销极小，且 `store` 内做过去重）。
    pub(crate) fn sync_fallback(&mut self) {
        let page = self.stack.current();
        if page.page_id.is_some() || self.aggregating || page.is_loading {
            return; // 嵌套页有自己的过滤（get_items search）；加载中不打扰
        }
        let query = page.list.query().to_string();
        if query.is_empty() || page.list.has_regular_match() {
            self.stack.current_mut().list.clear_fallback();
            return;
        }
        // 无匹配：渲染缓存模板（缓存空 → 空集，列表区回落"没有匹配项"）
        let lang = self.lang_effective;
        let rendered = self.fallback_store.render(&query, lang);
        self.stack.current_mut().list.set_fallback(rendered);
        // 若还有扩展未拉取模板且当前无在途请求 → 链式发起（每轮 1 个）
        self.start_fallback_fetch_chain();
    }

    /// 链式拉取：找第一个「需要模板 且 有 warm 进程」的扩展，take 进程后在
    /// 后台线程调 `fallback_commands`；完成后 [`Self::poll_fallback`] 存模板并
    /// 继续下一个（每次 1 个在途，遵守协议 §4 同进程串行化）。
    pub(crate) fn start_fallback_fetch_chain(&mut self) {
        if self.fallback_rx.is_some() {
            return; // 已有在途
        }
        let target = self
            .processes
            .iter()
            .find(|(id, _)| self.fallback_store.wants(id))
            .map(|(id, _)| id.clone());
        let Some(ext_id) = target else {
            return;
        };
        let name = self
            .exts
            .iter()
            .find(|e| e.manifest.id == ext_id)
            .map(|e| e.manifest.name.clone())
            .unwrap_or_default();
        self.fallback_store.begin_fetch(&ext_id);
        let idx = self
            .processes
            .iter()
            .position(|(id, _)| *id == ext_id)
            .expect("target 来自 processes，必在");
        let (_, mut proc) = self.processes.remove(idx);
        self.inflight.insert(ext_id.clone());
        eprintln!("[dd-gui] 拉取兜底模板：ext={ext_id}");
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = dd_gui::fallback::fetch_fallback_commands(&mut proc);
            let _ = tx.send(FallbackFetchOutcome {
                ext_id,
                proc: Some(proc),
                name,
                result,
            });
        });
        self.fallback_rx = Some(rx);
    }

    /// 后台 `fallback_commands` 结果：存模板（非空 → Ready；空/失败 → Exhausted
    /// 不再重拉）→ 归还/丢弃进程 → 链式拉下一个 → 重渲染当前查询的兜底项。
    pub(crate) fn poll_fallback(&mut self, ctx: &egui::Context) {
        let (outcome, disconnected) = match &self.fallback_rx {
            Some(rx) => match rx.try_recv() {
                Ok(o) => (Some(o), false),
                Err(TryRecvError::Empty) => (None, false),
                Err(TryRecvError::Disconnected) => (None, true),
            },
            None => (None, false),
        };
        if disconnected {
            self.fallback_rx = None;
        }
        let Some(outcome) = outcome else {
            return;
        };
        let FallbackFetchOutcome {
            ext_id,
            proc,
            name,
            result,
        } = outcome;
        self.inflight.remove(&ext_id);
        self.fallback_rx = None;
        match result {
            Ok(templates) => {
                self.fallback_store.store(&ext_id, &name, templates);
                eprintln!(
                    "[dd-gui] 兜底模板就绪：ext={ext_id}（{} 条）",
                    self.fallback_store.template_count(&ext_id)
                );
                ctx.request_repaint(); // 模板到达 → 立即重绘展示兜底项
            }
            Err(e) => {
                eprintln!("[dd-gui] fallback_commands 失败：ext={ext_id}：{e}（本会话不再重试）");
                self.fallback_store.store_failure(&ext_id);
            }
        }
        if let Some(mut p) = proc {
            if p.has_exited() {
                // A8：拉取期间进程崩溃 → 丢弃死进程、回落 stub（不误判为正常）
                self.drop_source_to_stub(&ext_id);
            } else {
                self.store_warm_process(ext_id.clone(), p);
            }
        }
        // 链式拉下一个未取模板的扩展
        self.start_fallback_fetch_chain();
        // 当前查询可能已匹配到新模板 → 重渲染
        self.rerender_fallback();
        ctx.request_repaint();
    }

    /// 无匹配时用 store 最新状态重渲染兜底项（拉取完成后调用）。
    pub(crate) fn rerender_fallback(&mut self) {
        let page = self.stack.current();
        if page.page_id.is_some() || page.is_loading || page.list.has_regular_match() {
            return;
        }
        let query = page.list.query().to_string();
        if query.is_empty() {
            return;
        }
        let lang = self.lang_effective;
        let rendered = self.fallback_store.render(&query, lang);
        self.stack.current_mut().list.set_fallback(rendered);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ctx, dying_process, make_app};
    use dd_gui::state::PanelItem;
    use dd_gui::state::PanelState;
    use dd_protocol::model::CommandRef;

    /// calc 兜底模板（title 含 `{query}`，render 时替换）。
    fn calc_template() -> CommandItem {
        CommandItem {
            id: "calc.eval.query".to_string(),
            title: "= {query}".to_string(),
            subtitle: Some("计算表达式".to_string()),
            icon: None,
            section: Some("计算".to_string()),
            tags: None,
            details: None,
            text_to_suggest: None,
            more_commands: None,
            command: CommandRef::Invoke,
        }
    }

    // ── A10 宿主 fallback 渲染接线（纯逻辑，无进程） ──────────

    #[test]
    fn sync_fallback_injects_rendered_templates_when_no_regular_match() {
        let mut app = make_app();
        app.aggregating = false; // 聚合已完成
        app.stack.root_mut().list = PanelState::new(vec![PanelItem::new("Open Settings")]);
        app.stack.root_mut().list.set_query("zzz-no-match");
        // 预置 calc 兜底模板（已 Ready）
        app.fallback_store
            .store("com.ddrun.calc", "Calculator", vec![calc_template()]);

        app.sync_fallback();

        let page = app.stack.current();
        assert!(page.list.is_fallback_mode(), "无常规匹配时应进入兜底模式");
        assert_eq!(page.list.visible_count(), 1);
        assert_eq!(
            page.list.selected_item().map(|i| i.title.as_str()),
            Some("= zzz-no-match"),
            "模板 {{query}} 应被当前查询替换"
        );
    }

    #[test]
    fn sync_fallback_clears_fallback_when_regular_match() {
        let mut app = make_app();
        app.aggregating = false;
        app.stack.root_mut().list = PanelState::new(vec![PanelItem::new("Open Settings")]);
        app.stack.root_mut().list.set_query("open"); // 命中常规项
        app.fallback_store
            .store("com.ddrun.calc", "Calculator", vec![calc_template()]);

        app.sync_fallback();

        let page = app.stack.current();
        assert!(!page.list.is_fallback_mode(), "常规有匹配时不进入兜底");
        assert_eq!(page.list.visible_count(), 1, "应展示常规匹配项而非兜底");
    }

    #[test]
    fn rerender_fallback_updates_templates_on_query_change() {
        let mut app = make_app();
        app.aggregating = false;
        app.stack.root_mut().list = PanelState::new(vec![PanelItem::new("Open Settings")]);
        app.stack.root_mut().list.set_query("zzz");
        app.fallback_store
            .store("com.ddrun.calc", "Calculator", vec![calc_template()]);
        app.sync_fallback();
        assert!(app.stack.current().list.is_fallback_mode());

        // 查询变化（仍无常规匹配）→ 重渲染应反映新查询
        app.stack.current_mut().list.set_query("abc");
        app.rerender_fallback();
        assert_eq!(
            app.stack
                .current()
                .list
                .selected_item()
                .map(|i| i.title.as_str()),
            Some("= abc")
        );
    }

    // ── A10 拉取链路（死进程 → Exhausted） ──────────────────

    #[test]
    fn fallback_fetch_chain_marks_exhausted_on_dead_process() {
        let mut app = make_app();
        app.aggregating = false;
        let ext_id = "com.ddrun.calc";
        assert!(app.fallback_store.wants(ext_id), "初始应需要拉取模板");

        // 注入一个死进程（start_fallback_fetch_chain 会 take 它去后台拉取）
        app.processes
            .push((ext_id.to_string(), dying_process(ext_id)));

        app.start_fallback_fetch_chain();
        assert!(app.fallback_rx.is_some(), "应已发起后台拉取");

        // 驱动 poll_fallback 直到链结束（死进程 fetch 快速失败）
        let c = ctx();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.fallback_rx.is_some() {
            if std::time::Instant::now() >= deadline {
                break;
            }
            app.poll_fallback(&c);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // 死进程 → fetch 失败 → store_failure → Exhausted（本会话不再拉取）
        assert!(
            !app.fallback_store.wants(ext_id),
            "失败后该扩展应被标记 Exhausted"
        );
        assert!(app.fallback_store.is_empty());
        assert_eq!(app.fallback_store.template_count(ext_id), 0);
    }
}
