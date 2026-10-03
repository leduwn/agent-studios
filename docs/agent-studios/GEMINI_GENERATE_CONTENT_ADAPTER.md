# Gemini generateContent Protocol Adapter (Milestones M07 & M07.1)

## Overview

The `agent-studios-protocol-adapters::gemini` module provides a pure, deterministic protocol translation layer between OpenAI Codex Responses API semantics (`codex_api::ResponsesApiRequest`, `codex_api::ResponseEvent`) and the Google Gemini `generateContent` / `streamGenerateContent` wire protocol, with M07.1 continuation semantics and cryptographic thought signature hardening.

This protocol adapter enables Agent Studios to target any Google Gemini API-compatible endpoint—including Google AI Studio (`generativelanguage.googleapis.com`), Google Cloud Vertex AI (`us-central1-aiplatform.googleapis.com`), and enterprise proxies/gateways—without modifying the upstream Codex runtime core or coupling to specific vendor brands.

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
│       agent-studios-protocol-adapters::gemini               │
│  ┌────────────────────────┐    ┌──────────────────────────┐ │
│  │   Request Translator   │    │ Stream Event Translator  │ │
│  │  (translate_request)   │    │ (GeminiStreamTranslator) │ │
│  └───────────┬────────────┘    └────────────▲─────────────┘ │
│              │                              │               │
│              │    GeminiContinuationState   │               │
│              │ (turns, call IDs, signatures,│               │
│              │  item/call origin references)│               │
│              └──────────────┬───────────────┘               │
└─────────────────────────────┼───────────────────────────────┘
                              │
  GeminiGenerateContentRequest│ GeminiGenerateContentResponse
  (endpoint path: {model})    │ (SSE chunks / JSON stream)
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│         Standard Gemini generateContent Wire Protocol       │
│       (Google AI Studio, Vertex AI, Enterprise Proxies)     │
└─────────────────────────────────────────────────────────────┘
```

---

## Architectural Invariants

1. **Pure Protocol Translation (Zero Network Transport)**:
   - Does not perform HTTP requests.
   - Does not depend on `reqwest`, `hyper`, an asynchronous Tokio runtime networking stack, or Google Gemini SDKs.
   - Operates entirely synchronously on in-memory Rust data structures and strings.
2. **Zero Secret Handling**:
   - Does not consume, parse, or inject API keys, Google Cloud IAM tokens, or OAuth credentials.
   - Credential injection remains strictly isolated in external transport/client layers.
3. **Preserved Path-Based Model Parameter**:
   - Unlike OpenAI or Anthropic protocols where the model name is part of the JSON request body, Gemini wire endpoints specify the model name in the URL path (`v1beta/models/{model}:generateContent`).
   - The adapter returns a `GeminiRequestTranslation` bundle containing `model: String`, `request: GeminiGenerateContentRequest`, and `warnings: Vec<GeminiAdapterWarning>`.
   - The adapter preserves the target model identifier verbatim (e.g. `gemini-2.5-pro`, `gemini-2.5-flash`) without adding a `models/` prefix.
4. **Authoritative Provider-Native Turn Replay (M07.1 Hardening)**:
   - When a model turn originates from Gemini, `GeminiContinuationState` stores the exact raw `GeminiContent` turn verbatim.
   - Multi-turn conversation continuation replays this stored native turn directly rather than reconstructing it from lossy Codex `ResponseItem` representations.
   - Preserves exact Part ordering, thought signatures on original parts, and lossless extra metadata (`#[serde(flatten)] extra: BTreeMap<String, Value>`).
   - Reverse origin association tracks `GeminiOriginRef { response_id, candidate_index, part_index }` from Codex item IDs and function call IDs back to native response turns.
   - Model turns are replayed exactly once per `response_id`: subsequent Codex items from the same response are skipped to prevent duplicate turn emission.
5. **Thought Signature Preservation**:
   - Gemini thinking models emit opaque cryptographic thought signatures on thinking parts, text parts, and function call parts.
   - These signatures are captured and passed back verbatim during multi-turn continuations.
   - Signatures are preserved without decoding, trimming, normalization, re-encoding, or modification.
   - Both `ActiveText` and `ActiveReasoning` blocks record signatures into continuation state.
