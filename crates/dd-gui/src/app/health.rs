//! 崩溃保护：健康巡检、崩溃计数、熔断（协议 §11）。

use crate::app::pool::WARM_IDLE_TTL;
use crate::app::PaletteApp;
use dd_gui::aggregator::SourceStatus;
use dd_gui::robustness::CrashGuard;
use dd_gui::robustness::MAX_CONSECUTIVE_CRASHES;

impl PaletteApp {
    /// 崩溃检测 + 连续崩溃保护（M4/协议 §11，A8）：
    /// 每帧检查保活集，已退出进程按退出码区分——**非 0 退出码 = 崩溃**，
    /// 记录到 [`Self::crash_guards`]（连续 N 次 → 熔断"暂时不可用"）；
    /// **0 退出码 = 正常退出**（如扩展自行 close），仅回落 stub 不计数。
    /// 两种情形都：从保活集移除、LRU 清出、源状态回落（下次点击复热 spawn）。
    pub(crate) fn refresh_health(&mut self) {
        if self.processes.is_empty() {
            return;
        }
        // 诊断摘要必须在**进程被 drop 之前**取出（stderr 随进程对象一起消失）：
        // 退出码 + stderr 末行。2026-09-10 真机反馈驱动——此前这里只打「已退出」，
        // 扩展写在 stderr 的根因（如 `'python' 不是内部或外部命令`）被直接丢掉。
        let exited: Vec<(String, bool, Option<String>)> = self
            .processes
            .iter_mut()
            .filter_map(|(id, p)| {
                // §11：非 0 退出码 = 崩溃；0 = 正常退出
                p.exit_status().map(|st| {
                    let detail = p.failure_detail();
                    (id.clone(), !st.success(), detail)
                })
            })
            .collect();
        for (id, crashed, detail) in exited {
            eprintln!(
                "[dd-gui] 扩展进程已退出：{id}（{}，移除保活，点击命令将重新拉起）{}",
                if crashed {
                    "崩溃/非 0 退出码"
                } else {
                    "正常退出"
                },
                detail
                    .as_deref()
                    .map(|d| format!("；诊断：{d}"))
                    .unwrap_or_default()
            );
            self.drop_source_to_stub(&id);
            if crashed {
                self.record_crash(&id, detail.as_deref());
            }
        }
        // C 批次：warm 空闲超时回收（独立于崩溃巡检——进程仍存活但长期未使用）
        self.warm_idle_reclaim();
    }

    /// C 批次（性能打磨）：warm 进程**空闲超时回收**。
    ///
    /// 背景：`LRU_WARM_CAPACITY`(8) > 扩展总数(6) → LRU **永不触发驱逐**，保活集
    /// 行为上"只增不减"（稳态常驻全部扩展，私有工作集约 13MB）。本方法把空闲超
    /// [`WARM_IDLE_TTL`] 的保活进程按**既有驱逐路径**释放（[`Self::evict_warm`]：
    /// `close` + 回落 stub），下次点击走桩复热（含 L5 修复后的兜底模板直执行）。
    ///
    /// 守卫：① 仅**面板隐藏**时回收——用户正在看列表时不回收，避免"看着就变慢"；
    /// ② 进程已被 take（in-flight 请求在途）的扩展跳过，等结果归还后再判。
    pub(crate) fn warm_idle_reclaim(&mut self) {
        if self.visible || self.processes.is_empty() {
            return;
        }
        let mut evicted = false;
        for id in self.lru.idle_victims(WARM_IDLE_TTL) {
            if !self.processes.iter().any(|(pid, _)| pid == &id) {
                continue; // 在途请求（进程已 take 出保活集）：等归还后再判
            }
            let idle = self
                .lru
                .last_access(&id)
                .map(|t| t.elapsed().as_secs())
                .unwrap_or(0);
            eprintln!(
                "[dd-gui] warm 空闲回收：{id}（空闲 {idle}s ≥ {}s，close+释放，回落 stub）",
                WARM_IDLE_TTL.as_secs()
            );
            self.evict_warm(&id);
            evicted = true;
        }
        // 内存优化 M1（docs/memory-optimization-plan.md §3）：驱逐后同步修剪
        // 工作集——本 tick 由 request_repaint_after 驱动，无额外唤醒源。
        if evicted {
            crate::platform::trim_working_set();
        }
    }

