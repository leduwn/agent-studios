use serde::{Deserialize, Serialize};

use crate::capabilities::RuntimeCapability;
use crate::id::{RuntimeImplementationId, RuntimeSessionId};
use crate::lifecycle::RuntimeLifecycleState;

/// Strips common secret patterns (API keys, bearer tokens, passwords) from diagnostic strings.
pub fn sanitize_error_message(input: &str) -> String {
    let mut sanitized = input.to_string();

    // Redact sk-ant-* Anthropic API keys directly
    while let Some(idx) = sanitized.find("sk-ant-") {
        let after = &sanitized[idx..];
        let token_bytes = after
            .char_indices()
            .find(|(_, c)| c.is_whitespace() || *c == ';' || *c == '&' || *c == '"' || *c == '\'')
            .map(|(i, _)| i)
            .unwrap_or(after.len());
        if token_bytes > 0 {
            sanitized.replace_range(idx..idx + token_bytes, "[REDACTED_API_KEY]");
        } else {
            break;
        }
    }

    // Redact ghp_* GitHub tokens directly
    while let Some(idx) = sanitized.find("ghp_") {
        let after = &sanitized[idx..];
        let token_bytes = after
            .char_indices()
            .find(|(_, c)| c.is_whitespace() || *c == ';' || *c == '&' || *c == '"' || *c == '\'')
            .map(|(i, _)| i)
            .unwrap_or(after.len());
        if token_bytes > 0 {
            sanitized.replace_range(idx..idx + token_bytes, "[REDACTED_GITHUB_TOKEN]");
        } else {
            break;
        }
    }

    // Redact Bearer tokens
    while let Some(idx) = sanitized.to_lowercase().find("bearer ") {
        let after = &sanitized[idx + 7..];
        if after.starts_with("[REDACTED") {
            break;
        }
        let token_bytes = after
            .char_indices()
            .find(|(_, c)| c.is_whitespace() || *c == ';' || *c == '&' || *c == '"' || *c == '\'')
            .map(|(i, _)| i)
            .unwrap_or(after.len());
        if token_bytes > 0 {
            sanitized.replace_range(idx..idx + 7 + token_bytes, "[REDACTED_BEARER_TOKEN]");
        } else {
            break;
        }
    }

    // Redact api_key / apikey / x-api-key patterns
    for prefix in &["api_key=", "api-key=", "apikey=", "api_key:", "x-api-key:"] {
        while let Some(idx) = sanitized.to_lowercase().find(prefix) {
            let val_start = idx + prefix.len();
            let after = &sanitized[val_start..];
            if after.starts_with("[REDACTED") {
                break;
            }
            let val_bytes = after
                .char_indices()
                .find(|(_, c)| {
                    c.is_whitespace() || *c == ';' || *c == '&' || *c == '"' || *c == '\''
                })
                .map(|(i, _)| i)
                .unwrap_or(after.len());
            if val_bytes > 0 {
                sanitized.replace_range(idx..val_start + val_bytes, "[REDACTED_API_KEY]");
            } else {
                break;
            }
        }
    }

    // Redact secret= patterns
    for prefix in &["secret=", "secret:", "password="] {
        while let Some(idx) = sanitized.to_lowercase().find(prefix) {
            let val_start = idx + prefix.len();
            let after = &sanitized[val_start..];
            if after.starts_with("[REDACTED") {
                break;
            }
            let val_bytes = after
                .char_indices()
                .find(|(_, c)| {
                    c.is_whitespace() || *c == ';' || *c == '&' || *c == '"' || *c == '\''
                })
                .map(|(i, _)| i)
                .unwrap_or(after.len());
            if val_bytes > 0 {
                sanitized.replace_range(idx..val_start + val_bytes, "[REDACTED_SECRET]");
            } else {
                break;
            }
        }
    }

    sanitized
}

/// Strongly-typed errors emitted by the external agent runtime subsystem.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeError {
    #[error("Runtime implementation not found: {id}")]
    RuntimeNotFound { id: String },

    #[error("Runtime implementation unavailable: {implementation_id} ({reason})")]
    RuntimeUnavailable {
        implementation_id: RuntimeImplementationId,
        reason: String,
    },

    #[error("Runtime discovery failed: {reason}")]
    DiscoveryFailed { reason: String },

    #[error("Unsupported runtime capability: {capability} ({reason})")]
    UnsupportedCapability {
        capability: RuntimeCapability,
        reason: String,
    },

    #[error("Invalid lifecycle transition from {from} to {to}: {reason}")]
    InvalidLifecycleTransition {
        from: RuntimeLifecycleState,
        to: RuntimeLifecycleState,
        reason: String,
    },

    #[error("Runtime session not found: {session_id}")]
    SessionNotFound { session_id: RuntimeSessionId },

    #[error("Runtime session startup failed: {reason}")]
    StartupFailed { reason: String },

    #[error("Runtime send failed for session {session_id}: {reason}")]
    SendFailed {
        session_id: RuntimeSessionId,
        reason: String,
    },

    #[error("Runtime interruption failed for session {session_id}: {reason}")]
    InterruptionFailed {
        session_id: RuntimeSessionId,
        reason: String,
    },

    #[error("Runtime resume failed for session {session_id}: {reason}")]
    ResumeFailed {
        session_id: RuntimeSessionId,
        reason: String,
    },

    #[error("Runtime stop failed for session {session_id}: {reason}")]
    StopFailed {
        session_id: RuntimeSessionId,
        reason: String,
    },

    #[error("Runtime transport failed: {reason}")]
    TransportFailed { reason: String },

    #[error("Runtime operation '{operation}' timed out after {duration_secs}s")]
    Timeout {
        operation: String,
        duration_secs: u64,
    },

    #[error("Invalid runtime configuration: {reason}")]
    InvalidConfiguration { reason: String },

    #[error("Duplicate runtime implementation registration: {implementation_id}")]
    DuplicateRuntime {
        implementation_id: RuntimeImplementationId,
    },

    #[error("Invalid identifier: {reason}")]
    InvalidId { reason: String },

    #[error("Internal runtime error: {reason}")]
    Internal { reason: String },
}

impl RuntimeError {
    /// Constructs a sanitized StartupFailed error.
    pub fn startup_failed(reason: impl AsRef<str>) -> Self {
        Self::StartupFailed {
            reason: sanitize_error_message(reason.as_ref()),
        }
    }

    /// Constructs a sanitized TransportFailed error.
    pub fn transport_failed(reason: impl AsRef<str>) -> Self {
        Self::TransportFailed {
            reason: sanitize_error_message(reason.as_ref()),
        }
    }

    /// Constructs a sanitized SendFailed error.
    pub fn send_failed(session_id: RuntimeSessionId, reason: impl AsRef<str>) -> Self {
        Self::SendFailed {
            session_id,
            reason: sanitize_error_message(reason.as_ref()),
        }
    }

    /// Constructs a sanitized DiscoveryFailed error.
    pub fn discovery_failed(reason: impl AsRef<str>) -> Self {
        Self::DiscoveryFailed {
            reason: sanitize_error_message(reason.as_ref()),
        }
    }

    /// Constructs a sanitized InvalidConfiguration error.
    pub fn invalid_configuration(reason: impl AsRef<str>) -> Self {
        Self::InvalidConfiguration {
            reason: sanitize_error_message(reason.as_ref()),
        }
    }
}
