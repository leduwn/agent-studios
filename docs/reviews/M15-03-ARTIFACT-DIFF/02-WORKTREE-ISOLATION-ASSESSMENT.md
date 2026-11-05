# M15-03 Worktree Isolation Assessment

**Milestone:** M15-03  
**Status:** Complete  
**Date:** 2026-11-01  
**Author:** Git Worktree Engineer & Systems Architect  

---

## 1. Problem Statement

Autonomous agent runtimes operate directly in managed Git worktree filesystems. When generating diffs to review changes, traditional Git workflows run commands like `git add -A` and `git diff --cached` directly against `.git/index`.

If executed directly in the shared worktree:
1. The developer's or runtime's index is modified, destroying the ability to inspect untracked vs tracked files cleanly.
2. Concurrent operations or crashes during diff calculation leave the Git index locked (`.git/index.lock`).
3. Worktree reconciliation or branch switching becomes hazardous due to index pollution.

---

## 2. Ephemeral Index Design

To eliminate all index pollution, `agent-studios-workspace::capture_changes` enforces isolated ephemeral Git indexes.

```rust
pub fn capture_changes(
    worktree_path: &Path,
    base_commit: &str,
) -> Result<CapturedChanges, WorkspaceError> {
    // 1. Allocate dedicated temp dir
    let temp_dir = tempfile::tempdir()?;
    let temp_index_path = temp_dir.path().join("temp_git_index");

    // 2. Read base commit tree into ephemeral index
    Command::new("git")
        .current_dir(worktree_path)
        .env("GIT_INDEX_FILE", &temp_index_path)
        .args(["read-tree", base_commit])
        .output()?;

    // 3. Stage worktree filesystem state into ephemeral index
    Command::new("git")
        .current_dir(worktree_path)
        .env("GIT_INDEX_FILE", &temp_index_path)
        .args(["add", "-A", "--", "."])
        .output()?;

    // 4. Extract diff between base commit and ephemeral index
    let diff_output = Command::new("git")
        .current_dir(worktree_path)
        .env("GIT_INDEX_FILE", &temp_index_path)
        .args(["diff", "--cached", "--binary", "--full-index", "--no-ext-diff", base_commit, "--", "."])
        .output()?;
...
```

---

## 3. Mathematical & Empirical Proof of Isolation

### 3.1 Index Invariant
Let $I_{real}$ be the file located at `<worktree>/.git/index`.  
Let $I_{ephem}$ be the file located at `<temp_dir>/temp_git_index`.

For all Git invocations during `capture_changes`:
$$\forall t \in [t_{start}, t_{end}], \quad \text{hash}(I_{real}(t)) = \text{hash}(I_{real}(t_{start}))$$

`GIT_INDEX_FILE` environment variable explicitly diverts Git's internal index path resolution (`read_index_from`, `write_locked_index`) to `$I_{ephem}`. The file `$I_{real}$` is never opened for write, and no `.git/index.lock` is ever created in the repository or worktree.

### 3.2 Crash Safety
Because `$I_{ephem}$` resides in a `tempfile::TempDir`, Rust's RAII drop semantics guarantee that when the function returns or if a panic occurs:
1. The temporary directory and `$I_{ephem}` are automatically unlinked.
2. Even in the event of `SIGKILL`, the temporary directory resides in the operating system's temp folder, leaving the repository completely clean.

---

## 4. Race Hazard Analysis

| Hazard Scenario | Analysis | Mitigation in M15-03 |
|---|---|---|
| Concurrent runtime file writes during capture | Git reads files sequentially; uncommitted files may be captured mid-write. | Capture is executed *after* the runtime emits its terminal event (`SessionCompleted`, `SessionFailed`, `Stopped`). The runtime is quiescent before capture starts. |
| Worktree directory deleted before capture | `capture_changes` receives a non-existent path. | `capture_and_register_worktree_artifact` checks `wt_root.exists()` and returns early with a logged warning if deleted. |
| Stale lock file in worktree | A previous external command left `.git/index.lock`. | `capture_changes` uses `$I_{ephem}`, so it completely bypasses `.git/index.lock`. Capture succeeds even if `.git/index.lock` is present. |
| Extremely large worktree (>100k files) | `git add -A` overhead. | Executed asynchronously off the main IPC event loop via `tokio::task::spawn_blocking` inside `capture_changes_async`. |
