# VS Code Extension Host Integration Verification Report

**Milestone:** M14-UI-02  
**Run ID:** RUN-20261030-M14-UI-02  
**Date:** 2026-10-30  
**Test Framework:** `@vscode/test-electron` + Mocha 10  
**Target VS Code Binary:** Version 1.144.0 (win32-x64)

---

## 1. Extension Host Test Architecture

Milestone M14-UI-02 upgrades verification from in-memory Node mocks to the official VS Code Extension Development Host.

```
┌────────────────────────────────────────────────────────────────────────┐
│                        VS Code Process Hierarchy                       │
├────────────────────────────────┬───────────────────────────────────────┤
│ Runner Process (Node.js)       │ Extension Development Host (Electron) │
│ - out/test/integration/runTest │ - Code.exe (1.144.0)                  │
│ - Unsets ELECTRON_RUN_AS_NODE  │ - Dedicated --user-data-dir           │
│ - Launches download/cache      │ - Dedicated --extensions-dir          │
│ - Waits for exit code 0        │ - Loads ./package.json in dev mode    │
└────────────────────────────────┴───────────────────┬───────────────────┘
                                                     │
                                                     ▼
                                        ┌─────────────────────────┐
                                        │ Mocha Test Suite Runner │
                                        │ - activation.test.ts    │
                                        │ - navigation.test.ts    │
                                        │ - diffProvider.test.ts  │
                                        │ - securityHost.test.ts  │
                                        └─────────────────────────┘
```

### Critical Environment Isolation:
1. **Unsetting `ELECTRON_RUN_AS_NODE`:**  
   When spawned from certain Node-based harnesses, `process.env.ELECTRON_RUN_AS_NODE=1` is propagated into child processes, forcing `Code.exe` to run in headless Node CLI mode where Electron UI and extension host arguments (`--disable-extensions`) fail immediately with exit code 9. `runTest.ts` explicitly deletes `process.env.ELECTRON_RUN_AS_NODE` prior to invoking `runTests()`.
2. **Ephemeral Sandbox Paths:**  
   - User Data: `.vscode-test/user-data/`
   - Extension Directory: `.vscode-test/extensions/`
   This isolates test runs from local user settings and prevents state pollution across executions.

---

## 2. Test Execution Matrix & Results

All 14 real extension host integration tests executed and passed cleanly:

```
  Extension Host: Activation & Commands Integration Tests
    √ verifies extension is present in VS Code extensions catalog
    √ activates extension cleanly in real extension host (166ms)
    √ registers all 14 workbench and editor commands in vscode.commands
    √ verifies default extension settings schema in vscode.workspace (638ms)
    √ executes agent-studios.toggleFixturePreview command and toggles preview mode (775ms)

  Extension Host: Views & Chat Navigation Integration Tests
    √ executes agent-studios.openAgents without error (203ms)
    √ executes agent-studios.showApprovals view navigation (54ms)
    √ executes agent-studios.refreshViews command across all TreeViews
    √ executes agent-studios.disconnectDaemon cleanly returning to fail-closed state

  Extension Host: Virtual Diff Document Provider Integration Tests
    √ opens and reads virtual document from agent-studios-diff URI scheme (60ms)
    √ triggers native vscode.diff command with virtual URI without workspace disk mutation (452ms)

  Extension Host: Security & Fail-Closed Guard Integration Tests
    √ blocks editor action safely when no active editor is open (281ms)
    √ blocks refactor action safely when selection is empty
    √ blocks test generation safely when disconnected

  14 passing (4s)
```

---

## 3. Verified Extension Subsystems

### A. Command Registry
Verified that all 14 commands declared in `package.json` are registered in the extension host's internal command table:
1. `agent-studios.openAgents`
2. `agent-studios.refreshViews`
3. `agent-studios.connectDaemon`
4. `agent-studios.disconnectDaemon`
5. `agent-studios.toggleFixturePreview`
6. `agent-studios.showTaskGraph`
7. `agent-studios.showSessionDetail`
8. `agent-studios.inspectArtifact`
9. `agent-studios.approveAction`
10. `agent-studios.denyAction`
11. `agent-studios.showApprovals`
12. `agent-studios.explainCode`
13. `agent-studios.refactorCode`
14. `agent-studios.generateTests`

### B. View Container & Views
- `agents` container in Activity Bar resolves.
- TreeViews registered: `agent-studios.sessionsView`, `agent-studios.approvalsView`, `agent-studios.agentsView`, `agent-studios.artifactsView`, `agent-studios.checkpointsView`, `agent-studios.providersView`.
- WebviewView registered: `agent-studios.chat`.

### C. Virtual Text Document Provider (`agent-studios-diff`)
- Validated `vscode.workspace.registerTextDocumentContentProvider` for the `agent-studios-diff` scheme.
- Successfully opened virtual URI `agent-studios-diff://artifact/artifact-diff-001/diff`.
- Executed `vscode.diff` between memory URI and working tree URI without any file writes to disk.

### D. Settings Schema
- Validated `agentStudios.daemonEndpoint` default string (`ipc://agent-studios.sock`).
- Validated `agentStudios.previewMode` boolean flag and runtime reconfiguration.