6. **Deterministic Call ID Generation and Wire Omission**:
   - Gemini function calls may omit an explicit call `id`.
   - When missing, the adapter generates a deterministic internal call ID (`gemini-call-{response_id}-{candidate_index}-{part_index}`) and records the metadata in `GeminiContinuationState`.
   - When the client returns a tool output for an omitted call ID, the wire `functionResponse` part omits the `id` field, preserving exact Gemini wire compatibility.
7. **Conservative Callable Tool Naming Profile**:
   - Validates function tool names against strict callable profile `^[a-zA-Z0-9_-]{1,128}$` (dots and colons excluded from callable tool set).
8. **Strict Tool Semantics & VALIDATED Mode**:
   - When any tool declaration specifies `strict: true`, the adapter promotes the Gemini function calling mode to `VALIDATED`.
   - If a mixed set of strict and non-strict tools is supplied, the entire set is promoted to `VALIDATED` and a typed warning `GeminiAdapterWarning::StrictToolScopePromoted` is emitted.
9. **Tool Choice "none" Preservation**:
   - When `tool_choice: "none"` is specified, the tool definitions in `request.tools` are retained while `functionCallingConfig.mode` is set to `NONE`.
10. **Sequential Tool Calling Rejection**:
    - Gemini generateContent operates with concurrent tool execution by default.
    - If `parallel_tool_calls: false` is requested while tools are active, translation fails closed with `GeminiAdapterError::SequentialToolCallingNotEnforceable`.
11. **Accurate Token Accounting with Thoughts**:
    - In Gemini's `UsageMetadata`, `promptTokenCount` already includes `cachedContentTokenCount`.
    - Maps `input_tokens = promptTokenCount`, `cached_input_tokens = cachedContentTokenCount`, and `output_tokens = candidatesTokenCount`.
    - `reasoning_output_tokens` maps directly to `thoughtsTokenCount`.
    - `total_tokens` maps to `totalTokenCount`, falling back to checked addition `prompt + candidates + thoughts` when omitted by upstream.
    - Provider-specific usage metadata is attached to `ResponseUsageMetadata.metadata` in `ResponseEvent::Completed`.
12. **Modern `responseFormat` Wire Format**:
    - Deprecated `responseSchema` is removed from `GeminiGenerationConfig`.
    - Modern `generationConfig.responseFormat.text = { mimeType, schema }` is emitted.
13. **Comprehensive Finish Reason Coverage**:
    - Complete fail-closed coverage for all Gemini terminal reasons: `MissingThoughtSignature`, `UnexpectedToolCall`, `TooManyToolCalls`, `MalformedResponse`, `ImageGenerationBlocked`, `FinishReasonUnspecified`, `MalformedFunctionCall`, `RecitationBlocked`, `UnsupportedLanguage`, `ContentBlocked`, and `SafetyBlocked`.
14. **Single Candidate Enforcement**:
    - Multi-candidate generation (`candidates.len() > 1`) is rejected fail-closed with `GeminiAdapterError::MultipleCandidatesUnsupported`.
15. **Turn Coalescing**:
    - Gemini requires alternating `user` and `model` turns.
    - Adjacent user messages and function outputs are coalesced into a single `user` Content turn containing multiple parts.
    - Adjacent model responses and function calls are coalesced into a single `model` Content turn.
16. **Upstream Codex Isolation**:
    - Zero modifications to `codex-rs/` (`git diff origin/main -- codex-rs` is completely empty).
    - All adapter code resides in `agent-studios-rs/crates/protocol-adapters`.
    - Zero modifications to `codex-rs/` (`git diff origin/main -- codex-rs` is completely empty).
    - All adapter code resides in `agent-studios-rs/crates/protocol-adapters`.

---

## Request Translation (`translate_request`)

The `translate_request` function transforms a Codex `ResponsesApiRequest` into a `GeminiGenerateContentRequest`.

### Translation Rules:

