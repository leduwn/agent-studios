# Agent Studios Architecture Specification

> **Precedence Note**: This subsystem specification is governed by [`MASTER_VISION.md`](MASTER_VISION.md) and [`PRODUCT_PRINCIPLES.md`](PRODUCT_PRINCIPLES.md). In the event of any discrepancy, `MASTER_VISION.md` is authoritative.

## 1. Product Overview

- **Name**: Agent Studios
- **Type**: Open-source AI coding-agent desktop platform built upon the open-source OpenAI Codex engine.
- **Platform Strategy**:
  - **Windows x64 First**: Primary development, testing, and initial stabilization target.
  - **macOS / Linux**: Planned for subsequent milestones after Windows native stability is validated.

Agent Studios does not rewrite Codex from scratch. It builds upon Codex's proven execution engine while expanding multi-agent orchestration, multi-runtime interoperability, vendor-neutral LLM connectivity, and dual desktop user experiences.

---

## 2. Dual User Experience (UX) Modes

The desktop client provides two complementary workflows tailored to distinct development contexts:

### 2.1. Agent Mode
A streamlined, task-oriented conversational workspace designed for high-level direction, delegation, and autonomous execution. This mode avoids copying proprietary Codex desktop branding or visual assets.

Core capabilities:
- **Conversation**: Contextual interaction with orchestrator and subagents.
- **Task Progress**: Visual milestone and execution step tracker.
- **Tool Activity**: Real-time display of tool invocations, inputs, and results.
- **Approvals**: Explicit interactive permission dialogs for commands, edits, and network actions.
- **Diff View**: Interactive patch inspection prior to workspace application.
- **Terminal Activity**: Live command output streaming and interactive sessions.
- **Agent Activity**: Subagent status, communication traces, and resource consumption.
- **Artifacts**: Management of created files, plans, summaries, and test logs.

### 2.2. IDE Mode (Code-OSS Foundation)
A comprehensive code editor workspace for full-scale software development, inspection, and direct authoring alongside AI assistance.

Rather than implementing a custom, fragile Tauri + Monaco recreation of VS Code, the desktop IDE foundation is built on **Code-OSS**. This guarantees production-grade VS Code-class stability across:
- Editor, Explorer, Search, Source Control
- Integrated terminal, Debugging, Problems, Output
- Keybindings, Themes, Extensions, Language services (LSP)
- Tabs, Editor groups, Command Palette, and Workspace behavior

### 2.3. Unified Runtime & Session Model
IDE Mode and Agent Mode are two views of the **same underlying runtime session**:

```text
IDE Mode ───┐
            ├── Agent Studios Session (Shared State)
Agent Mode ─┘
                     |
                     v
             Codex Runtime Core
```

- **Shared State**: Both modes share the same session ID, working directory, tool execution history, interactive approvals, tasks, subagents, provider/model configuration, and Git worktree states.
- **Switching Layouts Without State Loss**: Users can switch between IDE layout and Agent-focused layout at any time without resetting context or aborting active agent tasks.
- **Default AI Surface**: The built-in AI extension replaces the surface GitHub Copilot would normally occupy. Users do NOT need to type `@agentstudios` for normal usage; chat and composer route directly to the Agent Studios Codex runtime.
- **Parity of Operations**: IDE Chat is not a reduced question-answer bot; it possesses full Agent Mode capabilities (inspect/edit files, `apply_patch`, execute commands in terminal, run tests/builds, handle approvals, inspect diffs, trigger MCP tools/Skills/Plugins/hooks, manage subagents/worktrees, and interrupt/resume execution).

### 2.4. Visual Direction & Branding Boundaries
- **Monochrome Palette**: Default visual styling emphasizes high-contrast monochrome (white / black / neutral grays) without gratuitous gradient-heavy "AI branding".
- **Workbench Familiarity**: Preserves standard Code-OSS / VS Code layout, hierarchy, and icon familiarity (standard Codicons remain intact for editor/file explorer/debugger).
- **Branding Isolation**:
  - Only Agent Studios-specific panels and surfaces receive custom iconography.
  - Zero proprietary Codex desktop branding or visual assets.
  - Zero GitHub Copilot logos or Microsoft Visual Studio Code trademark assets.
  - Icon design is intentionally deferred to Milestone M13 (no icons designed during protocol adapter milestones).

