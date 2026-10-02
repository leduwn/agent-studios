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
