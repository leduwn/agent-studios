# Runtime Provider Transport Foundation

## 1. Overview & Architectural Role

The `agent-studios-runtime-transport` crate bridges Codex's agentic execution core (`codex-rs`) with multi-provider execution endpoints across heterogeneous wire protocols.

Prior milestones established pure in-memory protocol adapters (`agent-studios-protocol-adapters`) capable of translating between Codex Responses API semantics (`codex_api::ResponsesApiRequest`, `codex_api::ResponseEvent`) and external wire formats (OpenAI Chat Completions, Anthropic Messages, and Google Gemini `generateContent`). Milestones M07.5 and M07.5.1 implement and harden the live network transport layer that resolves credentials securely, executes HTTP requests, decodes Server-Sent Events (SSE) safely across byte boundaries, manages multi-turn continuation state transactionally, and implements Codex's pluggable `ModelInferenceBackend` seam.

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

## 3. Runtime Transport Architecture & Hardening (M07.5.1)

The `agent-studios-runtime-transport` crate is organized into modular, security-hardened components:

```
agent-studios-runtime-transport/
├── src/
│   ├── auth.rs           # Authentication resolution, sensitive headers, collision checks
│   ├── diagnostic.rs     # RuntimeDiagnosticSink for capability and runtime telemetry
│   ├── drivers/          # Wire protocol network execution drivers
│   │   ├── anthropic.rs  # Anthropic Messages driver (POST /v1/messages)
│   │   ├── chat_completions.rs # OpenAI Chat Completions driver (POST /chat/completions)
│   │   ├── gemini.rs     # Gemini generateContent driver (POST /models/{model}:streamGenerateContent)
│   │   └── mod.rs
│   ├── error.rs          # Strongly-typed transport error taxonomy & URL sanitization
│   ├── lib.rs            # Public API exports
│   ├── options.rs        # RuntimeTransportOptions, URL validation & bounded error bodies
│   ├── router.rs         # RuntimeRouter implementing ModelInferenceBackend & capability gating
│   ├── secret.rs         # Zeroized SecretString & asynchronous SecretResolver
│   ├── sse.rs            # Incremental byte-buffering SSE parser and stream adapter
│   └── state.rs          # ContinuationKey, RAII lease & transactional state machine
└── tests/
    └── transport_mock_tests.rs # 21 comprehensive WireMock integration tests
```

### 3.1. Zero-Plaintext Secret Management & Auth Security (`secret.rs`, `auth.rs`)
- `SecretString`: Secure container wrapping heap-allocated credential strings.
  - Implements `zeroize::ZeroizeOnDrop` to securely erase memory when dropped.
  - Custom `fmt::Debug`, `fmt::Display`, and `serde::Serialize` implementations that strictly output `"[REDACTED]"`.
  - Secret exposure is restricted to explicit calls to `.expose_secret()`.
- `SecretResolver`: Asynchronous trait resolving `SecretReference` into `SecretString`.
  - Rejects empty secrets with `TransportError::EmptySecret`.
  - Rejects unsupported backends with `TransportError::UnsupportedSecretBackend`.
  - `InMemorySecretResolver`: Thread-safe in-memory map for scoped tests and dynamic ephemeral credentials.
  - `EnvSecretResolver`: Process environment resolver mapping `SecretBackend::EnvironmentVariable`.
- `ResolvedAuth`:
  - Custom `Debug` implementation redacting all credentials.
  - Marks outbound credential headers sensitive (`HeaderValue::set_sensitive(true)`).
  - Performs pre-flight case-insensitive collision detection against static headers and query parameters (`check_collisions`).
  - Safe error sanitization (`sanitize_error_message`) redacting sensitive query parameters (e.g. `key=...`) from reqwest errors.

### 3.2. Provider-Instance-Scoped Continuation & Concurrency Guards (`state.rs`)
- `ContinuationKey`: Composite struct `ContinuationKey { provider_instance_id: ProviderInstanceId, thread_id: String }` ensuring multi-turn session continuation state is fully isolated per provider instance, preventing cross-provider state leakage.
- `ContinuationLease`: RAII guard acquired during `begin_transaction`. If another inference request arrives for the same active `ContinuationKey`, it is deterministically rejected with `TransportError::ConcurrentThreadInference`. Releasing the lease occurs automatically on drop across all exit paths (commit, rollback, cancellation, receiver drop, or panic).
- `ContinuationTransaction`:
  - **Completed-Only Commit Invariant**: Turn changes (e.g., Anthropic message ID/thinking blocks, Gemini candidate parts/signatures) are staged in private memory. Durable commitment occurs strictly when `ResponseEvent::Completed` is received.
  - **Rollback on Early EOF**: If a stream terminates (EOF) without emitting a completion event, staged changes are discarded and a typed stream error (`ApiError::Stream`) is emitted.
  - **Commit Error Propagation**: Errors from `tx.commit()` are never swallowed (`let _ = tx.commit()`); commit errors immediately emit a stream error and abort the stream.

