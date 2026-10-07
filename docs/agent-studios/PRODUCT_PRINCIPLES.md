# Agent Studios — Product Principles & Architectural Ruleset

This document defines the 27 immutable architectural principles and rules governing Agent Studios. Every design decision, code modification, and roadmap milestone must adhere to these rules.

---

### 1. Preserve Codex Capability Instead of Reimplementing Simplified Substitutes
Agent Studios is built upon the open-source OpenAI Codex engine. We do not rewrite Codex from scratch. We inherit and preserve portable Codex primitives: the core agent loop, tool routing (`exec_command`, `apply_patch`), sandbox containment, approval workflows, context management, history compaction, interrupt/cancellation mechanics, MCP integration, Skills (`SKILL.md`), Plugins, hooks, `AGENTS.md`, subagents, and app-server concepts. When upstream capabilities depend on proprietary OpenAI infrastructure, we document the dependency, abstract the capability, and provide practical fallbacks rather than discarding the feature.

### 2. The Control Plane Owns Deterministic Truth
The Control Plane is the single source of authoritative state for studios, agents, tasks, runs, approvals, worktrees, artifacts, leases, and event sequences. Orchestration state must never exist solely in memory, in ephemeral chat transcripts, or inside an LLM's working context window. If the LLM hallucinates, crashes, or loses context, the entire run remains completely reconstructible from the Control Plane event log.

### 3. LLM Intelligence May Be Probabilistic; State Transitions May Not
Cognitive reasoning (goal decomposition, role selection, code generation, review heuristics) is inherently probabilistic. State transitions (task states, run states, approval states, event sequencing, DAG dependencies) must be strictly deterministic, typed, and mathematically validated. The state machine enforces hard invariants; the LLM merely proposes actions.

### 4. Git Remains Git
Agent Studios operates on real Git repositories using standard Git primitives: branches, commits, diffs, status, worktrees, merges, and patches. We never substitute Git with an internal proprietary database pretending to be version control. All code isolation and artifact promotion build directly upon native Git mechanisms.

### 5. Workspaces Isolate Parallel Mutation
Concurrent mutating workers must not operate in the same physical working tree. Parallel coding agents execute inside dedicated, isolated Git worktrees. This eliminates file contention, dirty write races, and studio-wide single-writer bottlenecks.

### 6. UI Is a Projection, Not Authoritative State
The user interface—whether Agent Mode or IDE Mode—is a reactive projection over the Control Plane and runtime event stream. The UI never owns orchestration state. State must never be reverse-engineered by scraping terminal output, parsing stdout streams, or extracting heuristics from chat strings when structured runtime events exist.

### 7. Provider and Runtime Are Separate Abstractions
- **Provider**: An LLM API vendor or wire protocol driver (e.g., OpenAI, Anthropic, Google Gemini, OpenRouter, Ollama). It defines model descriptors, endpoints, token limits, and wire formats.
- **Runtime**: An agent execution environment with process lifecycle, tool dispatch, sandboxing, and session state (e.g., Internal Codex Engine, OpenCode, Claude Code).
The Anthropic provider is not the Claude Code runtime; OpenAI is not Codex CLI. Never conflate the two.

### 8. Provider Instance + Model Defines Runtime Identity
Model slug or name alone is insufficient runtime identity. The canonical model target is strictly:
```rust
ModelRef {
    provider_instance_id: ProviderInstanceId,
    model_id: ModelId,
}
```
Two distinct instances (e.g., local Ollama vs. remote vLLM, or corporate 9Router vs. direct Anthropic) may expose identical model slugs like `claude-3-7-sonnet`. They represent distinct runtime targets with different credentials, endpoints, and latency profiles.

### 9. Agent Is a Worker, Not the Whole Product
Agents are implementation resources allocated to execute tasks. The user interacts primarily with Agent Studios and its cognitive Coordinator. The user must never be forced to become a manual project manager juggling multiple independent chatbot windows.

### 10. Canonical Product Workflow
Every operation flows through the canonical sequence:
$$\text{Goal} \longrightarrow \text{Run} \longrightarrow \text{Task Graph (DAG)} \longrightarrow \text{Assigned Agents} \longrightarrow \text{Execution} \longrightarrow \text{Result}$$
This pipeline governs all multi-agent execution across Agent Studios.

### 11. Agent State and Task State Are Semantically Distinct
- **Agent Lifecycle**: `Registered` $\rightarrow$ `Starting` $\rightarrow$ `Idle` $\leftrightarrow$ `Busy` $\rightarrow$ `Stopping` $\rightarrow$ `Stopped`.
  - Operational states: `Running`, `Waiting`, `Blocked`, `Retrying`, `Paused`, `Failed`, `Cancelled`.
- **Task Lifecycle**: `Pending` $\rightarrow$ `Blocked` $\leftrightarrow$ `Ready` $\rightarrow$ `Running` $\rightarrow$ `Succeeded` / `Failed` / `Cancelled`.
An agent may be `Idle` while its assigned task is `Blocked`. Never merge or conflate these lifecycles for UI convenience.

