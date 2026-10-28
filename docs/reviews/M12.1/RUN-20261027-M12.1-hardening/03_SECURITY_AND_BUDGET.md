# Milestone M12.1 — Security, Approval Watchdog & Budget Hardening

- **Milestone**: `M12.1 — Claude Code Runtime Hardening & Protocol Corrections`
- **Execution Run**: `RUN-20261027-M12.1-hardening`
- **Date**: `2026-10-27T23:57:00+07:00`
- **Lead Owner**: Agent C (Security & Approvals) & Principal Engineering Lead
- **Target Repository**: `https://github.com/leduwn/agent-studios`
- **Impacted Files**:
  - `agent-studios-rs/crates/claude-code-runtime/src/approval.rs`
  - `agent-studios-rs/crates/claude-code-runtime/src/runtime.rs`
  - `agent-studios-rs/crates/claude-code-runtime/tests/security_and_budget_tests.rs`

---

## 1. Technical Audit of M12-REV-003: Budget Validation Bypass

### 1.1 Defect Mechanism
In `ClaudeCodeRuntime::start()`, the metadata budget check was implemented as:
```rust
// M12 INITIAL (DEFECTIVE):
let max_budget_usd = request
    .metadata
    .get("max_budget_usd")
    .and_then(|v| v.as_str().parse::<f64>().ok());
```
When a caller passed an invalid, negative, NaN, or non-numeric string (e.g. `"-100"`, `"unlimited"`, `"NaN"`), `.ok()` converted the error to `None`. The runtime then launched the Claude Code process with no budget flag (`--max-budget-usd` was omitted), silently discarding the operator's intended spend restriction.

### 1.2 Remediated Validation Pipeline
In M12.1:
1. `request.metadata.get("max_budget_usd")` explicitly distinguishes between absent and present values.
2. If absent: `max_budget_usd = None`.
3. If present: The string value is parsed with `parse_max_budget_usd()`:
   - Validates that the string is non-empty.
   - Parses as `f64`.
   - Rejects `NaN`, `f64::INFINITY`, `f64::NEG_INFINITY`.
   - Rejects negative values (`amount < 0.0`).
4. If validation fails, `start()` aborts immediately and returns:
   ```rust
   RuntimeError::invalid_configuration(format!(
       "Invalid 'max_budget_usd' specified in metadata: {err}"
   ))
   ```
5. The subprocess is **never spawned** when a malformed budget is provided.

---

## 2. Technical Audit of M12-REV-004: Approval Timeout Watchdog

### 2.1 Defect Mechanism
In Milestone M12, `ApprovalAuthority` contained an async `request_approval()` method with timeout handling, but the runtime loop never used it. In `runtime.rs`:
```rust
if let ClaudeStreamMessage::ControlRequest(ref req) = msg {
    approval_clone.register_request(req);
}
```
The request was registered, and `RuntimeEventKind::ApprovalRequested` was emitted to the event hub. However, if the external operator or host UI failed to call `resolve_approval()`, no active mechanism ever enforced the timeout. The child process remained blocked on stdin indefinitely waiting for a response that would never come.

### 2.2 Active Watchdog Implementation
In M12.1, when `ControlRequestPayload::CanUseTool` is received:
1. `approval_authority.register_request(req)` registers the request and creates a cancellation channel.
2. An asynchronous watchdog task is spawned with a defined deadline (default: 60 seconds):
   ```rust
   let approval_id = approval_authority.register_request(req);
   let process_stdin = Arc::clone(&process);
   let hub = Arc::clone(&hub);
   let timeout_duration = Duration::from_secs(60);

   tokio::spawn(async move {
       tokio::select! {
           _ = tokio::time::sleep(timeout_duration) => {
               // Timeout reached: fail-closed to Deny
               let deny_frame = ControlResponse::deny(
                   &request_id,
                   "Tool approval timed out after 60s (fail-closed)",
                   false
               );
               let _ = process_stdin.write_stdin_line(&serde_json::to_string(&deny_frame).unwrap()).await;
               let _ = hub.emit(RuntimeEventKind::Diagnostic {
                   level: "warn".to_string(),
                   message: SanitizedRuntimeMessage::new(format!(
                       "Tool approval for request '{request_id}' timed out; sent fail-closed denial"
                   )),
               }).await;
           }
           _ = cancel_rx => {
               // Resolved normally by operator before timeout
           }
       }
   });
   ```
3. If the host resolves the approval via `resolve_approval_by_id`, the cancellation signal is triggered and the watchdog exits immediately without sending a duplicate response.
4. On session `stop()` or process termination, all active watchdogs are cancelled and pending requests fail closed.

---

## 3. Regression Tests

Added to `tests/security_and_budget_tests.rs`:
1. `test_start_rejects_negative_budget_string`: Verifies `"-10.0"` returns `InvalidConfiguration` before spawn.
2. `test_start_rejects_nan_and_inf_budget`: Verifies `"NaN"` and `"inf"` fail closed.
3. `test_start_rejects_non_numeric_budget`: Verifies `"unlimited"` fails closed.
4. `test_approval_timeout_watchdog_fires_deny`: Verifies watchdog emits deny frame on expiration.
5. `test_approval_resolved_before_timeout_cancels_watchdog`: Verifies early resolution cancels watchdog cleanly.
