//! 批次 B（refactor-structure-2026-10-21.md §4.2）：协议常量与聚合数据模型。
//! 自 `aggregator.rs` 原块（51–144 行）逐字节搬入、零逻辑改动；
//! 外部路径经 `aggregator/mod.rs` 的 `pub use` 桥保持不变。

use crate::ext_client::ExtClient;
use dd_protocol::model::CommandItem;

/// 协议版本（protocol.md §13：`MAJOR.MINOR` 两段）。
pub const PROTOCOL_VERSION: &str = "1.0";
/// 宿主版本（semver，`initialize` 的 `host.version`）。
pub const HOST_VERSION: &str = env!("CARGO_PKG_VERSION");

/// N1（2026-10-04）：自定义直达命令虚拟条目的所属扩展 id——**保留 id**，
/// 不对应任何清单扩展；`dispatch_invoke` 对它走宿主内部分发（§3 惯例 ⑤）。
pub const HOST_CUSTOM_EXT_ID: &str = "com.ddrun.host";
/// N1：自定义直达命令条目 id 前缀（`custom:{keyword}`）。
pub const CUSTOM_ITEM_PREFIX: &str = "custom:";

/// 单个扩展的拉取结果（**不携带进程**，便于纯逻辑单测构造）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtItems {
    /// 进程保活、命令已取回（warm）。
    Ready {
        id: String,
        name: String,
        items: Vec<CommandItem>,
    },
    /// 磁盘桩命中（frozen 冷启动，**无进程**，A6）；点击其命令触发复热。
    Stub {
        id: String,
        name: String,
        items: Vec<CommandItem>,
    },
    Failed {
        id: String,
        name: String,
        error: String,
    },
}

/// 一次聚合的完整结果。
pub struct CollectResult {
    /// 成功拉取后**保活的客户端**（顺序与 [`ExtItems::Ready`] 一一对应，
    /// 供 M2 的 `invoke` 复用；不手动 `close`，随宿主退出由 Drop 清理）。
    /// M9：包含内置 in-process 与第三方子进程两类后端（[`ExtClient`]）。
    pub processes: Vec<ExtClient>,
    /// 每个扩展的拉取结果（含失败项）。
    pub per_ext: Vec<ExtItems>,
}

/// 扩展源状态汇总（页脚/列表尾展示）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSummary {
    pub id: String,
    pub name: String,
    pub status: SourceStatus,
}

/// 单个扩展源的展示状态（M3 起为**三态**，供页脚展示与 A6 真机观察）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceStatus {
    /// 进程保活中（warm）：冷启动 fresh 扩展、或桩项复热成功后。
    Warm {
        commands: usize,
    },
    /// 仅磁盘桩、无活进程（frozen 冷启动读桩；LRU 驱逐后回落）。点击其命令会触发复热。
    Stub {
        commands: usize,
    },
    Failed {
        error: String,
    },
}

impl SourceStatus {
    /// 是否处于失败态。
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    /// 是否处于桩态（无进程，点击需复热）。
    pub fn is_stub(&self) -> bool {
        matches!(self, Self::Stub { .. })
    }
}

impl ExtItems {
    /// 扩展清单 id（`Ready` / `Stub` / `Failed` 均有，供进程配对与诊断）。
    pub fn id(&self) -> &str {
        match self {
            ExtItems::Ready { id, .. }
            | ExtItems::Stub { id, .. }
            | ExtItems::Failed { id, .. } => id,
        }
    }

    /// 是否已成功拉取并保活进程（warm）。
    pub fn is_ready(&self) -> bool {
        matches!(self, ExtItems::Ready { .. })
    }
}
