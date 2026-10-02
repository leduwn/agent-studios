use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::TransitionError;
use crate::id::{AgentId, RunId, TaskId};

/// Lifecycle state for an execution attempt (Run) of a Task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Queued,
    Starting,
    Running,
    Paused,
    Succeeded,
    Failed,
    Cancelled,
}

impl RunState {
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }

    pub const fn can_transition_to(&self, target: Self) -> bool {
        if self.is_terminal() {
            return false;
        }

        matches!(
            (self, target),
            (Self::Queued, Self::Starting | Self::Cancelled)
                | (
                    Self::Starting,
                    Self::Running | Self::Failed | Self::Cancelled
                )
                | (
                    Self::Running,
                    Self::Paused | Self::Succeeded | Self::Failed | Self::Cancelled
                )
                | (Self::Paused, Self::Running | Self::Failed | Self::Cancelled)
        )
    }

    pub fn validate_transition_to(&self, target: Self) -> Result<(), TransitionError> {
        if self.is_terminal() {
            return Err(TransitionError::TerminalRunTransition { state: *self });
        }

        if self.can_transition_to(target) {
            Ok(())
        } else {
            Err(TransitionError::InvalidRunTransition {
                from: *self,
                to: target,
            })
        }
    }
}

/// Durable record of a Task execution attempt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: RunId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub attempt: u32,
    pub state: RunState,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl RunRecord {
    pub fn new(task_id: TaskId, agent_id: AgentId, attempt: u32) -> Self {
        Self {
            id: RunId::new(),
            task_id,
            agent_id,
            attempt,
            state: RunState::Queued,
            started_at: None,
            finished_at: None,
        }
    }
}
