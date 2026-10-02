use std::collections::BTreeMap;

use crate::definition::ProviderDefinition;
use crate::error::ProviderError;
use crate::id::{ModelId, ProviderId, ProviderInstanceId};
use crate::instance::ProviderInstance;
use crate::model::{ModelDescriptor, ModelRef};

/// Single-owner registry managing ProviderDefinitions and ProviderInstances.
///
/// Ensures uniqueness, referential integrity between instances and definitions,
/// and protocol support validation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderRegistry {
    definitions: BTreeMap<ProviderId, ProviderDefinition>,
    instances: BTreeMap<ProviderInstanceId, ProviderInstance>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new provider definition. Fails if ID already exists.
    pub fn register_provider_definition(
        &mut self,
        def: ProviderDefinition,
    ) -> Result<(), ProviderError> {
        if self.definitions.contains_key(&def.id) {
            return Err(ProviderError::DuplicateProviderDefinition(def.id));
        }
        def.validate()?;
        self.definitions.insert(def.id.clone(), def);
        Ok(())
    }

    /// Looks up a provider definition by ID.
    pub fn get_provider_definition(&self, id: &ProviderId) -> Option<&ProviderDefinition> {
        self.definitions.get(id)
    }

    /// Checks if a provider definition exists.
    pub fn contains_definition(&self, id: &ProviderId) -> bool {
        self.definitions.contains_key(id)
    }

    /// Removes a provider definition. Rejects if any configured instances reference it.
    pub fn remove_provider_definition(
        &mut self,
        id: &ProviderId,
    ) -> Result<ProviderDefinition, ProviderError> {
        if !self.definitions.contains_key(id) {
            return Err(ProviderError::UnknownProviderDefinition(id.clone()));
        }

        let instance_count = self
            .instances
            .values()
            .filter(|inst| &inst.provider_id == id)
            .count();

        if instance_count > 0 {
            return Err(ProviderError::ProviderDefinitionInUse {
                provider_id: id.clone(),
                instance_count,
            });
        }

        let def = self.definitions.remove(id).expect("checked presence above");
        Ok(def)
    }

    /// Registers a new provider instance. Validates:
    /// - Instance ID uniqueness
    /// - Referenced ProviderDefinition existence
    /// - Protocol is in definition's supported_protocols
    /// - Endpoint and authentication validity
    pub fn register_provider_instance(
        &mut self,
        instance: ProviderInstance,
    ) -> Result<(), ProviderError> {
        if self.instances.contains_key(&instance.id) {
            return Err(ProviderError::DuplicateProviderInstance(instance.id));
        }

        let def = self.definitions.get(&instance.provider_id).ok_or_else(|| {
            ProviderError::UnknownProviderDefinition(instance.provider_id.clone())
        })?;

        if !def.supports_protocol(&instance.protocol) {
            return Err(ProviderError::UnsupportedProtocol {
                provider_id: instance.provider_id.clone(),
                protocol: instance.protocol.clone(),
            });
        }

        instance.validate()?;
        self.instances.insert(instance.id, instance);
        Ok(())
    }

    /// Looks up a provider instance by ID.
    pub fn get_provider_instance(&self, id: &ProviderInstanceId) -> Option<&ProviderInstance> {
        self.instances.get(id)
    }

    /// Checks if a provider instance exists.
    pub fn contains_instance(&self, id: &ProviderInstanceId) -> bool {
        self.instances.contains_key(id)
    }

    /// Updates an existing provider instance. Validates protocol and endpoint invariants.
    pub fn update_provider_instance(
        &mut self,
        instance: ProviderInstance,
    ) -> Result<(), ProviderError> {
        if !self.instances.contains_key(&instance.id) {
            return Err(ProviderError::UnknownProviderInstance(instance.id));
        }

        let def = self.definitions.get(&instance.provider_id).ok_or_else(|| {
            ProviderError::UnknownProviderDefinition(instance.provider_id.clone())
        })?;

        if !def.supports_protocol(&instance.protocol) {
            return Err(ProviderError::UnsupportedProtocol {
                provider_id: instance.provider_id.clone(),
                protocol: instance.protocol.clone(),
            });
        }

        instance.validate()?;
        self.instances.insert(instance.id, instance);
        Ok(())
    }

    /// Removes a provider instance by ID.
    pub fn remove_provider_instance(
        &mut self,
        id: &ProviderInstanceId,
    ) -> Result<ProviderInstance, ProviderError> {
        self.instances
            .remove(id)
            .ok_or(ProviderError::UnknownProviderInstance(*id))
    }

    /// Lists all registered provider definitions in deterministic order.
    pub fn list_provider_definitions(&self) -> Vec<&ProviderDefinition> {
        self.definitions.values().collect()
    }

    /// Lists all registered provider instances in deterministic order.
    pub fn list_provider_instances(&self) -> Vec<&ProviderInstance> {
        self.instances.values().collect()
    }
}

/// Single-owner registry managing ModelDescriptors scoped to ProviderInstances.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelRegistry {
    models: BTreeMap<(ProviderInstanceId, ModelId), ModelDescriptor>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a model descriptor. Rejects duplicate model under the same provider instance.
    pub fn register_model(&mut self, descriptor: ModelDescriptor) -> Result<(), ProviderError> {
        let key = (descriptor.provider_instance_id, descriptor.id.clone());
        if self.models.contains_key(&key) {
            return Err(ProviderError::DuplicateModel {
                instance_id: descriptor.provider_instance_id,
                model_id: descriptor.id,
            });
        }
        descriptor.validate()?;
        self.models.insert(key, descriptor);
        Ok(())
    }

    /// Looks up a model descriptor by instance and model ID.
    pub fn get_model(
        &self,
        instance_id: &ProviderInstanceId,
        model_id: &ModelId,
    ) -> Option<&ModelDescriptor> {
        self.models.get(&(*instance_id, model_id.clone()))
    }

    /// Resolves a ModelRef to its descriptor.
    pub fn resolve(&self, model_ref: &ModelRef) -> Option<&ModelDescriptor> {
        self.get_model(&model_ref.provider_instance_id, &model_ref.model_id)
    }

    /// Removes a model descriptor.
    pub fn remove_model(
        &mut self,
        instance_id: &ProviderInstanceId,
        model_id: &ModelId,
    ) -> Result<ModelDescriptor, ProviderError> {
        let key = (*instance_id, model_id.clone());
        self.models.remove(&key).ok_or(ProviderError::UnknownModel {
            instance_id: *instance_id,
            model_id: model_id.clone(),
        })
    }

    /// Returns all models registered under a specific provider instance.
    pub fn models_for_provider_instance(
        &self,
        instance_id: &ProviderInstanceId,
    ) -> Vec<&ModelDescriptor> {
        self.models
            .iter()
            .filter(|((inst, _), _)| inst == instance_id)
            .map(|(_, desc)| desc)
            .collect()
    }

    /// Returns the number of models registered under a specific provider instance.
    pub fn model_count_for_instance(&self, instance_id: &ProviderInstanceId) -> usize {
        self.models
            .keys()
            .filter(|(inst, _)| inst == instance_id)
            .count()
    }

    /// Checks if a model exists under a provider instance.
    pub fn contains(&self, instance_id: &ProviderInstanceId, model_id: &ModelId) -> bool {
        self.models.contains_key(&(*instance_id, model_id.clone()))
    }

    /// Lists all models in deterministic order.
    pub fn list_models(&self) -> Vec<&ModelDescriptor> {
        self.models.values().collect()
    }
}
