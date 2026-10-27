use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Top-level wire message emitted by Claude Code over NDJSON stdout or received on stdin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClaudeStreamMessage {
    #[serde(rename = "system")]
    System(SystemMessage),
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),
    #[serde(rename = "user")]
    User(UserMessage),
    #[serde(rename = "stream_event")]
    StreamEvent(StreamEventWrapper),
    #[serde(rename = "tool_progress")]
    ToolProgress(ToolProgressMessage),
    #[serde(rename = "result")]
    Result(ResultSummary),
    #[serde(rename = "control_request")]
    ControlRequest(ControlRequest),
    #[serde(rename = "control_response")]
    ControlResponse(ControlResponse),
    #[serde(rename = "prompt_suggestion")]
    PromptSuggestion(serde_json::Value),
    #[serde(rename = "command_lifecycle")]
    CommandLifecycle(serde_json::Value),
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

/// System event categories emitted by Claude Code.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "subtype")]
pub enum SystemMessage {
    #[serde(rename = "init")]
    Init(SystemInitMessage),
    #[serde(rename = "task_started")]
    TaskStarted(TaskStartedMessage),
    #[serde(rename = "task_notification")]
    TaskNotification(TaskNotificationMessage),
    #[serde(rename = "api_retry")]
    ApiRetry(ApiRetryMessage),
    #[serde(rename = "status")]
    Status(SystemStatusMessage),
    #[serde(rename = "ui_invalidate")]
    UiInvalidate(UiInvalidateMessage),
    #[serde(rename = "thinking_tokens")]
    ThinkingTokens(ThinkingTokensMessage),
    #[serde(untagged)]
    Other(serde_json::Value),
}

/// Emitted by Claude Code when UI needs re-rendering.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct UiInvalidateMessage {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
}

/// Emitted during reasoning with estimated thinking token count.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ThinkingTokensMessage {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default, alias = "estimatedTokens")]
    pub estimated_tokens: Option<u64>,
}

/// Process startup event emitted once when Claude Code initializes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct SystemInitMessage {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub claude_code_version: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default)]
    pub mcp_servers: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub plugins: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub capabilities: Option<Vec<String>>,
    #[serde(default)]
    pub output_style: Option<String>,
    #[serde(default)]
    pub fast_mode_state: Option<String>,
    #[serde(default, alias = "permissionMode", alias = "permission_mode")]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub slash_commands: Option<Vec<String>>,
    #[serde(default)]
    pub agents: Option<Vec<String>>,
    #[serde(default)]
    pub skills: Option<Vec<String>>,
    #[serde(default)]
    pub user_message_uuids: Option<Vec<String>>,
}

/// Emitted when a background task or subagent is spawned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskStartedMessage {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub task_id: String,
    pub tool_use_id: String,
    pub description: String,
    #[serde(default)]
    pub task_type: Option<String>,
    #[serde(default)]
    pub is_backgrounded: Option<bool>,
    #[serde(default)]
    pub spawn_depth: Option<u32>,
}

/// Emitted when a subagent or background task finishes execution.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskNotificationMessage {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub task_id: String,
    pub status: String,
    #[serde(default)]
    pub output_file: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub usage: Option<ClaudeTaskUsage>,
}

/// Resource usage statistics for a subagent or background task.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ClaudeTaskUsage {
    #[serde(default)]
    pub total_tokens: Option<u64>,
    #[serde(default)]
    pub tool_uses: Option<u64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

/// Emitted when an API retry occurs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ApiRetryMessage {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub attempt: Option<u32>,
    #[serde(default)]
    pub max_retries: Option<u32>,
    #[serde(default)]
    pub retry_delay_ms: Option<u64>,
    #[serde(default)]
    pub error_status: Option<u16>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Emitted when status changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct SystemStatusMessage {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

/// Completed model turn or assistant blocks emitted by Claude Code.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssistantMessage {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub user_message_uuid: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub user_message_uuids: Option<Vec<String>>,
    pub message: AssistantMessageInner,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct AssistantMessageInner {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub usage: Option<ClaudeTokenUsage>,
}

/// Content blocks inside an assistant message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(default)]
        signature: Option<String>,
    },
    #[serde(rename = "redacted_thinking")]
    RedactedThinking {
        #[serde(default)]
        data: Option<String>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

/// User message envelope (used for tool results or replayed user input).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct UserMessage {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub message: Option<UserMessageInner>,
    #[serde(default)]
    pub tool_use_result: Option<serde_json::Value>,
    #[serde(default, alias = "isReplay")]
    pub is_replay: Option<bool>,
}

/// Content payload for a user message, either raw text (replayed user input) or blocks (tool results).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserMessageContent {
    Text(String),
    Blocks(Vec<ToolResultBlock>),
}