- **System Instructions**:
  - Top-level `instructions` and leading `system` or `developer` messages are extracted into `request.system_instruction`.
  - **Interleaved System Turns**: If a `system` or `developer` turn appears after conversational user/model turns, translation fails closed with `GeminiAdapterError::UnsupportedSystemHistoryPlacement`.
- **Role Mapping**:
  - `user` -> `user`
  - `assistant` -> `model`
  - Unrecognized roles fail closed with `GeminiAdapterError::UnsupportedRole`.
- **Multimodal Inputs**:
  - Supported inline images: `image/jpeg`, `image/png`, `image/gif`, `image/webp`. Maps to `GeminiPart::inline_data`.
  - Supported inline audio: `audio/wav`, `audio/mp3`, `audio/mpeg`, `audio/aac`, `audio/ogg`, `audio/flac`. Maps to `GeminiPart::inline_data`.
  - Remote file references (`ImageReference::File`) and external audio URLs fail closed with `GeminiAdapterError::UnsupportedContent`.
- **Tool Mapping**:
  - Validates function tool names against Gemini callable regex contract `^[a-zA-Z0-9_-]{1,128}$` (dots and colons excluded from callable tool set).
  - Rejects non-function tools with `GeminiAdapterError::UnsupportedToolType`.
  - Preserves JSON schema parameters in `GeminiFunctionDeclaration.parameters`.
- **Tool Choice Mapping**:
  - `""` -> `None` (omitted, or `VALIDATED` if strict tools present).
  - `"auto"` -> `Auto` (or `VALIDATED` if strict tools present).
  - `"none"` -> `None` (`functionCallingConfig.mode = "NONE"`, tool definitions preserved).
  - `"required"` -> `Any` (`functionCallingConfig.mode = "ANY"`).
  - Unrecognized strings fail closed with `GeminiAdapterError::UnsupportedToolChoice`.
- **Structured Outputs (Modern Wire Format)**:
  - Codex `TextControls.format` with `json_schema` translates to:
    - `generationConfig.responseMimeType = "application/json"`
    - `generationConfig.responseFormat.text = { mimeType: "application/json", schema: ... }`
    - Deprecated `responseSchema` field is omitted from `GeminiGenerationConfig`.
- **Thinking / Extended Reasoning**:
  - `GeminiThinkingPolicy::ExactReasoningEffort`:
    - `Minimal` -> `GeminiThinkingLevel::Minimal` (`"MINIMAL"`)
    - `Low` -> `GeminiThinkingLevel::Low` (`"LOW"`)
    - `Medium` -> `GeminiThinkingLevel::Medium` (`"MEDIUM"`)
    - `High` -> `GeminiThinkingLevel::High` (`"HIGH"`)
    - Non-standard tiers (`None`, `XHigh`, `Max`, `Ultra`, `Custom`) fail closed with `GeminiAdapterError::UnsupportedReasoningEffort`.
  - `GeminiThinkingPolicy::LegacyBudget(budget)`:
    - Sets `generationConfig.thinkingConfig.thinkingBudget = budget`.
- **Continuation State & Authoritative Turn Replay (M07.1)**:
  - `GeminiContinuationState` stores verbatim provider-native `GeminiContent` model turns emitted during generation.
  - Multi-turn conversation continuation replays this stored native turn directly, preserving exact Part ordering, thought signatures, and unknown/future fields (`#[serde(flatten)] extra: BTreeMap<String, Value>`).
  - Reverse origin association tracks `GeminiOriginRef { response_id, candidate_index, part_index }` from Codex item IDs and function call IDs.
  - Deduplicated turn replay: when encountering the first item belonging to a `response_id`, the full native turn is replayed once, and subsequent items belonging to the same `response_id` are skipped.
  - Missing continuation state for a Gemini reasoning item fails closed with `GeminiAdapterError::MissingContinuationState`.

---

## Streaming Event Translation (`GeminiStreamTranslator`)

The `GeminiStreamTranslator` parses SSE lines (`data: {...}`) and JSON response chunks from Gemini `streamGenerateContent`.

### Event Translation Matrix:

