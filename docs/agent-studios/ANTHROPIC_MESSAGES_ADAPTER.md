# Anthropic Messages Protocol Adapter (Milestone M06)

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
6. **No Reverse Dependencies**:
   - Neither `agent-studios-provider` nor `agent-studios-control-plane` depends on this crate.

---

## Request Translation (`translate_request`)

The `translate_request` function converts a Codex `ResponsesApiRequest` into an `AnthropicMessagesRequest` wrapped in an `AnthropicRequestTranslation` bundle with typed warnings.

### Key Behaviors:

- **Model & Token Configuration**:
  - Passes the target `model` identifier unchanged.
  - Configures `max_tokens` from `AnthropicRequestOptions`. Rejects `max_tokens == 0` with `AnthropicAdapterError::InvalidMaxTokens(0)`.
- **System Instructions & Role Placement**:
  - Extracts Codex `instructions` and leading `system`/`developer` messages into top-level system text blocks (`AnthropicSystemBlock::Text`).
  - **Interleaved System Rejection**: If a `system` or `developer` message appears *after* conversational turns (`user` or `assistant`), translation fails closed with `AnthropicAdapterError::UnsupportedSystemHistoryPlacement`.
- **Message Content & Multimodal Inputs**:
  - `ContentItem::InputText` and `ContentItem::OutputText` map to `AnthropicContentBlock::Text`.
  - Multimodal inputs: `ContentItem::InputImage` with inline `data:` URIs are parsed and validated against supported MIME types (`image/jpeg`, `image/png`, `image/gif`, `image/webp`). Unsupported MIME types fail closed with `AnthropicAdapterError::UnsupportedImageMime`.
  - Remote file IDs (`ImageReference::File`) and audio inputs (`ContentItem::InputAudio`) fail closed with `AnthropicAdapterError::UnsupportedContent`.
- **Tool Calls & Grouped Tool Results**:
  - `ResponseItem::FunctionCall` and `ResponseItem::CustomToolCall` map to `AnthropicContentBlock::ToolUse` in assistant messages.
  - Consecutive `FunctionCallOutput` and `CustomToolCallOutput` items are coalesced into a single `role: "user"` message containing multiple `AnthropicContentBlock::ToolResult` blocks.
  - `tool_result` status: maps `FunctionCallOutputPayload.success` to `is_error` (`Some(!success)`).
  - Rejects missing `call_id` with `AnthropicAdapterError::MissingToolCallId`.
- **Tool Definitions & Naming Validation**:
  - Translates function schemas into Anthropic tool definitions (`AnthropicTool`).
  - Validates tool naming rules against Anthropic regex `^[a-zA-Z0-9_-]{1,64}$`.
  - Rejects non-function tools with `AnthropicAdapterError::UnsupportedToolType`.
- **Tool Choice Mapping**:
  - `"auto"` -> `AnthropicToolChoice::Auto`.
  - `"none"` -> omits tools and tool_choice from request.
  - `"required"` -> `AnthropicToolChoice::Any`.
  - Named function -> `AnthropicToolChoice::Tool { name }`.
  - Propagates `disable_parallel_tool_use` when `parallel_tool_calls` is `false`.
  - If a forced tool name is not present in defined tools, emits `AnthropicAdapterWarning::ForcedToolChoiceUnverified`.
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
- **Prompt Caching Policies**:
  - Supports `AnthropicPromptCachePolicy`:
    - `None`: No cache breakpoints injected.
    - `LastUserMessage`: Ephemeral cache control on last user content block.
    - `ToolsAndSystem`: Ephemeral cache control on last system block and last tool definition.
    - `AutomaticBreakpoint`: Caches both system/tools and last user message.
- **Thinking / Extended Reasoning**:
  - Supports `AnthropicThinkingPolicy`:
    - `Disabled`: Thinking omitted.
    - `Adaptive`: Emits `{ "type": "adaptive" }`.
    - `BudgetTokens(tokens)`: Emits `{ "type": "enabled", "budget_tokens": tokens }`.
  - Reconstitutes native thinking blocks from `AnthropicContinuationState` during multi-turn continuation. Missing continuation state for an Anthropic reasoning item fails closed with `AnthropicAdapterError::MissingContinuationState`. Cross-provider reasoning items are omitted with `AnthropicAdapterWarning::CrossProviderReasoningOmitted`.
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
   │ message_delta (stop_reason, usage)
   │ message_stop
   ▼
