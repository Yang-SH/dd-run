//! §4.5（dd-run-改进分析.md）：`Decoder::push` 成帧热路径 bench。
//!
//! 负载 = 64 行 × 1 KiB NDJSON（与协议单帧上限 1 MiB 同族的典型行尺寸），
//! 单次迭代 = 一次 `push` 喂入全部 64 KiB 并切出 64 条消息——覆盖增量缓冲、
//! 行切分（P3 单遍扫描）、UTF-8 校验的完整热路径。

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use dd_protocol::framing::Decoder;

/// 64 行紧凑 JSON，每行填充到恰好 1 KiB（含行尾 `\n`）。
fn payload_64kib() -> Vec<u8> {
    let mut buf = Vec::with_capacity(64 * 1024);
    for i in 0..64 {
        let mut line = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{i},\"result\":{{\"items\":[{{\"id\":\"cmd-{i}\",\"title\":\"Command {i}\"}},{{\"id\":\"cmd2-{i}\",\"title\":\"Command2 {i}\"}}],\"padding\":\""
        );
        // 填充 padding 字符串使整行（不含 \n）恰为 1023 字节（收尾 `"}}\n` 补 4 字节）
        while line.len() < 1024 - 4 {
            line.push('x');
        }
        line.push_str("\"}}\n");
        assert_eq!(line.len(), 1024, "行尺寸必须为 1 KiB（含 \\n）");
        buf.extend_from_slice(line.as_bytes());
    }
    buf
}

fn bench_decoder_push(c: &mut Criterion) {
    let payload = payload_64kib();
    c.bench_function("decoder_push_64x1kib", |b| {
        b.iter(|| {
            let mut d = Decoder::with_default_limit();
            let frames = d.push(black_box(&payload));
            black_box(frames.len())
        })
    });
}

criterion_group!(framing, bench_decoder_push);
criterion_main!(framing);
