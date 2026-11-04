# M15 — Effective Capability Matrix & Truthfulness Specification

**Document Reference**: `docs/reviews/M15-UI-EXECUTION/10_EFFECTIVE_CAPABILITY_MATRIX.md`  
**Milestone**: M14 Final Integration Freeze & M15 UI Execution Preparation  
**PR**: `https://github.com/leduwn/agent-studios/pull/25`  
**Target Base**: `feat/m14-mission-control-connected-integration`  
**Author**: Principal VS Code Extension Architect & Capability Matrix Lead  
**Date**: 2026-10-30  
**Status**: SYNCHRONIZED ACROSS EXTENSION, BRIDGE & TESTS  

---

## 1. Six Granular Capability Dimensions

To eliminate ambiguity between backend protocol definitions and actual client UI capabilities, Agent Studios evaluates capabilities across six orthogonal dimensions:

1. **Backend Command Exists**: Does `IpcCommand` variant exist in `agent-studios-protocol`?
2. **Wire Protocol Tested**: Has the binary framing, serialization, and response parsing been verified in unit fixtures (`RustIpcContractFixtures.test.ts`)?
3. **Caller Authorized**: Does the caller hold required credentials (standard client key vs admin key vs operator confirmation challenge)?
4. **Extension Feature Wired**: Is the client-side adapter or UI command implemented (`SubmitTaskAdapter`, `ApprovalCommands`, `MissionControlWebview`)?
5. **Production Feature Enabled**: Is the capability enabled in the production `CAPABILITY_REGISTRY`?
6. **Real Execution E2E Verified**: Has end-to-end execution against real compiled `agsd` been proven in automated E2E tests?

---

## 2. Definitive Capability Dimension Matrix

| Capability ID | Tier | (1) Rust Cmd | (2) Wire Tested | (3) Authorized | (4) UI Wired | (5) Prod Enabled | (6) Real E2E | Status / Notes |
|---|---|:---:|:---:|:---:|:---:|:---:|:---:|---|
| `daemon.health` | `READ_ONLY_LIVE` | Yes | Yes | Client | Yes | **Yes** | Yes | Live telemetry query verified |
| `daemon.ping` | `READ_ONLY_LIVE` | Yes | Yes | Client | Yes | **Yes** | Yes | RTT latency query verified |
| `studio.list` | `READ_ONLY_LIVE` | Yes | Yes | Client | Yes | **Yes** | Yes | Studio catalog query verified |
| `task.list` | `READ_ONLY_LIVE` | Yes | Yes | Client | Yes | **Yes** | Yes | Task catalog query verified |
| `run.list` | `READ_ONLY_LIVE` | Yes | Yes | Client | Yes | **Yes** | Yes | Run history query verified |
| `provider.list` | `READ_ONLY_LIVE` | Yes | Yes | Client | Yes | **Yes** | Yes | Secret scrubbing verified |
| `event.stream` | `READ_ONLY_LIVE` | Yes | Yes | Client | Yes | **Yes** | Yes | Watermark replay deduplicated |
| `task.submit` | `TASK_SUBMISSION_READY` | Yes | Yes | Operator Token | Yes | **No** | Read-Only | Gated: requires M15 backend PRs |
| `approval.resolve` | `APPROVAL_READY` | Yes | Yes | Operator Token | Yes | **No** | Fixture Only | Read-only vertical slice in M14 |
| `task.cancel` | `CANCEL_READY` | Yes | Yes | Operator Token | Yes | **No** | Fixture Only | Read-only vertical slice in M14 |
| `artifact.review` | `ARTIFACT_REVIEW_READY` | No | No | N/A | Partial | **No** | No | Requires backend PR-M15-03 |
| `model.per_task_override` | `UNSUPPORTED` | No | No | N/A | No | **No** | No | Requires backend PR-M15-01 |
| `runtime.per_task_adapter` | `UNSUPPORTED` | No | No | N/A | No | **No** | No | Requires backend PR-M15-02 |
| `chat.multi_turn_prompt` | `UNSUPPORTED` | No | No | N/A | No | **No** | No | Requires backend PR-M15-04 |
| `daemon.restart_recovery` | `UNSUPPORTED` | No | No | N/A | No | **No** | No | Requires backend PR-M15-05 |

---

## 3. Bridge Synchronization & Preview Mode Truthfulness

1. **`DaemonConnectedBridge` (Live Connected Mode)**:
   - Operates strictly as a read-only vertical slice in M14.
   - `cancelCurrentRun()` throws a fail-closed `Error` to prevent fabricating cancellation in the UI.
   - `requestApprovalDecision()` returns `{ applied: false }` to prevent fabricating approval decisions in the UI.
   - `submitTask()` enforces `CapabilityMatrix.evaluateTaskSubmissionPreflight` and fails closed when `task.submit` is disabled.

2. **`BridgeManager`**:
   - `isCapabilityEnabled(capabilityId)`: returns `true` for `approval.resolve` and `task.cancel` ONLY when active bridge is `FixtureBridge` (preview simulation mode). In `DaemonConnectedBridge` or disconnected mode, delegates strictly to `CapabilityMatrix.isCapabilityEnabled(capabilityId)`.
   - `getCapabilityDescriptor(capabilityId)`: dynamically synchronizes the `enabled` boolean with the active bridge mode.

3. **UI Command Truthfulness**:
   - `ApprovalCommands` returns `ApprovalDecisionResult` (`applied: boolean`).
   - `ChatWebviewViewProvider` inspects `applied` flag before rendering UI checkmarks, preventing false positive indications on denied or unsupported actions.

---

## 4. Model and Provider Routing Parity

Parity with `crates/ipc-transport/src/connection.rs` lines 1771-1827:
- Rust `agsd` routes all tasks exclusively via daemon startup defaults (`AGENT_STUDIOS_DEFAULT_MODEL`).
- `SubmitTaskAdapter.verifyDaemonConfiguration` queries `ListModels` and `ListProviders` to verify catalog consistency.
- If the user requests a model (`requestedModelId`) that differs from the daemon's verified default model, `CapabilityMatrix` and `SubmitTaskAdapter` fail closed with a hard rejection error. Silent fallback or fake overrides are strictly prohibited.
