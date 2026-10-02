# Anthropic Messages Protocol Adapter (Milestone M06 & M06.1)

## Overview

The `agent-studios-protocol-adapters::anthropic` module provides a pure, deterministic protocol translation layer between OpenAI Codex Responses API semantics (`codex_api::ResponsesApiRequest`, `codex_api::ResponseEvent`) and the Anthropic Messages wire protocol (`POST /v1/messages` and SSE stream events).

This protocol adapter enables Agent Studios to target any Anthropic Messages API-compatible endpoint—including direct Anthropic endpoints, AWS Bedrock Anthropic proxies, Google Cloud Vertex AI Anthropic proxies, and local/enterprise gateways—without modifying the upstream Codex runtime core or coupling to specific vendor brands.

```
┌─────────────────────────────────────────────────────────────┐
│                    Codex Runtime Core                       │
│  (Operates exclusively on Responses API request/events)     │
└──────────────────────────────┬──────────────────────────────┘
                               │
               ResponsesApiRequest / ResponseEvent
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│      agent-studios-protocol-adapters::anthropic             │
│  ┌────────────────────────┐    ┌──────────────────────────┐ │
│  │   Request Translator   │    │ Stream Event Translator  │ │
│  │  (translate_request)   │    │(AnthropicStreamTranslator│ │
│  └───────────┬────────────┘    └────────────▲─────────────┘ │
│              │                              │               │
│              │ AnthropicContinuationState   │               │
│              │ (thinking + signatures)      │               │
│              └──────────────┬───────────────┘               │
└─────────────────────────────┼───────────────────────────────┘
                              │
    AnthropicMessagesRequest  │ AnthropicStreamEvent (SSE)
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│          Standard Anthropic Messages Wire Protocol          │
│          (Anthropic API, Bedrock, Vertex AI, Proxies)       │
└─────────────────────────────────────────────────────────────┘
```

---

## Architectural Invariants

1. **Pure Protocol Translation (Zero Network Transport)**:
   - Does not perform HTTP requests.
   - Does not depend on `reqwest`, `hyper`, an asynchronous Tokio runtime, or Anthropic SDKs.
   - Operates entirely synchronously on in-memory Rust data structures and strings.
2. **Zero Secret Handling**:
   - Does not consume or resolve API keys, auth tokens, or environment variables.
   - Authentication and credential injection remain isolated in the HTTP client / transport layers.
3. **Zero Provider Brand Branching**:
   - Logic does not branch on vendor names or IDs (`anthropic`, `claude`, `bedrock`).
   - Translates strictly against the canonical Anthropic Messages wire specification.
4. **Upstream Codex Isolation**:
   - `codex-rs/` is not modified (`git diff origin/main -- codex-rs` is completely empty).
   - All adapter code resides in `agent-studios-rs/crates/protocol-adapters`.
5. **Lossless Multi-Turn Reasoning Continuation**:
   - Preserves native thinking content and cryptographic signatures across conversational turns using `AnthropicContinuationState` with deterministic reasoning IDs (`anthropic-reasoning-{message_id}-{block_index}`).
   - Cryptographic signatures are preserved verbatim without trimming, normalization, or modification.
6. **No Reverse Dependencies**:
   - Neither `agent-studios-provider` nor `agent-studios-control-plane` depends on this crate.
7. **No Native Server Tools**:
   - Native Anthropic server tools (e.g., Anthropic web search, computer use, code execution bash) are not exposed or translated.
   - Codex runtime retains full authority over tool definitions, approvals, sandboxing, execution, and auditability.

---

## Request Translation (`translate_request`)

The `translate_request` function converts a Codex `ResponsesApiRequest` into an `AnthropicMessagesRequest` wrapped in an `AnthropicRequestTranslation` bundle with typed warnings.

### Key Behaviors:

- **Explicit Request Options (No Silent Defaults)**:
   - `AnthropicRequestOptions` does not implement `Default` to prevent accidental 4096 or speculative token limits.
   - Constructed explicitly via `AnthropicRequestOptions::new(max_tokens: u64)`.
   - Rejects `max_tokens == 0` fail-closed with `AnthropicAdapterError::InvalidMaxTokens(0)`.
