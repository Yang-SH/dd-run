//! 崩溃保护：健康巡检、崩溃计数、熔断（协议 §11）。

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
        let exited: Vec<(String, bool)> = self
            .processes
            .iter_mut()
            .filter_map(|(id, p)| {
                // §11：非 0 退出码 = 崩溃；0 = 正常退出
                p.exit_status().map(|st| (id.clone(), !st.success()))
            })
            .collect();
        if exited.is_empty() {
            return;
        }
        for (id, crashed) in exited {
            eprintln!(
                "[dd-gui] 扩展进程已退出：{id}（{}，移除保活，点击命令将重新拉起）",
                if crashed {
                    "崩溃/非 0 退出码"
                } else {
                    "正常退出"
                }
            );
            self.drop_source_to_stub(&id);
            if crashed {
                self.record_crash(&id);
            }
        }
    }

    /// M4/§11：记录一次崩溃，连续 [`MAX_CONSECUTIVE_CRASHES`] 次 → 熔断（暂时不可用）。
    pub(crate) fn record_crash(&mut self, ext_id: &str) {
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
            eprintln!(
                "[dd-gui] 扩展 {ext_id} 连续崩溃 {n} 次 ≥ {MAX_CONSECUTIVE_CRASHES}，标记暂时不可用（设置→扩展管理可手动重试）"
            );
            self.show_error_toast(format!(
                "扩展 {ext_id} 暂时不可用（连续崩溃 {n} 次），可在设置→扩展管理重试"
            ));
            if let Some(s) = self.sources.iter_mut().find(|s| s.id == ext_id) {
                s.status = SourceStatus::Failed {
                    error: format!("暂时不可用（连续崩溃 {n} 次），可在设置→扩展管理重试"),
                };
            }
        } else {
            eprintln!("[dd-gui] 扩展 {ext_id} 连续崩溃 {n}/{MAX_CONSECUTIVE_CRASHES} 次");
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
    use crate::test_support::{dying_process, make_app};
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
            .push((ext_id.to_string(), dying_process(ext_id)));

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
                .push((ext_id.to_string(), dying_process(ext_id)));
            app.refresh_health();
            assert!(!app.is_crash_tripped(ext_id), "未达阈值不应熔断");
        }
        // 第 N 次触发熔断
        app.processes
            .push((ext_id.to_string(), dying_process(ext_id)));
        app.refresh_health();
        assert!(
            app.is_crash_tripped(ext_id),
            "连续 N 次崩溃应熔断（暂时不可用）"
        );
        let s = app.sources.iter().find(|s| s.id == ext_id).unwrap();
        assert!(s.status.is_failed(), "熔断后源状态应为 Failed");
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
}
