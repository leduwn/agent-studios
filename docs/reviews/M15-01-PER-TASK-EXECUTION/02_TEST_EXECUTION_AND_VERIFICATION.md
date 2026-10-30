# M15-01 Test Execution & Verification Evidence

**Milestone**: M15-01  
**Target Branch**: `feat/m15-01-per-task-execution-contract`  
**Base Branch**: `feat/m13-ipc-02-control-plane` (PR #19)  
**Date**: 2026-10-30  
**Status**: 100% GREEN (ALL TEST SUITES PASSING)  

---

## 1. Test Suite Summary Table

| Test Suite / Package | Scope | Tests Run | Passed | Failed | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `m15_execution_contract_tests` | End-to-end M15 execution routing, readiness, validation | 6 | 6 | 0 | **PASS** |
| `golden_fixture_tests` | IPC serialization, tagging, roundtrip, backward compatibility | 13 | 13 | 0 | **PASS** |
| `m13_ipc_02_2_integration_tests` | Worktree lifecycle, cancellation, approvals, determinism | 17 | 17 | 0 | **PASS** |
| `control_plane_ipc_tests` | Control plane IPC commands, runs, tasks, studios | 11 | 11 | 0 | **PASS** |
| `agent-studios-ipc-protocol` | Framing, capabilities, auth HMAC, replay, redaction | 51 | 51 | 0 | **PASS** |
| `agent-studios-control-plane` | In-memory store, task graph, transactions, reconciler | 54 | 54 | 0 | **PASS** |

**Total Verified Tests Across Crate Dependencies**: 152+ passing tests.

---

## 2. Dedicated M15 Integration Test Suite (`m15_execution_contract_tests.rs`)

### 2.1 Test Catalog & Scenarios Verified
1. **`test_execution_readiness_report_truthful_and_secret_free`**:
   - Authenticated client requests execution readiness via `client.get_execution_readiness()`.
   - Verifies discovered runtimes (Mock Test Instance), models (Claude 3.7 Sonnet, GPT-4o), authorized workspaces (Repo A and Repo B), and defaults.
   - Verifies zero leakage of API secrets, keys, or internal bearer credentials.
2. **`test_unauthenticated_client_readiness_rejected`**:
   - Unauthenticated client sending raw `IpcCommand::GetExecutionReadiness` is rejected with `PermissionDenied`.
3. **`test_per_task_authorized_workspaces_and_unauthorized_rejection`**:
   - Task targeting authorized Repository A succeeds and creates isolated worktree.
   - Task targeting authorized Repository B succeeds and creates isolated worktree.
   - Task targeting unauthorized repository fails closed: run state transitions to `Failed`, task transitions to `Failed`, no worktree created.
4. **`test_per_task_non_git_and_traversal_workspace_fails_closed`**:
   - Non-git directory fails closed with `InvalidGitRepository` error.
   - Path traversal attempt (`../../etc`) fails closed with `PathTraversalAttempt` error.
5. **`test_per_task_model_routing_and_prompt_integrity`**:
   - Explicit model pair `(provider_instance_id, model_id)` routes to targeted provider and model.
   - Complex prompt with Unicode and code blocks preserved verbatim without concatenation.
   - Unambiguous model routing (specifying only `model_id`) resolves deterministically across providers.
   - Unknown model ID (`"nonexistent-super-model"`) fails closed immediately.
   - Runtime preference mismatch (`"claude-code"` requested when only `"mock-test-runtime"` available) fails closed immediately.
6. **`test_backward_compatible_m13_task_submission`**:
   - Legacy M13 task submission (`execution: None`) succeeds, retains legacy `"Title: Description"` prompt synthesis, and resolves to default catalog model.

---

## 3. Quality Gates Verification

### 3.1 Rustfmt Check
```powershell
Push-Location D:\Projects\agent-studios-m15\agent-studios-rs;
cargo fmt -p agent-studios-protocol -p agent-studios-ipc-protocol -p agent-studios-ipc-transport -p agent-studios-control-plane -p agent-studios-internal-agent -- --check;
Pop-Location
```
**Result**: Clean pass (exit code 0, 0 formatting diffs).

### 3.2 Subtree Isolation Verification
```powershell
git -C D:\Projects\agent-studios-m15 status -- codex-rs
```
**Result**: 100% clean (`nothing to commit, working tree clean`). Zero bytes or lines modified in `codex-rs/`.