| Gemini Stream Payload | State Transition / Action | Codex `ResponseEvent` Emitted |
| :--- | :--- | :--- |
| First chunk with `responseId` | Initializes stream identity and model version | `ResponseEvent::Created`, `ResponseEvent::ServerModel` |
| Candidate part (`text`, `thought: false`) | Emits text delta, records item origin & thought signature | `OutputItemAdded(Message)` (if not open), `OutputTextDelta` |
| Candidate part (`text`, `thought: true`) | Emits reasoning delta, buffers thought signature | `OutputItemAdded(Reasoning)` (if not open), `ReasoningContentDelta` |
| Candidate part (`functionCall`) | Emits complete atomic function call, records ID mapping & origin | `OutputItemAdded(FunctionCall)`, `ToolCallInputDelta`, `OutputItemDone(FunctionCall)` |
| `finishReason: "STOP"` (no tools) | Completes turn | Closes open text/reasoning items, `Completed { end_turn: true }` |
| `finishReason: "STOP"` (with tools) | Completes turn awaiting tool outputs | `Completed { end_turn: false }` |
| `finishReason: "MAX_TOKENS"` | Truncation | Fails closed with `GeminiAdapterError::MaxTokensExceeded` |
| `finishReason: "SAFETY"` | Safety violation | Fails closed with `GeminiAdapterError::SafetyBlocked` |
| `finishReason: "RECITATION"` | Recitation check | Fails closed with `GeminiAdapterError::RecitationBlocked` |
| `finishReason: "LANGUAGE"` | Unsupported language | Fails closed with `GeminiAdapterError::UnsupportedLanguage` |
| `finishReason: "BLOCKLIST"` | Prohibited content | Fails closed with `GeminiAdapterError::ContentBlocked` |
| `finishReason: "UNEXPECTED_TOOL_CALL"` | Unexpected tool invocation | Fails closed with `GeminiAdapterError::UnexpectedToolCall` |
| `finishReason: "TOO_MANY_TOOL_CALLS"` | Tool call count exceeded | Fails closed with `GeminiAdapterError::TooManyToolCalls` |
| `finishReason: "MALFORMED_RESPONSE"` | Upstream malformed response | Fails closed with `GeminiAdapterError::MalformedResponse` |
| `finishReason: "MISSING_THOUGHT_SIGNATURE"` | Missing thought signature | Fails closed with `GeminiAdapterError::MissingThoughtSignature` |
| `finishReason: "IMAGE_SAFETY"` / `"IMAGE_GENERATION_BLOCKED"` | Image generation blocked | Fails closed with `GeminiAdapterError::ImageGenerationBlocked` |
| `finishReason: "FINISH_REASON_UNSPECIFIED"` | Unspecified finish reason | Fails closed with `GeminiAdapterError::FinishReasonUnspecified` |
| `promptFeedback.blockReason` | Prompt blocked | Fails closed with `GeminiAdapterError::PromptBlocked` |
| Candidate `safetyRatings[].blocked: true` | Safety rating block | Fails closed with `GeminiAdapterError::SafetyBlocked` |

### Token Usage Accounting:

Gemini reports token counts in `usageMetadata`:
- `promptTokenCount`: Total tokens in prompt (includes cached tokens).
- `cachedContentTokenCount`: Tokens read from prompt cache.
- `candidatesTokenCount`: Tokens generated in candidates.
- `thoughtsTokenCount`: Tokens generated in model thinking/reasoning parts.
- `totalTokenCount`: Total tokens (`promptTokenCount + candidatesTokenCount + thoughtsTokenCount`).

The adapter maps this into Codex `TokenUsage`:
$$\text{input\_tokens} = \text{promptTokenCount}$$
$$\text{cached\_input\_tokens} = \text{cachedContentTokenCount}$$
$$\text{cache\_write\_input\_tokens} = 0$$
$$\text{output\_tokens} = \text{candidatesTokenCount}$$
$$\text{reasoning\_output\_tokens} = \text{thoughtsTokenCount}$$
$$\text{total\_tokens} = \text{totalTokenCount} \text{ (fallback: checked addition of prompt + candidates + thoughts)}$$

The raw upstream JSON metadata is preserved verbatim in `ResponseUsageMetadata.metadata`. Negative token counts or arithmetic overflows fail closed with `GeminiAdapterError::InvalidUsage`.

