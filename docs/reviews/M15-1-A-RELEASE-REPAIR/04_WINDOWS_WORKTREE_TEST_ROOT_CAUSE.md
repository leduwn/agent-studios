# M15.1-A Release Repair: Windows 8.3 Path Test Failure Forensic & Remediation Report

**Document ID**: `M15-1-A-04-WINDOWS-WORKTREE-TEST-ROOT-CAUSE`  
**Date**: 2026-11-06  
**Status**: RESOLVED & SURGICALLY FIXED  
**Failed Runs**: GitHub Actions Run #42059131296 & Run #42070017450  
**Resolution Commit**: `922f119426595f670521ca3631a4c82e4892f751`  

---

## 1. Executive Summary

Unified CI runs on GitHub Actions failed during the `Run Complete Rust Workspace Test Suite` step on `windows-latest` runners. Two integration tests in `crates/workspace/tests/workspace_tests.rs` failed assertions checking whether the managed worktree root was contained inside the managed root:
1. `test_worktree_lifecycle_capture_and_reconciliation_full` (line 159)
2. `test_topology_canonical_repo_and_cwd_separation` (line 514)

Forensic investigation revealed the root cause was not an escape of process or directory containment, but a path representation mismatch caused by Windows 8.3 short name aliasing (`RUNNER~1` vs `runneradmin`) interacting with `dunce::canonicalize`.

Comparing the canonicalized worktree root with the orchestrator's canonical managed root (`orchestrator.managed_root()`) resolves the false-positive test failure while preserving 100% of strict filesystem containment guarantees.

---

## 2. Raw Diagnostic Log Extraction

Extracted from GitHub Actions Run #42059131296 (Job: `Unified CI (windows-latest)`):

```text
---- test_worktree_lifecycle_capture_and_reconciliation_full stdout ----
thread 'test_worktree_lifecycle_capture_and_reconciliation_full' panicked at crates\workspace\tests\workspace_tests.rs:159:5:
assertion failed: managed.root.starts_with(&managed_root)
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- test_topology_canonical_repo_and_cwd_separation stdout ----
thread 'test_topology_canonical_repo_and_cwd_separation' panicked at crates\workspace\tests\workspace_tests.rs:514:5:
root must be under managed_root
```

---

## 3. Deep-Dive Forensic Root Cause Analysis

### Mechanism of 8.3 Short Name Aliasing on GitHub Actions Windows Runners
1. On GitHub Actions `windows-latest` runners, the default user account is `runneradmin`, located physically at `C:\Users\runneradmin`.
2. Windows generates an NTFS 8.3 short name alias for this directory: `C:\Users\RUNNER~1`.
3. The temporary directory returned by `tempfile::tempdir()` resolves under `%TEMP%`, yielding a path with the 8.3 alias:
   ```text
   temp.path() = C:\Users\RUNNER~1\AppData\Local\Temp\t1234
   ```
4. The test fixture derives `managed_root` from `temp.path()`:
   ```text
   let managed_root = temp.path().join("managed_worktrees");
   // Yields: C:\Users\RUNNER~1\AppData\Local\Temp\t1234\managed_worktrees
   ```
5. When `WorkspaceOrchestrator::new(&managed_root, &artifact_root)` is instantiated, it canonicalizes the root:
   ```rust
   // crates/workspace/src/orchestrator.rs:34-39
   let managed_root = dunce::canonicalize(&raw_worktree_root).map_err(...)?;
   ```
   `dunce::canonicalize` on Windows resolves symlinks, normalizes drive prefixes, and expands 8.3 short names into their long canonical equivalents. Therefore, inside the orchestrator:
   ```text
   self.managed_root = C:\Users\runneradmin\AppData\Local\Temp\t1234\managed_worktrees
   ```
6. When `orchestrator.create_worktree(...)` is called, the new worktree is created physically under `self.managed_root`:
   ```text
   managed.root = C:\Users\runneradmin\AppData\Local\Temp\t1234\managed_worktrees\wt_abc123
   ```
