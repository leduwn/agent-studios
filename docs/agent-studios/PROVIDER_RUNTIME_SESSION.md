# Provider Runtime Assembly & Codex Thread Injection (Milestone M07.6)

This document describes the architecture, design principles, and operational invariants for Milestone M07.6: **Provider Runtime Assembly & Codex Thread Injection** implemented in the `agent-studios-runtime-session` crate and supported by the generic `ModelRuntimeOverride` seam in `codex-rs`.

---

## 1. Architectural Motivation & Overview

Agent Studios enables users to define, manage, and configure heterogeneous AI model providers—spanning native OpenAI Responses APIs, OpenAI-compatible Chat Completions, Anthropic Messages, and Google Gemini `generateContent`—within a unified control plane catalog.

To execute agent workflows against these custom providers using Codex's native execution engine (tool router, approval flow, sandboxing, session state, compaction, and response handling), the runtime must inject configured provider instances and static model metadata directly into newly spawned Codex threads without:
1. Hardcoding vendor-specific logic into `codex-rs`.
2. Making network round-trips to discover model capabilities or pricing at startup (zero-discovery requirement).
3. Allowing split-brain configurations where a thread queries model capabilities from one provider but routes inference through another.
4. Causing route or state collisions between concurrent sessions configured with identical model names across different provider instances.

Milestone M07.6 solves this through a two-part architecture:
1. **Generic Codex Seam (`codex-rs`)**: A provider-neutral `ModelRuntimeOverride` handle encapsulating a strictly paired `(SharedModelProvider, SharedModelsManager)` tuple, threaded through `StartThreadOptions`, `ThreadSpawnRequest`, `SessionSpawnArgs`, and `Session::spawn_internal`.
2. **Runtime Assembly Factory (`agent-studios-runtime-session`)**: A dedicated crate providing `AgentStudiosRuntimeSessionFactory`, `StaticModelsManager`, and `PreparedRuntimeSession` that seamlessly binds `ProviderCatalog` definitions, `InMemorySecretResolver` credentials, and `RuntimeRouter` drivers into thread-ready injection options.

---

## 2. Generic Codex Seam: `ModelRuntimeOverride` (Patch 002)

To maintain strict isolation between `codex-rs` and Agent Studios (see `docs/agent-studios/CODEX_PATCHES.md`), `codex-rs` contains zero vendor-specific types or Agent Studios dependencies. Instead, it exposes a generic, clean abstraction:

### Seam Invariants

- **Tuple Encapsulation**:
  ```rust
  #[derive(Clone)]
  pub struct ModelRuntimeOverride {
      provider: SharedModelProvider,
      models_manager: SharedModelsManager,
  }
  ```
  `ModelRuntimeOverride` forces `SharedModelProvider` and `SharedModelsManager` to be supplied together as an atomic unit. It is impossible to override the inference provider without also supplying its corresponding models manager, preventing split-brain states.

- **Early `models_manager` Resolution**:
  In `codex_core::Session::spawn_internal`, when `model_runtime_override` is present in `SessionSpawnArgs`:
  ```rust
  let (override_provider, override_models_manager) = match model_runtime_override {
      Some(r) => {
          let (p, m) = r.into_parts();
          (Some(p), Some(m))
      }
      None => (None, None),
  };
  let models_manager = override_models_manager.unwrap_or(default_models_manager);
  ```
  This resolution happens *before* `ModelClientSession` construction and *before* `models_manager.list_models()` or `get_model_info()` can be invoked, ensuring all model metadata checks during thread initialization query the overridden models manager.

- **Conditional Child Session Inheritance**:
  When a thread spawns child sessions (subagents, delegate tasks, worktree sessions, or forks):
  - If the thread was spawned with an explicit `model_runtime_override`, the override is cloned and passed into `SessionSpawnArgs` for the child session.
  - If the thread was not spawned with an override, standard default provider resolution applies.

- **Custom Backend Safety Guards**:
  When `ModelProvider::inference_backend()` is `Some`:
  - Remote compaction is forced to `RemoteCompactionSupport::Unsupported`.
  - WebSocket transports and prewarming handshakes are unconditionally disabled.
  - All prompt compilation, tool routing, history tracking, and response stream mapping run through Codex's standard pipeline without modification.

