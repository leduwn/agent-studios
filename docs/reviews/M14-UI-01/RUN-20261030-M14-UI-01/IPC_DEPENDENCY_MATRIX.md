# Agent Studios — Milestone M14-UI-01: IPC Dependency Matrix

## Executive Summary
Milestone M14-UI-01 establishes the chat-first UI layer for Agent Studios. All UI features communicate through the `AgentStudiosBridge` interface abstraction. When the authoritative daemon IPC layer (Milestones M13/IPC-02 and M13/IPC-03) is attached, the mock `FixtureBridge` can be replaced with the real `DaemonIpcBridge` without touching any UI or view provider code.

---

## 1. Feature to Daemon IPC Method Mapping

| UI Feature | Bridge Method | Daemon IPC JSON-RPC Method | Request Payload | Response / Stream Format |
|---|---|---|---|---|
| **Chat Prompt Submission** | `bridge.sendMessage(request, onChunk)` | `agent.sendMessage` | `{ message: string, mode: "ask" \| "plan" \| "agent", modelId?: string, providerInstanceId?: string, attachments: ContextAttachment[] }` | SSE Stream: `text_delta`, `tool_call`, `task_progress`, `approval_required`, `message_complete` |
| **Run Cancellation** | `bridge.cancelCurrentRun()` | `agent.cancelRun` | `{ runId?: string }` | `{ cancelled: boolean, timestamp: number }` |
| **Human Approval Decision** | `bridge.requestApprovalDecision(id, decision)` | `approval.decide` | `{ approvalId: string, decision: "approved" \| "denied", reason?: string }` | `{ approvalId: string, applied: boolean, timestamp: number }` |
| **List Sessions** | `bridge.listSessions()` | `session.list` | `{}` | `SessionSummary[]` |
| **Create Session** | `bridge.createSession(goal)` | `session.create` | `{ goalSummary?: string }` | `SessionInfo` |
| **Switch Session** | `bridge.switchSession(sessionId)` | `session.switch` | `{ sessionId: string }` | `SessionInfo` |
| **Get Current Session** | `bridge.getCurrentSession()` | `session.getCurrent` | `{}` | `SessionInfo` |
| **Provider & Model Discovery** | `bridge.listProviderSummaries()` | `provider.listSummaries` | `{}` | `ProviderSummary[]` (Read-only; secrets stripped) |
| **List Tasks** | `bridge.listTasks()` | `task.list` | `{}` | `TaskItem[]` |
| **Get Task Graph (DAG)** | `bridge.getTaskGraph(runId)` | `task.getGraph` | `{ runId?: string }` | `TaskGraphData` (`nodes: TaskNode[]`, `edges: TaskEdge[]`) |
| **List Patch Artifacts** | `bridge.listArtifacts()` | `artifact.list` | `{}` | `Artifact[]` |
| **Get Artifact Diff** | `bridge.getArtifactDiff(id)` | `artifact.getDiff` | `{ artifactId: string }` | `{ diffContent: string, format: "unified" }` |
| **Reconcile Workspace File** | `reviewManager.reconcileWorkspaceFile(path, content, proof)` | `workspace.reconcileDiff` | `{ filePath: string, diffContent: string, proof: BackendReconciliationProof }` | `ReconciliationResult` (`{ success: boolean, reconciled: boolean }`) |
| **Editor Context Actions** | `bridge.submitEditorAction(action)` | `editor.submitAction` | `{ actionType: "explain" \| "refactor" \| "generateTests", filePath: string, selectedText: string, languageId: string }` | `EditorActionResult` |

---

## 2. Event Envelope & Sequencing Contract

### 2.1 Monotonic Sequence Ordering
The daemon emits events wrapped in an authoritative envelope:

```typescript
export interface TimelineEvent {
  readonly id: string;
  readonly sequenceNumber: number;  // Strictly monotonically increasing integer
  readonly timestamp: number;       // Unix epoch milliseconds
  readonly type: TimelineEventType;
  readonly agentId?: string;
  readonly agentName?: string;
  readonly summary: string;
  readonly details?: Record<string, unknown>;
}
```

### 2.2 Extension Host Handling Rules
Implemented in `SessionStateProjection.ts`:
1. **Monotonic Order**: Events are processed in strict ascending sequence order.
2. **Duplicate Suppression**: Events with `sequenceNumber <= lastProcessedSequence` are silently discarded.
3. **Gap Detection**: If `event.sequenceNumber > lastProcessedSequence + 1`, the projection transitions into degraded health state (`healthState: 'degraded'`) and queues a background resynchronization request (`session.getCurrent`).
4. **Bounded Event Buffer**: The in-memory buffer caps at 1000 events, pruning the oldest events FIFO to prevent memory leaks in long-running sessions.

---

## 3. Bridge Abstraction Architecture

```
                    +---------------------------+
                    |    AgentStudiosBridge     |  (Interface Contract)
                    +---------------------------+
                                  ^
         +------------------------+------------------------+
         |                        |                        |
+------------------+    +------------------+    +--------------------+
| Disconnected     |    | FixtureBridge    |    | DaemonIpcBridge    |
| Bridge           |    | (Preview Mode)   |    | (M13/IPC-03)       |
|                  |    |                  |    |                    |
| - Fail-closed    |    | - 3 Sessions     |    | - Domain socket /  |
| - 0 connections  |    | - Streaming SSE  |    |   named pipe       |
| - Safe default   |    | - Realistic mock |    | - Authoritative    |
+------------------+    +------------------+    +--------------------+
```

### 3.1 Transition from Preview to Production
- When running in **Preview Mode** (`agentStudios.previewMode: true`), `FixtureBridge` delivers realistic multi-turn streaming, mock approvals, task DAG updates, and artifacts.
- When `DaemonIpcBridge` is ready, it binds to the configured daemon endpoint (`agentStudios.daemonEndpoint`, default `ipc://agent-studios.sock`).
- If the socket connection drops, `BridgeManager` automatically switches to `DisconnectedBridge`, guaranteeing that all approvals and code modifications fail closed immediately.
