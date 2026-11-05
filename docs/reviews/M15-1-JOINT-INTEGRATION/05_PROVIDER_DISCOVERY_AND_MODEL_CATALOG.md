# Agent Studios — Milestone M15.1 Joint Integration Preflight
## Document 05: Provider Discovery Engine & Model Catalog Audit

**Milestone:** M15.1 (Joint Integration Preflight & Cross-Platform Release Gate)  
**Date:** 2026-11-06  
**Role:** Principal Integration Engineer & Backend Architect  
**Subsystem:** `agent-studios-rs/crates/provider` & `crates/cli`  
**PR Origin:** PR #31 (`feat/m15-1-backend-cli-consolidation`)  
**Status:** DISCOVERY & CATALOG ACCEPTED  

---

### 1. Provider Discovery Architecture

The provider discovery engine in `crates/provider` decouples agent goal execution from concrete model vendor APIs. It provides a uniform protocol abstraction over commercial and local LLM runtimes:

```
                      ┌───────────────────────────────┐
                      │    Agent Studios Engine /     │
                      │      Task Orchestrator        │
                      └───────────────┬───────────────┘
                                      │
                         Uniform Provider Trait
                                      │
              ┌───────────────────────┼───────────────────────┐
              │                       │                       │
              ▼                       ▼                       ▼
    [ Anthropic Messages ]  [ OpenAI Responses ]    [ Ollama Native ]
     Claude 3.5 / Claude 5   GPT-4o / GPT-4.5        Llama 3 / DeepSeek
      (Streaming SSE)         (Streaming SSE)         (Local REST/JSON)
```

---

### 2. Supported Provider Protocols

1. **`anthropic_messages`:**
   - Implements Anthropic Messages API v1 with Server-Sent Events (SSE).
   - Handles thought block delimitation (`is_thought: true`), assistant message chunks, and tool calling frames.
   - Built-in retry and backoff on HTTP 429 / 529.

2. **`openai_responses`:**
   - Implements OpenAI Chat Completions & Responses API.
   - Handles streaming delta chunks and function calling protocols.

3. **`ollama_native`:**
   - Native HTTP client for local Ollama instances (`http://127.0.0.1:11434`).
   - Supports zero-token cost, fully offline inference for privacy-sensitive workspaces.

---

### 3. Credential Security & Invariant Enforcement

- **Zero Token Leakage Invariant:**
  - API keys (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`) and bearer tokens are stored strictly in memory within the credential vault.
  - IPC serialization explicitly strips auth secrets:
    ```json
    {
      "id": "6a7eba57-4986-4c97-8fc4-e0825aa62d7b",
      "name": "OpenAI Main",
      "provider": "openai",
      "protocol": "openai_responses",
      "endpoint": "https://api.openai.com/v1",
      "auth": "none",
      "is_default": true,
      "models": 1
    }
    ```
  - CLI outputs (`ags provider list`, `ags model list`) and logs mask or omit credentials.
  - CLI command-line parameters forbid passing tokens via arguments (preventing process table leaks via `ps` or Task Manager).

---

### 4. Real CLI Output Acceptance Evidence

Executed against the compiled `ags` binary on the integrated tree:

#### 4.1 `ags provider list --format json`
```json
[
  {
    "auth": "none",
    "endpoint": "https://api.openai.com/v1",
    "id": "6a7eba57-4986-4c97-8fc4-e0825aa62d7b",
    "is_default": true,
    "models": 1,
    "name": "OpenAI Main",
    "protocol": "openai_responses",
    "provider": "openai"
  },
  {
    "auth": "none",
    "endpoint": "https://api.anthropic.com",
    "id": "d9d6c648-5649-48cb-9a80-6b0270b98597",
    "is_default": false,
    "models": 0,
    "name": "Anthropic Dev",
    "protocol": "anthropic_messages",
    "provider": "anthropic"
  }
]
```

#### 4.2 `ags model list --format json`
```json
[
  {
    "capabilities": "-",
    "context_tokens": "128k",
    "instance_id": "6a7eba57-4986-4c97-8fc4-e0825aa62d7b",
    "max_output": "4k",
    "model_id": "gpt-4o",
    "name": "GPT-4o Omni",
    "provider": "openai"
  }
]
```

---

### 5. Mock Provider & Offline Fixtures

To ensure zero billable execution across continuous integration and developer workflows:
- All provider integration tests utilize in-memory mock transports with deterministic response sequences.
- Total billable API expenditure across verification: **$0.00**.
