use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::{AgentId, ArtifactId, RunId, StudioId, TaskId, WorktreeId};

/// Default schema version for artifacts (version 1).
pub const fn default_artifact_version() -> u32 {
    1
}

/// Category of an artifact generated during task execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    File,
    Patch,
    Log,
    Report,
    Plan,
    Other,
}

/// Metadata record for an artifact managed by Agent Studios.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub id: ArtifactId,
    pub studio_id: StudioId,
    pub task_id: TaskId,
    pub producer_agent_id: AgentId,
    pub kind: ArtifactKind,
    pub logical_name: String,
    pub content_hash: Option<String>,
    pub location: String,
    pub created_at: DateTime<Utc>,

    // Backward-compatible extensions (M09)
    #[serde(default)]
    pub run_id: Option<RunId>,
    #[serde(default)]
    pub worktree_id: Option<WorktreeId>,
    #[serde(default = "default_artifact_version")]
    pub version: u32,
    #[serde(default)]
    pub supersedes: Option<ArtifactId>,
    #[serde(default)]
    pub relative_path: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
}

impl ArtifactRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        studio_id: StudioId,
        task_id: TaskId,
        producer_agent_id: AgentId,
        kind: ArtifactKind,
        logical_name: impl Into<String>,
        content_hash: Option<String>,
        location: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: ArtifactId::new(),
            studio_id,
            task_id,
            producer_agent_id,
            kind,
            logical_name: logical_name.into(),
            content_hash,
            location: location.into(),
            created_at,
            run_id: None,
            worktree_id: None,
            version: 1,
            supersedes: None,
            relative_path: None,
            size_bytes: None,
        }
    }

    pub fn with_run_id(mut self, run_id: Option<RunId>) -> Self {
        self.run_id = run_id;
        self
    }

    pub fn with_worktree_id(mut self, worktree_id: Option<WorktreeId>) -> Self {
        self.worktree_id = worktree_id;
        self
    }

    pub fn with_version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    pub fn with_supersedes(mut self, supersedes: Option<ArtifactId>) -> Self {
        self.supersedes = supersedes;
        self
    }

    pub fn with_relative_path(mut self, relative_path: Option<String>) -> Self {
        self.relative_path = relative_path;
        self
    }

    pub fn with_size_bytes(mut self, size_bytes: Option<u64>) -> Self {
        self.size_bytes = size_bytes;
        self
    }
}
