use codex_core::ModelRuntimeOverride;
use codex_core::StartThreadOptions;

/// Prepared runtime session artifacts ready for injection into a Codex thread.
///
/// Bundles the paired `ModelRuntimeOverride` (`SharedModelProvider` + `SharedModelsManager`),
/// the deterministic Codex provider key, the selected model slug, and the full list of
/// available model identifiers registered for the provider instance.
#[derive(Debug, Clone)]
pub struct PreparedRuntimeSession {
    runtime_override: ModelRuntimeOverride,
    model_provider_id: String,
    selected_model: String,
    available_models: Vec<String>,
}

impl PreparedRuntimeSession {
    /// Creates a new `PreparedRuntimeSession`.
    pub fn new(
        runtime_override: ModelRuntimeOverride,
        model_provider_id: impl Into<String>,
        selected_model: impl Into<String>,
        available_models: Vec<String>,
    ) -> Self {
        Self {
            runtime_override,
            model_provider_id: model_provider_id.into(),
            selected_model: selected_model.into(),
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

    /// Returns the deterministic Codex provider identifier (e.g. `agent-studios-<uuid>`).
    pub fn model_provider_id(&self) -> &str {
        &self.model_provider_id
    }

    /// Returns the selected model slug for this session.
    pub fn selected_model(&self) -> &str {
        &self.selected_model
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
    /// - `options.config.model = Some(self.selected_model)`
    /// - `options.allow_provider_model_fallback = false`
    pub fn prepare_start_thread_options(&self, options: &mut StartThreadOptions) {
        options.model_runtime_override = Some(self.runtime_override.clone());
        options.config.model_provider_id = self.model_provider_id.clone();
        options.config.model = Some(self.selected_model.clone());
        options.allow_provider_model_fallback = false;
    }

    /// Alias for `prepare_start_thread_options`.
    pub fn apply_to_start_thread_options(&self, options: &mut StartThreadOptions) {
        self.prepare_start_thread_options(options);
    }
}
