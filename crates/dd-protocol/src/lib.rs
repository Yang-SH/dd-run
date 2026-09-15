//! dd-run Extension Protocol v1.0 的 Rust 类型投影。
//!
//! 契约来源：[`docs/protocol.md`](../../docs/protocol.md)（SSOT）。
//! 本 crate 是协议的数据层投影：JSON-RPC 2.0 信封、12 个方法的参数/结果、
//! §8 数据模型与 §2.2 NDJSON 成帧。方法名常量层见 [`methods`]（单一来源，O2）；
//! §3.2/§3.4 信封校验见 [`envelope`]（单一来源，O1）；日志后端与级别开关见 [`logging`]（O4）。
//! 字段可选性与文档字段表逐项对应；未知字段一律忽略（§13 协议演进规则）。

pub mod envelope;
pub mod framing;
pub mod logging;
pub mod messages;
pub mod methods;
pub mod model;

pub use envelope::{error_response, validate, validate_value, Envelope, InvalidReason};
pub use framing::{encode, Decoder, Frame, DEFAULT_MAX_MESSAGE_BYTES};
pub use messages::{RawMessage, RpcError};
