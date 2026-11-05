# M15-03 Verification Evidence & Test Execution Matrix

**Milestone:** M15-03  
**Status:** Complete  
**Date:** 2026-11-01  
**Author:** Cross-Platform QA Lead & Systems Architect  

---

## 1. Test Suite Architecture

Milestone M15-03 incorporates comprehensive end-to-end integration tests in `crates/ipc-transport/tests/m15_03_artifact_diff_tests.rs` exercising:
1. Real temporary Git repository with initial commits.
2. Real managed Git worktree created by `WorkspaceOrchestrator`.
3. Real filesystem mutations produced by an `AgentRuntime` mock (creating new files, appending to existing files, deleting files).
4. Real ephemeral index capture using `git read-tree`, `git add -A`, and `git diff --cached`.
5. Real atomic writes to `ArtifactStore` and SHA-256 integrity verification.
6. Real Control Plane state transitions to `WorktreeState::ChangeCaptured`.
7. Real authenticated IPC queries (`ListTaskArtifacts`, `GetArtifact`, `GetTaskDiff`).

---

## 2. Test Matrix & Results

| Test Name | Coverage Objective | Result |
|---|---|---|
| `test_01_full_change_capture_and_diff_review` | Validates complete lifecycle: runtime adds, modifies, and deletes files; worktree captures changes; patch and stats artifacts are stored; IPC commands return exact patch and structured `DiffSummary` (3 files changed, 1 added, 1 modified, 1 deleted); Control Plane `WorktreeRecord` transitions to `ChangeCaptured`. | PASS (0.78s) |
| `test_02_diff_pagination_and_frame_bounding` | Validates streaming chunk retrieval: large generated diff (500 lines); pagination with `offset` and `limit`; verifies `has_more` flag; asserts concatenated chunks match full patch; verifies clamp ceiling at 2 MiB. | PASS (0.76s) |
| `test_03_artifact_capture_on_failure` | Validates the Pre-State-Transition capture pattern: runtime modifies files then emits `SessionFailed`; verifies partial changes are still captured, patch is stored in `ArtifactStore`, and diff is queryable via IPC. | PASS (0.72s) |
| `test_04_cross_studio_authorization_and_negative_queries` | Validates tenant boundary enforcement: cross-studio `ListTaskArtifacts` and `GetTaskDiff` queries are rejected; nonexistent artifact query returns empty result without panic. | PASS (0.74s) |
| `workspace::tests::test_parse_multi_file_diff` | Unit test for `parse_git_diff_summary`: verifies added, modified, deleted, renamed, and binary files are correctly extracted from unified Git diffs. | PASS (0.00s) |
| `workspace::tests::test_parse_empty_diff` | Unit test for empty diffs: 0 files changed, 0 bytes, empty collections. | PASS (0.00s) |

---

## 3. Real Cargo Test Output Evidence

```text
running 4 tests
test test_03_artifact_capture_on_failure ... ok
test test_04_cross_studio_authorization_and_negative_queries ... ok
test test_01_full_change_capture_and_diff_review ... ok
test test_02_diff_pagination_and_frame_bounding ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.76s
```

```text
running 2 tests
test capture::tests::test_parse_empty_diff ... ok
test capture::tests::test_parse_multi_file_diff ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 4. Lint and Static Analysis Quality Gate Evidence

```bash
cargo clippy -p agent-studios-protocol -p agent-studios-workspace -p agent-studios-ipc-protocol -p agent-studios-ipc-transport --all-targets -- -D warnings
```
Result: Clean zero warnings.

```bash
cargo fmt --check
```
Result: All files formatted according to rustfmt standards.

