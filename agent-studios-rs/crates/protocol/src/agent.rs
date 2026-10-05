use serde::{Deserialize, Serialize};

use crate::error::TransitionError;
use crate::id::{AgentId, StudioId};

/// Distinguishes between native Codex-derived agents and externally adapted agent runtimes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Internal,
    External,
}

/// Operational state of an agent within a Studio.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Registered,
    Starting,
    Idle,
    Busy,
    Paused,
    Stopping,
    Stopped,
    Failed,
}

impl AgentState {
    pub const fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Registered | Self::Starting | Self::Idle | Self::Busy | Self::Paused
        )
    }

    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Stopped | Self::Failed)
    }

    pub const fn can_transition_to(&self, target: Self) -> bool {
        if self.is_terminal() {
            return false;
        }

        matches!(
            (self, target),
            (
                Self::Registered,
                Self::Starting | Self::Stopped | Self::Failed
            ) | (Self::Starting, Self::Idle | Self::Stopping | Self::Failed)
                | (Self::Idle, Self::Busy | Self::Stopping | Self::Failed)
                | (
                    Self::Busy,
                    Self::Idle | Self::Paused | Self::Stopping | Self::Failed
                )
                | (
                    Self::Paused,
                    Self::Busy | Self::Idle | Self::Stopping | Self::Failed
                )
                | (Self::Stopping, Self::Stopped | Self::Failed)
        )
    }

    pub fn validate_transition_to(&self, target: Self) -> Result<(), TransitionError> {
        if self.is_terminal() {
            return Err(TransitionError::TerminalAgentTransition { state: *self });
        }

        if self.can_transition_to(target) {
            Ok(())
        } else {
            Err(TransitionError::InvalidAgentTransition {
                from: *self,
                to: target,
            })
        }
    }
}

/// Descriptor representing an agent registered to a Studio.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDescriptor {
    pub id: AgentId,
    pub studio_id: StudioId,
    pub display_name: String,
    pub kind: AgentKind,
    pub state: AgentState,
    pub role: Option<String>,
}

impl AgentDescriptor {
    pub fn new(
        studio_id: StudioId,
        display_name: impl Into<String>,
        kind: AgentKind,
        role: Option<String>,
    ) -> Self {
        Self {
            id: AgentId::new(),
            studio_id,
            display_name: display_name.into(),
            kind,
            state: AgentState::Registered,
            role,
        }
    }
}

/// Execution budget limits for an agent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentExecutionBudget {
    pub max_turns: Option<u32>,
    pub max_tool_calls: Option<u32>,
    pub max_wall_clock_secs: Option<u64>,
    pub max_child_agents: Option<u32>,
}

impl AgentExecutionBudget {
    pub fn unlimited() -> Self {
        Self {
            max_turns: None,
            max_tool_calls: None,
            max_wall_clock_secs: None,
            max_child_agents: None,
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

    pub fn with_child_agents(mut self, child_agents: u32) -> Self {
        self.max_child_agents = Some(child_agents);
        self
    }
}
