# Agent Studios — Mission Control Workbench Product Specification
**Milestone:** M14 Mission Control Workbench  
**Status:** Approved Architectural Document  
**Date:** 2026-10-30  
**Target:** VS Code / Code-OSS Extension Host (`apps/agent-studios-vscode`)  

---

## 1. Executive Summary & Purpose

Agent Studios Mission Control is a dedicated observability, monitoring, and review workbench rendered as a VS Code `WebviewPanel` inside the main editor area. 

### Core Product Philosophy
- **Complement, Not Replace:** Mission Control works in tandem with the primary Chat & Assistant sidebar (`agent-studios.chat`). While Chat provides conversational interaction, prompt composition, and inline streaming, Mission Control provides an expansive multi-column view for orchestrating concurrent coding agents, tracking task DAGs, inspecting execution runs, auditing changed files, and overseeing system health.
- **Observability First:** Truthful reflection of runtime state. Every metric, status pill, and timeline entry is tied to an authoritative data model. No fabricated progress percentages or synthetic task states.
- **Fail-Closed Operations:** Destructive mutations, unvetted approvals, and raw shell executions are disabled or strictly guarded until end-to-end sandbox security gates pass.
- **Native VS Code Integration:** Uses standard VS Code CSS tokens, native diff editor launches, command palette registrations, and keyboard navigation.

---

## 2. Information Architecture & Navigation

Mission Control is launched via the command:
`Agent Studios: Open Mission Control` (`agent-studios.openMissionControl`)
or deep-linked via an action button in the Chat sidebar header.

The workbench operates with a persistent top navigation bar hosting tabs:
1. **Overview:** High-level dashboard, fleet health, lifecycle summaries, and Attention Queue.
2. **Agents:** Fleet list, status grid, active runs, and deep Agent Inspector drawer.
3. **Tasks:** Three complementary projections: Searchable List, Kanban Board, and interactive DAG Graph.
4. **Runs:** Execution run records, tool invocations, workspace/worktree assignments, and terminal outcome evidence.
5. **Timeline:** Chronological event stream, filtering by agent/task/category, replay watermark tracking.
6. **Review & Artifacts:** Changed files, diff viewer trigger (`vscode.diff`), artifact inspector, and test results.
7. **Approvals:** Pending permission and tool requests with risk levels (read-only / guarded).
8. **Providers:** Configured provider instances, active model parameters, and token/latency telemetry.
9. **Diagnostics:** IPC daemon connectivity, watermark tracking, projection freshness, and error logs.

---

## 3. Detailed View Specifications

### 3.1 Overview Dashboard
- **Active Studio / Session:** Title, workspace path, creation timestamp, and run status.
- **Connection Badge:** Explicit status pills (`Connected`, `Connecting`, `Disconnected`, `Degraded`, `Preview`).
- **Fleet Summary:** Number of active, idle, blocked, and failed agents.
- **Task Lifecycle Distribution:** Metric chips for Pending, Ready, Running, Succeeded, Failed, and Cancelled.
- **Attention Queue:** Elevated section highlighting pending approvals, failed tasks, degraded providers, or disconnection alerts.
- **Recent Activity Ticker:** The 5 most recent timeline events with clickable deep links to the respective task or run.

### 3.2 Agents Fleet View & Agent Inspector
- **Fleet Grid / Table:**
  - `agent_id`: Stable unique identifier.
  - `name`: Human-readable persona or role (e.g., "Architect", "Rust Engineer", "Security Reviewer").
  - `status`: `idle` | `busy` | `blocked` | `failed`.
  - `assigned_task`: Linked task title and ID.
  - `active_run`: Current execution run ID.
  - `runtime`: e.g., `codex-v2`, `claude-code-cli`, `opencode`.
  - `provider_model`: e.g., `anthropic:claude-3-7-sonnet`.
  - `last_activity`: Relative timestamp with millisecond hover tooltip.
- **Agent Inspector Drawer:**
  - Slide-out panel or split view displaying assigned tasks history, tool invocation stream, associated git worktree path, terminal exit codes, and generated artifacts.

### 3.3 Task Management
- **Searchable List:** Dense table with instant substring filtering on title, description, and ID.
- **Kanban Board:**
  - Columns strictly derived from verified backend states:
    - *Pending*: Not yet scheduled.
    - *Ready*: Dependencies satisfied, awaiting execution slot.
    - *Running*: Actively being processed by an agent.
    - *Blocked*: Waiting on approval, dependency, or external resource.
    - *Review*: Derived state indicating task finished code generation and awaits user verification.
    - *Completed*: Succeeded and verified.
    - *Failed / Cancelled*: Errored out or aborted.
  - Drag-and-drop between columns is disabled unless an authorized backend state transition command exists.
- **DAG Graph Viewer:**
  - SVG/Canvas rendering of dependency nodes and directed edges.
  - Pan, zoom (wheel and control buttons), fit-to-view.
  - Interactive node selection opening task details in side panel.
  - Cycle detection and orphan node indicators.
  - Semantic theme coloring: Blue (running), Green (succeeded), Red (failed), Yellow (blocked/pending).

### 3.4 Run Inspector
- Details of individual execution attempts (`RunRecord`).
- Captures: Run ID, parent task, runtime adapter, start/end timestamps, workspace directory, assigned git worktree, tool call transcript, stdout/stderr snippets, and outcome classification (Success, Interrupted, Disconnected, Failed).

### 3.5 Activity Timeline
- Chronological stream of all `TimelineEvent` records.
- Filtering by event type (`tool_call`, `task_lifecycle`, `approval`, `file_change`, `diagnostic`).
- Real-time handling of monotonic sequence watermarks and replay deduplication.

### 3.6 Review & Artifacts
- List of modified, added, and deleted files across agent runs.
- Action button "Open Diff" executing native VS Code command `vscode.diff(originalUri, modifiedUri)`.
- Test execution summary: Passed, failed, skipped tests from agent runs.
- Git metadata: Active branch, worktree directory, and commit reference.

### 3.7 Approval Center & Providers
- Read-only presentation of approval requests with risk ratings (Low, Medium, High).
- Visibly disabled action buttons if connected in read-only slice.
- Model provider overview listing configured backends without exposing private keys.

---

## 4. State & Synchronization Guarantees

1. **Bi-Directional Synchronization:** Selecting a session or task in the Chat view immediately updates Mission Control, and selecting an item in Mission Control focuses it in the sidebar.
2. **Stable Identity:** All objects are tracked via immutable UUIDs (`studio_id`, `task_id`, `agent_id`, `run_id`, `sequence`).
3. **No State Loss:** Switching workbench tabs or collapsing panels retains scroll position, active filters, and inspector selections.
