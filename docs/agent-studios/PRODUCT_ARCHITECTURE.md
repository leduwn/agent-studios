# Agent Studios — Product Architecture

> **Status**: Core Architecture Specification
> **Precedence**: Subservient to `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. High-Level Architectural Topology

Agent Studios cleanly decouples user experience presentation, orchestration control, execution runtimes, and model provider transport:

```text
                    AGENT STUDIOS
                         │
              ┌──────────┴──────────┐
              │                     │
          AGENT MODE             IDE MODE
      (Agent-First UX)     (Code-OSS Workbench)
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

---

## 2. Desktop Process & IPC Topology

The desktop application is architected around an isolated client-server IPC model:

```text
┌────────────────────────────────────────────────────────┐
│               Code-OSS Desktop Workbench               │
│                                                        │
│   ┌─────────────────────┐    ┌─────────────────────┐   │
│   │     Agent Mode      │    │      IDE Mode       │   │
│   │ (Agent Studios View)│    │ (Full Editor Shell) │   │
│   └──────────┬──────────┘    └──────────┬──────────┘   │
└──────────────┼──────────────────────────┼──────────────┘
               │                          │
               └────────────┬─────────────┘
                            │ Native IPC (JSON-RPC / Named Pipes)
                            ▼
┌────────────────────────────────────────────────────────┐
│             Agent Studios App Server                   │
│                                                        │
│   ┌────────────────────────────────────────────────┐   │
│   │        Deterministic Control Plane             │   │
│   │    - TaskGraph DAG       - EventStore (Replay) │   │
│   │    - Worktree Lifecycle  - Artifact Lineage    │   │
│   │    - Budget Tracker      - Approval Manager    │   │
│   └───────────────────────┬────────────────────────┘   │
│                           │                            │
│   ┌───────────────────────┴────────────────────────┐   │
│   │              Execution Runtimes                │   │
│   │  ┌────────────────────┐ ┌────────────────────┐ │   │
│   │  │ Codex Engine Core  │ │  External Runtimes │ │   │
│   │  │ (Threads, Sandbox) │ │(OpenCode, Claude)  │ │   │
│   │  └─────────┬──────────┘ └────────────────────┘ │   │
│   └────────────┼───────────────────────────────────┘   │
│                ▼                                       │
│   ┌────────────────────────────────────────────────┐   │
│   │            Provider Transport Layer            │   │
│   │   - ProviderCatalog     - Protocol Adapters    │   │
│   │   - RuntimeRouter       - Secret Resolvers     │   │
│   └────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────┘
```

The UI is strictly a reactive presentation client; it never directly mutates or bypasses authoritative Control Plane state.

---

## 3. Shared Session Semantics

Agent Mode and IDE Mode operate on the **exact same underlying runtime session**:

| Shared Context Field | Semantic Definition |
| :--- | :--- |
| `session_id` | Globally unique identifier (UUID v4) for the active user collaboration session. |
| `cwd` / `source_root` | Physical Git repository root and active project directory. |
| `run` | Current active run instance tracking turn counts, elapsed time, and token expenditures. |
| `task_graph` | The canonical DAG of completed, running, ready, and blocked tasks. |
| `agents` | Registered cognitive agents (Coordinator, Coder, Reviewer, Tester) with assigned models. |
| `tool_history` | Complete historical audit trail of shell executions, file edits, and outputs. |
| `approvals` | Pending, approved, and denied human-in-the-loop permission requests. |
| `provider/model` | Active provider instance configurations and selected `ModelRef` mappings. |
| `worktrees` | Allocated physical Git worktrees bound to active worker threads. |
| `artifacts` | Content-addressed patch diffs and generated deliverables. |
| `context` | AGENTS.md instructions, active skills, and context compaction state. |

**Invariance Guarantee**: Toggling UI layouts between Agent Mode and IDE Mode is purely a visual layout switch. It does not restart threads, reset context, drop worktrees, or cancel active tasks.

---

## 4. Repository Crate Architecture

The backend implementation resides in `agent-studios-rs/crates/`, cleanly layered and decoupled from upstream Codex crates:

