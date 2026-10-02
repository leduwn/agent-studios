use serde::{Deserialize, Serialize};

use super::error::AnthropicStreamApiError;

/// Root request payload for Anthropic Messages API (`POST /v1/messages`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnthropicMessagesRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    pub max_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Vec<AnthropicSystemBlock>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<AnthropicToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<AnthropicThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<AnthropicOutputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AnthropicMetadata>,
}

/// System prompt block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicSystemBlock {
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<AnthropicCacheControl>,
    },
}

/// Message in the conversation history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: Vec<AnthropicContentBlock>,
}

/// Content block variants inside a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicContentBlock {
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<AnthropicCacheControl>,
    },
    Image {
        source: AnthropicImageSource,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<AnthropicCacheControl>,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<AnthropicCacheControl>,
    },
    ToolResult {
        tool_use_id: String,
        content: AnthropicToolResultContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<AnthropicCacheControl>,
    },
    Thinking {
        thinking: String,
        signature: String,
    },
    RedactedThinking {
        data: String,
    },
}

/// Content for a tool_result block: text string or structured content items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicToolResultContent {
    Text(String),
    Blocks(Vec<AnthropicToolResultBlock>),
}

/// Structured content item inside tool_result content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicToolResultBlock {
    Text { text: String },
    Image { source: AnthropicImageSource },
}

/// Image source payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnthropicImageSource {
    pub r#type: String, // "base64"
    pub media_type: String,
    pub data: String,
}

/// Client tool definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnthropicTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<AnthropicCacheControl>,
}

/// Tool choice selection configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicToolChoice {
    Auto {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    Any {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    Tool {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
}

/// Thinking configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicThinkingConfig {
    Adaptive,
    Enabled { budget_tokens: u64 },
}

/// Output configuration (effort, structured output format).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnthropicOutputConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<AnthropicOutputFormat>,
}

/// Structured output schema configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicOutputFormat {
    JsonSchema { schema: serde_json::Value },
}

/// Cache control configuration for prompt caching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicCacheControl {
    Ephemeral {
        #[serde(skip_serializing_if = "Option::is_none")]
        ttl: Option<String>,
    },
}

/// Request metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnthropicMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

/// Token usage reported by Anthropic.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnthropicUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<i64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<i64>,
}

/// Incoming SSE stream event representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicStreamEvent {
    MessageStart {
        message: AnthropicStreamMessage,
    },
    ContentBlockStart {
        index: u64,
        content_block: AnthropicStreamBlockStart,
    },
    ContentBlockDelta {
        index: u64,
        delta: AnthropicStreamBlockDelta,
    },
    ContentBlockStop {
        index: u64,
    },
    MessageDelta {
        delta: AnthropicStreamMessageDelta,
        #[serde(default)]
        usage: Option<AnthropicStreamDeltaUsage>,
    },
    MessageStop,
    Ping,
    Error {
        error: AnthropicStreamApiError,
    },
}

/// Initial message descriptor in message_start stream event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnthropicStreamMessage {
    pub id: String,
    pub r#type: String,
    pub role: String,
    pub content: Vec<AnthropicContentBlock>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub usage: Option<AnthropicUsage>,
}

/// Starting descriptor of a content block in a stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicStreamBlockStart {
    Text { text: String },
    ToolUse { id: String, name: String },
    Thinking { thinking: String },
    RedactedThinking { data: String },
}

/// Incremental delta inside a content block stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicStreamBlockDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
    ThinkingDelta { thinking: String },
    SignatureDelta { signature: String },
}

/// Message-level delta in stream.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnthropicStreamMessageDelta {
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
}

/// Usage delta reported in message_delta.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnthropicStreamDeltaUsage {
    pub output_tokens: Option<i64>,
}
