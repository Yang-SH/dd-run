//! 宿主逻辑层。
//!
//! 契约来源：
//! - [`docs/manifest-schema.md`](../../docs/manifest-schema.md)（清单扫描，见 [`manifest`]）
//! - [`docs/protocol.md`](../../docs/protocol.md)（子进程通信，见 [`process`]）
//!
//! S-05 扩展信任分级（宿主私有台账，**非**协议/清单契约）见 [`trust`]。

pub mod builtin;
pub mod cache;
pub mod manifest;
pub mod process;
pub mod trust;
