# Agent Studios — Providers & Models Architecture

> **Status**: Core Architecture Specification
> **Precedence**: Subservient to `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. Provider Philosophy & Open Ecosystem

Agent Studios is built on an open, vendor-neutral provider philosophy:

1. **No Mandatory Proprietary Login**: Agent Studios functions out-of-the-box without requiring an OpenAI, ChatGPT, or Microsoft Copilot account login.
2. **First-Class Bring-Your-Own-Key (BYOK)**: Developers configure their own API keys stored securely in local OS credential stores or environment variables.
3. **Local & Custom Runtimes as Peers**: Self-hosted LLMs (Ollama, LM Studio, vLLM, LocalAI) and corporate reverse proxies (9Router, LiteLLM) are first-class execution endpoints, not degraded second-class citizens.
4. **Target Provider Landscape**:
   - **Commercial Cloud**: OpenAI, Anthropic, Google Gemini, DeepSeek, Groq.
   - **Multi-Model Gateways**: OpenRouter, 9Router.
   - **Local / Self-Hosted**: Ollama, LM Studio, vLLM.
   - **Custom Endpoints**: Any standards-compliant HTTP/SSE API endpoint.

---

## 2. Fundamental Distinction: Provider vs. Runtime

Agent Studios enforces an immutable architectural separation between **Providers** and **Runtimes**:

| Dimension | Provider (Model / API Wire Layer) | Runtime (Agent Execution Environment) |
| :--- | :--- | :--- |
| **Definition** | Network protocol, inference endpoint, and token translation driver. | The active execution process, sandbox, tool router, and session loop. |
| **Examples** | • OpenAI Responses API<br>• OpenAI Chat Completions API<br>• Anthropic Messages API<br>• Google Gemini `generateContent`<br>• OpenRouter / Ollama | • Internal Codex Engine (`codex-rs`)<br>• OpenCode CLI Adapter (Planned M11)<br>• Claude Code CLI Adapter (Planned M12)<br>• Future external agent runtimes |
| **Responsibilities** | Wire serialization, SSE parsing, prompt caching, token usage accounting. | Process containment, shell execution, file operations, approvals, and context compaction. |

**Crucial Invariant**: The **Anthropic provider** (calling `POST https://api.anthropic.com/v1/messages`) is **NOT** the **Claude Code runtime** (a CLI agent execution harness). Never conflate provider transport with agent runtime supervision.

---

## 3. Authoritative Runtime Identity: `ModelRef`

In Agent Studios, a model slug or name alone (such as `"claude-3-7-sonnet"` or `"gpt-4o"`) is **never** sufficient to identify a runtime model target.

### The Canonical Identifier
Every agent, task, and session must reference a composite, strongly typed `ModelRef`:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelRef {
    pub provider_instance_id: ProviderInstanceId,
    pub model_id: ModelId,
}
```

### Why Model Slugs Alone Are Inadequate
Consider a developer with two configured instances in their catalog:
- **Instance A (`ProviderInstanceId = 4a12...`)**: Direct Anthropic Cloud (`https://api.anthropic.com/v1/messages`), production billing key.
- **Instance B (`ProviderInstanceId = 9b88...`)**: Internal Corporate Gateway (`https://gateway.internal/v1/messages`), rate-limited test tier.

Both instances serve a model named `"claude-3-7-sonnet"`. Binding an agent to the string `"claude-3-7-sonnet"` introduces non-deterministic routing, secret bleeding, and unpredictable latency. Binding to `ModelRef { provider_instance_id: "4a12...", model_id: "claude-3-7-sonnet" }` deterministically binds the agent to the exact desired endpoint, credentials, and capabilities.

---

## 4. Provider Domain Architecture

The `agent-studios-provider` crate decouples vendor specifications from concrete user deployments:

```text
┌─────────────────────────────────────────────────────────────────┐
│                       ProviderCatalog                           │
│                                                                 │
│   ┌───────────────────────────┐   ┌───────────────────────────┐ │
│   │    ProviderDefinition     │   │     ProviderInstance      │ │
│   │    (Metadata / Vendor)    │   │  (Concrete Endpoint & Key)│ │
│   │                           │   │                           │ │
│   │ - id: "anthropic"         │   │ - id: "instance-corp"     │ │
│   │ - name: "Anthropic"       │   │ - provider_def: "anthropic"│
│   │ - protocols: [Messages]   │◄──┤ - endpoint: "https://..." │ │
│   │ - auth: ApiKeyHeader      │   │ - secret: SecretRef(ENV)  │ │
│   └───────────────────────────┘   └─────────────┬─────────────┘ │
│                                                 │               │
│                                                 ▼               │
│                                   ┌───────────────────────────┐ │
│                                   │      ModelDescriptor      │ │
│                                   │  (Registered Model Specs) │ │
│                                   │                           │ │
│                                   │ - id: "claude-3-7-sonnet" │ │
│                                   │ - limits: ModelLimits     │ │
│                                   │ - capabilities: Tristate  │ │
│                                   └───────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### Core Data Definitions
1. **`ProviderDefinition`**: Static catalog metadata for a known vendor (supported wire protocols, standard authentication scheme, documentation URL).
2. **`ProviderInstance`**: A concrete user deployment with a unique `ProviderInstanceId`, base URL, static headers, query parameters, timeout/retry settings, and a zero-plaintext `SecretReference`.
3. **`ModelDescriptor`**: Per-model metadata including context limits (`context_window_tokens`, `max_output_tokens`), input modalities (`Text`, `Image`, `Audio`), and reasoning tiers.
4. **`CapabilitySupport`**: Tristate capability tracking (`Supported`, `Unsupported`, `Unknown`) across 10 distinct features (`tool_calling`, `parallel_tools`, `vision_input`, `reasoning`, `streaming`, `structured_output`, `prompt_caching`, `web_search`, `audio_input`, `context_window`) to eliminate false-negative assumptions.

---

## 5. Canonical Transport Architecture

When Codex generates a turn, Agent Studios routes the semantic request through protocol adapters and asynchronous transport drivers:

```text
               Codex Agent Engine
                       │
             ResponsesApiRequest
                       │
                       ▼
           Agent Studios RuntimeRouter
                       │
       ┌───────────────┼───────────────┐
       ▼               ▼               ▼
OpenAI Chat       Anthropic         Google
Completions       Messages          Gemini
 Adapter           Adapter          Adapter
(M05/M05.1)        (M06)             (M07)
       │               │               │
       ▼               ▼               ▼
   Chat HTTP       Messages HTTP    Gemini HTTP
  SSE Driver        SSE Driver      SSE Driver
       │               │               │
       └───────────────┼───────────────┘
                       │
                       ▼
        Incremental SSE Frame Stream
                       │
                       ▼
           ResponseEvent Stream
                       │
                       ▼
               Codex Agent Loop
           (Tool Routing, Approvals,
             Compaction, Telemetry)
```

### Layer Responsibilities
- **Protocol Adapters (`agent-studios-protocol-adapters`)**: Pure in-memory transformation between Codex `ResponsesApiRequest`/`ResponseEvent` semantics and wire specifications. Zero networking, zero secret resolution.
- **Runtime Transport (`agent-studios-runtime-transport`)**: Asynchronous HTTP transport, chunked SSE decoding, transactional continuation state replay (preserving thinking tokens and cryptographic signatures across turns), and in-memory secret zeroization.
- **Codex Native Bridge (`agent-studios-codex-bridge`)**: Connects `OpenAiResponses` endpoints directly to Codex's native client without translation overhead.
