//! Anthropic Messages protocol adapter for Agent Studios.
//!
//! Provides pure, deterministic protocol translation between Codex `ResponsesApiRequest` /
//! `ResponseEvent` semantics and the Anthropic Messages wire protocol (`POST /v1/messages` and SSE).

pub mod continuation;
pub mod error;
pub mod request;
pub mod stream;
pub mod types;

pub use continuation::{AnthropicContinuationState, NativeThinkingBlock};
pub use error::{AnthropicAdapterError, AnthropicAdapterWarning, AnthropicStreamApiError};
pub use request::{
    AnthropicPromptCachePolicy, AnthropicRequestOptions, AnthropicRequestTranslation,
    AnthropicThinkingPolicy, translate_request,
};
pub use stream::AnthropicStreamTranslator;
pub use types::*;
