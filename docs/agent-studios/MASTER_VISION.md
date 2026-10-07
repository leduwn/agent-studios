# Agent Studios — Master Vision & Project Constitution

> **Status**: Canonical Master Product Constitution
>
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

1. **NOT an AI chatbot with an editor**: Agent Studios is not a standard text editor with a passive sidebar chat widget. Agents are autonomous workers executing in isolated workspaces with real tool use, terminal interaction, testing, and patch reconciliation.
2. **NOT a multi-agent dashboard**: Agent Studios is not a giant SaaS observability dashboard with neon graphs, simulated agent dialogues, and disconnected chat rooms. The user assigns goals and receives verified code.
3. **NOT a Codex clone**: Agent Studios does not merely recompile the OpenAI Codex CLI. It builds a complete IDE, adds multi-provider neutrality (Anthropic, Gemini, Ollama, DeepSeek, etc.), introduces deterministic multi-agent DAG coordination, and isolates execution into dedicated Git worktrees.
4. **NOT an OpenCode clone**: Agent Studios is inspired by OpenCode's provider openness and desktop ergonomics, but OpenCode is an external architectural reference and planned external runtime adapter (Milestone M11), not our core internal execution engine.
5. **NOT an AgentTeams clone**: Agent Studios adopts manager-worker and human-in-the-loop coordination concepts from AgentTeams, but preserves its own product model rooted in native Git primitives, compiler feedback, and deterministic task graphs rather than conversational chat room consensus.
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
             (session_id, cwd, run, tasks,
             agents, tools, approvals, wt)
                         │
                Agent Studios Runtime
                         │
              ┌──────────┴──────────┐
              │                     │
        Control Plane           Codex Engine
      (Deterministic Kernel) (Execution Engine)
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
- **Zero Interruption Switching**: Toggling from Agent Mode to IDE Mode (or vice versa) does not restart the agent process, restart active runs, lose tool history, lose task state, lose graph state, lose context, lose worktree bindings, or create an unrelated session.

---

## 4. Agent Mode: Agent-First Desktop UX

When launched, Agent Studios opens into an **Agent-first desktop experience** inspired by modern autonomous coding tools (Codex App, OpenCode, Claude Desktop).

### Default Startup Behavior
The default Agent Mode is **NOT** a Task Graph dashboard, worktree dashboard, or multi-agent monitoring dashboard.

It is conceptually:

```text
┌────────────────────────────────────────────────────────┐
│ Agent Studios                                 [ IDE ]  │
├────────────────────────────────────────────────────────┤
│                                                        │
│             What do you want to work on?               │
│                                                        │
│             [ Ask Agent Studios...               ]     │
│                                                        │
│             Workspace ▼          Model / runtime ▼     │
│                                                        │
│             Recent sessions                            │
│             • feat/auth-service (2h ago)               │
│             • fix/db-migration (yesterday)             │
│                                                        │
└────────────────────────────────────────────────────────┘
```

The exact visual layout is future design work. Canonical behavior is:
- **Agent-First**: Centered on user intent and cognitive collaboration.
- **Session-First**: Projects and tasks are framed around cohesive working sessions.
- **Conversation-First**: Initial interaction is natural language dialogue with the Coordinator.
- **Progressive Disclosure**: Detailed orchestration views (Task Graph DAGs, physical worktrees, raw logs) remain concealed until task execution begins.
- **Monochrome & Quiet**: Clean, high-contrast, distraction-free aesthetic. No neon borders or sci-fi gimmicks.

---

## 5. Progressive Orchestration Disclosure

When the user enters a request, the orchestration machinery gradually reveals itself:

```text
User:
"Build authentication with Google and GitHub and add tests."

Agent Studios (Coordinator):
"I'll inspect the project and coordinate the implementation."

✓ Analyze project
● Backend authentication (Coder)
● Frontend login (Frontend)
○ Testing (Tester)
○ Review (Reviewer)

Coder
Working on authentication routes...

Frontend
Updating login components...

[ View Plan ]  [ Open in IDE ]
```

During execution, progressively reveal:
1. **Goal**: User-defined intent and constraints.
2. **Plan**: Coordinator task decomposition.
3. **Task Progress**: Active checklist of tasks with real-time state.
4. **Active Workers**: Currently assigned agent roles and their current actions.
5. **Important Tool Activity**: Key compiler runs, test results, and file mutations.
6. **Approvals**: Real-time permission gate for sensitive commands.
7. **Results**: Reconciled patch artifacts and summary deliverables.

