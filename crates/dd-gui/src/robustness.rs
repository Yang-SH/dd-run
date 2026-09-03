//! M4 健壮性纯逻辑：扩展崩溃保护状态机（协议 §11，验收 A8）。
//!
//! 宿主侧崩溃恢复契约（[`docs/protocol.md`](../../docs/protocol.md) §11）：
//! 1. stdout EOF / 非 0 退出码 → 判定崩溃；
//! 2. in-flight 请求立即失败，UI 不得卡住；
//! 3. frozen 且有磁盘桩 → 命令回退 stub 保留在列表；
//! 4. 宿主继续运行（绝不退出）；
//! 5. **连续崩溃 N 次（默认 3）→ 标记"暂时不可用"，宿主重启或手动重试后才恢复**。
//!
//! 本模块只管理「计数 → 熔断 → 恢复」这一状态机（不接触进程/UI），
//! 便于纯逻辑单测；实际崩溃检测与 UI 反馈在 `main`（bin）接线。

/// 熔断阈值：连续崩溃 N 次后标记"暂时不可用"（协议 §11 建议 N=3，可配置）。
pub const MAX_CONSECUTIVE_CRASHES: u32 = 3;

/// 单个扩展的崩溃保护状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashGuard {
    /// 扩展清单 id。
    id: String,
    /// 当前连续崩溃计数（未熔断时递增；成功恢复后清零）。
    consecutive: u32,
    /// 是否已熔断（连续崩溃达到 [`MAX_CONSECUTIVE_CRASHES`]）。
    tripped: bool,
}

impl CrashGuard {
    /// 新建（初始未熔断、计数 0）。
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            consecutive: 0,
            tripped: false,
        }
    }

    /// 扩展 id。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 是否处于熔断（"暂时不可用"）态。
    pub fn is_tripped(&self) -> bool {
        self.tripped
    }

    /// 当前连续崩溃次数。
    pub fn consecutive(&self) -> u32 {
        self.consecutive
    }

    /// 记录一次崩溃。返回本次是否**新触发**熔断（调用方据此弹提示/更新 UI）。
    ///
    /// 已熔断后再调用 `record_crash` 不会改变状态（保持熔断，等待恢复动作）。
    pub fn record_crash(&mut self) -> bool {
        if self.tripped {
            return false;
        }
        self.consecutive += 1;
        if self.consecutive >= MAX_CONSECUTIVE_CRASHES {
            self.tripped = true;
            true
        } else {
            false
        }
    }

    /// 扩展成功恢复（warm / 手动重试成功）→ 清零计数、解除熔断。
    pub fn reset(&mut self) {
        self.consecutive = 0;
        self.tripped = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_clear() {
        let g = CrashGuard::new("ext.a");
        assert_eq!(g.id(), "ext.a");
        assert!(!g.is_tripped());
        assert_eq!(g.consecutive(), 0);
    }

    #[test]
    fn trips_after_n_consecutive_crashes() {
        let mut g = CrashGuard::new("ext.a");
        // 前 N-1 次不熔断
        for _ in 0..(MAX_CONSECUTIVE_CRASHES - 1) {
            assert!(!g.record_crash());
            assert!(!g.is_tripped());
        }
        // 第 N 次触发熔断，且 record_crash 返回 true 表示"新触发"
        assert!(g.record_crash());
        assert!(g.is_tripped());
        assert_eq!(g.consecutive(), MAX_CONSECUTIVE_CRASHES);
    }

    #[test]
    fn record_after_trip_is_noop() {
        let mut g = CrashGuard::new("ext.a");
        for _ in 0..MAX_CONSECUTIVE_CRASHES {
            g.record_crash();
        }
        assert!(g.is_tripped());
        // 熔断后再崩溃：计数不再涨、状态不变、返回 false
        assert!(!g.record_crash());
        assert_eq!(g.consecutive(), MAX_CONSECUTIVE_CRASHES);
        assert!(g.is_tripped());
    }

    #[test]
    fn reset_clears_count_and_trip() {
        let mut g = CrashGuard::new("ext.a");
        for _ in 0..MAX_CONSECUTIVE_CRASHES {
            g.record_crash();
        }
        assert!(g.is_tripped());
        g.reset();
        assert!(!g.is_tripped());
        assert_eq!(g.consecutive(), 0);
        // 复位后可重新计数
        assert!(!g.record_crash());
        assert!(!g.is_tripped());
    }

    #[test]
    fn one_crash_then_recovery_does_not_trip() {
        // 崩一次 → 恢复 → 崩一次 → 恢复：永远不熔断（计数被清零）
        let mut g = CrashGuard::new("ext.a");
        for _ in 0..4 {
            assert!(!g.record_crash());
            g.reset();
        }
        assert!(!g.is_tripped());
    }
}
