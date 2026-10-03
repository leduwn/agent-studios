use agent_studios_provider::instance::ProviderInstance;
use agent_studios_provider::model::ModelDescriptor;
use async_trait::async_trait;
use codex_api::{ResponseStream, ResponsesApiRequest};
use codex_model_provider::ModelInferenceContext;
use std::fmt;

use crate::auth::ResolvedAuth;
use crate::error::TransportError;
use crate::options::RuntimeTransportOptions;
use crate::state::ContinuationTransaction;

pub mod anthropic;
pub mod chat_completions;
pub mod gemini;

pub use anthropic::AnthropicDriver;
pub use chat_completions::ChatCompletionsDriver;
pub use gemini::GeminiDriver;

/// Abstract driver responsible for translating and executing a specific wire protocol over HTTP/SSE.
#[async_trait]
pub trait ProtocolDriver: Send + Sync + fmt::Debug {
    /// Translates the request, initiates the HTTP stream, and produces a `ResponseStream`.
    #[allow(clippy::too_many_arguments)]
    async fn stream(
        &self,
        client: &reqwest::Client,
        instance: &ProviderInstance,
        descriptor: &ModelDescriptor,
        auth: &ResolvedAuth,
        request: ResponsesApiRequest,
        context: ModelInferenceContext,
        continuation_tx: ContinuationTransaction,
        options: &RuntimeTransportOptions,
    ) -> Result<ResponseStream, TransportError>;
}
