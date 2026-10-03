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

### M06: Anthropic Messages Protocol Adapter (Completed on `feat/anthropic-messages-adapter`)

- Implement `agent-studios-protocol-adapters::anthropic` module providing pure protocol translation between Codex Responses API semantics (`codex_api::ResponsesApiRequest`, `codex_api::ResponseEvent`) and Anthropic Messages wire protocol (`POST /v1/messages` and SSE stream events).
- Zero network transport (`reqwest`, hyper, Tokio runtime, Anthropic SDKs omitted; pure in-memory transformation).
- Zero secret handling (no API key, header, or credential resolution).
- Zero provider brand branching (translates against canonical Anthropic Messages wire specification).
- Pure Codex isolation (`codex-rs/` remains unmodified; zero diff with `origin/main`).
- Request translation (`translate_request`):
  - Top-level `system` block extraction: leading developer/system messages converted to system text blocks; interleaved system messages after conversational turns rejected fail-closed with `UnsupportedSystemHistoryPlacement`.
  - Non-zero `max_tokens` validation (`max_tokens == 0` fails with `InvalidMaxTokens(0)`).
  - Multimodal inputs: inline base64 image data URLs parsed into `AnthropicContentBlock::Image` with MIME validation (`image/jpeg`, `image/png`, `image/gif`, `image/webp`); unsupported MIME types reject with `UnsupportedImageMime`.
  - Tool calls & results: function calls mapped to `tool_use` blocks; consecutive tool execution outputs coalesced into a single user message containing multiple `tool_result` blocks with `is_error` status mapping.
  - Tool naming validation against Anthropic regex `^[a-zA-Z0-9_-]{1,64}$`.
  - Tool choice mapping: `"auto"`, `"none"`, `"required"` (`"any"`), and named tool choice; propagates `disable_parallel_tool_use`; emits `ForcedToolChoiceUnverified` warning if target tool is missing.
  - Structured outputs: maps Codex `TextControls.format` (`json_schema`) into `output_config.format`.
  - Reasoning effort mapping: exact semantic mapping for `Low`, `Medium`, `High`, `XHigh`, `Max`; fail-closed on `None`, `Minimal`, `Ultra`, `Persistent`, `Custom` with `UnsupportedReasoningEffort`.
  - Prompt cache injection policies: `None`, `LastUserMessage`, `ToolsAndSystem`, `AutomaticBreakpoint`.
  - Thinking policies: `Disabled`, `Adaptive`, `BudgetTokens`.
  - Continuation state & cryptographic signatures: restores native thinking blocks and cryptographic signatures (`anthropic-reasoning-{message_id}-{block_index}`) from `AnthropicContinuationState` for multi-turn loops. Rejects missing state with `MissingContinuationState`; emits `CrossProviderReasoningOmitted` for foreign reasoning IDs.
  - Security fail-closed: security-sensitive `access_programs` rejected with `UnsupportedSecurityFeature`.
  - Typed warning model for dropped Responses-only parameters (`store`, `service_tier`, `include`, `client_metadata`, `stream_options`, `prompt_cache_key`).
- Streaming translation (`AnthropicStreamTranslator`):
  - Lifecycle state machine: `Initial` -> `Started` -> `ActiveContentBlock` -> `OutputItemDone` -> `Completed`.
  - Event decoding: `message_start` -> `Created`, `ServerModel`; `content_block_start` -> `OutputItemAdded`; `content_block_delta` -> `OutputTextDelta`, `ToolCallInputDelta`, `ReasoningContentDelta`; `content_block_stop` -> `OutputItemDone`; `message_delta` -> token accumulation & stop reason; `message_stop` -> `Completed`.
  - Turn continuation: `stop_reason == "tool_use"` maps to `end_turn: Some(false)` to trigger Codex agent tool execution loops; `stop_reason == "end_turn" | "stop_sequence"` maps to `end_turn: Some(true)`.
  - Fail-closed truncation and limits: `max_tokens` (`MaxTokensExceeded`), `model_context_window_exceeded` (`ContextWindowExceeded`), `refusal` (`ModelRefusal`), `pause_turn` (`TurnPaused`).
  - Native thinking block and signature capture into `AnthropicContinuationState`.
  - Robust SSE line and chunk buffering supporting SSE event frames and raw JSON lines.
  - Negative token delta validation and open block rejection on stream termination.
