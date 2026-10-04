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

In Milestone M07.6.1, `model_descriptor_to_model_info` is fallible (`Result<ModelInfo, RuntimeSessionError>`) and strictly scrubbed to eliminate all synthetic fallback defaults inherited from upstream `codex_models_manager::model_info::model_info_from_slug`:

| Field | Source / Transformation | Fallback Scrubbing Invariant |
| :--- | :--- | :--- |
| `slug` | `descriptor.id.as_str()` | Exact model identifier |
| `display_name` | `descriptor.display_name.clone()` | Preserved |
| `description` | `Some("{display_name} via Agent Studios")` | Informational |
| `visibility` | `ModelVisibility::List` | Discoverable in thread |
| `priority` | `0` | Default priority |
| `supported_in_api` | `true` | Explicitly supported |
| `used_fallback_model_metadata` | `false` | Explicitly marked non-fallback |
| `supports_search_tool` | `false` | Scrubbed server-hosted search tool |
| `context_window` / `max_context_window` | Checked conversion from `descriptor.limits.context_window_tokens` via `i64::try_from(ctx)` | If `None`, set strictly to `None` (never retains synthetic 272,000 fallback). Overflow returns `RuntimeSessionError::ModelMetadataOutOfRange`. |
| `input_modalities` | Derived from `descriptor.capabilities` | Explicitly constructed: always `[Text]`; includes `Image` iff `vision_input == Supported`; includes `Audio` iff `audio_input == Supported`. |
| `default_reasoning_level` | `None` | Synthetic default reasoning level cleared |
| `supported_reasoning_levels` | `vec![]` | Synthetic `[Low, Medium, High]` presets eliminated |
| `include_skills_usage_instructions` | `true` | Preserved host instructions |
| `include_agent_descriptions_instructions` | `true` | Preserved host instructions |
| `include_shell_command_descriptions_instructions` | `true` | Preserved host instructions |

---

## 4. Runtime Session Factory & Execution Semantics (M07.6.1)

`AgentStudiosRuntimeSessionFactory` converts catalog definitions into injection-ready `PreparedRuntimeSession` objects using an authoritative model reference.

### Authoritative `ModelRef` Contract

- `prepare_runtime_session(&self, model_ref: &ModelRef) -> Result<PreparedRuntimeSession, RuntimeSessionError>` requires an immutable composite key `ModelRef(ProviderInstanceId, ModelId)`.
- Eliminates accidental or alphabetical default model fallbacks; sessions are strictly bound to the requested model.

### Custom Provider Login Independence

- Custom protocol providers (`OpenAiChatCompletions`, `AnthropicMessages`, `GeminiGenerateContent`) inject `auth_manager = None` into both `create_model_provider_with_inference_backend` and `StaticModelsManager::new`.
- This decouples custom providers completely from Codex account login mechanisms, refresh tokens, and authentication prompts.

### Protocol Routing & Wire Safety

- Explicitly routes:
  - `ProtocolFamily::OpenAiResponses`: Native Responses bridge (`inference_backend = None`).
  - `ProtocolFamily::OpenAiChatCompletions`, `ProtocolFamily::AnthropicMessages`, `ProtocolFamily::GeminiGenerateContent`: Driven by session-scoped `RuntimeRouter`.
  - `ProtocolFamily::Custom(name)`: Fails closed immediately with `RuntimeSessionError::UnsupportedProtocol(name)`.
- Protocol adapters unpack nested tool namespaces (`{"type": "namespace", "tools": [...]}`) into flat function definitions for non-Responses protocols while skipping server-hosted Responses API tools (`web_search`, `tool_search`).

### Prepared Session Introspection & Secret Safety

- `PreparedRuntimeSession` stores:
  - `model_ref: ModelRef`
  - `protocol: ProtocolFamily`
  - `model_provider_id: String`
  - `selected_model: String`
  - `available_models: Vec<String>`
  - `model_runtime_override: ModelRuntimeOverride`
- Implements accessors: `model_ref()`, `provider_instance_id()`, `protocol()`, `selected_model()`, `model_provider_id()`, `available_models()`, `model_runtime_override()`.
- Implements secret-free `Debug` formatting redacting internal implementation pointers.

### Session-Scoped Router Isolation

To prevent route and state collisions across concurrent sessions:
- Each call to `prepare_runtime_session` instantiates a dedicated, session-scoped `RuntimeRouter`.
- Only the routes for the specific `ProviderInstanceId` are registered in that router.
- Multiple concurrent sessions using identical model names (e.g. `shared-model-name` across two different private Ollama or DeepSeek instances) maintain complete namespace and credential isolation.

---

## 5. Thread Injection Lifecycle

1. **Selection**: Control plane selects authoritative `ModelRef(ProviderInstanceId, ModelId)`.
2. **Preparation**:
   ```rust
   let prepared = factory.prepare_runtime_session(&model_ref)?;
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
5. **Session Execution**: The thread executes with strict pairing between static model metadata and runtime inference dispatch. Internal child/fork sessions inherit the runtime override when provider IDs match.
