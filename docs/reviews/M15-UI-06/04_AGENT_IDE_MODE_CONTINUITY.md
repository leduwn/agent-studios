# M15 UI-06 — Report 04: Agent Mode ↔ IDE Mode Continuity

## Metadata
- **Milestone**: M15 UI-06: Agent-First Experience, Shared Session Continuity & Real Daemon Integration Acceptance
- **Worktree**: `D:\Projects\agent-studios-active\m15-ui-06`
- **Subsystem**: Commands, Webview Routing & Session Projection
- **Date**: 2026-11-05
- **Status**: **VERIFIED & ACCEPTED**

---

## 1. Architectural Concept: Dual Surfaces, Single Truth

Agent Studios presents two complementary views over the same running engineering session:
1. **Agent Mode**: The conversation-first workspace housed inside `agent-studios.chat`. Focuses on intent formulation, high-level task breakdown, streaming execution progress, and artifact review.
2. **IDE Mode**: The Code-OSS editor workbench containing the file explorer, multi-tab editor groups, terminal panes, git source control, and debugging tools.

Crucially, **both surfaces operate on the exact same underlying session state**:
- `session_id` is identical in both modes.
- `workspacePath` (cwd) is bound to the same root.
- In-flight tasks and DAG execution state in `TaskExecutionStore` and `SessionStateProjection` are shared.
- Native daemon process (`agsd`) runs continuously without interruption during view switching.

---

## 2. Command Routing & Navigation Contracts

### 2.1 Transition to IDE Mode (`agent-studios.openInIde`)
- **Trigger**: Click "Open in IDE Mode" button in Welcome Surface, or title bar icon `$(file-submodule)` in `agent-studios.chat` view header, or command palette.
- **Implementation**:
  ```typescript
  vscode.commands.registerCommand('agent-studios.openInIde', async () => {
    await vscode.commands.executeCommand('workbench.view.explorer');
    if (vscode.window.activeTextEditor) {
      await vscode.commands.executeCommand('workbench.action.focusActiveEditorGroup');
    }
  });
  ```
- **Guarantees**: Switches editor view focus without tearing down webview state, resetting conversation history, or interrupting streaming tasks.

### 2.2 Transition to Agent Mode (`agent-studios.switchToAgentMode`)
- **Trigger**: Command palette, keyboard shortcut, or activity bar click on `Agents` container (`agent-studios-activitybar`).
- **Implementation**:
  ```typescript
  vscode.commands.registerCommand('agent-studios.switchToAgentMode', async () => {
    await vscode.commands.executeCommand('workbench.view.extension.agent-studios-activitybar');
    await vscode.commands.executeCommand('agent-studios.chat.focus');
  });
  ```
- **Guarantees**: Brings chat webview to foreground, restores scroll position, and re-binds message listeners without re-initializing the underlying bridge.

### 2.3 Session Continuity Restore (`agent-studios.continueSession`)
- **Trigger**: Click "Restore Session" in Welcome Surface, or command palette with optional `sessionId`.
- **Implementation**:
  ```typescript
  vscode.commands.registerCommand('agent-studios.continueSession', async (targetSessionId?: string) => {
    if (targetSessionId) {
      const session = await bridgeManager.bridge.switchSession(targetSessionId);
      bridgeManager.stateProjection.switchSession(session);
      return session;
    }
    const sessions = await bridgeManager.bridge.listSessions().catch(() => []);
    if (sessions && sessions.length > 0) {
      const sorted = [...sessions].sort((a, b) => (b.updatedAt || 0) - (a.updatedAt || 0));
      const session = await bridgeManager.bridge.switchSession(sorted[0].id);
      bridgeManager.stateProjection.switchSession(session);
      return session;
    }
    return null;
  });
  ```
- **Guarantees**: Swaps the projected session cleanly in `SessionStateProjection`, notifies listeners, and suppresses redundant task executions.

---

## 3. Transition Invariants Matrix

| Invariant | Description | Verification Check | Result |
|---|---|---|---|
| **PID Invariance** | Switching between Agent Mode and IDE Mode never restarts native `agsd` process. | `assert.strictEqual(daemonProcess.pid, initialPid)` | **PASSED** (PID unchanged) |
| **Session Invariance** | Active `session_id` in `SessionStateProjection` remains identical before and after transition. | `assert.strictEqual(postSessionId, preSessionId)` | **PASSED** (Identity preserved) |
| **Zero Task Duplication** | Switching views never resubmits tasks, restarts runs, or duplicates approvals. | `assert.strictEqual(postTasks.length, initialTasks.length)` | **PASSED** (Task count invariant) |
| **Zero Webview Ghosting** | Closing and reopening chat webview rehydrates latest state from projection without re-sending events. | Webview disposal lifecycle test in `AgentIdeContinuity.test.ts` | **PASSED** (Hydration clean) |
| **Workspace Invariance** | Working directory (`cwd`) stays bound to current repository across view transitions. | Workspace path comparison in state projection | **PASSED** (Path unchanged) |
