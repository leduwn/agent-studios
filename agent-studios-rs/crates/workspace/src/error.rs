use std::path::PathBuf;
use thiserror::Error;

/// Root error type for Agent Studios workspace and artifact management.
#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("Worktree creation failed: {0}")]
    WorktreeCreationFailed(String),

    #[error("Worktree not found: {0}")]
    WorktreeNotFound(String),

    #[error("Worktree already exists: {0}")]
    WorktreeAlreadyExists(String),

    #[error("Thread binding conflict: {0}")]
    ThreadBindingConflict(String),

    #[error("Worktree removal failed: {0}")]
    WorktreeRemovalFailed(String),

    #[error("Path validation failed: {path:?} - {reason}")]
    PathValidationFailed { path: PathBuf, reason: String },

    #[error(
        "Cross-repository mismatch: source {source_repo:?} does not match target {target_repo:?}"
    )]
    CrossRepoMismatch {
        source_repo: PathBuf,
        target_repo: PathBuf,
    },

    #[error("Git command failed: {0}")]
    GitError(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Artifact store error: {0}")]
    ArtifactStore(#[from] ArtifactStoreError),

    #[error("Reconciliation error: {0}")]
    ReconciliationError(String),

    #[error("Invalid workspace operation: {0}")]
    InvalidOperation(String),
}

/// Errors raised by the content-addressed ArtifactStore.
#[derive(Debug, Error)]
pub enum ArtifactStoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Blob not found for hash: {0}")]
    BlobNotFound(String),

    #[error("Content hash mismatch: expected {expected}, calculated {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid artifact path: {0}")]
    InvalidPath(String),
}
