use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::error::TransportError;

/// A decoded Server-Sent Event frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
    pub id: Option<String>,
}

/// Incremental byte-safe parser for Server-Sent Events over raw byte chunks.
///
/// Buffers raw `Vec<u8>` to ensure multibyte UTF-8 sequences split across TCP packet
/// boundaries are preserved until complete lines are assembled. Enforces `max_event_bytes`.
#[derive(Debug)]
pub struct SseParser {
    buffer: Vec<u8>,
    current_event: Option<String>,
    current_data: Vec<String>,
    current_id: Option<String>,
    max_event_bytes: usize,
    accumulated_data_bytes: usize,
}

impl Default for SseParser {
    fn default() -> Self {
        Self::new(4 * 1024 * 1024)
    }
}

impl SseParser {
    pub fn new(max_event_bytes: usize) -> Self {
        Self {
            buffer: Vec::new(),
            current_event: None,
            current_data: Vec::new(),
            current_id: None,
            max_event_bytes,
            accumulated_data_bytes: 0,
        }
    }

    /// Pushes incoming byte chunks into the buffer and drains all complete SSE events.
    pub fn push_chunk(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, TransportError> {
        self.buffer.extend_from_slice(chunk);
        if self.buffer.len() > self.max_event_bytes {
            return Err(TransportError::SseFrameTooLarge {
                size: self.buffer.len(),
                max_bytes: self.max_event_bytes,
            });
        }

        let mut events = Vec::new();

        while let Some(line_end) = self.buffer.iter().position(|&b| b == b'\n') {
            let mut line_bytes = &self.buffer[..line_end];
            if line_bytes.ends_with(b"\r") {
                line_bytes = &line_bytes[..line_bytes.len() - 1];
            }

            let line = std::str::from_utf8(line_bytes).map_err(|e| {
                TransportError::StreamDecode(format!("Invalid UTF-8 sequence in SSE stream: {e}"))
            })?;

            let line_str = line.to_string();
            self.buffer.drain(..=line_end);

            if line_str.is_empty() {
                // Empty line is dispatch boundary for SSE event
                if !self.current_data.is_empty() {
                    let data = self.current_data.join("\n");
                    events.push(SseEvent {
                        event: self.current_event.take(),
                        data,
                        id: self.current_id.take(),
                    });
                    self.current_data.clear();
                    self.accumulated_data_bytes = 0;
                }
            } else if line_str.starts_with(':') {
                // Comment line, ignore
                continue;
            } else if let Some(stripped) = line_str.strip_prefix("data:") {
                let data_line = stripped.strip_prefix(' ').unwrap_or(stripped);
                self.accumulated_data_bytes += data_line.len();
                if self.accumulated_data_bytes > self.max_event_bytes {
                    return Err(TransportError::SseFrameTooLarge {
                        size: self.accumulated_data_bytes,
                        max_bytes: self.max_event_bytes,
                    });
                }
                self.current_data.push(data_line.to_string());
            } else if let Some(stripped) = line_str.strip_prefix("event:") {
                let event_type = stripped.strip_prefix(' ').unwrap_or(stripped);
                self.current_event = Some(event_type.to_string());
            } else if let Some(stripped) = line_str.strip_prefix("id:") {
                let id = stripped.strip_prefix(' ').unwrap_or(stripped);
                self.current_id = Some(id.to_string());
            }
        }

        Ok(events)
    }

    /// Flushes any remaining trailing event in the buffer upon stream completion.
    pub fn finish(&mut self) -> Result<Vec<SseEvent>, TransportError> {
        if !self.buffer.is_empty() {
            let mut line_bytes = self.buffer.as_slice();
            if line_bytes.ends_with(b"\r") {
                line_bytes = &line_bytes[..line_bytes.len() - 1];
            }
            if !line_bytes.is_empty() {
                let line = std::str::from_utf8(line_bytes).map_err(|e| {
                    TransportError::StreamDecode(format!("Invalid UTF-8 in trailing SSE data: {e}"))
                })?;
                if let Some(stripped) = line.strip_prefix("data:") {
                    let data_line = stripped.strip_prefix(' ').unwrap_or(stripped);
                    self.current_data.push(data_line.to_string());
                } else if let Some(stripped) = line.strip_prefix("event:") {
                    let event_type = stripped.strip_prefix(' ').unwrap_or(stripped);
                    self.current_event = Some(event_type.to_string());
                } else if let Some(stripped) = line.strip_prefix("id:") {
                    let id = stripped.strip_prefix(' ').unwrap_or(stripped);
                    self.current_id = Some(id.to_string());
                }
            }
            self.buffer.clear();
        }

        let mut events = Vec::new();
        if !self.current_data.is_empty() {
            let data = self.current_data.join("\n");
            events.push(SseEvent {
                event: self.current_event.take(),
                data,
                id: self.current_id.take(),
            });
            self.current_data.clear();
            self.accumulated_data_bytes = 0;
        }
        Ok(events)
    }
}

/// An asynchronous stream of decoded `SseEvent` items backed by a byte stream.
pub struct SseStream<S> {
    inner: S,
    parser: SseParser,
    pending_events: std::collections::VecDeque<SseEvent>,
    ended: bool,
}

impl<S> SseStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    pub fn new(inner: S) -> Self {
        Self::with_max_bytes(inner, 4 * 1024 * 1024)
    }

