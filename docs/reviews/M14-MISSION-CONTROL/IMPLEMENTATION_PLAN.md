# Implementation Plan: M14 Mission Control Workbench
**Milestone:** M14 Mission Control Workbench  
**Status:** Approved Architectural Document  
**Date:** 2026-10-30  
**Target Worktree:** `D:\Projects\agent-studios-mission-control`  
**Target Branch:** `feat/m14-mission-control-workbench`  

---

## 1. Subagent Workstream Partitioning

To maintain strict modularity and prevent merge collisions, implementation responsibilities are divided into clear architectural domains:

| Subagent | Domain | Key Files Owned |
| :--- | :--- | :--- |
| **Agent A** | **Shell, Webview & Commands** | `MissionControlPanel.ts`, `MissionControlHtmlBuilder.ts`, `MissionControlCommands.ts` |
| **Agent B** | **Overview & Attention** | `views/OverviewView.ts`, `components/AttentionQueue.ts` |
| **Agent C** | **Tasks, Kanban & DAG** | `views/TasksView.ts`, `components/DagGraphRenderer.ts`, `components/KanbanBoard.ts` |
| **Agent D** | **Agents, Runs & Timeline** | `views/AgentsView.ts`, `views/RunsView.ts`, `views/TimelineView.ts`, `components/AgentInspector.ts` |
| **Agent E** | **Review, Approvals, Providers** | `views/ReviewView.ts`, `views/ApprovalsView.ts`, `views/ProvidersView.ts`, `views/DiagnosticsView.ts` |
| **Agent F** | **Read Models & Chat Sync** | `models/`, `state/MissionControlProjection.ts`, `bridge/MissionControlBridgeAdapter.ts` |
| **Agent G** | **Accessibility & Styles** | `styles/missionControl.css`, theme variables, responsive media queries |
| **Agent H** | **Host Tests & Packaging** | `test/unit/`, `test/integration/`, `package.json` validation, `@vscode/vsce` packaging |

---

## 2. Phased Delivery Roadmap

### Phase 0: Approved Documentation Baseline (Current)
- Complete all 7 architectural specifications.
- Author documentation-first commit on `feat/m14-mission-control-workbench`.

### Phase 1: Core Read Models & State Projection (Agent F)
- Implement `apps/agent-studios-vscode/src/mission-control/models/types.ts`.
- Implement `MissionControlProjection.ts` mapping `AgentStudiosBridge` data to `MissionControlState`.
- Author fixture datasets covering all 7 lifecycle states, 50+ tasks, 8 agents, and 100+ events.

### Phase 2: WebviewPanel Host Shell & Commands (Agent A)
- Implement `MissionControlPanel.ts` supporting singleton lifecycle (`createOrShow`), disposal, and message handling.
- Implement `MissionControlHtmlBuilder.ts` with strict CSP, nonce generation, and script bundle loading.
- Register `agent-studios.openMissionControl` command and add command palette entry.

### Phase 3: Overview Dashboard & Attention Queue (Agent B)
- Render studio title, active session metrics, fleet health counters, and task lifecycle distribution.
- Implement the Attention Queue surfacing pending approvals, failed tasks, and connection status.

### Phase 4: Agents Fleet View & Agent Inspector (Agent D)
- Implement agent status table (Status, Role, Current Task, Current Run, Runtime, Model, Last Activity).
- Implement slide-out Agent Inspector drawer with tool invocation history and worktree details.

### Phase 5: Task Management: List, Kanban & DAG Graph (Agent C)
- Implement search-filterable Task List.
- Implement Kanban Board with 7 verified lifecycle columns (Pending, Ready, Running, Blocked, Review, Completed, Failed).
- Implement interactive SVG DAG Dependency Graph with pan, zoom, fit-to-view, and node selection.

### Phase 6: Run Inspector & Activity Timeline (Agent D)
- Implement Run Inspector detailing individual execution attempts, tool calls, and terminal evidence.
- Implement Activity Timeline with category filtering, search, monotonic sequence watermarking, and deduplication.

### Phase 7: Review & Native Diff Integration (Agent E)
- Implement modified file list with change status badges (Added, Modified, Deleted).
- Implement "Open Diff" button routing `vscode.diff` requests to the extension host.
- Render test run outcomes and git metadata.

### Phase 8: Approval Center, Providers & Diagnostics (Agent E)
- Implement Approval Center with risk ratings and fail-closed disabled buttons.
- Implement Model Providers view with scrubbed secrets.
- Implement Diagnostics view with daemon connection telemetry and watermark tracking.

### Phase 9: Chat ↔ Mission Control Synchronization (Agent F & A)
- Synchronize session selection between Chat sidebar and Mission Control.
- Ensure switching views never cancels active tasks.

### Phase 10: Styling, Accessibility & Host Testing (Agent G & H)
- Bind all UI elements to VS Code CSS variables.
- Validate WCAG 2.1 AA keyboard navigation and screen-reader ARIA tags.
- Run unit tests, extension host integration tests, and `@vscode/vsce package --no-dependencies`.
- Author comprehensive review report under `docs/reviews/M14-MISSION-CONTROL/RUN-<timestamp>/`.
- Push branch and open stacked Draft PR targeting `feat/m14-vscode-extension-host-hardening`.
