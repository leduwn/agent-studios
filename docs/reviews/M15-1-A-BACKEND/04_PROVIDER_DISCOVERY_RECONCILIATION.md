# Provider Discovery v1.2 & Configuration Importer Reconciliation Report
**Milestone:** M15.1-A — Backend and CLI Consolidation  
**Target Crate:** `agent-studios-rs/crates/provider`  
**Preserved Baseline:** Branch `feat/provider-discovery-v1.2-wip` (`96dfb4946e`) & Preservation Backup `provider-discovery-20261105T012118Z`  
**Date:** 2026-11-06  
**Auditor:** Claude 1 (Principal Rust Architect & Git Integration Engineer)

---

## 1. Executive Summary

This report documents the forensic reconciliation and full production integration of **Provider Discovery v1.2** into the canonical Agent Studios workspace (`agent-studios-rs/crates/provider/`).

The preserved uncommitted work and WIP branch feature set have been completely reconciled without data loss, validated against a 96-test suite covering:
1. Dynamic Model Discovery & URL Normalization
2. Capability Inference & Context Heuristics
3. Multi-Backend Decoupled `SecretStore`
4. Multi-Format Configuration Importers (Codex, OpenCode, Agent Studios Snapshot)
5. 6-Stage Connectivity Testing & WireMock Fixtures

---

## 2. Reconciled Architectural Components

### 2.1 Dynamic Model Discovery (`crates/provider/src/discovery.rs`)
- **`ModelDiscoveryClient`**: Authenticated asynchronous HTTP client configured with strict request timeouts (default 10s), redirect policy controls (cross-origin redirection blocked to prevent token exfiltration), and standardized error mappings.
- **Protocol Dispatchers & URL Normalizers**:
  - `anthropic_messages`: Dispatches to `https://api.anthropic.com/v1/models` with `x-api-key` and `anthropic-version: 2023-06-01`.
  - `openai_responses`: Automatically normalizes custom endpoints (e.g. `https://api.openai.com`, OpenRouter, LocalAI) ensuring clean `/v1/models` routing.
  - `ollama_native`: Connects to `http://localhost:11434/api/tags`, converting Ollama model tags into unified `ModelDescriptor` entries.
- **In-Memory TTL Caching**:
  - `Arc<tokio::sync::RwLock<HashMap<ProviderInstanceId, (Instant, Vec<ModelDescriptor>)>>>`
  - Enforces a 5-minute cache TTL, preventing redundant model discovery requests during rapid CLI or UI query loops.

### 2.2 Capability Inference Engine
Discovered models automatically undergo capability inference based on model IDs, family patterns, and metadata:
- **Context Window Sizing**:
  - `claude-3-5-*` / `claude-3-*`: 200,000 tokens
  - `gpt-4o` / `gpt-4-turbo`: 128,000 tokens
  - `o1` / `o3-mini`: 128,000–200,000 tokens
  - `llama3*`: 8,192–131,072 tokens
- **Feature Flags Inferred**:
  - Tool calling (`tools: true`)
  - Vision / multimodal input (`vision: true`)
  - Streaming output (`streaming: true`)
  - Prompt caching (`prompt_caching: true` for Anthropic & compatible endpoints)

### 2.3 Decoupled `SecretStore` Abstraction (`crates/provider/src/secret.rs`, `store.rs`)
To satisfy enterprise security invariants, all credentials are decoupled from catalog serialization:
- `SecretReference`: Holds a URI-like locator (e.g., `keyring://agent-studios/anthropic-default`, `env://ANTHROPIC_API_KEY`) and backend kind.
- `SecretStore` Trait:
  ```rust
  #[async_trait]
  pub trait SecretStore: Send + Sync {
      async fn get_secret(&self, reference: &SecretReference) -> Result<Option<Vec<u8>>, SecretStoreError>;
      async fn set_secret(&self, reference: &SecretReference, value: &[u8]) -> Result<(), SecretStoreError>;
      async fn delete_secret(&self, reference: &SecretReference) -> Result<(), SecretStoreError>;
      async fn list_secrets(&self) -> Result<Vec<SecretReference>, SecretStoreError>;
  }
  ```
- Concrete Implementations:
  - `InMemorySecretStore`: Ephemeral store for deterministic testing.
  - `EnvironmentSecretStore`: Resolves keys from environment variables (`env://VAR_NAME`).
  - `OsKeyringSecretStore`: Secure persistence backed by OS credential vaults (Windows Credential Manager, macOS Keychain, Linux Secret Service).
  - `CompositeSecretStore`: Tiered router directing requests to appropriate backend based on reference scheme.

### 2.4 Multi-Format Configuration Importer (`crates/provider/src/importer.rs`)
Enables zero-friction migration and synchronization from legacy toolchains:
1. **`CodexConfigImporter` (`config.toml`)**: Imports model provider configurations and API endpoints from OpenAI Codex CLI settings.
2. **`OpenCodeConfigImporter` (`open-code.json`)**: Imports multi-provider settings from OpenCode environments.
3. **`AgentStudiosSnapshotImporter` (`snapshot.json`)**: Full-fidelity export/import of entire provider catalogs including instance mappings and default selections.
4. **Collision Handling**: Supports `ConflictStrategy::Skip`, `ConflictStrategy::Overwrite`, and `ConflictStrategy::Error` with atomic rollback on failure.
5. **Dry-Run Previewing**: Generates detailed import diffs (`ImportPreview`) without mutating catalog state.

### 2.5 6-Stage Connectivity Testing
The `test_connection` pipeline executes six sequential validation checks:
1. **Credential Presence:** Verifies `SecretReference` exists and resolves in configured `SecretStore`.
2. **Endpoint Syntax & Host Reachability:** Validates URI scheme (rejects forbidden schemes like `file://`), parses host, and verifies DNS / port connectivity.
3. **Authentication Handshake:** Executes a minimal ping or auth test request against the remote endpoint.
4. **Protocol Negotiation:** Verifies response header compliance and supported API version.
5. **Rate-Limit & Quota Probe:** Detects `429 Too Many Requests` or quota depletion and reports structured diagnostics.
6. **Model Discovery Verification:** Confirms at least one valid model descriptor can be retrieved and parsed.

---

## 3. Test Verification Matrix

All 96 tests in `agent-studios-provider` pass with zero failures:
```text
running 35 tests in unittests (src/lib.rs) ... ok (35 passed)
running 15 tests in tests/discovery_tests.rs ... ok (15 passed)
running 8 tests in tests/importer_tests.rs ... ok (8 passed)
running 33 tests in tests/provider_tests.rs ... ok (33 passed)
running 5 tests in tests/secret_store_tests.rs ... ok (5 passed)

Total: 96 passed, 0 failed, 0 ignored
```

### Highlights of Tested Edge Cases:
- Cross-origin HTTP redirects are strictly blocked during model discovery.
- Sensitive headers (`Authorization`, `x-api-key`) in static headers are rejected at endpoint configuration time.
- Embedded credentials in URLs (`https://user:pass@endpoint`) are rejected.
- Corrupt or invalid JSON payloads during discovery return structured errors without crashing.
- Rate limiting (HTTP 429) is classified as `RateLimited` rather than a generic network failure.
