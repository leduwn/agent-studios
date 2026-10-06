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
    if !target_worktree.is_dir() {
        return Err(WorkspaceError::InvalidOperation(format!(
            "target worktree {} is not a directory",
            target_worktree.display()
        )));
    }

    // Empty patch has nothing to apply.
    if patch_data.is_empty() {
        return Ok(ReconciliationOutcome::Applied { merge_commit: None });
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
        return Ok(ReconciliationOutcome::Conflicted {
            conflicted_files,
            reason: stderr_str,
        });
    }

    // 2. Real application: git apply --binary -
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
        return Ok(ReconciliationOutcome::Failed {
            error: format!(
                "git apply failed after check succeeded: {}",
                String::from_utf8_lossy(&apply_output.stderr)
            ),
        });
    }

    // 3. Optional commit if message is provided
    let merge_commit = if let Some(msg) = commit_message {
        let add_out = Command::new("git")
            .current_dir(target_worktree)
            .args(["add", "-A", "--", "."])
            .output()?;
        if !add_out.status.success() {
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

/// Parses git apply error messages to discover which files conflicted.
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
                if !file.is_empty()
                    && !file.contains(' ')
                    && !files.iter().any(|f| f == file)
                {
                    files.push(file.to_string());
                }
            }
        }
    }
    files
}