impl Default for UserMessageContent {
    fn default() -> Self {
        UserMessageContent::Blocks(Vec::new())
    }
}

impl From<Vec<ToolResultBlock>> for UserMessageContent {
    fn from(blocks: Vec<ToolResultBlock>) -> Self {
        UserMessageContent::Blocks(blocks)
    }
}

impl From<String> for UserMessageContent {
    fn from(text: String) -> Self {
        UserMessageContent::Text(text)
    }
}

impl From<&str> for UserMessageContent {
    fn from(text: &str) -> Self {
        UserMessageContent::Text(text.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct UserMessageInner {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: UserMessageContent,
}

impl UserMessageInner {
    pub fn new_text(role: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            role: Some(role.into()),
            content: UserMessageContent::Text(text.into()),
        }
    }

    pub fn new_blocks(role: impl Into<String>, blocks: Vec<ToolResultBlock>) -> Self {
        Self {
            role: Some(role.into()),
            content: UserMessageContent::Blocks(blocks),
        }
    }

    pub fn blocks(&self) -> &[ToolResultBlock] {
        match &self.content {
            UserMessageContent::Blocks(blocks) => blocks.as_slice(),
            UserMessageContent::Text(_) => &[],
        }
    }

    pub fn text(&self) -> Option<&str> {
        match &self.content {
            UserMessageContent::Text(text) => Some(text.as_str()),
            UserMessageContent::Blocks(_) => None,
        }
    }
}

/// Tool execution result block inside a user message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolResultBlock {
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        #[serde(default)]
        content: serde_json::Value,
        #[serde(default)]
        is_error: Option<bool>,
    },
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

/// Token-level streaming chunk wrapper emitted when `--include-partial-messages` is active.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StreamEventWrapper {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub user_message_uuid: Option<String>,
    pub event: StreamEventPayload,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEventPayload {
    #[serde(rename = "message_start")]
    MessageStart {
        #[serde(default)]
        message: Option<serde_json::Value>,
    },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: usize,
        #[serde(default)]
        content_block: Option<serde_json::Value>,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: usize, delta: StreamDelta },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },
    #[serde(rename = "message_delta")]
    MessageDelta {
        #[serde(default)]
        delta: Option<serde_json::Value>,
        #[serde(default)]
        usage: Option<ClaudeTokenUsage>,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(untagged)]
    Other(serde_json::Value),
}

/// Delta variants for incremental streaming.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
    #[serde(rename = "signature_delta")]
    SignatureDelta { signature: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
    #[serde(untagged)]
    Other(serde_json::Value),
}

/// Tool execution progress notification.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolProgressMessage {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub tool_use_id: String,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_f64")]
    pub elapsed_time_seconds: Option<f64>,
    #[serde(default)]
    pub task_id: Option<String>,
}

/// Helper deserializer for optional floats resilient to `serde_json/arbitrary_precision`.
pub(crate) fn deserialize_opt_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = Option::<serde_json::Value>::deserialize(deserializer)?;
    match val {
        Some(serde_json::Value::Number(n)) => Ok(n.as_f64()),
        Some(serde_json::Value::String(s)) => {
            s.parse::<f64>().map(Some).map_err(serde::de::Error::custom)
        }
        Some(serde_json::Value::Null) | None => Ok(None),
        _ => Ok(None),
    }
}

/// Final summary emitted at the end of each execution turn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ResultSummary {
    #[serde(default)]
    pub subtype: Option<String>,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub duration_api_ms: Option<u64>,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub errors: Option<Vec<String>>,
    #[serde(default)]
    pub num_turns: Option<u32>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_f64")]
    pub total_cost_usd: Option<f64>,
    #[serde(default)]
    pub usage: Option<ClaudeTokenUsage>,
    #[serde(default, rename = "modelUsage")]
    pub model_usage: Option<BTreeMap<String, ClaudeModelCost>>,
    #[serde(default)]
    pub permission_denials: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub terminal_reason: Option<String>,
    #[serde(default)]
    pub subagent_stats: Option<serde_json::Value>,
}

/// Token usage accounting across turns or runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ClaudeTokenUsage {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u64>,
}

/// Per-model token usage and spend summary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ClaudeModelCost {
    #[serde(default, alias = "inputTokens")]
    pub input_tokens: Option<u64>,
    #[serde(default, alias = "outputTokens")]
    pub output_tokens: Option<u64>,
    #[serde(default, alias = "costUSD", deserialize_with = "deserialize_opt_f64")]
    pub cost_usd: Option<f64>,
}

/// Control plane request emitted by Claude Code (e.g. tool approval).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlRequest {
    pub request_id: String,
    pub request: ControlRequestPayload,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "subtype")]