- **Model & Token Configuration**:
  - Passes the target `model` identifier unchanged.
- **System Instructions & Role Placement**:
  - Extracts Codex `instructions` and leading `system`/`developer` messages into top-level system text blocks (`AnthropicSystemBlock::Text`).
  - **Interleaved System Rejection**: If a `system` or `developer` message appears *after* conversational turns (`user` or `assistant`), translation fails closed with `AnthropicAdapterError::UnsupportedSystemHistoryPlacement`.
- **Message Content & Multimodal Inputs**:
  - `ContentItem::InputText` and `ContentItem::OutputText` map to `AnthropicContentBlock::Text`.
  - Multimodal inputs: `ContentItem::InputImage` with inline `data:` URIs are parsed and validated against supported MIME types (`image/jpeg`, `image/png`, `image/gif`, `image/webp`). Unsupported MIME types fail closed with `AnthropicAdapterError::UnsupportedImageMime`.
  - Remote file IDs (`ImageReference::File`) and audio inputs (`ContentItem::InputAudio`) fail closed with `AnthropicAdapterError::UnsupportedContent`.
- **Tool Definitions & Strict Property Preservation**:
  - Translates function schemas into Anthropic tool definitions (`AnthropicTool`).
  - Preserves `strict: Option<bool>` from both flat (`tool.strict`) and nested (`tool.function.strict`) tool declarations. Serialized conditionally via `skip_serializing_if = "Option::is_none"`.
  - Validates tool naming rules against the official Anthropic Messages API specification contract (`^[a-zA-Z0-9_-]{1,64}$`). Rejects invalid names fail-closed.
  - Rejects non-function tools with `AnthropicAdapterError::UnsupportedToolType`.
- **Tool Choice Mapping**:
  - `""` -> `None` (omitted from request).
  - `"auto"` -> `AnthropicToolChoice::Auto { disable_parallel_tool_use }`.
  - `"none"` -> `AnthropicToolChoice::None` (`{"type": "none"}`). **Crucial**: Tool definitions in `request.tools` are strictly preserved to maintain prompt cache prefixes while disabling tool calling.
  - `"required"` -> `AnthropicToolChoice::Any { disable_parallel_tool_use }`.
  - Arbitrary named tool choice strings (e.g. `"custom_tool"`) are rejected fail-closed with `AnthropicAdapterError::UnsupportedToolChoice` to ensure deterministic execution.
  - Propagates `disable_parallel_tool_use` when `parallel_tool_calls` is `false`.
- **Custom Tool Call & Output Handling**:
  - `ResponseItem::CustomToolCall`: Must parse as a valid JSON Object (`Value::Object`). Non-object inputs, scalars, and malformed JSON fail closed with `AnthropicAdapterError::UnsupportedCustomToolInput`.
  - `ResponseItem::CustomToolCallOutput` & `ResponseItem::FunctionCallOutput`: Handled losslessly through a shared helper `convert_tool_output_payload`.
    - Supports `FunctionCallOutputBody::Text` and `FunctionCallOutputBody::ContentItems` (text and supported inline base64 images).
    - Rejects audio, encrypted content, and file references fail-closed.
    - Maps `output.success` faithfully to `is_error` (`Some(!success)`).
- **Structured Outputs**:
  - Maps Codex `TextControls.format` (`json_schema`) into `output_config.format = AnthropicOutputFormat::JsonSchema { schema }`.
- **Reasoning Effort Mapping**:
  - Maps Codex `ReasoningEffort`:
    - `Low` -> `"low"`
    - `Medium` -> `"medium"`
    - `High` -> `"high"`
    - `XHigh` -> `"xhigh"`
    - `Max` -> `"max"`
  - Non-equivalent reasoning effort settings (`None`, `Minimal`, `Ultra`, `Persistent`, `Custom`) fail closed with `AnthropicAdapterError::UnsupportedReasoningEffort`.