---

## Test Verification Matrix

The test suite in `agent-studios-rs/crates/protocol-adapters` provides 100% coverage of Gemini adapter requirements across four test suites (63 tests total):

1. **`tests/gemini_request_tests.rs` (20 tests)**:
   - Basic request translation and options construction.
   - Max output tokens validation and non-zero checks.
   - System instruction extraction and interleaved turn rejection.
   - Conversational role mappings (`user`, `model`) and unsupported role fail-closed.
   - Multimodal inputs (inline JPEG, PNG, GIF, WebP, MP3, WAV).
   - Unsupported media rejection (BMP, TIFF, external file IDs, external audio URLs).
   - Parallel tool output coalescing into single user turn.
   - Tool output error payload mapping (`success: false` -> `{"error": ...}`).
   - Tool arguments JSON object validation.
   - Custom tool call structured input validation.
   - Tool choice `none` tool definition preservation.
   - Strict tool promotion to `VALIDATED` mode with warning.
   - Sequential tool calling rejection.
   - Security rejection of `access_programs`.
   - Structured outputs mapping to `generationConfig.responseFormat.text`.
   - Thinking policy mapping (`ExactReasoningEffort`, `LegacyBudget`) and reasoning summaries.
2. **`tests/gemini_stream_tests.rs` (15 tests)**:
   - Incremental text streaming happy path.
   - Effective prompt token accounting with cached tokens.
   - Tool call streaming and continuation recording.
   - Deterministic internal call ID generation when provider omits ID.
   - Extended thinking and thought reasoning stream with signature preservation.
   - Prompt feedback safety blocking fail-closed.
   - Finish reason `MAX_TOKENS` fail-closed.
   - Finish reason `SAFETY` fail-closed.
   - Multiple candidate rejection fail-closed.
   - Mid-stream response ID mismatch fail-closed.
   - Candidate safety rating blocked fail-closed.
   - Malformed function call arguments fail-closed.
   - Multi-line SSE chunk feed parsing.
   - Finish stream before terminal finish reason rejection.
   - Negative token usage fail-closed.
3. **`tests/gemini_roundtrip_tests.rs` (6 tests)**:
   - Multi-turn conversation continuation with dialogue history.
   - Multi-turn tool call and tool result roundtrip with thought signature preservation.
   - Deterministic call ID omitted on wire `functionResponse`.
   - Multi-tool output coalescing into a single user turn.
   - Strict tools roundtrip with `mode = VALIDATED`.
   - Thinking policy high roundtrip.
4. **`tests/gemini_hardening_tests.rs` (22 tests)**:
   - Tool name regex boundaries (1 char, 128 chars, 129 chars rejected, invalid characters).
   - Conservative callable tool naming rejecting dots and colons.
   - Invalid tool name fail-closed in translation.
   - Sequential tool calling rejection when tools are active vs allowed when no tools.
   - Interleaved system turn rejection fail-closed.
   - Max output tokens zero rejection and positive translation.
   - Mixed strict tools warning and promotion.
   - Exact reasoning effort mappings (Minimal, Low, Medium, High).
   - Unsupported reasoning effort fail-closed (`None`).
   - Terminal finish reasons fail-closed (`RECITATION`, `LANGUAGE`, `BLOCKLIST`, `SPII`).
   - Mid-stream model version mismatch fail-closed.
   - Opaque thought signature verbatim byte preservation.
   - Test 17: Exact provider-native roundtrip turn replay (authoritative verbatim replay, deduplication, extra fields preserved).
   - Test 18: Signed final text roundtrip (thought signature persisted across ActiveText and replayed).
   - Test 19: Native turn without visible output (empty thought part with signature retained).
   - Test 20: Usage accounting with thoughts (`reasoning_output_tokens = thoughtsTokenCount`, raw metadata preserved).
   - Test 21: Structured outputs exact JSON format (`generationConfig.responseFormat.text`).
   - Test 22: Complete finish reasons coverage (`MissingThoughtSignature`, `UnexpectedToolCall`, `TooManyToolCalls`, `MalformedResponse`, `ImageGenerationBlocked`, `FinishReasonUnspecified`).