pub enum ControlRequestPayload {
    #[serde(rename = "can_use_tool")]
    CanUseTool(CanUseToolRequest),
    #[serde(rename = "interrupt")]
    Interrupt {
        #[serde(default)]
        cancel_queued: Option<bool>,
    },
    #[serde(untagged)]
    Other(serde_json::Value),
}

/// Tool authorization request payload emitted under `--permission-prompts=host`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanUseToolRequest {
    pub tool_name: String,
    pub input: serde_json::Value,
    #[serde(default)]
    pub tool_use_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub decision_reason: Option<String>,
    #[serde(default)]
    pub blocked_path: Option<String>,
    #[serde(default)]
    pub default_to_no: Option<bool>,
    #[serde(default)]
    pub suggestions: Option<Vec<serde_json::Value>>,
}

/// Host control response sent on stdin to answer a `ControlRequest` or received over wire.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlResponse {
    #[serde(default = "default_control_response_type")]
    pub r#type: String,
    pub response: ControlResponseBody,
}

fn default_control_response_type() -> String {
    "control_response".to_string()
}

/// Body of a host control response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "subtype", rename_all = "lowercase")]
pub enum ControlResponseBody {
    #[serde(rename = "success")]
    Success {
        request_id: String,
        response: ControlDecision,
    },
    #[serde(rename = "error")]
    Error { request_id: String, error: String },
}

impl ControlResponse {
    /// Creates an allow decision response.
    pub fn allow(request_id: impl Into<String>, updated_input: Option<serde_json::Value>) -> Self {
        Self {
            r#type: default_control_response_type(),
            response: ControlResponseBody::Success {
                request_id: request_id.into(),
                response: ControlDecision::Allow { updated_input },
            },
        }
    }

    /// Creates a deny decision response.
    pub fn deny(
        request_id: impl Into<String>,
        message: Option<String>,
        interrupt: Option<bool>,
    ) -> Self {
        Self {
            r#type: default_control_response_type(),
            response: ControlResponseBody::Success {
                request_id: request_id.into(),
                response: ControlDecision::Deny { message, interrupt },
            },
        }
    }

    /// Creates an error response.
    pub fn error(request_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            r#type: default_control_response_type(),
            response: ControlResponseBody::Error {
                request_id: request_id.into(),
                error: error.into(),
            },
        }
    }

    /// Returns the request ID associated with this control response.
    pub fn request_id(&self) -> &str {
        match &self.response {
            ControlResponseBody::Success { request_id, .. } => request_id,
            ControlResponseBody::Error { request_id, .. } => request_id,
        }
    }

    /// Returns true if this is a success response subtype.
    pub fn is_success(&self) -> bool {
        matches!(self.response, ControlResponseBody::Success { .. })
    }

    /// Returns true if this is an error response subtype.
    pub fn is_error(&self) -> bool {
        matches!(self.response, ControlResponseBody::Error { .. })
    }
}

/// Operator decision for tool execution authorization.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "behavior", rename_all = "lowercase")]
pub enum ControlDecision {
    Allow {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "updatedInput"
        )]
        updated_input: Option<serde_json::Value>,
    },
    Deny {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        interrupt: Option<bool>,
    },
}

/// Prompt submission payload written to child stdin in stream-json mode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaudeUserPromptInput {
    pub r#type: String,
    pub uuid: String,
    pub session_id: String,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    pub message: ClaudeStdinUserMessage,
    pub priority: String,
    pub origin: ClaudeInputOrigin,
    #[serde(rename = "shouldQuery")]
    pub should_query: bool,
    pub client_composed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaudeStdinUserMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaudeInputOrigin {
    pub kind: String,
}

impl ClaudeUserPromptInput {
    pub fn new(session_id: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            r#type: "user".to_string(),
            uuid: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            parent_tool_use_id: None,
            message: ClaudeStdinUserMessage {
                role: "user".to_string(),
                content: prompt.into(),
            },
            priority: "now".to_string(),
            origin: ClaudeInputOrigin {
                kind: "human".to_string(),
            },
            should_query: true,
            client_composed: true,
        }
    }
}

/// Host interrupt request written to child stdin to abort current turn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaudeInterruptInput {
    pub r#type: String,
    pub request_id: String,
    pub request: ClaudeInterruptPayload,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClaudeInterruptPayload {
    pub subtype: String,
    pub cancel_queued: bool,
}

impl ClaudeInterruptInput {
    pub fn new(request_id: impl Into<String>) -> Self {
        Self {
            r#type: "control_request".to_string(),
            request_id: request_id.into(),
            request: ClaudeInterruptPayload {
                subtype: "interrupt".to_string(),
                cancel_queued: true,
            },
        }
    }
}