---

## 3. High-Level Architecture

Agent Studios decouples the presentation layer, the orchestration control plane, runtime engines, and provider protocol drivers:

```text
Agent Studios Desktop
        |
        v
Agent Studios Control Plane
        |
        +------------------------------+
        |                              |
        v                              v
Codex-derived Runtime           External Runtime Adapters
        |                              |
        |                         Claude Code
        |                         OpenCode
        |                         Codex CLI
        |                         future runtimes
        |
        v
Provider Abstraction
        |
        +-- OpenAI
        +-- Anthropic
        +-- Google
        +-- OpenRouter
        +-- DeepSeek
        +-- Groq
        +-- 9Router
        +-- Ollama
        +-- LM Studio
        +-- custom compatible providers
```

### Architectural Guarantees
- **Snapshot-Based Upstream Synchronization**: Agent Studios synchronizes with upstream Codex via clean snapshot-based diff porting rather than preserving direct Git commit ancestry. The core design minimizes invasive changes to Codex-derived crates and keeps Agent Studios additions modular, making upstream diffs straightforward to analyze, test, and port cleanly without claiming direct Git merges will always succeed automatically.
- **Configurable Endpoints**: Providers like 9Router, OpenRouter, or local backends (Ollama, LM Studio) are first-class configurable endpoints; no single proxy or provider is hardcoded as an architectural center.
- **Additive Design**: Agent Studios functionality resides in standalone crates and modules layered on top of or alongside Codex crates.

---

## 4. Provider Architecture & Protocol Adapters

Agent Studios does not confine its core to the OpenAI Responses API. It introduces a modular Provider Abstraction and Protocol Adapter layer.

### 4.1. Supported Protocols
The engine communicates across heterogeneous provider APIs via dedicated protocol adapters:
- **OpenAI Responses API**
- **OpenAI Chat Completions API**
- **Anthropic Messages API**
- **Google Gemini generateContent API**
- **OpenAI-Compatible Custom Protocols** (Local runtimes, proxy servers, custom LLM gateways)

### 4.2. Capability-Based Registry
Hardcoded conditionals such as `if provider == "openai"` are strictly prohibited throughout the internal engine. Connectivity is mediated by a centralized Provider Registry exposing declarative Model Capabilities:

- **tool calling**: Function/tool invocation mechanics.
- **parallel tools**: Simultaneous multi-tool call dispatch.
- **vision**: Image input handling and multimodal analysis.
- **reasoning**: Extended thinking/reasoning effort configuration.
- **streaming**: Incremental token and event delivery.
- **structured output**: JSON schema enforcement and constrained grammar decoding.
- **prompt caching**: Prefix cache breakpoints and token reduction.
- **web search**: Provider-native web search grounding.
- **audio**: Voice input/output processing.
- **context size**: Maximum context window token limits.
- **max output**: Maximum generation token ceilings.


---

## 5. Multi-Agent Architecture

Agent Studios supports a hybrid multi-agent topology combining native internal agents with external agent runtimes.

### 5.1. Internal Agents
Agents running directly on the Agent Studios Codex-derived runtime. Each internal agent can be independently configured with:
- **Provider & Model**: Model assignment tailored to the agent's role complexity.
- **Reasoning Effort**: Low, medium, or high deliberation depth.
- **Role**: Specialized system guidelines and prompt behavior.
- **Skills**: Domain-specific skill packs and workflows.
- **Plugins & MCP Servers**: Dynamic capabilities and context tools.
- **Sandbox & Approval Policies**: Granular command and filesystem permission boundaries.
- **Budget**: Token expenditure and execution time limits.
- **Working Directory / Worktree**: Dedicated physical workspace isolation.

### 5.2. External Runtime Agents
Agents orchestrated through standardized runtime adapters wrapping standalone external CLI tools (e.g., Claude Code, OpenCode, Codex CLI, and future agent runtimes).

