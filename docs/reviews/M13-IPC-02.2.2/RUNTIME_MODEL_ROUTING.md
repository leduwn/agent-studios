# M13 IPC-02.2.2 Explicit Production Runtime & Model Routing Audit

**Date**: 2026-10-30  
**Target Branch**: `feat/m13-ipc-02-control-plane` (Pull Request #19)  
**Status**: VERIFIED & DETERMINISTIC  

---

## 1. Problem Statement: Silent Arbitrary Fallback

In earlier prototypes of task auto-execution, model and runtime selection used arbitrary fallbacks:
```rust
// FLAWED PROTOTYPE PATTERNS:
let model = catalog.list_models().first().map(|m| m.model_ref());
let runtime_instance = runtime.discover().await?.into_iter().find(|i| i.is_available());
```

In production deployments with multi-model catalogs (e.g., both `gpt-4o` and `gpt-4o-mini`, or `claude-3-5-sonnet` and `claude-3-5-haiku`), silent fallback to the first catalog entry:
1. Causes non-deterministic execution where agent reasoning capacity varies based on catalog registration order.
2. Risks routing high-cost or unverified models unintentionally.
3. Obscures misconfigured deployments where an intended model is unavailable.

---

## 2. Deterministic Routing Architecture

The auto-execution engine in `crates/ipc-transport/src/connection.rs` (`execute_task_lifecycle`) enforces a strict, deterministic resolution hierarchy.

### 2.1 Model Selection Hierarchy
1. **Explicit Builder Configuration**: If `default_model_ref` is set via `IpcServerBuilder::with_default_model_ref`, verify that the model exists in `provider_catalog`.
   - If found: select the model.
   - If missing: **FAIL CLOSED**. Record run outcome as failed with:
     `Configured model '<ref>' not found in provider catalog`. Transition run and task to `Failed`.
2. **Environment Variable Configuration**: If not set on builder, check `AGENT_STUDIOS_DEFAULT_MODEL` (formatted as `<provider_instance_id>/<model_id>`).
   - If valid and present: select the model.
   - If malformed or missing: proceed to catalog check.
3. **Single-Model Catalog Auto-Selection**: If no explicit route is configured and the catalog contains **exactly 1 model**, select that model unambiguously.
4. **Multi-Model Catalog Ambiguity Rejection**: If the catalog contains **more than 1 model** and no explicit route is configured:
   - **FAIL CLOSED**.
   - Record run outcome with descriptive actionable error:
     `Multiple models available (<N>) in catalog but no explicit model route configured. Specify default model via IpcServerBuilder::with_default_model_ref or AGENT_STUDIOS_DEFAULT_MODEL environment variable`.
   - Transition task and run to `Failed`.
5. **Empty Catalog**: If the catalog has 0 models, pass `None` (for test fixtures and non-LLM mock environments).

### 2.2 Runtime Instance Selection Hierarchy
1. **Explicit Builder Configuration**: If `configured_instance_id` is set via `IpcServerBuilder::with_configured_instance_id`:
   - Verify instance is in `discover()` output and is available.
   - If missing or unavailable: **FAIL CLOSED**. Record failure with:
     `Configured runtime instance '<id>' is unavailable or not found`.
2. **Unambiguous Single Instance**: If exactly 1 instance is available, select it.
3. **Ambiguous or Missing Instances**: If 0 or >1 instances are available without an explicit route, fail closed with descriptive configuration error.

---

## 3. Verification & Evidence

The following integration tests in `crates/ipc-transport/tests/m13_ipc_02_2_integration_tests.rs` verify deterministic model routing:
- `test_deterministic_model_routing_multi_model_unconfigured_fails_closed`:
  - Registers 2 models in provider catalog.
  - Builds server without explicit default model.
  - Submits task.
  - Verifies task immediately transitions to `Failed`.
  - Verifies run outcome event in replay buffer contains: `Multiple models available (2) in catalog but no explicit model route configured`.
- `test_deterministic_model_routing_missing_model_fails_closed`:
  - Registers catalog with 1 model.
  - Configures `default_model_ref` with non-existent model ID.
  - Submits task.
  - Verifies task immediately transitions to `Failed`.
  - Verifies error contains: `not found in provider catalog`.
- `test_deterministic_model_routing_configured_model_succeeds`:
  - Registers 2 models in catalog.
  - Configures explicit `default_model_ref` pointing to model 1.
  - Submits task.
  - Verifies task transitions to `Running` and runtime session starts successfully.