- **Prompt Caching Policies & Typed TTL**:
  - Typed TTL via `AnthropicCacheTtl`: `FiveMinutes` (`"5m"`), `OneHour` (`"1h"`).
  - Caching policy options under `AnthropicPromptCachePolicy`:
    - `None`: No cache breakpoints injected.
    - `LastUserMessage { ttl: Option<AnthropicCacheTtl> }`: Ephemeral cache control on last user content block.
    - `ToolsAndSystem { ttl: Option<AnthropicCacheTtl> }`: Ephemeral cache control on last system block and last tool definition.
    - `AutomaticBreakpoint { ttl: Option<AnthropicCacheTtl> }`: Caches both system/tools and last user message.
- **Thinking / Extended Reasoning Policies**:
  - Supports `AnthropicThinkingPolicy`:
    - `ProviderDefault`: Thinking config omitted from the request wire payload.
    - `Adaptive`: Emits `{ "type": "adaptive" }`.
    - `LegacyBudgetTokens(budget)`: Emits `{ "type": "enabled", "budget_tokens": budget }`. Enforces `budget >= 1024` or fails closed with `AnthropicAdapterError::InvalidThinkingBudget(budget)`. Emits `AnthropicAdapterWarning::LegacyThinkingBudgetUsed(budget)` warning.
  - Reconstitutes native thinking blocks from `AnthropicContinuationState` during multi-turn continuation.
  - Reconstructed message ordering strictly follows Codex `ResponseItem` history sequence.
  - Missing continuation state for an Anthropic reasoning item fails closed with `AnthropicAdapterError::MissingContinuationState`. Cross-provider reasoning items are omitted with `AnthropicAdapterWarning::CrossProviderReasoningOmitted`.
- **Fail-Closed Security Features**:
  - `access_programs` is security-sensitive and immediately rejected with `AnthropicAdapterError::UnsupportedSecurityFeature`.
  - Encrypted agent messages and encrypted tool outputs are rejected.

---

## Streaming Event Translation (`AnthropicStreamTranslator`)

The `AnthropicStreamTranslator` state machine parses SSE lines and chunks from the Anthropic Messages streaming API and converts them into Codex `ResponseEvent` sequences.

### Lifecycle State Machine:

```
[Initial]
   │
   │ message_start
   ▼
[Started] ──── content_block_start ───► [ActiveContentBlock]
   ▲                                              │
   │                                              │ content_block_delta
   │                                              ▼
   │                                    [OutputTextDelta / ToolCallInputDelta / ReasoningContentDelta]
   │                                              │
   │                                              │ content_block_stop
   └──────────── OutputItemDone ──────────────────┘
   │
   │ message_delta (stop_reason, cumulative usage)
   │ message_stop
   ▼
[Completed] (ResponseEvent::Completed)
```

### Event Translation Matrix:

| Anthropic SSE Event | State Transition / Action | Codex `ResponseEvent` Emitted |
| :--- | :--- | :--- |
| `message_start` | Initializes response ID, model, base usage | `ResponseEvent::Created`, `ResponseEvent::ServerModel` |
| `content_block_start` (`text`) | Opens text block | `ResponseEvent::OutputItemAdded(ResponseItem::Message)` |
| `content_block_start` (`tool_use`) | Validates non-empty `id` and `name`, opens tool call | `ResponseEvent::OutputItemAdded(ResponseItem::FunctionCall)` |
| `content_block_start` (`thinking`) | Opens thinking block | `ResponseEvent::OutputItemAdded(ResponseItem::Reasoning)` |
| `content_block_start` (`redacted_thinking`) | Stores redacted data block | `ResponseEvent::OutputItemAdded(ResponseItem::Reasoning)` |
| `content_block_delta` (`text_delta`) | Accumulates text | `ResponseEvent::OutputTextDelta` |
| `content_block_delta` (`input_json_delta`) | Accumulates arguments | `ResponseEvent::ToolCallInputDelta` |
| `content_block_delta` (`thinking_delta`) | Accumulates thinking | `ResponseEvent::ReasoningContentDelta` |
| `content_block_delta` (`signature_delta`) | Buffers cryptographic signature | *Internal continuation buffer* |
| `content_block_stop` (`tool_use`) | Validates accumulated arguments is valid JSON Object | `ResponseEvent::OutputItemDone` |
| `content_block_stop` (`thinking`) | Enforces non-empty cryptographic signature | `ResponseEvent::OutputItemDone` |
| `content_block_stop` (other) | Closes block, persists continuation state | `ResponseEvent::OutputItemDone` |
| `message_delta` | Captures `stop_reason`, overwrites cumulative `output_tokens` | *None (buffered)* |
| `message_stop` | Computes effective input & total `TokenUsage` via checked arithmetic | `ResponseEvent::Completed { response_id, token_usage, end_turn }` |
| `ping` | Keepalive | *None (ignored)* |
| `error` | Stream error | Fails with `AnthropicAdapterError::StreamApiError` |

