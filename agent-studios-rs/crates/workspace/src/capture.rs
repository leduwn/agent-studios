use std::path::Path;
use std::process::Command;

use crate::error::WorkspaceError;

/// Result of capturing a change-set from an isolated worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedChanges {
    pub patch_bytes: Vec<u8>,
    pub files_changed: usize,
    pub changed_files: Vec<String>,
    pub stats_summary: String,
    pub base_commit: String,
    pub head_commit: Option<String>,
}

/// Captures all modifications, new files, and deletions in a worktree against `base_commit`
/// using an isolated temporary Git index file.
///
/// Invariant: The real `.git/index` in the worktree is never modified.
pub fn capture_changes(
    worktree_path: &Path,
    base_commit: &str,
) -> Result<CapturedChanges, WorkspaceError> {
    if !worktree_path.is_dir() {
        return Err(WorkspaceError::InvalidOperation(format!(
            "worktree path {} is not a directory",
            worktree_path.display()
        )));
    }

    // Allocate a dedicated temporary directory for the ephemeral index.
    let temp_dir = tempfile::tempdir()?;
    let temp_index_path = temp_dir.path().join("temp_git_index");

    // 1. Read base tree into ephemeral index: git read-tree <base_sha>
    let output = Command::new("git")
        .current_dir(worktree_path)
        .env("GIT_INDEX_FILE", &temp_index_path)
        .args(["read-tree", base_commit])
        .output()?;
    if !output.status.success() {
        return Err(WorkspaceError::GitError(format!(
            "git read-tree failed on base {base_commit}: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    // 2. Stage worktree filesystem state into ephemeral index: git add -A -- .
    let output = Command::new("git")
        .current_dir(worktree_path)
        .env("GIT_INDEX_FILE", &temp_index_path)
        .args(["add", "-A", "--", "."])
        .output()?;
    if !output.status.success() {
        return Err(WorkspaceError::GitError(format!(
            "git add -A failed in worktree {}: {}",
            worktree_path.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    // 3. Compute cached binary diff: git diff --cached --binary --full-index --no-ext-diff <base_sha> -- .
    let diff_output = Command::new("git")
        .current_dir(worktree_path)
        .env("GIT_INDEX_FILE", &temp_index_path)
        .args([
            "diff",
            "--cached",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            base_commit,
            "--",
            ".",
        ])
        .output()?;
    if !diff_output.status.success() {
        return Err(WorkspaceError::GitError(format!(
            "git diff --cached failed against {base_commit}: {}",
            String::from_utf8_lossy(&diff_output.stderr)
        )));
    }
    let patch_bytes = diff_output.stdout;

    // 4. Compute file stats: git diff --cached --numstat <base_sha> -- .
    let stat_output = Command::new("git")
        .current_dir(worktree_path)
        .env("GIT_INDEX_FILE", &temp_index_path)
        .args(["diff", "--cached", "--numstat", base_commit, "--", "."])
        .output()?;

    let mut changed_files = Vec::new();
    let stats_summary = if stat_output.status.success() {
        let text = String::from_utf8_lossy(&stat_output.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                changed_files.push(parts[2].to_string());
            }
        }
        text.to_string()
    } else {
        String::new()
    };

    let files_changed = changed_files.len();

    // 5. Query HEAD commit sha
    let head_output = Command::new("git")
        .current_dir(worktree_path)
        .args(["rev-parse", "HEAD"])
        .output();
    let head_commit = match head_output {
        Ok(out) if out.status.success() => {
            let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !sha.is_empty() {
                Some(sha)
            } else {
                None
            }
        }
        _ => None,
    };

    // Ephemeral index is automatically cleaned up when temp_dir drops.
    drop(temp_dir);

    Ok(CapturedChanges {
        patch_bytes,
        files_changed,
        changed_files,
        stats_summary,
        base_commit: base_commit.to_string(),
        head_commit,
    })
}
