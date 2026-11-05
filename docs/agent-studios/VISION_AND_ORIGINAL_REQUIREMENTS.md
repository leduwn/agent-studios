# Agent Studios — Product Vision & Original Requirements

> **Status**: Authoritative Architectural Baseline  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Precedence**: Derived directly from `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`

---

## 1. Executive Summary & Product Statement

`[USER_DIRECT_REQUIREMENT]`
> **Agent Studios is a Windows-first, open-source, VS Code-class AI development environment that combines the portable capabilities of Codex with a deterministic multi-agent control plane, multi-provider model routing, external agent runtimes, isolated Git workspaces, artifact-based orchestration, and a native IDE experience.**

### The Canonical Formula

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

`[EXISTING_CANONICAL_DECISION]`
To preserve uncompromising architectural integrity, Agent Studios explicitly rejects seven common reductive industry tropes:

1. **NOT an AI chatbot in an editor**: Not a conventional text editor with an ephemeral sidebar chat window. Subagents execute autonomously inside isolated worktrees with real tool use, integrated terminals, compiler passes, test runners, and patch reconciliation.
2. **NOT a multi-agent dashboard**: Not a SaaS observability platform with neon graphs, simulated agent dialogues, and disconnected chat rooms. The user delegates goals and receives verified, tested code.
3. **NOT a Codex clone**: Does not merely recompile the OpenAI Codex CLI. It builds a complete IDE, adds multi-provider neutrality (Anthropic, Gemini, Ollama, DeepSeek), introduces deterministic multi-agent DAG coordination, and isolates execution into dedicated Git worktrees.
4. **NOT an OpenCode clone**: OpenCode serves as an architectural reference and planned external runtime adapter target (`M11`), not our core internal execution engine.
5. **NOT an AgentTeams clone**: Adopts manager-worker and human-in-the-loop concepts, but anchors state in Git primitives, compiler feedback, and deterministic task graphs rather than conversational chat room consensus.
6. **NOT a provider-router wrapper**: Not an API proxy or simple LLM gateway. Provider routing is merely one foundational substrate serving deep code execution.
7. **NOT a custom IDE replacing Code-OSS**: Does not attempt to reimplement VS Code in Tauri or Monaco webviews. Code-OSS is our permanent, production-grade IDE foundation.

---

## 3. What Agent Studios IS

`[EXISTING_CANONICAL_DECISION]`
1. **VS Code-class professional IDE**: Code-OSS core providing Monaco editor, LSP diagnostics, DAP debugging, integrated terminal, source control, and extension ecosystem.
2. `[IMPLEMENTED_BEHAVIOR]` **Coding Agent Runtime**: Inherits upstream `codex-rs` engine, tool dispatch (`exec_command`, `apply_patch`), sandboxing, approvals, context compaction, subagents via `AgentControl`, MCP, Skills (`SKILL.md`), and `AGENTS.md`.
3. `[IMPLEMENTED_BEHAVIOR]` **Deterministic Multi-Agent Control Plane**: Single-writer Rust actor (`agent-studios-control-plane`), Kahn's algorithm DAG engine, transactional state staging, monotonic event log, and SHA-256 artifact versioning.
4. `[IMPLEMENTED_BEHAVIOR]` **Open Provider Ecosystem**: Multi-provider neutrality, BYOK (OpenAI, Anthropic, Gemini, Chat Completions, Ollama, local models), zero mandatory OpenAI/Microsoft login, zero secret leaks (`SecretReference` resolved in-memory).

---

## 4. Target Personas & Core Journeys

### Persona A: The Operator / Delegator
- **Role**: Software architect, engineering manager, or product lead.
- **Goal**: Formulates high-level specifications and delegates complex multi-file features.
- **Workflow**: Opens Agent Studios in **Agent Mode**, selects model and workspace, defines goal, reviews generated DAG, approves high-risk actions, inspects reconciled diffs.
- **Core Requirement**: High confidence, truthful telemetry, zero silent failures, complete reviewability of code artifacts before merging.

### Persona B: The Hands-on Developer
- **Role**: Implementation engineer, systems programmer, or frontend specialist.
- **Goal**: Writes code directly while coordinating background autonomous workers.
- **Workflow**: Works in **IDE Mode** with full Code-OSS workbench, sets DAP breakpoints, navigates LSP definitions, uses integrated terminal, reviews agent diffs in side-by-side Monaco editors.
- **Core Requirement**: Zero friction, instantaneous mode switching, non-blocking background agents, no interference with active workspace files.

---

## 5. The 30 Canonical Invariants

`[EXISTING_CANONICAL_DECISION]`
These 30 constitutional invariants govern all design and code in Agent Studios:

