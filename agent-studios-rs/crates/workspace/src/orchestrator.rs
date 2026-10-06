use std::path::{Path, PathBuf};
use std::sync::Arc;

use codex_worktree::{CreateWorktree, ManagedWorktree, WorktreeManager, WorktreeSettings};

use agent_studios_protocol::artifact::{ArtifactKind, ArtifactRecord};
use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId, WorktreeId};
use agent_studios_protocol::worktree::WorktreeRecord;

use crate::artifact_store::{ArtifactOptions, ArtifactStore};
use crate::capture::{self, CapturedChanges};
use crate::error::WorkspaceError;
use crate::reconciliation::{self, ReconciliationOutcome};

/// Unified workspace and artifact orchestrator for Agent Studios.
///
/// Wraps `codex_worktree::WorktreeManager`, coordinates change-set capture
/// via ephemeral Git indices, persists content-addressed artifacts,
/// and executes safe patch reconciliation against integration worktrees.
#[derive(Clone, Debug)]
pub struct WorkspaceOrchestrator {
    worktree_manager: Arc<WorktreeManager>,
    artifact_store: Arc<ArtifactStore>,
    managed_root: PathBuf,
}

impl WorkspaceOrchestrator {
    pub fn new(
        worktree_root: impl Into<PathBuf>,
        artifact_root: impl Into<PathBuf>,
    ) -> Result<Self, WorkspaceError> {
        let raw_worktree_root = worktree_root.into();
        std::fs::create_dir_all(&raw_worktree_root)?;
        let managed_root = dunce::canonicalize(&raw_worktree_root).map_err(|e| {
            WorkspaceError::PathValidationFailed {
                path: raw_worktree_root.clone(),
                reason: e.to_string(),
            }
        })?;

        let settings = WorktreeSettings {
            root: managed_root.clone(),
            auto_cleanup_enabled: false,
            keep_count: codex_worktree::DEFAULT_WORKTREE_KEEP_COUNT,
        };
        let worktree_manager = Arc::new(WorktreeManager::new(settings));
        let artifact_store = Arc::new(ArtifactStore::new(artifact_root.into())?);

        Ok(Self {
            worktree_manager,
            artifact_store,
            managed_root,
        })
    }

    pub fn managed_root(&self) -> &Path {
        &self.managed_root
    }

    pub fn artifact_store(&self) -> &Arc<ArtifactStore> {
        &self.artifact_store
    }

    pub fn worktree_manager(&self) -> &Arc<WorktreeManager> {
        &self.worktree_manager
    }

    /// Validates that a worktree root and its working directory lie strictly
    /// inside the managed root and do not escape via traversal or symlinks.
    pub fn validate_worktree_path(&self, root: &Path, cwd: &Path) -> Result<(), WorkspaceError> {
        let canonical_root =
            dunce::canonicalize(root).map_err(|e| WorkspaceError::PathValidationFailed {
                path: root.to_path_buf(),
                reason: format!("cannot canonicalize root: {e}"),
            })?;
        let canonical_cwd =
            dunce::canonicalize(cwd).map_err(|e| WorkspaceError::PathValidationFailed {
                path: cwd.to_path_buf(),
                reason: format!("cannot canonicalize cwd: {e}"),
            })?;

        if !canonical_root.starts_with(&self.managed_root) {
            return Err(WorkspaceError::PathValidationFailed {
                path: root.to_path_buf(),
                reason: format!(
                    "root {} escapes managed root {}",
                    canonical_root.display(),
                    self.managed_root.display()
                ),
            });
        }

        if !canonical_cwd.starts_with(&canonical_root) {
            return Err(WorkspaceError::PathValidationFailed {
                path: cwd.to_path_buf(),
                reason: format!(
                    "cwd {} escapes worktree root {}",
                    canonical_cwd.display(),
                    canonical_root.display()
                ),
            });
        }

        Ok(())
    }

