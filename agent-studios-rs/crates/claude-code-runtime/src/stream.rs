use bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncReadExt};
use tracing::warn;

use crate::error::ClaudeCodeError;
use crate::protocol::ClaudeStreamMessage;

/// Maximum line length allowed for a single NDJSON message (16 MiB).
pub const MAX_LINE_LENGTH: usize = 16 * 1024 * 1024;

/// Maximum total buffer capacity before failing closed against memory exhaustion (64 MiB).
pub const MAX_BUFFER_CAPACITY: usize = 64 * 1024 * 1024;

/// Chunk size used when reading from the underlying async stream.
const READ_CHUNK_SIZE: usize = 64 * 1024;

/// Bounded NDJSON stream reader for child stdout telemetry.
pub struct NdjsonStreamReader<R> {
    reader: R,
    buffer: BytesMut,
    eof_reached: bool,
}

impl<R: AsyncRead + Unpin> NdjsonStreamReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: BytesMut::with_capacity(READ_CHUNK_SIZE),
            eof_reached: false,
        }
    }

    /// Reads the next non-empty newline-terminated string line from the stream.
    pub async fn read_line(&mut self) -> Result<Option<String>, ClaudeCodeError> {
        loop {
            // 1. Look for existing newline in accumulated buffer
            if let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
                if pos > MAX_LINE_LENGTH {
                    return Err(ClaudeCodeError::StreamParseError(format!(
                        "NDJSON line length {pos} exceeds maximum allowed limit of {MAX_LINE_LENGTH} bytes"
                    )));
                }

                let line_bytes = self.buffer.split_to(pos + 1);
                // Strip trailing \n and optional \r
                let mut slice = &line_bytes[..pos];
                if slice.ends_with(b"\r") {
                    slice = &slice[..slice.len() - 1];
                }

                if slice.is_empty() {
                    continue;
                }

                let line = std::str::from_utf8(slice)
                    .map_err(|e| {
                        ClaudeCodeError::StreamParseError(format!(
                            "Invalid UTF-8 sequence in NDJSON stream: {e}"
                        ))
                    })?
                    .to_string();

                return Ok(Some(line));
            }

            // 2. Check buffer ceiling
            if self.buffer.len() >= MAX_BUFFER_CAPACITY {
                return Err(ClaudeCodeError::StreamParseError(format!(
                    "Buffer capacity exceeded {MAX_BUFFER_CAPACITY} bytes without encountering newline delimiter"
                )));
            }

            // 3. If EOF reached and remaining buffer has content without trailing newline
            if self.eof_reached {
                if self.buffer.is_empty() {
                    return Ok(None);
                }

                if self.buffer.len() > MAX_LINE_LENGTH {
                    return Err(ClaudeCodeError::StreamParseError(format!(
                        "Final NDJSON line length {} exceeds maximum allowed limit of {MAX_LINE_LENGTH} bytes",
                        self.buffer.len()
                    )));
                }

                let remaining = self.buffer.split();
                let mut slice = remaining.as_ref();
                if slice.ends_with(b"\r") {
                    slice = &slice[..slice.len() - 1];
                }

                if slice.is_empty() {
                    return Ok(None);
                }

                let line = std::str::from_utf8(slice)
                    .map_err(|e| {
                        ClaudeCodeError::StreamParseError(format!(
                            "Invalid UTF-8 sequence in final NDJSON line: {e}"
                        ))
                    })?
                    .to_string();

                return Ok(Some(line));
            }

            // 4. Read additional bytes from async stream
            let bytes_read = self
                .reader
                .read_buf(&mut self.buffer)
                .await
                .map_err(ClaudeCodeError::ProcessIo)?;

            if bytes_read == 0 {
                self.eof_reached = true;
            }
        }
    }

    /// Reads and deserializes the next `ClaudeStreamMessage` from the stream.
    pub async fn read_message(&mut self) -> Result<Option<ClaudeStreamMessage>, ClaudeCodeError> {
        while let Some(line) = self.read_line().await? {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            match parse_ndjson_line(trimmed) {
                Ok(msg) => return Ok(Some(msg)),
                Err(e) => {
                    warn!(
                        error = %e,
                        preview = if trimmed.len() > 256 { &trimmed[..256] } else { trimmed },
                        "Failed to deserialize NDJSON message"
                    );
                    return Err(e);
                }
            }
        }

        Ok(None)
    }
}

/// Parses a single NDJSON line into a strongly-typed `ClaudeStreamMessage`.
pub fn parse_ndjson_line(line: &str) -> Result<ClaudeStreamMessage, ClaudeCodeError> {
    serde_json::from_str::<ClaudeStreamMessage>(line).map_err(|e| {
        let snippet = if line.len() > 200 {
            format!("{}...", &line[..200])
        } else {
            line.to_string()
        };
        ClaudeCodeError::StreamParseError(format!("JSON deserialization failed: {e}; input: '{snippet}'"))
    })
}
