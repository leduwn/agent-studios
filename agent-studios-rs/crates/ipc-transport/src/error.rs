use agent_studios_ipc_protocol::ProtocolError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Invalid connection state: expected {expected:?}, actual {actual:?}")]
    InvalidState { expected: String, actual: String },

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Connection timed out")]
    Timeout,

    #[error("Frame size {size} exceeds maximum limit {max}")]
    FrameTooLarge { size: usize, max: usize },

    #[error("Security descriptor creation failed: {0}")]
    SecurityDescriptor(String),

    #[error("Permission check failed: {0}")]
    PermissionDenied(String),

    #[error("Daemon error: {0}")]
    Daemon(String),
}

impl From<serde_json::Error> for TransportError {
    fn from(err: serde_json::Error) -> Self {
        TransportError::Serialization(err.to_string())
    }
}