- Test verification: 27 new tests across 3 test suites (`anthropic_request_tests.rs`, `anthropic_stream_tests.rs`, `anthropic_roundtrip_tests.rs`); 74 total passing tests across `agent-studios-protocol-adapters`.

### M07: Google Gemini generateContent Protocol Adapter (Completed on `feat/gemini-generate-content-adapter`)

- Implement `agent-studios-protocol-adapters::gemini` module providing pure in-memory protocol translation between Codex Responses API semantics (`codex_api::ResponsesApiRequest`, `codex_api::ResponseEvent`) and Google Gemini `generateContent` / `streamGenerateContent` wire protocol (`models/{model}:generateContent` and `streamGenerateContent`).
- Pure in-memory protocol translation: zero network transport (`reqwest`, `hyper`, asynchronous Tokio runtime, or Google SDKs omitted).
- Zero secret handling: no API keys, Google Cloud IAM tokens, or OAuth credentials consumed or resolved.
- Preserved path-based model parameterization: model ID returned verbatim in `GeminiRequestTranslation.model` without `models/` prefix.
- Upstream Codex isolation: `codex-rs/` remains completely unmodified (`git diff origin/main -- codex-rs` is empty).
- Request translation (`translate_request`):
  - System instructions: top-level `instructions` and leading `system`/`developer` turns mapped into `request.system_instruction`; interleaved system turns fail closed with `UnsupportedSystemHistoryPlacement`.
  - Conversational roles: `user` -> `user`, `assistant` -> `model`; unsupported roles fail closed with `UnsupportedRole`.
  - Multimodal inputs: inline data URLs for supported images (`image/jpeg`, `image/png`, `image/gif`, `image/webp`) and audio (`audio/wav`, `audio/mp3`, `audio/mpeg`, `audio/aac`, `audio/ogg`, `audio/flac`) mapped to `GeminiPart::inline_data`. External file references and audio URLs fail closed with `UnsupportedContent`.
  - Tool mapping: regex validation against Gemini tool naming contract `^[a-zA-Z0-9_.-]{1,128}$`; parameters schema preserved.
  - Strict tool semantics: `strict: true` tools promote tool choice mode to `VALIDATED`; mixed strict/non-strict tools promote entire set with `StrictToolScopePromoted` warning.
  - Tool choice mapping: `""` -> omitted / `VALIDATED`, `"auto"` -> `Auto` / `VALIDATED`, `"none"` -> `None` (retaining tool definitions), `"required"` -> `Any`.
  - Sequential tool calling rejection: `parallel_tool_calls: false` when tools are active fails closed with `SequentialToolCallingNotEnforceable`.
  - Structured outputs: maps `TextControls.format` (`json_schema`) into `response_mime_type = "application/json"` and `response_schema`.
  - Thinking policies: `ExactReasoningEffort` (`Minimal`, `Low`, `Medium`, `High`), `LegacyBudget(u64)`. Non-standard tiers fail closed with `UnsupportedReasoningEffort`.
  - Turn coalescing: adjacent function outputs coalesced into single `user` Content turn; adjacent tool calls coalesced into single `model` Content turn.
  - Opaque thought signature replay: restores native thinking parts and cryptographic signatures from `GeminiContinuationState`.
  - Fail-closed security features: `access_programs` rejected immediately with `UnsupportedSecurityFeature`.
