# Agent Studios — Milestone M14-UI-00: Test Execution Guide

## Overview

Agent Studios M14-UI-00 implements a pure Node.js test runner using an in-memory VS Code mock (`vscodeMock.ts`). This architecture enables 100% of unit tests to execute in milliseconds without launching the Electron/VS Code desktop binary in CI.

---

## 1. Test Suite Summary

- **Total Unit Tests**: 77 passing
- **Execution Time**: ~120ms
- **Coverage Areas**:
  1. `ApprovalCommands.test.ts`: Approval/denial commands, high-risk modal confirmation dialogs, fail-closed disconnected handling.
  2. `ArtifactCommands.test.ts`: Inspecting artifacts via diff editor, quickpick fallbacks, missing artifact handling.
  3. `Bridge.test.ts`: `DisconnectedBridge` fail-closed behavior, `FixtureBridge` deterministic preview data and mutation events.
  4. `DiffDocumentProvider.test.ts`: In-memory unified patch application (`applyPatch`), diff path extraction, `agent-studios-diff` virtual document provider.
  5. `SessionStateProjection.test.ts`: 1000-event bounded FIFO buffer, monotonic sequence ordering, duplicate rejection, sequence gap degradation, resource cleanup on disposal.
  6. `TaskGraphLayout.test.ts`: Level 0 root node placement, topological depth for dependent nodes, 3-color DFS cycle detection, acyclic validation.
  7. `views.test.ts`: `SessionTreeProvider`, `TasksTreeProvider`, `ApprovalsTreeProvider`, `ArtifactsTreeProvider`, `TimelineTreeProvider`, and read-only `ProvidersTreeProvider`.
  8. `webviews.test.ts`: HTML sanitization / XSS prevention in `markdownRenderer`, DAG webview panel lifecycle, and session detail webview panel.

---

## 2. Command Line Execution

From the extension directory (`apps/agent-studios-vscode`):

```bash
# 1. Compile TypeScript
npm run build

# 2. Run pure Node.js test suite
npm test

# 3. Run linting
npm run lint

# 4. Run typechecking
npm run typecheck
```

Output:
```text
> agent-studios-vscode@0.1.0 test
> node out/test/unit/runner.js

  ApprovalCommands Tests (7 tests)
  ArtifactCommands Tests (5 tests)
  Bridge Pure Unit Tests (6 tests)
  DiffDocumentProvider Tests (8 tests)
  SessionStateProjection Pure Unit Tests (8 tests)
  TaskGraphLayout Pure Unit Tests (4 tests)
  TreeView Providers Tests (14 tests)
  Webviews & Graph Layout Tests (25 tests)

  77 passing (144ms)
```

---

## 3. Extension Development Host (Manual Verification Guide)

To manually test the UI in the real Code-OSS / VS Code Extension Development Host:

### 3.1 Launching the Extension Host
1. Open `apps/agent-studios-vscode` in VS Code.
2. Press `F5` (or select **Run and Debug** -> **Run Extension**).
3. A new Extension Development Host window will open with Agent Studios activated.

### 3.2 Verifying Views in the Activity Bar
1. In the left Activity Bar, click on the **Agent Studios** icon (`$(hubot)`).
2. Verify all six TreeViews render in the sidebar:
   - **Current Session**: Shows `[MOCK / PREVIEW FIXTURE]`, connection status, active run, agent count (2), and model.
   - **Tasks**: Shows 5 tasks with status icons (`sync~spin` for running, `check` for completed, `circle-outline` for pending).
   - **Approvals**: Shows `npm install @vscode/test-electron` marked with `[MEDIUM] exec_command`.
   - **Artifacts**: Shows two `.patch` files.
   - **Timeline**: Shows chronological activity stream (10 events, newest first).
   - **Providers (Read-Only)**: Shows Anthropic, OpenAI, and Ollama with models.

### 3.3 Verifying Human Approvals
1. In the **Approvals** TreeView, hover over the pending item:
   - Click the checkmark icon (**Approve Action**).
   - Verify the approval item disappears and moves to approved status.
   - Verify the timeline updates with an `approval_decided` entry.
2. In the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`), type `Agent Studios: Focus Approvals View` and press Enter.

### 3.4 Verifying Diff Artifact Inspection
1. In the **Artifacts** TreeView, click `task-m14-workbench-diff.patch`.
2. VS Code's native side-by-side Diff Editor opens showing changes to `package.json`.
3. Verify no temporary files were written to disk (URI uses `agent-studios-diff://`).

### 3.5 Verifying Interactive DAG Webview
1. Click on any task in the **Tasks** TreeView or run `Agent Studios: Show Interactive Task Graph`.
2. A webview panel opens in the editor area titled `Agent Studios: Task Graph`.
3. Verify nodes are arranged from left to right (levels 0 to 4) connected by smooth bezier arrows.
4. Click on a node: verify the detail sidebar reveals task metadata, status, agent assignment, and progress.

### 3.6 Verifying Session Detail Webview
1. Run `Agent Studios: Show Deep Session Details` from the Command Palette.
2. A webview panel opens titled `Agent Studios: Session Details`.
3. Verify metrics cards (Status: Running, 2 Active Agents, 3/5 Tasks Completed, Elapsed 6m 0s) and task tables render cleanly with responsive layout.
