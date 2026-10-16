use agent_studios_external_runtime::{
    RuntimeEventKind, RuntimeInstanceId, RuntimeSessionId, SanitizedRuntimeMessage, SessionEventHub,
};
use agent_studios_protocol::id::ApprovalId;
use bytes::Bytes;
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, warn};

use crate::error::OpenCodeError;

/// Maximum buffer capacity for an SSE stream parser (1 MiB) to guard against unbounded memory growth.
pub const MAX_SSE_BUFFER_CAPACITY: usize = 1024 * 1024;
/// Maximum line length for an individual SSE line (64 KiB).
pub const MAX_SSE_LINE_LENGTH: usize = 64 * 1024;

/// Raw Server-Sent Event frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    pub event_type: String,
    pub data: String,
    pub id: Option<String>,
}

/// Robust bounded SSE line buffer and parser.
pub struct SseParser {
    buffer: String,
    current_event: Option<String>,
    current_data: Vec<String>,
    current_id: Option<String>,
    last_event_id: Option<String>,
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
            last_event_id: None,
        }
    }

    /// Last observed event ID in the SSE stream, if any.
    pub fn last_event_id(&self) -> Option<&str> {
        self.last_event_id.as_deref()
    }

    /// Clears any pending incomplete buffer state.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.current_event = None;
        self.current_data.clear();
        self.current_id = None;
    }

    /// Feeds a chunk of bytes into the parser and extracts completed SSE events.
    /// Enforces maximum buffer capacity to prevent memory exhaustion.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        if self.buffer.len() + chunk.len() > MAX_SSE_BUFFER_CAPACITY {
            warn!(
                buffer_len = self.buffer.len(),
                chunk_len = chunk.len(),
                limit = MAX_SSE_BUFFER_CAPACITY,
                "SSE parser buffer capacity exceeded; resetting pending buffer to prevent memory exhaustion"
            );
            self.clear();
        }

        let chunk_str = String::from_utf8_lossy(chunk);
        self.buffer.push_str(&chunk_str);

        let mut events = Vec::new();
        while let Some(line_end) = self.buffer.find('\n') {
            if line_end > MAX_SSE_LINE_LENGTH {
                warn!(
                    line_len = line_end,
                    limit = MAX_SSE_LINE_LENGTH,
                    "SSE line exceeded maximum permitted length; discarding line"
                );
                self.buffer.drain(..=line_end);
                continue;
            }

            let mut line = self.buffer[..line_end].to_string();
            self.buffer.drain(..=line_end);

            if line.ends_with('\r') {
                line.pop();
            }

            if line.is_empty() {
                // Dispatch event on empty line (SSE boundary)
                if !self.current_data.is_empty() || self.current_event.is_some() {
                    let event_type = self
                        .current_event
                        .take()
                        .unwrap_or_else(|| "message".to_string());
                    let data = self.current_data.join("\n");
                    self.current_data.clear();
                    let id = self.current_id.take();

                    if let Some(ref eid) = id {
                        self.last_event_id = Some(eid.clone());
                    }

                    events.push(SseEvent {
                        event_type,
                        data,
                        id,
                    });
                }
            } else if line.starts_with(':') {
                // Comment or keep-alive ping, ignore
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

/// Helper to extract a property value checking upstream OpenCode `properties` envelope first,
/// then falling back to root-level attribute.
fn get_prop<'a>(root: &'a Value, key: &str) -> Option<&'a Value> {
    root.get("properties")
        .and_then(|props| props.get(key))
        .or_else(|| root.get(key))
}

/// Helper to extract session ID checking both `properties` and root under multiple naming conventions.
fn extract_session_id(root: &Value) -> Option<&str> {
    get_prop(root, "sessionID")
        .or_else(|| get_prop(root, "sessionId"))
        .or_else(|| get_prop(root, "session_id"))
        .and_then(Value::as_str)
}

/// Translates an OpenCode SSE event frame into normalized M10 `RuntimeEventKind`.
pub fn translate_opencode_event(
    event_type: &str,
    data: &str,
    session_id: &RuntimeSessionId,
    instance_id: &RuntimeInstanceId,
) -> Result<Option<RuntimeEventKind>, OpenCodeError> {
    translate_opencode_event_filtered(event_type, data, session_id, instance_id, None)
}

