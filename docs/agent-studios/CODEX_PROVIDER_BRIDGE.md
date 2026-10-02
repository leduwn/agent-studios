# Agent Studios: Codex Provider Bridge Architecture (M04)

## Overview

The `agent-studios-codex-bridge` crate provides a narrow, unidirectional runtime adapter connecting Agent Studios' core provider catalog (`agent-studios-provider`) to the OpenAI Codex Responses runtime configuration (`codex-model-provider-info`).

The bridge ensures that Agent Studios control plane and multi-agent systems can configure and target arbitrary OpenAI Responses-compatible endpoints—such as local proxies, 9Router instances, self-hosted gateways, and direct OpenAI APIs—without modifying upstream Codex runtime code.

---

## Architectural Principles & Boundaries

### 1. Unidirectional Dependency Flow
```
agent-studios-provider (Provider Core)
        ▲
        │ (depends on)
agent-studios-codex-bridge (Bridge Adapter)
        │ (depends on)
        ▼
codex-model-provider-info (Codex Metadata)
```
- **Provider Core Independence**: `agent-studios-provider` has zero dependencies on `codex-rs` or `codex-model-provider-info`.
- **Zero Codex Upstream Modifications**: Upstream Codex source code (`codex-rs/`) remains completely untouched (`git diff main -- codex-rs` is strictly empty).
- **Ephemeral Adapter Product**: The bridge produces `CodexResponsesBinding`, an in-memory runtime adapter structure. It is never serialized into persistent configuration files.

### 2. Upstream Codex Wire Protocol Constraints
In upstream Codex, `wire_api = "chat"` has been deprecated and completely removed. The Codex runtime engine natively and exclusively communicates via the OpenAI Responses API (`/v1/responses`, `WireApi::Responses`).

Accordingly, `agent-studios-codex-bridge` strictly gates on `ProtocolFamily::OpenAiResponses`. Any attempt to resolve a provider instance configured for other protocols (such as `OpenAiChatCompletions`, `AnthropicMessages`, or `GeminiGenerateContent`) is rejected with a typed `CodexBridgeError::UnsupportedProtocol`. Non-Responses protocols will be bridged in subsequent milestones via dedicated translation adapters (M05–M07).

---

## Invariant Guardrails & Security Policies

### 1. Deterministic Instance Keying & Multi-Instance Coexistence
In multi-agent environments, users often configure multiple instances of the same provider family or proxy (for example, "9Router Local" at `http://localhost:8080/v1` and "9Router VPS" at `https://vps.example.com/v1`) hosting identical model IDs (`gpt-4o`, `deepseek-r1`).

To prevent key collisions and decouple routing from mutable human-readable display names, the bridge computes deterministic provider keys keyed by the immutable `ProviderInstanceId` UUID:
```rust
pub fn deterministic_codex_provider_key(instance_id: &ProviderInstanceId) -> String {
    format!("agent-studios-{}", instance_id)
}
```
This guarantees deterministic coexistence in a shared runtime catalog without collisions.

### 2. Zero-Plaintext Credential Resolution & Reference Mapping
Agent Studios enforces a strict zero-plaintext secret architecture:
- Neither Provider Core nor Codex Bridge ever reads, stores, or transmits raw secret values.
- M04 does not inspect process environment variables (`std::env::var` is never called for credentials).
- Secret references are mapped indirectly so that the Codex runtime client itself reads the environment at request time:
  - `AuthenticationScheme::BearerToken { secret }`: mapped to `ModelProviderInfo.env_key = Some(secret.locator)`.
  - `AuthenticationScheme::OAuthToken { secret }`: mapped to `ModelProviderInfo.env_key = Some(secret.locator)`.
  - `AuthenticationScheme::ApiKeyHeader { header_name, secret }`: mapped to `ModelProviderInfo.env_http_headers = Some({ header_name: secret.locator })`.
  - `AuthenticationScheme::None`: `env_key` and `env_http_headers` remain `None`.
  - `AuthenticationScheme::QueryParameter`: strictly rejected with `SecretQueryParameterAuthUnsupported` to prevent credentials appearing in URLs or request logs.
- `experimental_bearer_token` is never populated.
- Debug outputs and error structures never contain secret values.

