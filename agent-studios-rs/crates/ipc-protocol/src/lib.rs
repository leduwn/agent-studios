//! Agent Studios IPC Protocol Foundation (M13 IPC-00)
//!
//! Provides a high-assurance protocol substrate for local inter-process communication:
//! - Versioned envelopes with major/minor semver compatibility
//! - Strongly typed request, correlation, session, and approval IDs
//! - Command and response framing with typed error taxonomy
//! - Monotonic event replay buffer with sequence gap detection
//! - Capability negotiation (required vs optional features)
//! - Explicit approval contracts and lifecycle verification
//! - Secret redaction for keys, tokens, and authorization headers
//! - HMAC-SHA256 signature authentication with nonce replay protection

pub mod approval;
pub mod auth;
pub mod capabilities;
pub mod envelope;
pub mod error;
pub mod framing;
pub mod id;
pub mod redaction;
pub mod replay;

pub use approval::{ApprovalAction, ApprovalContract, ApprovalDecision, ApprovalStatus, RiskLevel};
pub use auth::{
    AuthBinding, AuthVerificationBinding, AuthVerificationContext, AuthVerificationFlow,
    HmacAuthHeader, HmacSigner, NonceTracker,
};
pub use capabilities::{
    Capability, CapabilityNegotiationRequest, CapabilityNegotiationResponse,
    ServerCapabilityNegotiator,
};
pub use envelope::{IpcEnvelope, ProtocolVersion};
pub use error::ProtocolError;
pub use framing::{
    AccessMode, DEFAULT_MAX_FRAME_SIZE, ErrorCode, FrameCodec, IpcCommand, IpcErrorResponse,
    IpcResponse, decode_frame, encode_frame,
};
pub use id::{ApprovalId, ClientId, CorrelationId, RequestId, SessionId, SubscriptionId};
pub use redaction::{MaskedTelemetry, Redacted, SecretRedactor};
pub use replay::{IpcEvent, ReplayBuffer};

pub mod prelude {
    pub use super::approval::*;
    pub use super::auth::*;
    pub use super::capabilities::*;
    pub use super::envelope::*;
    pub use super::error::*;
    pub use super::framing::*;
    pub use super::id::*;
    pub use super::redaction::*;
    pub use super::replay::*;
}
