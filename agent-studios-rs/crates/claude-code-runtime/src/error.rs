use agent_studios_external_runtime::{RuntimeError, SanitizedRuntimeMessage};
use thiserror::Error;

/// Error type for the Claude Code CLI runtime adapter.
#[derive(Error, Debug)]
pub enum ClaudeCodeError {
    #[error("Failed to spawn Claude Code process: {0}")]
    ProcessSpawn(String),

    #[error("I/O error during Claude Code execution: {0}")]
    ProcessIo(#[from] std::io::Error),

    #[error("Claude Code child process exited unexpectedly with code {0}")]
    ProcessExited(i32),

    #[error("Stream parsing error: {0}")]
    StreamParseError(String),

    #[error("Session {0} was not found")]
    SessionNotFound(String),

    #[error("Session {0} is already active")]
    SessionAlreadyActive(String),

    #[error("Invalid session lifecycle state: {0}")]
    InvalidSessionState(String),

    #[error("API budget limit reached: {0}")]
    BudgetExceeded(String),

    #[error("Tool approval timed out: {0}")]
    ApprovalTimeout(String),

    #[error("Tool approval rejected: {0}")]
    ApprovalDenied(String),

    #[error("Process sandbox policy violation: {0}")]
    SandboxPolicyViolation(String),

    #[error("Unsupported platform or configuration: {0}")]
    UnsupportedPlatform(String),

    #[error("Claude Code binary was not found: {0}")]
    BinaryNotFound(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Internal adapter error: {0}")]
    Internal(String),
}

impl From<ClaudeCodeError> for RuntimeError {
    fn from(err: ClaudeCodeError) -> Self {
        match err {
            ClaudeCodeError::SessionNotFound(_id) => RuntimeError::SessionNotFound {
                session_id: agent_studios_external_runtime::RuntimeSessionId::new(),
            },
            ClaudeCodeError::SessionAlreadyActive(id) => RuntimeError::invalid_configuration(format!(
                "Session {id} is already active"
            )),
            ClaudeCodeError::InvalidSessionState(msg) => RuntimeError::invalid_configuration(msg),
            ClaudeCodeError::SandboxPolicyViolation(msg) => RuntimeError::StartupFailed {
                reason: SanitizedRuntimeMessage::new(msg),
            },
            ClaudeCodeError::UnsupportedPlatform(msg) => RuntimeError::StartupFailed {
                reason: SanitizedRuntimeMessage::new(format!(
                    "Unsupported platform containment: {msg}"
                )),
            },
            ClaudeCodeError::BinaryNotFound(msg) => RuntimeError::discovery_failed(msg),
            ClaudeCodeError::ApprovalDenied(msg) | ClaudeCodeError::ApprovalTimeout(msg) => {
                RuntimeError::send_failed(
                    agent_studios_external_runtime::RuntimeSessionId::new(),
                    msg,
                )
            }
            ClaudeCodeError::BudgetExceeded(msg) => RuntimeError::send_failed(
                agent_studios_external_runtime::RuntimeSessionId::new(),
                format!("Budget exceeded: {msg}"),
            ),
            other => RuntimeError::Internal {
                reason: SanitizedRuntimeMessage::new(other.to_string()),
            },
        }
    }
}
