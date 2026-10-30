use serde::{Deserialize, Serialize};

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
    pub available_runtimes: Vec<String>,
    pub runtime_instances: Vec<RuntimeInstanceReadiness>,
    pub available_models: Vec<ModelReadiness>,
    pub authorized_workspaces: Vec<String>,
    pub defaults: ExecutionDefaultsReadiness,
    pub blocking_reasons: Vec<String>,
}
