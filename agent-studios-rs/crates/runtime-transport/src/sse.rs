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

/// Incremental parser for Server-Sent Events over raw byte chunks.
#[derive(Debug, Default)]
pub struct SseParser {
    buffer: String,
    current_event: Option<String>,
    current_data: Vec<String>,
    current_id: Option<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes incoming byte chunks into the buffer and drains all complete SSE events.
    pub fn push_chunk(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, TransportError> {
        let chunk_str = std::str::from_utf8(chunk)
            .map_err(|e| TransportError::StreamDecode(format!("Invalid UTF-8 chunk: {e}")))?;
        self.buffer.push_str(chunk_str);

        let mut events = Vec::new();

        while let Some(line_end) = self.buffer.find('\n') {
            let mut line = self.buffer[..line_end].to_string();
            if line.ends_with('\r') {
                line.pop();
            }
            self.buffer.drain(..=line_end);

            if line.is_empty() {
                // Empty line is dispatch boundary for SSE event
                if !self.current_data.is_empty() {
                    let data = self.current_data.join("\n");
                    events.push(SseEvent {
                        event: self.current_event.take(),
                        data,
                        id: self.current_id.take(),
                    });
                    self.current_data.clear();
                }
            } else if line.starts_with(':') {
                // Comment line, ignore
                continue;
            } else if let Some(stripped) = line.strip_prefix("data:") {
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

        Ok(events)
    }

    /// Flushes any remaining trailing event in the buffer upon stream completion.
    pub fn finish(&mut self) -> Vec<SseEvent> {
        if !self.buffer.is_empty() {
            let mut line = std::mem::take(&mut self.buffer);
            if line.ends_with('\r') {
                line.pop();
            }
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

        let mut events = Vec::new();
        if !self.current_data.is_empty() {
            let data = self.current_data.join("\n");
            events.push(SseEvent {
                event: self.current_event.take(),
                data,
                id: self.current_id.take(),
            });
            self.current_data.clear();
        }
        events
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
        Self {
            inner,
            parser: SseParser::new(),
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
                    let remaining = self.parser.finish();
                    for ev in remaining {
                        self.pending_events.push_back(ev);
                    }
                    if let Some(ev) = self.pending_events.pop_front() {
                        return Poll::Ready(Some(Ok(ev)));
                    }
                    return Poll::Ready(None);
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
        let mut parser = SseParser::new();
        let chunk = b"event: message\ndata: hello world\nid: 1\n\n";
        let events = parser.push_chunk(chunk).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("message"));
        assert_eq!(events[0].data, "hello world");
        assert_eq!(events[0].id.as_deref(), Some("1"));
    }

    #[test]
    fn test_sse_parser_multiline_data_and_comments() {
        let mut parser = SseParser::new();
        let chunk = b": heart beat comment\ndata: first line\ndata: second line\n\n";
        let events = parser.push_chunk(chunk).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "first line\nsecond line");
        assert_eq!(events[0].event, None);
    }

    #[test]
    fn test_sse_parser_fragmented_chunks() {
        let mut parser = SseParser::new();
        let events1 = parser.push_chunk(b"data: hel").unwrap();
        assert!(events1.is_empty());

        let events2 = parser.push_chunk(b"lo\n\n").unwrap();
        assert_eq!(events2.len(), 1);
        assert_eq!(events2[0].data, "hello");
    }

    #[test]
    fn test_sse_parser_finish_trailing_data() {
        let mut parser = SseParser::new();
        let _ = parser.push_chunk(b"data: final unclosed event");
        let events = parser.finish();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "final unclosed event");
    }
}