- Streaming event translation (`GeminiStreamTranslator`):
  - Stream identity & model continuity: validates non-empty `responseId` on first chunk, enforces response ID and model version continuity across chunks.
  - Single candidate enforcement: rejects multiple candidates fail-closed with `MultipleCandidatesUnsupported`.
  - Safety & prompt feedback: blocks on prompt feedback `blockReason` or candidate safety ratings (`SafetyBlocked`).
  - Delta streaming: emits `OutputTextDelta` for text parts, `ReasoningContentDelta` for reasoning parts with thought signature buffering.
  - Atomic function call streaming: emits complete `OutputItemAdded(FunctionCall)`, `ToolCallInputDelta`, and `OutputItemDone(FunctionCall)`.
  - Deterministic call ID generation: when provider omits call ID, generates `gemini-call-{response_id}-{candidate_index}-{part_index}`, records mapping in `GeminiContinuationState`, and omits `id` on wire `functionResponse`.
  - Turn continuation: finish reason `"STOP"` maps to `end_turn: false` if tool calls were emitted, or `end_turn: true` otherwise.
  - Fail-closed terminal reasons: `MAX_TOKENS` (`MaxTokensExceeded`), `SAFETY` (`SafetyBlocked`), `RECITATION` (`RecitationBlocked`), `LANGUAGE` (`UnsupportedLanguage`), `BLOCKLIST` / `PROHIBITED_CONTENT` / `SPII` (`ContentBlocked`), `MALFORMED_FUNCTION_CALL` (`MalformedFunctionCall`).
  - Effective input token accounting: `input_tokens = promptTokenCount` (includes cached tokens), `cached_input_tokens = cachedContentTokenCount`, `total_tokens = totalTokenCount`. Rejects negative token counts.
- Full verification: 57 tests across 4 test suites (`gemini_request_tests.rs`, `gemini_stream_tests.rs`, `gemini_roundtrip_tests.rs`, `gemini_hardening_tests.rs`); 150 total passing tests in `agent-studios-protocol-adapters`.

### M07.5: Runtime Provider Transport Foundation (Completed on `feat/runtime-provider-transport`)

- Pluggable model inference backend seam in `codex-rs`:
  - `ModelInferenceBackend` trait and `ModelInferenceContext` in `codex-model-provider`.
  - Optional `inference_backend: Option<Arc<dyn ModelInferenceBackend>>` in `ModelProvider` (defaults to `None`).
  - Native dispatch branch in `codex-core::ModelClient::stream_custom_inference`, routing custom backend streams while preserving 100% of Codex agent loop semantics, tool approval gates, compaction, and telemetry.
  - Documented in `docs/agent-studios/CODEX_PATCHES.md`.
- `agent-studios-runtime-transport` crate:
  - Zero-plaintext secret management: `SecretString` implementing `zeroize::ZeroizeOnDrop` with redacted Debug, Display, and Serialize.
  - Dynamic secret resolution: `SecretResolver` trait with `InMemorySecretResolver` and `EnvSecretResolver`.
  - Authentication resolution: `ResolvedAuth` supporting `BearerToken`, `ApiKeyHeader`, and `QueryParameter` schemes.
  - Incremental SSE decoding: `SseParser` and `SseStream` handling multi-line data, comment stripping, and byte chunk buffering.
  - Transactional continuation state machine: `ContinuationManager` and `ContinuationTransaction` with commit-on-completion and guaranteed rollback-on-error semantics.
  - Protocol drivers: `ChatCompletionsDriver`, `AnthropicDriver`, and `GeminiDriver` connecting protocol adapters to live HTTP SSE endpoints.
  - `RuntimeRouter`: implements `codex_model_provider::ModelInferenceBackend` for provider-neutral dispatch.
- Verification & Integration testing:
  - 11 unit tests covering secrets, auth, SSE parsing, and continuation transactions.
  - 6 end-to-end `wiremock` integration tests covering Chat Completions streaming, Anthropic Messages streaming with continuation state persistence, Gemini streamGenerateContent streaming, transactional rollback on HTTP error, cancellation propagation via interrupt channel, and concurrent thread state isolation.

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
