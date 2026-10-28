# Milestone M12.1 — Independent Review Findings Ledger

- **Milestone**: `M12.1 — Claude Code Runtime Hardening & Protocol Corrections`
- **Execution Run**: `RUN-20261027-M12.1-hardening`
- **Date**: `2026-10-27T23:55:00+07:00`
- **Principal Engineering Lead**: Agent Studios Lead
- **Target Repository**: `https://github.com/leduwn/agent-studios`
- **Active Branch**: `feat/m12-claude-code-runtime`
- **Review Baseline Commit**: `710051e6566e12cf869e2db02590f81726fe7943`
- **Audit Source**: External Independent Review (ChatGPT Review)
- **Status**: **FINDINGS AUDITED & REMEDIATION ACTIVE**

---

## 1. Executive Summary of Audit Findings

The independent review of Milestone M12 identified eight specific technical defects and capability overstatements in the initial `agent-studios-claude-code-runtime` implementation. While the foundational crate architecture and Win32 Job Object containment were sound, wire-level protocol discrepancies, lifecycle event deduplication bugs, budget validation bypasses, and an unintegrated approval timeout prevented verified production readiness.

Milestone M12.1 directly remediates all eight findings with verified code, deterministic test coverage, and updated intelligence monitoring.

---

## 2. Findings Ledger

| ID | Title | Severity | Impacted Files | Remediation Status |
|---|---|:---:|---|:---:|
| **M12-REV-001** | Incorrect `ControlResponse` Wire Format | **P0** (Blocker) | `protocol.rs`, `approval.rs`, `runtime.rs` | **REMEDIATING (Agent A)** |
| **M12-REV-002** | Duplicate `SessionStarted` Event Emission | **P1** (Critical) | `mapping.rs`, `runtime.rs` | **REMEDIATING (Agent B)** |
| **M12-REV-003** | Silent Budget Validation Bypass | **P1** (Critical) | `runtime.rs`, `approval.rs` | **REMEDIATING (Agent C)** |
| **M12-REV-004** | Approval Timeout Not Integrated in Loop | **P0** (Blocker) | `approval.rs`, `runtime.rs` | **REMEDIATING (Agent C)** |
| **M12-REV-005** | Incorrect Process Exit Status & Terminal Logic | **P1** (Critical) | `process.rs`, `runtime.rs` | **REMEDIATING (Agent B)** |
| **M12-REV-006** | False `Interrupted` on Control Acknowledgement | **P1** (Critical) | `mapping.rs` | **REMEDIATING (Agent A)** |
| **M12-REV-007** | Capability Inaccuracy (`persistent_session`) | **P2** (Major) | `discovery.rs`, `process.rs` | **REMEDIATING (Agent B)** |
| **M12-REV-008** | Upstream Intelligence Duplication & Coverage | **P2** (Major) | `.github/workflows/upstream-intelligence.yml` | **REMEDIATING (Agent D)** |

---

## 3. Detailed Finding Profiles & Remediation Architecture

### Finding M12-REV-001: Incorrect `ControlResponse` Wire Format
- **Description**: The initial Serde struct for `ControlResponse` placed `request_id` at the top level and omitted the nested `subtype: "success"` envelope required by the Claude Agent SDK wire specification:
  ```json
  // INCORRECT (M12 initial):
  {"type": "control_response", "request_id": "req_01", "response": {"behavior": "allow"}}
  ```