    /// M4/§11：记录一次崩溃，连续 [`MAX_CONSECUTIVE_CRASHES`] 次 → 熔断（暂时不可用）。
    ///
    /// `detail` = 该次崩溃的**诊断摘要**（[`dd_host::process::ExtensionProcess::failure_detail`]：
    /// 退出码 + stderr 末行）。熔断时并入 Failed 原因与日志——设置页扩展卡片会直接
    /// 展示它，用户无需再去翻日志（2026-09-10 真机反馈）。
    pub(crate) fn record_crash(&mut self, ext_id: &str, detail: Option<&str>) {
        let lang = self.lang_effective; // 预捕获：sources.iter_mut() 借用期内不能调 self.tr
        let guard = self
            .crash_guards
            .entry(ext_id.to_string())
            .or_insert_with(|| CrashGuard::new(ext_id));
        if guard.is_tripped() {
            return;
        }
        let just_tripped = guard.record_crash();
        let n = guard.consecutive();
        if just_tripped {
            let mut error = crate::text::t(lang, "toast.ext_unavailable_crash")
                .replace("{id}", ext_id)
                .replace("{n}", &n.to_string());
            if let Some(d) = detail {
                error.push_str(&format!("（诊断：{d}）"));
            }
            eprintln!(
                "[dd-gui] 扩展 {ext_id} 连续崩溃 {n} 次 ≥ {MAX_CONSECUTIVE_CRASHES}，标记暂时不可用（设置→扩展管理可手动重试）；诊断：{}",
                detail.unwrap_or("无")
            );
            self.show_error_toast(
                self.tr("toast.ext_unavailable_crash")
                    .replace("{id}", ext_id)
                    .replace("{n}", &n.to_string()),
            );
            if let Some(s) = self.sources.iter_mut().find(|s| s.id == ext_id) {
                s.status = SourceStatus::Failed { error };
            }
        } else {
            eprintln!(
                "[dd-gui] 扩展 {ext_id} 连续崩溃 {n}/{MAX_CONSECUTIVE_CRASHES} 次；诊断：{}",
                detail.unwrap_or("无")
            );
        }
    }

    /// M4/§11：扩展成功恢复（warm/复热成功）→ 清零连续崩溃计数、解除熔断。
    pub(crate) fn reset_crash(&mut self, ext_id: &str) {
        if let Some(g) = self.crash_guards.get_mut(ext_id) {
            if g.is_tripped() || g.consecutive() > 0 {
                eprintln!("[dd-gui] 扩展 {ext_id} 恢复成功，清零连续崩溃计数");
            }
            g.reset();
        }
    }

