use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::WorkspaceError;

/// Outcome of a patch reconciliation attempt on an integration worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconciliationOutcome {
    Applied {
        merge_commit: Option<String>,
    },
    Conflicted {
        conflicted_files: Vec<String>,
        reason: String,
    },
    Failed {
        error: String,
    },
}

/// Checks whether a patch applies cleanly against a target integration worktree.
/// Returns `Ok(None)` if check succeeds (clean apply possible).
/// Returns `Ok(Some(ReconciliationOutcome::Conflicted { .. }))` if check fails.
pub fn check_patch(
    target_worktree: &Path,
    patch_data: &[u8],
) -> Result<Option<ReconciliationOutcome>, WorkspaceError> {
    if !target_worktree.is_dir() {
        return Err(WorkspaceError::InvalidOperation(format!(
            "target worktree {} is not a directory",
            target_worktree.display()
        )));
    }

    // Preflight cleanliness check: git status --porcelain -uno
    let status_out = Command::new("git")
        .current_dir(target_worktree)
        .args(["status", "--porcelain", "-uno"])
        .output()?;
    if !status_out.status.success() {
        return Err(WorkspaceError::GitError(format!(
            "git status failed in target worktree: {}",
            String::from_utf8_lossy(&status_out.stderr)
        )));
    }
    let status_str = String::from_utf8_lossy(&status_out.stdout);
    if !status_str.trim().is_empty() {
        return Err(WorkspaceError::IntegrationWorkspaceDirty(format!(
            "target worktree {} has uncommitted changes:\n{}",
            target_worktree.display(),
            status_str.trim()
        )));
    }

    if patch_data.is_empty() {
        return Ok(None);
    }

    // Dry-run check: git apply --check --binary -
    let mut check_child = Command::new("git")
        .current_dir(target_worktree)
        .args(["apply", "--check", "--binary", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = check_child.stdin.take() {
        stdin.write_all(patch_data)?;
    }

    let check_output = check_child.wait_with_output()?;
    if !check_output.status.success() {
        let stderr_str = String::from_utf8_lossy(&check_output.stderr).to_string();
        let conflicted_files = extract_conflicted_files(&stderr_str);
        return Ok(Some(ReconciliationOutcome::Conflicted {
            conflicted_files,
            reason: stderr_str,
        }));
    }

    Ok(None)
}

/// Applies a patch to a target integration worktree using an isolated Git transaction.
///
/// Builds a candidate commit in an ephemeral temporary index (`GIT_INDEX_FILE`),
/// revalidates pre-promotion cleanliness and HEAD equality, and safely fast-forwards
/// the target worktree via `git merge --ff-only <candidate_commit>`.
///
/// Unrelated untracked files are never staged or deleted. Destructive commands
/// (`git reset --hard`, `git clean`) are never executed.
pub fn apply_patch_with_pre_promotion_hook<F>(
    target_worktree: &Path,
    patch_data: &[u8],
    commit_message: Option<&str>,
    pre_promotion_hook: Option<F>,
) -> Result<ReconciliationOutcome, WorkspaceError>
where
    F: FnOnce(&Path) -> Result<(), WorkspaceError>,
{
    if !target_worktree.is_dir() {
        return Err(WorkspaceError::InvalidOperation(format!(
            "target worktree {} is not a directory",
            target_worktree.display()
        )));
    }

    if patch_data.is_empty() {
        return Ok(ReconciliationOutcome::Applied { merge_commit: None });
    }

    // 1. Preflight cleanliness check: git status --porcelain -uno
    let status_out = Command::new("git")
        .current_dir(target_worktree)
        .args(["status", "--porcelain", "-uno"])
        .output()?;
    if !status_out.status.success() {
        return Err(WorkspaceError::GitError(format!(
            "git status failed in target worktree: {}",
            String::from_utf8_lossy(&status_out.stderr)
        )));
    }
    let status_str = String::from_utf8_lossy(&status_out.stdout);
    if !status_str.trim().is_empty() {
        return Err(WorkspaceError::IntegrationWorkspaceDirty(format!(
            "target worktree {} has uncommitted changes:\n{}",
            target_worktree.display(),
            status_str.trim()
        )));
    }

    // 2. Snapshot pre-HEAD commit
    let rev_out = Command::new("git")
        .current_dir(target_worktree)
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !rev_out.status.success() {
        return Err(WorkspaceError::GitError(format!(
            "git rev-parse HEAD failed in target worktree: {}",
            String::from_utf8_lossy(&rev_out.stderr)
        )));
    }
    let pre_head = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();

    // 3. Create isolated temporary directory for ephemeral GIT_INDEX_FILE
    let temp_dir = tempfile::tempdir()?;
    let temp_index_file = temp_dir.path().join("reconcile_index");

    // 4. Populate isolated index with pre_head tree: GIT_INDEX_FILE=<temp> git read-tree <pre_head>
    let read_tree_out = Command::new("git")
        .current_dir(target_worktree)
        .env("GIT_INDEX_FILE", &temp_index_file)
        .args(["read-tree", &pre_head])
        .output()?;
    if !read_tree_out.status.success() {
        return Ok(ReconciliationOutcome::Failed {
            error: format!(
                "git read-tree failed in isolated index: {}",
                String::from_utf8_lossy(&read_tree_out.stderr)
            ),
        });
    }

    // 5. Apply patch to temporary index ONLY: GIT_INDEX_FILE=<temp> git apply --cached --binary -
    let mut apply_child = Command::new("git")
        .current_dir(target_worktree)
        .env("GIT_INDEX_FILE", &temp_index_file)
        .args(["apply", "--cached", "--binary", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = apply_child.stdin.take() {
        stdin.write_all(patch_data)?;
    }

    let apply_output = apply_child.wait_with_output()?;
    if !apply_output.status.success() {
        let stderr_str = String::from_utf8_lossy(&apply_output.stderr).to_string();
        let conflicted_files = extract_conflicted_files(&stderr_str);
        return Ok(ReconciliationOutcome::Conflicted {
            conflicted_files,
            reason: stderr_str,
        });
    }

    // 6. Write tree from isolated index: GIT_INDEX_FILE=<temp> git write-tree
    let write_tree_out = Command::new("git")
        .current_dir(target_worktree)
        .env("GIT_INDEX_FILE", &temp_index_file)
        .args(["write-tree"])
        .output()?;
    if !write_tree_out.status.success() {
        return Ok(ReconciliationOutcome::Failed {
            error: format!(
                "git write-tree failed in isolated index: {}",
                String::from_utf8_lossy(&write_tree_out.stderr)
            ),
        });
    }
    let tree_sha = String::from_utf8_lossy(&write_tree_out.stdout)
        .trim()
        .to_string();

    // 7. Create candidate commit: git commit-tree <tree> -p <pre_head> -m <msg>
    let commit_msg = commit_message.unwrap_or("Reconcile patch");
    let commit_tree_out = Command::new("git")
        .current_dir(target_worktree)
        .args(["commit-tree", &tree_sha, "-p", &pre_head, "-m", commit_msg])
        .output()?;
    if !commit_tree_out.status.success() {
        return Ok(ReconciliationOutcome::Failed {
            error: format!(
                "git commit-tree failed for candidate commit: {}",
                String::from_utf8_lossy(&commit_tree_out.stderr)
            ),
        });
    }
    let candidate_commit = String::from_utf8_lossy(&commit_tree_out.stdout)
        .trim()
        .to_string();

    // 8. Deterministic test seam hook (if supplied) to simulate concurrent mutations
    if let Some(hook) = pre_promotion_hook {
        hook(target_worktree)?;
    }

    // 9. Revalidate integration worktree immediately before promotion
    let current_rev_out = Command::new("git")
        .current_dir(target_worktree)
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !current_rev_out.status.success() {
        return Ok(ReconciliationOutcome::Failed {
            error: format!(
                "git rev-parse HEAD revalidation failed: {}",
                String::from_utf8_lossy(&current_rev_out.stderr)
            ),
        });
    }
    let current_head = String::from_utf8_lossy(&current_rev_out.stdout)
        .trim()
        .to_string();
    if current_head != pre_head {
        return Ok(ReconciliationOutcome::Failed {
            error: format!(
                "Reconciliation pre-promotion revalidation failed: HEAD moved concurrently from {pre_head} to {current_head}"
            ),
        });
    }

    let status_recheck_out = Command::new("git")
        .current_dir(target_worktree)
        .args(["status", "--porcelain", "-uno"])
        .output()?;
    if !status_recheck_out.status.success() {
        return Ok(ReconciliationOutcome::Failed {
            error: format!(
                "git status pre-promotion revalidation failed: {}",
                String::from_utf8_lossy(&status_recheck_out.stderr)
            ),
        });
    }
    let status_recheck_str = String::from_utf8_lossy(&status_recheck_out.stdout);
    if !status_recheck_str.trim().is_empty() {
        return Ok(ReconciliationOutcome::Failed {
            error: format!(
                "Reconciliation pre-promotion revalidation failed: integration worktree became dirty:\n{}",
                status_recheck_str.trim()
            ),
        });
    }

    // 10. Safe promotion: git merge --ff-only <candidate_commit>
    let merge_out = Command::new("git")
        .current_dir(target_worktree)
        .args(["merge", "--ff-only", &candidate_commit])
        .output()?;
    if !merge_out.status.success() {
        let stderr_str = String::from_utf8_lossy(&merge_out.stderr).to_string();
        return Ok(ReconciliationOutcome::Failed {
            error: format!("git merge --ff-only failed: {stderr_str}"),
        });
    }

    Ok(ReconciliationOutcome::Applied {
        merge_commit: Some(candidate_commit),
    })
}

/// Applies a previously checked patch to a target integration worktree.
pub fn apply_patch(
    target_worktree: &Path,
    patch_data: &[u8],
    commit_message: Option<&str>,
) -> Result<ReconciliationOutcome, WorkspaceError> {
    apply_patch_with_pre_promotion_hook(
        target_worktree,
        patch_data,
        commit_message,
        None::<fn(&Path) -> Result<(), WorkspaceError>>,
    )
}

/// Safely reconciles a patch against a dedicated integration worktree with an optional test hook.
pub fn reconcile_patch_with_pre_promotion_hook<F>(
    target_worktree: &Path,
    patch_data: &[u8],
    commit_message: Option<&str>,
    pre_promotion_hook: Option<F>,
) -> Result<ReconciliationOutcome, WorkspaceError>
where
    F: FnOnce(&Path) -> Result<(), WorkspaceError>,
{
    if let Some(conflict) = check_patch(target_worktree, patch_data)? {
        return Ok(conflict);
    }
    apply_patch_with_pre_promotion_hook(
        target_worktree,
        patch_data,
        commit_message,
        pre_promotion_hook,
    )
}

/// Safely reconciles a patch against a dedicated integration worktree.
pub fn reconcile_patch(
    target_worktree: &Path,
    patch_data: &[u8],
    commit_message: Option<&str>,
) -> Result<ReconciliationOutcome, WorkspaceError> {
    reconcile_patch_with_pre_promotion_hook(
        target_worktree,
        patch_data,
        commit_message,
        None::<fn(&Path) -> Result<(), WorkspaceError>>,
    )
}

/// Parses git apply error messages to discover which files conflicted.
#[allow(clippy::collapsible_if)]
pub fn extract_conflicted_files(stderr: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in stderr.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("error: patch failed: ") {
            if let Some(file) = rest.split(':').next() {
                let file = file.trim();
                if !file.is_empty() && !files.iter().any(|f| f == file) {
                    files.push(file.to_string());
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("error: ") {
            if let Some(file) = rest.split(':').next() {
                let file = file.trim();
                if !file.is_empty() && !file.contains(' ') && !files.iter().any(|f| f == file) {
                    files.push(file.to_string());
                }
            }
        }
    }
    files
}
