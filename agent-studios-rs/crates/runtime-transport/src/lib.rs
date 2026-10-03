//! Agent Studios Runtime Provider Transport
//!
//! Provides protocol execution drivers, Server-Sent Events stream decoding,
//! secure secret resolution, transactional multi-turn continuation state,
//! and routing for Codex's `ModelInferenceBackend`.

pub mod auth;
pub mod diagnostic;
pub mod drivers;
pub mod error;
pub mod options;
pub mod router;
pub mod secret;
pub mod sse;
pub mod state;

pub use auth::ResolvedAuth;
pub use diagnostic::{
    DiagnosticLevel, NoopRuntimeDiagnosticSink, RecordingRuntimeDiagnosticSink, RuntimeDiagnostic,
    RuntimeDiagnosticSink,
};
pub use drivers::{AnthropicDriver, ChatCompletionsDriver, GeminiDriver, ProtocolDriver};
pub use error::{TransportError, sanitize_error_message};
pub use options::{RuntimeTransportOptions, read_bounded_error_body};
pub use router::{RuntimeModelRoute, RuntimeRouter};
pub use secret::{EnvSecretResolver, InMemorySecretResolver, SecretResolver, SecretString};
pub use sse::{SseEvent, SseParser, SseStream};
pub use state::{
    ContinuationKey, ContinuationLease, ContinuationManager, ContinuationTransaction,
    ThreadContinuation,
};