    /// M4/§11：该扩展是否处于熔断（暂时不可用）态。
    pub(crate) fn is_crash_tripped(&self, ext_id: &str) -> bool {
        self.crash_guards
            .get(ext_id)
            .map(|g| g.is_tripped())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{dying_client, make_app};
    use dd_gui::aggregator::SourceSummary;

    // ── A8 崩溃恢复接线（进程退出 → 回落 stub + 记崩溃 + 熔断） ──

    #[test]
    fn refresh_health_drops_to_stub_and_records_crash() {
        let mut app = make_app();
        let ext_id = "com.example.dying";
        app.sources.push(SourceSummary {
            id: ext_id.to_string(),
            name: "Dying".to_string(),
            status: SourceStatus::Warm { commands: 5 },
        });
        app.processes
            .push((ext_id.to_string(), dying_client(ext_id)));

        app.refresh_health();

        assert!(
            app.processes.iter().all(|(id, _)| id != ext_id),
            "崩溃进程应从保活集移除（回落 stub）"
        );
        assert!(
            app.crash_guards
                .get(ext_id)
                .map(|g| g.consecutive())
                .unwrap_or(0)
                >= 1,
            "应记录一次崩溃"
        );
        let s = app.sources.iter().find(|s| s.id == ext_id).unwrap();
        assert!(s.status.is_stub(), "源状态应回落 Stub，实际 {:?}", s.status);
    }

    #[test]
    fn consecutive_crashes_trip_circuit_breaker() {
        let mut app = make_app();
        let ext_id = "com.example.dying";
        app.sources.push(SourceSummary {
            id: ext_id.to_string(),
            name: "Dying".to_string(),
            status: SourceStatus::Warm { commands: 3 },
        });

        // 前 N-1 次不熔断
        for _ in 0..(MAX_CONSECUTIVE_CRASHES - 1) {
            app.processes
                .push((ext_id.to_string(), dying_client(ext_id)));
            app.refresh_health();
            assert!(!app.is_crash_tripped(ext_id), "未达阈值不应熔断");
        }
        // 第 N 次触发熔断
        app.processes
            .push((ext_id.to_string(), dying_client(ext_id)));
        app.refresh_health();
        assert!(
            app.is_crash_tripped(ext_id),
            "连续 N 次崩溃应熔断（暂时不可用）"
        );
        let s = app.sources.iter().find(|s| s.id == ext_id).unwrap();
        assert!(s.status.is_failed(), "熔断后源状态应为 Failed");
    }

    /// 崩溃诊断（2026-09-10 增补）：熔断时把**诊断摘要**并入 Failed 原因，
    /// 设置页扩展卡片据此直接展示根因（用户无需再翻日志）。
    #[test]
    fn crashed_failed_reason_carries_diagnostics() {
        let mut app = make_app();
        let ext_id = "com.example.dying";
        app.sources.push(SourceSummary {
            id: ext_id.to_string(),
            name: "Dying".to_string(),
            status: SourceStatus::Warm { commands: 1 },
        });

        for _ in 0..MAX_CONSECUTIVE_CRASHES {
            app.record_crash(ext_id, Some("退出码 1；stderr: boom"));
        }

        let s = app.sources.iter().find(|s| s.id == ext_id).unwrap();
        match &s.status {
            SourceStatus::Failed { error } => {
                assert!(error.contains("诊断"), "应含诊断段：{error}");
                assert!(error.contains("stderr: boom"), "应含 stderr 末行：{error}");
            }
            other => panic!("应熔断为 Failed，实际 {other:?}"),
        }
    }

    /// L2（M6.4）：熔断后 `reset_crash` 解除熔断态，使扩展管理页「重试」按钮
    /// 可恢复该扩展（解除后重聚合会重新 spawn，成功则 Warm、失败则再次熔断）。
    #[test]
    fn reset_crash_clears_trip() {
        let mut app = make_app();
        let ext_id = "com.example.dying";
        let mut g = CrashGuard::new(ext_id);
        for _ in 0..MAX_CONSECUTIVE_CRASHES {
            g.record_crash();
        }
        app.crash_guards.insert(ext_id.to_string(), g);
        assert!(app.is_crash_tripped(ext_id), "应先处于熔断态");

        app.reset_crash(ext_id);
        assert!(!app.is_crash_tripped(ext_id), "reset_crash 应解除熔断");
        assert_eq!(
            app.crash_guards
                .get(ext_id)
                .map(|g| g.consecutive())
                .unwrap_or(0),
            0,
            "reset_crash 应清零连续崩溃计数"
        );
    }

    // ── C 批次：warm 空闲超时回收 ──

    /// 隐藏态下仅回收「空闲 ≥ `WARM_IDLE_TTL`」的保活进程（复用 evict_warm 同路径：
    /// 保活集 + LRU 移除、源回落 stub）；刚触达者与在途（进程已 take）不受影响。
    #[test]
    fn warm_idle_reclaim_evicts_only_idle_when_hidden() {
        let mut app = make_app();
        let (idle, fresh, inflight) = (
            "com.example.idle",
            "com.example.fresh",
            "com.example.inflight",
        );
        for id in [idle, fresh, inflight] {
            app.sources.push(SourceSummary {
                id: id.to_string(),
                name: id.to_string(),
                status: SourceStatus::Warm { commands: 2 },
            });
            app.processes.push((id.to_string(), dying_client(id)));
        }
        let now = std::time::Instant::now();
        let stale = now - WARM_IDLE_TTL - std::time::Duration::from_secs(5);
        app.lru.access_at(idle, stale);
        app.lru.access_at(fresh, now);
        app.lru.access_at(inflight, stale);
        // 模拟 in-flight 请求：进程已 take 出保活集（结果未归还）
        app.processes.retain(|(pid, _)| pid != inflight);

        app.warm_idle_reclaim();

        assert!(
            app.processes.iter().all(|(id, _)| id != idle),
            "空闲超阈值者应被回收（移出保活集）"
        );
        assert!(
            app.processes.iter().any(|(id, _)| id == fresh),
            "刚触达者不应被回收"
        );
        assert!(!app.lru.contains(idle), "被回收者应从 LRU 集移除");
        assert!(app.lru.contains(fresh), "未回收者仍在 LRU 集内");
        assert!(
            app.lru.contains(inflight),
            "在途扩展（无进程）应跳过回收，仍留在 LRU 集"
        );
        let s = app.sources.iter().find(|s| s.id == idle).unwrap();
        assert!(s.status.is_stub(), "被回收者源状态应回落 Stub");
        let s = app.sources.iter().find(|s| s.id == fresh).unwrap();
        assert!(!s.status.is_stub(), "未回收者源状态保持 Warm");
    }

    /// 面板**可见**时不回收（用户正在看列表，避免"看着就变慢"）。
    #[test]
    fn warm_idle_reclaim_skipped_when_panel_visible() {
        let mut app = make_app();
        let id = "com.example.idle";
        app.sources.push(SourceSummary {
            id: id.to_string(),
            name: id.to_string(),
            status: SourceStatus::Warm { commands: 1 },
        });
        app.processes.push((id.to_string(), dying_client(id)));
        app.lru.access_at(
            id,
            std::time::Instant::now() - WARM_IDLE_TTL - std::time::Duration::from_secs(5),
        );
        app.visible = true;

        app.warm_idle_reclaim();

        assert!(
            app.processes.iter().any(|(pid, _)| pid == id),
            "可见时不应回收保活进程"
        );
        assert!(app.lru.contains(id), "可见时 LRU 记录不变");
    }
}
