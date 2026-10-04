use agent_studios_protocol::id::AgentId;
use agent_studios_provider::model::ModelRef;
use codex_protocol::openai_models::ReasoningEffort;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentReasoningEffort {
    None,
    Low,
    Medium,
    High,
    XHigh,
}

impl From<AgentReasoningEffort> for Option<ReasoningEffort> {
    fn from(effort: AgentReasoningEffort) -> Self {
        match effort {
            AgentReasoningEffort::None => None,
            AgentReasoningEffort::Low => Some(ReasoningEffort::Low),
            AgentReasoningEffort::Medium => Some(ReasoningEffort::Medium),
            AgentReasoningEffort::High => Some(ReasoningEffort::High),
            AgentReasoningEffort::XHigh => Some(ReasoningEffort::XHigh),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentReasoningSelection {
    pub effort: Option<AgentReasoningEffort>,
    pub summary: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceAccessMode {
    ReadOnly,
    Mutating,
}

impl WorkspaceAccessMode {
    pub fn is_mutating(&self) -> bool {
        matches!(self, Self::Mutating)
    }

    pub fn is_read_only(&self) -> bool {
        matches!(self, Self::ReadOnly)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExecutionBudget {
    pub max_turns: Option<u32>,
    pub max_tool_calls: Option<u32>,
    pub max_wall_clock_secs: Option<u64>,
}

impl Default for AgentExecutionBudget {
    fn default() -> Self {
        Self {
            max_turns: Some(25),
            max_tool_calls: Some(100),
            max_wall_clock_secs: Some(600),
        }
    }
}

impl AgentExecutionBudget {
    pub fn unlimited() -> Self {
        Self {
            max_turns: None,
            max_tool_calls: None,
            max_wall_clock_secs: None,
        }
    }

    pub fn with_turns(mut self, turns: u32) -> Self {
        self.max_turns = Some(turns);
        self
    }

    pub fn with_tool_calls(mut self, tool_calls: u32) -> Self {
        self.max_tool_calls = Some(tool_calls);
        self
    }

    pub fn with_wall_clock_secs(mut self, secs: u64) -> Self {
        self.max_wall_clock_secs = Some(secs);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalAgentSpec {
    pub agent_id: AgentId,
    pub display_name: String,
    pub role: String,
    pub model_ref: ModelRef,
    pub reasoning: Option<AgentReasoningSelection>,
    pub budget: AgentExecutionBudget,
    pub workspace_access: WorkspaceAccessMode,
}

impl InternalAgentSpec {
    pub fn new(
        agent_id: AgentId,
        display_name: impl Into<String>,
        role: impl Into<String>,
        model_ref: ModelRef,
    ) -> Self {
        Self {
            agent_id,
            display_name: display_name.into(),
            role: role.into(),
            model_ref,
            reasoning: None,
            budget: AgentExecutionBudget::default(),
            workspace_access: WorkspaceAccessMode::ReadOnly,
        }
    }

    pub fn with_workspace_access(mut self, mode: WorkspaceAccessMode) -> Self {
        self.workspace_access = mode;
        self
    }

    pub fn with_budget(mut self, budget: AgentExecutionBudget) -> Self {
        self.budget = budget;
        self
    }

    pub fn with_reasoning(mut self, reasoning: AgentReasoningSelection) -> Self {
        self.reasoning = Some(reasoning);
        self
    }
}
