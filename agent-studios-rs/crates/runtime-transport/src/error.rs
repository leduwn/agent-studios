use agent_studios_provider::id::ProviderInstanceId;
use codex_api::ApiError;
use thiserror::Error;

/// Core error type for runtime provider transport and protocol execution.
#[derive(Debug, Error)]
pub enum TransportError {
    #[error(
        "Concurrent inference request rejected for thread '{thread_id}' on provider instance '{provider_instance_id}'"
    )]
    ConcurrentThreadInference {
        provider_instance_id: ProviderInstanceId,
        thread_id: String,
    },

    #[error("Missing required model limit '{limit}' for model '{model}'")]
    MissingRequiredModelLimit { model: String, limit: &'static str },

    #[error("Secret reference locator '{locator}' resolved to an empty secret")]
    EmptySecret { locator: String },

    #[error("Unsupported secret backend '{backend}'")]
    UnsupportedSecretBackend { backend: String },

    #[error("Authentication collision: '{name}' already present in {location}")]
    AuthenticationCollision {
        name: String,
        location: &'static str,
    },

    #[error("SSE frame too large: {size} bytes exceeds limit of {max_bytes} bytes")]
    SseFrameTooLarge { size: usize, max_bytes: usize },

    #[error(
        "Insecure remote HTTP endpoint rejected: '{url}' (HTTPS required or allow_insecure_remote_http must be set)"
    )]
    InsecureRemoteHttpRejected { url: String },

    #[error("Unsupported model capability '{feature}' requested for model '{model}'")]
    UnsupportedCapability {
        feature: &'static str,
        model: String,
    },

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Network connection error: {0}")]
    Network(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Secret not found for backend '{backend}' at locator '{locator}'")]
    SecretNotFound { backend: String, locator: String },

    #[error("Unsupported protocol: {0}")]
    UnsupportedProtocol(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Stream decode error: {0}")]
    StreamDecode(String),

    #[error("Stream was cancelled")]
    StreamCancelled,

    #[error("Continuation state error: {0}")]
    Continuation(String),

    #[error("Invalid endpoint URL: {0}")]
    InvalidEndpoint(String),

    #[error("Provider not found: {0}")]
    ProviderNotFound(String),
}

/// Sanitizes an error message by redacting credential query parameters and secret tokens.
pub fn sanitize_error_message(msg: &str) -> String {
    let mut result = String::with_capacity(msg.len());
    let mut remaining = msg;

    while let Some(pos) = remaining.find("key=") {
        let (before, after) = remaining.split_at(pos);
        result.push_str(before);
        result.push_str("key=[REDACTED]");
        let val_start = &after["key=".len()..];
        let end_idx = val_start
            .find(['&', ' ', ')', '"', '\'', '>'])
            .unwrap_or(val_start.len());
        remaining = &val_start[end_idx..];
    }
    result.push_str(remaining);
    result
}

impl From<TransportError> for ApiError {
    fn from(err: TransportError) -> Self {
        ApiError::Stream(err.to_string())
    }
}

impl From<agent_studios_protocol_adapters::ChatAdapterError> for TransportError {
    fn from(err: agent_studios_protocol_adapters::ChatAdapterError) -> Self {
        Self::Protocol(err.to_string())
    }
}

impl From<agent_studios_protocol_adapters::anthropic::AnthropicAdapterError> for TransportError {
    fn from(err: agent_studios_protocol_adapters::anthropic::AnthropicAdapterError) -> Self {
        Self::Protocol(err.to_string())
    }
}

impl From<agent_studios_protocol_adapters::gemini::GeminiAdapterError> for TransportError {
    fn from(err: agent_studios_protocol_adapters::gemini::GeminiAdapterError) -> Self {
        Self::Protocol(err.to_string())
    }
}

impl From<reqwest::Error> for TransportError {
    fn from(err: reqwest::Error) -> Self {
        let sanitized = sanitize_error_message(&err.to_string());
        if err.is_connect() || err.is_timeout() {
            Self::Network(sanitized)
        } else {
            Self::Http(sanitized)
        }
    }
}
