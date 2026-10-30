# M15 — Task Submission & Streaming Execution Presentation Design

**Document Reference**: `docs/reviews/M15-UI-EXECUTION/03_TASK_SUBMISSION_AND_STREAMING_DESIGN.md`  
**Milestone**: M14 Final Integration Freeze & M15 UI Execution Preparation  
**Auditor**: Principal VS Code Extension Engineer & Multi-Agent UX Engineer  
**Date**: 2026-10-30  
**Status**: APPROVED DESIGN & IMPLEMENTATION SPECIFICATION  

---

## 1. Architectural Mission

The goal of M15 is to deliver a seamless, verified user journey:
$$\text{User opens VS Code} \longrightarrow \text{Enters prompt} \longrightarrow \text{Selects runtime \& model} \longrightarrow \text{Submits to agsd} \longrightarrow \text{Observes live stream in Chat \& Mission Control} \longrightarrow \text{Reviews outcome}$$

### Strict Invariant: No False Claims
This journey is **not** complete until it has actually executed against an external AI coding runtime in an end-to-end environment. Seeded mock tasks prove UI fidelity and state projection, but they must never be presented as real completed AI coding work.

---

## 2. Typed Submit-Task Adapter Specification

The UI uses `SubmitTaskAdapter.ts` to dispatch `IpcCommand::SubmitTask` with payload `task_spec: BatchTaskSpec`.

### 2.1 Request Structure
```typescript
export interface SubmitTaskRequest {
  studioId: string;
  prompt: string;
  confirmedByUser: boolean;
  requestedModelId?: string;
  requestedProviderId?: string;
  workspacePath?: string;
  customKey?: string;
}
```

### 2.2 Wire Serialization Alignment
Rust `BatchTaskSpec` field names use `snake_case`. The TypeScript client maps the prompt into `title` (first line, capped at 100 chars) and `description` (full prompt verbatim):

```typescript
const taskSpec: RustBatchTaskSpec = {
  key: req.customKey || `task-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`,
  title: preflight.sanitizedSpec.title,
  description: preflight.sanitizedSpec.description,
  assigned_agent_id: null,
  parent_task_key: null,
  parent_task_id: null,
  dependency_keys: [],
  dependency_task_ids: [],
  dependency_output_policy: 'task_success',
};
```

### 2.3 Genuine Task Correlation
The client never invents or predicts task UUIDs. It extracts the authoritative UUID from `IpcResponse::TasksSubmitted`:
```typescript
if (resp.type !== 'tasks_submitted' || !resp.task_ids || resp.task_ids.length === 0) {
  throw new TaskSubmissionError('Daemon returned empty task_ids array upon successful submission.');
}
const assignedTaskId = resp.task_ids[0];
```

---

## 3. Streaming Event Consumption & Presentation

### 3.1 Emitted Backend Events
The Rust daemon emits the following events over the IPC subscription stream:

1. `task_submitted`: Initial submission confirmation (`{ task_id, studio_id }`).
2. `task_state_transitioned`: Lifecycle state change (`{ task_id, from, to }`).
3. `run_state_transitioned`: Coordinator execution state change (`{ run_id, from, to }`).
4. `runtime_output_delta`: Streaming token chunks (`{ task_id, run_id, text }`).
5. `tool_started`: Discrete tool invocation (`{ tool_name, call_id }`).
6. `tool_completed` / `tool_failed`: Tool outcome (`{ tool_name, duration_ms, error }`).
7. `approval_requested` / `approval_decided`: Two-phase approval barrier (`{ approval_id, risk_level, status }`).

### 3.2 Single Authoritative Presentation Architecture
Chat and Mission Control share a single authoritative state store (`SessionStateProjection` backed by `BridgeManager`):

```
                        +----------------------+
                        | Real agsd Daemon     |
                        +----------+-----------+
                                   | Length-Prefixed Wire Frames
                                   v
                        +----------------------+
                        | NativeIpcTransport   |
                        +----------+-----------+
                                   | RustIpcEvent
                                   v
                        +----------------------+
                        | DaemonConnectedBridge|
                        +----------+-----------+
                                   | TimelineEvent
                                   v
                        +----------------------+
                        |SessionStateProjection|
                        +----+------------+----+
                             |            |
             +---------------+            +---------------+
             v                                            v
+--------------------------+                +--------------------------+
| ChatWebviewViewProvider  |                | Mission Control Webview  |
| - Streaming text output  |                | - Live DAG task status   |
| - Tool execution cards   |                | - Attention queue        |
| - Interactive approvals  |                | - Monotonic timeline     |
+--------------------------+                +--------------------------+
```

### 3.3 Authoritative Progress & Indeterminate Semantics
- **Succeeded Tasks**: Rendered at exactly `100%`.
- **Running / Pending Tasks Without Metrics**: Progress is `undefined`, rendered as `Indeterminate` (animated shimmer or spinner).
- **Zero Fabrication**: Progress bars never fabricate synthetic percentages (e.g. jumping to 50% or 0% when unmeasured).
- **Token Counts & Latency**: Latency is displayed as `unmeasured` until measured via real IPC round-trip ping. No hardcoded constants.