```text
agent-studios-rs/
├── Cargo.toml
└── crates/
    ├── protocol/                [Implemented] Pure domain types, IDs, enums, events
    ├── control-plane/           [Implemented] Deterministic DAG, transactional state, event replay
    ├── provider/                [Implemented] Provider registry, ModelRef, zero-secret metadata
    ├── codex-bridge/            [Implemented] Bridges ProviderCatalog to Codex ModelProviderInfo
    ├── protocol-adapters/       [Implemented] Wire translation (Chat Completions, Anthropic, Gemini)
    ├── runtime-transport/       [Implemented] Async SSE transport, secret resolution, RuntimeRouter
    ├── runtime-session/         [Implemented] Thread session factory, runtime overrides, static models
    ├── internal-agent/          [Implemented] Multi-agent supervisor, DAG planning, tool budgets
    ├── orchestration/           [Implemented] Read models, projections, sequence trackers
    └── workspace/               [Under Review in M09] Git worktree isolation, artifact store, patch reconciliation
```

### Verified Crate Roles

1. **`agent-studios-protocol`**: Pure domain data structures. Strong ID newtypes (`StudioId`, `TaskId`, `AgentId`, `RunId`, `ApprovalId`, `WorktreeId`, `ArtifactId`), state enums, transition invariants, and immutable `ControlPlaneEvent` envelopes.
2. **`agent-studios-control-plane`**: Deterministic state kernel. Thread-safe single-writer actor (`ControlPlaneActor`), transactional state staging (`commit_transaction`), iterative Kahn's algorithm DAG cycle detection, hierarchical cancellation, and strict event replay engine.
3. **`agent-studios-provider`**: Provider registry and model catalog. Defines `ProviderDefinition`, `ProviderInstance`, `ProtocolFamily`, zero-secret `SecretReference`, capability tristate (`Supported`, `Unsupported`, `Unknown`), and composite `ModelRef(ProviderInstanceId, ModelId)`.
4. **`agent-studios-codex-bridge`**: Non-invasive bridge mapping `ProviderCatalog` and `ModelRef` into Codex `ModelProviderInfo` for native OpenAI Responses API routing.
5. **`agent-studios-protocol-adapters`**: Pure in-memory protocol translation between Codex Responses API semantics and external wire specifications (OpenAI Chat Completions, Anthropic Messages, Google Gemini generateContent). Zero network, zero secrets.
6. **`agent-studios-runtime-transport`**: Asynchronous HTTP transport, incremental SSE decoding, in-memory/environment secret resolvers, and `RuntimeRouter` implementing Codex `ModelInferenceBackend`.
7. **`agent-studios-runtime-session`**: Assembles `PreparedRuntimeSession` and thread start options, decoupling custom providers from mandatory OpenAI logins and synthetic discovery.
8. **`agent-studios-internal-agent`**: Multi-agent team supervisor (`AgentStudiosSupervisor`), structured coordinator DAG planning, workspace access arbitrator, pre-execution tool budget contributor, and real execution backends.
9. **`agent-studios-orchestration`**: Projection layer building read models (`TaskGraphSnapshot`, `AgentSummary`, `RunSummary`, `WorktreeSnapshot`, `ArtifactIndex`) from raw control plane event streams.
10. **`agent-studios-workspace`** *(M09 Branch - Under Final Review)*: Unified workspace orchestrator wrapping upstream `codex-worktree::WorktreeManager`, safe change capture via ephemeral Git index, content-addressed artifact store, and patch reconciliation.

---

## 5. Subsystem Decoupling & Status Index

| Subsystem | Architectural Role | Status |
| :--- | :--- | :--- |
| **Control Plane** | Deterministic state machine, DAG, event replay | Implemented |
| **Provider Core** | Provider registry, model catalog, zero-secret refs | Implemented |
| **Protocol Adapters** | In-memory wire protocol translation | Implemented |
| **Runtime Transport** | SSE streaming, secret resolution, transport drivers | Implemented |
| **Runtime Session** | Thread injection, runtime overrides | Implemented |
| **Internal Multi-Agent** | Real Codex multi-agent tree, supervisor, budgets | Implemented |
| **Workspace & Worktrees** | Isolated Git worktrees, change capture, artifacts | Under Final Review (M09.3) |
| **External Runtime API**| Standardized interface for non-Codex agents | Planned (M10) |
| **OpenCode Adapter** | External runtime adapter for OpenCode | Planned (M11) |
| **Claude Code Adapter** | External runtime adapter for Claude Code | Planned (M12) |
| **Code-OSS Shell** | Professional IDE workbench integration | Planned (M13) |
| **Built-in AI Extension**| Native Code-OSS chat & composer extension | Planned (M14) |
| **Unified Workbench** | Dual Agent Mode / IDE Mode layout switcher | Planned (M15) |
| **Full Codex Surface** | Skills, MCP, Plugins, hooks, AGENTS.md | Planned (M16) |
| **Packaging & Installer**| Windows native MSI/NSIS installers | Planned (M17) |
