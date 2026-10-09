use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::capabilities::RuntimeCapability;
use crate::id::{RuntimeConfigRef, RuntimeImplementationId, RuntimeInstanceId, RuntimeSessionId};
use crate::lifecycle::RuntimeLifecycleState;

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let needle_bytes = needle.as_bytes();
    let haystack_bytes = haystack.as_bytes();
    if needle_bytes.len() > haystack_bytes.len() {
        return None;
    }
    for (i, window) in haystack_bytes.windows(needle_bytes.len()).enumerate() {
        if window.eq_ignore_ascii_case(needle_bytes) && haystack.is_char_boundary(i) {
            return Some(i);
        }
    }
    None
}

fn extract_token_len(after: &str) -> usize {
    if let Some(inner) = after.strip_prefix('"') {
        inner.find('"').map(|i| i + 2).unwrap_or(after.len())
    } else if let Some(inner) = after.strip_prefix('\'') {
        inner.find('\'').map(|i| i + 2).unwrap_or(after.len())
    } else {
        after
            .char_indices()
            .find(|(_, c)| {
                c.is_whitespace()
                    || *c == ';'
                    || *c == '&'
                    || *c == '"'
                    || *c == '\''
                    || *c == '#'
                    || *c == '<'
                    || *c == '>'
                    || *c == ')'
            })
            .map(|(i, _)| i)
            .unwrap_or(after.len())
    }
}

fn extract_val_span(after: &str) -> (usize, usize) {
    let ws_len = after
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(i, _)| i)
        .unwrap_or(0);
    let token_len = extract_token_len(&after[ws_len..]);
    (ws_len, token_len)
}

fn redact_prefix_pattern(sanitized: &mut String, prefix: &str, replacement: &str) {
    let mut cursor = 0;
    while cursor < sanitized.len() {
        if let Some(rel) = sanitized[cursor..].find(prefix) {
            let idx = cursor + rel;
            let after = &sanitized[idx..];
            if after.starts_with(replacement) {
                cursor = idx + replacement.len();
                continue;
            }
            let token_bytes = extract_token_len(after);
            if token_bytes > 0 {
                sanitized.replace_range(idx..idx + token_bytes, replacement);
                cursor = idx + replacement.len();
            } else {
                cursor = idx + prefix.len();
            }
        } else {
            break;
        }
    }
}

fn redact_kv_pattern(sanitized: &mut String, prefix: &str, replacement: &str) {
    let mut cursor = 0;
    while cursor < sanitized.len() {
        if let Some(rel) = find_ascii_case_insensitive(&sanitized[cursor..], prefix) {
            let idx = cursor + rel;
            let val_start = idx + prefix.len();
            let after = &sanitized[val_start..];
            if after.starts_with("[REDACTED") {
                cursor = val_start + "[REDACTED".len();
                continue;
            }
            let (ws_len, val_bytes) = extract_val_span(after);
            if val_bytes > 0 {
                sanitized.replace_range(idx..val_start + ws_len + val_bytes, replacement);
                cursor = idx + replacement.len();
            } else {
                cursor = val_start;
            }
        } else {
            break;
        }
    }
}

