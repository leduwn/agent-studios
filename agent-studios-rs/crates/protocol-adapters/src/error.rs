use thiserror::Error;

/// Errors produced during protocol translation between Codex Responses and Chat Completions.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChatAdapterError {
    #[error("Missing response ID in stream chunk")]
    MissingResponseId,

    #[error("Response ID mismatch: expected '{expected}', got '{actual}'")]
    ResponseIdMismatch { expected: String, actual: String },

    #[error("Response model mismatch: expected '{expected}', got '{actual}'")]
    ResponseModelMismatch { expected: String, actual: String },

    #[error("Stream is already completed")]
    AlreadyCompleted,

    #[error("Stream finished without observing a valid terminal finish reason")]
    MissingFinishReason,

    #[error(
        "Tool call at index {index} is incomplete (missing_id: {missing_id}, missing_name: {missing_name})"
    )]
    IncompleteToolCall {
        index: usize,
        missing_id: bool,
        missing_name: bool,
    },

    #[error("Tool call identity mismatch: {0}")]
    ToolCallIdentityMismatch(String),

    #[error("Unsupported tool call type in stream: {0}")]
    UnsupportedToolCallType(String),

    #[error("Unsupported content in tool output: {0}")]
    UnsupportedToolOutputContent(String),

    #[error("Encrypted AgentMessage is not supported in Chat Completions wire format")]
    UnsupportedEncryptedAgentMessage,

    #[error("Invalid message role: '{0}'")]
    InvalidRole(String),

    #[error("Unsupported tool choice: '{0}'")]
    UnsupportedToolChoice(String),

    #[error("Invalid token usage: {0}")]
    InvalidUsage(String),

    #[error("Invalid stream state: {0}")]
    InvalidStreamState(String),

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
    NormalizedImageDetail { from: String, to: String },
    DroppedReasoning(String),
    DroppedPromptCacheKey(String),
    DroppedInclude(Vec<String>),
    DroppedVerbosity(String),
    DroppedClientMetadata(Vec<String>),
    DroppedStore(bool),
    DroppedServiceTier(String),
    Other(String),
}
