# M15 Backend Execution Readiness Matrix

**Milestone**: M15-01: Real AI Coding Agent Execution Contract & Per-Task Routing  
**Evaluation Date**: 2026-10-30  
**Target Branch**: `feat/m15-01-per-task-execution-contract`  
**Base Branch**: `feat/m13-ipc-02-control-plane` (Pull Request #19)  
**Author**: Principal Systems Architect (Claude 1)  

---

## 1. Feature Implementation & Verification Matrix

| Capability / Requirement | Backend Implementation | Security & Boundary Enforcement | Test Verification | Status |
| :--- | :--- | :--- | :--- | :--- |
| **Per-Task Execution Contract** | `TaskExecutionSpec` added to `BatchTaskSpec` and `RunRecord` | Additive, optional, backward-compatible with M13 | `test_fixture_task_execution_spec_roundtrip`, `test_per_task_model_routing_and_prompt_integrity` | **COMPLETE** |
| **Execution Readiness API** | `IpcCommand::GetExecutionReadiness` -> `IpcResponse::ExecutionReadiness` | Authenticated HMAC only; zero secret/token exposure | `test_execution_readiness_report_truthful_and_secret_free`, `test_unauthenticated_client_readiness_rejected` | **COMPLETE** |
| **Workspace Authorization** | `IpcServerBuilder::with_authorized_repository_path`, `verify_workspace_authorization` | Canonicalized paths (`dunce`), `.git` verification, traversal escape prevention | `test_per_task_authorized_workspaces_and_unauthorized_rejection`, `test_per_task_non_git_and_traversal_workspace_fails_closed` | **COMPLETE** |
| **Runtime Selection & Routing** | `runtime_preference` & `runtime_instance_id` routing | Matches implementation ID; fails closed on unavailable instance | `test_per_task_model_routing_and_prompt_integrity` | **COMPLETE** |
| **Provider/Model Decoupled Catalog** | Provider instance & model catalog resolution | Deterministic pair/single lookup; fails closed on ambiguous/unknown model | `test_per_task_model_routing_and_prompt_integrity` | **COMPLETE** |
| **Initial Prompt Integrity** | Verbatim prompt pass-through to `RuntimeStartRequest` | Preserves exact prompt text, Unicode, and formatting | `test_per_task_model_routing_and_prompt_integrity` | **COMPLETE** |
| **Physical Worktree Isolation** | `WorkspaceOrchestrator::create_worktree_async` outside write locks | Atomic rollback on creation failure; path containment validation | `test_per_task_authorized_workspaces_and_unauthorized_rejection`, `m13_ipc_02_2_integration_tests` | **COMPLETE** |
| **M13 Backward Compatibility** | `execution: None` supported in `SubmitTask` | Legacy `"Title: Description"` prompt synthesis retained | `test_backward_compatible_m13_task_submission`, all 13 golden fixture tests | **COMPLETE** |
| **`codex-rs` Subtree Protection** | 0 diffs in `codex-rs/` | Strict subtree freeze | Verified via `git status -- codex-rs` (0 bytes modified) | **COMPLETE** |
| **Claude 2 Isolation** | PR #23 & PR #25 branches untouched | Zero branch cross-contamination | Verified via git branch status | **COMPLETE** |

---

## 2. Conclusion & Readiness Verdict

All backend prerequisites for M15-01 are complete, tested, and verified.
Backend is 100% ready for Claude 2 frontend integration in Mission Control and the Code-OSS connected client.