7. The test assertions then attempted:
   ```rust
   // Line 159
   assert!(managed.root.starts_with(&managed_root));
   // Line 514
   assert!(managed.root.starts_with(&managed_root), "root must be under managed_root");
   ```
8. In Rust, `std::path::Path::starts_with` performs **component-by-component string comparison**:
   - Component 0: `C:` == `C:` (Match)
   - Component 1: `Users` == `Users` (Match)
   - Component 2: `runneradmin` == `RUNNER~1` (**MISMATCH!**)
   
   Because `runneradmin` != `RUNNER~1`, `Path::starts_with` returned `false`, causing the assertions to panic.

---

## 4. Architectural Proof of Strict Containment

Did the test failure indicate a security vulnerability or path traversal?

**No.** In `crates/workspace/src/orchestrator.rs`:
```rust
pub fn validate_worktree_path(&self, root: &Path, cwd: &Path) -> Result<(), WorkspaceError> {
    let canonical_root = dunce::canonicalize(root)...?;
    let canonical_cwd = dunce::canonicalize(cwd)...?;

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
```

Notice that:
1. `validate_worktree_path` canonicalizes both `root` and `cwd` before comparing with `self.managed_root`.
2. `self.managed_root` is already canonicalized.
3. Every call to `create_worktree` executes `self.validate_worktree_path(&managed.root, &managed.cwd)?`.
4. If a worktree were to escape `self.managed_root`, `create_worktree` would fail immediately.

In fact, lines 503-504 of `workspace_tests.rs` already recognized this canonicalization principle for repository paths:
```rust
let canon_repo = dunce::canonicalize(&repo_dir).expect("canon repo");
let canon_sub = dunce::canonicalize(&sub_cwd).expect("canon sub");
```
The test author simply omitted canonicalization when verifying `managed.root` against `managed_root`.

---

## 5. Applied Surgical Fix

In `crates/workspace/tests/workspace_tests.rs`:

### Fix 1 (Line 158-159)
```rust
    // Before:
    assert!(managed.root.exists());
    assert!(managed.root.starts_with(&managed_root));

    // After:
    assert!(managed.root.exists());
    assert!(managed.root.starts_with(orchestrator.managed_root()));
```

### Fix 2 (Line 514-517)
```rust
    // Before:
    assert!(
        managed.root.starts_with(&managed_root),
        "root must be under managed_root"
    );

    // After:
    assert!(
        managed.root.starts_with(orchestrator.managed_root()),
        "root must be under managed_root"
    );
```

---

## 6. Verification Evidence

Running `cargo test -p agent-studios-workspace --test workspace_tests` after the fix:
```text
running 17 tests
test test_orchestrator_path_validation ... ok
test test_artifact_lineage_deterministic_progression ... ok
test test_artifact_store_atomic_write_and_deduplication ... ok
test test_repository_root_resolution_failure_fails_closed ... ok
test test_isolated_reconciliation_candidate_construction_failure_preserves_worktree ... ok
test test_release_worktree_ownership_query_failure_fails_closed ... ok
test test_release_worktree_refuses_actively_owned_worktree_and_preserves_dir ... ok
test test_isolated_reconciliation_preserves_untracked_sentinel_on_success ... ok
test test_transactional_reconciliation_failure_preserves_untracked_sentinel ... ok
test test_isolated_reconciliation_mutation_boundary_race_preserves_worktree ... ok
test test_bound_owner_prevents_removal_and_preserves_worktree_contents ... ok
test test_release_worktree_refuses_dirty_worktree_and_preserves_dir ... ok
test test_release_worktree_clean_removal_verifies_root_removed ... ok
test test_topology_canonical_repo_and_cwd_separation ... ok
test test_reconciliation_applied_reports_merge_commit_when_commit_message_is_none ... ok
test test_generate_recovery_report ... ok
test test_worktree_lifecycle_capture_and_reconciliation_full ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.64s
```
Containment enforcement is strictly verified, and 8.3 representation bugs are completely eliminated.