    /// Synchronously creates an isolated managed worktree from `source_cwd` at `base_commit`.
    pub fn create_worktree(
        &self,
        source_cwd: &Path,
        base_commit: Option<&str>,
    ) -> Result<ManagedWorktree, WorkspaceError> {
        let request = CreateWorktree {
            source_cwd: source_cwd.to_path_buf(),
            base: base_commit.map(str::to_string),
        };
        let managed = self
            .worktree_manager
            .create(&request)
            .map_err(|e| WorkspaceError::WorktreeCreationFailed(e.to_string()))?;
        self.validate_worktree_path(&managed.root, &managed.cwd)?;
        Ok(managed)
    }

    /// Asynchronous wrapper for `create_worktree` using `spawn_blocking`.
    pub async fn create_worktree_async(
        &self,
        source_cwd: PathBuf,
        base_commit: Option<String>,
    ) -> Result<ManagedWorktree, WorkspaceError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            this.create_worktree(&source_cwd, base_commit.as_deref())
        })
        .await
        .map_err(|e| WorkspaceError::InvalidOperation(e.to_string()))?
    }

    /// Binds an existing worktree checkout to a specific Codex thread ID.
    pub fn bind_thread(&self, checkout: &Path, thread_id: &str) -> Result<(), WorkspaceError> {
        self.worktree_manager
            .bind_thread(checkout, thread_id)
            .map_err(|e| WorkspaceError::ThreadBindingConflict(e.to_string()))
    }

    /// Asynchronous wrapper for `bind_thread`.
    pub async fn bind_thread_async(
        &self,
        checkout: PathBuf,
        thread_id: String,
    ) -> Result<(), WorkspaceError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.bind_thread(&checkout, &thread_id))
            .await
            .map_err(|e| WorkspaceError::InvalidOperation(e.to_string()))?
    }

    /// Queries the currently bound thread ID for a managed worktree checkout.
    pub fn get_owner(&self, checkout: &Path) -> Result<Option<String>, WorkspaceError> {
        self.worktree_manager
            .owner(checkout)
            .map_err(|e| WorkspaceError::InvalidOperation(e.to_string()))
    }

    /// Releases a managed worktree. If `retain` is true, the directory is left intact on disk.
    pub fn release_worktree(
        &self,
        source_cwd: &Path,
        root: &Path,
        retain: bool,
        reason: Option<&str>,
    ) -> Result<(), WorkspaceError> {
        if retain {
            tracing::info!(
                worktree_root = %root.display(),
                reason = ?reason,
                "Retaining managed worktree without deleting directory"
            );
            return Ok(());
        }

        self.worktree_manager
            .remove(source_cwd, root)
            .map_err(|e| WorkspaceError::WorktreeRemovalFailed(e.to_string()))
    }

    /// Asynchronous wrapper for `release_worktree`.
    pub async fn release_worktree_async(
        &self,
        source_cwd: PathBuf,
        root: PathBuf,
        retain: bool,
        reason: Option<String>,
    ) -> Result<(), WorkspaceError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            this.release_worktree(&source_cwd, &root, retain, reason.as_deref())
        })
        .await
        .map_err(|e| WorkspaceError::InvalidOperation(e.to_string()))?
    }

    /// Captures all modifications in `worktree_path` relative to `base_commit` via an ephemeral index.
    pub fn capture_changes(
        &self,
        worktree_path: &Path,
        base_commit: &str,
    ) -> Result<CapturedChanges, WorkspaceError> {
        capture::capture_changes(worktree_path, base_commit)
    }

    /// Asynchronous wrapper for `capture_changes`.
    pub async fn capture_changes_async(
        &self,
        worktree_path: PathBuf,
        base_commit: String,
    ) -> Result<CapturedChanges, WorkspaceError> {
        tokio::task::spawn_blocking(move || capture::capture_changes(&worktree_path, &base_commit))
            .await
            .map_err(|e| WorkspaceError::InvalidOperation(e.to_string()))?
    }

    /// Stores the captured patch into the content-addressed `ArtifactStore` as an `ArtifactRecord`.
    pub fn store_patch_artifact(
        &self,
        studio_id: StudioId,
        task_id: TaskId,
        producer_agent_id: AgentId,
        run_id: Option<RunId>,
        worktree_id: Option<WorktreeId>,
        captured: &CapturedChanges,
    ) -> Result<ArtifactRecord, WorkspaceError> {
        let options = ArtifactOptions::new()
            .with_run_id(run_id)
            .with_worktree_id(worktree_id)
            .with_version(1);

        self.artifact_store
            .store_artifact(
                studio_id,
                task_id,
                producer_agent_id,
                ArtifactKind::Patch,
                format!("task-{task_id}-patch.diff"),
                &captured.patch_bytes,
                options,
            )
            .map_err(WorkspaceError::from)
    }

    /// Safely reconciles a patch against an integration worktree.
    pub fn reconcile_patch(
        &self,
        target_worktree: &Path,
        patch_data: &[u8],
        commit_message: Option<&str>,
    ) -> Result<ReconciliationOutcome, WorkspaceError> {
        reconciliation::reconcile_patch(target_worktree, patch_data, commit_message)
    }

    /// Asynchronous wrapper for `reconcile_patch`.
    pub async fn reconcile_patch_async(
        &self,
        target_worktree: PathBuf,
        patch_data: Vec<u8>,
        commit_message: Option<String>,
    ) -> Result<ReconciliationOutcome, WorkspaceError> {
        tokio::task::spawn_blocking(move || {
            reconciliation::reconcile_patch(
                &target_worktree,
                &patch_data,
                commit_message.as_deref(),
            )
        })
        .await
        .map_err(|e| WorkspaceError::InvalidOperation(e.to_string()))?
    }

    /// Generates a non-destructive recovery diagnostic report comparing physical worktrees with durable records.
    pub fn generate_recovery_report(
        &self,
        source_cwd: &Path,
        durable_worktrees: &[WorktreeRecord],
    ) -> Result<WorkspaceRecoveryReport, WorkspaceError> {
        let physical_checkouts = self
            .worktree_manager
            .list(source_cwd)
            .map_err(|e| WorkspaceError::InvalidOperation(e.to_string()))?;

        let mut report = WorkspaceRecoveryReport::default();
        let mut physical_matched = std::collections::HashSet::new();

        for record in durable_worktrees {
            let canon_root = match dunce::canonicalize(&record.root) {
                Ok(p) => p,
                Err(_) => {
                    report.missing.push(record.clone());
                    continue;
                }
            };

            let physical = physical_checkouts.iter().find(|c| {
                dunce::canonicalize(&c.root)
                    .map(|p| p == canon_root)
                    .unwrap_or(false)
            });

            match physical {
                Some(checkout) => {
                    physical_matched.insert(checkout.root.clone());
                    let physical_owner =
                        self.worktree_manager.owner(&checkout.root).unwrap_or(None);
                    if record.bound_thread_id != physical_owner {
                        report.ownership_mismatches.push(OwnershipMismatch {
                            worktree_id: record.id,
                            path: record.root.clone(),
                            expected_thread_id: record.bound_thread_id.clone(),
                            actual_thread_id: physical_owner.clone(),
                            reason: format!(
                                "Bound thread mismatch: durable record expected {:?}, physical checkout has {:?}",
                                record.bound_thread_id, physical_owner
                            ),
                        });
                    } else {
                        report.matched.push(record.clone());
                    }
                }
                None => {
                    report.missing.push(record.clone());
                }
            }
        }

        for checkout in physical_checkouts {
            if !physical_matched.contains(&checkout.root) {
                report.foreign.push(checkout.root);
            }
        }

        Ok(report)
    }
}

/// Structured diagnostic report of workspace health comparing durable records with physical checkouts.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct WorkspaceRecoveryReport {
    pub matched: Vec<WorktreeRecord>,
    pub missing: Vec<WorktreeRecord>,
    pub foreign: Vec<PathBuf>,
    pub ownership_mismatches: Vec<OwnershipMismatch>,
}

/// Diagnostic detail for a thread/worktree ownership discrepancy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnershipMismatch {
    pub worktree_id: WorktreeId,
    pub path: PathBuf,
    pub expected_thread_id: Option<String>,
    pub actual_thread_id: Option<String>,
    pub reason: String,
}
