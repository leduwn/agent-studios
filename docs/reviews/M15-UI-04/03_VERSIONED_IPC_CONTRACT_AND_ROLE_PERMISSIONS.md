# M15 UI-04 — Versioned IPC Contract & Role-Based Permissions

## 1. Protocol Contract Additions

The authoritative Rust daemon M15-04.1 introduces the `GetRecoveryStatus` command and `RecoveryStatus` response.

### Command Wire Envelope
```json
{
  "type": "get_recovery_status"
}
```

### Response Wire Envelope (Internally Tagged Enum)
```json
{
  "type": "recovery_status",
  "status": {
    "last_recovery_at": "2026-11-02T02:00:00Z",
    "interrupted_tasks_count": 1,
    "interrupted_runs_count": 1,
    "worktrees_reconciled": 2,
    "worktrees_missing": 0,
    "worktrees_foreign": 0,
    "database_healthy": true,
    "database_path": null,
    "total_persisted_events": 42
  }
}
```

---

## 2. Least-Privilege Role Authorization

- **Standard Client Role (`ClientRole::Standard`)**:
  - `database_path` is serialized as `null` / `undefined`.
  - Standard clients can inspect operational health, interrupted counts, and event totals, but cannot inspect absolute host filesystem paths.
- **Admin Client Role (`ClientRole::Admin`)**:
  - `database_path` is populated with the verified filesystem path (e.g. `C:\Users\Duwn\AppData\Roaming\AgentStudios\agsd.db`).
  - Required for administrative backup, diagnostics, and debugging tools.

---

## 3. Durable Task States After Recovery

When the daemon restarts following an abrupt shutdown or crash:
1. In-flight tasks in `TaskState::Running` are transitioned to `TaskState::Failed` during startup reconciliation.
2. In-flight runs in `RunState::Running` are transitioned to `RunState::Failed`.
3. Dangling approvals in `ApprovalStatus::Requested` are transitioned to `ApprovalStatus::Cancelled`.
4. Domain failure reason is set to `"daemon_interrupted"`.
5. Historical task graph, sequence numbers, and artifacts are fully preserved from SQLite WAL storage.