### Stream Validation Invariants:

1. **Content Block Start Invariants**:
   - `ToolUse` block start requires non-empty `id` and `name`. If either is empty, translation fails closed with `AnthropicAdapterError::MissingStreamIdentity`.
2. **Tool Argument Verification at Block Stop**:
   - At `content_block_stop` for a `ToolUse` block, the accumulated `arguments` string must parse as a valid JSON Object (`serde_json::Value::Object`). Malformed JSON or non-object payloads fail closed with `AnthropicAdapterError::InvalidToolArguments`. The exact raw string is preserved in `ResponseItem::FunctionCall.arguments`.
3. **Thinking Cryptographic Signature Completeness**:
   - At `content_block_stop` for a `Thinking` block, a non-empty signature is strictly required. If missing or empty, translation fails closed with `AnthropicAdapterError::MissingThinkingSignature`.
4. **Cumulative Output Token Usage Handling**:
   - In `message_delta`, `usage.output_tokens` is cumulative for the entire message.
   - The translator overwrites `self.accumulated_output_tokens` with the latest value (rejecting negative values). Output tokens are never added incrementally.
5. **Effective Input Token Accounting**:
   - Anthropic wire protocol reports `input_tokens` as uncached prompt tokens only; `cache_read_input_tokens` and `cache_creation_input_tokens` are reported separately.
   - Codex `TokenUsage` expects `input_tokens` to represent the complete effective input context (`uncached + cached + written`).
   - Formulas use checked 64-bit arithmetic:
     $$\text{effective\_input} = \text{uncached} + \text{cached} + \text{written}$$
     $$\text{total\_tokens} = \text{effective\_input} + \text{output\_tokens}$$
     $$\text{cached\_input\_tokens} = \text{cached}$$
     $$\text{cache\_write\_input\_tokens} = \text{written}$$
   - Any arithmetic overflow fails closed with `AnthropicAdapterError::InvalidUsage`.
6. **Turn Continuation & Stop Reasons**:
   - `stop_reason: "tool_use"` -> `end_turn: Some(false)`: Signals Codex agent runtime that tool calls were issued and the turn must continue after tool execution.
   - `stop_reason: "end_turn"` or `"stop_sequence"` -> `end_turn: Some(true)`: Turn completed normally.
   - `stop_reason: "max_tokens"` -> Fails closed with `AnthropicAdapterError::MaxTokensExceeded`.
   - `stop_reason: "model_context_window_exceeded"` -> Fails closed with `AnthropicAdapterError::ContextWindowExceeded`.
   - `stop_reason: "refusal"` -> Fails closed with `AnthropicAdapterError::ModelRefusal`.
   - `stop_reason: "pause_turn"` -> Fails closed with `AnthropicAdapterError::TurnPaused`.

---

## Test Verification

The adapter includes 57 dedicated integration tests across four test suites:

