//! §2.2/§2.3/§2.4 NDJSON 成帧：一行一条紧凑 JSON，`\n` 结尾，UTF-8。
//!
//! 解码器按 §2.4 做**增量缓冲**：一次 `push` 可能返回零条、一条或多条消息，
//! 未遇 `\n` 的残留保留在内部缓冲区。

use serde::{Deserialize, Serialize};

/// §2.3 默认单条消息上限 1 MiB（1 048 576 字节）。
pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 1_048_576;

/// 解码产物。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// 一条完整消息（不含行尾 `\n`）。
    Message(String),
    /// §2.3：单条消息超过上限。接收方应回 `-32600 Invalid Request`
    ///（无法解析 `id` 时为 `null`）**并关闭连接**——继续读取可能导致流错位。
    TooLarge { size: usize, max: usize },
    /// §2.2 规则 3：行内容不是合法 UTF-8。
    InvalidUtf8,
}

/// 编码错误（§2.2 规则 2：JSON 内部不得出现裸换行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BareNewlineError;

/// NDJSON 增量解码器。
#[derive(Debug, Clone)]
pub struct Decoder {
    max: usize,
    buf: Vec<u8>,
}

impl Decoder {
    /// 以指定的单条消息上限创建解码器。
    pub fn new(max: usize) -> Self {
        Self {
            max,
            buf: Vec::new(),
        }
    }

    /// 以默认上限（1 MiB）创建解码器。
    pub fn with_default_limit() -> Self {
        Self::new(DEFAULT_MAX_MESSAGE_BYTES)
    }

    /// 喂入一段字节，切出所有完整消息。
    ///
    /// 行为按 §2.2：剥离行尾 `\r`（CRLF 容错）、忽略空行、超限产出
    /// [`Frame::TooLarge`]、非法 UTF-8 产出 [`Frame::InvalidUtf8`]。
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Frame> {
        self.buf.extend_from_slice(chunk);
        let mut frames = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
            line.pop(); // 去掉行尾 \n
            if line.last() == Some(&b'\r') {
                line.pop(); // §2.2 规则 1：CRLF 容错
            }
            if line.is_empty() {
                continue; // §2.2 规则 5：空行忽略
            }
            if line.len() > self.max {
                frames.push(Frame::TooLarge {
                    size: line.len(),
                    max: self.max,
                });
                continue;
            }
            match String::from_utf8(line) {
                Ok(s) => frames.push(Frame::Message(s)),
                Err(_) => frames.push(Frame::InvalidUtf8),
            }
        }
        frames
    }

    /// 缓冲区中尚未构成完整消息的字节数（§2.4 增量缓冲残留）。
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }
}

/// 编码一条消息：附加行尾 `\n`。
///
/// 消息内部不得含裸 `\n`（§2.2 规则 2）——JSON 字符串中的换行必须
/// 以 `\n` 转义（两个字符），serde_json 等序列化器天然保证。
pub fn encode(message: &str) -> Result<Vec<u8>, BareNewlineError> {
    if message.contains('\n') {
        return Err(BareNewlineError);
    }
    let mut out = Vec::with_capacity(message.len() + 1);
    out.extend_from_slice(message.as_bytes());
    out.push(b'\n');
    Ok(out)
}

/// serde 便捷编码：序列化为紧凑 JSON 并成帧（紧凑 = 无 pretty-print，§2.2 规则 4）。
pub fn encode_message<T: Serialize>(value: &T) -> Result<Vec<u8>, EncodeMessageError> {
    let line = serde_json::to_string(value)?;
    encode(&line).map_err(|_| EncodeMessageError::BareNewline)
}

/// [`encode_message`] 的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeMessageError {
    Serialize(String),
    BareNewline,
}

impl From<serde_json::Error> for EncodeMessageError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialize(e.to_string())
    }
}

/// serde 便捷解码：把一条消息行反序列化为指定类型。
pub fn decode_message<T: for<'de> Deserialize<'de>>(line: &str) -> serde_json::Result<T> {
    serde_json::from_str(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(s: &str) -> Frame {
        Frame::Message(s.to_string())
    }

    #[test]
    fn splits_multiple_messages_in_one_chunk() {
        let mut d = Decoder::with_default_limit();
        let frames = d.push(b"{\"a\":1}\n{\"a\":2}\n");
        assert_eq!(frames, vec![m("{\"a\":1}"), m("{\"a\":2}")]);
        assert_eq!(d.buffered(), 0);
    }

    #[test]
    fn retains_partial_message_across_pushes() {
        // §2.4：一次 read 可能返回半条消息
        let mut d = Decoder::with_default_limit();
        assert!(d.push(b"{\"a\":").is_empty());
        assert_eq!(d.buffered(), 5);
        let frames = d.push(b"1}\n");
        assert_eq!(frames, vec![m("{\"a\":1}")]);
    }

    #[test]
    fn strips_trailing_cr() {
        let mut d = Decoder::with_default_limit();
        let frames = d.push(b"{\"a\":1}\r\n");
        assert_eq!(frames, vec![m("{\"a\":1}")]);
    }

    #[test]
    fn ignores_empty_lines() {
        let mut d = Decoder::with_default_limit();
        let frames = d.push(b"\n\n{\"a\":1}\n\n");
        assert_eq!(frames, vec![m("{\"a\":1}")]);
    }

    #[test]
    fn rejects_oversized_message() {
        let mut d = Decoder::new(8);
        let frames = d.push(b"123456789\n{\"a\":1}\n");
        assert_eq!(
            frames,
            vec![Frame::TooLarge { size: 9, max: 8 }, m("{\"a\":1}")]
        );
    }

    #[test]
    fn reports_invalid_utf8() {
        let mut d = Decoder::with_default_limit();
        let frames = d.push(&b"\xff\xfe\n"[..]);
        assert_eq!(frames, vec![Frame::InvalidUtf8]);
    }

    #[test]
    fn encode_appends_newline_and_rejects_bare_newline() {
        assert_eq!(encode("{}").unwrap(), b"{}\n");
        assert_eq!(encode("a\nb"), Err(BareNewlineError));
    }

    #[test]
    fn message_roundtrip_via_serde() {
        use crate::model::CommandResult;
        let r = CommandResult::ShowToast {
            message: "= 2".into(),
            duration_ms: Some(2000),
        };
        let bytes = encode_message(&r).unwrap();
        assert_eq!(
            &bytes[..],
            b"{\"kind\":\"ShowToast\",\"args\":{\"message\":\"= 2\",\"duration_ms\":2000}}\n"
        );
        let line = std::str::from_utf8(&bytes).unwrap().trim_end_matches('\n');
        let back: CommandResult = decode_message(line).unwrap();
        assert_eq!(back, r);
    }
}