/// Translates an OpenCode SSE event frame with session-id filtering to prevent cross-session event leakage.
pub fn translate_opencode_event_filtered(
    event_type: &str,
    data: &str,
    session_id: &RuntimeSessionId,
    instance_id: &RuntimeInstanceId,
    target_opencode_session_id: Option<&str>,
) -> Result<Option<RuntimeEventKind>, OpenCodeError> {
    if data.trim().is_empty() {
        return Ok(None);
    }

    let parsed_json: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => {
            // Unstructured plain-text data fallback
            return Ok(Some(RuntimeEventKind::OutputDelta {
                text: data.to_string(),
            }));
        }
    };

    // Session-ID filtering to prevent cross-session leakage (M11-R03)
    let filtered_mismatch = target_opencode_session_id
        .filter(|t| !t.is_empty())
        .and_then(|target| {
            extract_session_id(&parsed_json)
                .filter(|&ev_sid| ev_sid != target)
                .map(|ev_sid| (ev_sid, target))
        });
    if let Some((ev_sid, target)) = filtered_mismatch {
        debug!(
            event_session = ev_sid,
            target_session = target,
            "Dropping event belonging to different OpenCode session"
        );
        return Ok(None);
    }

    let normalized_type = parsed_json
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or(event_type);

    match normalized_type {
        "server.connected" | "ping" => {
            debug!("OpenCode server connection/ping event received");
            Ok(None)
        }
        "session.created" | "session_created" | "session.started" | "session_started" => {
            Ok(Some(RuntimeEventKind::SessionStarted {
                session_id: *session_id,
                instance_id: *instance_id,
            }))
        }
        "message.part.delta" | "output.delta" | "output_delta" | "message.delta"
        | "message_delta" | "delta" => {
            let text = get_prop(&parsed_json, "delta")
                .and_then(|v| {
                    if let Some(s) = v.as_str() {
                        Some(s.to_string())
                    } else {
                        v.get("text")
                            .and_then(Value::as_str)
                            .map(ToString::to_string)
                    }
                })
                .or_else(|| {
                    get_prop(&parsed_json, "part").and_then(|p| {
                        p.get("text")
                            .and_then(Value::as_str)
                            .map(ToString::to_string)
                    })
                })
                .or_else(|| {
                    get_prop(&parsed_json, "text")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
                .or_else(|| {
                    get_prop(&parsed_json, "content")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
                .unwrap_or_default();

            if text.is_empty() {
                Ok(None)
            } else {
                Ok(Some(RuntimeEventKind::OutputDelta { text }))
            }
        }
        "message.part.updated" => {
            if let Some(part) = get_prop(&parsed_json, "part") {
                let part_type = part.get("type").and_then(Value::as_str).unwrap_or_default();
                if part_type == "tool" || part_type == "tool_call" {
                    let tool_name = part
                        .get("tool")
                        .or_else(|| part.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("unknown_tool")
                        .to_string();
                    let tool_call_id = part
                        .get("call_id")
                        .or_else(|| part.get("id"))
                        .or_else(|| part.get("callID"))
                        .and_then(Value::as_str)
                        .unwrap_or("call_0")
                        .to_string();
                    let status = part
                        .get("status")
                        .or_else(|| part.get("state"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();

                    if status == "completed" || status == "success" {
                        let duration_ms = part
                            .get("duration_ms")
                            .or_else(|| part.get("duration"))
                            .and_then(Value::as_u64)
                            .unwrap_or(0);
                        return Ok(Some(RuntimeEventKind::ToolCompleted {
                            tool_name,
                            tool_call_id,
                            duration_ms,
                            success: true,
                        }));
                    } else if status == "failed" || status == "error" {
                        let duration_ms = part
                            .get("duration_ms")
                            .or_else(|| part.get("duration"))
                            .and_then(Value::as_u64)
                            .unwrap_or(0);
                        return Ok(Some(RuntimeEventKind::ToolCompleted {
                            tool_name,
                            tool_call_id,
                            duration_ms,
                            success: false,
                        }));
                    } else if status == "running" || status == "started" {
                        return Ok(Some(RuntimeEventKind::ToolStarted {
                            tool_name,
                            tool_call_id,
                        }));
                    }
                }
            }
            Ok(None)
        }
        "permission.updated" | "approval.requested" | "approval_requested" | "permission.asked"
        | "confirmation" => {
            let perm_obj = get_prop(&parsed_json, "permission").unwrap_or(&parsed_json);
            let perm_id_str = perm_obj
                .get("id")
                .or_else(|| perm_obj.get("permissionID"))
                .or_else(|| perm_obj.get("approval_id"))
                .and_then(Value::as_str)
                .unwrap_or_default();

            let approval_id = if let Ok(parsed_uuid) = perm_id_str.parse::<uuid::Uuid>() {
                ApprovalId::from(parsed_uuid)
            } else if !perm_id_str.is_empty() {
                // Deterministic UUID namespace derivation from permission ID string
                ApprovalId::from(uuid::Uuid::new_v5(
                    &uuid::Uuid::NAMESPACE_DNS,
                    perm_id_str.as_bytes(),
                ))
            } else {
                ApprovalId::from(uuid::Uuid::new_v4())
            };

            let desc = perm_obj
                .get("description")
                .or_else(|| perm_obj.get("message"))
                .or_else(|| perm_obj.get("title"))
                .or_else(|| perm_obj.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("Action requires approval");

            let command = perm_obj
                .get("command")
                .and_then(Value::as_str)
                .map(SanitizedRuntimeMessage::new);

            Ok(Some(RuntimeEventKind::ApprovalRequested {
                approval_id,
                description: SanitizedRuntimeMessage::new(desc),
                command,
            }))
        }
        "tool.started" | "tool_started" | "tool.call" | "tool_call" => {
            let tool_name = get_prop(&parsed_json, "tool")
                .or_else(|| get_prop(&parsed_json, "tool_name"))
                .or_else(|| get_prop(&parsed_json, "name"))
                .and_then(Value::as_str)
                .unwrap_or("unknown_tool")
                .to_string();
            let tool_call_id = get_prop(&parsed_json, "call_id")
                .or_else(|| get_prop(&parsed_json, "tool_call_id"))
                .or_else(|| get_prop(&parsed_json, "id"))
                .or_else(|| get_prop(&parsed_json, "callID"))
                .and_then(Value::as_str)
                .unwrap_or("call_0")
                .to_string();

            Ok(Some(RuntimeEventKind::ToolStarted {
                tool_name,
                tool_call_id,
            }))
        }
        "tool.completed" | "tool_completed" | "tool.result" | "tool_result" => {
            let tool_name = get_prop(&parsed_json, "tool")
                .or_else(|| get_prop(&parsed_json, "tool_name"))
                .or_else(|| get_prop(&parsed_json, "name"))
                .and_then(Value::as_str)
                .unwrap_or("unknown_tool")
                .to_string();
            let tool_call_id = get_prop(&parsed_json, "call_id")
                .or_else(|| get_prop(&parsed_json, "tool_call_id"))
                .or_else(|| get_prop(&parsed_json, "id"))
                .or_else(|| get_prop(&parsed_json, "callID"))
                .and_then(Value::as_str)
                .unwrap_or("call_0")
                .to_string();
            let duration_ms = get_prop(&parsed_json, "duration_ms")
                .or_else(|| get_prop(&parsed_json, "duration"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let success = get_prop(&parsed_json, "success")
                .and_then(Value::as_bool)
                .unwrap_or(true);

            Ok(Some(RuntimeEventKind::ToolCompleted {
                tool_name,
                tool_call_id,
                duration_ms,
                success,
            }))
        }
        "session.status" => {
            let status = get_prop(&parsed_json, "status")
                .and_then(Value::as_str)
                .unwrap_or_default();

            match status {
                "completed" => Ok(Some(RuntimeEventKind::Completed { summary: None })),
                "failed" => {
                    let err = get_prop(&parsed_json, "error")
                        .or_else(|| get_prop(&parsed_json, "message"))
                        .and_then(Value::as_str)
                        .unwrap_or("OpenCode session execution failed");
                    Ok(Some(RuntimeEventKind::Failed {
                        safe_error_summary: SanitizedRuntimeMessage::new(err),
                    }))
                }
                "aborted" | "interrupted" => Ok(Some(RuntimeEventKind::Interrupted {
                    reason: SanitizedRuntimeMessage::new("Session execution aborted"),
                })),
                "stopped" => Ok(Some(RuntimeEventKind::Stopped)),
                _ => Ok(None),
            }
        }
        "turn.completed" | "turn_completed" | "message.completed" | "session.idle" => {
            // Note: OpenCode turn completion is NOT terminal session completion (M11-R03/R05).
            // Agent Studios sessions remain active for subsequent turns.
            debug!("OpenCode turn completed; session remains active");
            Ok(Some(RuntimeEventKind::Diagnostic {
                level: "debug".to_string(),
                message: SanitizedRuntimeMessage::new("Turn completed"),
            }))
        }
        "artifact.produced" | "artifact_produced" => {
            let logical_name = get_prop(&parsed_json, "logical_name")
                .or_else(|| get_prop(&parsed_json, "name"))
                .and_then(Value::as_str)
                .unwrap_or("artifact")
                .to_string();
            let path = get_prop(&parsed_json, "path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();

            Ok(Some(RuntimeEventKind::ArtifactProduced {
                logical_name,
                path,
            }))
        }
        "usage" | "usage.updated" | "usage_updated" => {
            let prompt = get_prop(&parsed_json, "prompt_tokens")
                .or_else(|| get_prop(&parsed_json, "promptTokens"))
                .and_then(Value::as_u64);
            let completion = get_prop(&parsed_json, "completion_tokens")
                .or_else(|| get_prop(&parsed_json, "completionTokens"))
                .and_then(Value::as_u64);
            let total = get_prop(&parsed_json, "total_tokens")
                .or_else(|| get_prop(&parsed_json, "totalTokens"))
                .and_then(Value::as_u64);

            Ok(Some(RuntimeEventKind::UsageUpdated {
                prompt_tokens: prompt,
                completion_tokens: completion,
                total_tokens: total,
            }))
        }
        "diagnostic" | "log" => {
            let level = get_prop(&parsed_json, "level")
                .and_then(Value::as_str)
                .unwrap_or("info")
                .to_string();
            let msg = get_prop(&parsed_json, "message")
                .and_then(Value::as_str)
                .unwrap_or_default();

            Ok(Some(RuntimeEventKind::Diagnostic {
                level,
                message: SanitizedRuntimeMessage::new(msg),
            }))
        }
        "interrupted" | "session.interrupted" | "aborted" => {
            let reason = get_prop(&parsed_json, "reason")
                .and_then(Value::as_str)
                .unwrap_or("Execution interrupted by user");
            Ok(Some(RuntimeEventKind::Interrupted {
                reason: SanitizedRuntimeMessage::new(reason),
            }))
        }
        "failed" | "session.failed" | "session.error" | "error" => {
            let error_summary = get_prop(&parsed_json, "error")
                .or_else(|| get_prop(&parsed_json, "message"))
                .and_then(Value::as_str)
                .unwrap_or("OpenCode session execution failed");

            // Bound error summary length and sanitize
            let bounded = if error_summary.len() > 512 {
                format!("{}...", &error_summary[..512])
            } else {
                error_summary.to_string()
            };

            Ok(Some(RuntimeEventKind::Failed {
                safe_error_summary: SanitizedRuntimeMessage::new(bounded),
            }))
        }
        "completed" | "session.completed" => {
            let summary = get_prop(&parsed_json, "summary")
                .or_else(|| get_prop(&parsed_json, "text"))
                .and_then(Value::as_str)
                .map(ToString::to_string);

            Ok(Some(RuntimeEventKind::Completed { summary }))
        }
        "stopped" | "session.stopped" => Ok(Some(RuntimeEventKind::Stopped)),
        other => {
            debug!(
                event_type = other,
                "Unhandled OpenCode SSE event type; translating as static diagnostic without payload dump"
            );
            // Security: Never dump raw payload JSON into diagnostic messages (M11-R03)
            Ok(Some(RuntimeEventKind::Diagnostic {
                level: "debug".to_string(),
                message: SanitizedRuntimeMessage::new(format!(
                    "OpenCode unhandled event type: {other}"
                )),
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
    target_opencode_session_id: Option<String>,
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
            match translate_opencode_event_filtered(
                &event.event_type,
                &event.data,
                &session_id,
                &instance_id,
                target_opencode_session_id.as_deref(),
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