External agents are not hardcoded into the orchestrator. They conform to an `AgentRuntime` lifecycle interface:
- `discover`: Locate installed runtime binaries and query platform capabilities.
- `start`: Launch an agent session with environment and sandbox constraints.
- `send`: Deliver messages, instructions, or stdin commands.
- `interrupt`: Gracefully pause active generation or command execution.
- `resume`: Continue suspended execution states.
- `stop`: Terminate the runtime process cleanly.
- `status`: Query live process health and runtime state.
- `events`: Stream structured events (output tokens, tool calls, status updates).
- `capabilities`: Inspect runtime-supported feature sets.

---

## 6. Deterministic Orchestration vs. LLM Reasoning

Agent Studios maintains a strict boundary between non-deterministic LLM reasoning and deterministic orchestration state:

| LLM Manager (Reasoning) | Control Plane (Deterministic State) |
|---|---|
| Task decomposition & subtask design | Unique Task IDs & DAG dependency tracking |
| Worker selection & specialization | State machine transitions (Pending, Running, Paused, Succeeded, Failed) |
| Output verification & quality evaluation | Agent process lifecycle management & process supervision |
| Next-step heuristics & decision making | Retries, exponential backoffs, and execution timeouts |
| Synthesizing cross-agent summaries | Token & financial budget enforcement |
| | Concurrency locks & resource arbitration |
| | Git worktree lifecycle & branch tracking |
| | Artifact versioning & content hashing |
| | Event persistence, replay, & audit logging |
| | Cancellation propagation across worker trees |

Multi-agent coordination state MUST NOT reside exclusively within the LLM's working context window. All task graphs, execution logs, and state transitions are durably recorded in the Control Plane.

---

## 7. Workspace Isolation via Git Worktrees

To eliminate race conditions and dirty writes across concurrent agents, Agent Studios employs Git worktrees for workspace isolation:

```text
Project Repository
    |
    +-- coordinator / main workspace (Primary workspace / user view)
    |
    +-- agent / frontend worktree    (Isolated branch: worktree/agent-frontend-*)
    |
    +-- agent / backend worktree     (Isolated branch: worktree/agent-backend-*)
    |
    +-- agent / reviewer worktree    (Read-only or test evaluation worktree)
```

- Worker agents operate strictly inside their assigned worktrees.
- Concurrent workers do not modify the user's primary working tree directly.
- Merge, cherry-pick, conflict detection, and reconciliation workflows are governed deterministically by the Control Plane.
- Existing upstream Codex worktree capabilities will be audited and leveraged prior to building custom implementations.


---

## 8. Plugin & Skill Compatibility

Agent Studios prioritizes high compatibility with portable Codex standards:
- **`AGENTS.md`**: Custom agent instructions and subagent definitions.
- **`SKILL.md`**: Declarative skill packs and contextual workflows.
- **Model Context Protocol (MCP)**: Full support for external MCP servers over stdio and SSE.
- **Hooks**: Execution lifecycle hooks for pre-commit, pre-tool, and post-task automation.
- **Codex Portable Plugins**: Plugin manifests and configuration specifications.
- **MCP Apps & UI**: Compatible client UI rendering for MCP interactive views where portable.

*Note*: Agent Studios does not emulate OpenAI-hosted proprietary backend cloud services.

---

## 9. Secret Management

- **Zero Plaintext Secrets**: API keys, access tokens, and credentials MUST NOT be stored in plaintext within project configurations or source repositories.
- **OS Credential Storage**: On Windows, secrets are stored using the Windows Credential Manager (or equivalent secure OS keychain abstractions).
- **Reference-Based Configuration**: Workspace configuration files only store opaque credential identifiers or environment variable references.

---

## 10. Reference Runtimes & Third-Party Code

- **OpenCode Reference**: OpenCode is studied strictly as an architectural reference for provider abstraction, API compatibility patterns, UX ergonomics, and runtime interoperability.
- **Core Independence**: Agent Studios agent reasoning and tool execution run exclusively on the Codex-derived engine.
- **Clean Repository**: OpenCode source code is not included in the repository.

