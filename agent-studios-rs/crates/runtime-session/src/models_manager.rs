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
    InputModality, ModelInfo, ModelPreset, ModelVisibility, ModelsResponse,
};

use crate::error::RuntimeSessionError;

/// Converts an Agent Studios `ModelDescriptor` into a Codex `ModelInfo`.
///
/// Ensures explicit, scrubbed metadata without false fallback values:
/// - Explicit `context_window` and `max_context_window` using checked `i64::try_from` (or `None`).
/// - Input modalities strictly derived from descriptor capabilities (`Text` always; `Image` and `Audio` when supported).
/// - No synthetic reasoning levels invented (`supported_reasoning_levels` empty, `default_reasoning_level` none).
/// - Provider-native server features disabled (`supports_search_tool = false`).
/// - Codex host-level capabilities preserved (`include_skills_usage_instructions = true`, etc.).
pub fn model_descriptor_to_model_info(
    descriptor: &ModelDescriptor,
) -> Result<ModelInfo, RuntimeSessionError> {
    let slug = descriptor.id.as_str();
    let mut info = codex_models_manager::model_info::model_info_from_slug(slug);
    info.slug = slug.to_string();
    info.display_name = descriptor.display_name.clone();
    info.description = Some(format!("{} via Agent Studios", descriptor.display_name));
    info.visibility = ModelVisibility::List;
    info.priority = 0;
    info.supported_in_api = true;
    info.used_fallback_model_metadata = false;

    // Checked context window conversion; unknown context remains None (never 272,000 fallback)
    let context_window = match descriptor.limits.context_window_tokens {
        Some(tokens) => {
            let converted = i64::try_from(tokens).map_err(|_| {
                RuntimeSessionError::ModelMetadataOutOfRange(format!(
                    "Model '{slug}' context_window_tokens '{tokens}' exceeds i64 range"
                ))
            })?;
            Some(converted)
        }
        None => None,
    };
    info.context_window = context_window;
    info.max_context_window = context_window;

    // Explicit input modalities derived solely from descriptor capabilities
    let mut modalities = vec![InputModality::Text];
    if descriptor.capabilities.vision_input == CapabilitySupport::Supported {
        modalities.push(InputModality::Image);
    }
    if descriptor.capabilities.audio_input == CapabilitySupport::Supported {
        modalities.push(InputModality::Audio);
    }
    info.input_modalities = modalities;

    // Do not invent synthetic reasoning effort levels
    info.supported_reasoning_levels = Vec::new();
    info.default_reasoning_level = None;

    // Disable provider-native server search while preserving host tool instructions
    info.supports_search_tool = false;
    info.include_skills_usage_instructions = true;
    info.include_plugin_usage_instructions = true;
    info.include_apps_usage_instructions = true;

    Ok(info)
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
    pub fn new(
        auth_manager: Option<Arc<AuthManager>>,
        descriptors: &[ModelDescriptor],
    ) -> Result<Self, RuntimeSessionError> {
        let mut models = Vec::with_capacity(descriptors.len());
        for descriptor in descriptors {
            models.push(model_descriptor_to_model_info(descriptor)?);
        }
        let model_count = models.len();
        let inner = codex_models_manager::manager::StaticModelsManager::new(
            auth_manager,
            ModelsResponse { models },
        );
        Ok(Self { inner, model_count })
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
