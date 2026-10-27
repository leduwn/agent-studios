# Milestone M12.1 — Integration Test Execution & Protocol Parity Report

- **Milestone**: `M12.1 — Claude Code Runtime Hardening & Protocol Corrections`
- **Execution Run**: `RUN-20261027-M12.1-hardening`
- **Date**: `2026-10-28T00:15:00+07:00`
- **Lead Owner**: Principal Engineering Lead & Independent QA Lead
- **Target Repository**: `https://github.com/leduwn/agent-studios`
- **Active Branch**: `feat/m12-claude-code-runtime`
- **Test Framework**: Rust cargo test harness (`tokio::test`, `serde_json`, `tempfile`)
- **Host Environment**: Windows 11 Home Single Language 10.0.26200, Rust 1.86+, Claude Code CLI `2.1.310`

---

## 1. Executive Test Suite Summary

All 45 integration and protocol verification tests in `agent-studios-claude-code-runtime` passed cleanly with zero failures, zero errors, and zero warnings.

```
test result: ok. 45 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
```

### Test Suite Breakdown

| Test Suite File | Tests Count | Result | Scope / Target |
|---|:---:|:---:|---|
| `tests/stream_and_subagent_tests.rs` | 14 | **14 PASSED** | M12-REV-001 (Wire protocol envelope), M12-REV-006 (Event mapping), NDJSON stream reader, subagent DAG |
| `tests/runtime_lifecycle_tests.rs` | 14 | **14 PASSED** | M12-REV-002 (Single SessionStarted), M12-REV-005 (Exit reaping & classification), M12-REV-007 (Capabilities) |
| `tests/security_and_budget_tests.rs` | 13 | **13 PASSED** | M12-REV-003 (Fail-closed budget parsing), M12-REV-004 (Active approval watchdog), workspace escapes |
| `tests/sandbox_containment_tests.rs` | 4 | **4 PASSED** | Win32 Job Object containment, kill-on-close policy, memory limits |
| **Total** | **45** | **45 PASSED** | **100% Pass Rate** |

---

## 2. Protocol Fixture Roundtrip & Wire Envelope Verification (M12-REV-001 & M12-REV-006)

### 2.1 Nested Envelope Schema Compliance
In `tests/stream_and_subagent_tests.rs`:
- `test_control_response_allow_wire_schema_and_roundtrip`: Validates that serializing `ControlResponse::allow("req_example", Some(json!({})))` produces the exact nested wire schema required by the Claude Agent SDK:
  ```json
  {
    "type": "control_response",
    "response": {
      "subtype": "success",
      "request_id": "req_example",
      "response": {
        "behavior": "allow",
        "updatedInput": {}
      }
    }
  }
  ```
- `test_control_response_deny_wire_schema_and_roundtrip`: Validates deny response formatting:
  ```json
  {
    "type": "control_response",
    "response": {
      "subtype": "success",
      "request_id": "req_example",
      "response": {
        "behavior": "deny",
        "message": "Operator rejected execution",
        "interrupt": false
      }
    }
  }
  ```
- `test_control_response_error_wire_schema_and_roundtrip`: Validates error subtype envelope:
  ```json
  {
    "type": "control_response",
    "response": {
      "subtype": "error",
      "request_id": "req_example",
      "error": "Failed to resolve approval: channel closed"
    }
  }
  ```
- `test_resolve_approval_by_id_wire_protocol_integration`: Validates end-to-end integration between `ApprovalAuthority::resolve_approval_by_id` and the wire schema.

### 2.2 Control Response Event Mapping Immunity (M12-REV-006)
- `test_control_response_stream_message_never_emits_interrupted`: Validates that incoming `ControlResponse` frames (whether allow, deny, or error) map to `Vec::new()` and never emit `RuntimeEventKind::Interrupted`.

---

## 3. Lifecycle Invariant & Diagnostic Demotion Verification (M12-REV-002)

### 3.1 Verification Scenario
In `tests/runtime_lifecycle_tests.rs` (`test_startup_sequence_has_single_session_started_and_init_emits_diagnostic`):
1. `SessionEventHub` records `RuntimeEventKind::SessionStarted` at sequence 1.
2. An incoming Claude Code initialization message `{"type":"system","subtype":"init",...}` arrives from stdout.
3. `map_claude_message` maps `SystemMessage::Init` into an informational `RuntimeEventKind::Diagnostic` with level `"info"`.
4. Sequence 2 is emitted as `Diagnostic` with version `2.1.310`, model `claude-sonnet-5`, and tools count 3.
5. Replay and history audits confirm:
   - Exactly **one** `SessionStarted` exists at sequence 1.
   - Sequence 2 is `Diagnostic`.
   - `SessionEventHub::rejects_duplicate_session_started` confirms subsequent `SessionStarted` attempts fail with `InvalidSequenceOrder`.

---

## 4. Budget Fail-Closed & Rejection Verification (M12-REV-003)

### 4.1 Malformed String Rejection
In `tests/security_and_budget_tests.rs` (`test_start_rejects_malformed_max_budget_usd_without_launching_process`):
The adapter strictly tests and rejects before process spawn:
- `"invalid"`
- `"-5.0"` (negative float)
- `"-0.01"` (negative float)
- `"NaN"` / `"nan"`
- `"inf"` / `"-infinity"`
- `""` (empty string)
- `"   "` (whitespace string)
- `"unlimited"` (non-numeric string)
- `"$10.00"` (currency symbol prefix)

