# M15 UI-02 — Multi-Runtime Backend Integration Architecture & Security Review

## Metadata
- **Milestone**: M15 UI-02
- **Topic**: Multi-Runtime Backend Integration, Typed Runtime Selection, Authoritative Dispatch Revalidation & Safety Gates
- **Target Branch**: `feat/m15-real-agent-execution-ui` (Draft PR #27)
- **Base Branch**: `feat/m14-mission-control-connected-integration`
- **Pinned Backend Commit**: `61cb7e88e4ae0e3a0432134b77eee0c30f56c302` (PR #28 / PR #26)
- **Date**: 2026-10-31

---

## 1. Architectural Overview

Milestone M15 UI-02 establishes end-to-end integration between the VS Code Extension and the multi-runtime execution architecture provided by the authoritative Rust backend daemon (`agsd`).

```
+----------------------------------------------------------------------------------------------------+
|                                    VS CODE EXTENSION HOST                                          |
|                                                                                                    |
|  +-------------------------+      +--------------------------+      +---------------------------+  |
|  | ChatWebviewViewProvider | ---> |    CapabilityMatrix      | ---> |     SubmitTaskAdapter     |  |
|  | - Discovered options    |      | - Discovered Instance ID |      | - HMAC Challenge verify   |  |
|  | - No silent auto-pick   |      | - Implementation Pref    |      | - No model_ref on wire    |  |
|  | - Strict disabled states|      | - Tri-state caps check   |      | - Bounded frame check     |  |
|  +-------------------------+      +--------------------------+      +---------------------------+  |
|               |                                                                   |                |
|               | (Distinct option: `${instance_id}|${impl}`)                       |                |
|               v                                                                   v                |
|  +-------------------------+                                        +---------------------------+  |
|  |   TaskExecutionStore    |                                        |  Native Command Transport |  |
|  | - Track instance & pref |                                        | - Length-prefixed framed  |  |
|  | - Daemon event corr     |                                        | - 4 MiB frame boundary    |  |
|  +-------------------------+                                        +---------------------------+  |
+-----------------------------------------------------------------------------------|----------------+
                                                                                    |
                                                              IPC (Named Pipe / UDS)|
                                                                                    v
+----------------------------------------------------------------------------------------------------+
|                                    RUST DAEMON (agsd)                                              |
|                                                                                                    |
|   Pinned Commit: 61cb7e88e4ae0e3a0432134b77eee0c30f56c302                                         |
|   - TaskExecutionSpec: { runtime_instance_id, runtime_preference, model_id, provider_instance_id } |
|   - Zero model_ref references                                                                     |
|   - Strict discovery, tri-state capability reporting, process isolation                            |
+----------------------------------------------------------------------------------------------------+
```

---

## 2. Core Security & Safety Guarantees

### 2.1 Distinct Runtime Identities & Discovered Instance Resolution
1. **Implementation Preference vs. Discovered Instance ID**:
   - `runtime_preference`: Identifies the runtime implementation kind (e.g. `'opencode'`, `'fake-alpha'`).
   - `runtime_instance_id`: Identifies the concrete discovered daemon runtime instance (e.g. `'rt-inst-alpha-01'`).
   - In UI select elements, options use composite identifiers: `${instance_id}|${implementation}`. Multiple instances of the same implementation maintain distinct selectable identities and are never collapsed.

2. **No Silent Auto-Picking**:
   - When multiple healthy instances exist without an explicit configured default in `readiness.defaults.configured_instance_id`, `ChatWebviewViewProvider` leaves `selectedRuntime` empty and requires explicit user selection.

3. **Fail-Closed Selection Semantics**:
   - Any selection targeting an unhealthy instance (`healthy: false`), an unknown instance ID, an unknown implementation, or a duplicate instance identity fails closed and blocks task submission.

4. **Elimination of Hardcoded Production Defaults**:
   - Hardcoded fallbacks to `'opencode'` or `'claude_code'` have been eradicated.
   - Native Claude Code (`claude_code`) and Codex (`codex`) adapters are visibly marked disabled and fail closed unless the daemon authoritatively reports them in `readiness.available_runtimes` with healthy discovered instances.

5. **Tri-State Capability Matrix**:
   - Runtimes reporting `'unsupported'` or `'unknown'` for essential capabilities (`streaming_events`, `stop`) are rejected prior to dispatch.

### 2.2 Revalidation at Dispatch Time
- Runtime selections are revalidated at the moment of task dispatch against fresh daemon readiness. If a runtime degraded or vanished after the operator opened the workbench, task execution is immediately aborted rather than redirected.

### 2.3 Cryptographic Operator Confirmation Binding
- The HMAC-SHA256 operator confirmation token cryptographically binds:
  - `studioId`
  - `promptHash`
  - `workspacePath`
  - `modelId`
  - `providerId`
  - `runtimePreference`
  - `runtimeInstanceId`
  - `intent: 'task_execution'`
- Tokens are single-use and atomically consumed upon dispatch. Tampering with any parameter invalidates the challenge.

### 2.4 Authoritative Multi-Runtime Wire Contract
- Wire shape strictly adheres to Rust `IpcCommand::SubmitTask` with `BatchTaskSpec`.
- The wire payload contains `runtime_preference`, `runtime_instance_id`, `provider_instance_id`, `model_id`, `repository_path`, and `initial_prompt`.
- **Zero `model_ref`**: Adheres strictly to the M15 backend contract, which completely removed `model_ref`.

---

## 3. Regression Test Verification Suite

All 15 required scenarios + backward compatibility verified deterministically in `MultiRuntimeBackendIntegration.test.ts`:

| # | Scenario | Test Name | Result |
|---|---|---|:---:|
| 1 | Two implementations with different healthy instances | `Scenario 1: distinguishes two implementations with different healthy instances` | PASS |
| 2 | Two healthy instances sharing one implementation | `Scenario 2: manages two healthy instances sharing one implementation without collision` | PASS |
| 3 | Registered but unavailable adapter | `Scenario 3: marks registered but unavailable adapter as disabled and fails closed on submit` | PASS |
| 4 | Unsupported/unknown runtime capability | `Scenario 4: fails closed when runtime reports unsupported required capabilities` | PASS |
| 5 | Unknown implementation or instance | `Scenario 5: rejects unknown instance ID and unknown implementation` | PASS |
| 6 | Duplicate instance identity | `Scenario 6: detects duplicate instance identity and disables execution` | PASS |
| 7 | Missing default with ambiguous selection | `Scenario 7: rejects ambiguous selection when multiple instances exist and default is missing` | PASS |
| 8 | Correct configured default | `Scenario 8: honors correct configured default and rejects unhealthy configured default` | PASS |
| 9 | Exact implementation/instance propagation (no model_ref) | `Scenario 9: propagates exact execution fields on wire and never includes model_ref` | PASS |
| 10 | Model/provider identity propagation | `Scenario 10: cryptographically binds and propagates model_id and provider_instance_id` | PASS |
| 11 | Stale selection after readiness refresh | `Scenario 11: rejects stale selection when selected instance degrades after refresh` | PASS |
| 12 | Operator challenge runtime binding | `Scenario 12: fails verification if runtime preference or instance ID is altered after challenge issued` | PASS |
| 13 | Failed routing after successful submission acknowledgement | `Scenario 13: transitions TaskExecutionStore to failed state on post-acknowledgement failure` | PASS |
| 14 | Chat/Mission Control event correlation | `Scenario 14: correlates run and execution parameters from typed daemon events` | PASS |
| 15 | Cancellation, replay and reconnect | `Scenario 15: dispatches typed cancellation and transitions store safely` | PASS |
| 16 | Backward compatibility: M15-01 readiness | `Scenario 16: cleanly handles M15-01 readiness payload missing M15-02 optional fields` | PASS |

Total Unit Tests: **522 passing**, 0 failing.
Typecheck: **0 errors**.
Lint: **0 errors** (456 stylistic naming warnings).
Packaging: **VSIX built successfully**.
CI Pinned Backend Commit: **`61cb7e88e4ae0e3a0432134b77eee0c30f56c302`**.
