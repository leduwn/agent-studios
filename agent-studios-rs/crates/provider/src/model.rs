use std::fmt;

use serde::{Deserialize, Serialize};

use crate::capabilities::ModelCapabilities;
use crate::error::ProviderError;
use crate::id::{ModelId, ProviderInstanceId};

/// Token context and output limits for a model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModelLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
}

impl ModelLimits {
    pub fn new(
        context_window_tokens: Option<u64>,
        max_output_tokens: Option<u64>,
    ) -> Result<Self, ProviderError> {
        let limits = Self {
            context_window_tokens,
            max_output_tokens,
        };
        limits.validate()?;
        Ok(limits)
    }

    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.context_window_tokens == Some(0) {
            return Err(ProviderError::InvalidModelLimits(
                "context_window_tokens cannot be explicit 0 (use None if unknown)".to_string(),
            ));
        }
        if self.max_output_tokens == Some(0) {
            return Err(ProviderError::InvalidModelLimits(
                "max_output_tokens cannot be explicit 0 (use None if unknown)".to_string(),
            ));
        }
        Ok(())
    }
}

/// Origin of model metadata descriptor.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelMetadataSource {
    #[default]
    Manual,
    StaticCatalog,
    ProviderDiscovery,
}

impl fmt::Display for ModelMetadataSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manual => write!(f, "manual"),
            Self::StaticCatalog => write!(f, "static_catalog"),
            Self::ProviderDiscovery => write!(f, "provider_discovery"),
        }
    }
}

/// Provider-neutral descriptor for a concrete model scoped to a ProviderInstance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDescriptor {
    pub provider_instance_id: ProviderInstanceId,
    pub id: ModelId,
    pub display_name: String,
    pub capabilities: ModelCapabilities,
    pub limits: ModelLimits,
    #[serde(default)]
    pub metadata_source: ModelMetadataSource,
}

impl ModelDescriptor {
    pub fn new(
        provider_instance_id: ProviderInstanceId,
        id: ModelId,
        display_name: impl Into<String>,
        capabilities: ModelCapabilities,
        limits: ModelLimits,
    ) -> Result<Self, ProviderError> {
        let display_name = display_name.into();
        let trimmed = display_name.trim();
        if trimmed.is_empty() {
            return Err(ProviderError::InvalidModelLimits(
                "Model display_name cannot be empty".to_string(),
            ));
        }
        limits.validate()?;
        Ok(Self {
            provider_instance_id,
            id,
            display_name: trimmed.to_string(),
            capabilities,
            limits,
            metadata_source: ModelMetadataSource::Manual,
        })
    }

    /// Returns the compact ModelRef for routing.
    pub fn model_ref(&self) -> ModelRef {
        ModelRef {
            provider_instance_id: self.provider_instance_id,
            model_id: self.id.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.display_name.trim().is_empty() {
            return Err(ProviderError::InvalidModelLimits(
                "Model display_name cannot be empty".to_string(),
            ));
        }
        self.limits.validate()?;
        Ok(())
    }
}

/// Compact composite identifier referencing a specific model under a specific provider instance.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ModelRef {
    pub provider_instance_id: ProviderInstanceId,
    pub model_id: ModelId,
}

impl ModelRef {
    pub fn new(provider_instance_id: ProviderInstanceId, model_id: ModelId) -> Self {
        Self {
            provider_instance_id,
            model_id,
        }
    }

    pub fn provider_instance_id(&self) -> &ProviderInstanceId {
        &self.provider_instance_id
    }

    pub fn model_id(&self) -> &ModelId {
        &self.model_id
    }
}

impl fmt::Display for ModelRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.provider_instance_id, self.model_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_limits_validation() {
        assert!(ModelLimits::new(None, None).is_ok());
        assert!(ModelLimits::new(Some(128_000), Some(4_096)).is_ok());
        assert!(ModelLimits::new(Some(0), Some(4_096)).is_err());
        assert!(ModelLimits::new(Some(128_000), Some(0)).is_err());
    }

    #[test]
    fn test_model_descriptor_and_ref() {
        let inst_id = ProviderInstanceId::new();
        let model_id = ModelId::new("gpt-5.6").unwrap();
        let desc = ModelDescriptor::new(
            inst_id,
            model_id.clone(),
            "GPT 5.6",
            ModelCapabilities::unknown(),
            ModelLimits::new(Some(200_000), Some(8_192)).unwrap(),
        )
        .unwrap();

        let mref = desc.model_ref();
        assert_eq!(mref.provider_instance_id, inst_id);
        assert_eq!(mref.model_id, model_id);
        assert_eq!(mref.to_string(), format!("{inst_id}/gpt-5.6"));
    }

    #[test]
    fn test_model_ref_serde() {
        let inst_id = ProviderInstanceId::new();
        let model_id = ModelId::new("claude-sonnet-4-5").unwrap();
        let mref = ModelRef::new(inst_id, model_id);

        let json = serde_json::to_string(&mref).unwrap();
        let de: ModelRef = serde_json::from_str(&json).unwrap();
        assert_eq!(mref, de);
    }
}
