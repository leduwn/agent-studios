# Agent Studios — Agent Mode & IDE Mode Experience Specification

> **Status**: Authoritative UX & Architecture Specification  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Reference**: Implements `MASTER_VISION.md` §3, `AGENT_MODE_AND_IDE_MODE.md`, and `CODE_OSS_INTEGRATION.md`

---

## 1. Architectural Philosophy: Dual Surfaces, Single Truth

`[USER_DIRECT_REQUIREMENT]` & `[EXISTING_CANONICAL_DECISION]`
Agent Studios rejects the dichotomy between "simple AI chat tools" and "heavy traditional IDEs". Modern software engineering demands both:
1. High-level goal formulation, delegation, and autonomous multi-agent orchestration.
2. In-depth code navigation, syntax highlighting, debugging, and precise manual editing.

Instead of running two independent applications, Agent Studios provides **two complementary presentation surfaces over one shared backend runtime session**:

```text
                        ┌────────────────────────┐
                        │      AGENT STUDIOS     │
                        └───────────┬────────────┘
                                    │
                  ┌─────────────────┴─────────────────┐
                  ▼                                   ▼
        ┌───────────────────┐               ┌───────────────────┐
        │    AGENT MODE     │               │     IDE MODE      │
        │  Agent-First UX   │ ◄───────────► │ Code-OSS Workbench│
        │  Conversation     │  Zero-Loss    │ Editor & Terminal │
        └─────────┬─────────┘  Mode Switch  └─────────┬─────────┘
                  │                                   │
                  └─────────────────┬─────────────────┘
                                    │
                                    ▼
                        ┌────────────────────────┐
                        │     SHARED SESSION     │
                        │ (session_id, cwd, run, │
                        │  tasks, agents, tools, │
                        │  approvals, worktrees) │
                        └───────────┬────────────┘
                                    │
                                    ▼
                        ┌────────────────────────┐
                        │   NATIVE AGSD DAEMON   │
                        │ (Control Plane, Git,   │
                        │  Runtimes, Providers)  │
                        └────────────────────────┘
```

---

## 2. Agent Mode Specification

### 2.1 Design Objectives
- **Default Startup Experience**: Opens immediately upon launching Agent Studios.
- **Progressive Disclosure**: Surfaces essential delegation controls first; hides complex task graphs, worktree tables, and raw logs until execution begins.
- **Useful, Native, Quiet**: Monochrome palette, crisp typography, Codicons, zero sci-fi neon animations.

### 2.2 Core UI Components
1. **Natural Language Goal Composer**:
   - Primary input box (`Ask Agent Studios...`).
   - Workspace selector dropdown (`source_root`, `source_cwd`).
   - Provider & Model selector (`ModelRef { provider_instance_id, model_id }`).
2. **Real-time Task Progress Checklist**:
   - Collapsible list of active, blocked, ready, and completed tasks.
   - Progress counter (`X / Y tasks completed`).
   - Direct link to `[ View Plan ]` and `[ Open in IDE ]`.
3. **Interactive Approval Inbox**:
   - Prominently displays actions awaiting operator confirmation.
   - Color-coded risk badges: Low (gray), Medium (blue), High (amber), Critical (red).
   - Side-by-side or unified diff preview of proposed modifications.
   - Explicit `[ Approve ]` and `[ Deny ]` buttons with optional feedback note.
4. **Recent Sessions Drawer**:
   - Historical list of local sessions stored in SQLite WAL database.
   - Quick restore of session state, goals, and artifact history.

---

## 3. IDE Mode Specification (Code-OSS Integration)

### 3.1 Design Objectives
- **Full Professional Workbench**: Powered by upstream Code-OSS, providing 100% compatibility with standard VS Code developer workflows.
- **Single Activity Bar Container**: Avoids extension button sprawl by contributing exactly **one** icon (`Agents`) to the primary Activity Bar.
- **Deep Control Plane Binding**: Built-in extension (`apps/agent-studios-vscode`) connects to native `agsd` via local Named Pipe / Unix Domain Socket.

### 3.2 Native IDE Capabilities Preserved
- Multi-tab Monaco editor (syntax highlighting, semantic tokens, code folding, multi-cursor).
- Full Explorer tree with file operations and drag-and-drop.
- Ripgrep-powered Search and Replace.
- Native Git staging, hunk staging, and visual side-by-side diffing.
- Integrated xterm.js terminal (PowerShell, Bash, Command Prompt).
- DAP debugger (breakpoints, conditional breakpoints, variable inspect, call stack).
- LSP client (diagnostics, code actions, autocomplete, hover documentation).

### 3.3 Dedicated Agent Studios Surfaces in IDE Mode
- **`Agents` Sidebar Container**:
  - `agent-studios.chat`: Conversational webview for prompt composition and streaming chat.
  - `agent-studios.session`: Active session metadata, provider instance, and working directory.
  - `agent-studios.tasks`: Interactive tree view of current run tasks and dependencies.
  - `agent-studios.approvals`: Approval queue with one-click approve/deny actions.
  - `agent-studios.artifacts`: Content-addressed patch artifacts and generated files.
  - `agent-studios.timeline`: Monotonic event stream with typed badges and timestamps.
- **Mission Control Workbench Tab** (`agent-studios.missionControl`):
  - Full-screen editor tab providing the 9-tab observability dashboard (Fleet, Tasks Kanban/DAG, Runs, Review, Diagnostics).

---

## 4. Zero-Loss Mode Switching Contract

### 4.1 Sequence Diagram: Toggling from Agent Mode to IDE Mode

```text
Operator          Agent Mode UI             agsd Daemon               IDE Mode Workbench
   │                    │                        │                            │
   │── Click "Open IDE" ┤                        │                            │
   │                    │── SwitchModeRequest ──►│                            │
   │                    │                        │── Retain Session & Run ───┐│
   │                    │                        │   (No process restart)    ││
   │                    │                        │◄──────────────────────────┘│
   │                    │◄── ModeTransitionReady─┤                            │
   │                    │                        │                            │
   │                    │   [UI Transitions]     │                            │
   │                    ▼                        │                            ▼
   │─────────────────────────────────────────────────────────────► Launch Code-OSS Shell
   │                                             │                 (Connects to agsd)
   │                                             │◄── IPC Connect ────────────┤
   │                                             │    (Existing session_id)   │
   │                                             │                            │
   │                                             │── Replay Events Since ────►│
   │                                             │   (Monotonic stream)       │
   │                                             │                            │
   │                                             │   [Active Turn Continues]  │
   │                                             │── SSE Token Stream ───────►│
```

### 4.2 State Preservation Invariants
1. **Thread Continuity**: Subagent OS processes remain running; their `ThreadId` and OS PID are preserved.
2. **Worktree Integrity**: Dedicated Git worktrees remain leased; no checkout or reset occurs.
3. **Stream Continuity**: Output token streams buffer in `agsd` and reconnect seamlessly to the IDE sidebar.
4. **Approval Safety**: Pending approval requests remain open in both modes until resolved.
5. **Zero Duplicate Spawns**: Switching modes never creates a second session or restarts completed tasks.
