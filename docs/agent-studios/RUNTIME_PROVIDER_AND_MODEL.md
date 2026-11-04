# Agent Studios — Runtime, Provider & Model Ecosystem Specification

> **Status**: Authoritative Runtime & Provider Specification  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Reference**: Implements `MASTER_VISION.md` §10, `PRODUCT_PRINCIPLES.md` Rules 7–8, `PROVIDERS.md`, `EXTERNAL_RUNTIMES.md`, and `RUNTIME_PROVIDER_TRANSPORT.md`

---

## 1. Core Abstraction: Decoupling Provider from Runtime

`[EXISTING_CANONICAL_DECISION]`
Agent Studios strictly separates the concept of an LLM API wire protocol (**Provider**) from the execution environment that runs tools and manages processes (**Runtime**):

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        PROVIDER ABSTRACTION                            │
│                 (crates/provider, protocol-adapters)                   │
├────────────────────────────────────────────────────────────────────────┤
│  - Wire protocol formatting (OpenAI, Anthropic, Gemini, Ollama)       │
│  - SSE stream decoding & token usage accounting                        │
│  - HTTP client transport, backoff, and retry handling                  │
│  - Endpoint authentication & in-memory secret zeroization              │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ ModelInferenceBackend
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                        RUNTIME ABSTRACTION                             │
│         (crates/internal-agent, opencode-runtime, claude-code-runtime) │
├────────────────────────────────────────────────────────────────────────┤
│  - Subprocess lifecycle management (Spawn, Stdin/Stdout, Kill)         │
│  - OS-level sandboxing (Win32 Job Objects, Linux process groups)       │
│  - Tool execution & approval gates (exec_command, apply_patch)         │
│  - Context history compaction, AGENTS.md, and SKILL.md discovery       │
└────────────────────────────────────────────────────────────────────────┘
```

The Anthropic provider is an API endpoint; the Claude Code runtime is an execution process. OpenAI is an API endpoint; Codex is an execution engine. They must never be conflated.

---

## 2. Strong Runtime Model Identity: `ModelRef`

`[EXISTING_CANONICAL_DECISION]`
A bare model string (such as `claude-3-7-sonnet` or `gpt-4o`) is insufficient runtime identity because identical slugs may exist across disparate infrastructure endpoints (e.g. direct cloud vs. local Ollama vs. corporate gateway).

The canonical model target across all Agent Studios crates is strictly defined as:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelRef {
    pub provider_instance_id: ProviderInstanceId,
    pub model_id: ModelId,
}
```

This ensures unambiguous routing to the exact configured credentials, base URL, rate limits, and latency profiles.

---

## 3. Supported Providers & Wire Protocols

`[IMPLEMENTED_BEHAVIOR]`
Agent Studios implements native adapters across four primary wire protocol families:

| Provider Protocol | Protocol Adapter Crate | Supported Target Endpoints |
|---|---|---|
| **Anthropic Messages** | `crates/protocol-adapters` (`anthropic`) | Claude 3.5 Sonnet, Claude 3.7 Sonnet, Claude 3.5 Haiku, Claude Opus |
| **OpenAI Chat Completions**| `crates/protocol-adapters` (`chat_completions`)| GPT-4o, GPT-4o-mini, o1, o3-mini, DeepSeek V3/R1, Groq, Together, OpenRouter |
| **Google Gemini** | `crates/protocol-adapters` (`gemini`) | Gemini 1.5 Pro, Gemini 1.5 Flash, Gemini 2.0 Flash |
| **Local / Custom HTTP** | `crates/protocol-adapters` (`ollama`) | Ollama, vLLM, LocalAI, LM Studio |

### Zero Secret Leakage Invariant
- `[IMPLEMENTED_BEHAVIOR]` API keys and bearer tokens are stored in-memory using zeroizing types (`SecretString`).
- `SecretReference` identifiers resolve secrets dynamically in memory immediately before wire transmission.
- Secret keys are stripped via regex-free token boundary scrubbing before emission to event logs, telemetry, or diagnostic channels.

---

## 4. External Agent Runtimes

Agent Studios supports multiple agent runtime backends via a unified adapter contract:

### 4.1 Internal Codex Engine (`crates/internal-agent`)
- `[IMPLEMENTED_BEHAVIOR]` Built directly on upstream `codex-rs` thread primitives.
- Provides deep internal tool execution (`exec_command`, `apply_patch`), workspace leases, pre-execution tool budgets, and subagent spawning via `AgentControl`.

### 4.2 Claude Code Runtime Adapter (`crates/claude-code-runtime`)
- `[IMPLEMENTED_BEHAVIOR]` Integrated in Milestone M12 / PR #15 (`3207b7462f`).
- Wraps the Anthropic Claude Code CLI subprocess.
- Maps Claude Code tool loops and streaming outputs into canonical Control Plane events.
- Employs a Two-Phase Commit (2PC) stdin protocol for interactive approval delivery.
- Enforces OS sandboxing via Win32 Job Objects (`#[cfg(windows)] JobObjectGuard`) ensuring no orphaned background processes.

### 4.3 OpenCode Runtime Adapter (`crates/opencode-runtime`)
- `[IMPLEMENTED_BEHAVIOR]` Integrated in Milestone M11 / PR #14 (`dc363d1e89`).
- Interfaces with the OpenCode runtime daemon.
- Maps OpenCode session steps to Control Plane `Task` and `Run` entities.

---

## 5. Tristate Capability Profiles & Lifecycle States

`[IMPLEMENTED_BEHAVIOR]`
Each runtime instance advertises its capabilities across 16 dimensions using a strict tristate enum (`Supported`, `Unsupported`, `Unknown`):

```rust
pub enum CapabilityStatus {
    Supported,
    Unsupported,
    Unknown,
}
```

Key dimensions evaluated:
1. `SubagentSpawning`
2. `ToolExecution`
3. `FilePatching`
4. `InteractiveApprovals`
5. `ContextCompaction`
6. `MonotonicEventReplay`
7. `OSProcessSandboxing`
8. `WorktreeIsolation`

Runtime instances strictly adhere to the lifecycle state machine:
$$\text{Registered} \longrightarrow \text{Starting} \longrightarrow \text{Idle} \longleftrightarrow \text{Busy} \longrightarrow \text{Stopping} \longrightarrow \text{Stopped}$$
Terminal states (`Stopped`, `Failed`) fail closed and reject post-terminal command dispatches.