### 3. Unsupported Secret Backends Safety Gate
In M04, only `SecretBackend::EnvironmentVariable` is supported for Codex Responses runtime mapping. If a provider instance references `SecretBackend::OsCredentialStore` or `SecretBackend::External`, the bridge rejects resolution with `CodexBridgeError::UnsupportedSecretBackend` rather than attempting unsafe or incomplete resolution.

### 4. Model Capability Safety & Tristate Evaluation
Codex runtime operations fundamentally rely on model tool calling (function calling) and streaming token responses. The bridge evaluates model capabilities using tristate semantics (`Supported`, `Unsupported`, `Unknown`):
- **Explicit `Unsupported`**: If `tool_calling` or `streaming` is explicitly marked as `Unsupported`, resolution fails immediately with `CodexBridgeError::UnsupportedModelCapability`.
- **`Unknown` Tracking**: If capabilities have not been probed (`Unknown`), resolution succeeds to avoid blocking unverified or proxy models, but unverified capabilities are tracked in `CodexCompatibilityReport.unverified_capabilities` (`is_fully_verified() == false`).
- **Fully Verified**: Models with both capabilities explicitly `Supported` produce an empty unverified list (`is_fully_verified() == true`).

### 5. Header & URL Sanitization
- **Forbidden Static Headers**: Static headers configured on `EndpointProfile` are screened against forbidden credential headers (case-insensitive: `authorization`, `proxy-authorization`, `x-api-key`, `api-key`, `x-auth-token`, `bearer`). Any match is rejected to prevent credential leaks.
- **Header Name Syntax**: All header names (for both static headers and custom API key headers) are validated against RFC 7230 token rules.
- **URL Credential Stripping**: URLs containing embedded userinfo (`https://user:pass@host`) are strictly rejected with typed errors.
- **Deterministic Catalog URL Resolution**: Absolute `http(s)` URLs are preserved verbatim. Relative and absolute path strings (`models`, `/models`) are resolved deterministically against the base URL host and path.

---

## Evaluation of Runtime Factory Smoke Test Dependency Cost

Upstream Codex provides client factory logic in `codex-model-provider` that constructs `ApiProvider` instances from `ModelProviderInfo`. We evaluated whether `agent-studios-codex-bridge` should depend on `codex-model-provider` to execute runtime factory smoke tests:

| Criterion | Depending on `codex-model-provider` | Depending on `codex-model-provider-info` (Selected) |
|---|---|---|
| **Dependency Tree** | Heavy (pulls Tokio, Reqwest, AWS SDK, Rama HTTP, TLS, tokenizers) | Ultra-light (pure data types, serde, url, thiserror) |
| **Compile Time** | Significant overhead (>3-5 minutes on clean build) | Fast (<2 seconds incremental) |
| **Modularity** | Blurs control-plane configuration and runtime transport execution | Clean separation: metadata bridge vs network driver |
| **Validation Coverage** | Redundant; factory primarily asserts fields already checked by `validate()` | Direct: `ModelProviderInfo::validate(&self)` verifies upstream invariants |

**Architectural Decision**: Keep `agent-studios-codex-bridge` strictly dependent on `codex-model-provider-info`. `ModelProviderInfo::validate(&self)` provides full upstream semantic validation without pulling the heavy network execution stack into the bridge crate.

---

## Verification & Test Coverage

The test suite in `tests/bridge_tests.rs` covers 21 distinct scenarios:
1. `test_multi_instance_coexistence_and_deterministic_keying`
2. `test_bearer_auth_mapping`
3. `test_oauth_auth_mapping`
4. `test_api_key_header_mapping`
5. `test_no_auth_mapping`
6. `test_query_parameter_auth_rejected`
7. `test_unsupported_secret_backend_rejected`
8. `test_static_headers_and_query_params_mapping`
9. `test_forbidden_static_headers_rejected`
10. `test_invalid_header_name_rejected`
11. `test_catalog_url_resolution`
12. `test_unsupported_protocol_rejected`
13. `test_capability_safety_explicit_unsupported_rejected`
14. `test_capability_unknown_tracked_in_compatibility_report`
15. `test_fully_verified_capabilities_report`
16. `test_disabled_provider_instance_rejected`
17. `test_unknown_model_rejected`
18. `test_bridge_options_propagation`
19. `test_brand_independence`
20. `test_aws_sigv4_rejected_with_unsupported_auth`
21. `test_zero_raw_credential_resolution_invariance`