The user interacts primarily with **Agent Studios / Coordinator**. Workers (coder, tester, reviewer) are implementation resources allocated by the Coordinator and scheduled by the Control Plane.

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

While progressive disclosure keeps the default UI clean, the **Task Graph** is a first-class interactive drill-down view available during execution:

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

### Interaction Requirements
- **Drill-Down Surface**: The Task Graph is an interactive drill-down surface, not the initial startup screen.
- **Navigation**: Pan, zoom, fit to screen, center, reset, minimap, and full keyboard navigation.
- **Layout**: Deterministic DAG auto-layout (top-to-bottom or left-to-right), node collapse, filtering by state (`Ready`, `Running`, `Blocked`, `Succeeded`), and execution following.
- **Subtle Connectors**: Subtle, quiet animated connectors signaling meaningful activity: task starts, agent assignment, dependency unblocking, artifact movement, and approval waiting. Animated connectors belong strictly to Task Graph interaction design, not the startup screen.
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

All Agent Studios concepts (Tasks, Runs, Artifacts, Worktrees, Approvals, Providers, Skills, MCP) reside within the dedicated Agent Studios sidebar, editor tabs, and status bar indicators. We do not pollute the primary Activity Bar with disparate icons.

---

## 9. Cognitive Coordinator vs. Deterministic Control Plane

Agent Studios establishes an absolute separation of concerns between cognitive reasoning and deterministic control:

