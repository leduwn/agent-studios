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

    #[error("No route configured for model '{model}'")]
    ModelRouteNotFound { model: String },

    #[error("Invalid model route: {0}")]
    InvalidModelRoute(String),

    #[error(
        "Duplicate route registration for model '{model}': already registered to instance '{existing_instance_id}'"
    )]
    DuplicateModelRoute {
        model: String,
        existing_instance_id: ProviderInstanceId,
    },

    #[error("Request headers timeout exceeded")]
    RequestHeadersTimeout,

    #[error("Stream idle timeout exceeded: no SSE events received within configured window")]
    StreamIdleTimeout,

    #[error("Reserved query parameter '{parameter}' collision: controlled by runtime transport")]
    ReservedQueryParameterCollision { parameter: String },

    #[error("Failed to construct HTTP client: {0}")]
    HttpClientBuildError(String),
}

/// Sanitizes an error message by redacting credential query parameters and secret tokens.
pub fn sanitize_error_message(msg: &str) -> String {
    let mut result = msg.to_string();

    // Strip "for url (...)" if present to avoid leaking full request URLs
    while let Some(start) = result.find("for url (") {
        if let Some(end) = result[start..].find(')') {
            result.replace_range(start..=start + end, "");
        } else {
            break;
        }
    }

    // Strip generic query params with credentials
    let sensitive_keys = [
        "key=",
        "token=",
        "access_token=",
        "secret=",
        "password=",
        "auth=",
        "api_key=",
        "apikey=",
    ];

    for key_pattern in sensitive_keys {
        let mut search_from = 0;
        while let Some(rel_pos) = result[search_from..].find(key_pattern) {
            let pos = search_from + rel_pos;
            let val_start = pos + key_pattern.len();
            if result[val_start..].starts_with("[REDACTED]") {
                search_from = val_start + "[REDACTED]".len();
                continue;
            }
            let end_idx = result[val_start..]
                .find(['&', ' ', ')', '"', '\'', '>', '\n'])
                .map(|i| val_start + i)
                .unwrap_or(result.len());
            let replacement = format!("{key_pattern}[REDACTED]");
            result.replace_range(pos..end_idx, &replacement);
            search_from = pos + replacement.len();
        }
    }

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
        let err = err.without_url();
        let is_connect = err.is_connect();
        let is_timeout = err.is_timeout();
        let sanitized = sanitize_error_message(&err.to_string());
        if is_connect || is_timeout {
            Self::Network(sanitized)
        } else {
            Self::Http(sanitized)
        }
    }
}
