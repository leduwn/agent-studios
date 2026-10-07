# Agent Studios — Master Vision & Project Constitution

> **Status**: Canonical Master Product Constitution
> **Precedence**: Supreme architectural document across all Agent Studios repositories, subsystems, and milestones.

---

## 1. Official Product Statement

> **Agent Studios is a Windows-first, open-source, VS Code-class AI development environment that combines the portable capabilities of Codex with a deterministic multi-agent control plane, multi-provider model routing, external agent runtimes, isolated Git workspaces, artifact-based orchestration, and a native IDE experience.**

### The Concise Formula

$$\begin{aligned}
\textbf{Agent Studios} = &\quad \textbf{VS Code-class IDE} \\
&+\; \textbf{Codex-class Agent Runtime} \\
&+\; \textbf{Deterministic Multi-Agent Control Plane} \\
&+\; \textbf{Open Provider / Runtime Ecosystem}
\end{aligned}$$

### Canonical Short Identity

$$\textbf{Agent Studios} = \textbf{VS Code-class IDE} + \textbf{deterministic multi-agent control plane.}$$

---

## 2. What Agent Studios Is NOT

To maintain absolute architectural clarity, Agent Studios explicitly rejects several common reductive paradigms:

1. **NOT an AI chatbot with an editor**: Agent Studios is not a standard text editor with a passive sidebar chat widget. Agents are autonomous workers executing in isolated workspaces with real tool use, bash/powershell terminals, testing, and patch reconciliation.
2. **NOT a multi-agent dashboard**: Agent Studios is not a giant SaaS observability dashboard with neon graphs, simulated agent dialogues, and disconnected chat rooms. The user does not sit and watch a swarm of bots chatter; the user assigns goals and receives verified code.
3. **NOT a Codex clone**: Agent Studios does not merely recompile the OpenAI Codex CLI. It builds a complete IDE, adds multi-provider neutrality (Anthropic, Gemini, Ollama, DeepSeek, etc.), introduces multi-agent DAG coordination, and decouples execution into isolated Git worktrees.
4. **NOT an OpenCode clone**: Agent Studios is inspired by OpenCode's provider openness and desktop ergonomics, but OpenCode is an external reference and future external runtime adapter, not our core execution engine.
5. **NOT an AgentTeams clone**: Agent Studios adopts manager-worker and human-in-the-loop coordination concepts, but rejects turning the application into a complex social-network simulation of agents.
6. **NOT a provider-router wrapper**: Agent Studios is not a proxy server or simple API gateway. Provider routing is merely one foundational substrate serving deep code execution.
7. **NOT a custom IDE replacing Code-OSS**: Agent Studios does not attempt to reimplement VS Code in Tauri or Monaco. Code-OSS is our permanent, production-grade IDE foundation.

---

## 3. High-Level Product Architecture

```text
                    AGENT STUDIOS
                         │
              ┌──────────┴──────────┐
              │                     │
          AGENT MODE             IDE MODE
              │                     │
   Agent-first Workspace      Code-OSS Workbench
              │                     │
              └──────────┬──────────┘
                         │
                    Shared Session
                         │
                Agent Studios Runtime
                         │
              ┌──────────┴──────────┐
              │                     │
        Control Plane           Codex Engine
              │                     │
      ┌───────┴────────┐       Provider Gateway
      │                │              │
 Internal Agents   External       OpenAI
      │             Runtimes      Anthropic
   Codex Agents        │          Gemini
                   OpenCode       OpenRouter
                   Claude Code    Ollama
                                 LM Studio
                                 vLLM
                                 Custom
```

### Shared Session Semantics
Agent Mode and IDE Mode are two presentation surfaces over **one shared backend session**:
- **Shared State**: Both modes share the exact same `session_id`, working directory (`cwd`), active `run`, `task_graph`, registered `agents`, tool execution history, interactive `approvals`, active `provider/model` selection, allocated `worktrees`, generated `artifacts`, and memory/context.
- **Zero Interruption Switching**: Toggling from Agent Mode to IDE Mode (or vice versa) does not restart the agent process, abort active tool turns, reset context history, or disrupt Git worktree locks.

---

## 4. Agent Mode: Agent-First Desktop UX

When launched, Agent Studios opens into an **Agent-first desktop experience** inspired by modern autonomous coding tools (Codex App, OpenCode, Claude Desktop). It does **not** initially open into a dense code editor or an overwhelming orchestration dashboard.

### Conceptual Initial State

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

### Information Architecture Principles
- **Agent-First**: Centered on user intent and cognitive collaboration.
- **Session-First**: Projects and tasks are framed around cohesive working sessions.
- **Conversation-First**: Initial interaction is natural language dialogue with the Coordinator.
- **Workspace-Aware**: Introspects local Git repositories and active branches.
- **Provider/Model-Aware**: Seamlessly displays and switches between configured LLM models.
- **Progressive Disclosure**: Detailed orchestration views (DAGs, worktrees, logs) remain tucked away until task execution begins.

---

## 5. Progressive Orchestration Disclosure

When the user enters a request, the orchestration machinery gradually reveals itself as needed:

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

### The Primary Interaction Axis
The user interacts primarily with **Agent Studios / Coordinator**. Workers (coder, tester, reviewer) are implementation resources allocated by the Coordinator and scheduled by the Control Plane. Users are never required to manually manage, prompt, or babysit individual sub-agents.

---

## 6. Canonical Product Workflow

Every action in Agent Studios follows this immutable lifecycle:

$$\text{User Goal} \longrightarrow \text{Run} \longrightarrow \text{Task Graph (DAG)} \longrightarrow \text{Assigned Agents} \longrightarrow \text{Execution} \longrightarrow \text{Result}$$

1. **User Goal**: High-level natural language prompt or specification.
2. **Run**: Execution container managed by the Control Plane, tracking budgets and lifecycles.
3. **Task Graph**: Directed acyclic graph of tasks created by the cognitive Coordinator and topologically validated by the Control Plane.
4. **Assigned Agents**: Dedicated internal Codex agents or external runtime processes bound to specific tasks.
5. **Execution**: Parallel execution within isolated Git worktrees with pre-execution budget and permission enforcement.
6. **Result**: Cryptographically tracked patch artifacts reconciled into the integration workspace.

---

## 7. Task Graph & Orchestration UX

While progressive disclosure keeps the default UI clean, the **Task Graph** is a first-class interactive view available at any time:

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
- **Navigation**: Pan, zoom, fit to screen, center, reset, minimap, and full keyboard navigation.
- **Layout**: Deterministic DAG auto-layout (top-to-bottom or left-to-right), node collapse, filtering by state (`Ready`, `Running`, `Blocked`, `Succeeded`), and execution following.
- **Visual Feedback**: Subtle, quiet animated connectors signaling meaningful activity: task starts, agent assignment, dependency unblocking, artifact movement, and approval waiting.
- **Aesthetic**: Strictly monochrome and neutral. No glowing neon borders or sci-fi dashboard visuals.

---

## 8. IDE Mode: Code-OSS Professional Workbench

When the user clicks `[ Open in IDE ]`, Agent Studios transitions smoothly into **IDE Mode**, revealing the full Code-OSS workbench:

### Code-OSS Subsystems
- **Code Editor**: Full Monaco editor with syntax highlighting, multi-cursor, code folding, minimap.
- **Explorer**: Native filesystem tree, file staging, drag-and-drop.
- **Search**: Fast ripgrep-powered text search across workspace.
- **Source Control**: Git status, visual diffing, branch management, staging.
- **Terminal**: Integrated xterm.js terminal with shell integration.
- **Debugging**: Interactive breakpoints, call stacks, variable watches via Debug Adapter Protocol (DAP).
- **Problems & Output**: Compiler diagnostics, LSP errors, runtime logs.
- **Ecosystem**: Full compatibility with standard VS Code themes, keybindings, and extensions.

### Activity Bar Integration Rule
Agent Studios respects standard Code-OSS workbench conventions. The Activity Bar contains standard entries plus **exactly one** primary Agent Studios entry:
1. `Explorer`
2. `Search`
3. `Source Control`
4. `Run & Debug`
5. `Extensions`
6. **`Agents`** (Agent Studios)

All Agent Studios concepts (Tasks, Runs, Artifacts, Worktrees, Approvals, Providers, Skills, MCP) reside within the dedicated Agent Studios sidebar, editor tabs, and status bar indicators. We do not pollute the primary Activity Bar with dozens of disparate icons.

---

## 9. Cognitive Coordinator vs. Deterministic Control Plane

Agent Studios establishes an absolute separation of concerns between cognitive reasoning and deterministic control:

| Domain | Cognitive Coordinator (LLM) | Deterministic Control Plane (Rust Kernel) |
| :--- | :--- | :--- |
| **Nature** | Probabilistic, creative, adaptive | Mathematical, typed, immutable, transactional |
| **Responsibilities** | • Decomposes high-level goals<br>• Synthesizes architectural plans<br>• Selects agent roles & models<br>• Generates & refines code<br>• Conducts heuristic code reviews | • Enforces task DAG acyclicity (Kahn's algorithm)<br>• Manages task & agent state transitions<br>• Schedules tasks by priority & readiness<br>• Allocates physical Git worktrees<br>• Enforces tool & turn execution budgets<br>• Owns artifact versioning & SHA-256 blobs<br>• Persists monotonic event log & replay<br>• Propagates hierarchical cancellation |
| **Failure Mode** | Hallucination, context overflow, prompt drift | Fail-closed errors, rollback, crash retention |

---

## 10. Upstream References & Engineering Philosophy

1. **OpenAI Codex**: Primary internal execution engine. We inherit portable Codex capabilities (agent loop, shell execution, `apply_patch`, sandboxing, approvals, context history compaction, subagents, MCP, Skills, `AGENTS.md`) rather than rebuilding simplified substitutes.
2. **OpenCode**: Reference for multi-provider openness, BYOK ergonomics, local models, and desktop UX. OpenCode is not our core engine, but an architectural reference and planned external runtime adapter.
3. **AgentTeams**: Reference for multi-agent coordination, human-in-the-loop approvals, and manager-worker patterns. Agent Studios avoids SaaS dashboard clutter.
4. **Code-OSS**: Our permanent IDE foundation. We build upon VS Code's open-source core instead of creating custom editor shells.

---

## 11. Persistent Memory Instruction for Future Agents

```markdown
<!-- CANONICAL INSTRUCTION FOR ALL CODING AGENTS -->
For every future Agent Studios milestone:

Before implementation, read:
- docs/agent-studios/MASTER_VISION.md
- docs/agent-studios/PRODUCT_PRINCIPLES.md
- docs/agent-studios/ROADMAP.md

Then read the subsystem-specific documents relevant to the milestone.

If the requested implementation conflicts with MASTER_VISION.md or an established architectural invariant, stop and report the conflict before changing architecture.
```
