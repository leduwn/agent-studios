# Runtime Provider Transport Foundation

## 1. Overview & Architectural Role

The `agent-studios-runtime-transport` crate bridges Codex's agentic execution core (`codex-rs`) with multi-provider execution endpoints across heterogeneous wire protocols.

Prior milestones established pure in-memory protocol adapters (`agent-studios-protocol-adapters`) capable of translating between Codex Responses API semantics (`codex_api::ResponsesApiRequest`, `codex_api::ResponseEvent`) and external wire formats (OpenAI Chat Completions, Anthropic Messages, and Google Gemini `generateContent`). Milestone M07.5 implements the live network transport layer that resolves credentials securely, executes HTTP requests, decodes Server-Sent Events (SSE), manages multi-turn continuation state transactionally, and implements Codex's pluggable `ModelInferenceBackend` seam.

---

## 2. Pluggable Model Inference Seam in Codex

Codex upstream natively couples `ModelClient` to the OpenAI Responses API (`/v1/responses`). To allow Agent Studios to route requests to arbitrary model endpoints without altering Codex agent loop semantics, tool approval gates, conversation compaction, or telemetry pipelines, we introduced a minimal, default-off pluggable backend hook in `codex-rs`:

1. **`codex-model-provider` Seam**:
   - `ModelInferenceBackend`: An asynchronous trait accepting `ResponsesApiRequest` and `ModelInferenceContext` (`thread_id`, `turn_id`), returning `Result<ResponseStream, ApiError>`.
   - `ModelProvider`: Houses an optional `inference_backend: Option<Arc<dyn ModelInferenceBackend>>`. Defaults to `None`.
   - When `inference_backend` is `None`, Codex operates 100% identically to upstream.

2. **`codex-core` Client Dispatch**:
   - `ModelClient::stream_custom_inference`: Dispatches through `provider.inference_backend` when present.
   - Response streams returned by the custom backend enter the exact same Codex interceptor, debug context, output truncation, and event processing channels.

All modifications to `codex-rs` are documented with patch rationales and upstream sync procedures in `docs/agent-studios/CODEX_PATCHES.md`.

---

## 3. Runtime Transport Crate Architecture

The `agent-studios-runtime-transport` crate is organized into modular components:

```
agent-studios-runtime-transport/
├── src/
│   ├── auth.rs           # Authentication resolution & header/query parameter generation
│   ├── drivers/          # Wire protocol network execution drivers
│   │   ├── anthropic.rs  # Anthropic Messages driver (POST /v1/messages)
│   │   ├── chat_completions.rs # OpenAI Chat Completions driver (POST /chat/completions)
│   │   ├── gemini.rs     # Gemini generateContent driver (POST /models/{model}:streamGenerateContent)
│   │   └── mod.rs
│   ├── error.rs          # Strongly-typed transport error taxonomy
│   ├── lib.rs            # Public API exports
│   ├── router.rs         # RuntimeRouter implementing ModelInferenceBackend
│   ├── secret.rs         # Zeroized SecretString & asynchronous SecretResolver
│   ├── sse.rs            # Incremental SSE parser and byte-stream adapter
│   └── state.rs          # Transactional continuation state machine
└── tests/
    └── transport_mock_tests.rs # End-to-end Wiremock test suite
```

### 3.1. Zero-Plaintext Secret Management (`secret.rs`)
- `SecretString`: Secure container wrapping heap-allocated credential strings.
  - Implements `zeroize::ZeroizeOnDrop` to securely erase memory when dropped.
  - Custom `fmt::Debug`, `fmt::Display`, and `serde::Serialize` implementations that strictly output `"[REDACTED]"`.
  - Secret exposure is restricted to explicit calls to `.expose_secret()`.
- `SecretResolver`: Asynchronous trait resolving `SecretReference` into `SecretString`.
  - `InMemorySecretResolver`: Thread-safe in-memory map for scoped tests and dynamic ephemeral credentials.
  - `EnvSecretResolver`: Process environment resolver mapping `SecretBackend::EnvironmentVariable`.

