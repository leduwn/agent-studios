# Component Architecture: M14 Mission Control Workbench

**Milestone:** M14 Mission Control Workbench  
**Run ID:** RUN-20261030-M14-MISSION-CONTROL  
**Date:** 2026-10-30  

---

## 1. Architectural Topology

Mission Control is implemented as an editor `WebviewPanel` registered under the view type `agent-studios.missionControl`. It operates as a singleton within the VS Code Extension Host:

```
┌────────────────────────────────────────────────────────────────────────┐
│                        VS Code Extension Host                          │
│                                                                        │
│  ┌────────────────────────┐         ┌───────────────────────────────┐  │
│  │   BridgeManager        │         │   SessionStateProjection      │  │
│  │   (Real IPC / Fixture) │         │   (Timeline & Session Events) │  │
│  └───────────┬────────────┘         └───────────────┬───────────────┘  │
│              │                                      │                  │
│              └───────────────────┬──────────────────┘                  │
│                                  │                                     │
│                                  ▼                                     │
│                  ┌───────────────────────────────┐                     │
│                  │   MissionControlProjection    │                     │
│                  │   (Authoritative State Mgt)   │                     │
│                  └───────────────┬───────────────┘                     │
│                                  │ onDidChangeState                    │
│                                  ▼                                     │
│                  ┌───────────────────────────────┐                     │
│                  │   MissionControlPanel         │                     │
│                  │   (WebviewPanel Singleton)    │                     │
│                  └───────────────┬───────────────┘                     │
│                                  │ postMessage / HTML Render           │
└──────────────────────────────────┼─────────────────────────────────────┘
                                   │
                                   ▼ Webview Boundary (Strict CSP)
┌────────────────────────────────────────────────────────────────────────┐
│                   Mission Control HTML Shell (Webview)                 │
│                                                                        │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  Top Navigation Bar (9 Tab Selector + Status Badges + Actions)   │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                        │
│  ┌──────────────┬──────────────┬──────────────┬─────────────────────┐  │
│  │ OverviewView │  AgentsView  │  TasksView   │      RunsView       │  │
│  ├──────────────┼──────────────┼──────────────┼─────────────────────┤  │
│  │ TimelineView │  ReviewView  │ApprovalsView │Providers/Diagnostics│  │
│  └──────────────┴──────────────┴──────────────┴─────────────────────┘  │
│                                                                        │
│  ┌─────────────────────────┐             ┌──────────────────────────┐  │
│  │     AttentionQueue      │             │    AgentInspector Drawer │  │
│  ├─────────────────────────┤             ├──────────────────────────┤  │
│  │       KanbanBoard       │             │    DagGraphRenderer(SVG) │  │
│  └─────────────────────────┘             └──────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Shell & Lifecycle Management (`MissionControlPanel.ts`)

### 2.1 WebviewPanel Singleton Pattern
- `createOrShow(extensionUri, projection, diffReviewManager)` creates a new editor panel column or reveals the existing panel if already open.
- Sets `retainContextWhenHidden: true` to prevent state loss or expensive re-renders when developers toggle between editor tabs.
- Registers `localResourceRoots: [extensionUri]` to ensure local assets are bounded.
- Implements `vscode.Disposable`, cleaning up subscriptions, state emitters, and resetting `currentPanel = undefined` upon disposal.

### 2.2 Content Security Policy (CSP)
The HTML shell enforces strict CSP headers with cryptographic nonces:
```html
<meta http-equiv="Content-Security-Policy" content="
  default-src 'none';
  style-src ${webview.cspSource} 'unsafe-inline';
  script-src 'nonce-${nonce}';
  font-src ${webview.cspSource};
  img-src ${webview.cspSource} data: https:;
" />
```

### 2.3 Bi-directional Message Routing
Incoming messages from the webview client script are validated and dispatched:
- `switchTab`: Updates projection active tab and pushes updated view state.
- `selectAgent`: Updates selected agent in projection, opening the Inspector drawer.
- `selectTask`: Updates selected task in projection, highlighting in Kanban/DAG.
- `selectRun`: Updates selected run ID for telemetry and terminal log inspection.
- `openDiff`: Invokes `DiffReviewManager.openDiff(artifactId)` using native `vscode.diff`.
- `openChat`: Focuses the Chat sidebar view (`agent-studios.chat.focus`) and switches session.
- `refresh`: Requests fresh data query across the bridge.
- `approveAction` / `denyAction`: Emits a security warning, enforcing fail-closed protection in this read-only slice.

---

## 3. Unidirectional State Projection (`MissionControlProjection.ts`)

The projection consolidates data streams into a single immutable `MissionControlState` object:
1. **Preview Mode Handling:** When `BridgeManager.isPreviewMode` is active, populates fixture data via `createPreviewMissionControlState()` with rich agents, tasks, runs, artifacts, and timeline events.
2. **Disconnected Mode Handling:** When disconnected, returns safe, empty data structures with an active attention warning guiding the user to connect or toggle preview mode.
3. **Task Lifecycle Aggregation:** Maps arbitrary bridge task statuses into the 7 verified task lifecycle states (`pending`, `ready`, `running`, `blocked`, `review`, `succeeded`, `failed`, `cancelled`).
4. **Attention Queue Synthesis:** Dynamically aggregates high-priority conditions (pending approvals, failed tasks) into a prioritized attention queue.
5. **Secret Scrubbing:** Strips API keys, auth tokens, and credential secrets before serializing provider data into state objects.
