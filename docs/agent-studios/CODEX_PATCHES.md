# Codex Upstream Patches Registry

This document catalogs all intentional, minimal patches maintained inside the `codex-rs/` tree by Agent Studios.

## Upstream Baseline

- **Upstream Repository**: https://github.com/openai/codex
- **Baseline Commit**: `d25c114d494ddb693290b76bf5e5f64ecbdb38fc`
- **Lock Reference**: `upstream/codex.lock.json`

---

## Architectural Principles & Invariants

1. **Minimal & Provider-Neutral Seams Only**:
   - `codex-rs/` must never depend on Agent Studios crates (`agent_studios_provider`, `agent_studios_protocol_adapters`, `agent_studios_runtime_transport`, etc.).
   - No vendor-specific or provider-specific terms (e.g. Anthropic, Gemini, 9Router, OpenRouter, ChatCompletions) are permitted inside `codex-rs/`.
   - The seam exposes only generic interfaces (`ModelInferenceBackend`, `ModelInferenceContext`) using standard Codex API types (`ResponsesApiRequest`, `ResponseStream`, `ApiError`).

2. **Default-Off & Strictly Backwards-Compatible**:
   - Default implementations return `None` (no custom inference backend).
   - Existing Codex providers (OpenAI, Azure, Bedrock, etc.) operate byte-identically without source changes.
   - Standard `create_model_provider` factory preserves identical behavior.

3. **Preservation of Codex Agent Loop**:
   - The seam does NOT replace Codex's prompt builder, tool dispatch loop, approval flow, sandboxing, history, compaction, or response stream processing.
   - When a custom backend is active, Codex builds the identical semantic `ResponsesApiRequest` and processes the returned `ResponseEvent` stream through `map_response_stream`.

4. **Upstream Synchronization Revalidation**:
   - On every upstream sync (see `docs/agent-studios/UPSTREAM.md`), this small patch set must be re-applied, verified, and audited.

---

## Catalog of Intentional Patches

### Patch 001: Pluggable Model Inference Backend Seam (Milestone M07.5)

- **Clean Replacement Seam Commit**: `f9e30b19f79e0b7a54ea9c6480d74bf4d1e2d39b` (branch: `feat/runtime-provider-transport-hardening`)
- **Purpose**: Allows external runtimes (such as Agent Studios) to supply an alternate inference transport for providers using protocols Codex does not speak natively (Chat Completions, Anthropic Messages, Gemini generateContent) while retaining the native Codex agent engine, tool routing, and response pipeline.
- **Files Modified**:
  - `codex-rs/model-provider/src/inference_backend.rs` (new file): Defines `ModelInferenceBackend` trait and `ModelInferenceContext` struct.
  - `codex-rs/model-provider/src/provider.rs`: Adds `inference_backend()` method (default `None`) to `ModelProvider` trait, adds `create_model_provider_with_inference_backend` factory, and integrates `inference_backend` field into `ConfiguredModelProvider`.
  - `codex-rs/model-provider/src/lib.rs`: Re-exports `ModelInferenceBackend` and `ModelInferenceContext`.
  - `codex-rs/core/src/client.rs`: In `ModelClientSession` / `TurnClientSession`, disables WebSocket
    and prewarm when `inference_backend().is_some()`, routes requests to custom backend with
    semantic `ResponsesApiRequest`, and passes the resulting stream through `map_response_stream`.
  - `codex-rs/core/src/session/session.rs`: Forwards session runtime provider into
    `ModelClient::new_with_provider`.
- **Targeted Verification Tests**:
  - `codex-rs/model-provider/src/inference_backend_tests.rs`: Verifies default `None`, custom
    backend attachment, and factory backwards compatibility.
  - `codex-rs/core/src/client_tests.rs`: Verifies custom backend invocation, request fidelity
    (model, thread/turn context), response stream flow through Codex pipeline (`map_response_stream`),
    error propagation, and WebSocket/prewarm bypass.

### Patch 002: Thread Model Runtime Override Seam (Milestone M07.6)

- **Seam Commit**: `3686e61f7c` (branch: `feat/provider-runtime-session-factory`)
- **Purpose**: Enables external thread managers to supply a thread-scoped model runtime override (`SharedModelProvider` paired strictly with `SharedModelsManager`) at thread spawn time, ensuring early models manager resolution, preventing split-brain session states, supporting child session inheritance, and disabling remote compaction / WebSockets for custom backends.
- **Files Modified**:
  - `codex-rs/core/src/model_runtime.rs` (new file): Defines `ModelRuntimeOverride` encapsulating `(SharedModelProvider, SharedModelsManager)`.
  - `codex-rs/core/src/lib.rs`: Re-exports `ModelRuntimeOverride`.
  - `codex-rs/core-api/src/lib.rs`: Re-exports `ModelRuntimeOverride`.
  - `codex-rs/core/src/thread_manager.rs`: Adds `model_runtime_override: Option<ModelRuntimeOverride>` to `StartThreadOptions` and `ThreadSpawnRequest`.
  - `codex-rs/core/src/session/session.rs`: Adds `model_runtime_override` to `SessionSpawnArgs`, resolves `override_models_manager` before `ModelClientSession` construction, propagates override to child/delegate/fork sessions, and forces `RemoteCompactionSupport::Unsupported` for custom inference backends.
- **Targeted Verification Tests**:
  - `codex-rs/core/src/model_runtime_tests.rs`: Verifies tuple encapsulation, accessors, into_parts decomposition, and clone semantics.
  - `codex-rs/core/src/thread_manager_tests.rs`: Verifies `StartThreadOptions` initialization with and without override.
