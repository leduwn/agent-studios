# 01 — Baseline & Dependency Graph: M15-05 Backend

## 1. Executive Context & Scope

Milestone **M15-05 BACKEND** ("Durable Event Replay, Crash-Safe Reconnect & Real Agent Execution Readiness") resolves the fundamental architectural discrepancy between persistent domain state storage (`SqliteEventStore`) and real-time IPC event delivery (`ReplayBuffer` / `SubscribeEvents`).

Prior to M15-05, `SqliteEventStore` persisted all `EventEnvelope` domain records durably to `agsd.db` using SQLite WAL mode with synchronous full/normal durability. However, `ReplayBuffer` in `crates/ipc-protocol` was an ephemeral in-memory ring buffer initialized empty upon daemon restart. When a client attempted to reconnect after a daemon restart or crash requesting `SubscribeEvents { from_sequence: K + 1 }`, the server returned `ProtocolError::SequenceGapRecoverable { expected: 1, found: K + 1 }`, forcing clients into fatal unrecoverable reconnect loops despite all historical events existing safely on disk.

M15-05 eliminates this limitation by introducing a two-tiered, SQLite-backed durable event replay mechanism with a race-free handshake between historical backfill and live broadcast streaming, while auditing and hardening execution readiness across Tiers A, B, C, and D.

---

## 2. Baseline Configuration & Worktree State

- **Primary Repository**: `https://github.com/leduwn/agent-studios`
- **Assigned Worktree**: `D:\Projects\agent-studios-m15-04`
- **Git Branch**: `feat/m15-04-durable-state-recovery`
- **Active Pull Request**: PR #30 (Maintained in **Draft** status)
- **Baseline Git HEAD**: `dcbcb39bf61225a4660be5b533cf1ffd5de14b29`
- **Frontend Integration PR**: PR #27 (Integration baseline `854eab9a5ccc5f18c16ba94ac04e01fc502a8447`)
- **Compilation / Toolchain Flags**: `CARGO_INCREMENTAL=0`, `-j 2` concurrency bounding for memory stability on Windows.

---

## 3. Crate Dependency Graph & Modification Boundaries

```
                         agent-studios-protocol
                                  │
                                  ▼
                      agent-studios-control-plane
                     (Store, SqliteStore, Engine)
                                  │
                                  ▼
                     agent-studios-ipc-protocol
                     (ReplayBuffer, Framing, Wire)
                                  │
                                  ▼
                     agent-studios-ipc-transport
                 (IpcServer, Connection, Streaming)
                                  │
        ┌─────────────────────────┼─────────────────────────┐
        ▼                         ▼                         ▼
agent-studios-workspace   agent-studios-opencode    agent-studios-orchestration
(Worktree/Diff review)    (Subprocess adapter)      (Task execution graph)
```

### Module Boundary Responsibilities in M15-05:
1. **`agent-studios-control-plane`**:
   - `crates/control-plane/src/store.rs`: Expanded `EventStore` trait with `events_tail(limit)` and `events_slice(offset, limit)` methods.
   - `crates/control-plane/src/sqlite_store.rs`: Implemented indexed SQL queries `SELECT sequence, payload FROM events ORDER BY sequence DESC LIMIT ?` and `SELECT sequence, payload FROM events ORDER BY sequence ASC LIMIT ? OFFSET ?`.
   - `crates/control-plane/src/engine.rs`: Exposed bounded slice and tail queries on `ControlPlane` engine.
2. **`agent-studios-ipc-protocol`**:
   - `crates/ipc-protocol/src/replay.rs`:
     - Added `with_initial_sequence(capacity, initial_sequence)` for watermark continuity on restart.
     - Added `hydrate_tail(events)` with sequence monotonicity checking relative to `self.buffer.back()`.
     - Updated `replay_from` on empty buffers: returns `SequenceGapUnrecoverable` when `from_sequence < expected` to allow persistent store fallback.
3. **`agent-studios-ipc-transport`**:
   - `crates/ipc-transport/src/server.rs`: During `IpcServerBuilder::build`, queries `cp.events_tail(capacity)` to hydrate `replay_buffer` up to its watermark.
   - `crates/ipc-transport/src/connection.rs`: Subscribes to live `broadcast_rx` first, executes two-tiered replay (memory buffer -> SQLite slice fallback -> memory bridge), and filters live broadcast events with `event.sequence <= current_seq`.
   - `crates/ipc-transport/tests/m15_05_durable_replay_tests.rs`: Comprehensive integration test suite validating durable recovery across restarts, ring buffer eviction fallbacks, caught-up reconnects, and race-free concurrent broadcasts.

---

## 4. Worktree Isolation Invariants

- **Isolated Working Directory**: All backend modifications remain strictly within `D:\Projects\agent-studios-m15-04`.
- **Zero Interference**: The primary worktree `D:\Projects\agent-studios`, external branches (`feat/m12-claude-code-runtime`, `main`), and sibling tasks remain untouched.
- **Untouched Subtrees**: `codex-rs/` is not modified.
- **PR Status**: PR #30 remains in Draft state. No force-pushes, no rebasing, and no direct merge to `main`.
- **No Billable AI Models**: All test runs strictly gate and mock execution; 0 billable AI model calls in automated tests and CI.
