# Agent Studios — Code-OSS Workbench Integration

> **Status**: Planned Architecture Specification (Target: Milestones M13–M15)
>
> **Current Reality**: Code-OSS workbench integration is **PLANNED**. Current milestones establish backend kernels, control plane, runtimes, and worktree isolation.
>
> **Precedence**: Subservient to `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. Code-OSS: Our Permanent IDE Foundation

Agent Studios adopts **Code-OSS (the open-source core of Visual Studio Code)** as its permanent IDE foundation:

- **No Custom Editor Rebuild**: We explicitly reject building a custom text editor using Monaco Editor wrapped in Tauri, Electron, or web views. Attempting to recreate VS Code's rich ecosystem (LSP, DAP debugging, terminal multiplexing, keyboard shortcuts, extensions) produces fragile, incomplete imitations.
- **Full IDE Parity**: Developers retain standard VS Code capabilities: multi-tab editor, tree explorer, ripgrep search, Git source control, integrated xterm terminal, breakpoint debugging, and extension marketplace support.

---

## 2. The Activity Bar Rule: Exactly ONE Primary Entry

To preserve clean developer ergonomics and avoid cluttered UI bars, Agent Studios enforces a strict Activity Bar rule:

```text
┌───┐
│ 📁│ Explorer
│ 🔍│ Search
│ 🔀│ Source Control
│ 🐞│ Run & Debug
│ 🧩│ Extensions
│ 🤖│ Agents (Agent Studios)
└───┘
```

Agent Studios adds **exactly ONE primary entry** to the Code-OSS Activity Bar: **`Agents`**.

We do **not** pollute the Activity Bar with separate icons for:
- Tasks
- Runs
- Artifacts
- Worktrees
- Approvals
- Providers
- Skills
- MCP

All agent orchestration controls are consolidated inside the cohesive **Agents View**.

---

## 3. The Agents Sidebar Layout

Clicking the `Agents` icon in the Activity Bar opens the structured Agent Studios sidebar:

```text
┌──────────────────────────────────────────────┐
│ AGENTS                                       │
├──────────────────────────────────────────────┤
│ ▶ CURRENT RUN                                │
│   Goal: Build authentication with Google...  │
│   Status: Running (2/5 tasks complete)       │
│   Active Agents: Coder, Frontend             │
├──────────────────────────────────────────────┤
│ ▼ TASKS                                      │
│   ✓ Analyze project                          │
│   ● Backend authentication (Coder)           │
│   ● Frontend login (Frontend)                │
│   ○ Testing (Tester)                         │
│   ○ Review (Reviewer)                        │
├──────────────────────────────────────────────┤
│ ▼ APPROVALS (1 PENDING)                      │
│   ⚠ Exec: npm install @auth/core             │
│   [ Approve ]  [ Deny ]                      │
├──────────────────────────────────────────────┤
│ ▶ ARTIFACTS (3)                              │
│   • task-1-patch.diff (SHA-256: 7f8a...)     │
│   • schema.sql                               │
├──────────────────────────────────────────────┤
│ ▶ TIMELINE & ACTIVITY                       │
│   14:02:11 Coder started task                │
│   14:02:15 Tool exec_command completed       │
└──────────────────────────────────────────────┘
```

Sections:
1. **Current Run**: Active objective, overall run status, active agents, token/cost counters.
2. **Tasks**: Interactive task checklist with quick actions to view details, inspect diffs, or open the visual DAG.
3. **Approvals**: Real-time human-in-the-loop permission inbox for shell commands, tool calls, and sensitive file operations.
4. **Artifacts**: List of generated patch diffs and build deliverables with SHA-256 hashes and version numbers.
5. **Timeline & Activity**: Chronological event feed of agent turns, tool calls, and reconciliation milestones.

---

## 4. Built-in AI Extension & Desktop IPC Boundary (Milestone M14)

Milestone M14 integrates a native, built-in Code-OSS extension (`agent-studios-vscode`):

```text
Code-OSS Workbench (Desktop UI / Extension)
        │
        │ IPC Boundary
        │ (Transport & Protocol TBD in M13 - Open Design Question)
        ▼
Agent Studios App Server (Rust)
        │
        ▼
Control Plane & Runtimes
```

### IPC Architecture & Open Transport/Protocol Question (OPEN DESIGN QUESTION in M13)
- **Canonical Transport-Independent Invariants**:
  - Typed request/response contracts and typed event envelopes.
  - Zero-secret wire payloads (tokens and credentials never cross in plaintext).
  - Deterministic event sequencing (monotonic sequence numbers).
  - Reconnect and state recovery semantics where required.
  - Shared-session identity (`session_id`).
- **Protocol & Physical Transport (OPEN DESIGN QUESTION in M13)**:
  - The exact choice of physical transport and protocol mechanism remains an open architectural decision to be determined during Milestone M13 architecture work.
  - Possible transport candidates include: named pipes, local WebSockets, stdio, native Electron/Node IPC, or another typed local IPC mechanism.
  - JSON-RPC serves as a **candidate / reference protocol** (given compatibility with Codex and app-server patterns), but is **not** a canonical required protocol today. The architecture constrains behavioral invariants, not premature implementation technology.
- **Extension Contributions**:
  - Contributes the `Agents` view container and sidebar tree views.
  - Contributes editor inline decorations (displaying which agent is editing or has edited specific lines).
  - Provides editor context menu commands (`Ask Agent Studios to Explain`, `Refactor with Coder`, `Generate Unit Tests`).

---

## 5. Dual Presentation over Shared Session (Milestone M15)

Milestone M15 introduces the unified desktop layout switcher:
- **Agent Mode**: Full-window conversational workspace focused on goals, planning, and progressive orchestration disclosure.
- **IDE Mode**: Traditional Code-OSS workbench with the Agents sidebar active.
- **Zero Interruption**: Switching between Agent Mode and IDE Mode is purely a visual layout toggling. Both views bind to the **exact same underlying runtime session**, sharing the active run, DAG, agents, tool history, worktrees, and approvals without restart, context loss, or process teleportation.
