//! Agent Studios Runtime Provider Transport
//!
//! Provides protocol execution drivers, Server-Sent Events stream decoding,
//! secure secret resolution, transactional multi-turn continuation state,
//! and routing for Codex's `ModelInferenceBackend`.

pub mod auth;
pub mod drivers;
pub mod error;
pub mod router;
pub mod secret;
pub mod sse;
pub mod state;

pub use auth::ResolvedAuth;
pub use drivers::{AnthropicDriver, ChatCompletionsDriver, GeminiDriver, ProtocolDriver};
pub use error::TransportError;
pub use router::RuntimeRouter;
pub use secret::{EnvSecretResolver, InMemorySecretResolver, SecretResolver, SecretString};
pub use sse::{SseEvent, SseParser, SseStream};
pub use state::{ContinuationManager, ContinuationTransaction, ThreadContinuation};
