//! 清单 §2.2 / 实施计划 §2.4（2026-10-30）：fuzz target 1——NDJSON 成帧。
//!
//! 判据：任意字节流喂 [`dd_protocol::Decoder::with_default_limit`] + `push`
//! （该 API 无 `Err` 分支，产物 [`dd_protocol::Frame`] 三态 = `Message` /
//! `TooLarge` / `InvalidUtf8`）——唯一断言 = **不 panic**；另锚两条契约不变式
//! （§2.3 / §2.4）：`Message` 行长 ≤ 上限、`TooLarge.size` > `TooLarge.max`。
//!
//! 语料种子（`corpus/decoder/`）：合法帧 / 多帧 / CRLF+空行 / 裸 `\n` /
//! 截断 UTF-8 / 无换行残留 / 多字节 UTF-8。刻意**不放** 1 MiB 超限种子——
//! libFuzzer 的 `max_len` 取语料最大单元，放大 max_len 会显著拉低执行吞吐，
//! 而验收判据是「累计 ≥ 10⁷ 次执行零 panic」（吞吐优先）；超限双路（终止行
//! 不毒化 / 无终止毒化）已有 S-02 专设单测覆盖（`framing` 单测 +
//! `unterminated_stream_is_bounded_and_reports_once`），fuzz 的职责是 crash
//! 狩猎而非路径覆盖。

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut decoder = dd_protocol::Decoder::with_default_limit();
    let frames = decoder.push(data);
    for frame in &frames {
        match frame {
            dd_protocol::Frame::Message(line) => assert!(
                line.len() <= dd_protocol::DEFAULT_MAX_MESSAGE_BYTES,
                "Message 超上限：{} > {}",
                line.len(),
                dd_protocol::DEFAULT_MAX_MESSAGE_BYTES
            ),
            dd_protocol::Frame::TooLarge { size, max } => {
                assert!(size > max, "TooLarge 但 size ≤ max：{size} ≤ {max}");
            }
            dd_protocol::Frame::InvalidUtf8 => {}
        }
    }
    std::hint::black_box(&frames);
});
