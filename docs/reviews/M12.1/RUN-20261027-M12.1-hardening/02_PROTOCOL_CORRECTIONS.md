# Milestone M12.1 — Protocol Corrections & Wire Compatibility

- **Milestone**: `M12.1 — Claude Code Runtime Hardening & Protocol Corrections`
- **Execution Run**: `RUN-20261027-M12.1-hardening`
- **Date**: `2026-10-27T23:56:00+07:00`
- **Lead Owner**: Agent A (Protocol Compatibility) & Principal Engineering Lead
- **Target Repository**: `https://github.com/leduwn/agent-studios`
- **Impacted Files**:
  - `agent-studios-rs/crates/claude-code-runtime/src/protocol.rs`
  - `agent-studios-rs/crates/claude-code-runtime/src/mapping.rs`
  - `agent-studios-rs/crates/claude-code-runtime/src/approval.rs`
  - `agent-studios-rs/crates/claude-code-runtime/tests/stream_and_subagent_tests.rs`

---

## 1. Technical Audit of M12-REV-001: `ControlResponse` Wire Envelope

### 1.1 Root Cause Analysis
In Milestone M12, the initial Rust model for `ControlResponse` was:
```rust
// M12 INITIAL (DEFECTIVE):
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlResponse {
    pub r#type: String,
    pub request_id: String,
    pub response: ControlDecision,
}
```
This emitted the following serialized JSON frame over stdin:
```json
{"type":"control_response","request_id":"req_01","response":{"behavior":"allow"}}
```
However, the upstream Claude Agent SDK wire specification and Claude Code CLI (`--input-format=stream-json`) require a nested `response` envelope with a `subtype` discriminator (`"success"` or `"error"`):
```json
{
  "type": "control_response",
  "response": {
    "subtype": "success",
    "request_id": "req_01",
    "response": {
      "behavior": "allow",
      "updatedInput": {}
    }
  }
}
```
Because of this shape mismatch, Claude Code CLI in headless mode failed to correlate host approval responses with in-flight `can_use_tool` requests.

### 1.2 Remediated Type Hierarchy
The corrected type definition in `src/protocol.rs`:
```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlResponse {
    pub r#type: String, // "control_response"
    pub response: ControlResponseEnvelope,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum ControlResponseEnvelope {
    #[serde(rename = "success")]
    Success(ControlResponseSuccess),
    #[serde(rename = "error")]
    Error(ControlResponseError),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlResponseSuccess {
    pub request_id: String,
    pub response: ControlDecision,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlResponseError {
    pub request_id: String,
    pub error: String,
}
```

### 1.3 Constructors & Helper Methods
Convenience constructors were added:
- `ControlResponse::allow(request_id, updated_input)`
- `ControlResponse::deny(request_id, message, interrupt)`
- `ControlResponse::error(request_id, error_message)`

---

## 2. Technical Audit of M12-REV-006: Control Response Event Mapping

### 2.1 Root Cause Analysis
In `src/mapping.rs`:
```rust
// M12 INITIAL (DEFECTIVE):
ClaudeStreamMessage::ControlResponse(_) => {
    vec![RuntimeEventKind::Interrupted {
        reason: SanitizedRuntimeMessage::new("Execution interrupted by host"),
    }]
}
```
Mapping `ControlResponse` to `RuntimeEventKind::Interrupted` was erroneous. A `ControlResponse` wire message is either an acknowledgement from the child process or an echo of an operator's decision. Treating it as an interruption caused sessions to transition unexpectedly into `RuntimeLifecycleState::Interrupted`.

### 2.2 Corrected Mapping
`ControlResponse` now maps to `Vec::new()` (or diagnostic telemetry), ensuring no false interruption transitions occur during normal tool approval exchanges. An actual interrupt is only triggered when explicitly requested via `ClaudeInterruptInput` or when indicated by terminal stop reasons.

---

## 3. Protocol Verification Tests

The following protocol fixture tests were added to `tests/stream_and_subagent_tests.rs`:
1. `test_control_response_allow_wire_serialization`: Verifies nested `subtype: "success"` envelope and `behavior: "allow"`.
2. `test_control_response_deny_wire_serialization`: Verifies nested `behavior: "deny"` with message and interrupt flag.
3. `test_control_response_error_wire_serialization`: Verifies `subtype: "error"` format.
4. `test_control_response_does_not_emit_interrupted_event`: Verifies that mapping `ControlResponse` produces zero interruption events.
