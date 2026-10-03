use thiserror::Error;

/// Fail-closed errors emitted during Gemini generateContent request or stream translation.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum GeminiAdapterError {
    #[error("Unsupported conversational role '{0}' for Gemini generateContent")]
    UnsupportedRole(String),

    #[error("System or developer instructions cannot appear after conversational history: {0}")]
    UnsupportedSystemHistoryPlacement(String),

    #[error("Unsupported image MIME type: {0}")]
    UnsupportedImageMime(String),

    #[error("Unsupported audio MIME type: {0}")]
    UnsupportedAudioMime(String),

    #[error("Unsupported content in request item: {0}")]
    UnsupportedContent(String),

    #[error("Unsupported tool specification: {0}")]
    UnsupportedToolType(String),

    #[error("Unsupported tool_choice value: '{0}'")]
    UnsupportedToolChoice(String),

    #[error("Security feature '{0}' is not supported")]
    UnsupportedSecurityFeature(String),

    #[error(
        "Sequential tool calling (parallel_tool_calls = false) is not enforceable in Gemini generateContent"
    )]
    SequentialToolCallingNotEnforceable,

    #[error("Tool call arguments cannot be parsed as a valid JSON object: {0}")]
    InvalidToolArguments(String),

    #[error("Custom tool call input cannot be parsed as a valid JSON object: {0}")]
    UnsupportedCustomToolInput(String),

    #[error("Unsupported tool output content: {0}")]
    UnsupportedToolOutputContent(String),

    #[error("Could not find preceding tool call association for call_id '{0}'")]
    MissingToolCallAssociation(String),

    #[error("Missing Gemini continuation state required to reconstruct native model turn: {0}")]
    MissingContinuationState(String),

    #[error("Unsupported reasoning effort '{0}' in ExactReasoningEffort policy")]
    UnsupportedReasoningEffort(String),

    #[error("Invalid max_output_tokens: {0} (must be non-zero)")]
    InvalidMaxOutputTokens(u64),

    #[error("Response ID mismatch in streaming sequence: expected '{expected}', got '{actual}'")]
    ResponseIdMismatch { expected: String, actual: String },

    #[error("Missing responseId in first streaming chunk")]
    MissingResponseId,

    #[error("Model version changed unexpectedly mid-stream: expected '{expected}', got '{actual}'")]
    ModelVersionMismatch { expected: String, actual: String },

    #[error(
        "Multiple candidates returned by Gemini ({0}), but only single candidate (index 0) is supported"
    )]
    MultipleCandidatesUnsupported(usize),

    #[error("Prompt was blocked by Gemini safety or policy filters: {0}")]
    PromptBlocked(String),

    #[error("Generation was truncated due to MAX_TOKENS limit")]
    MaxTokensExceeded,

    #[error("Generation was blocked by safety filters: {0}")]
    SafetyBlocked(String),

    #[error("Generation was blocked by recitation check: {0}")]
    RecitationBlocked(String),

    #[error("Generation was blocked due to unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("Generation was blocked due to policy or content filter: {0}")]
    ContentBlocked(String),

    #[error("Model emitted a malformed function call: {0}")]
    MalformedFunctionCall(String),

    #[error("Model response is missing required thought signature: {0}")]
    MissingThoughtSignature(String),

    #[error("Model made an unexpected tool call: {0}")]
    UnexpectedToolCall(String),

    #[error("Model exceeded tool call count limit: {0}")]
    TooManyToolCalls(String),

    #[error("Model emitted a malformed response: {0}")]
    MalformedResponse(String),

    #[error("Image generation was blocked: {0}")]
    ImageGenerationBlocked(String),

    #[error("Finish reason was unspecified")]
    FinishReasonUnspecified,

    #[error("Encountered unknown or unhandled finish reason: '{0}'")]
    UnknownFinishReason(String),

    #[error("Stream translator has already completed")]
    AlreadyCompleted,

    #[error("Cannot finish stream before receiving a valid terminal finishReason")]
    StreamMissingFinishReason,

    #[error("Received content event before response was initialized")]
    StreamMessageNotStarted,

    #[error("Malformed SSE event line: {0}")]
    MalformedSse(String),

    #[error("Invalid usage metadata: {0}")]
    InvalidUsage(String),
}

/// Typed non-fatal warnings emitted when protocol semantics differ.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum GeminiAdapterWarning {
    /// Emitted when mixed strict and non-strict tools caused the entire tool set to be promoted to VALIDATED.
    StrictToolScopePromoted,
    /// Structured output name was ignored because Gemini schema format does not take a name parameter.
    StructuredOutputNameIgnored(String),
    /// Emitted when Codex strict=false structured output is schema-constrained by Gemini.
    StructuredOutputSchemaConstrained,
    /// Granularity in ReasoningSummary (Concise/Detailed) cannot be represented in Gemini includeThoughts.
    ReasoningSummaryGranularityNotRepresentable(String),
    /// Codex ReasoningContext cannot be represented directly in generateContent wire format.
    ReasoningContextNotDirectlyRepresentable(String),
    /// Codex prompt_cache_key is not directly equivalent to Gemini cachedContent.
    PromptCacheKeyNotSupported(String),
    /// Client metadata cannot be represented in generateContent request body.
    ClientMetadataNotSupported,
    /// Codex store field is not mapped to Gemini request.
    StoreFieldNotSupported,
    /// Service tier field is not mapped to Gemini request.
    ServiceTierNotSupported(String),
    /// Codex include field is not mapped to Gemini request.
    IncludeFieldNotSupported(String),
    /// Text verbosity is not directly representable.
    VerbosityNotSupported(String),
    /// Provider-native reasoning or signature from another provider was omitted.
    CrossProviderReasoningOmitted(String),
}
