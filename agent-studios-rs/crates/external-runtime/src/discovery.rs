use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::capabilities::RuntimeCapabilities;
use crate::error::{RuntimeError, SanitizedRuntimeMessage};
use crate::handle::RuntimeStartRequest;
use crate::id::{RuntimeConfigRef, RuntimeImplementationId, RuntimeInstanceId};

/// Strongly-typed availability state for a discovered runtime instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RuntimeAvailability {
    Available,
    Unavailable { reason: SanitizedRuntimeMessage },
}

impl RuntimeAvailability {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    pub fn unavailable_reason(&self) -> Option<&SanitizedRuntimeMessage> {
        match self {
            Self::Available => None,
            Self::Unavailable { reason } => Some(reason),
        }
    }
}

/// Result of probing or discovering an external agent runtime instance in the environment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredRuntimeInstance {
    pub instance_id: RuntimeInstanceId,
    pub implementation_id: RuntimeImplementationId,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_path: Option<PathBuf>,
    pub capabilities: RuntimeCapabilities,
    pub availability: RuntimeAvailability,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_configs: Vec<RuntimeConfigRef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl DiscoveredRuntimeInstance {
    /// Creates a record for an available runtime instance.
    pub fn available(
        instance_id: RuntimeInstanceId,
        implementation_id: RuntimeImplementationId,
        display_name: impl Into<String>,
        capabilities: RuntimeCapabilities,
    ) -> Self {
        Self {
            instance_id,
            implementation_id,
            display_name: display_name.into(),
            version: None,
            binary_path: None,
            capabilities,
            availability: RuntimeAvailability::Available,
            supported_configs: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    /// Creates a record for an unavailable runtime instance with a safe diagnostic reason.
    pub fn unavailable(
        instance_id: RuntimeInstanceId,
        implementation_id: RuntimeImplementationId,
        display_name: impl Into<String>,
        capabilities: RuntimeCapabilities,
        reason: impl Into<SanitizedRuntimeMessage>,
    ) -> Self {
        Self {
            instance_id,
            implementation_id,
            display_name: display_name.into(),
            version: None,
            binary_path: None,
            capabilities,
            availability: RuntimeAvailability::Unavailable {
                reason: reason.into(),
            },
            supported_configs: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn is_available(&self) -> bool {
        self.availability.is_available()
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn with_binary_path(mut self, path: PathBuf) -> Self {
        self.binary_path = Some(path);
        self
    }

    pub fn with_supported_config(mut self, config: RuntimeConfigRef) -> Self {
        self.supported_configs.push(config);
        self
    }

    pub fn with_supported_configs(
        mut self,
        configs: impl IntoIterator<Item = RuntimeConfigRef>,
    ) -> Self {
        self.supported_configs.extend(configs);
        self
    }

    pub fn supports_config(&self, config_ref: &RuntimeConfigRef) -> bool {
        self.supported_configs.contains(config_ref)
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Validates that this discovered instance satisfies all start-time preconditions
    /// for the given implementation and start request.
    pub fn validate_start_request(
        &self,
        expected_impl: &RuntimeImplementationId,
        request: &RuntimeStartRequest,
    ) -> Result<(), RuntimeError> {
        if self.implementation_id != *expected_impl {
            return Err(RuntimeError::InstanceImplementationMismatch {
                expected: expected_impl.clone(),
                actual: self.implementation_id.clone(),
                instance_id: self.instance_id,
            });
        }
        if *request.implementation_id() != *expected_impl {
            return Err(RuntimeError::InstanceImplementationMismatch {
                expected: expected_impl.clone(),
                actual: request.implementation_id().clone(),
                instance_id: *request.instance_id(),
            });
        }
        if self.instance_id != *request.instance_id() {
            return Err(RuntimeError::UnknownInstance {
                instance_id: *request.instance_id(),
            });
        }
        if let RuntimeAvailability::Unavailable { reason } = &self.availability {
            return Err(RuntimeError::InstanceUnavailable {
                instance_id: self.instance_id,
                reason: reason.clone(),
            });
        }
        if let Some(config_ref) = request
            .runtime_config_ref()
            .filter(|c| !self.supports_config(c))
        {
            return Err(RuntimeError::UnsupportedConfiguration {
                instance_id: self.instance_id,
                config_ref: config_ref.clone(),
            });
        }
        request.validate()?;
        Ok(())
    }
}

/// Standalone reusable production validator checking that a discovered instance can start the requested session,
/// enforcing implementation identity, instance membership, availability, configuration authority, and workspace isolation.
pub fn validate_instance_start(
    instance: &DiscoveredRuntimeInstance,
    expected_impl: &RuntimeImplementationId,
    request: &RuntimeStartRequest,
) -> Result<(), RuntimeError> {
    instance.validate_start_request(expected_impl, request)
}
