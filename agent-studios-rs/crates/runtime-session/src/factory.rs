use std::sync::Arc;

use agent_studios_codex_bridge::{
    CodexBridgeOptions, CodexProviderBridge, deterministic_codex_provider_key,
};
use agent_studios_provider::ProviderCatalog;
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

    /// Returns a reference to the transport options.
    pub fn transport_options(&self) -> &RuntimeTransportOptions {
        &self.transport_options
    }

    /// Prepares a runtime session override for the specified authoritative `ModelRef`.
    ///
    /// - Verifies that the provider instance exists and is enabled.
    /// - Collects all registered model descriptors for this instance.
    /// - Verifies that the target model is explicitly registered for this provider instance.
    /// - Constructs a static, zero-discovery `StaticModelsManager` with scrubbed metadata.
    /// - Configures the provider runtime:
    ///   - For `OpenAiResponses`: resolves via `CodexProviderBridge` with native execution.
    ///   - For custom protocols (`OpenAiChatCompletions`, `AnthropicMessages`, `GeminiGenerateContent`):
    ///     instantiates a session-scoped `RuntimeRouter` registering all instance models.
    ///   - For `Custom(name)`: fails closed with `RuntimeSessionError::UnsupportedProtocol`.
    /// - Isolates custom providers from Codex `AuthManager` to ensure login-independence.
    /// - Strictly pairs `SharedModelProvider` and `SharedModelsManager` in `ModelRuntimeOverride`.
    pub fn prepare_runtime_session(
        &self,
        model_ref: &ModelRef,
    ) -> Result<PreparedRuntimeSession, RuntimeSessionError> {
        let instance_id = model_ref.provider_instance_id();
        let target_model_id = model_ref.model_id();

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

        // 3. Verify target model exists
        let found = models.iter().find(|m| &m.id == target_model_id);
        if found.is_none() {
            return Err(RuntimeSessionError::ModelNotFound {
                model_id: target_model_id.clone(),
                provider_instance_id: *instance_id,
            });
        }

        // 4. Custom providers must be login-independent (auth_manager = None)
        let custom_auth_manager = match &instance.protocol {
            ProtocolFamily::OpenAiResponses => self.auth_manager.clone(),
            _ => None,
        };

        // 5. Build StaticModelsManager (zero discovery, static catalog, scrubbed metadata)
        let models_manager =
            StaticModelsManager::new(custom_auth_manager.clone(), &models)?.into_shared();

        // 6. Deterministic Codex provider key
        let codex_provider_key = deterministic_codex_provider_key(instance_id);

        // 7. Build Provider info & inference backend with explicit protocol matching
        let (provider_info, inference_backend) = match &instance.protocol {
            ProtocolFamily::OpenAiResponses => {
                let binding =
                    CodexProviderBridge::resolve(&self.catalog, model_ref, self.bridge_options)?;
                (binding.provider_info, None)
            }
            ProtocolFamily::OpenAiChatCompletions
            | ProtocolFamily::AnthropicMessages
            | ProtocolFamily::GeminiGenerateContent => {
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
            ProtocolFamily::Custom(name) => {
                return Err(RuntimeSessionError::UnsupportedProtocol(name.clone()));
            }
        };

        let shared_provider = create_model_provider_with_inference_backend(
            provider_info,
            custom_auth_manager,
            inference_backend,
        );

        // 8. Strictly pair provider and models manager in ModelRuntimeOverride
        let runtime_override = ModelRuntimeOverride::new(shared_provider, models_manager);

        let available_models: Vec<String> = models
            .into_iter()
            .map(|m| m.id.as_str().to_string())
            .collect();

        Ok(PreparedRuntimeSession::new(
            runtime_override,
            model_ref.clone(),
            instance.protocol.clone(),
            codex_provider_key,
            available_models,
        ))
    }

    /// Alias for `prepare_runtime_session`.
    pub fn prepare_session(
        &self,
        model_ref: &ModelRef,
    ) -> Result<PreparedRuntimeSession, RuntimeSessionError> {
        self.prepare_runtime_session(model_ref)
    }
}
