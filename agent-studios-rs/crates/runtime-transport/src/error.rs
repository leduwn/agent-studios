use codex_api::ApiError;
use thiserror::Error;

/// Core error type for runtime provider transport and protocol execution.
#[derive(Debug, Error)]
pub enum TransportError {
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
        if err.is_connect() || err.is_timeout() {
            Self::Network(err.to_string())
        } else {
            Self::Http(err.to_string())
        }
    }
}