### 3.2. Authentication Resolution (`auth.rs`)
- `ResolvedAuth`: Bridges `AuthenticationScheme` and resolved credentials to outbound HTTP requests:
  - `BearerToken`: Emits `Authorization: Bearer <secret>`.
  - `ApiKeyHeader`: Emits `<header_name>: <secret>`.
  - `QueryParameter`: Appends `?<param_name>=<secret>` to the request URL.
  - `AwsSigV4`: Fails closed until AWS request signing is implemented.

### 3.3. Incremental SSE Parser (`sse.rs`)
- `SseParser`: Reassembles arbitrary TCP byte chunks into discrete `SseEvent` items:
  - Strips leading `:` comments (keepalive pings).
  - Handles multi-line `data:` blocks coalesced with newlines.
  - Emits events on blank lines (`\n\n`).
  - Correctly flushes trailing unclosed frames upon stream termination (`finish()`).
- `SseStream`: Wraps `reqwest::Response::bytes_stream()` into a `Stream<Item = Result<SseEvent, TransportError>>`.

### 3.4. Transactional Multi-Turn Continuation State Machine (`state.rs`)
Multi-turn conversational loops with Anthropic and Gemini require tracking provider-native state across turns (e.g. `message_id`, thinking blocks, cryptographic signatures, candidate parts, and synthetic call IDs):
- `ContinuationManager`: Thread-safe registry mapping `thread_id` to `ThreadContinuationState`.
- `ContinuationTransaction`: Two-phase transactional unit:
  - Changes made during a streaming turn are staged in private memory.
  - When the stream successfully emits `ResponseEvent::Completed`, the driver calls `tx.commit()`, promoting staged state into the durable registry.
  - If a stream encounters a network error, HTTP error, malformed chunk, or cancellation, the transaction drops without committing, automatically rolling back and leaving thread state pristine.

### 3.5. Protocol Drivers (`drivers/`)
- `ChatCompletionsDriver`:
  - Translates `ResponsesApiRequest` via `ChatCompletionsAdapter`.
  - Sends `POST` to `/chat/completions`.
  - Iterates over SSE events, parsing chunks through `ChatCompletionStreamTranslator`.
  - Emits `ResponseEvent` sequence into channel.
- `AnthropicDriver`:
  - Translates `ResponsesApiRequest` via `AnthropicMessagesAdapter`.
  - Sends `POST` to `/messages`, ensuring `anthropic-version: 2023-06-01` header presence.
  - Streams Anthropic SSE events (`message_start`, `content_block_*`, `message_delta`, `message_stop`).
  - Replays thinking blocks and cryptographic signatures from staged continuation state.
- `GeminiDriver`:
  - Translates `ResponsesApiRequest` via `GeminiAdapter`.
  - Sends `POST` to `models/{model}:streamGenerateContent`.
  - Emits text deltas, reasoning deltas with thought signatures, and atomic function calls.
  - Replays candidate parts and thought signatures from staged continuation state.

### 3.6. Runtime Router (`router.rs`)
`RuntimeRouter` implements `codex_model_provider::ModelInferenceBackend`:
- Maintains active and registered `ProviderInstance` configurations.
- Resolves authentication schemes dynamically per request using the registered `SecretResolver`.
- Dispatches requests to the appropriate driver based on `ProtocolFamily`.
- Exposes clean cancellation propagation through `interrupt` channels.

---

## 4. Verification & Mock Testing

The test suite in `tests/transport_mock_tests.rs` validates all core behaviors against a `wiremock` HTTP server:
1. `test_chat_completions_wiremock`: Full streaming cycle from mock OpenAI `/chat/completions` endpoint emitting text deltas and completing.
2. `test_anthropic_wiremock`: Full streaming cycle from mock Anthropic `/messages` endpoint with thinking deltas and durable message ID persistence in `ContinuationManager`.
3. `test_gemini_wiremock`: Full streaming cycle from mock Gemini `:streamGenerateContent` endpoint with reasoning deltas and completion.
4. `test_transactional_rollback_on_error`: Verifies that an HTTP 500 failure leaves thread continuation state completely empty (guaranteed rollback).
5. `test_cancellation_propagation`: Verifies that triggering the `interrupt` channel immediately terminates the stream loop and cleans up resources.
6. `test_concurrent_thread_isolation`: Verifies that parallel requests on separate threads (`thread-iso-A`, `thread-iso-B`) maintain strict state isolation without cross-contamination.
