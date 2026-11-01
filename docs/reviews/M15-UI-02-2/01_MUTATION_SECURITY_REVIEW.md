# M15 UI-02.2 — Mutation Security Review: Capability Policy & Legacy Fallback Elimination

## Metadata
- **Milestone**: M15 UI-02.2
- **Topic**: Mutation Authorization, Tri-State Capability Enforcement, Production Capability Security
- **Target Branch**: `feat/m15-real-agent-execution-ui` (Draft PR #27)
- **Base Branch**: `feat/m14-mission-control-connected-integration`
- **Pinned Backend Commit**: `61cb7e88e4ae0e3a0432134b77eee0c30f56c302` (PR #28 / PR #26)
- **Date**: 2026-11-02

---

## 1. Executive Summary

Milestone M15 UI-02.2 eliminates unsafe legacy execution fallbacks and hardens capability validation across the Agent Studios Extension Host and IPC boundary.

Prior to M15 UI-02.2:
1. Missing `runtime_capabilities` in daemon readiness reports could inadvertently allow task execution without verified capability guarantees.
2. Extension Host environment variables could be consulted as daemon configuration fallbacks.
3. Production capability overrides relied on `NODE_ENV` as the sole security boundary.

M15 UI-02.2 implements strict fail-closed capability verification:
- Mutating operations (`task_execution`, `task_cancellation`) require explicit, verified capability metadata. Missing capability metadata fails closed for mutating tasks while preserving read-only backward compatibility with legacy daemon responses.
- Tri-state capability enforcement strictly rejects `unsupported` or `unknown` capabilities.
- Production capability controls statically disable `task.submit` (`enabled: false`), preventing untrusted webviews or generic extension host callers from triggering mutating agent execution.
- Dynamic capability overrides and test dialog handlers are strictly prohibited in production packages and outside active test runner contexts.

---

## 2. Tri-State Runtime Capability Enforcement

### 2.1 Capability Evaluation Matrix

Runtime capabilities are reported per runtime instance in `RustExecutionReadinessReport.runtime_capabilities`. Each capability entry evaluates strictly to:
- `'supported'`
- `'unsupported'`
- `'unknown'`

| Action | Required Capabilities | Evaluation Policy |
| :--- | :--- | :--- |
| `task_execution` | `streaming_events: 'supported'`<br>`stop: 'supported'` | Fail-closed if capabilities metadata is missing or if any required capability is not `'supported'`. |
| `task_cancellation` | `stop: 'supported'` | Fail-closed if capabilities metadata is missing or if `stop` is not `'supported'`. |
| `read_only` | None required | Passes without capability checks. Preserves backward compatibility with M15-01 read-only clients. |

### 2.2 Action-Specific Fail-Closed Invariants

In `CapabilityMatrix.validateRuntimeInstanceSelection`:
1. If `action !== 'read_only'`:
   - If `readiness.runtime_capabilities` is null, undefined, or empty: task submission or cancellation immediately fails closed with an explicit error:
     `Runtime capabilities metadata missing from daemon readiness report. Action '<action>' requires verified capabilities: <requiredCaps>.`
   - If the specific runtime instance ID is not found in `readiness.runtime_capabilities`: validation fails closed.
   - If any required capability evaluates to `unsupported`, `unknown`, or is absent: validation fails closed with:
     `Runtime instance '<id>' lacks required '<cap>' capability (status: '<status>').`
2. If `action === 'read_only'`:
   - Read-only inspection passes without requiring mutating capability metadata, ensuring backwards compatibility for query-only workflows against older daemons.

---

## 3. Production Capability Security Architecture

### 3.1 Static Capability Registry Policy

In `CapabilityMatrix.ts`, `task.submit` is statically configured as disabled:
```typescript
{
  id: 'task.submit',
  name: 'Task Submission',
  description: 'Submits new tasks to the daemon task queue for agent execution',
  category: 'execution',
  enabled: false, // Statically disabled in production
  requiresConfirmation: true,
  riskLevel: 'high',
}
```

### 3.2 Dual Security Barrier for Capability Overrides

Dynamic capability overrides via `CapabilityMatrix.setCapabilityEnabled()` are strictly isolated:
1. **Production Mode Prohibition**: If `process.env.NODE_ENV === 'production'`, an exception is unconditionally thrown.
2. **Active Test Runner Boundary**: Overrides are permitted ONLY when running inside an active test runner environment where test globals `describe` and `it` exist, or `NODE_ENV === 'test'`.
3. **No Webview Access**: Webview code running in iframe or browser context has no mechanism to invoke `setCapabilityEnabled()` or alter extension host capability state.
4. **No Generic Extension Host Escape**: Public API surface exposes no backdoor or environment variable to enable `task.submit` in production builds.

---

## 4. Architectural Separation

```
+-----------------------------------------------------------------------------+
|                           UNTRUSTED WEBVIEW (Chat UI)                       |
|  - Renders user prompt, studio selection, discovered runtime dropdowns      |
|  - Has NO access to signing keys, HMAC generation, or capability overrides   |
|  - Cannot dispatch raw IPC commands or bypass operator modal               |
+-----------------------------------------------------------------------------+
                                      |
                           vscode.postMessage
                                      v
+-----------------------------------------------------------------------------+
|                      TRUSTED EXTENSION HOST BOUNDARY                        |
|  1. ChatWebviewViewProvider receives prompt submission                      |
|  2. Evaluates CapabilityMatrix.isCapabilityEnabled('task.submit')           |
|  3. Invokes OperatorConfirmationModal (native VS Code modal dialog)          |
|  4. Signs one-time challenge token bound to daemon sessionId                |
|  5. Submits to SubmitTaskAdapter                                            |
+-----------------------------------------------------------------------------+
                                      |
                       Strict Preflight & Fresh Readiness
                                      v
+-----------------------------------------------------------------------------+
|                           IPC DISPATCH TO AGSD                              |
|  - Serializes framed BatchTaskSpec with TaskExecutionSpec                   |
|  - Asserts frame bounds <= 4 MiB                                            |
|  - Correlates returned task UUIDs with Control Plane event stream           |
+-----------------------------------------------------------------------------+
```
