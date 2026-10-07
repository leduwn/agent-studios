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
                Self::ReconcilePending
                    | Self::Reconciled
                    | Self::Conflicted
                    | Self::Retained
                    | Self::InUse
                    | Self::Ready
                    | Self::Removing
                    | Self::Removed
                    | Self::Failed
            ) | (
                Self::ReconcilePending,
                Self::Reconciled | Self::Conflicted | Self::Retained | Self::Failed
            ) | (
                Self::Reconciled,
                Self::Ready | Self::Retained | Self::Removing | Self::Removed | Self::Failed
            ) | (
                Self::Conflicted,
                Self::Retained
                    | Self::InUse
                    | Self::Ready
                    | Self::Removing
                    | Self::Removed
                    | Self::Failed
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

/// Typed execution workspace identifying whether an agent executes directly in shared source or an isolated managed worktree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutionWorkspace {
    SharedSource {
        cwd: PathBuf,
    },
    Managed {
        worktree_id: WorktreeId,
        root: PathBuf,
        cwd: PathBuf,
        source_root: PathBuf,
        source_cwd: PathBuf,
        base_sha: String,
    },
}

impl Default for ExecutionWorkspace {
    fn default() -> Self {
        Self::SharedSource {
            cwd: PathBuf::new(),
        }
    }
}

impl ExecutionWorkspace {
    pub fn shared_source(cwd: impl Into<PathBuf>) -> Self {
        Self::SharedSource { cwd: cwd.into() }
    }

    pub fn managed(
        worktree_id: WorktreeId,
        root: impl Into<PathBuf>,
        cwd: impl Into<PathBuf>,
        source_root: impl Into<PathBuf>,
        source_cwd: impl Into<PathBuf>,
        base_sha: impl Into<String>,
    ) -> Self {
        Self::Managed {
            worktree_id,
            root: root.into(),
            cwd: cwd.into(),
            source_root: source_root.into(),
            source_cwd: source_cwd.into(),
            base_sha: base_sha.into(),
        }
    }

    pub fn is_managed(&self) -> bool {
        matches!(self, Self::Managed { .. })
    }

    pub fn cwd(&self) -> &std::path::Path {
        match self {
            Self::SharedSource { cwd } => cwd,
            Self::Managed { cwd, .. } => cwd,
        }
    }

    pub fn root(&self) -> Option<&std::path::Path> {
        match self {
            Self::SharedSource { .. } => None,
            Self::Managed { root, .. } => Some(root),
        }
    }

    pub fn worktree_id(&self) -> Option<WorktreeId> {
        match self {
            Self::SharedSource { .. } => None,
            Self::Managed { worktree_id, .. } => Some(*worktree_id),
        }
    }

    pub fn source_root(&self) -> Option<&std::path::Path> {
        match self {
            Self::SharedSource { .. } => None,
            Self::Managed { source_root, .. } => Some(source_root),
        }
    }

    pub fn source_cwd(&self) -> Option<&std::path::Path> {
        match self {
            Self::SharedSource { .. } => None,
            Self::Managed { source_cwd, .. } => Some(source_cwd),
        }
    }

    pub fn base_sha(&self) -> Option<&str> {
        match self {
            Self::SharedSource { .. } => None,
            Self::Managed { base_sha, .. } => Some(base_sha),
        }
    }
}

/// Typed source workspace distinguishing repository root from execution working directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceWorkspace {
    pub root: PathBuf,
    pub cwd: PathBuf,
    pub base_sha: String,
}

impl SourceWorkspace {
    pub fn new(
        root: impl Into<PathBuf>,
        cwd: impl Into<PathBuf>,
        base_sha: impl Into<String>,
    ) -> Self {
        Self {
            root: root.into(),
            cwd: cwd.into(),
            base_sha: base_sha.into(),
        }
    }
}

/// Typed integration workspace for durable studio reconciliation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationWorkspace {
    pub worktree_id: WorktreeId,
    pub root: PathBuf,
    pub cwd: PathBuf,
    pub base_sha: String,
}

impl IntegrationWorkspace {
    pub fn new(
        worktree_id: WorktreeId,
        root: impl Into<PathBuf>,
        cwd: impl Into<PathBuf>,
        base_sha: impl Into<String>,
    ) -> Self {
        Self {
            worktree_id,
            root: root.into(),
            cwd: cwd.into(),
            base_sha: base_sha.into(),
        }
    }
}

/// Durable record of a Git worktree managed by Agent Studios.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeRecord {
    pub id: WorktreeId,
    pub studio_id: StudioId,
    pub name: String,
    #[serde(alias = "repo_path")]
    pub source_root: PathBuf,
    #[serde(default)]
    pub source_cwd: PathBuf,
    #[serde(alias = "worktree_path")]
    pub root: PathBuf,
    #[serde(default)]
    pub cwd: PathBuf,
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
    pub fn repo_path(&self) -> &std::path::Path {
        &self.source_root
    }

    pub fn worktree_path(&self) -> &std::path::Path {
        &self.root
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        studio_id: StudioId,
        name: impl Into<String>,
        source_root: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        base_commit: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        let s_root = source_root.into();
        let w_root = root.into();
        Self {
            id: WorktreeId::new(),
            studio_id,
            name: name.into(),
            source_root: s_root.clone(),
            source_cwd: s_root,
            root: w_root.clone(),
            cwd: w_root,
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

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_cwds(
        studio_id: StudioId,
        name: impl Into<String>,
        source_root: impl Into<PathBuf>,
        source_cwd: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        cwd: impl Into<PathBuf>,
        base_commit: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: WorktreeId::new(),
            studio_id,
            name: name.into(),
            source_root: source_root.into(),
            source_cwd: source_cwd.into(),
            root: root.into(),
            cwd: cwd.into(),
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

    pub fn with_cwds(mut self, source_cwd: impl Into<PathBuf>, cwd: impl Into<PathBuf>) -> Self {
        self.source_cwd = source_cwd.into();
        self.cwd = cwd.into();
        self
    }

    pub fn with_roots(
        studio_id: StudioId,
        name: impl Into<String>,
        source_root: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        base_commit: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self::new(studio_id, name, source_root, root, base_commit, created_at)
    }
}
