use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::capabilities::RuntimeCapabilities;
use crate::id::RuntimeImplementationId;

/// Result of probing or discovering an external agent runtime implementation in the environment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredRuntime {
    pub implementation_id: RuntimeImplementationId,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_path: Option<PathBuf>,
    pub capabilities: RuntimeCapabilities,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl DiscoveredRuntime {
    /// Creates a record for an available runtime.
    pub fn available(
        implementation_id: RuntimeImplementationId,
        display_name: impl Into<String>,
        capabilities: RuntimeCapabilities,
    ) -> Self {
        Self {
            implementation_id,
            display_name: display_name.into(),
            version: None,
            binary_path: None,
            capabilities,
            available: true,
            unavailable_reason: None,
            metadata: HashMap::new(),
        }
    }

    /// Creates a record for an unavailable runtime with a safe diagnostic reason.
    pub fn unavailable(
        implementation_id: RuntimeImplementationId,
        display_name: impl Into<String>,
        capabilities: RuntimeCapabilities,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            implementation_id,
            display_name: display_name.into(),
            version: None,
            binary_path: None,
            capabilities,
            available: false,
            unavailable_reason: Some(reason.into()),
            metadata: HashMap::new(),
        }
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
