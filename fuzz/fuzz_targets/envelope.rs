//! 清单 §2.2 / 实施计划 §2.4（2026-10-30）：fuzz target 2——信封校验。
//!
//! 判据：任意字节 → `std::str::from_utf8`（非 UTF-8 输入跳过——UTF-8 解码面
//! 归 decoder target）→ [`dd_protocol::validate`]（三态 `Valid` / `ParseError`
//! / `InvalidRequest`，含 `InvalidReason` 全分支）——断言 = **不 panic**；
//! 连带走 [`dd_protocol::Envelope::to_error_response`]（`-32700` / `-32600`
//! 错误响应构造的纯格式化面，`reason.kind()` / `message()` / serde 序列化
//! 一并进猎场）。
//!
//! 语料种子（`corpus/envelope/`）：合法请求 / 合法响应 / 合法错误响应 /
//! 截断 JSON（`-32700`）/ 批处理数组 / 非对象标量 / 非法 `id` / 缺 `jsonrpc` /
//! 版本不符 / 非字符串 `method` / 数组形 params / `result`+`error` 并存 /
//! 非对象 `error`——`ParseError` 与 `InvalidReason` 各分支一枚。

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(line) = std::str::from_utf8(data) {
        let envelope = dd_protocol::validate(line);
        if let Some(error) = envelope.to_error_response() {
            std::hint::black_box(&error);
        }
        std::hint::black_box(&envelope);
    }
});
