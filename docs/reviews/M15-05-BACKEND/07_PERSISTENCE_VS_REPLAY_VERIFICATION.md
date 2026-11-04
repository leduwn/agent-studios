# 07 — Persistence vs. Replay Verification

## 1. Overview of M15-05 Test Suite

To verify that `SqliteEventStore` and `ReplayBuffer` function harmoniously across restarts and ring buffer boundaries, M15-05 introduces `crates/ipc-transport/tests/m15_05_durable_replay_tests.rs`.

The suite covers four critical production scenarios:
1. **Restart Replay from Persisted SQLite**: A reconnecting client retrieves missed historical events after daemon restart without triggering `SequenceGapRecoverable`.
2. **SQLite Fallback Beyond Ring Buffer Capacity**: Deep historical queries for events that have rolled off the in-memory ring buffer fall back transparently to SQLite.
3. **Caught-Up Client Reconnect**: A client reconnecting with `from_sequence = latest + 1` receives immediate live events without historical backfill delays.
4. **Race-Free SQLite Replay & Concurrent Broadcast Handshake**: Verification that historical backfill and concurrent live emissions are merged in strict monotonic order with zero gaps and zero duplicates.

---

## 2. Test Cases & Execution Details

### Test 1: `test_durable_replay_after_daemon_restart_from_persisted_sqlite`
- **Scenario**:
  - Server 1 commits events 1 and 2 (two studio creations).
  - Client 1 subscribes from sequence 1, receives event 1, and drops connection.
  - Server 1 shuts down completely.
  - Server 2 restarts from the same SQLite database (`agsd.db`).
  - Client 2 connects to Server 2 and requests `SubscribeEvents { from_sequence: 2 }`.
- **Assertion**:
  - The subscription succeeds immediately without throwing `SequenceGapRecoverable`.
  - Client 2 receives replayed event 2 (`sequence == 2`).
  - When Client 3 performs a live mutation (event 3), Client 2 immediately receives live event 3 (`sequence == 3`).
- **Status**: **PASS (0.05s)**.

### Test 2: `test_durable_replay_sqlite_fallback_beyond_ring_buffer_capacity`
- **Scenario**:
  - SQLite database is seeded with 6 events (sequences 1 through 6).
  - Server starts with restricted `replay_capacity = 2`.
  - The in-memory `ReplayBuffer` holds only the tail events (sequences 5 and 6); sequences 1 through 4 are evicted from memory.
  - Client connects and requests `SubscribeEvents { from_sequence: 2 }`.
- **Assertion**:
  - Since sequence 2 is missing from memory, `ReplayBuffer::replay_from` returns `SequenceGapUnrecoverable`.
  - Transport layer falls back to `ControlPlane::events_slice(1, 5)`.
  - Client receives events `[2, 3, 4, 5, 6]` in exact chronological sequence.
  - Client watermark advances cleanly to 6.
- **Status**: **PASS (0.04s)**.

### Test 3: `test_caught_up_client_reconnect_after_restart`
- **Scenario**:
  - Database contains 3 events (sequences 1, 2, 3).
  - Server restarts.
  - Client reconnects with `from_sequence = 4` (`latest + 1`, fully caught up).
- **Assertion**:
  - Subscription succeeds with zero historical replay overhead.
  - Server emits live event 4.
  - Client receives live event 4 without gap errors or dropped frames.
- **Status**: **PASS (0.02s)**.

### Test 4: `test_race_free_sqlite_replay_and_concurrent_broadcast_handshake`
- **Scenario**:
  - Database contains 4 historical events (sequences 1 through 4).
  - Server starts.
  - Client requests `SubscribeEvents { from_sequence: 2 }`.
  - Concurrently, during the historical replay of events 2..4, Server emits live events 5 and 6.
- **Assertion**:
  - Client receives `[2, 3, 4, 5, 6]`.
  - Sequences are strictly monotonic ($S_{i+1} = S_i + 1$).
  - Zero duplicate frames for boundary events.
  - Zero dropped live frames.
- **Status**: **PASS (0.03s)**.

---

## 3. Test Suite Output Record

```
Running tests\m15_05_durable_replay_tests.rs
test test_caught_up_client_reconnect_after_restart ... ok
test test_race_free_sqlite_replay_and_concurrent_broadcast_handshake ... ok
test test_durable_replay_sqlite_fallback_beyond_ring_buffer_capacity ... ok
test test_durable_replay_after_daemon_restart_from_persisted_sqlite ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; finished in 0.14s
```
