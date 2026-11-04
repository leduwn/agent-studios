# Agent Studios — Milestone M14-UI-00: Bridge Contract Specification

## Overview

The `AgentStudiosBridge` interface decouples the Code-OSS extension UI from the underlying multi-agent execution engine. This specification details the interface methods, connection states, data structures, event envelope sequencing, and fail-closed error contracts.

---

## 1. Interface Definition (`AgentStudiosBridge`)

```typescript
export interface AgentStudiosBridge {
  readonly connectionState: ConnectionState;
  readonly onConnectionStateChanged: vscode.Event<ConnectionState>;
  readonly onEvent: vscode.Event<TimelineEvent>;

  connect(): Promise<void>;
  disconnect(): Promise<void>;

  getCurrentSession(): Promise<SessionInfo | null>;
  getTaskGraph(runId?: string): Promise<TaskGraph | null>;
  listTasks(): Promise<TaskNode[]>;
  listPendingApprovals(): Promise<ApprovalRequest[]>;
  listArtifacts(): Promise<Artifact[]>;
  getArtifactDiff(artifactId: string): Promise<string | null>;
  listProviderSummaries(): Promise<ProviderSummary[]>;

  requestApprovalDecision(id: string, decision: 'approved' | 'denied'): Promise<ApprovalDecisionResult>;
  submitEditorAction(action: EditorActionRequest): Promise<EditorActionResult>;

  dispose(): void;
}
```

---

## 2. Connection States

| State | Semantics |
|---|---|
| `connected` | Daemon / Bridge is online and actively streaming events. Queries succeed. |
| `disconnected` | Daemon is absent or intentionally disconnected. UI displays placeholders; mutations fail closed. |
| `reconnecting` | Bridge is attempting to re-establish transport connection (IPC-03). |
| `error` | Unrecoverable transport or protocol failure. |

---

## 3. Data Structures

### 3.1 `SessionInfo`
```typescript
export interface SessionInfo {
  id: string;
  workspacePath: string;
  goalSummary: string;
  status: 'idle' | 'running' | 'paused' | 'completed' | 'failed';
  activeRunId?: string;
  activeAgentCount: number;
  totalAgentCount: number;
  completedTasksCount: number;
  totalTasksCount: number;
  providerSummary: string;
  startedAt: number;
  elapsedMs: number;
  isPreview?: boolean;
}
```

### 3.2 `TaskNode`
```typescript
export interface TaskNode {
  id: string;
  runId: string;
  title: string;
  description: string;
  status: 'pending' | 'ready' | 'running' | 'succeeded' | 'failed' | 'blocked' | 'cancelled';
  assignedAgentId?: string;
  assignedAgentName?: string;
  dependencies: string[];
  subtasks?: string[];
  progress?: number;
  error?: string;
  createdAt: number;
  updatedAt: number;
}
```

### 3.3 `ApprovalRequest`
```typescript
export interface ApprovalRequest {
  id: string;
  sessionId: string;
  taskId?: string;
  taskTitle?: string;
  actionType: 'exec_command' | 'write_file' | 'delete_file' | 'network_request' | 'elevate_permissions';
  commandOrTarget: string;
  riskLevel: 'low' | 'medium' | 'high' | 'critical';
  requestedPermissions: string[];
  status: 'pending' | 'approved' | 'denied' | 'timed_out';
  createdAt: number;
}
```

### 3.4 `TimelineEvent`
```typescript
export interface TimelineEvent {
  sequenceNumber: number;
  id: string;
  timestamp: number;
  type:
    | 'session_started'
    | 'session_ended'
    | 'run_started'
    | 'run_completed'
    | 'task_started'
    | 'task_completed'
    | 'task_failed'
    | 'task_blocked'
    | 'agent_assigned'
    | 'tool_executing'
    | 'approval_requested'
    | 'approval_decided'
    | 'artifact_created';
  summary: string;
  agentName?: string;
  taskId?: string;
  details?: Record<string, unknown>;
}
```

---

## 4. Event Envelope Sequencing & Ordering

To prevent race conditions across async IPC boundaries, events follow strict sequencing rules:

1. **Monotonic Progression**: Each event has a discrete integer `sequenceNumber` strictly greater than its predecessor.
2. **Duplicate Detection**: The projection compares `event.sequenceNumber <= lastProcessedSequence`. Matching or lower sequences are discarded immediately without mutations.
3. **Gap Detection**: If `event.sequenceNumber > lastProcessedSequence + 1`, a sequence gap occurred (e.g. dropped socket packets). The state transitions to `'degraded'` and fires `onStateDegraded`.
4. **Buffer Limit**: The timeline stores at most 1000 events. Oldest events are pruned FIFO when the capacity is exceeded.

---

## 5. Fail-Closed Error Contract (`BridgeDisconnectedError`)

When disconnected from the engine:
- Any approval decision call (`requestApprovalDecision`) or editor action (`submitEditorAction`) **must fail closed**.
- A `BridgeDisconnectedError` is thrown, with `applied: false` and a descriptive message.
- Mutation operations are rejected before reaching any network, shell, or file write paths.

---

## 6. M13 / IPC-03 Integration Strategy

In milestone M13/IPC-03, an `Ipc03Bridge` will be added implementing `AgentStudiosBridge`:
- Transport: Unix domain socket on POSIX, Named Pipe (`\\.\pipe\agent-studios-...`) on Windows.
- Protocol: JSON-RPC 2.0 with framing or binary Protobuf.
- The `BridgeManager` will dynamically instantiate `Ipc03Bridge` when the daemon socket is discovered in the workspace directory (`.agent-studios/daemon.sock`).
