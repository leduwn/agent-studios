# Agent Studios — Provider Core Architecture (M03)

> **Precedence Note**: This subsystem specification is governed by [`MASTER_VISION.md`](MASTER_VISION.md) and [`PRODUCT_PRINCIPLES.md`](PRODUCT_PRINCIPLES.md). In the event of any discrepancy, `MASTER_VISION.md` is authoritative.

## 1. Architectural Philosophy

Agent Studios requires a universal, provider-neutral model registry that accommodates:
- Commercial hosted APIs (OpenAI, Anthropic, Google Gemini, DeepSeek, Groq, OpenRouter).
- Self-hosted / local models (Ollama, LM Studio, vLLM).
- Dynamic local routing proxies (e.g. 9Router Local, 9Router VPS).
- Enterprise gateways and custom internal OpenAI/Anthropic-compatible proxies.

Crucially, **Agent Studios must NOT hardcode provider brands or assume that `wire_api = "responses"` + custom `base_url` is sufficient for true multi-provider orchestration.**

### Why Codex Baseline is Not Sufficient on Its Own
The imported Codex baseline (`codex-rs/model-provider-info/`) models provider configuration through a single structure where:
1. `wire_api` only supports `Responses` (upstream explicitly removed `Chat`).
2. Configuration mixes static provider metadata with endpoint URLs and secrets.
3. Custom base URLs only work if the remote server implements the exact OpenAI Responses API specification.

Agent Studios Provider Core sits above execution engines to prevent vendor-specific protocols or OpenAI-specific concepts from leaking into agent definition, task planning, and orchestration.

---

## 2. Core Concepts

### ProviderDefinition vs. ProviderInstance
- **`ProviderDefinition`**: Represents a provider family or vendor specification (e.g. `openai`, `anthropic`, `custom-openai`).
  - Contains **zero** credentials, **zero** API keys, and **no mutable account endpoints**.
  - Declares the set of wire protocols supported by this family (`supported_protocols: BTreeSet<ProtocolFamily>`).
- **`ProviderInstance`**: Represents a concrete, configured endpoint instance with unique credentials and settings.
  - Examples: `9Router Local`, `9Router VPS`, `OpenAI Personal`, `Company Gateway`.
  - Holds an `EndpointProfile`, an `AuthenticationScheme`, and selects one specific `ProtocolFamily`.

### Provider vs. Protocol Decoupling
Wire transport protocol is completely decoupled from provider brand. An instance does not deduce its wire protocol from substring matching on the provider name:
- `ProtocolFamily::OpenAiResponses`: OpenAI Responses API (`/v1/responses`).
- `ProtocolFamily::OpenAiChatCompletions`: Standard OpenAI Chat Completions API (`/v1/chat/completions`).
- `ProtocolFamily::AnthropicMessages`: Anthropic Messages API (`/v1/messages`).
- `ProtocolFamily::GeminiGenerateContent`: Google Gemini API (`/v1beta/models/...:generateContent`).
- `ProtocolFamily::Custom(String)`: Custom RPC or experimental protocol.

For example, a `custom-gateway` provider family can back both an `OpenAiResponses` instance and an `AnthropicMessages` instance simultaneously.

### ModelRef Routing
Agents, Tasks, and Studios never store endpoint URLs or secrets. They refer to models using the compact composite `ModelRef`:
```text
ModelRef {
    provider_instance_id: ProviderInstanceId,
    model_id: ModelId,
}
```
This guarantees deterministic routing to the exact configured instance and model without leaking runtime credentials or transport details.

### Tristate Capability Support (`CapabilitySupport`)
Custom providers and new models often have unknown or undocumented capabilities. A simple boolean `false` would incorrectly imply that a feature is definitively unsupported.
```rust
pub enum CapabilitySupport {
    Unknown,      // Capability not probed or declared (default)
    Unsupported,  // Explicitly known to be unsupported
    Supported,    // Explicitly supported
}
```
`ModelCapabilities` tracks:
- `tool_calling`
- `parallel_tool_calls`
- `vision_input`
- `reasoning`
- `streaming`
- `structured_output`
- `prompt_caching`
- `web_search`
- `audio_input`
- `audio_output`

### SecretReference Architecture
Agent Studios enforces zero plaintext credentials in provider configurations:
- Raw API keys or tokens are **never** stored in `EndpointProfile` or `ProviderInstance`.
- `SecretReference` holds only an opaque locator and backend:
  - `EnvironmentVariable`: e.g. `OPENAI_API_KEY`.
  - `OsCredentialStore`: e.g. Windows Credential Manager or macOS Keychain target `agent-studios/provider/<id>`.
  - `External`: external key vault or secret manager.
