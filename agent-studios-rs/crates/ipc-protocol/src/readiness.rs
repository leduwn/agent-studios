use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Readiness state of a discovered external runtime instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInstanceReadiness {
    pub instance_id: String,
    pub implementation: String,
    pub healthy: bool,
}

/// Readiness state of a registered model available on a provider instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelReadiness {
    pub provider_instance_id: String,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Configured execution defaults on the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExecutionDefaultsReadiness {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_repo_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_provider_instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configured_instance_id: Option<String>,
}

/// Truthful non-secret daemon execution readiness report (M15).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExecutionReadinessReport {
    pub is_ready: bool,
    /// Implementations installed in the registry, even if no instance is healthy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub registered_runtimes: Vec<String>,
    /// Implementations with at least one healthy discovered instance.
    pub available_runtimes: Vec<String>,
    /// Instance ID to tri-state capability support (supported/unsupported/unknown).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub runtime_capabilities: BTreeMap<String, BTreeMap<String, String>>,
    pub runtime_instances: Vec<RuntimeInstanceReadiness>,
    pub available_models: Vec<ModelReadiness>,
    pub authorized_workspaces: Vec<String>,
    pub defaults: ExecutionDefaultsReadiness,
    pub blocking_reasons: Vec<String>,
}
