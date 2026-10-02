use thiserror::Error;

/// Errors produced during protocol translation between Codex Responses and Chat Completions.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChatAdapterError {
    #[error("Security-sensitive feature rejected: {0}")]
    UnsupportedSecurityFeature(String),

    #[error("Unsupported content in message: {0}")]
    UnsupportedContent(String),

    #[error("Unsupported response item: {0}")]
    UnsupportedResponseItem(String),

    #[error("Missing tool call ID: {0}")]
    MissingToolCallId(String),

    #[error("Unsupported tool type: {0}")]
    UnsupportedToolType(String),

    #[error("Invalid tool definition: {0}")]
    InvalidToolDefinition(String),

    #[error("Invalid JSON payload: {0}")]
    InvalidJson(String),

    #[error("Single choice constraint violated: choice index {0} > 0")]
    MultipleChoicesUnsupported(u32),

    #[error("Stream incomplete: context length or max_tokens exceeded ({0})")]
    StreamIncomplete(String),

    #[error("Content filter triggered: {0}")]
    ContentFilterTriggered(String),

    #[error("Invalid stream chunk: {0}")]
    InvalidStreamChunk(String),

    #[error("Unexpected finish reason: {0}")]
    UnexpectedFinishReason(String),

    #[error("Stream state error: {0}")]
    StateError(String),
}

/// Typed warnings for dropped or normalized Responses API features that have no direct
/// representation in OpenAI Chat wire format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatAdapterWarning {
    DroppedReasoning(String),
    DroppedPromptCacheKey(String),
    DroppedInclude(Vec<String>),
    DroppedVerbosity(String),
    DroppedClientMetadata(Vec<String>),
    DroppedStore(bool),
    DroppedServiceTier(String),
    Other(String),
}
