use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::{AgentId, ArtifactId, RunId, StudioId, TaskId, WorktreeId};

/// Default schema version for artifacts (version 1).
pub const fn default_artifact_version() -> u32 {
    1
}

/// Category of an artifact generated during task execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    File,
    Patch,
    Log,
    Report,
    Plan,
    Other,
}

/// Lineage key identifying an artifact version family within a Studio.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ArtifactFamilyKey {
    pub studio_id: StudioId,
    pub task_id: TaskId,
    pub kind: ArtifactKind,
    pub logical_name: String,
}

impl ArtifactFamilyKey {
    pub fn new(
        studio_id: StudioId,
        task_id: TaskId,
        kind: ArtifactKind,
        logical_name: impl Into<String>,
    ) -> Self {
        Self {
            studio_id,
            task_id,
            kind,
            logical_name: logical_name.into(),
        }
    }
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
    pub fn family_key(&self) -> ArtifactFamilyKey {
        ArtifactFamilyKey {
            studio_id: self.studio_id,
            task_id: self.task_id,
            kind: self.kind,
            logical_name: self.logical_name.clone(),
        }
    }

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

/// High-level summary of filesystem modifications in a task or worktree diff.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DiffSummary {
    pub files_changed: usize,
    pub added_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub deleted_files: Vec<String>,
    pub renamed_files: Vec<String>,
    pub binary_files: Vec<String>,
    pub patch_size_bytes: usize,
    pub base_commit: String,
}

/// Paginated diff response for IPC clients reviewing task execution changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDiffResponse {
    pub studio_id: StudioId,
    pub task_id: TaskId,
    pub artifact_id: Option<ArtifactId>,
    pub base_commit: Option<String>,
    pub head_commit: Option<String>,
    pub summary: DiffSummary,
    /// Legacy UTF-8 representation of the patch chunk (lossy across arbitrary chunk boundaries).
    pub patch: String,
    pub offset: usize,
    pub limit: usize,
    pub total_bytes: usize,
    pub has_more: bool,
    /// Lossless base64-encoded chunk for exact byte-for-byte reconstruction across arbitrary boundaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_base64: Option<String>,
    /// Encoding descriptor for `patch_base64` (e.g. "base64").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    /// Full SHA-256 hex digest of the complete unchunked raw patch for end-to-end integrity verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_sha256: Option<String>,
    /// Exact byte length of this chunk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_bytes: Option<usize>,
}
