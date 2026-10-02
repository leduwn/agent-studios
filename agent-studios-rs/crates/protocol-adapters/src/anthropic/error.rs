use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stream API error returned by Anthropic in stream error events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnthropicStreamApiError {
    pub r#type: String,
    pub message: String,
}

impl std::fmt::Display for AnthropicStreamApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.r#type, self.message)
    }
}

/// Errors occurring during Anthropic Messages protocol translation.
#[derive(Debug, Error, PartialEq)]
pub enum AnthropicAdapterError {
    #[error("max_tokens must be non-zero (received: {0})")]
    InvalidMaxTokens(u64),

    #[error("Unsupported content in message: {0}")]
    UnsupportedContent(String),

    #[error(
        "Unsupported image MIME type: {0} (supported: image/jpeg, image/png, image/gif, image/webp)"
    )]
    UnsupportedImageMime(String),

    #[error("Unsupported security feature: {0}")]
    UnsupportedSecurityFeature(String),

    #[error("Unsupported tool definition type: {0}")]
    UnsupportedToolType(String),

    #[error("Unsupported tool choice: {0}")]
    UnsupportedToolChoice(String),

    #[error("Unsupported system/developer message placement: {0}")]
    UnsupportedSystemHistoryPlacement(String),

    #[error("Unsupported reasoning effort value: {0}")]
    UnsupportedReasoningEffort(String),

    #[error("Unsupported response item: {0}")]
    UnsupportedResponseItem(String),

    #[error("Unsupported tool output content: {0}")]
    UnsupportedToolOutputContent(String),

    #[error("Invalid message role: {0}")]
    InvalidRole(String),

    #[error("Missing tool_call_id for tool output item: {0}")]
    MissingToolCallId(String),

    #[error("Missing native continuation state for Anthropic-origin reasoning item: {0}")]
    MissingContinuationState(String),

    #[error("Invalid function call arguments JSON: {0}")]
    InvalidToolArguments(String),

    #[error("Stream operation rejected because stream is already completed")]
    StreamAlreadyCompleted,

    #[error("Stream event received before message_start")]
    StreamMessageNotStarted,

    #[error("Duplicate message_start event received")]
    StreamMessageAlreadyStarted,

    #[error("Content block delta/stop received for unstarted block index {0}")]
    StreamBlockNotStarted(u64),

    #[error("Duplicate content_block_start received for block index {0}")]
    StreamBlockAlreadyStarted(u64),

    #[error("Invalid stream state transition: {0}")]
    StreamInvalidState(String),

    #[error("message_stop received while content blocks are still open: indices {0:?}")]
    StreamOpenBlocksOnStop(Vec<u64>),

    #[error("Missing message ID in message_start event")]
    StreamMissingMessageId,

    #[error("Stream message ID mismatch: expected '{expected}', found '{found}'")]
    StreamResponseIdMismatch { expected: String, found: String },

    #[error("Stream model mismatch: expected '{expected}', found '{found}'")]
    StreamModelMismatch { expected: String, found: String },

    #[error("Stream finished without a valid terminal stop_reason")]
    StreamMissingStopReason,

    #[error("Stream stopped because max_tokens was exceeded (truncation)")]
    MaxTokensExceeded,

    #[error("Stream stopped because model context window was exceeded")]
    ContextWindowExceeded,

    #[error("Model refused generation: {0}")]
    ModelRefusal(String),

    #[error("Model generation paused: {0}")]
    TurnPaused(String),

    #[error("Unknown or unsupported stop reason: {0}")]
    UnknownStopReason(String),

    #[error("Invalid token usage: {0}")]
    InvalidUsage(String),

    #[error("Anthropic stream API error: {0}")]
    StreamApiError(AnthropicStreamApiError),

    #[error("Malformed SSE data: {0}")]
    MalformedSse(String),
}

/// Warnings emitted during Anthropic Messages protocol translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnthropicAdapterWarning {
    PromptCacheKeyNotDirectlyRepresentable(String),
    DroppedStore(bool),
    DroppedServiceTier(String),
    DroppedInclude(Vec<String>),
    DroppedClientMetadata(Vec<String>),
    DroppedStreamOptions(String),
    DroppedVerbosity(String),
    DroppedFormatMetadata(String),
    ForcedToolChoiceUnverified(String),
    CrossProviderReasoningOmitted(String),
}
