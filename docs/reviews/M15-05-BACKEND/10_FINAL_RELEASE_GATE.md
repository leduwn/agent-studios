# 10 — Final Release Gate & Handoff: M15-05 Backend

## 1. Release Gate Criteria & Verification Status

| Gate Item | Requirement | Verification Status |
|---|---|---|
| **Durable Replay on Restart** | `SubscribeEvents { from_sequence: K + 1 }` replays historical events from SQLite after daemon restart without `SequenceGapRecoverable` | **VERIFIED** (`test_durable_replay_after_daemon_restart_from_persisted_sqlite`) |
| **Ring Buffer Eviction Fallback** | Historical queries for events evicted from in-memory ring buffer fall back transparently to SQLite store slice | **VERIFIED** (`test_durable_replay_sqlite_fallback_beyond_ring_buffer_capacity`) |
| **Caught-Up Reconnect** | Reconnecting with `from_sequence = latest + 1` completes handshake cleanly without backfill delays | **VERIFIED** (`test_caught_up_client_reconnect_after_restart`) |
| **Race-Free Broadcast Handshake** | Zero event loss and zero duplicate frames when live events are emitted during historical replay | **VERIFIED** (`test_race_free_sqlite_replay_and_concurrent_broadcast_handshake`) |
| **Subprocess Crash Recovery** | Daemon crash/kill restarts cleanly with single-writer lock, orphaned task reconciliation, and no file corruption | **VERIFIED** (`m15_04_process_recovery_tests.rs`) |
| **Runtime Tier Gating** | Tiers A, B, C verified; Tier D strictly gated with $0 billable AI calls in tests | **VERIFIED** (`m15_multi_runtime_tests.rs`) |
| **Formatting Compliance** | `cargo fmt --all -- --check` passes cleanly with 0 diffs | **VERIFIED** |
| **Linting Compliance** | `cargo clippy --all-targets -- -D warnings` produces 0 warnings and 0 errors | **VERIFIED** |
| **Test Suite Health** | All unit and integration test suites pass across all backend crates | **VERIFIED (187+ tests passing)** |
| **Pull Request Integrity** | PR #30 kept in **Draft** state; no merge to `main`; no force-push | **VERIFIED** |

---

## 2. Pull Request & Branch Information

- **Branch**: `feat/m15-04-durable-state-recovery`
- **Pull Request**: `https://github.com/leduwn/agent-studios/pull/30`
- **PR State**: **Draft** (Do NOT mark ready for review or auto-merge)
- **Worktree**: `D:\Projects\agent-studios-m15-04`

---

## 3. Handoff Notes for Frontend Team (Claude 2 / PR #27)

1. **Reconnection Sequence Contract**:
   - The frontend client may safely resume event subscriptions after disconnect/restart by passing its stored `last_received_sequence + 1` to `SubscribeEvents`.
   - The daemon will automatically backfill from SQLite if the sequence has rolled off the server's in-memory ring buffer.
   - The client will never receive duplicate sequence frames.
2. **Recovery Status Exposure**:
   - When authenticated with standard credentials, `database_path` is `null` in `GetRecoveryStatus`. Admin credentials are required to view the host filesystem path.
3. **Draft Status**:
   - Both PR #30 (Backend) and PR #27 (Frontend) remain in Draft status pending coordinated end-to-end integration.
