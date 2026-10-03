use std::fmt;
use std::sync::Arc;

use codex_api::ApiError;
use codex_api::ResponseStream;
use codex_api::ResponsesApiRequest;

use crate::ModelProviderFuture;

/// Generic request-scoped context passed to a custom model inference backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInferenceContext {
    pub thread_id: String,
    pub turn_id: Option<String>,
}

/// Generic provider-neutral model inference backend trait.
pub trait ModelInferenceBackend: fmt::Debug + Send + Sync {
    fn stream<'a>(
        &'a self,
        request: ResponsesApiRequest,
        context: ModelInferenceContext,
    ) -> ModelProviderFuture<'a, Result<ResponseStream, ApiError>>;
}

/// Shared model inference backend handle.
pub type SharedModelInferenceBackend = Arc<dyn ModelInferenceBackend>;
