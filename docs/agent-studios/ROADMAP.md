# Agent Studios Architectural Roadmap

This roadmap defines the strategic progression for building Agent Studios upon the Codex upstream baseline. Milestones reflect architectural progression; uncompleted milestones remain prospective and directional.

---

### M01: Bootstrap Codex Upstream (Completed)
- Import Codex baseline as a clean source snapshot (`d25c114d494ddb693290b76bf5e5f64ecbdb38fc`, 2026-10-02) with clean Git root history.
- Record exact upstream baseline SHA in machine-readable `upstream/codex.lock.json`.
- Maintain required Codex license notices and attribution in `LICENSE`, `NOTICE`, and `UPSTREAM.md`.
- Validate Windows native baseline compilation (`cargo build -p codex-cli`).
- Establish snapshot-based upstream synchronization architecture and maintenance workflows.

### M02: Agent Studios Control-Plane Foundation (Completed)
- Scaffold core control plane library crate (`agent-studios-control-plane`) and protocol types (`agent-studios-protocol`).
- Define deterministic state structures: TaskGraph DAG engine, Agent lifecycle, Run execution, Approvals, Artifacts, and Event envelopes.
- Implement staged transactional pipeline (`commit_transaction`) and atomic batch persistence (ALL-or-ZERO) with sequence continuity and regression protection.
- Strict event replay engine with full domain validation and corruption rejection (19 corruption test cases + duplicate entity guardrails).
- Hierarchical cancellation propagation and pre-execution dependency readiness checks.
- Eliminated all mutable store/state escape hatches (`store_mut` removed).

### M03: Provider Core & Model Registry (Completed on `main`)

- Establish `agent-studios-provider` crate.
- Decouple `ProviderDefinition` (vendor/gateway metadata) from `ProviderInstance` (concrete configured endpoint + credentials).
- Establish wire protocol decoupling via `ProtocolFamily` (`OpenAiResponses`, `OpenAiChatCompletions`, `AnthropicMessages`, `GeminiGenerateContent`, `Custom`).
- Implement zero-plaintext secret architecture using `SecretReference` and `SecretBackend` (EnvironmentVariable, OsCredentialStore, External).
- Enforce strict endpoint security: forbid URL userinfo/credentials, forbid URL fragments, reject sensitive static headers (`Authorization`, `x-api-key`, etc.).
- Introduce tristate `CapabilitySupport` (`Unknown`, `Unsupported`, `Supported`) across 10 distinct model capabilities to prevent false-negative capability assumptions.
- Provide `ModelDescriptor`, non-zero `ModelLimits`, and composite `ModelRef` (`provider_instance_id` + `model_id`) for deterministic provider-neutral routing.
- Implement `ProviderRegistry`, `ModelRegistry`, and coordinating facade `ProviderCatalog` with safe removal invariants (rejection on in-use instances/definitions).
- Implement exportable `ProviderCatalogSnapshot` with atomic, staged import validation.
- *(Note: Provider Core models metadata and configuration only; network requests and LLM drivers arrive in subsequent milestones.)*

### M04: Codex Runtime ↔ Provider Core Bridge / OpenAI Responses Compatibility (Completed on `main`)

- Establish `agent-studios-codex-bridge` crate connecting `ProviderCatalog` and `ModelRef` to Codex `ModelProviderInfo`.
- Strictly decouple bridge from Codex runtime modifications (`codex-rs/` untouched, zero diff against `main`).
- Generate ephemeral `CodexResponsesBinding` runtime product (never stored in persistent configuration).
- Enforce protocol gating on `ProtocolFamily::OpenAiResponses` (Codex runtime natively only supports Responses API; rejects other wire APIs with typed errors).
- Zero-plaintext credential resolution: maps environment variable references to `env_key` or `env_http_headers` without reading process environment values.
- Guard against unsupported secret backends (`OsCredentialStore`, `External`) and query parameter authentication.
- Model capability safety: explicit `Unsupported` for `tool_calling` or `streaming` fails; `Unknown` capabilities tracked in `CodexCompatibilityReport.unverified_capabilities`.
- Deterministic instance keying (`agent-studios-<provider-instance-uuid>`), enabling multi-instance coexistence (e.g. 9Router Local vs 9Router VPS with identical model IDs).
- Deterministic model catalog URL resolution and strict static header sanitization.

### M05: Chat Completions Protocol Adapter (Completed on `feat/chat-completions-adapter`)

- Establish `agent-studios-protocol-adapters` crate (`agent-studios-protocol-adapters`).
- Implement pure protocol translation between Codex Responses API semantics (`codex_api::ResponsesApiRequest`, `codex_api::ResponseEvent`) and standard OpenAI Chat Completions wire protocol (`POST /v1/chat/completions`).
- Zero network transport (`reqwest` and async Tokio runtime omitted; pure synchronous data transformation).
- Zero secrets handling (no credential resolution or API key consumption).
- Zero provider brand branching (translates against canonical OpenAI Chat wire protocol).
- Request translation: model ID preservation, instructions mapped to leading system message, developer role normalized to system role, message history preservation, multimodal inline data URLs, function calls mapped to `tool_calls`, tool outputs mapped to `role = "tool"` with mandatory `tool_call_id`, tool definition schema conversion (flat Responses format converted to nested function format), structured output `response_format` (`json_schema`), typed warning model for dropped Responses-only fields, and strict security rejection of `access_programs`.
- Streaming translation (`ChatCompletionStreamTranslator`): deterministic event emission (`Created`, `OutputItemAdded`, `OutputTextDelta`, `ToolCallInputDelta`, `OutputItemDone`, `Completed`), fragmented/interleaved tool call reconstruction across parallel tool indices, finish reason validation (`stop`, `tool_calls`, rejecting `length` and `content_filter`), single choice constraint (`choice.index == 0`), token usage mapping, and SSE line decoding.
- Full verification: 17 unit and integration tests across 3 test suites, zero clippy warnings with `-D warnings`, zero modifications to upstream `codex-rs/`.

