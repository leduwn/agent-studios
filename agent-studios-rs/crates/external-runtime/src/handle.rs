use std::collections::BTreeMap;
use std::fmt;

use agent_studios_protocol::agent::AgentExecutionBudget;
use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId};
use agent_studios_protocol::worktree::ExecutionWorkspace;
use agent_studios_provider::{ModelRef, SecretReference};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::capabilities::RuntimeCapabilities;
use crate::error::RuntimeError;
use crate::id::{RuntimeConfigRef, RuntimeImplementationId, RuntimeInstanceId, RuntimeSessionId};
use crate::lifecycle::RuntimeLifecycleState;

/// Forbidden keyword substrings in non-secret values and variable names.
pub const FORBIDDEN_SECRET_KEYWORDS: &[&str] = &[
    "KEY", "TOKEN", "SECRET", "PASSWORD", "PASSWD", "AUTH", "BEARER",
];

/// Strongly-typed non-secret value for environment variables and metadata.
/// Rejects common credential patterns, API key prefixes, and prohibited keywords.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd, Default)]
pub struct NonSecretValue(String);

impl NonSecretValue {
    pub fn new(val: impl Into<String>) -> Result<Self, RuntimeError> {
        let val = val.into();
        let upper = val.to_uppercase();
        for keyword in FORBIDDEN_SECRET_KEYWORDS {
            if upper.contains(keyword) {
                return Err(RuntimeError::invalid_configuration(format!(
                    "Plaintext credential detected in non-secret value: contains forbidden keyword '{keyword}'"
                )));
            }
        }
        for prefix in &["sk-", "ghp_", "gho_", "xoxb-", "xoxp-", "glpat-", "npm_"] {
            if val.contains(prefix) {
                return Err(RuntimeError::invalid_configuration(format!(
                    "Plaintext API token or key prefix '{prefix}' detected in non-secret value"
                )));
            }
        }
        if upper.contains("BEARER ") {
            return Err(RuntimeError::invalid_configuration(
                "Plaintext Bearer token detected in non-secret value",
            ));
        }
        Ok(Self(val))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl Serialize for NonSecretValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for NonSecretValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for NonSecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for NonSecretValue {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Workspace access mode declaring whether the external runtime is permitted to mutate project source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceAccessMode {
    #[default]
    ReadOnly,
    Mutating,
}

/// Strongly-typed reference to a configured runtime instance, unifying implementation ID,
/// authoritative instance ID, and optional configuration reference.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RuntimeInstanceRef {
    pub implementation_id: RuntimeImplementationId,
    pub instance_id: RuntimeInstanceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_ref: Option<RuntimeConfigRef>,
}

impl RuntimeInstanceRef {
    pub fn new(implementation_id: RuntimeImplementationId, instance_id: RuntimeInstanceId) -> Self {
        Self {
            implementation_id,
            instance_id,
            config_ref: None,
        }
    }

    pub fn with_config_ref(mut self, config_ref: RuntimeConfigRef) -> Self {
        self.config_ref = Some(config_ref);
        self
    }
}

/// Three-tuple reference defining authoritative runtime session ownership.
/// Prevents cross-instance and cross-implementation session confusion across all lifecycle operations.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RuntimeSessionRef {
    pub implementation_id: RuntimeImplementationId,
    pub instance_id: RuntimeInstanceId,
    pub session_id: RuntimeSessionId,
}

impl RuntimeSessionRef {
    pub fn new(
        implementation_id: RuntimeImplementationId,
        instance_id: RuntimeInstanceId,
        session_id: RuntimeSessionId,
    ) -> Self {
        Self {
            implementation_id,
            instance_id,
            session_id,
        }
    }
}

/// Source for an environment variable injected into an external runtime session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EnvironmentBindingSource {
    Literal { value: NonSecretValue },
    Secret { secret: SecretReference },
}

/// Typed environment variable binding replacing raw plaintext credential maps.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentVariableBinding {
    pub name: String,
    pub source: EnvironmentBindingSource,
}

impl EnvironmentVariableBinding {
    pub fn literal(
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, RuntimeError> {
        let name = name.into();
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(RuntimeError::invalid_configuration(
                "Environment variable name cannot be empty",
            ));
        }
        let upper = trimmed.to_uppercase();
        for keyword in FORBIDDEN_SECRET_KEYWORDS {
            if upper.contains(keyword) {
                return Err(RuntimeError::invalid_configuration(format!(
                    "Environment variable name '{trimmed}' contains forbidden credential keyword '{keyword}'; secret bindings must use EnvironmentBindingSource::Secret"
                )));
            }
        }
        let non_secret_val = NonSecretValue::new(value)?;
        Ok(Self {
            name: trimmed.to_string(),
            source: EnvironmentBindingSource::Literal {
                value: non_secret_val,
            },
        })
    }

    pub fn secret(name: impl Into<String>, secret: SecretReference) -> Result<Self, RuntimeError> {
        let name = name.into();
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(RuntimeError::invalid_configuration(
                "Environment variable name cannot be empty",
            ));
        }
        secret
            .validate()
            .map_err(|e| RuntimeError::invalid_configuration(e.to_string()))?;
        Ok(Self {
            name: trimmed.to_string(),
            source: EnvironmentBindingSource::Secret { secret },
        })
    }
}

/// Correlation metadata linking an external runtime session to Agent Studios control plane entities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeCorrelation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub studio_id: Option<StudioId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
}

