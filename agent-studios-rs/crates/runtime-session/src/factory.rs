use std::sync::Arc;

use agent_studios_codex_bridge::{
    CodexBridgeOptions, CodexProviderBridge, deterministic_codex_provider_key,
};
use agent_studios_provider::ProviderCatalog;
use agent_studios_provider::id::{ModelId, ProviderInstanceId};
use agent_studios_provider::model::{ModelDescriptor, ModelRef};
use agent_studios_provider::protocol::ProtocolFamily;
use agent_studios_runtime_transport::diagnostic::{
    NoopRuntimeDiagnosticSink, RuntimeDiagnosticSink,
};
use agent_studios_runtime_transport::options::RuntimeTransportOptions;
use agent_studios_runtime_transport::router::{RuntimeModelRoute, RuntimeRouter};
use agent_studios_runtime_transport::secret::SecretResolver;
use agent_studios_runtime_transport::state::ContinuationManager;
use codex_core::ModelRuntimeOverride;
use codex_login::AuthManager;
use codex_model_provider::{ModelInferenceBackend, create_model_provider_with_inference_backend};
use codex_model_provider_info::{ModelProviderInfo, WireApi};

use crate::error::RuntimeSessionError;
use crate::models_manager::StaticModelsManager;
use crate::prepared::PreparedRuntimeSession;

/// Factory assembling and preparing runtime provider session overrides for Codex threads.
///
/// Converts Agent Studios `ProviderCatalog` definitions and transport configurations into
/// unified, strictly paired `ModelRuntimeOverride` handles ready for thread injection.
#[derive(Clone)]
pub struct AgentStudiosRuntimeSessionFactory {
    catalog: Arc<ProviderCatalog>,
    secret_resolver: Arc<dyn SecretResolver>,
    continuation_manager: Arc<ContinuationManager>,
    transport_options: RuntimeTransportOptions,
    diagnostic_sink: Arc<dyn RuntimeDiagnosticSink>,
    auth_manager: Option<Arc<AuthManager>>,
    bridge_options: CodexBridgeOptions,
}

impl AgentStudiosRuntimeSessionFactory {
    /// Creates a factory with default transport and bridge options.
    pub fn new(
        catalog: Arc<ProviderCatalog>,
        secret_resolver: Arc<dyn SecretResolver>,
        continuation_manager: Arc<ContinuationManager>,
    ) -> Self {
        Self {
            catalog,
            secret_resolver,
            continuation_manager,
            transport_options: RuntimeTransportOptions::default(),
            diagnostic_sink: Arc::new(NoopRuntimeDiagnosticSink),
            auth_manager: None,
            bridge_options: CodexBridgeOptions::default(),
        }
    }

    /// Sets custom transport options.
    pub fn with_transport_options(mut self, options: RuntimeTransportOptions) -> Self {
        self.transport_options = options;
        self
    }

    /// Sets custom diagnostic sink.
    pub fn with_diagnostic_sink(mut self, sink: Arc<dyn RuntimeDiagnosticSink>) -> Self {
        self.diagnostic_sink = sink;
        self
    }

    /// Sets optional Codex `AuthManager`.
    pub fn with_auth_manager(mut self, auth_manager: Option<Arc<AuthManager>>) -> Self {
        self.auth_manager = auth_manager;
        self
    }

    /// Sets optional `CodexBridgeOptions` for native Responses routing.
    pub fn with_bridge_options(mut self, bridge_options: CodexBridgeOptions) -> Self {
        self.bridge_options = bridge_options;
        self
    }

    /// Returns a reference to the provider catalog.
    pub fn catalog(&self) -> &Arc<ProviderCatalog> {
        &self.catalog
    }

    /// Returns a reference to the secret resolver.
    pub fn secret_resolver(&self) -> &Arc<dyn SecretResolver> {
        &self.secret_resolver
    }

    /// Returns a reference to the continuation manager.
    pub fn continuation_manager(&self) -> &Arc<ContinuationManager> {
        &self.continuation_manager
    }

