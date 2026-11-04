# M15-04 IPC Recovery Protocol & Diagnostics Design

## 1. Specification

Milestone M15-04 introduces a dedicated, read-only diagnostic command to allow operators, CLI tools, and desktop UI wrappers to inspect daemon recovery status:

```rust
// In agent-studios-ipc-protocol:
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcCommand {
    // ... existing variants
    GetRecoveryStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "resp", rename_all = "snake_case")]
pub enum IpcResponse {
    // ... existing variants
    RecoveryStatus { status: RecoveryStatusReport },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RecoveryStatusReport {
    pub last_recovery_at: Option<DateTime<Utc>>,
    pub interrupted_tasks_count: usize,
    pub interrupted_runs_count: usize,
    pub worktrees_reconciled: usize,
    pub worktrees_missing: usize,
    pub worktrees_foreign: usize,
    pub database_healthy: bool,
    pub database_path: Option<String>,
    pub total_persisted_events: usize,
}
```

---

## 2. Security Boundaries & Authorization

1. **Authentication Requirement:** `GetRecoveryStatus` requires an authenticated session (`Authenticated` state). Unauthenticated connections receive `TransportError::AuthenticationFailed`.
2. **Read-Only Invariant:** `GetRecoveryStatus` does not accept mutation parameters or trigger disk operations; it queries the cached `Arc<Mutex<RecoveryStatusReport>>` assembled during daemon startup.
3. **No Sensitive Leakage:** Database path reporting is sanitized and provides operational visibility without exposing credentials, provider keys, or token secrets.
