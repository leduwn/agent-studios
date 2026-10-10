# Agent Studios — Master Architectural Roadmap (M01 – M19)

> **Status**: Authoritative Milestone Progression
> **Precedence**: Subservient to `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## Executive Summary & Milestone Progress Matrix

```text
┌────────────────────────────────────────────────────────────────────────┐
│                          MILESTONE PROGRESS                            │
├─────────┬────────────────────────────────────────┬─────────────────────┤
│ Target  │ Capability Domain                      │ Implementation State│
├─────────┼────────────────────────────────────────┼─────────────────────┤
│ M01     │ Bootstrap Codex Upstream               │ [COMPLETED]         │
│ M02     │ Deterministic Control Plane Foundation │ [COMPLETED]         │
│ M03     │ Provider Core & Model Registry         │ [COMPLETED]         │
│ M04     │ Codex Bridge / Responses Compatibility │ [COMPLETED]         │
│ M05/5.1 │ Chat Completions Protocol Adapter      │ [COMPLETED]         │
│ M06     │ Anthropic Messages Protocol Adapter    │ [COMPLETED]         │
│ M07     │ Google Gemini generateContent Adapter  │ [COMPLETED]         │
│ M07.5   │ Runtime Transport & SSE Streaming      │ [COMPLETED]         │
│ M07.6   │ Runtime Session Factory & Thread E2E   │ [COMPLETED]         │
│ M08/8.1 │ Internal Multi-Agent Runtime Hardening │ [COMPLETED]         │
│ M09     │ Worktree, Task & Artifact Orchestration│ [COMPLETED]         │
│ M10/10.1│ External Runtime Contract Hardening    │ [COMPLETED]         │
│ M11     │ OpenCode CLI Runtime Adapter           │ [PLANNED]           │
│ M12     │ Claude Code CLI Runtime Adapter        │ [PLANNED]           │
│ M13     │ Code-OSS Shell Integration Foundation   │ [PLANNED]           │
│ M14     │ Agent Studios Built-in AI Extension    │ [PLANNED]           │
│ M15     │ Unified Agent Mode / IDE Mode Workbench│ [PLANNED]           │
│ M16     │ Full Codex Feature Surface (Skills/MCP)│ [PLANNED]           │
│ M17     │ Windows Native Packaging & Installer   │ [PLANNED]           │
│ M18     │ Hardening, Stress & Security Audit     │ [PLANNED]           │
│ M19     │ Production Windows Stable Release      │ [PLANNED]           │
└─────────┴────────────────────────────────────────┴─────────────────────┘
```

---

## Detailed Milestone Specifications

### M01: Bootstrap Codex Upstream [COMPLETED]
- Import Codex baseline as a clean source snapshot (`d25c114d494ddb693290b76bf5e5f64ecbdb38fc`, 2026-10-02) with clean Git root history.
- Record exact upstream baseline SHA in machine-readable `upstream/codex.lock.json`.
- Maintain required Codex license notices and attribution in `LICENSE`, `NOTICE`, and `UPSTREAM.md`.
- Validate Windows native baseline compilation (`cargo build -p codex-cli`).

### M02: Deterministic Control Plane Foundation [COMPLETED]
- Scaffold core control plane library crate (`agent-studios-control-plane`) and protocol types (`agent-studios-protocol`).
- Define deterministic state structures: TaskGraph DAG engine, Agent lifecycle, Run execution, Approvals, Artifacts, and Event envelopes.
- Implement staged transactional pipeline (`commit_transaction`) and atomic batch persistence (ALL-or-ZERO) with sequence continuity.
- Strict event replay engine with full domain validation and corruption rejection.
- Hierarchical cancellation propagation and pre-execution dependency readiness checks.

### M03: Provider Core & Model Registry [COMPLETED]
- Establish `agent-studios-provider` crate.
- Decouple `ProviderDefinition` (vendor metadata) from `ProviderInstance` (concrete configured endpoint + credentials).
- Establish wire protocol decoupling via `ProtocolFamily` (`OpenAiResponses`, `OpenAiChatCompletions`, `AnthropicMessages`, `GeminiGenerateContent`, `Custom`).
- Implement zero-plaintext secret architecture using `SecretReference` and `SecretBackend`.
- Introduce tristate `CapabilitySupport` across 10 distinct model capabilities to eliminate false-negative assumptions.
- Provide `ModelDescriptor`, non-zero `ModelLimits`, and composite `ModelRef` (`provider_instance_id` + `model_id`).

### M04: Codex Runtime Bridge / Responses Compatibility [COMPLETED]
- Establish `agent-studios-codex-bridge` crate connecting `ProviderCatalog` and `ModelRef` to Codex `ModelProviderInfo`.
- Decouple bridge from Codex runtime modifications (`codex-rs/` untouched).
- Generate ephemeral `CodexResponsesBinding` runtime product.
- Enforce protocol gating on `ProtocolFamily::OpenAiResponses`.
- Zero-plaintext credential mapping without reading process environment values.

### M05 & M05.1: Chat Completions Protocol Adapter [COMPLETED]
- Establish `agent-studios-protocol-adapters` crate.
- Pure protocol translation between Codex Responses API semantics and standard OpenAI Chat Completions wire protocol.
- Zero network transport, zero secrets, zero provider brand branching.
- Correctness hardening: turn continuation (`finish_reason == "tool_calls"` maps to `end_turn = false`), response ID/model continuity, buffering fragmented tool arguments, and lossless tool output conversion.

### M06: Anthropic Messages Protocol Adapter [COMPLETED]
- Implement `agent-studios-protocol-adapters::anthropic` module translating to Anthropic Messages wire protocol (`POST /v1/messages` and SSE stream events).
- Top-level `system` block extraction, non-zero `max_tokens` validation, multimodal image parsing, and coalesced tool execution results.
- Transactional continuation state replay preserving thinking tokens and cryptographic signatures across multi-turn loops.

### M07: Google Gemini generateContent Protocol Adapter [COMPLETED]
- Implement `agent-studios-protocol-adapters::gemini` translating to Google Gemini `generateContent` / `streamGenerateContent` wire protocol.
- System instruction mapping, multimodal inline data (images and audio), regex tool validation, and opaque thought signature replay.
- Deterministic synthetic call ID generation and effective cached token accounting.

### M07.5: Runtime Provider Transport Foundation [COMPLETED]
- Pluggable model inference backend seam in `codex-rs` (Patch 001).
- `agent-studios-runtime-transport` crate with in-memory secret zeroization (`SecretString`).
- Incremental SSE decoding and transactional continuation state machine.
- Protocol drivers connecting adapters to live HTTP SSE endpoints.
- `RuntimeRouter` implementing `codex_model_provider::ModelInferenceBackend`.

### M07.6: Runtime Session Factory & Thread E2E [COMPLETED]
- Generic Codex seam (`ModelRuntimeOverride` handle in `codex-rs`, Patch 002).
- `agent-studios-runtime-session` crate with `StaticModelsManager` and `PreparedRuntimeSession`.
- Full decoupling of custom providers from mandatory OpenAI accounts and synthetic discovery.
- Comprehensive end-to-end WireMock integration suite across all 4 wire protocols.

### M08 & M08.1: Internal Multi-Agent Runtime & Hardening [COMPLETED]
- Establish `agent-studios-internal-agent` crate.
- Codex spawn runtime override seams (Patch 003 & Patch 004).
- Independent agent specs (`InternalAgentSpec`, `InternalTeamSpec`) and single-writer actor (`ControlPlaneActor`).
- Workspace access policy arbitrator (`WorkspacePolicyArbitrator`, `WorkspaceLease`).
- Pre-execution tool budget contributor (`AgentStudiosToolLifecycleContributor`).
- Structured coordinator DAG planning with Kahn's algorithm acyclicity validation.
- Supervisors with configurable failure policies (`FailFast`, `ContinueIndependent`, `RetryTask`).

### M09: Worktree, Task & Artifact Orchestration [COMPLETED]
- **Completed on `main`**: Fast-forwarded and integrated into `main` (`04650b52cc0af06d14d16771d3f7a9b9910024bf`).
- **M09.1 (Isolated Execution Workspaces)**: Unified workspace orchestrator wrapping upstream `codex-worktree::WorktreeManager`. Dedicates isolated Git worktrees and temporary branches to mutating workers.
- **M09.2 (Ephemeral Index Change Capture & Content-Addressed Store)**: Ephemeral index change capture via private `GIT_INDEX_FILE` without dirtying working index. Content-addressed artifact store indexing diffs by SHA-256 in `.git/agent-studios/artifacts/blobs/`. Control Plane authoritative version allocation.
- **M09.3 (Deterministic Patch Reconciliation & Final Gate)**: Reconciliation engine validating patches via `git apply --check` and applying to integration workspaces. Safe worktree retention (zero destructive `git reset --hard` or `git clean -fdx`). Non-conflated state transitions (`Blocked(ReconciliationConflict)` != `Cancelled`). Full end-to-end WireMock integration passing all quality gates.
- **M09.7 (Orphan-Thread Safety Fix)**: Enforces safe thread shutdown and removal invariant (`shutdown_and_wait_thread.await?` before `remove_thread_if_matches`), preventing untracked orphan threads on timeout or failure.

### M10 & M10.1: External Runtime Interface & Contract Hardening [COMPLETED]
- Establish vendor-neutral `agent-studios-external-runtime` crate with zero provider coupling.
- Define asynchronous `AgentRuntime` lifecycle trait (`discover`, `capabilities`, `start`, `send`, `interrupt`, `resume`, `stop`, `status`, `events`, `events_after`).
- Strongly-typed identifier families with wire collision prevention (`RuntimeImplementationId`, `rt-inst-<uuid>`, `rt-sess-<uuid>`).
- Strict lifecycle state machine with active/resumable `Interrupted` and fail-closed terminal transitions (`Stopped`, `Completed`, `Failed`).
- Tristate capability profiles across 16 dimensions (`Supported`, `Unsupported`, `Unknown`).
- Monotonically numbered event streams with typed event kinds and regex-free secret scrubbing.
- Remediated all 10 initial M10.1 contract review findings (P1-01 to P1-06, P2-01 to P2-04):
  - P1-01: Reliable event startup, subscription, and replay (`SessionEventHub`, `RuntimeEventSubscription`, replay before broadcast, gap/lag detection, `EventRetentionExceeded`).
  - P1-02: Elimination of plaintext credential bypass (`NonSecretValue`, forbidden keywords/prefixes, custom serde validation).
  - P1-03: Strict pre-spawn start validation (`validate_instance_start`, supported configuration checks, workspace isolation).
  - P1-04: Canonical lifecycle precedence and terminal semantics (validated ordering, idempotent stop, fail-closed terminal states).
  - P1-05: Authoritative event sequence enforcement (`EventSequencer` monotonic from 1, `EventBoundaryValidator` boundary checking, post-terminal rejection).
  - P1-06: Per-instance capability authority (instance capabilities override global defaults).
  - P2-01: `RuntimeConfigRef` deserialization integrity (trimmed non-empty string validation).
  - P2-02: Diagnostic sanitization coverage (case-insensitive, quoted, query string, basic auth URI scrubbing).
  - P2-03: `ExecutionWorkspace` isolation policy (`WorkspaceAccessMode::Mutating` prohibits `SharedSource`).
  - P2-04: Fault-tolerant `RuntimeRegistry` discovery (`JoinSet` concurrency, lock drop before await, `RegistryDiscoveryOutcome` failure isolation).
- Remediated all 10 second-round source review findings (R01 to R10):
  - R01: `SessionEventHub::ingest()` routes through `EventBoundaryValidator` preventing state corruption.
  - R02: `SessionEventHub::emit()` serializes sequence, validation, history, and broadcast under atomic `HubState` lock.
  - R03: Enforce strict `after_sequence = N` replay offset semantics and retention exceeded boundaries.
  - R04: Encapsulate `EnvironmentVariableBinding` private fields and enforce serde deserialization invariants.
  - R05: Finalize event streams on terminal `StatusChanged` events (`Stopped`, `Completed`, `Failed`).
  - R06: Enforce canonical session ref and terminal state check precedence in `FakeAgentRuntime::send()`.
  - R07: Attribute `JoinSet` discovery worker panics and cancellations to runtime implementation IDs.
  - R08: Iterative credential sanitization across multi-URL strings and query parameters without truncation.
  - R09: Strict fail-closed rejection of `ExecutionWorkspace::SharedSource` under all access modes.
  - R10: Full documentation, roadmap, and evidence alignment across all contracts.
- Remediated all 8 third-round targeted closure findings (SR3-01 to SR3-08):
  - SR3-01: Correct retention capacity 1 bookkeeping in `event.rs` so `earliest_retained_sequence` is never stale.
  - SR3-02: Strictly single authoritative `SessionStarted` validation in `EventBoundaryValidator` (rejects duplicates without state mutation).
  - SR3-03: Sanitize registry worker panic diagnostics before logging to `tracing::error!()`.
  - SR3-04: Multi-producer Tokio barrier concurrency tests (10 producers, 100 events, 5 competing terminals) proving monotonic gapless event streams.
  - SR3-05: Real registry worker panic attribution with sensitive credential canary and elimination of task mapping races.
  - SR3-06: Typed `InvalidReplayOffset` validation and deterministic terminal subscription completion.
  - SR3-07: Clarified workspace isolation scope: M10 enforces typed request/policy boundaries (rejecting `SharedSource`); OS process sandboxing is deferred to M11.
  - SR3-08: Complete milestone quality gates (fmt, clippy, workspace tests, builds) and review artifact generation.
- Remediated all 3 fourth-round security boundary & verification findings (SR4-01 to SR4-03):
  - SR4-01: Documented and verified process-level panic output security boundary; proved library cannot safely override `std::panic::set_hook()`; subprocess regression test verified zero canary leakage in registry diagnostics and truthful evaluation of process stderr under default hook.
  - SR4-02: Multi-producer concurrent emission, replay-to-live handoff under active writes, retention overflow/lag recovery, independent subscriber isolation, and competing terminal closure across diverse terminal kinds.
  - SR4-03: Full 10 workspace quality gates executed, 20 repeated concurrency test runs, clean milestone diff audit against `af4b96d17f`, and review bundle exported.
- 63-test contract verification suite (61 contract regression tests + 2 unit tests) passing with zero failures.

### M11: OpenCode Runtime Adapter & OS Process Sandboxing [PLANNED]

- Implement runtime adapter wrapping OpenCode CLI and server sessions.
- Implement OS-level process sandboxing and containment (Windows Job Objects / AppContainer, Linux cgroups/namespaces).
- Map OpenCode provider configurations to Agent Studios `ProviderCatalog` instances.
- Translate OpenCode session steps into Control Plane `Task` and `Run` entities.

### M12: Claude Code Runtime Adapter [PLANNED]
- Implement runtime adapter wrapping Claude Code CLI.
- Map Claude Code sub-agent loops and tool interactions into Control Plane events.
- Enforce Agent Studios pre-execution tool budgets and approvals over Claude Code tool calls.

### M13: Code-OSS Integration Foundation [PLANNED]
- Code-OSS source snapshot targeting Windows 11 x64 first.
- Independent Agent Studios desktop application branding, typography, and monochrome visual language.
- Native IPC bridge (candidate/reference protocol: JSON-RPC; exact transport and protocol mechanism TBD in M13 open design question) connecting Code-OSS desktop workbench to local Agent Studios App Server.
- Full IDE feature parity (Monaco editor, Explorer, Search, Git, Terminal, Debugger, LSP).

### M14: Agent Studios Built-in AI Extension [PLANNED]
- Native built-in Code-OSS extension contributing the single `Agents` Activity Bar item and sidebar.
- Structured views: Current Run, Tasks checklist, Approval Inbox, Artifacts index, Timeline feed.
- Editor inline decorations and context menu commands.

### M15: Unified Agent Mode / IDE Mode Workbench [PLANNED]
- Dual presentation surfaces over one shared underlying runtime session.
- Agent Mode: Conversation-first, progressive orchestration disclosure startup experience.
- IDE Mode: Full Code-OSS professional workbench.
- Instant, zero-loss mode toggling sharing `session_id`, `cwd`, active runs, tasks, agents, tool history, worktrees, and approvals.

### M16: Full Codex Feature Surface [PLANNED]
- Comprehensive Codex capability integration surfaced in desktop UI:
  - Skills (`SKILL.md`) with GitHub installation (`/skill install`).
  - Model Context Protocol (MCP) servers across workspace and global scopes.
  - Plugins, lifecycle hooks (`pre_turn`, `post_task`, `on_reconciliation`).
  - Hierarchical repository instruction discovery (`AGENTS.md`).

### M17: Packaging, Installer & Auto-Updater [PLANNED]
- Native Windows 11 x64 installers (MSI / NSIS).
- Code signing and secure update delivery channels.
- Portable release builds for macOS and Linux.

### M18: Hardening, Stress & Security Audit [PLANNED]
- Multi-agent stress testing, high-concurrency DAG execution, and worktree conflict benchmarks.
- Comprehensive security audit of credential resolution and process sandboxing.
- Memory leak and long-session stability profiling.

### M19: Production Windows Stable Release [PLANNED]
- Final user documentation, localization verification (English & Vietnamese), and public distribution.