/// Strips common secret patterns (API keys, bearer tokens, passwords) from diagnostic strings.
/// Processes all occurrences across multiple URLs and query parameters without early truncation.
pub fn sanitize_error_message(input: &str) -> String {
    let mut sanitized = input.to_string();

    // Redact sk-ant-* Anthropic API keys directly
    redact_prefix_pattern(&mut sanitized, "sk-ant-", "[REDACTED_API_KEY]");

    // Redact ghp_* GitHub tokens directly
    redact_prefix_pattern(&mut sanitized, "ghp_", "[REDACTED_GITHUB_TOKEN]");

    // Redact other known secret prefixes
    for prefix in &["gho_", "glpat-", "xoxb-", "xoxp-", "npm_"] {
        redact_prefix_pattern(&mut sanitized, prefix, "[REDACTED_TOKEN]");
    }

    // Redact Bearer tokens
    redact_kv_pattern(&mut sanitized, "bearer ", "[REDACTED_BEARER_TOKEN]");

    // Redact api_key / apikey / x-api-key / key= patterns
    for prefix in &[
        "api_key=",
        "api-key=",
        "apikey=",
        "api_key:",
        "x-api-key:",
        "key=",
    ] {
        redact_kv_pattern(&mut sanitized, prefix, "[REDACTED_API_KEY]");
    }

    // Redact secret= patterns
    for prefix in &["secret=", "secret:", "password=", "passwd=", "pwd="] {
        redact_kv_pattern(&mut sanitized, prefix, "[REDACTED_SECRET]");
    }

    // Redact general sk- tokens (OpenAI project/admin keys, etc.)
    for prefix in &["sk-proj-", "sk-admin-", "sk-svcacct-", "sk-"] {
        redact_prefix_pattern(&mut sanitized, prefix, "[REDACTED_API_KEY]");
    }

    // Redact token=, token:, access_token=, refresh_token=, auth=, authorization:
    for prefix in &[
        "token=",
        "token:",
        "access_token=",
        "refresh_token=",
        "auth=",
        "authorization:",
    ] {
        redact_kv_pattern(&mut sanitized, prefix, "[REDACTED_TOKEN]");
    }

    // Redact embedded user:password credentials in URIs across all occurrences (e.g. https://user:pass@host)
    let mut uri_search_start = 0;
    while uri_search_start < sanitized.len() {
        if let Some(rel_scheme) = sanitized[uri_search_start..].find("://") {
            let scheme_pos = uri_search_start + rel_scheme;
            let after_scheme_pos = scheme_pos + 3;
            let after_scheme = &sanitized[after_scheme_pos..];

            // Authority ends at first '/', '?', '#', whitespace, quote, or delimiter
            let authority_len = after_scheme
                .char_indices()
                .find(|(_, c)| {
                    matches!(c, '/' | '?' | '#' | '"' | '\'' | '<' | '>' | '`') || c.is_whitespace()
                })
                .map(|(i, _)| i)
                .unwrap_or(after_scheme.len());

            let authority = &after_scheme[..authority_len];
            if let Some(at_idx) = authority.find('@') {
                let user_info = &authority[..at_idx];
                if let Some(colon_idx) = user_info.find(':') {
                    let pass_start = after_scheme_pos + colon_idx + 1;
                    let pass_end = after_scheme_pos + at_idx;
                    if pass_end > pass_start {
                        let current_pass = &sanitized[pass_start..pass_end];
                        if !current_pass.starts_with("[REDACTED") {
                            sanitized.replace_range(pass_start..pass_end, "[REDACTED_PASSWORD]");
                            uri_search_start = pass_start + "[REDACTED_PASSWORD]".len();
                            continue;
                        }
                    }
                }
            }
            uri_search_start = after_scheme_pos;
        } else {
            break;
        }
    }

    sanitized
}

/// Strongly-typed sanitized message wrapping strings to prevent credential leaks.
/// Private inner field guarantees sanitization on construction and deserialization.
#[derive(Clone, Eq, PartialEq, Hash, PartialOrd, Ord, Default)]
pub struct SanitizedRuntimeMessage(String);

