use agent_studios_external_runtime::{
    RuntimeEventKind, RuntimeInstanceId, RuntimeSessionId, SanitizedRuntimeMessage, SessionEventHub,
};
use agent_studios_protocol::id::ApprovalId;
use bytes::Bytes;
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, warn};

use crate::error::OpenCodeError;

/// Raw Server-Sent Event frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    pub event_type: String,
    pub data: String,
    pub id: Option<String>,
}

/// Robust zero-copy SSE line buffer and parser.
pub struct SseParser {
    buffer: String,
    current_event: Option<String>,
    current_data: Vec<String>,
    current_id: Option<String>,
}

impl Default for SseParser {
    fn default() -> Self {
        Self::new()
    }
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            current_event: None,
            current_data: Vec::new(),
            current_id: None,
        }
    }

    /// Feeds a chunk of bytes into the parser and extracts completed SSE events.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        let chunk_str = String::from_utf8_lossy(chunk);
        self.buffer.push_str(&chunk_str);

        let mut events = Vec::new();
        while let Some(line_end) = self.buffer.find('\n') {
            let mut line = self.buffer[..line_end].to_string();
            self.buffer.drain(..=line_end);

            if line.ends_with('\r') {
                line.pop();
            }

            if line.is_empty() {
                // Dispatch event on empty line
                if !self.current_data.is_empty() || self.current_event.is_some() {
                    let event_type = self
                        .current_event
                        .take()
                        .unwrap_or_else(|| "message".to_string());
                    let data = self.current_data.join("\n");
                    self.current_data.clear();
                    let id = self.current_id.take();

                    events.push(SseEvent {
                        event_type,
                        data,
                        id,
                    });
                }
            } else if line.starts_with(':') {
                // Comment/keep-alive, ignore
                continue;
            } else if let Some(stripped) = line.strip_prefix("event:") {
                self.current_event = Some(stripped.trim().to_string());
            } else if let Some(stripped) = line.strip_prefix("data:") {
                let data_part = stripped.strip_prefix(' ').unwrap_or(stripped);
                self.current_data.push(data_part.to_string());
            } else if let Some(stripped) = line.strip_prefix("id:") {
                self.current_id = Some(stripped.trim().to_string());
            }
        }

        events
    }
}

