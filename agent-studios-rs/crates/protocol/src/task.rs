use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::TransitionError;
use crate::id::{AgentId, StudioId, TaskId};

/// Deterministic lifecycle states for a Task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Pending,
    Blocked,
    Ready,
    Running,
    Paused,
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskState {
    /// Returns true if the task state is terminal and immutable.
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }

    /// Evaluates if transitioning from `self` to `target` is allowed.
    pub const fn can_transition_to(&self, target: Self) -> bool {
        if self.is_terminal() {
            return false;
        }

        matches!(
            (self, target),
            (Self::Pending, Self::Blocked | Self::Ready | Self::Cancelled)
                | (Self::Blocked, Self::Ready | Self::Cancelled)
                | (Self::Ready, Self::Running | Self::Cancelled)
                | (
                    Self::Running,
                    Self::Paused | Self::Succeeded | Self::Failed | Self::Cancelled
                )
                | (Self::Paused, Self::Running | Self::Cancelled | Self::Failed)
        )
    }

    /// Validates transition from `self` to `target`, returning a typed error on violation.
    pub fn validate_transition_to(&self, target: Self) -> Result<(), TransitionError> {
        if self.is_terminal() {
            return Err(TransitionError::TerminalTaskTransition { state: *self });
        }

        if self.can_transition_to(target) {
            Ok(())
        } else {
            Err(TransitionError::InvalidTaskTransition {
                from: *self,
                to: target,
            })
        }
    }
}

/// Durable record of a Task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: TaskId,
    pub studio_id: StudioId,
    pub parent_task_id: Option<TaskId>,
    pub assigned_agent_id: Option<AgentId>,
    pub title: String,
    pub description: String,
    pub state: TaskState,
    pub dependencies: Vec<TaskId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TaskRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        studio_id: StudioId,
        title: impl Into<String>,
        description: impl Into<String>,
        parent_task_id: Option<TaskId>,
        assigned_agent_id: Option<AgentId>,
        dependencies: Vec<TaskId>,
        created_at: DateTime<Utc>,
    ) -> Self {
        let initial_state = if dependencies.is_empty() {
            TaskState::Ready
        } else {
            TaskState::Blocked
        };

        Self {
            id: TaskId::new(),
            studio_id,
            parent_task_id,
            assigned_agent_id,
            title: title.into(),
            description: description.into(),
            state: initial_state,
            dependencies,
            created_at,
            updated_at: created_at,
        }
    }
}
