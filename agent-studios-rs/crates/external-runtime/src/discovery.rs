use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::capabilities::RuntimeCapabilities;
use crate::error::SanitizedRuntimeMessage;
use crate::id::{RuntimeImplementationId, RuntimeInstanceId};

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

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}
