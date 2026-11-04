# Milestone M12.1 — Lifecycle, Process Exit & Capabilities Hardening

- **Milestone**: `M12.1 — Claude Code Runtime Hardening & Protocol Corrections`
- **Execution Run**: `RUN-20261027-M12.1-hardening`
- **Date**: `2026-10-27T23:58:00+07:00`
- **Lead Owner**: Agent B (Runtime Lifecycle) & Principal Engineering Lead
- **Target Repository**: `https://github.com/leduwn/agent-studios`
- **Impacted Files**:
  - `agent-studios-rs/crates/claude-code-runtime/src/mapping.rs`
  - `agent-studios-rs/crates/claude-code-runtime/src/process.rs`
  - `agent-studios-rs/crates/claude-code-runtime/src/runtime.rs`
  - `agent-studios-rs/crates/claude-code-runtime/src/discovery.rs`
  - `agent-studios-rs/crates/claude-code-runtime/tests/runtime_lifecycle_tests.rs`

---

## 1. Technical Audit of M12-REV-002: Duplicate `SessionStarted`

### 1.1 Root Cause
In Milestone M12:
- `ClaudeCodeRuntime::start()` unconditionally emitted `RuntimeEventKind::SessionStarted` at sequence 1 via `hub.emit()`.
- However, when Claude Code printed its initialization frame `{"type":"system","subtype":"init",...}`, `src/mapping.rs` did:
  ```rust
  SystemMessage::Init(init) => {
      vec![RuntimeEventKind::SessionStarted {
          session_id: session_ref.session_id,
          instance_id: session_ref.instance_id,
      }]
  }
  ```
  This attempted to emit `SessionStarted` again at sequence 2, which violated the invariant that `SessionStarted` is emitted strictly once per session.

### 1.2 Remediated Mapping
In `src/mapping.rs`:
```rust
SystemMessage::Init(init) => {
    let version = init.claude_code_version.as_deref().unwrap_or("unknown");
    let model = init.model.as_deref().unwrap_or("default");
    let tools_count = init.tools.as_ref().map(|t| t.len()).unwrap_or(0);
    vec![RuntimeEventKind::Diagnostic {
        level: "info".to_string(),
        message: SanitizedRuntimeMessage::new(format!(
            "Claude Code initialized: version {version}, model {model}, tools: {tools_count}"
        )),
    }]
}
```
This preserves the startup metadata in the event stream as an informational diagnostic while ensuring `SessionStarted` occurs exactly once at sequence 1.

---

## 2. Technical Audit of M12-REV-005: Process Exit Handling & Terminal Logic

### 2.1 Root Cause
In `process.rs`, `exit_status(&self)` returned `*lock` on an internal `exit_status` field. However, `exit_status` was only populated if `is_alive()` or `terminate()` had been called. When the stdout stream closed on normal or abnormal process exit, the background stream task called `process.exit_status().await`, which frequently returned `None`.
The runtime then fell through to a default branch that treated `None` as `Stopped`:
```rust
None => {
    *st = RuntimeLifecycleState::Stopped;
    let _ = hub_clone.emit(RuntimeEventKind::Stopped).await;
}
```
This misclassified natural process completions (code 0) and fatal crashes (nonzero) as user-initiated stops.

### 2.2 Remediated Process Reaping
In `src/process.rs`, `wait_for_exit(&self)` was added:
```rust
pub async fn wait_for_exit(&self) -> Option<std::process::ExitStatus> {
    let mut lock = self.child.lock().await;
    if let Some(ref mut child) = *lock {
        if let Ok(status) = child.wait().await {
            let mut exit_lock = self.exit_status.lock().await;
            *exit_lock = Some(status);
            return Some(status);
        }
    }
    let exit_lock = self.exit_status.lock().await;
    *exit_lock
}
```
When stdout closes, the background task calls `process.wait_for_exit().await` to safely reap the child and obtain the true exit status:
- Exit Code `0`: Transitions to `RuntimeLifecycleState::Completed`, emits `Completed`.
- Exit Code `2`: Transitions to `RuntimeLifecycleState::Failed`, emits `Failed` ("Claude Code terminated: budget limit reached or authentication error").
- Exit Code `130 / 143`: Transitions to `RuntimeLifecycleState::Stopped`, emits `Stopped`.
- Other Nonzero: Transitions to `RuntimeLifecycleState::Failed`, reads the 64 KiB stderr ring buffer, and emits `Failed` with sanitized stderr.
- Stream read errors (e.g. DoS overlong line or stream corruption): Terminates child, sets state to `Failed`, and emits `Failed` with error summary.

---

## 3. Technical Audit of M12-REV-007: Capabilities & Persistence Accuracy

### 3.1 Discrepancy
`discovery.rs` declared:
- `persistent_session`: `Supported`
- `resume`: `Supported`

However, `process.rs` invoked the CLI with:
`--no-session-persistence`
The adapter did not implement session restoration from disk across process terminations or Agent Studios restarts. Claiming `persistent_session` support was untruthful.

### 3.2 Corrected Capability Profile
In `src/discovery.rs`:
```rust
persistent_session: RuntimeCapabilitySupport::Unsupported,
```
- **Rationale**: Claude Code is launched in headless automation mode with `--no-session-persistence`. While in-process turn continuation (`resume`) is supported while the session process remains active, restoring cold session state from external disk storage across application restarts is unsupported.
- This declaration truthfully reflects the current adapter capabilities.
