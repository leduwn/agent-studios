# M15 UI-06 — Report 09: M17 Standalone Desktop Shell Gap Analysis & Boundaries

## Metadata
- **Milestone**: M15 UI-06: Agent-First Experience, Shared Session Continuity & Real Daemon Integration Acceptance
- **Worktree**: `D:\Projects\agent-studios-active\m15-ui-06`
- **Future Milestone**: M17 (Standalone Code-OSS Desktop Workbench & Shell)
- **Date**: 2026-11-05
- **Status**: **HONESTLY DOCUMENTED & DELINEATED**

---

## 1. Delineation of Scope: Extension Host vs Standalone Desktop

A core engineering principle of Agent Studios is **strict honesty regarding implementation altitude**. We explicitly do not claim full M17 Standalone Desktop Shell completion today.

Instead, M15 UI-06 implements the **complete extension-host vertical slice**:
- Proven bidirectional mode switching (`openInIde` ↔ `switchToAgentMode`).
- Complete shared session continuity over `SessionStateProjection` and native `agsd` SQLite WAL.
- All UX state indicators, welcome surface, and progressive disclosure patterns.

---

## 2. Capabilities Completed in M15 UI-06 (Today)

| Capability | Implementation Mechanism | Production Ready? |
|---|---|---|
| **Agent Mode Surface** | `agent-studios.chat` webview view inside unified `Agents` activity bar container | **YES** |
| **IDE Mode Surface** | Native Code-OSS editor workbench, explorer view, and active editor group | **YES** |
| **Mode Switching Commands** | `agent-studios.openInIde`, `agent-studios.switchToAgentMode`, `agent-studios.continueSession` | **YES** |
| **Shared Session Identity** | Synchronized across views via `SessionStateProjection` and `TaskExecutionStore` | **YES** |
| **Process Non-Disruption** | Daemon PID strictly invariant; zero daemon restarts across view transitions | **YES** |
| **Zero Task Mutation** | Task and approval counts invariant across view switches | **YES** |
| **Authentic Native IPC** | 2-phase HMAC handshake over Windows Named Pipes & POSIX sockets to `agsd.exe` | **YES** |

---

## 3. Residual Gaps Scheduled for Milestone M17 (Future Work)

The following architectural items require custom Code-OSS Electron application modifications and are explicitly scheduled for Milestone M17:

### 3.1 Custom Window Header & Root Mode Tabs
- **Current Behavior**: Mode switching is driven by commands, activity bar clicks, or webview buttons.
- **M17 Target**: Custom Electron title bar featuring top-level centered segmented controls: `[ Agent Mode | IDE Mode | Mission Control ]` directly embedded in the window frame.

### 3.2 Standalone Application Packaging
- **Current Behavior**: Extension packages as a standard `.vsix` file (`agent-studios-vscode-0.1.0.vsix`) installed into Code-OSS / VS Code.
- **M17 Target**: Standalone native desktop application bundles (Windows `.exe`/MSI, macOS `.dmg`, Linux `.AppImage`/`.deb`) built with custom Electron binary embedding `agsd`.

### 3.3 Deep OS URL Protocol Registration
- **Current Behavior**: Session restoration is initiated from the welcome surface card or command palette.
- **M17 Target**: OS-level protocol handler registration (`agentstudios://session/{id}`) allowing CLI, browser, or terminal triggers to open the application directly into a targeted session.

### 3.4 Custom Frameless Workspace Picker
- **Current Behavior**: Relies on VS Code's standard `vscode.workspace.updateWorkspaceFolders` or native folder dialogs.
- **M17 Target**: Custom Agent Studios workspace onboarding surface featuring recent worktree picker, git branch selector, and daemon auto-spawn.