| Domain | Cognitive Coordinator (LLM) | Deterministic Control Plane (Rust Kernel) |
| :--- | :--- | :--- |
| **Nature** | Probabilistic, creative, adaptive | Mathematical, typed, immutable, transactional |
| **Responsibilities** | • Decomposes high-level goals<br>• Synthesizes architectural plans<br>• Selects agent roles & models<br>• Generates & refines code<br>• Conducts heuristic code reviews | • Enforces task DAG acyclicity (Kahn's algorithm)<br>• Manages task & agent state transitions<br>• Schedules tasks by priority & readiness<br>• Allocates physical Git worktrees<br>• Enforces tool & turn execution budgets<br>• Owns artifact versioning & SHA-256 blobs<br>• Persists monotonic event log & replay<br>• Propagates hierarchical cancellation |
| **Failure Mode** | Hallucination, context overflow, prompt drift | Fail-closed errors, rollback, crash retention |

The LLM proposes actions; the Control Plane validates and executes state transitions.

---

## 10. Upstream References & Engineering Philosophy

1. **OpenAI Codex (`codex-rs`)**: Primary internal execution engine. We inherit portable Codex capabilities (agent loop, shell execution, `apply_patch`, sandboxing, approvals, context history compaction, subagents via `AgentControl`, MCP, Skills, `AGENTS.md`) rather than rebuilding simplified substitutes. Modifications to `codex-rs/` are strictly minimized to generic, provider-neutral seams.
2. **OpenCode (`opencode-ai`)**: Reference for multi-provider openness, Bring-Your-Own-Key (BYOK) ergonomics, local and self-hosted models, and desktop developer ergonomics. OpenCode is an architectural reference and planned external runtime adapter (Milestone M11), not our core internal engine.
3. **AgentTeams**: Reference for multi-agent coordination, human-in-the-loop approvals, and manager-worker delegation patterns. AgentTeams informs orchestration concepts but is not the target Agent Studios UI. Agent Studios roots coordination in native Git primitives, compiler feedback, and deterministic task graphs.
4. **Code-OSS**: Our permanent IDE foundation. We build upon VS Code's open-source core instead of creating custom editor shells.

---

## 11. Canonical Terminology & Conceptual Distinctions

### Terminology Table

| Term | Canonical Definition |
| :--- | :--- |
| **Agent Mode** | Agent-first, session-first conversational presentation surface focused on goal setting, planning, autonomous execution, and progressive orchestration disclosure. |
| **IDE Mode** | Editor-first, Code-OSS-based professional developer workbench presentation surface for manual editing, LSP, DAP debugging, terminal interaction, and Git control. |
| **Studio** | Top-level organizational workspace container binding a project repository, configuration, active sessions, and local control plane state. |
| **Goal** | High-level user objective expressed in natural language or specification submitted to the Coordinator. |
| **Run** | Concrete execution instance of a goal, tracking execution lifecycle, overall token/tool budgets, active task graph, and completion status. |
| **Task** | Discrete, typed unit of work in a Run with defined inputs, assigned agent role, workspace access policy, dependencies, and lifecycle states. |
| **Task Graph** | Directed Acyclic Graph (DAG) of Tasks validated by Kahn's algorithm, governing parallel readiness and execution ordering. |
| **Agent** | Autonomous worker thread or process executing tasks under assigned tool and turn constraints. |
| **Coordinator** | Cognitive LLM agent responsible for high-level goal decomposition, task graph formulation, agent assignment, and heuristic result review. |
| **Worker** | Specialized child agent (e.g., Coder, Reviewer, Tester) executing a specific Task within an allocated workspace. |
| **Control Plane** | Authoritative, deterministic Rust kernel (`agent-studios-control-plane`) managing state machines, DAG validation, leases, budgets, event logs, and artifacts. |
| **Provider** | API protocol driver and vendor endpoint configuration (e.g., OpenAI, Anthropic, Gemini, OpenRouter, Ollama) defining model descriptors and wire transports. |
| **Provider Instance** | Concrete, configured provider endpoint binding base URL, authentication secret reference, and rate limits. |
| **ModelRef** | Composite authoritative runtime model identifier: `{ provider_instance_id: ProviderInstanceId, model_id: ModelId }`. |
| **Runtime** | Execution environment providing process lifecycle, tool dispatch, sandboxing, and session containment for agents. |
| **Internal Runtime** | Built-in Codex execution engine (`codex-rs`) running hierarchical agent threads directly within the host process. |
| **External Runtime** | Process-isolated external agent engine (e.g., OpenCode CLI, Claude Code CLI) coordinated via standardized runtime adapters. |
| **ExecutionWorkspace** | Physical and subpath directory context for agent execution, decoupling `source_root` (Git repo root) from `source_cwd` (working sub-directory). |
| **Source Workspace** | Primary repository working tree where user edits occur and clean integrations land. |
| **Managed Worktree** | Dedicated, temporary Git worktree allocated under `.git/agent-studios/worktrees/` for isolated mutating worker execution. |
| **Artifact** | Immutable, content-addressed deliverable (e.g., unified patch diff, build binary) indexed by SHA-256 hash. |
| **Reconciliation** | Deterministic engine that validates (`git apply --check`) and applies worker patch artifacts to the integration workspace. |
| **ReconciledOutput** | Authoritative result produced upon successful patch reconciliation, recording applied files, patch stats, and target branch. |
| **Approval** | Interactive human-in-the-loop decision checkpoint required before executing privileged tools, dangerous shell commands, or destructive actions. |
| **Skill** | Modular, reusable instruction and tool bundle defined in `SKILL.md` format, progressively loaded into agent context. |
| **Plugin** | Distributable package combining Skills, MCP server configurations, and lifecycle hooks into a single archive. |
| **MCP** | Model Context Protocol standard enabling agents to interact with local and remote external tool and resource servers. |
| **Hook** | Deterministic script or executable triggered at specific lifecycle events (`pre_turn`, `post_task`, `on_reconciliation`). |

### Key Conceptual Distinctions

1. **Provider != Runtime**: A Provider is an LLM wire protocol adapter and model gateway (Anthropic Messages, OpenAI Chat Completions); a Runtime is an agent execution environment (Internal Codex, OpenCode, Claude Code).
2. **Agent != Task**: An Agent is an execution resource with its own lifecycle (`Registered` $\rightarrow$ `Idle` $\leftrightarrow$ `Busy`); a Task is a unit of work with its own lifecycle (`Pending` $\rightarrow$ `Blocked` $\leftrightarrow$ `Ready` $\rightarrow$ `Running` $\rightarrow$ `Succeeded`/`Failed`). An agent may be `Idle` while a task is `Blocked`.
3. **Blocked != Cancelled**: A task blocked by an unmerged dependency or reconciliation conflict is `Blocked(ReconciliationConflict)`, never `Cancelled`. Cancellation is permanent revocation of intent; blocked is a conditional pause awaiting resolution.
4. **Agent Mode != separate runtime**: Agent Mode is a presentation surface, not a separate backend or runtime. It shares the exact same backend session, run, tasks, and worktrees as IDE Mode.
5. **IDE Mode != separate runtime**: IDE Mode is a presentation surface over the same shared session as Agent Mode. Toggling modes never restarts agents or drops state.
6. **Model slug != ModelRef**: A model slug (e.g., `gpt-4o`) is ambiguous across providers; `ModelRef` uniquely identifies the exact provider instance and model ID.
7. **Worktree != Thread**: A Worktree is a physical filesystem directory checked out from Git; a Thread is a Codex agent execution context. A thread is bound to a worktree during execution, but they are distinct entities.
8. **Artifact != Git commit**: An Artifact is an immutable, content-addressed blob (SHA-256 patch diff or file); a Git commit is a version control node in Git history created upon reconciliation.
9. **Coordinator != Control Plane**: The Coordinator is a probabilistic LLM component that plans and reasons; the Control Plane is a deterministic Rust state engine that validates, schedules, and enforces.

---

## 12. Non-Negotiable Architectural Invariants

Every design decision, code modification, and roadmap milestone must adhere to these 30 non-negotiable rules:

1. **Codex is the primary internal execution engine**: Internal agents execute on the native OpenAI Codex engine (`codex-rs`).
2. **Do not rewrite portable Codex capabilities unnecessarily**: Preserve the core agent loop, tool dispatch, sandboxing, approvals, context compaction, subagents, MCP, Skills, and `AGENTS.md`.
3. **Control Plane is authoritative orchestration state**: The Control Plane owns durable state for studios, runs, tasks, agents, leases, and artifacts.
4. **UI is a projection**: UI surfaces (Agent Mode and IDE Mode) are reactive read-model projections over Control Plane events with zero speculative state ownership.
5. **Agent is a worker, not the whole product**: Users interact with Agent Studios and its Coordinator; sub-agents are implementation resources.
6. **Canonical Product Workflow**: Every execution strictly follows: $\text{Goal} \rightarrow \text{Run} \rightarrow \text{Task Graph (DAG)} \rightarrow \text{Assigned Agents} \rightarrow \text{Execution} \rightarrow \text{Result}$.
7. **Provider and Runtime are separate abstractions**: Never conflate LLM wire APIs with agent execution environments.
8. **Runtime model identity is Provider Instance + Model**: All runtime model routing uses composite `ModelRef`.
9. **Agent state != Task state**: Agent lifecycle (`Registered` $\rightarrow$ `Idle` $\leftrightarrow$ `Busy`) and task lifecycle (`Pending` $\rightarrow$ `Ready` $\rightarrow$ `Running`) remain decoupled.
10. **Blocked != Cancelled**: Unresolved dependencies, reconciliation conflicts, and waiting approvals transition to `Blocked`, never `Cancelled` or `Failed`.
11. **Mutating parallel workers use isolated workspaces**: Parallel mutating workers execute in dedicated Git worktrees.
12. **Thread/cwd/worktree affinity must remain coherent**: A thread's execution identity is inextricably bound to its allocated worktree path.
13. **Threads do not teleport between worktrees**: Dynamically mutating an active thread's `cwd` across worktree boundaries during a turn is strictly forbidden.
14. **Dirty worktrees are retained, not automatically destroyed**: Automatic destructive cleanup equivalent to `git reset --hard`, `git clean -fdx`, or `git worktree remove --force` is strictly forbidden.
15. **Artifact lineage is ControlPlane-owned**: The Control Plane authoritatively allocates artifact version numbers ($v1, v2\dots$).
16. **ReconciledOutput waits for accepted integrated output**: Downstream tasks requiring integration output wait for formal reconciliation completion.
17. **Agent Mode and IDE Mode share one session/runtime**: Both presentation modes bind to the exact same underlying session, run, tasks, agents, and worktrees.
18. **Agent Mode is conversation/session-first**: The application launches into a clean conversational prompt, not an overwhelming code editor or monitoring dashboard.
19. **IDE Mode uses Code-OSS as final IDE foundation**: Code-OSS provides permanent developer workbench capabilities.
20. **Only one primary Agents Activity Bar entry initially**: In Code-OSS, Agent Studios contributes exactly one primary icon: `Agents`.
21. **No mandatory OpenAI/ChatGPT login**: Bring-Your-Own-Key (BYOK) and local models are first-class day-one requirements.
22. **English and Vietnamese are first-class UI locales**: Complete day-one localization parity for English and Vietnamese.
23. **Display language is independent from agent response language**: Changing UI display language does not alter the language the LLM agent responds in.
24. **External runtimes remain below the Control Plane**: Non-Codex runtimes (OpenCode, Claude Code) are governed by the Control Plane via adapters.
25. **Git remains real Git**: All workspace isolation, branching, and patching build on standard native Git primitives.
26. **Durable events commit before broadcast**: Events commit to durable storage before broadcasting to UI subscribers.
27. **Avoid unnecessary codex-rs production patches**: Keep upstream modifications minimal, generic, provider-neutral, and default-off.
28. **Windows-first, portable core**: Primary development, testing, and release target is native Windows 11 x64 (MSVC), with a portable core.
29. **Useful / Native / Quiet**: Visual and interaction design is distraction-free monochrome; no neon borders, glowing gradients, or sci-fi gimmicks.
30. **Future coding agents must stop and report before violating these invariants**: If a prompt or task contradicts any of these invariants, stop and report the conflict before writing code.

---

## 13. Architectural Decision Status Index

| Topic / Decision | Status | Architectural Stance |
| :--- | :--- | :--- |
| **Code-OSS as IDE Foundation** | **CANONICAL** | Code-OSS is our permanent IDE foundation; custom editor shells are rejected. |
| **Shared Session Semantics** | **CANONICAL** | Agent Mode and IDE Mode share one underlying session with lossless toggling. |
| **Single `Agents` Activity Bar Icon** | **CANONICAL** | Exactly one primary entry added to Code-OSS Activity Bar. |
| **Internal Codex Engine** | **IMPLEMENTED** | Upstream Codex (`codex-rs`) serves as the core internal agent execution engine. |
| **Deterministic Control Plane** | **IMPLEMENTED** | Single-writer actor, Kahn's algorithm DAG engine, and event store (`M02`). |
| **Multi-Provider Adapters & BYOK** | **IMPLEMENTED** | OpenAI, Anthropic, Gemini, Chat Completions adapters, zero-secret references (`M03–M07.6`). |
| **Internal Multi-Agent Hierarchy** | **IMPLEMENTED** | Hierarchical agent tree spawned via `ThreadManager::start_thread` and `AgentControl::spawn` (`M08`). |
| **Worktree, Task & Artifact Orchestration** | **UNDER FINAL REVIEW** | Native Git worktrees, ephemeral index change capture, patch reconciliation (`M09`). |
| **External Runtime Interface** | **PLANNED** | Standardized `AgentRuntime` lifecycle interface and process containment (`M10`). |
| **OpenCode Runtime Adapter** | **PLANNED** | Adapter wrapping OpenCode CLI and sessions (`M11`). |
| **Claude Code Runtime Adapter** | **PLANNED** | Adapter wrapping Claude Code CLI (`M12`). |
| **Code-OSS Shell Integration** | **PLANNED** | Electron host integration and built-in AI extension (`M13–M15`). |
| **Full Codex Skills / MCP UX** | **PLANNED** | UI discovery and GitHub installation for Skills and MCP (`M16`). |
| **M13 IPC Transport Implementation** | **OPEN DESIGN QUESTION** | Transport choice (named pipe vs. WebSocket vs. stdio vs. other IPC) to be evaluated during M13 design. |
| **Process Containment Sandbox Architecture** | **OPEN DESIGN QUESTION** | Specific OS containment mechanics (Windows Job Objects/AppContainer, Linux cgroups, macOS sandbox-exec) to be finalized in M10+. |
| **Community Skill Signing & Trust Registry** | **OPEN DESIGN QUESTION** | Cryptographic provenance, signatures, and registry verification to be evaluated in M16 design. |

---

## 14. Persistent Memory Instruction for Future Agents

```markdown
<!-- CANONICAL INSTRUCTION FOR ALL CODING AGENTS -->
Before implementing any future Agent Studios milestone:

1. Read:
   docs/agent-studios/MASTER_VISION.md

2. Read:
   docs/agent-studios/PRODUCT_PRINCIPLES.md

3. Read:
   docs/agent-studios/ROADMAP.md

4. Read relevant subsystem documents.

5. Before modifying codex-rs, read:
   docs/agent-studios/CODEX_PATCHES.md

6. If the requested implementation conflicts with MASTER_VISION.md or a non-negotiable invariant:
   STOP and report the conflict before coding.
```
