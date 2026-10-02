# OpenAI Chat Completions Protocol Adapter (Milestone M05)

## Overview

The `agent-studios-protocol-adapters` crate provides a pure protocol translation layer between OpenAI Codex Responses API semantics (`codex_api::ResponsesApiRequest`, `codex_api::ResponseEvent`) and the industry-standard OpenAI Chat Completions wire protocol (`POST /v1/chat/completions`).

This protocol adapter enables Agent Studios to target any Chat Completions-compatible upstream endpoint—including local model runners (Ollama, vLLM, LocalAI), third-party gateways (Groq, DeepSeek, OpenRouter, Together AI, Mistral), and private enterprise proxies—without modifying upstream Codex runtime core or coupling to specific vendor brands.

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
│             agent-studios-protocol-adapters                 │
│  ┌────────────────────────┐    ┌──────────────────────────┐ │
│  │   Request Translator   │    │ Stream Event Translator  │ │
│  │  (translate_request)   │    │ (StreamTranslator FSM)   │ │
│  └───────────┬────────────┘    └────────────▲─────────────┘ │
└──────────────┼──────────────────────────────┼───────────────┘
               │                              │
    ChatCompletionRequest          ChatCompletionChunk / SSE
               │                              │
               ▼                              │
┌─────────────────────────────────────────────────────────────┐
│         Standard /v1/chat/completions Wire Protocol         │
│         (Ollama, vLLM, DeepSeek, Groq, 9Router, etc.)       │
└─────────────────────────────────────────────────────────────┘
```

---

## Architectural Invariants

1. **Pure Protocol Translation (Zero Network Transport)**:
   - Does not perform HTTP requests.
   - Does not depend on `reqwest`, `hyper`, or an asynchronous Tokio runtime.
   - Operates entirely synchronously on in-memory Rust data structures and strings.
2. **Zero Secret Handling**:
   - Does not consume or resolve API keys, bearer tokens, or environment variables.
   - Authentication and credential injection remain isolated in the HTTP client / transport layers.
3. **Zero Provider Brand Branching**:
   - Logic does not branch on vendor names or IDs (`openai`, `groq`, `deepseek`, `ollama`).
   - Translates strictly against the canonical OpenAI Chat Completions wire specification.
4. **Upstream Codex Isolation**:
   - `codex-rs/` is not modified (`git diff main -- codex-rs` is completely empty).
   - All adapter code resides in `agent-studios-rs/crates/protocol-adapters`.
5. **No Reverse Dependencies**:
   - Neither `agent-studios-provider` nor `agent-studios-control-plane` depends on this crate.

---

## Request Translation (`translate_request`)

The `translate_request` function converts a Codex `ResponsesApiRequest` into a `ChatCompletionRequest` wrapped in a `ChatRequestTranslation` bundle that includes typed warnings for unmappable optional fields.

### Key Behaviors:

- **Model Preservation**: Passes the target `model` identifier unchanged.
- **Instruction Mapping**: Extracts Codex `instructions` and injects them as the leading message with `role: "system"`.
- **Developer Role Normalization**: Codex input messages with `role: "developer"` are normalized to `role: "system"` to ensure broad proxy and model compatibility.
- **Message Content**:
  - `ContentItem::InputText` maps directly to message text content.
  - Multimodal inputs: `ContentItem::InputImage` with inline `data:` URIs maps to `ChatContentPart::ImageUrl` with optional detail (`low`, `high`, `auto`).
  - Remote file IDs (`ImageReference::File`) and audio inputs (`ContentItem::InputAudio`) are safely rejected with `ChatAdapterError::UnsupportedContent`.
- **Tool Calls and Results Correlation**:
  - `ResponseItem::FunctionCall` maps to assistant message `tool_calls` with ID and function arguments.
  - `ResponseItem::FunctionCallOutput` maps to `role: "tool"` messages with mandatory `tool_call_id`. Missing `call_id` returns `ChatAdapterError::MissingToolCallId`.
- **Tool Definition Conversion**:
  - Transforms Responses flat function schemas (`{ "type": "function", "name": ..., "description": ..., "parameters": ..., "strict": ... }`) into Chat nested definitions (`{ "type": "function", "function": { "name", "description", "parameters", "strict" } }`).
  - Non-function tools (such as Responses-specific `web_search`) are rejected with `ChatAdapterError::UnsupportedToolType`.
- **Tool Choice Mapping**: Maps `"none"`, `"auto"`, `"required"`, or custom function names to `ChatToolChoice`.
- **Structured Output**: Maps Codex `TextControls.format` (`json_schema`) to `ChatResponseFormat::json_schema` with strict validation.
- **Fail-Closed Security & Role Validation**:
  - If `access_programs` is present, translation fails immediately with `ChatAdapterError::UnsupportedSecurityFeature`.
  - Encrypted `AgentMessage` items are rejected with `ChatAdapterError::UnsupportedEncryptedAgentMessage`.
  - Unrecognized `ResponseItem` variants fail closed with `ChatAdapterError::UnsupportedResponseItem`.
  - Roles must be one of `user`, `assistant`, `system`, or `developer` (normalized to `system`); unknown roles return `ChatAdapterError::InvalidRole`.
  - `tool_choice` must be empty, `"auto"`, `"none"`, or `"required"`; other strings return `ChatAdapterError::UnsupportedToolChoice`.
  - Lossless tool output verification: `FunctionCallOutputBody::ContentItems` containing non-text items (`InputImage`, `InputAudio`, `EncryptedContent`) return `ChatAdapterError::UnsupportedToolOutputContent`.
- **Warning Model for Dropped or Normalized Fields**:
  - Emits typed `ChatAdapterWarning` variants for:
    - `reasoning` (e.g. reasoning effort controls)
    - `prompt_cache_key`
    - `include`
    - `client_metadata`
    - `store`
    - `service_tier`
    - `verbosity`
    - `NormalizedImageDetail` (when `ImageDetail::Original` is normalized to `"high"`)

---

## Streaming Event State Machine (`ChatCompletionStreamTranslator`)

The `ChatCompletionStreamTranslator` reconstructs streaming `ResponseEvent` sequences from incremental `ChatCompletionChunk` SSE payloads.

### Lifecycle Events Emitted:

1. **`ResponseEvent::Created`**:
   - Emitted exactly once upon receiving the first chunk containing the completion ID.
2. **`ResponseEvent::OutputItemAdded` (Message)**:
   - Emitted upon the first non-empty text content delta.
3. **`ResponseEvent::OutputTextDelta`**:
   - Emitted for each non-empty text delta chunk.
4. **`ResponseEvent::OutputItemDone` (Message)**:
   - Emitted when message text streaming completes (either when tool calls begin, or upon finish reason `stop`).
5. **Tool Call Lifecycle**:
   - Supports fragmented and interleaved tool calls across multiple tool indices.
   - Emits `ResponseEvent::OutputItemAdded` (FunctionCall) when a tool's `call_id` and `name` are first observed.
   - Emits `ResponseEvent::ToolCallInputDelta` for each incremental argument string fragment.
   - Emits `ResponseEvent::OutputItemDone` (FunctionCall) with fully accumulated JSON arguments when the tool concludes.
6. **`ResponseEvent::Completed`**:
   - Emitted on stream termination (`finish` or `[DONE]`), mapping token usage statistics (`prompt_tokens`, `completion_tokens`, `cached_tokens`, `total_tokens`) into `codex_protocol::protocol::TokenUsage`.

### Safety Guardrails:

- **Single Choice Constraint**: Rejects any chunk with `choice.index != 0` via `ChatAdapterError::MultipleChoicesUnsupported`.
- **Finish Reason Validation**:
  - `stop` and `tool_calls`: Handled normally.
  - `length`: Fails with `ChatAdapterError::StreamIncomplete`.
  - `content_filter`: Fails with `ChatAdapterError::ContentFilterTriggered`.
  - Unexpected reasons: Fails with `ChatAdapterError::UnexpectedFinishReason`.
- **Codex Continuation & Turn Lifecycle**:
  - `finish_reason == "tool_calls" | "function_call"` sets `ResponseEvent::Completed.end_turn = Some(false)` so Codex triggers follow-up tool execution.
  - `finish_reason == "stop"` sets `end_turn = Some(true)`.
  - Reaching completion without an explicit successful finish reason fails with `ChatAdapterError::MissingFinishReason`.
- **Response & Model Continuity**:
  - Initial chunk must provide a valid non-empty response ID (`ChatAdapterError::MissingResponseId`).
  - Subsequent chunks must match the established response ID (`ChatAdapterError::ResponseIdMismatch`).
  - Model continuity is enforced across chunks (`ChatAdapterError::ResponseModelMismatch`).
- **Strict Terminal State**:
  - Calling `feed_chunk`, `finish`, or `feed_done` after completion returns `ChatAdapterError::AlreadyCompleted`.
- **No Synthetic Tool Identity & Fragment Buffering**:
  - Does not synthesize fallback IDs or empty function names.
  - Arguments arriving before `id` and `name` are buffered and flushed in original order once identity arrives.
  - Incomplete tool calls at stream finish return `ChatAdapterError::IncompleteToolCall`.
  - Tool identity is immutable (`ChatAdapterError::ToolCallIdentityMismatch`).
  - Tool types other than `"function"` fail with `ChatAdapterError::UnsupportedToolCallType`.
- **Usage & Stream Validation**:
  - Negative token counts fail with `ChatAdapterError::InvalidUsage`.
  - Resuming text streaming after tool execution has begun fails with `ChatAdapterError::InvalidStreamState`.
- **Deterministic Synthetic IDs**:
  - Synthesizes item IDs deterministically:
    - Messages: `chat-msg-<response_id>`
    - Tool Calls: `chat-tool-<response_id>-<index>`
- **SSE Utilities**:
  - `decode_sse_line`: Decodes SSE `data:` payloads into `ChatCompletionChunk`, safely ignoring comments (`: ...`), whitespace, and `[DONE]`.
  - `is_done_line`: Identifies `data: [DONE]` terminal markers.

---

## Verification & Test Suites

The crate contains 47 automated tests covering request translation, streaming reconstruction, error enforcement, roundtrip workflows, and M05.1 correctness hardening:

1. `tests/request_translation_tests.rs` (11 tests):
   - Instructions and developer role normalization.
   - Multimodal image translation and rejection of file/audio inputs.
   - Security rejection of `access_programs`.
   - Tool schema conversion (flat to nested format).
   - Tool call and output correlation (`tool_call_id`).
   - Missing tool call ID rejection.
   - Structured output (`json_schema`) mapping.
   - Dropped Responses-only fields warning assertions.
   - Tool choice mapping.
2. `tests/stream_translation_tests.rs` (5 tests):
   - Basic text streaming lifecycle.
   - Interleaved and fragmented tool calls streaming.
   - Length and content filter finish reason rejections.
   - Multiple choices constraint enforcement.
   - SSE line decoding and comment handling.
3. `tests/roundtrip_semantic_tests.rs` (1 test):
   - End-to-end integration test translating a full request and streaming back simulated SSE chunks with fragmented tool call execution, continuation semantics (`end_turn: Some(false)`), and token usage.
4. `tests/correctness_hardening_tests.rs` (30 tests):
   - Comprehensive regression coverage for all M05.1 correctness requirements (finish before chunk, response ID continuity, model continuity, terminal state guards, tool identity buffering/immutability/validation, lossless tool output, fail-closed message roles, exactly-once item lifecycle).
