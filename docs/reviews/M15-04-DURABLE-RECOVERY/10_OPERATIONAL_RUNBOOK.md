# M15-04 Operational Runbook & Troubleshooting

## 1. Daemon Configuration

The `agsd` binary accepts CLI flags and environment variables for persistence:

```bash
agsd --state_dir /path/to/state --db_path /path/to/state/agsd.db --key_id ags-client
```

### Defaults:
- **Windows:** `%APPDATA%\AgentStudios\agsd.db`
- **Linux/macOS:** `~/.local/share/agent-studios/agsd.db`

---

## 2. Recovery Status Diagnostic Query

Clients can query recovery diagnostics via the IPC protocol using the `GetRecoveryStatus` command:

```json
{"cmd": "get_recovery_status"}
```

Expected response:
```json
{
  "resp": "recovery_status",
  "status": {
    "last_recovery_at": "2026-11-02T10:00:00Z",
    "interrupted_tasks_count": 0,
    "interrupted_runs_count": 0,
    "worktrees_reconciled": 2,
    "worktrees_missing": 0,
    "worktrees_foreign": 0,
    "database_healthy": true,
    "database_path": "/home/user/.local/share/agent-studios/agsd.db",
    "total_persisted_events": 142
  }
}
```

---

## 3. Troubleshooting Database Issues

### Issue 1: `StorageFailure: Failed to open SQLite store: ...`
- **Cause:** Database file permission issue or physical corruption.
- **Resolution:** Check file permissions (`0600` recommended). Run `sqlite3 agsd.db "PRAGMA integrity_check;"`. If corrupt, restore from backup; do not truncate.

### Issue 2: Interrupted Tasks after Unclean Daemon Exit
- **Observation:** Tasks appear as `Failed` with reason `"daemon_interrupted"`.
- **Expected Behavior:** This is normal and intentional after a crash. Inspect partial changes in the task's worktree using `git status` / `git diff`, and resubmit the task if needed.