All invalid inputs immediately fail with `RuntimeError::InvalidConfiguration`, preventing silent budget bypass.

### 4.2 Configuration Reference Rejection
In `test_config_ref_budget_validation_and_rejection`:
- JSON config references containing negative budgets (`{"max_budget_usd": -10.0}`) or NaN strings (`{"max_budget_usd": "NaN"}`) are rejected fail-closed.
- Query-style config references (`budget=-5.0`) are rejected fail-closed.

---

## 5. Active Approval Timeout & Watchdog Verification (M12-REV-004)

### 5.1 Active Watchdog Execution
In `tests/security_and_budget_tests.rs`:
- `test_approval_timeout_watchdog_fires_and_produces_deny_response`:
  - Registers a tool approval request for `rm -rf /`.
  - Spawns an async timeout watchdog with a 30ms timeout.
  - The watchdog elapses, automatically invoking the callback with a formatted deny `ControlResponse`:
    `{"type":"control_response","response":{"subtype":"success","request_id":"...","response":{"behavior":"deny","message":"Approval timed out after 30ms (fail-closed)","interrupt":false}}}`.
  - Confirms the request is evicted from pending tracking so it cannot be later approved.
- `test_approval_resolved_before_timeout_cancels_watchdog`:
  - Registers a tool approval request with a 200ms timeout.
  - Resolves approval affirmatively within 10ms.
  - Watchdog cancellation signal cancels the background watchdog cleanly before deadline.
- `test_approval_watchdog_cancelled_on_session_termination`:
  - Verifies `cancel_all_pending()` cancels all active watchdog tasks and fails pending approvals closed.

---

## 6. Process Exit & Exit Code Classification Verification (M12-REV-005)

### 6.1 Exit Reaping & Classification
In `tests/runtime_lifecycle_tests.rs` (`test_process_exit_code_classification` and `test_process_exit_status_wrapper`):
- Exit Code `0`: Classified as `RuntimeLifecycleState::Completed` with event `Completed { summary: None }`.
- Exit Code `2`: Classified as `RuntimeLifecycleState::Failed` with event `Failed { safe_error_summary: "Claude Code terminated: budget limit reached or authentication error" }`.
- Exit Code `130 / 143`: Classified as `RuntimeLifecycleState::Stopped` with event `Stopped`.
- Exit Code `1` (Crash with stderr): Classified as `RuntimeLifecycleState::Failed` with event `Failed { safe_error_summary: "Claude Code process exited with code 1: fatal error: could not connect" }`.
- Exit Code `127` (Command not found with empty stderr): Classified as `RuntimeLifecycleState::Failed` with event `Failed { safe_error_summary: "Claude Code process exited with status code 127" }`.
- Exit Code `None` (Process killed): Classified as `RuntimeLifecycleState::Stopped` with event `Stopped`.

---

## 7. Truthful Capability Advertising (M12-REV-007)

### 7.1 Verified Declarations
In `tests/runtime_lifecycle_tests.rs` (`test_capabilities_declaration_matches_verified_behavior`):
- `caps.supports(RuntimeCapability::PersistentSession)`: Asserts `false` (`Unsupported`).
- `caps.supports(RuntimeCapability::Resume)`: Asserts `true` (supported for in-process turn continuation).
- `caps.supports(RuntimeCapability::Approvals)`: Asserts `true`.
- `caps.supports(RuntimeCapability::StreamingEvents)`: Asserts `true`.
- `caps.supports(RuntimeCapability::Tools)`: Asserts `true`.
- `caps.supports(RuntimeCapability::Subagents)`: Asserts `true`.

---

## 8. Host CLI Verification (Claude Code `2.1.310`)

### 8.1 Local Binary Audit
- **Executable**: `C:\Users\Duwn\.local\bin\claude.exe`
- **Reported Version**: `2.1.310 (Claude Code)`
- **CLI Flag Audit**:
  - `--print`: Supported (headless batch mode).
  - `--output-format stream-json`: Supported (structured NDJSON event stream).
  - `--input-format stream-json`: Supported (structured NDJSON input stream on stdin).
  - `--permission-mode`: Supported (`manual`, `prompt`, `bypass`).
  - `--max-budget-usd`: Supported (strictly bounded monetary limit).
  - `--no-session-persistence`: Supported (disables session state writes to local disk).

### 8.2 Parity Analysis Matrix

| Feature / Protocol Element | Claude Code CLI `2.1.310` | Adapter Implementation | Parity Assessment |
|---|---|---|:---:|
| Wire Control Response | Expects `subtype: success`, `behavior: allow/deny` | Produces typed nested schema via `ControlResponse` | **100% PARITY** |
| Tool Approvals | Dispatches `control_request` on stdin | Handled by `ApprovalAuthority` + async watchdog | **100% PARITY** |
| Session Invariants | Emits `system/init` with telemetry | Demoted to `Diagnostic`, single `SessionStarted` | **100% PARITY** |
| Budget Enforcement | CLI exits code 2 when budget depleted | Pre-validated fail-closed; exit 2 mapped to Failed | **100% PARITY** |
| Process Reaping | Windows process tree | Managed via Win32 Job Objects + `wait_for_exit()` | **100% PARITY** |