### 12. Blocked Is Not Cancelled
A task that cannot proceed due to an unresolved dependency, a patch reconciliation conflict, a waiting approval, or workspace lease contention is `Blocked`, not `Cancelled`. Cancellation is a terminal revocation of intent; blocked is a conditional pause awaiting resolution.

### 13. Dirty Worktrees Must Be Retained Safely
Managed worktrees containing uncommitted modifications, failed agent attempts, or unmerged artifacts must never be automatically destroyed. Destructive operations like `git reset --hard`, `git clean -fdx`, or `git worktree remove --force` are strictly forbidden during standard lifecycle cleanup. Failed or conflicted worktrees are retained for developer inspection and recovery.

### 14. Threads Do Not Teleport Between Worktrees
A Codex thread's execution identity is inextricably bound to its working directory (`cwd`) and allocated worktree checkout. Mutating a thread's working directory out-of-band is prohibited. Switching workspaces requires an orderly handoff: verify no active turn $\rightarrow$ shut down old child agent $\rightarrow$ retain workspace $\rightarrow$ spawn new child agent $\rightarrow$ bind new worktree.

### 15. Artifact Lineage Belongs to the Control Plane
The Control Plane authoritatively allocates artifact versions ($v1, v2, v3\dots$). Individual agents, executors, or external callers cannot assign arbitrary version numbers. Historical version lineages are immutable and content-addressed via SHA-256.

### 16. Agent Mode Is Agent-First and Session-First
The desktop application opens into an agent-centric, conversation-first workspace. It presents a clean, progressive interface focused on goals, models, and recent sessions. It does not initially open into a full IDE or a sprawling monitoring dashboard.

### 17. IDE Mode Is Editor-First
IDE Mode provides a complete, Code-OSS-based professional development environment (editor, explorer, search, git, debugger, terminal, LSP, extensions). It is designed for hands-on software engineering alongside AI agents.

### 18. Both Modes Share One Underlying Session
Agent Mode and IDE Mode are two presentation surfaces over **one shared backend session**. They share session ID, working directory, task graph, agents, tool history, approvals, worktrees, and context. Toggling between modes incurs zero context loss, zero agent restarts, and zero process teleportation.

### 19. Code-OSS Is the Permanent IDE Foundation
We build upon Code-OSS for IDE capabilities. We do not attempt to construct a fragile Monaco Editor or Tauri clone of VS Code from scratch.

### 20. Single Primary Activity Bar Entry for Agent Studios
In Code-OSS, Agent Studios integrates natively by adding exactly **one** primary entry to the Activity Bar: `Agents`. We do not pollute the Activity Bar with separate buttons for Tasks, Runs, Artifacts, Worktrees, Approvals, or Providers; these concepts live inside the cohesive Agent Studios sidebar and editor panels.

### 21. No Mandatory Proprietary Login
Agent Studios functions fully without requiring an OpenAI, ChatGPT, or Microsoft account. Users can bring their own API keys (BYOK) or connect local runtimes (Ollama, LM Studio, vLLM). Account-based integrations may be supported optionally, but open provider access is fundamental.

### 22. Security Must Be Enforced at Actual Execution Boundaries
Permissions, sandboxing, and budgets are enforced at real execution gates, not merely as UI warnings or scheduler metadata. A tool call denied by budget or approval must never reach the tool handler. Secrets are resolved in-memory and zeroized on drop; plaintext secrets never appear in logs, events, or diagnostics.

### 23. Durable Events Must Support Deterministic Reconstruction
All domain events are stored sequentially with monotonic sequence numbers. Replaying the event stream must reconstruct an identical in-memory domain state without duplicate side-effects.

### 24. Avoid Unnecessary Upstream Codex Patches
Modifications to `codex-rs/` must remain minimal, generic, provider-neutral, default-off, and strictly backwards-compatible. Wherever possible, Agent Studios logic resides in additive crates (`agent-studios-rs/crates/`). Every patch must be registered in `CODEX_PATCHES.md`.

### 25. Windows-First with a Portable Core
Agent Studios prioritizes Windows 11 x64 (MSVC) as its tier-1 development, testing, and release platform, while maintaining portable Rust crate architecture for future macOS and Linux distributions.

### 26. English and Vietnamese Are First-Class UI Languages
Localization is an architectural requirement, not an afterthought. The user interface provides complete, first-class localization in English and Vietnamese from initial desktop milestones. Display language is strictly decoupled from agent conversational response language.

### 27. Useful / Native / Quiet Visual Language
The visual design of Agent Studios adheres to a quiet, monochrome, high-contrast palette. We explicitly reject neon borders, glowing "AI" gradients, cyberpunk aesthetic tropes, and busy monitoring-dashboard clutter. Agent Studios feels like a fast, focused, professional developer tool.
