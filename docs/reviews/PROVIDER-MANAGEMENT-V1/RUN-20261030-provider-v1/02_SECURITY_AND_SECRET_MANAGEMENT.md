# Provider Management V1: Security and Secret Management Audit

**Milestone:** Stream B — Provider Management V1 (`feat/provider-management-v1`)  
**Target:** Stacked on `feat/ags-cli-tui-foundation`  
**Date:** 2026-10-30  
**Security Status:** PASS (Fail-Closed, Zero Plaintext Secrets)  

---

## 1. Security Architecture & Threat Model

Provider Management V1 is engineered under strict security invariants:
1. **Zero Bearer Secrets in Git / Persistence:** API keys and sensitive tokens must NEVER be stored in catalog snapshots (`providers.json`), Git history, test fixtures, or log outputs.
2. **Decoupled Secret References (`SecretReference`):** The catalog stores only indirection pointers (`backend` + `locator`), such as `env:OPENAI_API_KEY` or `os:agent-studios:openai:work`.
3. **Endpoint Validation & Protection:** Endpoints undergo RFC 3986 validation, forbidding sensitive static headers (`Authorization`, `x-api-key`, `Proxy-Authorization`), credential embedding in URLs (`https://user:pass@host`), and URL fragments.
4. **Fail-Closed Resolution:** When secrets cannot be resolved from the configured backend, model discovery and runtime dispatch abort immediately without falling back to ambient process values.

---

## 2. Decoupled Cross-Platform `SecretStore`

The `SecretStore` trait abstracts credential resolution across storage backends:

```rust
pub trait SecretStore: Send + Sync {
    fn get_secret(&self, reference: &SecretReference) -> Result<String, SecretStoreError>;
    fn set_secret(&self, reference: &SecretReference, value: &str) -> Result<(), SecretStoreError>;
    fn delete_secret(&self, reference: &SecretReference) -> Result<(), SecretStoreError>;
}
```

### Supported Implementations:
1. **`OsKeyringSecretStore`:**
   - Backed by `keyring` v3.6.
   - Platform-native credential vaults:
     - **Windows:** Windows Credential Manager (`agent-studios:<service>:<account>`).
     - **Linux:** SecretService (DBus / GNOME Keyring / KWallet).
     - **macOS:** Apple Keychain Services.
2. **`EnvironmentSecretStore`:**
   - Reads strictly scoped environment variables.
   - Rejects non-environment references.
3. **`CompositeSecretStore`:**
   - Enterprise router querying `OsKeyringSecretStore` first, then falling back to `EnvironmentSecretStore`.
   - Prevents credential proliferation while allowing CI/CD overrides.
4. **`InMemorySecretStore`:**
   - Isolated thread-safe mock storage for integration testing without polluting host keyring.

---

## 3. Redaction and Display Invariants

All secret references implement strict redaction in formatting and serialization:
- `Debug` and `Display` representations output sanitized locators (e.g. `SecretReference(env:OPENAI_API_KEY)` or `SecretReference(os:agent-studios:openai:default)`), never revealing resolved values.
- In CLI commands (`ags provider get`, `ags provider test`), secret resolution outputs only length verification (e.g. `✓ Resolved (51 chars)`), preventing shoulder-surfing or terminal buffer exfiltration.

---

## 4. Verification and Audit Checklist

| Security Control | Implementation | Verification Test | Status |
| :--- | :--- | :--- | :--- |
| **No Plaintext Secrets in Snapshots** | `SecretReference` indirection in `ProviderInstance` | `test_security_bearer_stores_secret_reference_not_secret` | PASS |
| **URL Userinfo Injection Rejected** | `EndpointProfile` parses and rejects URLs with credentials | `test_security_url_userinfo_rejected` | PASS |
| **Sensitive Header Injection Blocked** | Endpoint headers filter out `Authorization` and `x-api-key` | `test_security_authorization_header_rejected`, `test_security_x_api_key_header_rejected` | PASS |
| **Zero Substring Routing Leaks** | Exact instance ID and model ID mapping | `test_mandatory_multi_instance_9router_custom_scenario` | PASS |
| **Atomic Importer Rollback** | Staged snapshot validation with rollback on corrupt data | `test_snapshot_atomic_rollback_on_error` | PASS |