impl RuntimeCorrelation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_studio_id(mut self, studio_id: StudioId) -> Self {
        self.studio_id = Some(studio_id);
        self
    }

    pub fn with_run_id(mut self, run_id: RunId) -> Self {
        self.run_id = Some(run_id);
        self
    }

    pub fn with_task_id(mut self, task_id: TaskId) -> Self {
        self.task_id = Some(task_id);
        self
    }

    pub fn with_agent_id(mut self, agent_id: AgentId) -> Self {
        self.agent_id = Some(agent_id);
        self
    }
}

/// Request to start an external agent runtime session.
/// Never embeds raw secrets directly in public serializable request structures.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeStartRequest {
    pub instance_ref: RuntimeInstanceRef,
    pub workspace: ExecutionWorkspace,
    #[serde(default)]
    pub workspace_access_mode: WorkspaceAccessMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<ModelRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment_bindings: Vec<EnvironmentVariableBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation: Option<RuntimeCorrelation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<AgentExecutionBudget>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, NonSecretValue>,
}

impl RuntimeStartRequest {
    pub fn new(instance_ref: RuntimeInstanceRef, workspace: ExecutionWorkspace) -> Self {
        Self {
            instance_ref,
            workspace,
            workspace_access_mode: WorkspaceAccessMode::default(),
            initial_prompt: None,
            model_ref: None,
            environment_bindings: Vec::new(),
            correlation: None,
            budget: None,
            metadata: BTreeMap::new(),
        }
    }

    pub fn instance_id(&self) -> &RuntimeInstanceId {
        &self.instance_ref.instance_id
    }

    pub fn implementation_id(&self) -> &RuntimeImplementationId {
        &self.instance_ref.implementation_id
    }

    pub fn runtime_config_ref(&self) -> Option<&RuntimeConfigRef> {
        self.instance_ref.config_ref.as_ref()
    }

    pub fn config_ref(&self) -> Option<&RuntimeConfigRef> {
        self.runtime_config_ref()
    }

    pub fn with_workspace_access_mode(mut self, mode: WorkspaceAccessMode) -> Self {
        self.workspace_access_mode = mode;
        self
    }

    pub fn with_initial_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.initial_prompt = Some(prompt.into());
        self
    }

    pub fn with_model_ref(mut self, model_ref: ModelRef) -> Self {
        self.model_ref = Some(model_ref);
        self
    }

    pub fn with_correlation(mut self, correlation: RuntimeCorrelation) -> Self {
        self.correlation = Some(correlation);
        self
    }

    pub fn with_budget(mut self, budget: AgentExecutionBudget) -> Self {
        self.budget = Some(budget);
        self
    }

    pub fn with_literal_env(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, RuntimeError> {
        let binding = EnvironmentVariableBinding::literal(name, value)?;
        self.environment_bindings.push(binding);
        Ok(self)
    }

    pub fn with_secret_env(
        mut self,
        name: impl Into<String>,
        secret: SecretReference,
    ) -> Result<Self, RuntimeError> {
        let binding = EnvironmentVariableBinding::secret(name, secret)?;
        self.environment_bindings.push(binding);
        Ok(self)
    }

    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, RuntimeError> {
        let key = key.into();
        let trimmed_key = key.trim();
        if trimmed_key.is_empty() {
            return Err(RuntimeError::invalid_configuration(
                "Metadata key cannot be empty",
            ));
        }
        let upper_key = trimmed_key.to_uppercase();
        for keyword in FORBIDDEN_SECRET_KEYWORDS {
            if upper_key.contains(keyword) {
                return Err(RuntimeError::invalid_configuration(format!(
                    "Metadata key '{trimmed_key}' contains forbidden credential keyword '{keyword}'"
                )));
            }
        }
        let non_secret_value = NonSecretValue::new(value)?;
        self.metadata
            .insert(trimmed_key.to_string(), non_secret_value);
        Ok(self)
    }

    /// Validates start request invariants, enforcing workspace isolation rules.
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.workspace_access_mode == WorkspaceAccessMode::Mutating
            && matches!(self.workspace, ExecutionWorkspace::SharedSource { .. })
        {
            return Err(RuntimeError::invalid_configuration(
                "ExecutionWorkspace::SharedSource is forbidden for mutating execution; use ExecutionWorkspace::Managed or configure WorkspaceAccessMode::ReadOnly",
            ));
        }
        Ok(())
    }
}

/// Authoritative handle identifying an active external runtime session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSessionHandle {
    pub session_id: RuntimeSessionId,
    pub instance_id: RuntimeInstanceId,
    pub implementation_id: RuntimeImplementationId,
    pub state: RuntimeLifecycleState,
    pub workspace: ExecutionWorkspace,
    pub workspace_access_mode: WorkspaceAccessMode,
    pub capabilities: RuntimeCapabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation: Option<RuntimeCorrelation>,
    pub created_at: DateTime<Utc>,
}

impl RuntimeSessionHandle {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: RuntimeSessionId,
        instance_id: RuntimeInstanceId,
        implementation_id: RuntimeImplementationId,
        state: RuntimeLifecycleState,
        workspace: ExecutionWorkspace,
        workspace_access_mode: WorkspaceAccessMode,
        capabilities: RuntimeCapabilities,
        correlation: Option<RuntimeCorrelation>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            session_id,
            instance_id,
            implementation_id,
            state,
            workspace,
            workspace_access_mode,
            capabilities,
            correlation,
            created_at,
        }
    }

    pub fn session_ref(&self) -> RuntimeSessionRef {
        RuntimeSessionRef::new(
            self.implementation_id.clone(),
            self.instance_id,
            self.session_id,
        )
    }

    pub fn instance_ref(&self) -> RuntimeInstanceRef {
        RuntimeInstanceRef::new(self.implementation_id.clone(), self.instance_id)
    }
}
