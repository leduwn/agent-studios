# Agent Studios — Agent Mode & IDE Mode UX Specification

> **Status**: Core User Experience Specification
> **Precedence**: Subservient to `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. Executive Summary

Agent Studios resolves the tension between high-level autonomous agent delegation and detailed manual software engineering by providing **two presentation modes over a single shared execution session**:

1. **Agent Mode**: An agent-first, session-first conversational workspace for goal specification, planning, autonomous multi-agent execution, and progressive orchestration disclosure.
2. **IDE Mode**: A full-scale, Code-OSS-based professional developer workbench for deep code editing, debugging, terminal interaction, source control, and extension integration.

Switching between modes is instantaneous and lossless. They share the same underlying session ID, working directory, task graph, active agents, approval queue, and Git worktrees.

---

## 2. Agent Mode: Initial Startup Experience

Agent Studios launches by default into **Agent Mode**. It does **not** open into a dense code editor, nor does it open into an overwhelming multi-agent dashboard.

### Initial Launch View (Information Architecture)

```text
┌────────────────────────────────────────────────────────┐
│ Agent Studios                                 [ IDE ]  │
├────────────────────────────────────────────────────────┤
│                                                        │
│             What do you want to work on?               │
│                                                        │
│             [ Ask Agent Studios...               ]     │
│                                                        │
│             Workspace ▼          Model ▼               │
│                                                        │
│             Recent sessions                            │
│             • feat/auth-service (2h ago)               │
│             • fix/db-migration (yesterday)             │
│                                                        │
└────────────────────────────────────────────────────────┘
```

### Core Design Qualities
- **Agent-First**: The entry point is natural language dialogue with the cognitive Coordinator.
- **Session-First**: Work is organized around cohesive development sessions tied to Git workspaces.
- **Progressive Disclosure**: Advanced orchestration controls (DAG visualization, physical worktree tables, raw event streams) remain concealed until task execution begins.
- **Monochrome & Quiet**: Clean, high-contrast, distraction-free aesthetic (white, black, neutral grays). No neon borders, glowing gradients, or sci-fi gimmicks.

---

## 3. Progressive Orchestration Disclosure

When the user enters a request, the UI smoothly unfolds execution progress without overwhelming the user:

### Conceptual Interaction Flow

```text
User:
"Build authentication with Google and GitHub and add tests."

Agent Studios (Coordinator):
"I'll inspect the project and coordinate the implementation."

✓ Analyze project
● Backend authentication
● Frontend login
○ Testing
○ Review

Coder
Working on authentication routes...

Frontend
Updating login components...

[ View Plan ]  [ Open in IDE ]
```

### Key Interaction Principles
- **Primary Interaction Axis**: The user speaks directly to **Agent Studios / Coordinator**.
- **Workers as Implementation Resources**: Specialized worker agents (coder, reviewer, tester) execute in parallel sub-tasks. The user is **never** forced to manually switch between separate chatbot tabs or prompt individual worker agents.
- **Actionable Callouts**: Simple inline actions allow users to inspect the topological DAG (`[ View Plan ]`) or transition to hands-on editing (`[ Open in IDE ]`).

---

## 4. The Canonical Product Flow

Every execution in Agent Studios strictly follows this deterministic pipeline:

$$\textbf{Goal} \;\longrightarrow\; \textbf{Run} \;\longrightarrow\; \textbf{Task Graph (DAG)} \;\longrightarrow\; \textbf{Assigned Agents} \;\longrightarrow\; \textbf{Execution} \;\longrightarrow\; \textbf{Result}$$

1. **Goal**: User's natural language request.
2. **Run**: Authoritative execution container tracked in the Control Plane.
3. **Task Graph**: Validated DAG of discrete subtasks.
4. **Assigned Agents**: Dedicated internal Codex agents or external runtime processes bound to specific tasks.
5. **Execution**: Isolated execution in parallel Git worktrees with pre-execution budget gating.
6. **Result**: Cryptographically tracked patch artifacts reconciled into the integration workspace.

---

## 5. Task Graph UX Specification

When the user clicks `[ View Plan ]` or navigates to the orchestration panel, the interactive **Task Graph** view renders:

```text
        Architecture
             │
      ┌──────┴──────┐
      ▼             ▼
   Backend       Frontend
      │             │
      └──────┬──────┘
             ▼
           Testing
             │
             ▼
           Review
