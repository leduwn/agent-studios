# Agent Studios — M15 UI-06 Release Hardening
## Document 02: Extension Host Acceptance & Verification Hierarchy

**Milestone:** M15 UI-06 (Agent-First Experience, Shared Session Continuity & Real Daemon Integration)  
**Branch:** `feat/m15-ui-06-agent-ide-continuity`  
**Target Base:** `integration/m15-unified-codebase`  
**Status:** VERIFIED & AUDITED  
**Date:** 2026-11-06  

---

### 1. Multi-Tier Acceptance Hierarchy

To eliminate ambiguity between unit command mocks and desktop host verification, Agent Studios enforces a four-tier testing taxonomy:

```
┌─────────────────────────────────────────────────────────────────┐
│ Tier 4: Standalone Code-OSS Desktop Shell (M17 Target)          │
│ - Full native Electron wrapper, desktop mode switches, menus    │
├─────────────────────────────────────────────────────────────────┤
│ Tier 3: Real VS Code Extension Development Host (@vscode/test)  │
│ - Real vscode process, real command registry, real IPC activation│
├─────────────────────────────────────────────────────────────────┤
│ Tier 2: Real Daemon Process & IPC Verification (agsd E2E)       │
│ - Authentic agsd.exe child process, HMAC-SHA256, SQLite WAL     │
├─────────────────────────────────────────────────────────────────┤
│ Tier 1: Fast Mocked Unit Tests (vscodeMock & Mocha)             │
│ - In-memory command spies, simulated dispatches, view state     │
└─────────────────────────────────────────────────────────────────┘
```

#### Acceptance Truth Matrix

| Validation Layer | Test Target | Execution Mechanism | Scope & Guarantees |
| :--- | :--- | :--- | :--- |
| **Tier 1: Unit Mocks** | `AgentIdeContinuity.test.ts` | `vscodeMock.ts`, in-memory spies | Proves TypeScript logic, arguments, state transitions. **Does not prove desktop host integration.** |
| **Tier 2: Real Daemon** | `RealAgentIdeContinuityE2E.test.ts` | Compiled `agsd.exe`, Named Pipe / UDS | Proves native daemon lifecycle, HMAC handshake, SQLite WAL state survival, role security. |
| **Tier 3: Extension Host** | `activation.test.ts`, `modeContinuity.test.ts` | `@vscode/test-electron` v1.145.0 | Proves real VS Code command registry, extension activation, configuration schemas, unmocked dispatches. |
| **Tier 4: Standalone Shell** | Standalone Code-OSS App | Standalone Electron workbench | Custom top-level window controls, native titlebar menus (Scheduled for Milestone M17). |

---

### 2. Real Extension Development Host Test Evidence

Tests were executed using `@vscode/test-electron` against VS Code `1.145.0`:

```bash
npm --prefix apps/agent-studios-vscode run test:integration
```

**Results:**
- **Passing Tests:** 25 passing (13s duration)
- **Failing Tests:** 0
- **Process Status:** Extension host exited cleanly with code `0`.

#### Verified Integration Test Suites

1. **Extension Host: Activation & Commands Integration Tests**
   - Extension catalog presence (`agent-studios.agent-studios-vscode`).
   - Clean activation via `vscode.extensions.getExtension().activate()`.
   - Command registration in real `vscode.commands.getCommands(true)`:
     - `agent-studios.openInIde`
     - `agent-studios.switchToAgentMode`
     - `agent-studios.continueSession`
     - `agent-studios.openMissionControl`
     - All 18 workbench and editor commands.
   - Settings schema registration in `vscode.workspace.getConfiguration('agentStudios')`.
   - Toggle preview command execution and configuration mutation.

2. **Extension Host: Mode Continuity & Navigation Integration Tests (M15 UI-06)**
   - `openInIde` registered and executable without errors.
   - `switchToAgentMode` registered and executable without errors.
   - `continueSession` executed without crashing, returning active/fixture session or null safely.

3. **Extension Host: Views & Chat Navigation Integration Tests**
   - `agent-studios.openAgents` execution.
   - `agent-studios.showApprovals` navigation.
   - `agent-studios.refreshViews` multi-tree view refresh.
   - `agent-studios.disconnectDaemon` returning to fail-closed state.

4. **Extension Host: Virtual Diff Document Provider Integration Tests**
   - Read virtual document via `agent-studios-diff` URI scheme.
   - Native `vscode.diff` invocation without disk mutation.
   - `agent-studios.openArtifactReview` inspection.

5. **Extension Host: Mission Control Integration Tests**
   - Singleton panel instantiation and reveal logic.
   - Panel reset on disposal.

6. **Extension Host: Security & Fail-Closed Guard Integration Tests**
   - Rejection of editor actions when no active editor exists.
   - Rejection of refactor actions when selection is empty.
   - Guarded test generation when disconnected.

---

### 3. Environment Assessment & Transparent Reporting

- **Console / CI Environment:** The test harness ran headless in a Windows 11 console. All Electron background services, extension hosts, and command dispatches functioned normally under `@vscode/test-electron`.
- **Graphical Shell Limitations:** While `@vscode/test-electron` tests run the authentic VS Code Extension Host process, they run headless or window-hidden. Standalone application UX (e.g. native OS application menus, standalone desktop shell framing) cannot be exercised by extension-level tests.
- **Reporting Invariant:** We explicitly distinguish extension host command acceptance from standalone Code-OSS desktop mode switching. The latter is documented as a planned deliverable for Milestone M17.
