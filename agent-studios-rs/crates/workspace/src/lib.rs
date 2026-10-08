pub mod artifact_store;
pub mod capture;
pub mod error;
pub mod orchestrator;
pub mod reconciliation;

pub use artifact_store::{ArtifactOptions, ArtifactStore};
pub use capture::{CapturedChanges, capture_changes};
pub use error::{ArtifactStoreError, WorkspaceError};
pub use orchestrator::{OwnershipMismatch, WorkspaceOrchestrator, WorkspaceRecoveryReport};
pub use reconciliation::{
    ReconciliationOutcome, apply_patch, apply_patch_with_pre_promotion_hook, check_patch,
    extract_conflicted_files, reconcile_patch, reconcile_patch_with_pre_promotion_hook,
};

pub use codex_worktree::{ManagedWorktree, WorktreeManager, WorktreeSettings};
