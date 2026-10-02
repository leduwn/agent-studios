use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::TransitionError;
use crate::id::{AgentId, ApprovalId, StudioId, TaskId};

/// Types of privileged or risky operations requiring explicit human approval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    CommandExecution,
    FileWrite,
    NetworkAccess,
    ExternalTool,
    Other,
}

/// Lifecycle state for an ApprovalRequest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    Pending,
    Approved,
    Denied,
    Cancelled,
    Expired,
}

impl ApprovalState {
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Approved | Self::Denied | Self::Cancelled | Self::Expired
        )
    }

    pub const fn can_transition_to(&self, target: Self) -> bool {
        matches!(
            (self, target),
            (
                Self::Pending,
                Self::Approved | Self::Denied | Self::Cancelled | Self::Expired
            )
        )
    }

    pub fn validate_transition_to(&self, target: Self) -> Result<(), TransitionError> {
        if self.is_terminal() {
            return Err(TransitionError::ApprovalAlreadyResolved { state: *self });
        }

        if self.can_transition_to(target) {
            Ok(())
        } else {
            Err(TransitionError::InvalidApprovalTransition {
                from: *self,
                to: target,
            })
        }
    }
}

/// Durable record of an approval request in the control plane.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: ApprovalId,
    pub studio_id: StudioId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub kind: ApprovalKind,
    pub summary: String,
    pub state: ApprovalState,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

impl ApprovalRequest {
    pub fn new(
        studio_id: StudioId,
        task_id: TaskId,
        agent_id: AgentId,
        kind: ApprovalKind,
        summary: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: ApprovalId::new(),
            studio_id,
            task_id,
            agent_id,
            kind,
            summary: summary.into(),
            state: ApprovalState::Pending,
            created_at,
            resolved_at: None,
        }
    }
}