    /// Prepares a runtime session override for the specified provider instance and model.
    ///
    /// - Verifies that the provider instance exists and is enabled.
    /// - Collects all registered model descriptors for this instance.
    /// - Verifies or derives the target model slug.
    /// - Constructs a static, zero-discovery `StaticModelsManager`.
    /// - Configures the provider runtime:
    ///   - For `OpenAiResponses`: resolves via `CodexProviderBridge` with native execution.
    ///   - For custom protocols (`OpenAiChatCompletions`, `AnthropicMessages`, `GeminiGenerateContent`):
    ///     instantiates a session-scoped `RuntimeRouter` registering all instance models.
    /// - Strictly pairs `SharedModelProvider` and `SharedModelsManager` in `ModelRuntimeOverride`.
    pub fn prepare_runtime_session(
        &self,
        instance_id: &ProviderInstanceId,
        target_model_id: Option<&ModelId>,
    ) -> Result<PreparedRuntimeSession, RuntimeSessionError> {
        // 1. Fetch provider instance
        let instance = self
            .catalog
            .get_provider_instance(instance_id)
            .ok_or(RuntimeSessionError::ProviderInstanceNotFound(*instance_id))?;

        if !instance.enabled {
            return Err(RuntimeSessionError::ProviderInstanceDisabled(*instance_id));
        }

        // 2. Fetch models for this instance
        let mut models: Vec<ModelDescriptor> = self
            .catalog
            .list_models()
            .into_iter()
            .filter(|m| m.provider_instance_id == *instance_id)
            .cloned()
            .collect();

        if models.is_empty() {
            return Err(RuntimeSessionError::NoModelsForProvider(*instance_id));
        }

        // Sort deterministically by id
        models.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));

        // 3. Resolve selected model
        let selected_model = match target_model_id {
            Some(target_id) => {
                let found = models.iter().find(|m| &m.id == target_id);
                if found.is_none() {
                    return Err(RuntimeSessionError::ModelNotFound {
                        model_id: target_id.clone(),
                        provider_instance_id: *instance_id,
                    });
                }
                target_id.as_str().to_string()
            }
            None => models[0].id.as_str().to_string(),
        };

        // 4. Build StaticModelsManager (zero discovery, static catalog)
        let models_manager =
            StaticModelsManager::new(self.auth_manager.clone(), &models).into_shared();

        // 5. Deterministic Codex provider key
        let codex_provider_key = deterministic_codex_provider_key(instance_id);

        // 6. Build Provider info & inference backend
        let (provider_info, inference_backend) = match &instance.protocol {
            ProtocolFamily::OpenAiResponses => {
                let target_id = ModelId::new(&selected_model)
                    .map_err(|e| RuntimeSessionError::Config(e.to_string()))?;
                let model_ref = ModelRef::new(*instance_id, target_id);
                let binding =
                    CodexProviderBridge::resolve(&self.catalog, &model_ref, self.bridge_options)?;
                (binding.provider_info, None)
            }
            _ => {
                // Session-scoped RuntimeRouter
                let router = RuntimeRouter::try_new_with_options(
                    self.secret_resolver.clone(),
                    self.continuation_manager.clone(),
                    self.transport_options.clone(),
                    self.diagnostic_sink.clone(),
                )?;
                router.register_instance(instance.clone());
                for model in &models {
                    router.register_route(
                        model.id.as_str(),
                        RuntimeModelRoute::new(instance.id, model.clone()),
                    )?;
                }
                let backend: Arc<dyn ModelInferenceBackend> = Arc::new(router);
                let provider_info = ModelProviderInfo {
                    name: codex_provider_key.clone(),
                    base_url: Some(instance.endpoint.base_url.clone()),
                    wire_api: WireApi::Responses,
                    requires_openai_auth: false,
                    ..Default::default()
                };
                (provider_info, Some(backend))
            }
        };

        let shared_provider = create_model_provider_with_inference_backend(
            provider_info,
            self.auth_manager.clone(),
            inference_backend,
        );

        // 7. Strictly pair provider and models manager in ModelRuntimeOverride
        let runtime_override = ModelRuntimeOverride::new(shared_provider, models_manager);

        let available_models: Vec<String> = models
            .into_iter()
            .map(|m| m.id.as_str().to_string())
            .collect();

        Ok(PreparedRuntimeSession::new(
            runtime_override,
            codex_provider_key,
            selected_model,
            available_models,
        ))
    }

    /// Alias for `prepare_runtime_session`.
    pub fn prepare_session(
        &self,
        instance_id: &ProviderInstanceId,
        target_model_id: Option<&ModelId>,
    ) -> Result<PreparedRuntimeSession, RuntimeSessionError> {
        self.prepare_runtime_session(instance_id, target_model_id)
    }
}