/// Translates an OpenCode SSE event frame into normalized M10 `RuntimeEventKind`.
pub fn translate_opencode_event(
    event_type: &str,
    data: &str,
    session_id: &RuntimeSessionId,
    instance_id: &RuntimeInstanceId,
) -> Result<Option<RuntimeEventKind>, OpenCodeError> {
    if data.trim().is_empty() {
        return Ok(None);
    }

    let parsed_json: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => {
            // Unstructured text data fallback
            return Ok(Some(RuntimeEventKind::OutputDelta {
                text: data.to_string(),
            }));
        }
    };

    let normalized_type = parsed_json
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or(event_type);

    match normalized_type {
        "session.created" | "session_created" | "session.started" | "session_started" => {
            Ok(Some(RuntimeEventKind::SessionStarted {
                session_id: *session_id,
                instance_id: *instance_id,
            }))
        }
        "output.delta" | "output_delta" | "message.delta" | "message_delta" | "delta" => {
            let text = parsed_json
                .get("text")
                .or_else(|| parsed_json.get("content"))
                .or_else(|| parsed_json.get("delta"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if text.is_empty() {
                Ok(None)
            } else {
                Ok(Some(RuntimeEventKind::OutputDelta { text }))
            }
        }
        "tool.started" | "tool_started" | "tool.call" | "tool_call" => {
            let tool_name = parsed_json
                .get("tool")
                .or_else(|| parsed_json.get("tool_name"))
                .or_else(|| parsed_json.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown_tool")
                .to_string();
            let tool_call_id = parsed_json
                .get("call_id")
                .or_else(|| parsed_json.get("tool_call_id"))
                .or_else(|| parsed_json.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("call_0")
                .to_string();

            Ok(Some(RuntimeEventKind::ToolStarted {
                tool_name,
                tool_call_id,
            }))
        }
        "tool.completed" | "tool_completed" | "tool.result" | "tool_result" => {
            let tool_name = parsed_json
                .get("tool")
                .or_else(|| parsed_json.get("tool_name"))
                .or_else(|| parsed_json.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown_tool")
                .to_string();
            let tool_call_id = parsed_json
                .get("call_id")
                .or_else(|| parsed_json.get("tool_call_id"))
                .or_else(|| parsed_json.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("call_0")
                .to_string();
            let duration_ms = parsed_json
                .get("duration_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let success = parsed_json
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            Ok(Some(RuntimeEventKind::ToolCompleted {
                tool_name,
                tool_call_id,
                duration_ms,
                success,
            }))
        }
        "approval.requested" | "approval_requested" | "confirmation" => {
            let approval_id = parsed_json
                .get("approval_id")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<uuid::Uuid>().ok())
                .map(ApprovalId::from)
                .unwrap_or_default();

            let desc = parsed_json
                .get("description")
                .or_else(|| parsed_json.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("Action requires approval");

            let command = parsed_json
                .get("command")
                .and_then(|v| v.as_str())
                .map(SanitizedRuntimeMessage::new);

            Ok(Some(RuntimeEventKind::ApprovalRequested {
                approval_id,
                description: SanitizedRuntimeMessage::new(desc),
                command,
            }))
        }
        "artifact.produced" | "artifact_produced" => {
            let logical_name = parsed_json
                .get("logical_name")
                .or_else(|| parsed_json.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("artifact")
                .to_string();
            let path = parsed_json
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            Ok(Some(RuntimeEventKind::ArtifactProduced {
                logical_name,
                path,
            }))
        }
        "usage" | "usage.updated" | "usage_updated" => {
            let prompt = parsed_json.get("prompt_tokens").and_then(|v| v.as_u64());
            let completion = parsed_json
                .get("completion_tokens")
                .and_then(|v| v.as_u64());
            let total = parsed_json.get("total_tokens").and_then(|v| v.as_u64());

            Ok(Some(RuntimeEventKind::UsageUpdated {
                prompt_tokens: prompt,
                completion_tokens: completion,
                total_tokens: total,
            }))
        }
        "diagnostic" | "log" => {
            let level = parsed_json
                .get("level")
                .and_then(|v| v.as_str())
                .unwrap_or("info")
                .to_string();
            let msg = parsed_json
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            Ok(Some(RuntimeEventKind::Diagnostic {
                level,
                message: SanitizedRuntimeMessage::new(msg),
            }))
        }
        "interrupted" | "session.interrupted" | "aborted" => {
            let reason = parsed_json
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("Execution interrupted by user");
            Ok(Some(RuntimeEventKind::Interrupted {
                reason: SanitizedRuntimeMessage::new(reason),
            }))
        }
        "failed" | "session.failed" | "error" => {
            let error_summary = parsed_json
                .get("error")
                .or_else(|| parsed_json.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("OpenCode session execution failed");

            Ok(Some(RuntimeEventKind::Failed {
                safe_error_summary: SanitizedRuntimeMessage::new(error_summary),
            }))
        }
        "completed" | "turn.completed" | "session.completed" => {
            let summary = parsed_json
                .get("summary")
                .or_else(|| parsed_json.get("text"))
                .and_then(|v| v.as_str())
                .map(ToString::to_string);

            Ok(Some(RuntimeEventKind::Completed { summary }))
        }
        "stopped" | "session.stopped" => Ok(Some(RuntimeEventKind::Stopped)),
        other => {
            debug!(
                event_type = other,
                "Unhandled OpenCode SSE event type; translating as diagnostic"
            );
            Ok(Some(RuntimeEventKind::Diagnostic {
                level: "debug".to_string(),
                message: SanitizedRuntimeMessage::new(format!("OpenCode event {other}: {data}")),
            }))
        }
    }
}

/// Runs background SSE ingestion loop, reading frames from the HTTP stream and feeding them into the SessionEventHub.
/// Guarantees that sequence 1 is SessionStarted, enforces monotonic ordering, and stops on terminal events.
pub async fn run_sse_ingestion_loop(
    mut response: reqwest::Response,
    hub: Arc<SessionEventHub>,
    session_id: RuntimeSessionId,
    instance_id: RuntimeInstanceId,
) {
    let mut parser = SseParser::new();

    // Enforce sequence 1 SessionStarted authoritative invariant
    let _ = hub
        .emit(RuntimeEventKind::SessionStarted {
            session_id,
            instance_id,
        })
        .await;
    let session_started_ingested = true;

    while let Some(chunk_res) = response.chunk().await.transpose() {
        let chunk: Bytes = match chunk_res {
            Ok(b) => b,
            Err(e) => {
                warn!(error = %e, "SSE stream read error");
                let _ = hub
                    .emit(RuntimeEventKind::Failed {
                        safe_error_summary: SanitizedRuntimeMessage::new(format!(
                            "SSE stream read error: {e}"
                        )),
                    })
                    .await;
                break;
            }
        };

        let events = parser.feed(&chunk);
        for event in events {
            match translate_opencode_event(
                &event.event_type,
                &event.data,
                &session_id,
                &instance_id,
            ) {
                Ok(Some(kind)) => {
                    // Skip redundant SessionStarted if already emitted
                    if matches!(kind, RuntimeEventKind::SessionStarted { .. })
                        && session_started_ingested
                    {
                        continue;
                    }

                    let is_terminal = kind.is_terminal();
                    if let Err(e) = hub.emit(kind).await {
                        warn!(error = %e, "Event hub rejected event or session terminal");
                        return;
                    }

                    if is_terminal {
                        debug!("Terminal event ingested; closing SSE reader loop");
                        return;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    warn!(error = %e, "Failed to translate OpenCode SSE event");
                }
            }
        }
    }

    debug!("SSE stream completed cleanly");
}
