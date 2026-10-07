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

    // 1. Dry-run check: git apply --check --binary -
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

/// Applies a previously checked patch to a target integration worktree and creates an optional commit.
pub fn apply_patch(
    target_worktree: &Path,
    patch_data: &[u8],
    commit_message: Option<&str>,
) -> Result<ReconciliationOutcome, WorkspaceError> {
    if !target_worktree.is_dir() {
        return Err(WorkspaceError::InvalidOperation(format!(
            "target worktree {} is not a directory",
            target_worktree.display()
        )));
    }

    if patch_data.is_empty() {
        return Ok(ReconciliationOutcome::Applied { merge_commit: None });
    }

    // Snapshot pre-HEAD commit
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

    // Snapshot pre-existing untracked files to guarantee non-destructive transactional rollback
    let pre_status = Command::new("git")
        .current_dir(target_worktree)
        .args(["status", "--porcelain"])
        .output()?;
    let mut pre_existing_untracked = std::collections::HashSet::new();
    if pre_status.status.success() {
        for line in String::from_utf8_lossy(&pre_status.stdout).lines() {
            if let Some(rest) = line.strip_prefix("?? ") {
                let path_str = rest.trim().trim_matches('"');
                pre_existing_untracked.insert(path_str.to_string());
            }
        }
    }

    let rollback = || -> Result<(), WorkspaceError> {
        // 1. Reset the index cleanly to pre_head without touching working tree files
        let reset_out = Command::new("git")
            .current_dir(target_worktree)
            .args(["reset", &pre_head])
            .output()?;
        if !reset_out.status.success() {
            return Err(WorkspaceError::ReconciliationRollbackFailed(format!(
                "git reset failed: {}",
                String::from_utf8_lossy(&reset_out.stderr)
            )));
        }

        // 2. Safely restore tracked files in working directory to pre_head without touching untracked files
        let checkout_out = Command::new("git")
            .current_dir(target_worktree)
            .args(["checkout", &pre_head, "--", "."])
            .output()?;
        if !checkout_out.status.success() {
            return Err(WorkspaceError::ReconciliationRollbackFailed(format!(
                "git checkout failed: {}",
                String::from_utf8_lossy(&checkout_out.stderr)
            )));
        }

        // 3. Remove only new untracked files created by the failed patch, preserving pre-existing untracked files
        let post_status = Command::new("git")
            .current_dir(target_worktree)
            .args(["status", "--porcelain"])
            .output()?;
        if post_status.status.success() {
            for line in String::from_utf8_lossy(&post_status.stdout).lines() {
                if let Some(rest) = line.strip_prefix("?? ") {
                    let path_str = rest.trim().trim_matches('"');
                    if !pre_existing_untracked.contains(path_str) {
                        let path = target_worktree.join(path_str);
                        if path.is_file() || path.is_symlink() {
                            let _ = std::fs::remove_file(&path);
                        } else if path.is_dir() {
                            let _ = std::fs::remove_dir_all(&path);
                        }
                    }
                }
            }
        }

        Ok(())
    };

    let mut apply_child = Command::new("git")
        .current_dir(target_worktree)
        .args(["apply", "--binary", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = apply_child.stdin.take() {
        stdin.write_all(patch_data)?;
    }

    let apply_output = apply_child.wait_with_output()?;
    if !apply_output.status.success() {
        rollback()?;
        return Ok(ReconciliationOutcome::Failed {
            error: format!(
                "git apply failed after check succeeded: {}",
                String::from_utf8_lossy(&apply_output.stderr)
            ),
        });
    }

    let merge_commit = if let Some(msg) = commit_message {
        let add_out = Command::new("git")
            .current_dir(target_worktree)
            .args(["add", "-A", "--", "."])
            .output()?;
        if !add_out.status.success() {
            rollback()?;
            return Ok(ReconciliationOutcome::Failed {
                error: format!(
                    "git add failed during reconciliation commit: {}",
                    String::from_utf8_lossy(&add_out.stderr)
                ),
            });
        }

        let commit_out = Command::new("git")
            .current_dir(target_worktree)
            .args(["commit", "-m", msg])
            .output()?;
        if !commit_out.status.success() {
            rollback()?;
            return Ok(ReconciliationOutcome::Failed {
                error: format!(
                    "git commit failed during reconciliation: {}",
                    String::from_utf8_lossy(&commit_out.stderr)
                ),
            });
        }

        let rev_out = Command::new("git")
            .current_dir(target_worktree)
            .args(["rev-parse", "HEAD"])
            .output()?;
        let sha = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();
        if sha.is_empty() { None } else { Some(sha) }
    } else {
        None
    };

    Ok(ReconciliationOutcome::Applied { merge_commit })
}

/// Safely reconciles a patch against a dedicated integration worktree.
///
/// Steps:
/// 1. Run `git apply --check --binary -` to verify patch applicability without modifying files.
/// 2. If check fails: return `Conflicted` with extracted conflicted files and unmodified worktree.
/// 3. If check succeeds: run `git apply --binary -` and optionally commit changes.
pub fn reconcile_patch(
    target_worktree: &Path,
    patch_data: &[u8],
    commit_message: Option<&str>,
) -> Result<ReconciliationOutcome, WorkspaceError> {
    if let Some(conflict) = check_patch(target_worktree, patch_data)? {
        return Ok(conflict);
    }
    apply_patch(target_worktree, patch_data, commit_message)
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