### M05.1: Chat Completions Correctness Hardening (Completed on `feat/chat-completions-adapter`)

- Strict Codex turn continuation: `finish_reason == "tool_calls" | "function_call"` sets `ResponseEvent::Completed.end_turn = Some(false)` to trigger upstream tool execution; `finish_reason == "stop"` sets `end_turn = Some(true)`.
- Response ID and model continuity: enforces non-empty response ID on first chunk (`MissingResponseId`), rejects ID mismatch across chunks (`ResponseIdMismatch`), and guarantees model continuity across stream (`ResponseModelMismatch`).
- Strict terminal state machine: fails closed with `AlreadyCompleted` on any call to `feed_chunk`, `finish`, or `feed_done` after completion; fails with `MissingFinishReason` on stream EOF without terminal finish reason.
- Elimination of synthetic tool identity: buffers argument fragments (`pending_argument_fragments`) until both `id` and `name` are received, then emits `OutputItemAdded(FunctionCall)` followed by queued `ToolCallInputDelta` fragments in original sequence; incomplete tool calls at finish fail with `IncompleteToolCall`.
- Tool identity immutability and type validation: rejects conflicting ID or function name updates (`ToolCallIdentityMismatch`), and rejects non-function stream tool calls (`UnsupportedToolCallType`).
- Lossless tool output translation: validates `FunctionCallOutputBody::ContentItems` and fails closed on non-text payloads (`UnsupportedToolOutputContent`).
- Fail-closed request parsing: rejects encrypted `AgentMessage` (`UnsupportedEncryptedAgentMessage`), unsupported `ResponseItem` variants (`UnsupportedResponseItem`), invalid roles (`InvalidRole`), and non-standard `tool_choice` strings (`UnsupportedToolChoice`). Emits `NormalizedImageDetail` warning when normalizing `ImageDetail::Original` to `"high"`.
- Usage validation & deterministic lifecycle: rejects negative token values (`InvalidUsage`), rejects text resumption after tool call execution (`InvalidStreamState`), and enforces exactly-once item lifecycle.
- Full regression suite: 30 new hardening tests in `correctness_hardening_tests.rs` (47 total tests in crate), zero clippy warnings, zero modifications to `codex-rs/`.

### M06: Anthropic Messages Protocol Adapter
- Implement Anthropic Messages protocol translation adapter.
- Provide token caching markers, thinking parameter mapping, and tool call serialization.

### M07: Google Gemini Protocol Adapter
- Implement Google Gemini `generateContent` adapter.
- Support multimodal payloads and Gemini-specific function calling conventions.

### M08: Internal Multi-Agent Extension
- Enhance internal agent runner to instantiate multiple isolated agent configurations.
- Allow per-agent role specifications, provider assignments, reasoning depths, and tool budgets.

### M09: Worktree, Task & Artifact Orchestration
- Extend Git worktree isolation for concurrent workers based on upstream Codex worktree infrastructure.
- Implement deterministic artifact tracking, versioning, and reconciliation workflows.

### M10: External Runtime Interface
- Define standard `AgentRuntime` interface (lifecycle, communication, supervision, capabilities).
- Establish process-level process containment and IPC protocol.

### M11: OpenCode Runtime Adapter
- Implement runtime adapter for OpenCode CLI sessions.
- Map input/output streams and tool interactions to Agent Studios control plane events.

### M12: Claude Code Runtime Adapter
- Implement runtime adapter wrapping Claude Code CLI.
- Handle session resumption, approval delegation, and output streaming.

### M13: Code-OSS Integration Foundation
- Maintained Code-OSS source snapshot/fork targeting Windows x64 first.
- Independent Agent Studios branding and layout configuration.
- Built-in extension loading mechanism and native Agent Studios runtime IPC.
- Preserve normal IDE behavior (Explorer, search, source control, terminal, debugger, LSP).
- Initiate custom iconography and visual identity pass (icon design deferred until M13).

### M14: Agent Studios Built-in AI Extension
- Default Agent Studios chat interface replacing the AI surface normally occupied by Copilot.
- Direct routing to Agent Studios / Codex runtime (no `@agentstudios` prefix required, no Copilot dependency).
- Full operational parity: inspect/edit files, `apply_patch`, integrated terminal commands, test/build execution.
- Interactive approvals, diffs, provider/model controls, and task queue visibility.

### M15: Unified Agent / IDE Workbench
- Dual layout system: IDE-focused layout and Agent-focused layout.
- One shared underlying runtime session (identical session ID, working directory, tool history, worktrees, active tasks).
- Instant switching between layouts without state loss or process interruption.
- Equal Codex capabilities available in both views.

### M16: Full Codex Feature Surface
- Comprehensive Codex capability integration: Skills (`SKILL.md`), Plugins, MCP servers, hooks, `AGENTS.md`.
- Advanced multi-agent orchestration: Git worktrees, subagents, multi-agent primitives, budget caps.
- Fine-grained approval and sandboxing controls.
- Integrated diagnostics, session replay, and runtime telemetry.

### M17: Packaging, Installer & Auto-Updater
- Configure Windows native installers (MSI / NSIS / WiX).
- Implement background update checks and secure patching channels.

### M18: Hardening & End-to-End Integration Testing
- Comprehensive multi-agent stress tests, worktree conflict benchmarks, and recovery drills.
- Windows security audit and memory profiling.

### M19: Windows Stable Release
- Final polish, user documentation, and initial Windows x64 public distribution.
