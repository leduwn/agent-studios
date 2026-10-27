use agent_studios_external_runtime::{
    RuntimeEventKind, RuntimeSessionRef, SanitizedRuntimeMessage,
};
use agent_studios_protocol::id::ApprovalId;
use uuid::Uuid;

use crate::protocol::{
    ClaudeStreamMessage, ContentBlock, ControlRequestPayload, StreamDelta, StreamEventPayload,
    SystemMessage, ToolResultBlock,
};
use crate::subagent::SubagentCorrelationTracker;

/// Translates a raw Claude Code NDJSON wire message into normalized `RuntimeEventKind` instances.
pub fn map_claude_message(
    message: &ClaudeStreamMessage,
    _session_ref: &RuntimeSessionRef,
    subagents: &mut SubagentCorrelationTracker,
) -> Vec<RuntimeEventKind> {
    match message {
        ClaudeStreamMessage::System(system) => match system {
            SystemMessage::Init(init) => {
                let version = init.claude_code_version.as_deref().unwrap_or("unknown");
                let model = init.model.as_deref().unwrap_or("unknown");
                let tools_count = init.tools.as_ref().map(|t| t.len()).unwrap_or(0);
                let capabilities = init
                    .capabilities
                    .as_ref()
                    .map(|c| c.join(", "))
                    .unwrap_or_else(|| "none".to_string());

                vec![RuntimeEventKind::Diagnostic {
                    level: "info".to_string(),
                    message: SanitizedRuntimeMessage::new(format!(
                        "Claude Code initialized: version={version}, model={model}, tools_count={tools_count}, capabilities=[{capabilities}]"
                    )),
                }]
            }
            SystemMessage::TaskStarted(task) => {
                subagents.register_task_started(task);
                vec![RuntimeEventKind::Diagnostic {
                    level: "info".to_string(),
                    message: SanitizedRuntimeMessage::new(format!(
                        "Subagent task started: {} (id: {}, tool_call: {}, depth: {})",
                        task.description,
                        task.task_id,
                        task.tool_use_id,
                        task.spawn_depth.unwrap_or(1)
                    )),
                }]
            }
            SystemMessage::TaskNotification(task) => {
                let node = subagents.register_task_notification(task);
                let duration_ms = task.usage.as_ref().and_then(|u| u.duration_ms).unwrap_or(0);
                let success = task.status == "completed";

                let mut events = Vec::new();
                if let Some(tool_use_id) = node.map(|n| n.tool_use_id.clone()) {
                    events.push(RuntimeEventKind::ToolCompleted {
                        tool_name: "Agent".to_string(),
                        tool_call_id: tool_use_id,
                        duration_ms,
                        success,
                    });
                }

                if let Some(usage) = task.usage.as_ref().filter(|u| u.total_tokens.is_some()) {
                    events.push(RuntimeEventKind::UsageUpdated {
                        prompt_tokens: None,
                        completion_tokens: None,
                        total_tokens: usage.total_tokens,
                    });
                }

                events
            }
            SystemMessage::ApiRetry(retry) => {
                vec![RuntimeEventKind::Diagnostic {
                    level: "warn".to_string(),
                    message: SanitizedRuntimeMessage::new(format!(
                        "API retry attempt {}/{}: {}",
                        retry.attempt.unwrap_or(1),
                        retry.max_retries.unwrap_or(3),
                        retry
                            .error
                            .as_deref()
                            .unwrap_or("rate limit / transient error")
                    )),
                }]
            }
            SystemMessage::Status(status) => {
                vec![RuntimeEventKind::Diagnostic {
                    level: "info".to_string(),
                    message: SanitizedRuntimeMessage::new(format!(
                        "Claude Code status: {}",
                        status
                            .status
                            .as_deref()
                            .or(status.message.as_deref())
                            .unwrap_or("active")
                    )),
                }]
            }
            SystemMessage::Other(_) => Vec::new(),
        },

        ClaudeStreamMessage::StreamEvent(wrapper) => match &wrapper.event {
            StreamEventPayload::ContentBlockDelta { delta, .. } => match delta {
                StreamDelta::TextDelta { text } => {
                    vec![RuntimeEventKind::OutputDelta { text: text.clone() }]
                }
                StreamDelta::ThinkingDelta { .. } | StreamDelta::InputJsonDelta { .. } => {
                    Vec::new()
                }
                StreamDelta::Other(_) => Vec::new(),
            },
            StreamEventPayload::Other(_) => Vec::new(),
        },

        ClaudeStreamMessage::Assistant(assistant) => {
            let mut events = Vec::new();
            for block in &assistant.message.content {
                match block {
                    ContentBlock::ToolUse { id, name, .. } => {
                        subagents.register_tool_use(id.clone(), name.clone());
                        events.push(RuntimeEventKind::ToolStarted {
                            tool_name: name.clone(),
                            tool_call_id: id.clone(),
                        });
                    }
                    ContentBlock::Text { .. }
                    | ContentBlock::Thinking { .. }
                    | ContentBlock::Unknown(_) => {}
                }
            }

            if let Some(ref usage) = assistant.message.usage {
                let total = match (usage.input_tokens, usage.output_tokens) {
                    (Some(i), Some(o)) => Some(i + o),
                    (Some(i), None) => Some(i),
                    (None, Some(o)) => Some(o),
                    (None, None) => None,
                };
                events.push(RuntimeEventKind::UsageUpdated {
                    prompt_tokens: usage.input_tokens,
                    completion_tokens: usage.output_tokens,
                    total_tokens: total,
                });
            }

            events
        }

        ClaudeStreamMessage::User(user) => {
            let mut events = Vec::new();
            if let Some(ref inner) = user.message {
                for block in &inner.content {
                    match block {
                        ToolResultBlock::ToolResult {
                            tool_use_id,
                            is_error,
                            ..
                        } => {
                            let tool_name = subagents
                                .get_tool_name(tool_use_id)
                                .unwrap_or("tool")
                                .to_string();
                            let success = !is_error.unwrap_or(false);
                            events.push(RuntimeEventKind::ToolCompleted {
                                tool_name,
                                tool_call_id: tool_use_id.clone(),
                                duration_ms: 0,
                                success,
                            });
                        }
                        ToolResultBlock::Unknown(_) => {}
                    }
                }
            }
            events
        }

        ClaudeStreamMessage::ToolProgress(progress) => {
            vec![RuntimeEventKind::Diagnostic {
                level: "info".to_string(),
                message: SanitizedRuntimeMessage::new(format!(
                    "Tool '{}' running (elapsed: {:.1}s)",
                    progress
                        .tool_name
                        .as_deref()
                        .unwrap_or(&progress.tool_use_id),
                    progress.elapsed_time_seconds.unwrap_or(0.0)
                )),
            }]
        }

        ClaudeStreamMessage::ControlRequest(req) => match &req.request {
            ControlRequestPayload::CanUseTool(can_use) => {
                let approval_id = ApprovalId::from_uuid(Uuid::new_v5(
                    &Uuid::NAMESPACE_DNS,
                    req.request_id.as_bytes(),
                ));
                let desc = can_use.decision_reason.clone().unwrap_or_else(|| {
                    format!("Approval requested for tool '{}'", can_use.tool_name)
                });
                let command = can_use
                    .input
                    .get("command")
                    .and_then(|v| v.as_str())
                    .map(SanitizedRuntimeMessage::new);

                vec![RuntimeEventKind::ApprovalRequested {
                    approval_id,
                    description: SanitizedRuntimeMessage::new(desc),
                    command,
                }]
            }
            _ => Vec::new(),
        },

        ClaudeStreamMessage::Result(result) => {
            let mut events = Vec::new();
            if result.is_error {
                let error_summary = result
                    .errors
                    .as_ref()
                    .and_then(|e| e.first().cloned())
                    .or_else(|| result.result.clone())
                    .unwrap_or_else(|| "Execution terminated with error".to_string());

                events.push(RuntimeEventKind::Failed {
                    safe_error_summary: SanitizedRuntimeMessage::new(error_summary),
                });
            } else {
                events.push(RuntimeEventKind::Diagnostic {
                    level: "info".to_string(),
                    message: SanitizedRuntimeMessage::new(format!(
                        "Turn completed: {}",
                        result.result.as_deref().unwrap_or("success")
                    )),
                });
            }

            if let Some(ref usage) = result.usage {
                let total = match (usage.input_tokens, usage.output_tokens) {
                    (Some(i), Some(o)) => Some(i + o),
                    (Some(i), None) => Some(i),
                    (None, Some(o)) => Some(o),
                    (None, None) => None,
                };
                events.push(RuntimeEventKind::UsageUpdated {
                    prompt_tokens: usage.input_tokens,
                    completion_tokens: usage.output_tokens,
                    total_tokens: total,
                });
            }

            events
        }

        ClaudeStreamMessage::ControlResponse(_) => Vec::new(),

        ClaudeStreamMessage::PromptSuggestion(_) | ClaudeStreamMessage::Unknown(_) => Vec::new(),
    }
}