- Static HTTP headers (`EndpointProfile::static_headers`) strictly forbid credential headers (`Authorization`, `x-api-key`, `api-key`, etc.).
- Embedded userinfo/passwords in URLs (`https://user:pass@host`) are strictly rejected.

---

## 3. Architecture Diagram

```text
Agent / Task
    │
    ▼
 ModelRef
    │
    ├── provider_instance_id: ProviderInstanceId
    └── model_id: ModelId
            │
            ▼
     ProviderCatalog
            │
            ├── ProviderRegistry
            │      ├── ProviderDefinition
            │      │      └── supported_protocols: BTreeSet<ProtocolFamily>
            │      │
            │      └── ProviderInstance
            │             ├── protocol: ProtocolFamily
            │             ├── endpoint: EndpointProfile (base_url, static_headers, query_params)
            │             └── authentication: AuthenticationScheme (SecretReference)
            │
            └── ModelRegistry
                   └── ModelDescriptor
                          ├── capabilities: ModelCapabilities (CapabilitySupport)
                          ├── limits: ModelLimits (context_window, max_output)
                          └── metadata_source: ModelMetadataSource
```

---

## 4. Multi-Instance and Custom Provider Support

Multiple instances of the same provider family coexist seamlessly:
```rust
// Provider family
let def = ProviderDefinition::new(
    ProviderId::new("custom-openai")?,
    "Custom OpenAI Gateway",
    vec![ProtocolFamily::OpenAiResponses, ProtocolFamily::OpenAiChatCompletions],
)?;
catalog.register_provider_definition(def)?;

// Instance 1: Local proxy (no auth)
catalog.register_provider_instance(ProviderInstance::new(
    inst_local_id,
    ProviderId::new("custom-openai")?,
    "9Router Local",
    ProtocolFamily::OpenAiChatCompletions,
    EndpointProfile::new("http://localhost:8080/v1")?,
    AuthenticationScheme::None,
)?);

// Instance 2: Remote VPS proxy (bearer token via env)
catalog.register_provider_instance(ProviderInstance::new(
    inst_vps_id,
    ProviderId::new("custom-openai")?,
    "9Router VPS",
    ProtocolFamily::OpenAiChatCompletions,
    EndpointProfile::new("https://vps.my-router.net/v1")?,
    AuthenticationScheme::BearerToken { secret: SecretReference::env("VPS_KEY")? },
)?);

// Same model ID registered under both instances
catalog.register_model(ModelDescriptor::new(
    inst_local_id,
    ModelId::new("meta/llama-3")?,
    "Llama 3 (Local)",
    ModelCapabilities::unknown(),
    ModelLimits::default(),
)?);

catalog.register_model(ModelDescriptor::new(
    inst_vps_id,
    ModelId::new("meta/llama-3")?,
    "Llama 3 (VPS)",
    ModelCapabilities::unknown(),
    ModelLimits::default(),
)?);
```
Both `ModelRef(inst_local_id, "meta/llama-3")` and `ModelRef(inst_vps_id, "meta/llama-3")` resolve independently and deterministically.

---

## 5. Catalog Snapshot & Invariant Guardrails

1. **Snapshot Schema Version**: `PROVIDER_CATALOG_SCHEMA_VERSION = 1`.
2. **Atomic / Staged Import**: Snapshot importation validates all definitions, instances, and model cross-references in a temporary catalog before touching state. Any broken reference results in an immediate error with zero mutation to existing catalog state.
3. **Safe Removal Invariants**:
   - `remove_provider_definition`: Rejected with `ProviderDefinitionInUse` if any instance references it.
   - `remove_provider_instance`: Rejected with `ProviderInstanceInUse` if any models are registered under it. An explicit cascade method `remove_provider_instance_cascade` is provided when destructive removal is specifically intended.
4. **Encapsulation**: No public `providers_mut()` or `models_mut()`. All mutations occur through deterministic catalog methods.

---

## 6. Future Integration Path

- **M04 (Codex Runtime ↔ Provider Core Bridge)**:
  - Adapter translating `ModelRef` + `ProviderInstance` into Codex runtime options when the selected protocol is `OpenAiResponses`.
  - Secure credential resolution from `SecretReference` via OS credential manager / environment.
- **Future Protocol Drivers**:
  - Direct `OpenAiChatCompletions` driver for local / non-Responses endpoints.
  - Direct `AnthropicMessages` adapter.
  - Direct `GeminiGenerateContent` adapter.
