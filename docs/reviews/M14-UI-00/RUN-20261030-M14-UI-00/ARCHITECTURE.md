# Agent Studios — Milestone M14-UI-00: Architecture Document

## Overview

Milestone M14-UI-00 establishes the Code-OSS Native Agents Workbench for Agent Studios. The extension integrates multi-agent orchestration, execution visibility, human-in-the-loop approvals, and diff artifact review directly into the Code-OSS / VS Code developer environment.

```
+-----------------------------------------------------------------------------------+
|                           VS Code / Code-OSS Host                                |
|                                                                                   |
|  +---------------------------+  +-----------------------------------------------+  |
|  | Activity Bar Container    |  | Main Editor Area                              |  |
|  | (agent-studios-activitybar)| |                                               |  |
|  |                           |  |  +--------------------+ +-------------------+ |  |
|  |  [Session TreeView]       |  |  | TaskGraphWebview   | | SessionDetails    | |  |
|  |  [Tasks TreeView]         |  |  | (Interactive DAG)  | | Webview           | |  |
|  |  [Approvals TreeView]     |  |  +--------------------+ +-------------------+ |  |
|  |  [Artifacts TreeView]     |  |                                               |  |
|  |  [Timeline TreeView]      |  |  +-----------------------------------------+ |  |
|  |  [Providers (Read-Only)]  |  |  | Virtual Diff View (agent-studios-diff)  | |  |
|  |                           |  |  +-----------------------------------------+ |  |
|  +-------------+-------------+  +-----------------------+-----------------------+  |
|                |                                        |                          |
|                +-------------------+--------------------+                          |
|                                    |                                               |
|                    +---------------+---------------+                               |
|                    |     SessionStateProjection    |                               |
|                    |  - 1000 Event FIFO Buffer     |                               |
|                    |  - Monotonic Sequencing       |                               |
|                    |  - Sequence Gap Degradation   |                               |
|                    +---------------+---------------+                               |
|                                    |                                               |
|                    +---------------+---------------+                               |
|                    |         BridgeManager         |                               |
|                    +---------------+---------------+                               |
|                                    |                                               |
|               +--------------------+--------------------+                          |
|               |                                         |                          |
|    +----------+----------+                   +----------+----------+               |
|    |  DisconnectedBridge |                   |    FixtureBridge    |               |
|    |  (Fail-Closed)      |                   |    (Preview Mode)   |               |
|    +---------------------+                   +---------------------+               |
|                                                         |                          |
|    (Future M13/IPC-03):                      +----------+----------+               |
|    [ Unix Domain Socket / Named Pipe IPC ]   | Daemon / Backend    |               |
+------------------------------------------------------------------------------------+
```

---

## 1. Single Activity Bar Container

All native workbench views are consolidated inside a single dedicated Activity Bar container:
- **Container ID**: `agent-studios-activitybar`
- **Icon**: `$(hubot)` / native VS Code theme icon
- **Placement**: Primary sidebar Activity Bar

### Architectural Rationale
1. **Zero Workspace Clutter**: Grouping all agent workbench views under a single activity bar container prevents polluting standard Explorer or Source Control sidebars.
2. **Cohesive Context**: Developers maintain a unified mental model for multi-agent execution, viewing status, tasks, pending human approvals, artifacts, and activity streams side by side.
3. **Progressive Disclosure**: High-level status is immediately visible in the sidebar, while deeper analytical visualizations (DAG graph, full session details) open on demand in the editor area.

---

## 2. Native TreeViews vs. Webviews

Agent Studios deliberately balances native Code-OSS `TreeDataProvider` components and isolated `WebviewPanel` surfaces:

| Component Type | Components | Key Characteristics & Tradeoffs |
|---|---|---|
| **Native TreeViews** | `SessionTreeProvider`<br>`TasksTreeProvider`<br>`ApprovalsTreeProvider`<br>`ArtifactsTreeProvider`<br>`TimelineTreeProvider`<br>`ProvidersTreeProvider` | - Native VS Code styling, theme tokens, and typography.<br>- Lightweight memory footprint (no separate Chromium process/DOM).<br>- Full keyboard accessibility, screen reader support, and native keybindings.<br>- Instant rendering and low battery/CPU overhead.<br>- Native action buttons on hover and inline context menus. |
| **Custom Webviews** | `TaskGraphWebview`<br>`SessionDetailWebview` | - Rendered only when explicitly invoked (progressive disclosure).<br>- Interactive SVG DAG rendering with cubic bezier curves.<br>- Pan, zoom, node selection, and interactive subtask breakdown.<br>- Sandboxed via cryptographic nonces (`nonce-${nonce}`) and strict Content Security Policy (CSP).<br>- Disposed when closed to free webview resources. |

---

## 3. Bridge Abstraction Layer

The workbench UI interacts with backends solely through the `AgentStudiosBridge` interface.

### Implementations

1. **`DisconnectedBridge` (Default / Fail-Closed)**
   - Initial state when no daemon connection is active.
   - Read operations return `null` or empty arrays.
   - **Fail-Closed Security**: Approval decisions (`requestApprovalDecision`) and editor actions (`submitEditorAction`) unconditionally throw `BridgeDisconnectedError` with `applied: false`. No unauthorized command or permission can be granted when disconnected.

2. **`FixtureBridge` (Preview / Developer Testing)**
   - Deterministic mock implementation used in Extension Development Host preview and unit tests.
   - Provides a multi-task run (5 tasks, 2 active agents, 1 pending approval, 2 patch artifacts, 10 timeline events).
   - Emits timeline events and updates internal state upon approval decisions or editor actions.
   - Tagged explicitly as `[MOCK / PREVIEW FIXTURE]` across all UI headers.

3. **`BridgeManager`**
   - Singleton manager allowing dynamic switching between bridge instances.
   - Maintains an authoritative `SessionStateProjection` bound to the active bridge.
   - Notifies registered views when the active bridge or connection state changes.

---

## 4. State Projection Model (`SessionStateProjection`)

To prevent UI race conditions and manage high-frequency streaming events from the agent engine, state is derived through a reactive projection model:

1. **Monotonic Sequence Ordering**:
   - Every incoming `TimelineEvent` carries a strictly increasing `sequenceNumber`.
   - Events with `sequenceNumber <= lastProcessedSequence` are rejected as duplicates or stale replays without side effects.

2. **Sequence Gap Detection & Degradation**:
   - If an event arrives where `sequenceNumber > lastProcessedSequence + 1`, a sequence gap is detected.
   - Projection enters `'degraded'` health state (`isDegraded === true`) and fires `onStateDegraded` with `{ expected, received }`.
   - Allows UI to warn users that events may have been lost or out of order.

3. **Bounded Event Buffer (1000 Events FIFO)**:
   - Buffer is strictly capped at `SessionStateProjection.maxEventBufferSize = 1000`.
   - When the 1001st event arrives, the oldest event is pruned (FIFO queue).
   - Guarantees bounded memory consumption during long-running agent execution.

4. **Resource Lifecycle & Disposal**:
   - Implements `vscode.Disposable`.
   - Cleans all internal event listeners, empties timeline buffers, and unbinds from the bridge upon disposal.
