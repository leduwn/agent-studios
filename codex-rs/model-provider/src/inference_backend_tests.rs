use std::sync::Arc;

use codex_api::ApiError;
use codex_api::ResponseStream;
use codex_api::ResponsesApiRequest;
use codex_model_provider_info::ModelProviderInfo;

use crate::ModelInferenceBackend;
use crate::ModelInferenceContext;
use crate::ModelProviderFuture;
use crate::create_model_provider;
use crate::create_model_provider_with_inference_backend;

#[derive(Debug)]
struct MockInferenceBackend {
    _name: &'static str,
}

impl ModelInferenceBackend for MockInferenceBackend {
    fn stream<'a>(
        &'a self,
        _request: ResponsesApiRequest,
        _context: ModelInferenceContext,
    ) -> ModelProviderFuture<'a, Result<ResponseStream, ApiError>> {
        Box::pin(async { Err(ApiError::Stream("mock backend stream error".to_string())) })
    }
}

#[test]
fn test_default_model_provider_has_no_inference_backend() {
    let provider_info = ModelProviderInfo::default();
    let provider = create_model_provider(provider_info, None);
    assert!(provider.inference_backend().is_none());
}

#[test]
fn test_model_provider_with_inference_backend_attaches_correctly() {
    let provider_info = ModelProviderInfo::default();
    let backend: Arc<dyn ModelInferenceBackend> = Arc::new(MockInferenceBackend {
        name: "test-backend",
    });
    let provider = create_model_provider_with_inference_backend(
        provider_info,
        None,
        Some(Arc::clone(&backend)),
    );

    let attached = provider.inference_backend();
    assert!(attached.is_some());
    let attached = attached.unwrap();
    let debug_str = format!("{attached:?}");
    assert!(debug_str.contains("MockInferenceBackend"));
    assert!(debug_str.contains("test-backend"));
}

#[test]
fn test_model_inference_context_properties() {
    let ctx = ModelInferenceContext {
        thread_id: "thread-123".to_string(),
        turn_id: Some("turn-456".to_string()),
    };
    assert_eq!(ctx.thread_id, "thread-123");
    assert_eq!(ctx.turn_id.as_deref(), Some("turn-456"));
    assert_eq!(ctx.clone(), ctx);
}