1. **Substance Over Hype**: Real code execution, real compilers, real tests, real Git commits.
2. **Preserve Codex Primitives**: Never discard upstream Codex tools, skills, or sandbox primitives.
3. **Control Plane Authoritative Truth**: The Rust control plane owns state, not ephemeral chat contexts.
4. **Deterministic Transitions**: LLMs are probabilistic; state machines and DAG scheduling are mathematically deterministic.
5. **Native Git**: All branching, worktree isolation, and diffs use standard Git mechanisms.
6. **Worktree Isolation**: Parallel mutating workers must execute in isolated Git worktrees.
7. **UI as Projection**: UI is a reactive projection over the event stream; it never owns state.
8. **Decoupled Provider and Runtime**: Provider is an API endpoint; runtime is an agent execution process.
9. **Strong Runtime Identity**: `ModelRef = (ProviderInstanceId, ModelId)`.
10. **Agent as Worker**: The user interacts with Agent Studios, not a loose collection of chatbots.
11. **Canonical Workflow**: `Goal -> Run -> Task Graph (DAG) -> Assigned Agents -> Execution -> Result`.
12. **Decoupled Agent and Task State**: Agent lifecycle (`Idle/Busy`) is distinct from Task state (`Ready/Running/Blocked`).
13. **Blocked != Cancelled**: Dependency or approval pauses are `Blocked`, never `Cancelled`.
14. **Safe Worktree Retention**: Never run `git reset --hard` or `git clean -fdx` on dirty worktrees.
15. **Thread-Worktree Affinity**: A thread's identity is permanently bound to its allocated worktree `cwd`.
16. **Control Plane Artifact Lineage**: Artifact versions ($v1, v2\dots$) are allocated authoritatively by the Control Plane.
17. **Agent Mode is Session-First**: Starts clean and conversation-centric, not as a complex IDE.
18. **IDE Mode is Editor-First**: Provides full Code-OSS capabilities for hands-on development.
19. **Zero-Loss Mode Switching**: Both modes share one active backend session without restart or data loss.
20. **One Activity Bar Icon**: Exactly ONE icon (`Agents`) added to Code-OSS Activity Bar.
21. **BYOK Neutrality**: Zero mandatory external account logins; user keys stored safely locally.
22. **First-Class Localization**: Native English and Vietnamese UI localization.
23. **Decoupled UI and Agent Language**: UI language does not force agent response language.
24. **Truthful Telemetry**: No fake progress bars; success requires real process exit code 0.
25. **Local-First Architecture**: Runs fully on developer host machine; no cloud daemon dependencies.
26. **Explicit Human Approvals**: Risky commands (filesystem, network, git) require typed approval gates.
27. **Quiet Visual Language**: Monochrome palette, zero neon borders, zero sci-fi animations.
28. **Windows-First Tier-1 Target**: Windows 11 x64 is first-class, supported by cross-platform Linux/macOS.
29. **Non-Destructive Rollback**: Failed tasks or aborted runs retain state for human recovery.
30. **Single-Writer Advisory Locks**: Concurrent writes to persistent databases or worktrees are strictly serialized.

---

## 6. Provenance Tag Audit

| Invariant / Concept | Provenance Tag | Source Document |
|:---|:---:|:---|
| Windows-First Tier-1 (MSVC x64) | `[USER_DIRECT_REQUIREMENT]` | `MASTER_VISION.md` §12 Rule 28 |
| English & Vietnamese Localization | `[USER_DIRECT_REQUIREMENT]` | `MASTER_VISION.md` §12 Rule 22 |
| Decouple UI Locale from Agent Language | `[USER_DIRECT_REQUIREMENT]` | `MASTER_VISION.md` §12 Rule 23 |
| BYOK Multi-Provider Neutrality | `[USER_DIRECT_REQUIREMENT]` | `MASTER_VISION.md` §12 Rule 21 |
| Dual Modes (Agent Mode / IDE Mode) | `[USER_DIRECT_REQUIREMENT]` | `MASTER_VISION.md` §3 |
| Single Activity Bar Icon (`Agents`) | `[USER_DIRECT_REQUIREMENT]` | `MASTER_VISION.md` §8 |
| Code-OSS as Permanent IDE Foundation | `[EXISTING_CANONICAL_DECISION]` | `MASTER_VISION.md` §2 Rule 7 |
| Codex (`codex-rs`) Core Runtime | `[EXISTING_CANONICAL_DECISION]` | `MASTER_VISION.md` §10 |
| Deterministic Rust Control Plane Actor | `[IMPLEMENTED_BEHAVIOR]` | `crates/control-plane` (M02) |
| Git Worktree Isolation & Ephemeral Index | `[IMPLEMENTED_BEHAVIOR]` | `crates/workspace` (M09) |
| Native Daemon (`agsd`) & IPC Transport | `[IMPLEMENTED_BEHAVIOR]` | `crates/ipc-transport` (M13) |
| Claude Code Runtime Adapter | `[IMPLEMENTED_BEHAVIOR]` | `crates/claude-code-runtime` (M12) |
| 9-Tab Mission Control Workbench | `[ASSISTANT_OR_CLAUDE_PROPOSAL]` | `MISSION_CONTROL_PRODUCT_SPEC.md` |
| Standalone `ags` CLI / TUI Specification | `[ASSISTANT_OR_CLAUDE_PROPOSAL]` | `AGS_CLI_TUI_SPEC.md` |
