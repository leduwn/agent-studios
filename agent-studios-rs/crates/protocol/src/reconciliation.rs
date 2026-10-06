use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::TransitionError;
use crate::id::{ArtifactId, ReconciliationId, RunId, StudioId, TaskId, WorktreeId};

/// Lifecycle states for patch reconciliation into an integration workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationState {
    Pending,
    Checking,
    Applying,
    Applied,
    Conflicted,
    Failed,
    Cancelled,
}

impl ReconciliationState {
    /// Returns true if the reconciliation is in a terminal, immutable state.
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Applied | Self::Conflicted | Self::Failed | Self::Cancelled
        )
    }

    /// Evaluates whether transitioning from `self` to `target` is allowed.
    pub const fn can_transition_to(&self, target: Self) -> bool {
        if self.is_terminal() {
            return false;
        }

        matches!(
            (self, target),
            (
                Self::Pending,
                Self::Checking | Self::Applying | Self::Cancelled | Self::Failed
            ) | (
                Self::Checking,
                Self::Applying | Self::Conflicted | Self::Failed | Self::Cancelled
            ) | (
                Self::Applying,
                Self::Applied | Self::Conflicted | Self::Failed | Self::Cancelled
            )
        )
    }

    /// Validates transition from `self` to `target`, returning a typed error on violation.
    pub fn validate_transition_to(&self, target: Self) -> Result<(), TransitionError> {
        if self.is_terminal() {
            return Err(TransitionError::TerminalReconciliationTransition { state: *self });
        }

        if self.can_transition_to(target) {
            Ok(())
        } else {
            Err(TransitionError::InvalidReconciliationTransition {
                from: *self,
                to: target,
            })
        }
    }
}

/// Durable record of a patch reconciliation operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationRecord {
    pub id: ReconciliationId,
    pub studio_id: StudioId,
    pub worktree_id: WorktreeId,
    pub task_id: TaskId,
    #[serde(default)]
    pub run_id: Option<RunId>,
    pub patch_artifact_id: ArtifactId,
    pub target_worktree_path: PathBuf,
    pub base_commit: String,
    pub state: ReconciliationState,
    #[serde(default)]
    pub conflicted_files: Vec<String>,
    #[serde(default)]
    pub merge_commit: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
}

impl ReconciliationRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        studio_id: StudioId,
        worktree_id: WorktreeId,
        task_id: TaskId,
        run_id: Option<RunId>,
        patch_artifact_id: ArtifactId,
        target_worktree_path: impl Into<PathBuf>,
        base_commit: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: ReconciliationId::new(),
            studio_id,
            worktree_id,
            task_id,
            run_id,
            patch_artifact_id,
            target_worktree_path: target_worktree_path.into(),
            base_commit: base_commit.into(),
            state: ReconciliationState::Pending,
            conflicted_files: Vec::new(),
            merge_commit: None,
            error_message: None,
            created_at,
            updated_at: created_at,
            completed_at: None,
        }
    }
}
