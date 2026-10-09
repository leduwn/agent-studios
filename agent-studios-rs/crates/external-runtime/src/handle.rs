use std::collections::HashMap;

use agent_studios_protocol::agent::AgentExecutionBudget;
use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId};
use agent_studios_protocol::worktree::ExecutionWorkspace;
use agent_studios_provider::ModelRef;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::{RuntimeImplementationId, RuntimeInstanceId, RuntimeSessionId};
use crate::lifecycle::RuntimeLifecycleState;

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
    pub instance_id: RuntimeInstanceId,
    pub implementation_id: RuntimeImplementationId,
    pub workspace: ExecutionWorkspace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_config_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<ModelRef>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub environment: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation: Option<RuntimeCorrelation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<AgentExecutionBudget>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl RuntimeStartRequest {
    pub fn new(
        instance_id: RuntimeInstanceId,
        implementation_id: RuntimeImplementationId,
        workspace: ExecutionWorkspace,
    ) -> Self {
        Self {
            instance_id,
            implementation_id,
            workspace,
            initial_prompt: None,
            runtime_config_ref: None,
            model_ref: None,
            environment: HashMap::new(),
            correlation: None,
            budget: None,
            metadata: HashMap::new(),
        }
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

    pub fn with_env_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment.insert(key.into(), value.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation: Option<RuntimeCorrelation>,
    pub created_at: DateTime<Utc>,
}

impl RuntimeSessionHandle {
    pub fn new(
        session_id: RuntimeSessionId,
        instance_id: RuntimeInstanceId,
        implementation_id: RuntimeImplementationId,
        state: RuntimeLifecycleState,
        workspace: ExecutionWorkspace,
        correlation: Option<RuntimeCorrelation>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            session_id,
            instance_id,
            implementation_id,
            state,
            workspace,
            correlation,
            created_at,
        }
    }
}
