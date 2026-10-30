# Mission Control Read Model & IPC Dependencies Specification
**Milestone:** M14 Mission Control Workbench  
**Status:** Approved Architectural Document  
**Date:** 2026-10-30  
**Target:** VS Code Extension Host (`apps/agent-studios-vscode`)  

---

## 1. Architectural Philosophy: Typed Read Models

Mission Control acts as a read-model projection over the state maintained by the Agent Studios Extension Host and the underlying `agsd` native daemon.

```
┌─────────────────────────────────┐
│     VS Code Extension Host      │
│                                 │
│  ┌───────────────────────────┐  │
│  │   AgentStudiosBridge      │  │
│  │ (DaemonConnected / Fixture)│  │
│  └─────────────┬─────────────┘  │
│                │                │
│  ┌─────────────▼─────────────┐  │
│  │  MissionControlProjection │  │
│  │   (Aggregates State)      │  │
│  └─────────────┬─────────────┘  │
│                │ postMessage    │
│  ┌─────────────▼─────────────┐  │
│  │   MissionControlPanel     │  │
│  │     (WebviewPanel)        │  │
│  └───────────────────────────┘  │
└─────────────────────────────────┘
```

Rather than burdening the webview with raw socket protocols, the Extension Host projects a clean, strictly typed, JSON-serializable `MissionControlState` object to the webview via `postMessage`.

---

## 2. Comprehensive Type Definitions

```typescript
export interface MissionControlState {
  version: string;
  timestamp: number;
  connectionState: ConnectionState;
  isPreview: boolean;
  activeStudio: StudioSummary | null;
  overview: OverviewProjection;
  fleet: AgentFleetMember[];
  tasks: TaskLifecycleModel[];
  taskGraph: TaskGraphData;
  runs: RunRecord[];
  timeline: TimelineEventRecord[];
  artifacts: ArtifactRecord[];
  approvals: ApprovalRecord[];
  providers: ProviderUsageSummary[];
  diagnostics: DiagnosticsRecord;
}

export type ConnectionState =
  | 'disconnected'
  | 'connecting'
  | 'authenticating'
  | 'connected'
  | 'reconnecting'
  | 'degraded'
  | 'auth_required'
  | 'authentication_failed'
  | 'incompatible'
  | 'unsupported_protocol';

export interface OverviewProjection {
  activeAgentCount: number;
  totalAgentsCount: number;
  taskCounts: {
    pending: number;
    ready: number;
    running: number;
    blocked: number;
    review: number;
    succeeded: number;
    failed: number;
    cancelled: number;
  };
  attentionItems: AttentionItem[];
  recentEvents: TimelineEventRecord[];
  activeRunCount: number;
}

export interface AttentionItem {
  id: string;
  severity: 'info' | 'warning' | 'error' | 'critical';
  title: string;
  description: string;
  targetTab: 'approvals' | 'tasks' | 'agents' | 'diagnostics';
  targetId?: string;
  timestamp: number;
}

export interface AgentFleetMember {
  agentId: string;
  name: string;
  role: string;
  status: 'idle' | 'running' | 'blocked' | 'failed' | 'completed';
  assignedTaskId?: string;
  assignedTaskTitle?: string;
  activeRunId?: string;
  runtime: string; // e.g., 'codex-rs', 'claude-code-cli', 'opencode'
  providerModel: string; // e.g., 'anthropic:claude-3-7-sonnet'
  lastActivityAt: number;
  toolCallCount: number;
  worktreePath?: string;
  error?: string;
}

export interface TaskLifecycleModel {
  id: string;
  studioId: string;
  title: string;
  description: string;
  state: 'pending' | 'ready' | 'running' | 'blocked' | 'review' | 'succeeded' | 'failed' | 'cancelled';
  dependencies: string[];
  assignedAgentId?: string;
  assignedAgentName?: string;
  createdAt: string;
  updatedAt: string;
  progress: number;
  worktreePath?: string;
}

export interface RunRecord {
  runId: string;
  taskId: string;
  agentId: string;
  runtime: string;
  status: 'queued' | 'running' | 'succeeded' | 'failed' | 'interrupted' | 'cancelled';
  startedAt: number;
  completedAt?: number;
  worktreePath?: string;
  toolCalls: ToolInvocationRecord[];
  terminalEvidence?: {
    exitCode?: number;
    command?: string;
    stdoutSnippet?: string;
    stderrSnippet?: string;
  };
  error?: string;
}

export interface ToolInvocationRecord {
  id: string;
  toolName: string;
  status: 'started' | 'succeeded' | 'failed';
  inputSummary: string;
  outputSummary?: string;
  timestamp: number;
}

export interface TimelineEventRecord {
  sequence: number;
  timestamp: string;
  type: string;
  taskId?: string;
  agentId?: string;
  runId?: string;
  summary: string;
  details?: Record<string, unknown>;
}

export interface ArtifactRecord {
  id: string;
  sessionId: string;
  taskId?: string;
  path: string;
  changeType: 'added' | 'modified' | 'deleted';
  diffSummary?: string;
  originalUri?: string;
  modifiedUri?: string;
  sizeBytes?: number;
}

export interface ApprovalRecord {
  id: string;
  sessionId: string;
  taskId?: string;
  actionType: string;
  commandOrTarget: string;
  riskLevel: 'low' | 'medium' | 'high';
  status: 'pending' | 'approved' | 'denied';
  createdAt: number;
  requestedPermissions: string[];
}

export interface ProviderUsageSummary {
  id: string;
  displayName: string;
  modelId: string;
  status: 'available' | 'degraded' | 'unavailable';
  contextWindow?: number;
  maxOutputTokens?: number;
  latencyMs?: number;
}

export interface DiagnosticsRecord {
  endpoint: string;
  sessionId: string | null;
  protocolVersion: string;
  agreedCapabilities: string[];
  watermark: number;
  activeClients: number;
  uptimeSecs: number;
  lastError: string | null;
}
```

---

## 3. IPC Wire Dependencies & Mapping

When operating in connected mode against `agsd`:
1. **Studios & Session:** Queried via `list_studios` and `get_studio`.
2. **Tasks:** Queried via `list_tasks(studio_id)`. Mapped directly from `RustTaskRecord`.
3. **Task Graph:** Constructed by topologically resolving `task.dependencies` into nodes and edges.
4. **Fleet Derivation:** Because the Rust daemon tracks agents as assignees on tasks and events, the Extension Host synthesizes `AgentFleetMember` records deterministically from task `assigned_agent_id` and incoming event headers.
5. **Providers & Models:** Queried via `list_providers` and `list_models(instance_id)`. Keys and secrets are strictly stripped.
6. **Streaming Events:** Ingested via `subscribe_events(from_sequence: watermark + 1)`.

---

## 4. Truthful Preview vs. Live Demarcation

- If `bridgeManager.isPreviewMode` is active:
  - `isPreview = true` is passed to the webview.
  - A prominent yellow/orange indicator badge labeled `[PREVIEW FIXTURE DATA]` is rendered at the top of Mission Control.
  - All mock records clearly indicate synthetic provenance.
- If connected live via `DaemonConnectedBridge`:
  - `isPreview = false`.
  - All data is verified against the socket stream.
  - Fail-closed guards prevent mutating commands from being accepted.