impl SanitizedRuntimeMessage {
    pub fn new(msg: impl AsRef<str>) -> Self {
        Self(sanitize_error_message(msg.as_ref()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SanitizedRuntimeMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for SanitizedRuntimeMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SanitizedRuntimeMessage({:?})", self.0)
    }
}

impl From<&str> for SanitizedRuntimeMessage {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for SanitizedRuntimeMessage {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl AsRef<str> for SanitizedRuntimeMessage {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Serialize for SanitizedRuntimeMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SanitizedRuntimeMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::new(raw))
    }
}

/// Strongly-typed errors emitted by the external agent runtime subsystem.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeError {
    #[error("Runtime implementation not found: {id}")]
    RuntimeNotFound { id: String },

    #[error("Runtime implementation unavailable: {implementation_id} ({reason})")]
    RuntimeUnavailable {
        implementation_id: RuntimeImplementationId,
        reason: SanitizedRuntimeMessage,
    },

    #[error("Runtime discovery failed: {reason}")]
    DiscoveryFailed { reason: SanitizedRuntimeMessage },

    #[error("Unsupported runtime capability: {capability} ({reason})")]
    UnsupportedCapability {
        capability: RuntimeCapability,
        reason: SanitizedRuntimeMessage,
    },

    #[error("Invalid lifecycle transition from {from} to {to}: {reason}")]
    InvalidLifecycleTransition {
        from: RuntimeLifecycleState,
        to: RuntimeLifecycleState,
        reason: SanitizedRuntimeMessage,
    },

    #[error("Runtime session not found: {session_id}")]
    SessionNotFound { session_id: RuntimeSessionId },

    #[error("Runtime session startup failed: {reason}")]
    StartupFailed { reason: SanitizedRuntimeMessage },

    #[error("Runtime send failed for session {session_id}: {reason}")]
    SendFailed {
        session_id: RuntimeSessionId,
        reason: SanitizedRuntimeMessage,
    },

    #[error("Runtime interruption failed for session {session_id}: {reason}")]
    InterruptionFailed {
        session_id: RuntimeSessionId,
        reason: SanitizedRuntimeMessage,
    },

    #[error("Runtime resume failed for session {session_id}: {reason}")]
    ResumeFailed {
        session_id: RuntimeSessionId,
        reason: SanitizedRuntimeMessage,
    },

    #[error("Runtime stop failed for session {session_id}: {reason}")]
    StopFailed {
        session_id: RuntimeSessionId,
        reason: SanitizedRuntimeMessage,
    },

    #[error("Runtime transport failed: {reason}")]
    TransportFailed { reason: SanitizedRuntimeMessage },

    #[error("Runtime operation '{operation}' timed out after {duration_secs}s")]
    Timeout {
        operation: String,
        duration_secs: u64,
    },

    #[error("Invalid runtime configuration: {reason}")]
    InvalidConfiguration { reason: SanitizedRuntimeMessage },

    #[error("Duplicate runtime implementation registration: {implementation_id}")]
    DuplicateRuntime {
        implementation_id: RuntimeImplementationId,
    },

    #[error("Invalid identifier: {reason}")]
    InvalidId { reason: SanitizedRuntimeMessage },

    #[error(
        "Session instance mismatch: session {session_id} belongs to instance {expected}, but request specified {actual}"
    )]
    SessionInstanceMismatch {
        expected: RuntimeInstanceId,
        actual: RuntimeInstanceId,
        session_id: RuntimeSessionId,
    },

    #[error(
        "Session implementation mismatch: session {session_id} belongs to implementation {expected}, but request specified {actual}"
    )]
    SessionImplementationMismatch {
        expected: RuntimeImplementationId,
        actual: RuntimeImplementationId,
        session_id: RuntimeSessionId,
    },

    #[error(
        "Instance implementation mismatch: instance {instance_id} belongs to implementation {expected}, but request specified {actual}"
    )]
    InstanceImplementationMismatch {
        expected: RuntimeImplementationId,
        actual: RuntimeImplementationId,
        instance_id: RuntimeInstanceId,
    },

    #[error("Unknown runtime instance: {instance_id}")]
    UnknownInstance { instance_id: RuntimeInstanceId },

    #[error("Runtime instance unavailable: {instance_id} ({reason})")]
    InstanceUnavailable {
        instance_id: RuntimeInstanceId,
        reason: SanitizedRuntimeMessage,
    },

    #[error("Unsupported configuration for instance {instance_id}: {config_ref}")]
    UnsupportedConfiguration {
        instance_id: RuntimeInstanceId,
        config_ref: RuntimeConfigRef,
    },

    #[error(
        "Terminal state violation: session {session_id} is in terminal state '{current_state}', action '{attempted_action}' is forbidden"
    )]
    TerminalStateError {
        session_id: RuntimeSessionId,
        current_state: RuntimeLifecycleState,
        attempted_action: String,
    },

    #[error(
        "Event stream lagged for session {session_id}: skipped {skipped_count} events (expected sequence: {expected_sequence})"
    )]
    EventStreamLagged {
        session_id: RuntimeSessionId,
        skipped_count: u64,
        expected_sequence: u64,
    },

    #[error(
        "Event retention exceeded for session {session_id}: requested sequence {requested_sequence} is older than earliest retained sequence {earliest_available_sequence}"
    )]
    EventRetentionExceeded {
        session_id: RuntimeSessionId,
        requested_sequence: u64,
        earliest_available_sequence: u64,
    },

    #[error(
        "Invalid replay offset for session {session_id}: requested sequence {requested_sequence} exceeds latest committed sequence {latest_available_sequence}"
    )]
    InvalidReplayOffset {
        session_id: RuntimeSessionId,
        requested_sequence: u64,
        latest_available_sequence: u64,
    },

    #[error(
        "Event session mismatch: event belongs to session {actual}, expected session {expected} (sequence: {sequence})"
    )]
    EventSessionMismatch {
        expected: RuntimeSessionId,
        actual: RuntimeSessionId,
        sequence: u64,
    },

    #[error("Invalid event sequence for session {session_id} at sequence {sequence}: {reason}")]
    InvalidEventSequence {
        session_id: RuntimeSessionId,
        sequence: u64,
        reason: SanitizedRuntimeMessage,
    },

    #[error(
        "Event emission rejected for session {session_id}: session is in terminal state '{terminal_state}' (attempted sequence: {attempted_sequence})"
    )]
    EventAfterTerminalState {
        session_id: RuntimeSessionId,
        terminal_state: RuntimeLifecycleState,
        attempted_sequence: u64,
    },

    #[error(
        "Event stream sequence gap detected for session {session_id}: expected sequence {expected_sequence}, received sequence {received_sequence}"
    )]
    EventStreamGap {
        session_id: RuntimeSessionId,
        expected_sequence: u64,
        received_sequence: u64,
    },

    #[error("Invalid workspace access for session or instance: {reason}")]
    InvalidWorkspaceAccess { reason: SanitizedRuntimeMessage },

    #[error("Internal runtime error: {reason}")]
    Internal { reason: SanitizedRuntimeMessage },
}

