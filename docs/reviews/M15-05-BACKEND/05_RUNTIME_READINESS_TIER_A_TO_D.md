# 05 — Runtime Readiness Tier Audit: Tier A Through Tier D

## 1. Multi-Tier Execution Model

Agent Studios defines four distinct tiers of runtime execution to guarantee safety, deterministic automated verification, and budget enforcement:

```
┌────────────────────────────────────────────────────────────────────────┐
│ Tier A: Pure In-Process Deterministic Fakes (100% Mocked, 0ms, $0)     │
├────────────────────────────────────────────────────────────────────────┤
│ Tier B: Simulated Subprocess & Wiremock Adapters (Process Boundary, $0)│
├────────────────────────────────────────────────────────────────────────┤
│ Tier C: Real Subprocess Acceptance Adapters (CLI Lifecycle, $0 Billable│
├────────────────────────────────────────────────────────────────────────┤
│ Tier D: Real Billable AI Model Execution (Strictly Operator Gated, $$$)│
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Tier Audit & Readiness Assessment

### Tier A: In-Process Deterministic Fakes
- **Scope**: Memory-only test drivers (`FakeRuntime`, mock command processors).
- **Execution Model**: Runs synchronously or via Tokio async tasks inside test binaries.
- **Cost**: $0.00. No network access, no disk I/O, no subprocess creation.
- **Verification**: Covered by `crates/control-plane/tests/integration_tests.rs` (48 tests) and `crates/ipc-protocol` unit tests.
- **Status**: **100% READY & VERIFIED**.

### Tier B: Simulated Subprocess & Lifecycle Adapters
- **Scope**: Actual operating system subprocesses executing deterministic scripts or echo servers (e.g., Python/Node mocks, pipe tests).
- **Execution Model**: Spawns real OS child processes; validates stdin/stdout framing, process termination (`SIGTERM`/`SIGKILL`), exit code propagation, single-writer file locking, and crash recovery.
- **Cost**: $0.00. Zero API calls.
- **Verification**: Covered by `crates/ipc-transport/tests/m15_04_process_recovery_tests.rs` (5 tests) and `crates/ipc-transport/tests/m13_ipc_02_2_integration_tests.rs` (17 tests).
- **Status**: **100% READY & VERIFIED**.

### Tier C: Real CLI Adapter Subprocess Acceptance (Claude Code & OpenCode)
- **Scope**: Integration with local CLI binaries (`claude` CLI, `opencode` CLI) executing `--version`, `--help`, mock prompts, or local dummy targets without invoking remote LLM APIs.
- **Execution Model**:
  - Validates adapter registration via `IpcServerBuilder::with_runtime_adapter`.
  - Verifies approval interception hooks: prompt submission generates `ApprovalRequested` events, waits for operator resolution (`Approve`/`Reject`), and fails closed if unresolved.
  - Verifies fail-fast cancellation: task cancellation sends OS termination signals to the runtime child process and marks the run `Cancelled`.
- **Cost**: $0.00 billable AI tokens. CI tests run strictly without API keys or with dummy keys in isolated test sandboxes.
- **Verification**: Covered by `crates/ipc-transport/tests/m15_multi_runtime_tests.rs` (14 tests) and `crates/ipc-transport/tests/m15_execution_contract_tests.rs` (8 tests).
- **Status**: **100% READY & VERIFIED**.

### Tier D: Real Billable AI Model Production Execution
- **Scope**: Invocation of live commercial LLMs (Anthropic Claude 3.5/3.7/Opus, OpenAI GPT-4o, etc.) with real API keys producing live billable tokens.
- **Security & Authorization Gate**:
  - **STRICTLY GATED**: Never executed in automated CI workflows or unit tests.
  - Requires explicit operator configuration with credentials stored outside the event database (via OS environment variables or secure credential managers).
  - Enforces per-task budget limits, model routing authorization, and operator approval tokens.
  - Task submission endpoints remain disabled in production mode unless explicit operator challenge tokens are validated.
- **Status**: **GATED BY POLICY (0 Billable Calls in CI)**.

---

## 3. Enforcement Invariant Summary

| Requirement | Tier A | Tier B | Tier C | Tier D |
|---|---|---|---|---|
| In-memory determinism | Yes | No | No | No |
| OS Process Isolation | No | Yes | Yes | Yes |
| CLI / Protocol Adapter | Mock | Mock | Real CLI | Real CLI |
| LLM API Token Cost | $0 | $0 | $0 | Billable ($) |
| Allowed in CI Pipeline | Yes | Yes | Yes | **NO (Gated)** |
| Allowed in M15-05 Tests | Yes | Yes | Yes | **NO (Gated)** |
