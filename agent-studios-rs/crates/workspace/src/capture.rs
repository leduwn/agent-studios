use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use agent_studios_protocol::artifact::DiffSummary;

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

impl CapturedChanges {
    /// Computes a structured `DiffSummary` from the captured patch bytes.
    pub fn diff_summary(&self) -> DiffSummary {
        parse_git_diff_summary(&self.patch_bytes, &self.base_commit)
    }
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
            if !sha.is_empty() { Some(sha) } else { None }
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

/// Parses a raw Git unified patch into a truthful, structured `DiffSummary`.
pub fn parse_git_diff_summary(patch_bytes: &[u8], base_commit: &str) -> DiffSummary {
    let mut added_files = Vec::new();
    let mut modified_files = Vec::new();
    let mut deleted_files = Vec::new();
    let mut renamed_files = Vec::new();
    let mut binary_files = Vec::new();
    let mut all_files = BTreeSet::new();

    let patch_str = String::from_utf8_lossy(patch_bytes);
    let mut current_file: Option<String> = None;
    let mut is_new = false;
    let mut is_deleted = false;
    let mut is_renamed = false;
    let mut is_binary = false;
    let mut rename_target: Option<String> = None;

    let finalize_current = |current: Option<String>,
                            is_new: bool,
                            is_deleted: bool,
                            is_renamed: bool,
                            is_binary: bool,
                            rename_target: Option<String>,
                            added_files: &mut Vec<String>,
                            modified_files: &mut Vec<String>,
                            deleted_files: &mut Vec<String>,
                            renamed_files: &mut Vec<String>,
                            binary_files: &mut Vec<String>,
                            all_files: &mut BTreeSet<String>| {
        if let Some(file) = current {
            all_files.insert(file.clone());
            if is_binary {
                binary_files.push(file.clone());
            }
            if is_new {
                added_files.push(file);
            } else if is_deleted {
                deleted_files.push(file);
            } else if is_renamed {
                let display = if let Some(target) = rename_target {
                    all_files.insert(target.clone());
                    format!("{file} -> {target}")
                } else {
                    file
                };
                renamed_files.push(display);
            } else if !is_binary {
                modified_files.push(file);
            }
        }
    };

    for line in patch_str.lines() {
        if line.starts_with("diff --git ") {
            finalize_current(
                current_file.take(),
                is_new,
                is_deleted,
                is_renamed,
                is_binary,
                rename_target.take(),
                &mut added_files,
                &mut modified_files,
                &mut deleted_files,
                &mut renamed_files,
                &mut binary_files,
                &mut all_files,
            );
            is_new = false;
            is_deleted = false;
            is_renamed = false;
            is_binary = false;

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let file_path = parts[3].strip_prefix("b/").unwrap_or(parts[3]);
                current_file = Some(file_path.to_string());
            }
        } else if line.starts_with("new file mode ") {
            is_new = true;
        } else if line.starts_with("deleted file mode ") {
            is_deleted = true;
        } else if line.starts_with("rename from ") {
            is_renamed = true;
            let from = line.strip_prefix("rename from ").unwrap_or("").trim();
            if !from.is_empty() {
                current_file = Some(from.to_string());
            }
        } else if line.starts_with("rename to ") {
            let target = line.strip_prefix("rename to ").unwrap_or("").trim();
            if !target.is_empty() {
                rename_target = Some(target.to_string());
            }
        } else if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
            is_binary = true;
        }
    }

    finalize_current(
        current_file.take(),
        is_new,
        is_deleted,
        is_renamed,
        is_binary,
        rename_target.take(),
        &mut added_files,
        &mut modified_files,
        &mut deleted_files,
        &mut renamed_files,
        &mut binary_files,
        &mut all_files,
    );

    DiffSummary {
        files_changed: all_files.len(),
        added_files,
        modified_files,
        deleted_files,
        renamed_files,
        binary_files,
        patch_size_bytes: patch_bytes.len(),
        base_commit: base_commit.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_diff() {
        let summary = parse_git_diff_summary(&[], "abc1234");
        assert_eq!(summary.files_changed, 0);
        assert_eq!(summary.patch_size_bytes, 0);
        assert_eq!(summary.base_commit, "abc1234");
        assert!(summary.added_files.is_empty());
        assert!(summary.modified_files.is_empty());
        assert!(summary.deleted_files.is_empty());
        assert!(summary.renamed_files.is_empty());
        assert!(summary.binary_files.is_empty());
    }

    #[test]
    fn test_parse_multi_file_diff() {
        let patch = r#"diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
+// new line
diff --git a/src/new.rs b/src/new.rs
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/src/new.rs
@@ -0,0 +1,5 @@
+fn test() {}
diff --git a/src/old.rs b/src/old.rs
deleted file mode 100644
index 4444444..0000000
--- a/src/old.rs
+++ /dev/null
@@ -1,5 +0,0 @@
-fn dead() {}
diff --git a/src/before.rs b/src/after.rs
similarity index 100%
rename from src/before.rs
rename to src/after.rs
diff --git a/asset.png b/asset.png
Binary files a/asset.png and b/asset.png differ
"#;

        let summary = parse_git_diff_summary(patch.as_bytes(), "base-sha");
        assert_eq!(summary.files_changed, 6); // lib.rs, new.rs, old.rs, before.rs, after.rs, asset.png
        assert_eq!(summary.modified_files, vec!["src/lib.rs"]);
        assert_eq!(summary.added_files, vec!["src/new.rs"]);
        assert_eq!(summary.deleted_files, vec!["src/old.rs"]);
        assert_eq!(summary.renamed_files, vec!["src/before.rs -> src/after.rs"]);
        assert_eq!(summary.binary_files, vec!["asset.png"]);
        assert_eq!(summary.patch_size_bytes, patch.len());
        assert_eq!(summary.base_commit, "base-sha");
    }
}