impl RuntimeError {
    /// Constructs an InvalidReplayOffset error.
    pub fn invalid_replay_offset(
        session_id: RuntimeSessionId,
        requested_sequence: u64,
        latest_available_sequence: u64,
    ) -> Self {
        Self::InvalidReplayOffset {
            session_id,
            requested_sequence,
            latest_available_sequence,
        }
    }

    /// Constructs an InvalidWorkspaceAccess error.
    pub fn invalid_workspace_access(reason: impl Into<SanitizedRuntimeMessage>) -> Self {
        Self::InvalidWorkspaceAccess {
            reason: reason.into(),
        }
    }

    /// Constructs an InstanceUnavailable error.
    pub fn instance_unavailable(
        instance_id: RuntimeInstanceId,
        reason: impl Into<SanitizedRuntimeMessage>,
    ) -> Self {
        Self::InstanceUnavailable {
            instance_id,
            reason: reason.into(),
        }
    }

    /// Constructs an UnsupportedConfiguration error.
    pub fn unsupported_configuration(
        instance_id: RuntimeInstanceId,
        config_ref: RuntimeConfigRef,
    ) -> Self {
        Self::UnsupportedConfiguration {
            instance_id,
            config_ref,
        }
    }

    /// Constructs a sanitized StartupFailed error.
    pub fn startup_failed(reason: impl Into<SanitizedRuntimeMessage>) -> Self {
        Self::StartupFailed {
            reason: reason.into(),
        }
    }

    /// Constructs a sanitized TransportFailed error.
    pub fn transport_failed(reason: impl Into<SanitizedRuntimeMessage>) -> Self {
        Self::TransportFailed {
            reason: reason.into(),
        }
    }

    /// Constructs a sanitized SendFailed error.
    pub fn send_failed(
        session_id: RuntimeSessionId,
        reason: impl Into<SanitizedRuntimeMessage>,
    ) -> Self {
        Self::SendFailed {
            session_id,
            reason: reason.into(),
        }
    }

    /// Constructs a sanitized InterruptionFailed error.
    pub fn interruption_failed(
        session_id: RuntimeSessionId,
        reason: impl Into<SanitizedRuntimeMessage>,
    ) -> Self {
        Self::InterruptionFailed {
            session_id,
            reason: reason.into(),
        }
    }

    /// Constructs a sanitized ResumeFailed error.
    pub fn resume_failed(
        session_id: RuntimeSessionId,
        reason: impl Into<SanitizedRuntimeMessage>,
    ) -> Self {
        Self::ResumeFailed {
            session_id,
            reason: reason.into(),
        }
    }

    /// Constructs a sanitized StopFailed error.
    pub fn stop_failed(
        session_id: RuntimeSessionId,
        reason: impl Into<SanitizedRuntimeMessage>,
    ) -> Self {
        Self::StopFailed {
            session_id,
            reason: reason.into(),
        }
    }

    /// Constructs a sanitized DiscoveryFailed error.
    pub fn discovery_failed(reason: impl Into<SanitizedRuntimeMessage>) -> Self {
        Self::DiscoveryFailed {
            reason: reason.into(),
        }
    }

    /// Constructs a sanitized InvalidConfiguration error.
    pub fn invalid_configuration(reason: impl Into<SanitizedRuntimeMessage>) -> Self {
        Self::InvalidConfiguration {
            reason: reason.into(),
        }
    }

    /// Constructs a sanitized InvalidId error.
    pub fn invalid_id(reason: impl Into<SanitizedRuntimeMessage>) -> Self {
        Self::InvalidId {
            reason: reason.into(),
        }
    }

    /// Constructs a TerminalStateError.
    pub fn terminal_state_error(
        session_id: RuntimeSessionId,
        current_state: RuntimeLifecycleState,
        attempted_action: impl Into<String>,
    ) -> Self {
        Self::TerminalStateError {
            session_id,
            current_state,
            attempted_action: attempted_action.into(),
        }
    }
}
