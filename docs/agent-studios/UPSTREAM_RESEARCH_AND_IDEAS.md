# Agent Studios — Upstream Research, Inspirations & Ideas

> **Status**: Authoritative Upstream Research & Synthesis Specification  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Reference**: Implements `UPSTREAM.md`, `UPSTREAM_REFERENCES.md`, `CODEX_PATCHES.md`, and Subagent C Findings

---

## 1. Upstream Architectural Heritage

`[UPSTREAM_INSPIRED_IDEA]` & `[EXISTING_CANONICAL_DECISION]`
Agent Studios is not built in isolation; it stands on the shoulders of the open-source developer tooling ecosystem. Rather than reinventing proven primitives, Agent Studios synthesizes the best ideas from four major upstream projects into a cohesive, deterministic developer environment:

```text
┌───────────────────────────┐      ┌───────────────────────────┐
│       OPENAI CODEX        │      │         OPENCODE          │
│   (codex-rs / app-server) │      │       (opencode-ai)       │
├───────────────────────────┤      ├───────────────────────────┤
│ • Core agent execution    │      │ • Universal BYOK catalog  │
│ • exec_command & patch    │      │ • Desktop UI ergonomics   │
│ • Sandboxing & approvals  │      │ • Local model routing     │
│ • MCP & Skills (SKILL.md) │      │ • Session step structures │
│ • Hierarchical AGENTS.md  │      │                           │
└─────────────┬─────────────┘      └─────────────┬─────────────┘
              │                                  │
              └─────────────────┬────────────────┘
                                │
                                ▼
                    ┌───────────────────────┐
                    │     AGENT STUDIOS     │
                    │   Deterministic DAG   │
                    │   Git Worktrees       │
                    │   Code-OSS IDE        │
                    │   Shared Sessions     │
                    └───────────┬───────────┘
                                │
              ┌─────────────────┴────────────────┐
              │                                  │
              ▼                                  ▼
┌───────────────────────────┐      ┌───────────────────────────┐
│    ANTHROPIC CLAUDE CODE  │      │    VS CODE / CODE-OSS     │
│       (@anthropic-ai)     │      │   (Microsoft / Eclipse)   │
├───────────────────────────┤      ├───────────────────────────┤
│ • Subagent worker loops   │      │ • Monaco editor & LSP/DAP │
│ • 2PC stdin approvals     │      │ • Source control diffing  │
│ • Strict budget watchdogs │      │ • Integrated terminal     │
│ • Scoped CLAUDE.md files  │      │ • Single Activity Bar icon│
└───────────────────────────┘      └───────────────────────────┘
```

---

## 2. Deep Dive: Upstream Systems Analysis

### 2.1 OpenAI Codex (`codex-rs`)
- **Primary Architectural Role**: Core internal execution engine.
- **Inherited Primitives**:
  - Agent execution loop and tool dispatch (`exec_command`, `apply_patch`).
  - Context compaction and history pruning algorithms.
  - Model Context Protocol (MCP) native client (`codex-mcp`).
  - Skills architecture (`SKILL.md`) and progressive loading.
  - Hierarchical instruction discovery (`AGENTS.md`).
- **Agent Studios Enhancements**:
  - Replaces OpenAI-exclusive cloud routing with multi-provider neutrality (Patches 001–004).
  - Adds physical Git worktree isolation for concurrent mutating agents.
  - Adds deterministic Kahn DAG task graph scheduling.

### 2.2 OpenCode (`opencode-ai`)
- **Primary Architectural Role**: Model openness and desktop ergonomics benchmark.
- **Inherited Concepts**:
  - Bring-Your-Own-Key (BYOK) multi-provider openness without mandatory subscriptions.
  - Local model auto-detection and execution (Ollama, LM Studio, vLLM).
  - Clean desktop presentation of agent steps.
- **Agent Studios Enhancements**:
  - Adds native Git worktree isolation (OpenCode runs in the main workspace directory).
  - Integrates full Code-OSS IDE capabilities rather than a standalone chat shell.

### 2.3 Anthropic Claude Code (`@anthropic-ai/claude-code`)
- **Primary Architectural Role**: Specialized CLI subprocess harness and approval protocol.
- **Inherited Concepts**:
  - Subagent worker loops (`--forward-subagent-text`).
  - Hierarchical instruction discovery (`CLAUDE.md`).
  - Pre-execution budget caps (`--max-budget-usd`).
- **Agent Studios Enhancements**:
  - Two-Phase Commit (2PC) stdin approval delivery protocol (M12.3) ensuring fail-closed termination on pipe write failure.
  - Win32 Job Object sandbox (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) ensuring zero orphaned CLI processes.

### 2.4 VS Code / Code-OSS
- **Primary Architectural Role**: Permanent IDE foundation.
- **Inherited Capabilities**:
  - Monaco editor with full language servers (LSP) and debug adapters (DAP).
  - Native Git staging, gutter decorations, and side-by-side diff views.
  - Integrated terminal (xterm.js).
- **Agent Studios Enhancements**:
  - Strict Activity Bar Rule: Consolidates all AI surfaces into a single `Agents` icon to avoid UI sprawl.
  - Native 9-tab Mission Control observability workbench webview.
  - Bidirectional local IPC bridge to native `agsd` daemon.

---

## 3. Upstream Comparison & Synthesis Matrix

| Architectural Feature | OpenAI Codex (`codex-rs`) | OpenCode (`opencode-ai`) | Anthropic Claude Code | Code-OSS (VS Code) | Agent Studios Synthesis |
|---|---|---|---|---|---|
| **Core Nature** | CLI & App Server | Go / Node CLI & Daemon | Node.js CLI Tool | Desktop IDE Workbench | Unified IDE + Daemon + CLI |
| **Engine Language** | Rust | Go / TypeScript | TypeScript | TypeScript / Electron | Rust (`agsd`) + TypeScript UI |
| **Multi-Agent Orchestration** | Thread control trees | Single agent loop | Subagent CLI flags | None | Deterministic Kahn DAG actor |
| **Workspace Isolation** | Basic worktree hook | Single working directory | Single working directory | Single workspace folder | Dedicated physical Git worktrees |
| **Patch Reconciliation** | Direct file writes | Direct file writes | Direct file writes | Manual Git staging | Lossless diff + 3-way reconcile |
| **Provider Support** | OpenAI-centric | Broad multi-provider | Anthropic-exclusive | Extension-dependent | Universal BYOK + dynamic discovery |
| **Process Sandboxing** | Platform tokens | Subprocess | Subprocess | Extension host sandbox | Win32 Job Objects & setpgid |
| **Interactive Approvals** | Stdio prompt | HTTP API prompt | Stdio prompt | VS Code modal dialog | 2PC stdin + VS Code modal |
| **UI Presentation** | Terminal / Web | Custom Electron shell | Terminal CLI | Full Monaco IDE | Dual Mode: Agent Mode + IDE Mode |
| **Extensibility** | MCP, SKILL.md, AGENTS.md| Custom plugins | CLAUDE.md, MCP | VS Code extensions | MCP, SKILL.md, AGENTS.md, Extensions |
| **Activity Bar Impact** | N/A | N/A | N/A | N/A | Exactly ONE icon (`Agents`) |
