# Provider Management V1: Architecture and Technical Design

**Milestone:** Stream B — Provider Management V1 (`feat/provider-management-v1`)  
**Target:** Stacked on `feat/ags-cli-tui-foundation`  
**Date:** 2026-10-30  
**Status:** IMPLEMENTED & VERIFIED  

---

## 1. Executive Summary

Provider Management V1 establishes the enterprise-grade foundation for managing heterogeneous AI providers, model definitions, authentication secrets, and dynamic discovery across the Agent Studios ecosystem (`ags` CLI, interactive Ratatui TUI, and daemon runtimes).

Prior to this milestone, provider configurations suffered from:
1. **Tight Coupling to Substring Heuristics:** OpenCode model routing relied on ambient substring checks (e.g. `contains("anthropic")`), causing routing errors on multi-provider setups (e.g., Anthropic models accessed through OpenRouter or Bedrock).
2. **Ambient Secret Leakage:** Plaintext bearer tokens and environment variables were read directly from process environments without redaction, audit locators, or OS keyring abstraction.
3. **Single-Instance Assumptions:** A provider family (e.g. `openai`) could only be defined once, preventing concurrent configurations such as "OpenAI Personal", "OpenAI Work", or local proxy endpoints (`http://localhost:8080/v1`).
4. **Lack of Importer Portability:** Developers migrating from Codex CLI (`config.toml`) or OpenCode (`open-code.json`) faced manual transcription without conflict resolution or validation.

---

## 2. Core Architectural Pillars

### 2.1 Multi-Instance Provider Catalog & Scoping
- **Hierarchical Identification:**
  - `ProviderId`: Identifies the provider family (e.g., `openai`, `anthropic`, `openrouter`, `ollama`, `google`, `custom`).
  - `ProviderInstanceId`: Universally unique identifier (UUID v4) representing a concrete configured instance (e.g., corporate proxy vs personal endpoint).
  - `ModelId`: Model identifier scoped to an instance.
  - `ModelRef`: Compact string representation (`<instance_id>/<model_id>`) providing unambiguous routing.
- **Default Instance Designation:**
  - Each `ProviderId` family maintains an optional pointer to a designated default instance (`default_instances: BTreeMap<ProviderId, ProviderInstanceId>`).
  - Enables intuitive CLI and TUI model routing (`openai/gpt-4o` resolves to default instance if explicit instance ID is omitted).
- **Relational Integrity and Cascade Protection:**
  - Removal of a `ProviderInstance` is rejected if associated models are registered, preventing orphaned model routing.
  - Explicit cascade deletion (`remove_provider_instance_cascade`) provides atomic purge of an instance and all child model descriptors.

### 2.2 Wire Protocol Decoupling (`ProtocolFamily`)
The provider instance explicitly declares its wire serialization protocol independently of provider branding:
- `OpenAiResponses`: Structured responses API.
- `OpenAiChatCompletions`: OpenAI standard `/v1/chat/completions`.
- `AnthropicMessages`: Anthropic `/v1/messages` protocol.
- `GeminiGenerateContent`: Google Gemini `/v1beta/models/...:generateContent`.
- `Custom(String)`: Extensible protocol for custom runtime proxies.

This enables routing arbitrary third-party endpoints (e.g., OpenRouter, LiteLLM, vLLM, LM Studio) to their appropriate wire adapter regardless of provider family.

### 2.3 Staged Atomic Importers
Importers support heterogeneous input formats:
- **Codex CLI:** `config.toml` (`[model_providers.<id>]`).
- **OpenCode:** JSON configurations (`provider`, `model`, `apiKey`).
- **Agent Studios Native:** `ProviderCatalogSnapshot` (schema versioned JSON).

**Transaction Safety:**
Import logic executes in two stages:
1. `preview_import`: Analyzes delta against catalog without side effects, classifying planned additions, updates, skips, and conflicts.
2. `apply_import`: Operates on a staged catalog clone. If any structural or catalog invariant fails, the transaction aborts with zero side effects.
3. **Conflict Strategies:** `Overwrite`, `Skip`, and `Error` (fail-closed).

### 2.4 Dynamic Model Discovery (`ModelDiscoveryClient`)
Dynamic authenticated model discovery queries remote endpoints and populates descriptors:
- **OpenAI:** `GET /v1/models`
- **Anthropic:** `GET /v1/models`
- **Ollama:** `GET /api/tags`
- **OpenRouter:** `GET /v1/models`
- **Caching:** In-memory TTL cache (`tokio::sync::RwLock<HashMap<ProviderInstanceId, (Instant, Vec<ModelDescriptor>)>>`) preventing rate-limit thrashing.
- **Error Mapping:** Structured `DiscoveryError` classifying `AuthenticationFailed`, `RateLimited`, `NetworkError`, `InvalidPayload`, `UnsupportedProtocol`, and `SecretResolutionFailed`.

### 2.5 CLI & TUI Management Surfaces
- **CLI Commands (`ags`):**
  - `ags provider list`, `add`, `get`, `remove`, `import`, `test`
  - `ags model list`, `add`, `get`, `remove`, `test`
- **TUI Management Panel:**
  - Split-pane layout: Left pane (configured provider instances with default indicators), Right pane (registered models list & metadata inspector).
  - Keyboard navigation: `Tab` for pane cycling, `j`/`k` or arrow keys for cursor traversal, `d` for marking default instance, `?` for help modal, `Esc`/`q` for dismissal.
  - Interactive slash command `/providers` and `/model` accessible from main composer.