```

### Long-Term Interaction Requirements
- **Viewport Manipulation**: Pan, smooth zoom, fit to window, center selected node, reset camera, minimap overview.
- **Keyboard Navigation**: Arrow keys to traverse dependency edges, Tab to navigate between nodes, Enter to open node details, Space to toggle node expansion.
- **Auto-Layout**: Deterministic hierarchical layout engine (top-to-bottom or left-to-right) preserving DAG structure.
- **Filtering & Focus**: Filter nodes by state (`Ready`, `Running`, `Blocked`, `Succeeded`, `Failed`, `Cancelled`), highlight critical paths, and automatically follow active execution.
- **Subtle Animated Connectors**: Modest, quiet visual indicators along dependency lines signaling:
  - Task started
  - Worker assigned
  - Dependency unblocked / handoff
  - Artifact created & moving to reconciliation
  - Waiting for human approval
- **Visual Restraint**: Strictly neutral styling. No neon lines, glowing particles, or pulsing circles.

---

## 6. IDE Mode: Full Code-OSS Workbench

Clicking `[ Open in IDE ]` transitions seamlessly into **IDE Mode**, presenting the complete professional Code-OSS development environment:

```text
AGENT MODE
    │
    │ [ Open in IDE ]
    ▼
IDE MODE (Code-OSS)
    │
    │ [ Back to Agent ]
    ▼
AGENT MODE
```

### Full IDE Feature Parity
- **Editor**: Multi-tab Monaco editor with full syntax highlighting, bracket matching, multi-cursor editing, code folding, minimap, and breadcrumb navigation.
- **Explorer**: Real filesystem tree, file staging, file creation/deletion, drag-and-drop.
- **Search**: Global fast text search and replacement powered by ripgrep.
- **Source Control**: Full Git staging, branch switching, commit authoring, and side-by-side visual diffing.
- **Terminal**: Integrated xterm.js terminal emulator with PowerShell, Bash, and CMD shell profiles.
- **Debugging**: Breakpoint debugging, step-through, watch expressions, and call-stack inspection via DAP.
- **Problems & Output**: Compiler errors, LSP diagnostics, build output, runtime logs.
- **Extensions & Themes**: Full ecosystem compatibility with standard VS Code themes and language extensions.

---

## 7. Code-OSS Activity Bar & Sidebar Integration

To preserve the familiar developer ergonomics of VS Code, Agent Studios enforces a strict Activity Bar rule:

### The Activity Bar Rule
Agent Studios adds **exactly ONE primary entry** to the Code-OSS Activity Bar:

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

We do **not** clutter the Activity Bar with separate icons for Tasks, Runs, Artifacts, Worktrees, Approvals, Providers, Skills, or MCP. All Agent Studios functionality is unified inside the **Agents View**.

### The Agents Sidebar Structure
Selecting the `Agents` icon in the Activity Bar opens the cohesive Agent Studios sidebar:

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

---

## 8. Shared Session Semantics: Zero-Loss Toggling

Agent Mode and IDE Mode are two windows into the **same active runtime process**:

```text
┌───────────────────────────┐      ┌───────────────────────────┐
│        Agent Mode         │      │         IDE Mode          │
│   (Conversational UX)     │      │     (Code-OSS Shell)      │
└─────────────┬─────────────┘      └─────────────┬─────────────┘
              │                                  │
              └────────────────┬─────────────────┘
                               │
                               ▼
              ┌─────────────────────────────────┐
              │      Shared Session Kernel      │
              │  - Session ID (session_id)      │
              │  - Working Directory (cwd)      │
              │  - Active Run & Budgets (run)   │
              │  - TaskGraph & State (tasks)    │
              │  - Assigned Agents (agents)     │
              │  - Tool History (tool_history)  │
              │  - Approval Queue (approvals)   │
              │  - Provider & Model             │
              │  - Runtime State                │
              │  - Git Worktree Allocations     │
              │  - Content-Addressed Artifacts  │
              │  - Context & AGENTS.md State    │
              └─────────────────────────────────┘
```

### Non-Disruptive Transition Guarantees
1. **Zero Agent Restarts**: Active turns, tool executions, and LLM streaming are completely uninterrupted when switching modes.
2. **Zero Context Loss**: Conversation history, token counts, and compaction summaries remain identical.
3. **Zero Worktree Teleportation**: Physical worktrees and Git locks remain firmly pinned to their respective worker threads.
4. **Zero Task Graph Loss**: Running and blocked tasks maintain their exact state across transitions.
