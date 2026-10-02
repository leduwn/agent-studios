use crate::builtin::builtin_definitions;
use crate::definition::ProviderDefinition;
use crate::error::ProviderError;
use crate::id::{ModelId, ProviderId, ProviderInstanceId};
use crate::instance::ProviderInstance;
use crate::model::{ModelDescriptor, ModelRef};
use crate::registry::{ModelRegistry, ProviderRegistry};

/// Higher-level facade coordinating ProviderRegistry and ModelRegistry.
///
/// Enforces cross-registry invariants:
/// 1. A Model can only be registered if its ProviderInstance exists.
/// 2. A ProviderInstance cannot be removed while models exist for it (unless explicit cascade).
/// 3. A ProviderDefinition cannot be removed while provider instances reference it.
/// 4. No direct mutable access to internal registries (no `providers_mut` / `models_mut`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderCatalog {
    provider_registry: ProviderRegistry,
    model_registry: ModelRegistry,
}

impl ProviderCatalog {
    /// Creates an empty ProviderCatalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a ProviderCatalog pre-populated with standard built-in provider family definitions.
    pub fn with_builtin_definitions() -> Self {
        let mut catalog = Self::new();
        for def in builtin_definitions() {
            catalog
                .register_provider_definition(def)
                .expect("builtin definitions must be valid");
        }
        catalog
    }

    /// Read-only reference to the provider registry.
    pub fn provider_registry(&self) -> &ProviderRegistry {
        &self.provider_registry
    }

    /// Read-only reference to the model registry.
    pub fn model_registry(&self) -> &ModelRegistry {
        &self.model_registry
    }

    // ------------------------------------------------------------------------
    // Provider Definition Operations
    // ------------------------------------------------------------------------

    pub fn register_provider_definition(
        &mut self,
        def: ProviderDefinition,
    ) -> Result<(), ProviderError> {
        self.provider_registry.register_provider_definition(def)
    }

    pub fn get_provider_definition(&self, id: &ProviderId) -> Option<&ProviderDefinition> {
        self.provider_registry.get_provider_definition(id)
    }

    pub fn remove_provider_definition(
        &mut self,
        id: &ProviderId,
    ) -> Result<ProviderDefinition, ProviderError> {
        self.provider_registry.remove_provider_definition(id)
    }

    pub fn list_provider_definitions(&self) -> Vec<&ProviderDefinition> {
        self.provider_registry.list_provider_definitions()
    }

    // ------------------------------------------------------------------------
    // Provider Instance Operations
    // ------------------------------------------------------------------------

    pub fn register_provider_instance(
        &mut self,
        instance: ProviderInstance,
    ) -> Result<(), ProviderError> {
        self.provider_registry.register_provider_instance(instance)
    }

    pub fn get_provider_instance(&self, id: &ProviderInstanceId) -> Option<&ProviderInstance> {
        self.provider_registry.get_provider_instance(id)
    }

    pub fn update_provider_instance(
        &mut self,
        instance: ProviderInstance,
    ) -> Result<(), ProviderError> {
        self.provider_registry.update_provider_instance(instance)
    }

    /// Safely removes a provider instance.
    ///
    /// Invariant: Rejects removal with `ProviderInstanceInUse` if any models
    /// are registered under this instance.
    pub fn remove_provider_instance(
        &mut self,
        id: &ProviderInstanceId,
    ) -> Result<ProviderInstance, ProviderError> {
        let model_count = self.model_registry.model_count_for_instance(id);
        if model_count > 0 {
            return Err(ProviderError::ProviderInstanceInUse {
                instance_id: *id,
                model_count,
            });
        }
        self.provider_registry.remove_provider_instance(id)
    }

    /// Explicit cascade removal of a provider instance and all its associated models.
    pub fn remove_provider_instance_cascade(
        &mut self,
        id: &ProviderInstanceId,
    ) -> Result<(ProviderInstance, Vec<ModelDescriptor>), ProviderError> {
        // Collect model IDs to remove
        let model_ids: Vec<ModelId> = self
            .model_registry
            .models_for_provider_instance(id)
            .iter()
            .map(|m| m.id.clone())
            .collect();

        let mut removed_models = Vec::new();
        for mid in model_ids {
            let model = self
                .model_registry
                .remove_model(id, &mid)
                .expect("model was just queried");
            removed_models.push(model);
        }

        let instance = self.provider_registry.remove_provider_instance(id)?;
        Ok((instance, removed_models))
    }

    pub fn list_provider_instances(&self) -> Vec<&ProviderInstance> {
        self.provider_registry.list_provider_instances()
    }

    // ------------------------------------------------------------------------
    // Model Operations
    // ------------------------------------------------------------------------

    /// Registers a model descriptor.
    ///
    /// Invariant: Verifies that the referenced `ProviderInstance` exists in the catalog.
    pub fn register_model(&mut self, descriptor: ModelDescriptor) -> Result<(), ProviderError> {
        if !self
            .provider_registry
            .contains_instance(&descriptor.provider_instance_id)
        {
            return Err(ProviderError::UnknownProviderInstance(
                descriptor.provider_instance_id,
            ));
        }
        self.model_registry.register_model(descriptor)
    }

    pub fn get_model(
        &self,
        instance_id: &ProviderInstanceId,
        model_id: &ModelId,
    ) -> Option<&ModelDescriptor> {
        self.model_registry.get_model(instance_id, model_id)
    }

    pub fn resolve_model(&self, model_ref: &ModelRef) -> Option<&ModelDescriptor> {
        self.model_registry.resolve(model_ref)
    }

    pub fn remove_model(
        &mut self,
        instance_id: &ProviderInstanceId,
        model_id: &ModelId,
    ) -> Result<ModelDescriptor, ProviderError> {
        self.model_registry.remove_model(instance_id, model_id)
    }

    pub fn models_for_provider_instance(
        &self,
        instance_id: &ProviderInstanceId,
    ) -> Vec<&ModelDescriptor> {
        self.model_registry
            .models_for_provider_instance(instance_id)
    }

    pub fn list_models(&self) -> Vec<&ModelDescriptor> {
        self.model_registry.list_models()
    }
}
