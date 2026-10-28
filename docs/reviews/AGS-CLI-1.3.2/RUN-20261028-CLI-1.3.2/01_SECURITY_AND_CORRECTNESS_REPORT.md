# Milestone AGS CLI 1.3.2 — CI Hotfix & Physical Worktree Quality Gate Audit

- **Milestone**: `AGS CLI 1.3.2 CI Hotfix & Physical Worktree Verification`
- **Execution Run**: `RUN-20261028-CLI-1.3.2`
- **Date**: `2026-10-28T17:30:00+07:00`
- **Branch**: `feat/ags-cli-tui-foundation`
- **Target PR**: `#16` (`https://github.com/leduwn/agent-studios/pull/16`)
- **Status**: **VERIFIED & COMPLIANT (STRICT NO SELF-MERGE)**

---

## 1. Executive Summary & Defect Remediation Matrix

This audit evaluates and confirms the resolution of the two GitHub Actions CI failures in PR #16:
1. Ubuntu CI compilation failure on platform-specific export `JobObjectGuard` in `crates/opencode-runtime/src/lib.rs:25`.
2. Windows CI physical worktree allocation failure on `test_submit_goal_and_task_graph` in `crates/cli-client/tests/client_tests.rs`.

| Defect / Invariant | Prior Failure Symptom | Engineering Hotfix | Verification Status |
|---|---|---|:---:|
| **Subagent A — Cross-Platform Export** | `error[E0432]: unresolved import sandbox::JobObjectGuard` on Ubuntu Linux CI | Gated `JobObjectGuard` export behind `#[cfg(windows)]` in `crates/opencode-runtime/src/lib.rs:25` and conditionalized imports in integration tests | **PASS** |
| **Subagent B — Physical Worktree Isolation** | `Failed to allocate physical worktree for execution: Worktree creation failed: cannot populate managed worktree` in Windows CI | Provisioned dedicated temporary Git repository fixtures and custom orchestrator instances; added `with_working_dir` support on `EmbeddedClient`; validated physical isolation (`worktree_root != source_root`) | **PASS** |
| **Nested Repository Execution** | Execution from nested paths (e.g. `agent-studios-rs`) failed safe cwd validation if nested directory was uncommitted | Ensured committed fixture layout with nested workspace path; verified Git top-level root discovery and safe working directory resolution | **PASS** |
| **Source Dirty State Preservation** | Untracked or modified files in developer working copy risked leakage or corruption | Verified uncommitted dirty files in source repository are completely omitted from physical worktrees and preserved untouched in source | **PASS** |
| **Test Concurrency & Git Locks** | Concurrent tests running against host repository collided on Git locks (`.git/worktrees/...`) and shallow clone limits | Isolated all test suites (`client_tests`, `subscription_tests`) with dedicated `tempdir()` fixtures, eliminating lock contention and dropping test runtime from >120s to <15s | **PASS** |
| **Upstream Integrity** | Vendor dependencies in `codex-rs/` must remain completely unmodified | Zero modifications to `codex-rs/` tree; verified `git status` and `git diff` | **PASS** |

---

## 2. Technical Findings Deep Dive

### 2.1. Subagent A — Ubuntu Cross-Platform Export Resolution (`crates/opencode-runtime`)
- **Root Cause**: `JobObjectGuard` encapsulates Win32 Job Object handles and is defined only under `#[cfg(windows)]` in `crates/opencode-runtime/src/sandbox.rs:324`. In `crates/opencode-runtime/src/lib.rs:25`, `JobObjectGuard` was exported unconditionally:
  ```rust
  pub use sandbox::{JobObjectGuard, ProcessContainment, SandboxLevel, SandboxPolicy};
  ```
  On Ubuntu Linux runners, this triggered compilation error `error[E0432]: unresolved import sandbox::JobObjectGuard`.
- **Surgical Resolution**:
  ```rust
  pub use runtime::{OpenCodeActiveSession, OpenCodeRuntime};
  #[cfg(windows)]
  pub use sandbox::JobObjectGuard;
  pub use sandbox::{ProcessContainment, SandboxLevel, SandboxPolicy};
  ```
  Additionally, guarded `JobObjectGuard` imports in `tests/sandbox_tests.rs` and `tests/m11_sandbox_containment_tests.rs` with `#[cfg(windows)]`.

### 2.2. Subagent B — Windows Physical Worktree Failure Resolution (`crates/cli-client`)
- **Root Cause**:
  1. `client_tests.rs::test_submit_goal_and_task_graph` did not create a temporary repository fixture. It called `EmbeddedClient::bootstrap("test-workspace")`, which defaulted to `std::env::current_dir()` (`D:\a\agent-studios\agent-studios\agent-studios-rs`).
  2. In GitHub Actions, the runner checkout is shallow (`fetch-depth: 1`). During concurrent integration test execution, multiple processes attempted `git worktree add --detach` and `git --work-tree=. reset --hard <head_sha>` against the same repository, hitting Git index lock contention and commit dereference failures (`cannot populate managed worktree`).
  3. Worktree path allocation across drives (`D:` checkout to `C:\Users\runneradmin\AppData\Local\Temp\agent-studios\worktrees`) exacerbated path resolution and lock duration.
- **Surgical Resolution**:
  1. Added `with_working_dir` builder method to `EmbeddedClient`, allowing tests to specify an explicit working directory without mutating process-global state.
  2. Updated `client_tests.rs::test_submit_goal_and_task_graph` to provision an isolated temporary Git repository fixture using `tempdir()` and an isolated `WorkspaceOrchestrator`.
  3. Tested nested execution from `agent-studios-rs` directory inside the repository to verify that `codex_worktree` correctly resolves the root repository and verifies the safe working directory.
  4. Verified all 4 core isolation invariants:
     - `managed.root != source_root`: Physical directory separation.
     - `managed.head_sha == base_sha`: Deterministic clean base commit.
     - Source untracked dirty files remain present in source and absent from worktree.
     - Tracked file content in source remains untouched.
  5. Isolated `subscription_tests.rs` with `TestFixture` to prevent cross-test Git worktree lock contention, reducing test execution time from >120s to 13s.

---

## 3. Upstream & Security Invariants

1. **Codex Upstream Preservation**: Upstream `codex-rs/` is 100% untouched.
2. **Containment Realism**: No claims of filesystem virtualization from Windows Job Objects. File boundaries are strictly enforced via physical worktree directories (`WorkspaceOrchestrator`).
3. **Zero Bearer Secrets in Git**: No credentials, tokens, or sensitive variables added to git working copy or test fixtures.
4. **Strict No Self-Merge**: Branch `feat/ags-cli-tui-foundation` is updated for CI verification and PR #16 remains unmerged pending final human review.