---

## 3. Zero-Discovery Static Catalog: `StaticModelsManager`

Standard Codex `ModelsManager` implementations periodically make external HTTP calls to `/models` endpoints to discover available models, context windows, and pricing metadata.

In Agent Studios, all provider and model metadata is managed authoritatively by `ProviderCatalog`. Querying remote endpoints during thread startup introduces latency, fails in offline or air-gapped environments, and fails on custom OpenAI-compatible endpoints that omit the `/models` endpoint.

`StaticModelsManager` solves this by wrapping `codex_models_manager::manager::StaticModelsManager`:
- It accepts a slice of `ModelDescriptor` objects from `ProviderCatalog`.
- Translates each descriptor into Codex's `ModelInfo` representation via `model_descriptor_to_model_info`.
- Satisfies the `codex_models_manager::manager::ModelsManager` trait completely in-memory.
- Zero network requests are made during thread startup or execution.

### Translation Semantics (`model_descriptor_to_model_info`)

| Field | Source / Transformation |
| :--- | :--- |
| `slug` | `descriptor.id.as_str()` |
| `display_name` | `descriptor.display_name.clone()` |
| `description` | `Some("{display_name} via Agent Studios")` |
| `visibility` | `ModelVisibility::List` |
| `priority` | `0` |
| `supported_in_api` | `true` |
| `used_fallback_model_metadata` | `false` |
| `context_window` / `max_context_window` | Mapped from `descriptor.limits.context_window_tokens` |
| `default_reasoning_level` | `Some(ReasoningEffort::Medium)` when `capabilities.reasoning == Supported` |
| `supported_reasoning_levels` | `[Low, Medium, High]` presets when `capabilities.reasoning == Supported` |

---

## 4. Runtime Session Factory & Dual Execution Paths

`AgentStudiosRuntimeSessionFactory` converts catalog definitions into injection-ready `PreparedRuntimeSession` objects.

### Dual Execution Paths

1. **Native `OpenAiResponses` Bridge**:
   - Provider instances configured with `ProtocolFamily::OpenAiResponses` resolve via `CodexProviderBridge::resolve`.
   - Attaches `inference_backend = None`.
   - Codex uses its native Responses client directly against the provider's endpoint with resolved credentials.

2. **Custom Protocols (`OpenAiChatCompletions`, `AnthropicMessages`, `GeminiGenerateContent`)**:
   - Provider instances configured with custom protocols construct a session-scoped `RuntimeRouter`.
   - The router registers the instance and all its associated models.
   - Attaches the router as `Arc<dyn ModelInferenceBackend>`.
   - Forces `RemoteCompactionSupport::Unsupported`.
   - Codex compiles semantic `ResponsesApiRequest` payloads and routes them through the router, which drives HTTP SSE streaming via the appropriate protocol driver and emits canonical `ResponseEvent` streams.

### Session-Scoped Router Isolation

To prevent route and state collisions across concurrent sessions:
- Each call to `prepare_runtime_session` instantiates a dedicated, session-scoped `RuntimeRouter`.
- Only the routes for the specific `ProviderInstanceId` are registered in that router.
- Multiple concurrent sessions using identical model names (e.g. `shared-model-name` across two different private Ollama or DeepSeek instances) maintain complete namespace and credential isolation.

---

## 5. Thread Injection Lifecycle

1. **Selection**: Control plane selects `ProviderInstanceId` and optional target `ModelId`.
2. **Preparation**:
   ```rust
   let prepared = factory.prepare_runtime_session(&instance_id, target_model_id)?;
   ```
3. **Options Injection**:
   ```rust
   let mut options = StartThreadOptions::new(config);
   prepared.prepare_start_thread_options(&mut options);
   ```
4. **Thread Spawn**:
   ```rust
   let thread = thread_manager.start_thread_with_options(options).await?;
   ```
5. **Session Execution**: The thread executes with strict pairing between static model metadata and runtime inference dispatch.
