# Agent Studios — Code-OSS Workbench Integration

> **Status**: Planned Architecture Specification (Target: Milestones M13–M15)
> **Current Reality**: Code-OSS workbench integration is **PLANNED**. Current milestones establish backend kernels, control plane, runtimes, and worktree isolation.
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

## 4. Built-in AI Extension (Milestone M14)

Milestone M14 integrates a native, built-in Code-OSS extension (`agent-studios-vscode`):
- Connects directly to the local Agent Studios App Server over native JSON-RPC IPC.
- Contributes the `Agents` view container and tree views.
- Contributes editor inline decorations (displaying which agent is editing or has edited specific lines).
- Provides editor context menu commands (`Ask Agent Studios to Explain`, `Refactor with Coder`, `Generate Unit Tests`).

---

## 5. Dual Presentation over Shared Session (Milestone M15)

Milestone M15 introduces the unified desktop layout switcher:
- **Agent Mode**: Full-window conversational workspace focused on goals, planning, and progressive orchestration disclosure.
- **IDE Mode**: Traditional Code-OSS workbench with the Agents sidebar active.
- **Zero Interruption**: Switching between Agent Mode and IDE Mode is purely a visual layout toggling. Both views bind to the **exact same underlying runtime session**, sharing the active run, DAG, agents, tool history, worktrees, and approvals without restart or context loss.
