use agent_studios_provider::id::ProviderInstanceId;
use agent_studios_provider::model::ModelRef;
use agent_studios_provider::protocol::ProtocolFamily;
use codex_core::ModelRuntimeOverride;
use codex_core::StartThreadOptions;

/// Prepared runtime session artifacts ready for injection into a Codex thread.
///
/// Bundles the paired `ModelRuntimeOverride` (`SharedModelProvider` + `SharedModelsManager`),
/// the authoritative `ModelRef`, the active `ProtocolFamily`, the deterministic Codex provider
/// key, and the full list of available model identifiers registered for the provider instance.
#[derive(Clone)]
pub struct PreparedRuntimeSession {
    runtime_override: ModelRuntimeOverride,
    model_ref: ModelRef,
    protocol: ProtocolFamily,
    model_provider_id: String,
    available_models: Vec<String>,
}

impl std::fmt::Debug for PreparedRuntimeSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedRuntimeSession")
            .field("model_ref", &self.model_ref)
            .field("protocol", &self.protocol)
            .field("model_provider_id", &self.model_provider_id)
            .field("selected_model", &self.selected_model())
            .field("available_models", &self.available_models)
            .finish()
    }
}

impl PreparedRuntimeSession {
    /// Creates a new `PreparedRuntimeSession`.
    pub fn new(
        runtime_override: ModelRuntimeOverride,
        model_ref: ModelRef,
        protocol: ProtocolFamily,
        model_provider_id: impl Into<String>,
        available_models: Vec<String>,
    ) -> Self {
        Self {
            runtime_override,
            model_ref,
            protocol,
            model_provider_id: model_provider_id.into(),
            available_models,
        }
    }

    /// Returns a reference to the generic `ModelRuntimeOverride`.
    pub fn runtime_override(&self) -> &ModelRuntimeOverride {
        &self.runtime_override
    }

    /// Consumes the session wrapper and yields the inner `ModelRuntimeOverride`.
    pub fn into_runtime_override(self) -> ModelRuntimeOverride {
        self.runtime_override
    }

    /// Returns a reference to the authoritative `ModelRef`.
    pub fn model_ref(&self) -> &ModelRef {
        &self.model_ref
    }

    /// Returns a reference to the `ProviderInstanceId` from the authoritative `ModelRef`.
    pub fn provider_instance_id(&self) -> &ProviderInstanceId {
        self.model_ref.provider_instance_id()
    }

    /// Returns a reference to the active `ProtocolFamily`.
    pub fn protocol(&self) -> &ProtocolFamily {
        &self.protocol
    }

    /// Returns the deterministic Codex provider identifier (e.g. `agent-studios-<uuid>`).
    pub fn model_provider_id(&self) -> &str {
        &self.model_provider_id
    }

    /// Returns the selected model slug for this session.
    pub fn selected_model(&self) -> &str {
        self.model_ref.model_id().as_str()
    }

    /// Returns all registered model slugs for the provider instance.
    pub fn available_models(&self) -> &[String] {
        &self.available_models
    }

    /// Injects the runtime override and model configuration into `StartThreadOptions`.
    ///
    /// Sets:
    /// - `options.model_runtime_override = Some(self.runtime_override)`
    /// - `options.config.model_provider_id = self.model_provider_id`
    /// - `options.config.model = Some(self.selected_model().to_string())`
    /// - `options.allow_provider_model_fallback = false`
    pub fn prepare_start_thread_options(&self, options: &mut StartThreadOptions) {
        options.model_runtime_override = Some(self.runtime_override.clone());
        options.config.model_provider_id = self.model_provider_id.clone();
        options.config.model = Some(self.selected_model().to_string());
        options.allow_provider_model_fallback = false;
    }

    /// Alias for `prepare_start_thread_options`.
    pub fn apply_to_start_thread_options(&self, options: &mut StartThreadOptions) {
        self.prepare_start_thread_options(options);
    }
}
