# M15 UI-02.2 — Authoritative Readiness & Routing Audit

## Metadata
- **Milestone**: M15 UI-02.2
- **Topic**: Authoritative Execution Readiness, Per-Task Model Routing, Two-Phase Fresh Readiness Dispatch
- **Target Branch**: `feat/m15-real-agent-execution-ui` (Draft PR #27)
- **Base Branch**: `feat/m14-mission-control-connected-integration`
- **Pinned Backend Commit**: `61cb7e88e4ae0e3a0432134b77eee0c30f56c302` (PR #28 / PR #26)
- **Date**: 2026-11-02

---

## 1. Executive Summary

Milestone M15 UI-02.2 establishes authoritative execution readiness and model routing parity between the VS Code Extension Host and the Rust daemon (`agsd`).

Prior implementations permitted:
1. Fallbacks to Extension Host environment variables (`AGENT_STUDIOS_DEFAULT_MODEL`, `AGENT_STUDIOS_REPO_PATH`) when daemon configuration was missing.
2. Unverified model execution when the provider catalog was empty.
3. Potential dispatch race conditions where daemon readiness or runtime instances could degrade between user confirmation and IPC transmission.

M15 UI-02.2 enforces:
- **Mandatory Authoritative Readiness**: Every mutating task dispatch requires a fresh, authenticated `RustExecutionReadinessReport`. If missing or reports `is_ready: false`, dispatch fails closed.
- **Strict Catalog Validation**: Every explicit per-task model must be verified directly against `readiness.available_models`. An empty catalog unconditionally rejects execution.
- **Two-Phase Fresh Readiness Check**: Daemon readiness is queried immediately before operator modal presentation, and re-queried immediately prior to IPC frame dispatch.
- **Workspace Authorization & Containment**: Path containment is validated against `readiness.authorized_workspaces` and `readiness.defaults.default_repo_path`, blocking null bytes, UNC paths, and path traversal.

---

## 2. Per-Task Model & Provider Routing Parity

### 2.1 Backend Contract Parity (`connection.rs:1771-1827`)

The authoritative daemon resolves task execution parameters via `connection.rs`:
1. If `model_id` is explicitly specified in `TaskExecutionSpec`, it must exist in `available_models`. If `provider_instance_id` is also specified, it must match the model's provider instance.
2. If `model_id` is omitted in `TaskExecutionSpec`, the daemon falls back to `readiness.defaults.default_model_id` (or the single unambiguous model if exactly one model exists in the catalog).
3. If multiple models exist and no valid default is configured, the daemon fails closed.

### 2.2 Client-Side Preflight Enforcement

In `CapabilityMatrix.evaluateTaskSubmissionPreflight`:
- The client strictly asserts that `req.readiness` is present.
- If `req.readiness.available_models` is empty, preflight fails closed:
  `Daemon provider catalog is empty in execution readiness. No models available for execution.`
- If the user selects an explicit model:
  - If `readiness.available_models` contains the model and provider: the selection is validated and resolved as `resolvedModelId` and `resolvedProviderId`.
  - If the model is not found in `readiness.available_models`: preflight fails closed:
    `Requested model '<modelId>' is not available in daemon provider catalog.`
  - If the provider does not match the catalog entry: preflight fails closed:
    `Requested provider '<providerId>' does not match model provider '<actualProvider>'.`
- Client never falls back to `process.env.AGENT_STUDIOS_DEFAULT_MODEL` or any client-side environment variable to authorize execution.

---

## 3. Workspace Authorization & Containment

In `WorkspaceVerification.verify`:
1. **Normalization & Canonicalization**:
   - Rejects null bytes (`\0`).
   - Rejects foreign platform path formats (e.g. Windows paths on POSIX or POSIX absolute paths on Windows).
   - Rejects UNC network paths (`\\server\share`).
   - Rejects path traversal sequences (`..`).
2. **Containment Verification**:
   - Resolves target path against `readiness.authorized_workspaces` and `readiness.defaults.default_repo_path`.
   - Resolves symlinks and verifies that the canonical workspace root contains the canonical target path.
   - Any uncontained or unauthorized path fails closed with an explicit security error.

---

## 4. Two-Phase Fresh Readiness Dispatch Flow

To prevent race conditions where a daemon runtime instance crashes or becomes unhealthy while the operator confirmation modal is open:

```
[User initiates Task Submit]
            |
            v
[Phase 1: Pre-Modal Readiness Check]
  - Query authenticated daemon: get_execution_readiness()
  - Assert readiness.is_ready === true
  - Validate selected runtime instance & capabilities
            |
            v
[Operator Confirmation Modal]
  - Present native VS Code modal with exact runtime, model, workspace, and prompt preview
  - User clicks 'Confirm & Execute'
  - Issue cryptographic challenge token
            |
            v
[Phase 2: Pre-Dispatch Fresh Readiness Revalidation]
  - Immediately re-query daemon: get_execution_readiness()
  - Verify daemon remains ready (freshReadiness.is_ready === true)
  - Verify selected runtime instance is still registered, healthy, and supports required capabilities
  - IF ANY PARAMETER DEGRADED:
      -> Fail closed
      -> Invalidate challenge token
      -> Abort dispatch and notify operator
            |
            v
[Phase 3: IPC Wire Dispatch]
  - Verify and atomically consume challenge token
  - Encode and dispatch IpcCommand::SubmitTask
  - Correlate returned Rust task UUID
```
