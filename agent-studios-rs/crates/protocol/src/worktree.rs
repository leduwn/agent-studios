use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::TransitionError;
use crate::id::{AgentId, ArtifactId, RunId, StudioId, TaskId, WorktreeId};

/// Deterministic lifecycle states for a Managed Worktree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeState {
    Creating,
    Ready,
    InUse,
    ChangeCaptured,
    ReconcilePending,
    Reconciled,
    Conflicted,
    Retained,
    Removing,
    Removed,
    Failed,
}

impl WorktreeState {
    /// Returns true if the worktree is in a terminal, immutable state.
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Removed | Self::Failed)
    }

    /// Returns true if the worktree is currently active and usable.
    pub const fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Ready
                | Self::InUse
                | Self::ChangeCaptured
                | Self::ReconcilePending
                | Self::Reconciled
                | Self::Conflicted
                | Self::Retained
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
                Self::Creating,
                Self::Ready | Self::Failed | Self::Removing | Self::Removed
            ) | (
                Self::Ready,
                Self::InUse | Self::Retained | Self::Removing | Self::Removed | Self::Failed
            ) | (
                Self::InUse,
                Self::InUse | Self::ChangeCaptured | Self::Ready | Self::Retained | Self::Failed
            ) | (
                Self::ChangeCaptured,
                Self::ReconcilePending | Self::Retained | Self::InUse | Self::Ready | Self::Failed
            ) | (
                Self::ReconcilePending,
                Self::Reconciled | Self::Conflicted | Self::Retained | Self::Failed
            ) | (
                Self::Reconciled,
                Self::Ready | Self::Retained | Self::Removing | Self::Removed | Self::Failed
            ) | (
                Self::Conflicted,
                Self::Retained | Self::InUse | Self::Ready | Self::Failed
            ) | (
                Self::Retained,
                Self::Ready | Self::InUse | Self::Removing | Self::Removed | Self::Failed
            ) | (Self::Removing, Self::Removed | Self::Failed)
        )
    }

    /// Validates transition from `self` to `target`, returning a typed error on violation.
    pub fn validate_transition_to(&self, target: Self) -> Result<(), TransitionError> {
        if self.is_terminal() {
            return Err(TransitionError::TerminalWorktreeTransition { state: *self });
        }

        if self.can_transition_to(target) {
            Ok(())
        } else {
            Err(TransitionError::InvalidWorktreeTransition {
                from: *self,
                to: target,
            })
        }
    }
}

/// Durable record of a Git worktree managed by Agent Studios.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeRecord {
    pub id: WorktreeId,
    pub studio_id: StudioId,
    pub name: String,
    pub repo_path: PathBuf,
    pub worktree_path: PathBuf,
    pub base_commit: String,
    #[serde(default)]
    pub branch_name: Option<String>,
    #[serde(default)]
    pub assigned_task_id: Option<TaskId>,
    #[serde(default)]
    pub assigned_agent_id: Option<AgentId>,
    #[serde(default)]
    pub assigned_run_id: Option<RunId>,
    #[serde(default)]
    pub bound_thread_id: Option<String>,
    pub state: WorktreeState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub released_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub retained: bool,
    #[serde(default)]
    pub retained_reason: Option<String>,
    #[serde(default)]
    pub last_captured_commit: Option<String>,
    #[serde(default)]
    pub patch_artifact_id: Option<ArtifactId>,
}

impl WorktreeRecord {
    pub fn new(
        studio_id: StudioId,
        name: impl Into<String>,
        repo_path: impl Into<PathBuf>,
        worktree_path: impl Into<PathBuf>,
        base_commit: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: WorktreeId::new(),
            studio_id,
            name: name.into(),
            repo_path: repo_path.into(),
            worktree_path: worktree_path.into(),
            base_commit: base_commit.into(),
            branch_name: None,
            assigned_task_id: None,
            assigned_agent_id: None,
            assigned_run_id: None,
            bound_thread_id: None,
            state: WorktreeState::Creating,
            created_at,
            updated_at: created_at,
            released_at: None,
            retained: false,
            retained_reason: None,
            last_captured_commit: None,
            patch_artifact_id: None,
        }
    }
}
