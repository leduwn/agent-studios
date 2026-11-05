# Agent Studios — Milestone M14-UI-00: UI Components Catalog

## Overview

This catalog documents the views, webviews, commands, and virtual document providers comprising the Agent Studios workbench UI.

---

## 1. Native Sidebar Views (`agent-studios-activitybar`)

All six views are registered in `package.json` under `views.agent-studios-activitybar`.

### 1.1 Current Session (`agent-studios.session`)
- **Provider**: `SessionTreeProvider`
- **Purpose**: Displays high-level execution context and session status at a glance.
- **Tree Items**:
  1. `[MOCK / PREVIEW FIXTURE]` (present only when bridge indicates preview mode).
  2. `Status: Connected` / `Status: Disconnected` (with indicator icon).
  3. `Workspace`: Active workspace path.
  4. `Goal`: Agent objective summary (collapsible to display full text).
  5. `Run: <runId>`: Progress summary (e.g., `Running: 3/5 tasks completed`).
  6. `Active Agents: <count>`: Expandable to show individual agent names.
  7. `Provider / Model`: Active LLM provider and model identifier.
  8. `Elapsed Time`: Human-formatted duration (e.g., `6m 0s`).
- **Context Actions**:
  - `agent-studios.showSessionDetails`: Opens the deep session overview webview panel.
  - `agent-studios.refresh`: Forces manual refresh of all sidebar providers.

### 1.2 Tasks (`agent-studios.tasks`)
- **Provider**: `TasksTreeProvider`
- **Purpose**: Displays running, completed, pending, blocked, and failed agent tasks with hierarchy.
- **Visual Design**:
  - Status Icons:
    - Running: `sync~spin`
    - Succeeded: `check`
    - Failed: `error`
    - Blocked: `lock`
    - Pending: `circle-outline`
    - Ready: `circle-filled`
  - Descriptions: Displays assigned agent name and percentage progress: `(Coder Agent) 60%`.
  - Hierarchical nesting: Subtasks nested under parent tasks.
- **Click Behavior**: Clicking a task executes `agent-studios.showTaskGraph`, revealing the interactive DAG with the selected task focused.

### 1.3 Human Approvals (`agent-studios.approvals`)
- **Provider**: `ApprovalsTreeProvider`
- **Purpose**: Human-in-the-loop permission gate for high-risk actions.
- **Visual Design**:
  - Label: Action target or command to execute (e.g., `npm install @vscode/test-electron`).
  - Description: Risk level badge and action type (e.g., `[MEDIUM] exec_command`).
  - Icon: Shield icon with warning color.
  - Tooltip: Detailed breakdown of requested permissions, task ID, and full command.
- **Inline Actions**:
  - Approve (`agent-studios.approveAction`): Checkmark icon on hover. For high-risk items, displays a modal confirmation dialog before submitting.
  - Deny (`agent-studios.denyAction`): Cross/X icon on hover. Rejects the action immediately.

### 1.4 Artifacts & Diffs (`agent-studios.artifacts`)
- **Provider**: `ArtifactsTreeProvider`
- **Purpose**: Lists code changes, patch files, and outputs produced by autonomous agents.
- **Visual Design**:
  - Label: Patch/artifact file name (e.g., `task-m14-workbench-diff.patch`).
  - Description: Version, affected file count, and SHA-256 hash prefix.
  - Icon: `diff` symbol icon.
- **Click Behavior**: Executes `agent-studios.inspectArtifact`. Resolves diff contents and launches VS Code's native `vscode.diff` viewer using the virtual `agent-studios-diff` document provider.

### 1.5 Timeline & Activity (`agent-studios.timeline`)
- **Provider**: `TimelineTreeProvider`
- **Purpose**: Chronological streaming log of agent lifecycle events, ordered newest-first.
- **Visual Design**:
  - Events mapped with specific icons:
    - `session_started`: `play`
    - `task_completed`: `check`
    - `task_failed`: `error`
    - `task_blocked`: `lock`
    - `tool_executing`: `tools`
    - `approval_requested`: `shield`
    - `artifact_created`: `diff`
  - Formatted relative timestamp and assigned agent badge (`[Coder Agent]`).

### 1.6 Model Providers (`agent-studios.providers`)
- **Provider**: `ProvidersTreeProvider`
- **STRICT PRODUCT PRINCIPLE: Strictly Read-Only**:
  - Displays providers (Anthropic, OpenAI, Ollama) and their models.
  - **No add key, no edit, no configuration commands exist**.
  - Provider management is strictly owned by the Rust backend daemon.
  - All tooltips explicitly denote: `(Read-Only: Managed by Rust backend)`.

---

## 2. Interactive Webviews

### 2.1 Interactive DAG Task Graph (`TaskGraphWebview`)
- **View ID**: `agent-studios.taskGraph`
- **Invocation**: Command `agent-studios.showTaskGraph` or clicking any task in the Tasks TreeView.
- **Key Capabilities**:
  - **Topological Layout Engine**: Computes level assignments (roots at level 0, dependent tasks at depth = longest path).
  - **Cycle Detection**: 3-color DFS algorithm detects cycles and gracefully falls back to grid presentation with visual cycle warnings.
  - **SVG Rendering**: Smooth cubic bezier curves with arrows connecting dependent nodes.
  - **Interactivity**: Clicking a node selects it, shows side panel details, and fires an event back to the extension host.
  - **Progressive Disclosure**: Detailed task breakdown and agent logs stay collapsed until a node is inspected.

### 2.2 Session Detail View (`SessionDetailWebview`)
- **View ID**: `agent-studios.sessionDetail`
- **Invocation**: Command `agent-studios.showSessionDetails` or clicking the Session Details button.
- **Key Capabilities**:
  - Metric cards: Status, Active Agents, Tasks Completed / Total, Elapsed Duration.
  - Goal statement and session parameters.
  - Full task inventory table with status chips and agent assignments.
  - Approval history and pending risk warnings.

---

## 3. Virtual Diff Document Provider (`DiffDocumentProvider`)

- **Scheme**: `agent-studios-diff`
- **Role**: Feeds unified diff content directly into VS Code's side-by-side Diff Editor without writing temporary files to the user's workspace filesystem.
- **Utilities**:
  - `extractAffectedPaths`: Parses unified diff headers (`+++ b/<path>`) to find relative file paths.
  - `applyPatch`: Applies unified diff hunks in-memory to compute the proposed state for visual diffing.