    pub fn with_max_bytes(inner: S, max_event_bytes: usize) -> Self {
        Self {
            inner,
            parser: SseParser::new(max_event_bytes),
            pending_events: std::collections::VecDeque::new(),
            ended: false,
        }
    }
}

impl<S> Stream for SseStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<SseEvent, TransportError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if let Some(event) = self.pending_events.pop_front() {
                return Poll::Ready(Some(Ok(event)));
            }

            if self.ended {
                return Poll::Ready(None);
            }

            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => match self.parser.push_chunk(&bytes) {
                    Ok(events) => {
                        for ev in events {
                            self.pending_events.push_back(ev);
                        }
                    }
                    Err(e) => return Poll::Ready(Some(Err(e))),
                },
                Poll::Ready(Some(Err(e))) => {
                    self.ended = true;
                    return Poll::Ready(Some(Err(TransportError::from(e))));
                }
                Poll::Ready(None) => {
                    self.ended = true;
                    match self.parser.finish() {
                        Ok(remaining) => {
                            for ev in remaining {
                                self.pending_events.push_back(ev);
                            }
                            if let Some(ev) = self.pending_events.pop_front() {
                                return Poll::Ready(Some(Ok(ev)));
                            }
                            return Poll::Ready(None);
                        }
                        Err(e) => return Poll::Ready(Some(Err(e))),
                    }
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_parser_single_event() {
        let mut parser = SseParser::default();
        let chunk = b"event: message\ndata: hello world\nid: 1\n\n";
        let events = parser.push_chunk(chunk).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("message"));
        assert_eq!(events[0].data, "hello world");
        assert_eq!(events[0].id.as_deref(), Some("1"));
    }

    #[test]
    fn test_sse_parser_multiline_data_and_comments() {
        let mut parser = SseParser::default();
        let chunk = b": heart beat comment\ndata: first line\ndata: second line\n\n";
        let events = parser.push_chunk(chunk).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "first line\nsecond line");
        assert_eq!(events[0].event, None);
    }

    #[test]
    fn test_sse_parser_fragmented_chunks() {
        let mut parser = SseParser::default();
        let events1 = parser.push_chunk(b"data: hel").unwrap();
        assert!(events1.is_empty());

        let events2 = parser.push_chunk(b"lo\n\n").unwrap();
        assert_eq!(events2.len(), 1);
        assert_eq!(events2[0].data, "hello");
    }

    #[test]
    fn test_sse_parser_split_multibyte_utf8() {
        // "Việt Nam" in UTF-8:
        // 'V' = 0x56, 'i' = 0x69, 'ệ' = [0xE1, 0xBB, 0x87], 't' = 0x74
        let full_text = "Việt Nam";
        let utf8_bytes = full_text.as_bytes();
        assert_eq!(
            utf8_bytes,
            &[0x56, 0x69, 0xE1, 0xBB, 0x87, 0x74, 0x20, 0x4E, 0x61, 0x6D]
        );

        let mut parser = SseParser::default();

        // Chunk 1 splits right in the middle of 'ệ' (after 0xE1, 0xBB)
        let mut chunk1 = b"data: Vi".to_vec();
        chunk1.extend_from_slice(&[0xE1, 0xBB]);

        let events1 = parser.push_chunk(&chunk1).unwrap();
        assert!(events1.is_empty());

        // Chunk 2 provides the final byte of 'ệ' (0x87) plus remaining text and newline
        let mut chunk2 = vec![0x87];
        chunk2.extend_from_slice(b"t Nam\n\n");

        let events2 = parser.push_chunk(&chunk2).unwrap();
        assert_eq!(events2.len(), 1);
        assert_eq!(events2[0].data, "Việt Nam");
    }

    #[test]
    fn test_sse_parser_frame_size_limit() {
        let mut parser = SseParser::new(50);
        let big_chunk = vec![b'a'; 60];
        let err = parser.push_chunk(&big_chunk).unwrap_err();
        match err {
            TransportError::SseFrameTooLarge { size, max_bytes } => {
                assert_eq!(size, 60);
                assert_eq!(max_bytes, 50);
            }
            other => panic!("Unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_sse_parser_finish_trailing_data() {
        let mut parser = SseParser::default();
        let _ = parser.push_chunk(b"data: final unclosed event");
        let events = parser.finish().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "final unclosed event");
    }
}