### 3.3. Capability Gating & Diagnostic Sink (`router.rs`, `diagnostic.rs`)
- Route resolution extracts both the `ProviderInstance` and its associated `ModelDescriptor`.
- Request capabilities (streaming, tool calling, parallel tool calling) are validated against `ModelDescriptor.capabilities`. Unsupported features fail closed with `TransportError::UnsupportedCapability`.
- Unknown features emit non-blocking diagnostic notifications via `RuntimeDiagnosticSink`.
- Anthropic requests require `ModelDescriptor.limits.max_output_tokens` and fail closed with `TransportError::MissingRequiredModelLimit` if unspecified.

### 3.4. Bounded Resource Safety & Network Policy (`options.rs`)
- `RuntimeTransportOptions`:
  - Configurable timeouts (`connect_timeout`, `request_timeout`, `stream_idle_timeout`).
  - Limits on buffer sizes (`max_error_body_bytes`, `max_sse_event_bytes`).
  - `validate_url`: Rejects insecure remote HTTP endpoints (`InsecureRemoteHttpRejected`) unless loopback/localhost or explicitly opted into via `allow_insecure_remote_http: true`.
- `read_bounded_error_body`: Reads upstream HTTP error responses up to `max_error_body_bytes` to prevent unbounded memory consumption on faulty or malicious upstreams.
- Gemini model slugs are safely percent-encoded in request URLs.

### 3.5. Byte-Safe Incremental SSE Parser (`sse.rs`)
- `SseParser`:
  - Buffers raw bytes (`Vec<u8>`) rather than decoding byte chunks into UTF-8 strings before framing.
  - Splits frames on newline markers (`\n`), ensuring multibyte UTF-8 sequences (e.g. emojis, non-ASCII characters) split across TCP chunks are preserved intact.
  - Enforces `max_event_bytes` on accumulated frame size, rejecting oversized frames with `TransportError::SseFrameTooLarge`.
  - Handles comments, multi-line data blocks, and flushes trailing unclosed frames upon completion.

---

## 4. Verification & Mock Testing

The test suite in `tests/transport_mock_tests.rs` validates all 14 correctness and security behaviors against a `wiremock` HTTP server:
1. `test_chat_completions_wiremock`: Full streaming cycle from mock OpenAI `/chat/completions` endpoint.
2. `test_anthropic_wiremock`: Full streaming cycle from mock Anthropic `/messages` endpoint.
3. `test_gemini_wiremock`: Full streaming cycle from mock Gemini `:streamGenerateContent` endpoint.
4. `test_transactional_rollback_on_error`: HTTP 500 error triggers automatic rollback leaving thread continuation state clean.
5. `test_cancellation_propagation`: Triggering `interrupt` terminates stream loop cleanly.
6. `test_concurrent_thread_isolation`: Distinct threads maintain isolated state.
7. `test_cross_instance_continuation_key_isolation`: Same thread ID on different provider instances maintains independent continuation state.
8. `test_same_thread_concurrency_rejection`: Concurrent inference targeting the same `ContinuationKey` is rejected with `ConcurrentThreadInference`.
9. `test_completed_only_commit_invariant`: Premature stream termination rolls back staged continuation state.
10. `test_anthropic_max_tokens_fail_closed`: Missing max output tokens fails closed.
11. `test_routing_with_model_descriptor`: Explicit model descriptor routing.
12. `test_capability_gating_unsupported_rejected_and_unknown_warning`: Unsupported capabilities rejected; unknown features warn via sink.
13. `test_sensitive_header_marking`: Outbound auth headers marked sensitive.
14. `test_auth_collision_rejection`: Collisions between auth headers and static headers rejected.
15. `test_empty_secret_rejection`: Empty secrets rejected fail-closed.
16. `test_safe_error_mapping_redacts_keys`: Credentials redacted from error messages.
17. `test_sse_split_multibyte_utf8`: Multibyte UTF-8 split across SSE frames decoded without corruption.
18. `test_sse_frame_size_limit`: Oversized SSE frames rejected with `SseFrameTooLarge`.
19. `test_bounded_error_body_truncation`: Upstream error bodies truncated to limit.
20. `test_remote_http_rejection_and_loopback_allowed`: Insecure remote HTTP rejected; localhost HTTP accepted.
21. `test_gemini_url_encoding`: Model slugs with special characters correctly percent-encoded.