- **Remediation Specification**:
  Redefine `ControlResponse` to serialize into the exact nested envelope:
  ```json
  // CORRECT (M12.1):
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
  Additionally support the `"error"` subtype for protocol-level refusal.

---

### Finding M12-REV-002: Duplicate `SessionStarted` Emission
- **Description**: `ClaudeCodeRuntime::start()` emits `RuntimeEventKind::SessionStarted` at sequence 1 upon initialization. Later, when Claude Code emits its first stdout message `SystemMessage::Init(init)`, `mapping.rs` unconditionally produced a second `SessionStarted` event. This violated `SessionEventHub` sequence invariants and caused potential boundary validation rejections.
- **Remediation Specification**:
  Map `SystemMessage::Init` to `RuntimeEventKind::Diagnostic` with metadata (version, model, tools count, capabilities), ensuring `SessionStarted` is emitted strictly once per session at sequence 1.

---

### Finding M12-REV-003: Silent Budget Validation Bypass
- **Description**: In `ClaudeCodeRuntime::start()`, budget parsing used:
  ```rust
  let max_budget_usd = request.metadata.get("max_budget_usd")
      .and_then(|v| v.as_str().parse::<f64>().ok());
  ```
  If a user supplied an invalid, negative, NaN, or non-numeric budget string (e.g. `"-5"`, `"NaN"`, `"invalid"`), `.ok()` silently converted it to `None`, launching the subprocess with zero budget ceiling.
- **Remediation Specification**:
  Explicitly distinguish absent from invalid. Validate budget strings through `parse_max_budget_usd()`. If invalid or negative, fail closed immediately with `RuntimeError::invalid_configuration` before process spawn.

---

### Finding M12-REV-004: Approval Timeout Not Integrated in Runtime Loop
- **Description**: While `ApprovalAuthority` possessed an asynchronous `request_approval` method with timeout logic, the runtime background task only called `register_request()`. If an operator did not respond to `ApprovalRequested`, no timeout watchdog existed to resolve the wire request fail-closed, leaving Claude Code blocked indefinitely on stdin.
- **Remediation Specification**:
  Implement an active timeout watchdog per registered approval request. If the deadline (e.g. 60s) expires before host resolution, the watchdog automatically issues a fail-closed `ControlResponse` with `behavior: "deny"`, writes it to stdin, and emits a warning diagnostic.

---

### Finding M12-REV-005: Incorrect Process Exit Handling & Terminal Logic
- **Description**: In `process.rs`, `exit_status()` read a cached status that was never populated on EOF unless `terminate()` had been explicitly invoked. The runtime treated `None` as `Stopped` and failed to classify nonzero exit codes, premature EOFs, or stream parse errors accurately.
- **Remediation Specification**:
  Add `wait_for_exit(&self)` to await child process termination and obtain the true `ExitStatus`. Classify exit codes:
  - `0` -> `Completed`
  - `2` -> `Failed` ("budget limit reached or authentication error")
  - `130 / 143` -> `Stopped`
  - Other nonzero -> `Failed` with sanitized stderr snapshot.
  If stream parsing encounters fatal corruption or EOF, classify appropriately and reap the child.

---

### Finding M12-REV-006: Incorrect `ControlResponse` Event Mapping
- **Description**: In `mapping.rs`, incoming `ClaudeStreamMessage::ControlResponse` wire messages mapped unconditionally to `RuntimeEventKind::Interrupted`. A control response is an acknowledgement from the host/engine, NOT an interruption.
- **Remediation Specification**:
  Map `ControlResponse` to empty `Vec::new()` (or diagnostic), preventing false interruption transitions.

---

### Finding M12-REV-007: Capability & Persistence Accuracy
- **Description**: `discovery.rs` declared `persistent_session: Supported` while `process.rs` launched Claude Code with `--no-session-persistence`. Disk-persisted reload across process restarts is not implemented.
- **Remediation Specification**:
  Truthfully declare `persistent_session: RuntimeCapabilitySupport::Unsupported`. Clarify that in-process turn continuation (`resume`) is supported, but persistent session across process restarts is unsupported.

---

### Finding M12-REV-008: Upstream Intelligence Reliability
- **Description**: `.github/workflows/upstream-intelligence.yml` only monitored 4 repositories, used hardcoded target strings, created duplicate issues on every run, and had a weekly schedule instead of daily.
- **Remediation Specification**:
  Track all 6 target repositories: `openai/codex`, `anomalyco/opencode`, `anthropics/claude-code`, `microsoft/vscode`, `777genius/agent-teams-ai`, `agentscope-ai/AgentTeams`. Implement state tracking via `.github/upstream-state.json` and query open issues to prevent duplicate issue creation. Schedule daily at 02:00 UTC.