[Completed] (ResponseEvent::Completed)
```

### Event Translation Matrix:

| Anthropic SSE Event | State Transition / Action | Codex `ResponseEvent` Emitted |
| :--- | :--- | :--- |
| `message_start` | Initializes response ID, model, input token usage | `ResponseEvent::Created`, `ResponseEvent::ServerModel` |
| `content_block_start` (`text`) | Opens text block | `ResponseEvent::OutputItemAdded(ResponseItem::Message)` |
| `content_block_start` (`tool_use`) | Opens tool use block | `ResponseEvent::OutputItemAdded(ResponseItem::FunctionCall)` |
| `content_block_start` (`thinking`) | Opens thinking block | `ResponseEvent::OutputItemAdded(ResponseItem::Reasoning)` |
| `content_block_start` (`redacted_thinking`) | Stores redacted data block | `ResponseEvent::OutputItemAdded(ResponseItem::Reasoning)` |
| `content_block_delta` (`text_delta`) | Accumulates text | `ResponseEvent::OutputTextDelta` |
| `content_block_delta` (`input_json_delta`) | Accumulates arguments | `ResponseEvent::ToolCallInputDelta` |
| `content_block_delta` (`thinking_delta`) | Accumulates thinking | `ResponseEvent::ReasoningContentDelta` |
| `content_block_delta` (`signature_delta`) | Buffers cryptographic signature | *Internal continuation buffer* |
| `content_block_stop` | Closes block, persists continuation state | `ResponseEvent::OutputItemDone` |
| `message_delta` | Captures `stop_reason` and output token usage | *None (buffered)* |
| `message_stop` | Completes stream, computes `TokenUsage` | `ResponseEvent::Completed { response_id, token_usage, end_turn }` |
| `ping` | Keepalive | *None (ignored)* |
| `error` | Stream error | Fails with `AnthropicAdapterError::StreamApiError` |

### Turn Continuation & Stop Reasons:

- `stop_reason: "tool_use"` -> `end_turn: Some(false)`: Signals Codex agent runtime that tool calls were issued and the turn must continue after tool execution.
- `stop_reason: "end_turn"` or `"stop_sequence"` -> `end_turn: Some(true)`: Turn completed normally.
- `stop_reason: "max_tokens"` -> Fails closed with `AnthropicAdapterError::MaxTokensExceeded`.
- `stop_reason: "model_context_window_exceeded"` -> Fails closed with `AnthropicAdapterError::ContextWindowExceeded`.
- `stop_reason: "refusal"` -> Fails closed with `AnthropicAdapterError::ModelRefusal`.
- `stop_reason: "pause_turn"` -> Fails closed with `AnthropicAdapterError::TurnPaused`.

---

## Test Verification

The adapter includes 27 dedicated integration tests across three test suites:

1. **Request Translation (`tests/anthropic_request_tests.rs`)** - 15 tests:
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
   - Caching policies (`AutomaticBreakpoint`) and thinking policies (`BudgetTokens`).
   - Continuation state thinking block and signature restoration, plus cross-provider reasoning warnings.
2. **Stream Translation (`tests/anthropic_stream_tests.rs`)** - 10 tests:
   - Complete text streaming happy path with usage accumulation and cache details.
   - Stop reason turn continuation mapping (`tool_use` -> `end_turn: false`, `end_turn` -> `end_turn: true`).
   - Fail-closed truncation and limits (`max_tokens`, `model_context_window_exceeded`, `refusal`).
   - Tool call streaming with fragmented `input_json_delta`.
   - Extended thinking and signature state preservation in `AnthropicContinuationState`.
   - Redacted thinking block preservation.
   - SSE ping handling and stream API error propagation.
   - Premature EOF with active unclosed blocks (`StreamOpenBlocksOnStop`).
   - State machine ordering violations (`StreamMessageNotStarted`, `StreamMessageAlreadyStarted`).
   - Negative token delta rejection.
3. **Multi-Turn Coding Agent Loop Roundtrip (`tests/anthropic_roundtrip_tests.rs`)** - 2 tests:
   - Complete multi-turn coding agent loop simulation: prompt -> thinking + tool use -> tool execution output -> continuation with cryptographic signature preservation -> final response.
   - Parallel tool call issuance and coalesced multi-block tool result handling.
