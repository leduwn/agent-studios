use std::sync::Arc;
use tokio::sync::TryLockError;

use agent_studios_provider::capabilities::CapabilitySupport;
use agent_studios_provider::model::ModelDescriptor;
use codex_http_client::HttpClientFactory;
use codex_login::AuthManager;
use codex_models_manager::ModelsManagerConfig;
use codex_models_manager::manager::{
    ModelsManager, ModelsManagerFuture, RefreshStrategy, SharedModelsManager,
};
use codex_protocol::config_types::CollaborationModeMask;
use codex_protocol::openai_models::{
    ModelInfo, ModelPreset, ModelVisibility, ModelsResponse, ReasoningEffort, ReasoningEffortPreset,
};

/// Converts an Agent Studios `ModelDescriptor` into a Codex `ModelInfo`.
pub fn model_descriptor_to_model_info(descriptor: &ModelDescriptor) -> ModelInfo {
    let slug = descriptor.id.as_str();
    let mut info = codex_models_manager::model_info::model_info_from_slug(slug);
    info.slug = slug.to_string();
    info.display_name = descriptor.display_name.clone();
    info.description = Some(format!("{} via Agent Studios", descriptor.display_name));
    info.visibility = ModelVisibility::List;
    info.priority = 0;
    info.supported_in_api = true;
    info.used_fallback_model_metadata = false;

    if let Some(ctx) = descriptor.limits.context_window_tokens {
        info.context_window = Some(ctx as i64);
        info.max_context_window = Some(ctx as i64);
    }

    if descriptor.capabilities.reasoning == CapabilitySupport::Supported {
        info.default_reasoning_level = Some(ReasoningEffort::Medium);
        info.supported_reasoning_levels = vec![
            ReasoningEffortPreset {
                effort: ReasoningEffort::Low,
                description: "Low reasoning effort".to_string(),
            },
            ReasoningEffortPreset {
                effort: ReasoningEffort::Medium,
                description: "Medium reasoning effort".to_string(),
            },
            ReasoningEffortPreset {
                effort: ReasoningEffort::High,
                description: "High reasoning effort".to_string(),
            },
        ];
    }

    info
}

/// Static, zero-discovery model manager serving model definitions directly from
/// `ProviderCatalog` model descriptors without making external HTTP calls.
#[derive(Debug)]
pub struct StaticModelsManager {
    inner: codex_models_manager::manager::StaticModelsManager,
    model_count: usize,
}

impl StaticModelsManager {
    /// Constructs a `StaticModelsManager` from descriptors.
    pub fn new(auth_manager: Option<Arc<AuthManager>>, descriptors: &[ModelDescriptor]) -> Self {
        let models: Vec<ModelInfo> = descriptors
            .iter()
            .map(model_descriptor_to_model_info)
            .collect();
        let model_count = models.len();
        let inner = codex_models_manager::manager::StaticModelsManager::new(
            auth_manager,
            ModelsResponse { models },
        );
        Self { inner, model_count }
    }

    /// Converts this manager into a shared dynamic reference suitable for Codex sessions.
    pub fn into_shared(self) -> SharedModelsManager {
        Arc::new(self)
    }

    /// Returns the number of statically registered models.
    pub fn model_count(&self) -> usize {
        self.model_count
    }
}

impl ModelsManager for StaticModelsManager {
    fn set_api_key_model_discovery_enabled(&self, enabled: bool) {
        self.inner.set_api_key_model_discovery_enabled(enabled);
    }

    fn list_models(
        &self,
        refresh_strategy: RefreshStrategy,
        http_client_factory: HttpClientFactory,
    ) -> ModelsManagerFuture<'_, Vec<ModelPreset>> {
        self.inner
            .list_models(refresh_strategy, http_client_factory)
    }

    fn raw_model_catalog(
        &self,
        refresh_strategy: RefreshStrategy,
        http_client_factory: HttpClientFactory,
    ) -> ModelsManagerFuture<'_, ModelsResponse> {
        self.inner
            .raw_model_catalog(refresh_strategy, http_client_factory)
    }

    fn refresh_after_auth_change(
        &self,
        http_client_factory: HttpClientFactory,
    ) -> ModelsManagerFuture<'_, ()> {
        self.inner.refresh_after_auth_change(http_client_factory)
    }

    fn get_remote_models(&self) -> ModelsManagerFuture<'_, Vec<ModelInfo>> {
        self.inner.get_remote_models()
    }

    fn try_get_remote_models(&self) -> Result<Vec<ModelInfo>, TryLockError> {
        self.inner.try_get_remote_models()
    }

    fn auth_manager(&self) -> Option<&AuthManager> {
        self.inner.auth_manager()
    }

    fn build_available_models(&self, remote_models: Vec<ModelInfo>) -> Vec<ModelPreset> {
        self.inner.build_available_models(remote_models)
    }

    fn list_collaboration_modes(&self) -> Vec<CollaborationModeMask> {
        self.inner.list_collaboration_modes()
    }

    fn try_list_models(&self) -> Result<Vec<ModelPreset>, TryLockError> {
        self.inner.try_list_models()
    }

    fn get_default_model<'a>(
        &'a self,
        model: &'a Option<String>,
        allow_provider_model_fallback: bool,
        refresh_strategy: RefreshStrategy,
        http_client_factory: HttpClientFactory,
    ) -> ModelsManagerFuture<'a, String> {
        self.inner.get_default_model(
            model,
            allow_provider_model_fallback,
            refresh_strategy,
            http_client_factory,
        )
    }

    fn get_model_info<'a>(
        &'a self,
        model: &'a str,
        config: &'a ModelsManagerConfig,
    ) -> ModelsManagerFuture<'a, ModelInfo> {
        self.inner.get_model_info(model, config)
    }

    fn refresh_if_new_etag(
        &self,
        etag: String,
        http_client_factory: HttpClientFactory,
    ) -> ModelsManagerFuture<'_, ()> {
        self.inner.refresh_if_new_etag(etag, http_client_factory)
    }
}
