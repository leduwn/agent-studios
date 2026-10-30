# M15-01 Security & Correctness Audit Report: Real AI Coding Agent Execution Contract & Per-Task Routing

**Milestone**: M15-01  
**Target Branch**: `feat/m15-01-per-task-execution-contract` (stacked on `feat/m13-ipc-02-control-plane` / PR #19)  
**Author**: Principal Systems Architect (Claude 1)  
**Audit Date**: 2026-10-30  
**Status**: VERIFIED & PASSING  

---

## 1. Executive Summary

Milestone M15-01 delivers version-aware, additive task execution contracts (`TaskExecutionSpec`), truthful authenticated readiness reporting (`IpcCommand::GetExecutionReadiness`), and deterministic per-task execution routing across multiple repositories, runtime preferences, provider instances, and AI models.

Prior daemon architecture was limited to singletons configured at server boot, preventing clients from selecting specific authorized workspaces, runtime implementations, or decoupled AI models on a per-task basis. M15-01 resolves this limitation while strictly adhering to fail-closed security invariants, physical worktree isolation, secret redaction, and 100% backward compatibility for existing M13 callers.

---

## 2. Core Architectural & Security Invariants

### 2.1 Workspace Authorization & Path Traversal Prevention
- **Canonicalization**: All repository paths (`TaskExecutionSpec.repository_path` and `IpcServerBuilder::with_authorized_repository_path`) are canonicalized via `dunce::canonicalize` to eliminate Windows UNC prefixes (`\\?\`) and cross-platform symlink escapes.
- **Git Repository Validation**: Each requested repository path is strictly verified for `.git` presence (directory or gitlink file). Non-git directories immediately fail closed without spawning processes.
- **Strict Allowlist Containment**: Every requested task repository path must be an exact match or a child of an authorized repository root configured on `IpcServer`. Path traversal attempts (e.g., `../../etc`) or unauthorized directories fail closed with clear error attribution, transitioning the run and task to `Failed`.

### 2.2 Truthful, Secret-Free Execution Readiness Audit
- **Command & Response**: `IpcCommand::GetExecutionReadiness` -> `IpcResponse::ExecutionReadiness { readiness: ExecutionReadinessReport }`.
- **Authentication Gating**: Unauthenticated clients requesting readiness are rejected with `PermissionDenied` prior to executing discovery.
- **Secret Redaction**: `ExecutionReadinessReport` is audited to ensure zero leakage of API keys, bearer tokens, HMAC secrets, or credential paths. Only non-sensitive metadata (instance IDs, implementation IDs, friendly labels, capabilities, model IDs, context windows, authorized workspace paths) is emitted.

### 2.3 Deterministic Runtime Preference & Instance Routing
- **Runtime Preference**: When a task specifies `runtime_preference` (e.g., `"mock-test-runtime"`, `"opencode"`), the daemon validates that the available runtime implementation matches. Mismatches fail closed immediately.
- **Instance Disambiguation**: When multiple runtime instances are discovered, tasks must disambiguate via `runtime_instance_id` unless a configured default instance ID is set on the server. Ambiguous multi-instance states fail closed.

### 2.4 Decoupled Provider Catalog Model Routing
- **Provider & Model Catalog**: Built-in provider definitions (`anthropic`, `openai`) and user-registered instances decouple model selection from runtime implementations.
- **Deterministic Resolution**:
  - `(provider_instance_id, model_id)`: Exact pair lookup in catalog.
  - `(None, model_id)`: Unambiguous lookup across all provider instances. If 0 or >1 instances match, fails closed.
  - `(provider_instance_id, None)`: Unambiguous single model in provider instance.
  - `(None, None)`: Default server model ref or single model in catalog.
- **Zero Silent Fallback**: Unknown, invalid, or unresolvable model IDs fail closed immediately.

### 2.5 Physical Git Worktree Isolation & Prompt Integrity
- **Physical Isolation**: Dynamic provisioning of isolated physical git worktrees (`orchestrator.create_worktree_async`) outside Control Plane write locks with atomic rollback on failure.
- **Initial Prompt Integrity**: When `TaskExecutionSpec.initial_prompt` is provided, it is delivered verbatim to `RuntimeStartRequest` without synthesized `"Title: Description"` concatenation. Backward-compatible synthesis is retained only when `initial_prompt` is `None`.

### 2.6 Subtree Boundary Enforcement
- **`codex-rs/` Subtree**: 100% untouched (0 diffs, 0 bytes modified).
- **Claude 2 Worktrees**: Zero modifications to UI branches (PR #23, PR #25).

---

## 3. Threat Model & Failure Mode Analysis

| Threat / Scenario | Mitigation / Defense | Resulting Behavior |
| :--- | :--- | :--- |
| Client requests unlisted repository | Allowlist check against canonical paths | Run transitions to `Failed`, error recorded in outcome |
| Client attempts directory traversal (`../../`) | Canonicalization via `dunce` + prefix check | Fail closed, no worktree created |
| Target directory lacks `.git` | `git_dir` validation check | Fail closed, no execution launched |
| Client requests unknown model ID | Provider catalog deterministic lookup | Fail closed, no silent fallback to cheap/costly model |
| Client requests unavailable runtime | Discovery status + implementation ID match | Fail closed, clear diagnostic logged |
| Unauthenticated client calls readiness | HMAC session authentication gate | Protocol `PermissionDenied` error |
| Worktree creation collision or failure | Exponential retry + atomic cleanup | Rollback, run marked `Failed`, no leak |

---

## 4. Verification Verdict

All core security invariants and failure scenarios have been implemented, verified, and proven with end-to-end integration test coverage in `agent-studios-ipc-transport` and `agent-studios-ipc-protocol`.
