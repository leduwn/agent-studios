use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Protocol version mismatch: expected {expected}, received {received}")]
    VersionMismatch { expected: String, received: String },

    #[error("Invalid envelope: {0}")]
    InvalidEnvelope(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Required capability missing: {0}")]
    CapabilityMissing(String),

    #[error("Approval error: {0}")]
    ApprovalError(String),

    #[error("Duplicate or expired request ID: {0}")]
    DuplicateOrExpired(String),

    #[error("Unrecoverable sequence gap: expected sequence {expected}, found {found}")]
    SequenceGapUnrecoverable { expected: u64, found: u64 },

    #[error("Recoverable sequence gap: expected sequence {expected}, found {found}")]
    SequenceGapRecoverable { expected: u64, found: u64 },

    #[error("Frame too large: size {size} exceeds maximum {max}")]
    FrameTooLarge { size: usize, max: usize },

    #[error("Frame decoding error: {0}")]
    FrameDecoding(String),

    #[error("Malformed frame: {0}")]
    MalformedFrame(String),

    #[error("Invalid buffer capacity: {0}")]
    InvalidCapacity(String),

    #[error("Correlation mismatch: expected {expected}, found {found}")]
    CorrelationMismatch { expected: String, found: String },

    #[error("Invalid state transition: {0}")]
    InvalidState(String),
}

impl From<serde_json::Error> for ProtocolError {
    fn from(err: serde_json::Error) -> Self {
        ProtocolError::Serialization(err.to_string())
    }
}