1. **Hardening & Semantic Invariants (`tests/anthropic_hardening_tests.rs`)** - 30 tests:
   - Cache read token mapping into effective input.
   - Cache write token mapping into effective input.
   - Mixed cache read/write token calculation and total check.
   - Cumulative output token handling without double addition.
   - Native `tool_choice: "none"` preserving tool definitions.
   - Native `tool_choice: "none"` wire serialization (`{"type": "none"}`).
   - Rejection of arbitrary tool choice strings (`UnsupportedToolChoice`).
   - `strict: true` preservation from flat and nested tool definitions.
   - `strict: false` preservation.
   - Absence of `strict` stays omitted.
   - Tool naming boundary conditions (1 char, 64 chars, 65 chars rejected, valid/invalid characters).
   - Non-object custom tool input fail-closed rejection.
   - Valid structured custom tool input success.
   - Custom tool output text handling.
   - Custom tool output inline image handling.
   - Custom tool output audio rejection.
   - Custom tool output `success: false` mapping to `is_error: true`.
   - Rejection of default `max_tokens` (explicit construction enforcement).
   - `ProviderDefault` thinking policy omits explicit configuration.
   - `Adaptive` thinking policy wire serialization (`{"type": "adaptive"}`).
   - Legacy budget below 1024 rejection (`InvalidThinkingBudget`).
   - Legacy budget compatibility warning emission (`LegacyThinkingBudgetUsed`).
   - Missing thinking signature rejection on block stop (`MissingThinkingSignature`).
   - Exact thinking signature roundtrip without modification.
   - Thinking block order preservation in request history.
   - Cache TTL 5m serialization.
   - Cache TTL 1h serialization.
   - Malformed tool JSON argument rejection on block stop (`InvalidToolArguments`).
   - Empty tool use ID rejection on block start (`MissingStreamIdentity`).
   - Empty tool use name rejection on block start (`MissingStreamIdentity`).
2. **Request Translation (`tests/anthropic_request_tests.rs`)** - 15 tests:
   - Basic request translation and `max_tokens` verification.
   - `max_tokens == 0` validation.
   - Instructions and leading system/developer message extraction.
   - Fail-closed rejection of interleaved system messages.
   - Inline base64 image data URL parsing and validation.
   - Unsupported image MIME type rejection (`image/bmp`).
   - Grouped parallel tool result coalescing into single user message.
   - Function call arguments JSON validation.
   - Tool naming regex validation (`^[a-zA-Z0-9_-]{1,64}$`).
   - Security rejection of `access_programs`.
   - Tool choice mapping (`auto`, `none`, `required`, specific tool name).
   - Reasoning effort mapping across all five tiers (`Low` through `Max`) and rejection of unsupported tiers (`Ultra`).
   - Structured output format mapping (`JsonSchema`).
   - Caching policies (`AutomaticBreakpoint`) and thinking policies (`LegacyBudgetTokens`).
   - Continuation state thinking block and signature restoration, plus cross-provider reasoning warnings.
3. **Stream Translation (`tests/anthropic_stream_tests.rs`)** - 10 tests:
   - Complete text streaming happy path with effective input usage calculation and cache details.
   - Stop reason turn continuation mapping (`tool_use` -> `end_turn: false`, `end_turn` -> `end_turn: true`).
   - Fail-closed truncation and limits (`max_tokens`, `model_context_window_exceeded`, `refusal`).
   - Tool call streaming with fragmented `input_json_delta`.
   - Extended thinking and signature state preservation in `AnthropicContinuationState`.
   - Redacted thinking block preservation.
   - SSE ping handling and stream API error propagation.
   - Premature EOF with active unclosed blocks (`StreamOpenBlocksOnStop`).
   - State machine ordering violations (`StreamMessageNotStarted`, `StreamMessageAlreadyStarted`).
   - Negative token delta rejection.
4. **Multi-Turn Coding Agent Loop Roundtrip (`tests/anthropic_roundtrip_tests.rs`)** - 2 tests:
   - Complete multi-turn coding agent loop simulation: prompt -> thinking + tool use -> tool execution output -> continuation with cryptographic signature preservation -> final response.
   - Parallel tool call issuance and coalesced multi-block tool result handling.
