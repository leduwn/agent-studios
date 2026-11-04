# 02 — Volatile ReplayBuffer vs. Persistent SqliteEventStore Mismatch Audit

## 1. Architectural Discrepancy Overview

The Agent Studios architecture separates the authoritative domain state store from the transport-level event distribution layer:
- **`SqliteEventStore`** (`crates/control-plane/src/sqlite_store.rs`): A durable, append-only SQLite table (`events`) operating under WAL mode with `PRAGMA synchronous = FULL` or `NORMAL`. Every state mutation (studio creation, task registration, run execution, worktree binding) commits an `EventEnvelope` atomically before acknowledging the IPC command.
- **`ReplayBuffer`** (`crates/ipc-protocol/src/replay.rs`): An in-memory ring buffer (backed by `VecDeque<IpcEvent>`) designed for low-latency linearizable historical event queries (`SubscribeEvents { from_sequence }`) and broadcast gap recovery.

### The Problem Prior to M15-05:
Prior to M15-05, when `agsd` started, `ReplayBuffer::new(capacity)` created an empty ring buffer:
```rust
pub fn new(capacity: usize) -> Self {
    Self {
        capacity,
        buffer: VecDeque::with_capacity(capacity),
        last_sequence: None,
        contiguous_only: true,
    }
}
```
When `buffer` was empty (`last_sequence == None`), `replay_from(from_sequence, limit)` behaved as follows:
```rust
pub fn replay_from(&self, from_sequence: u64, limit: Option<usize>) -> Result<Vec<IpcEvent>, ProtocolError> {
    if self.buffer.is_empty() {
        return match self.last_sequence {
            None => {
                if from_sequence == 1 {
                    Ok(Vec::new())
                } else {
                    Err(ProtocolError::SequenceGapRecoverable {
                        expected: 1,
                        found: from_sequence,
                    })
                }
            }
            ...
        };
    }
    ...
}
```

---

## 2. Failure Sequence Diagram: Client Reconnection on Daemon Restart

```
  Client (Reconnecting)                      agsd Server (Restarted)                 SQLite Store (agsd.db)
         │                                            │                                        │
         │  (Crash / Restart of agsd occurs)          │                                        │
         │  Client held watermark = 2                 │                                        │
         │                                            │                                        │
         │───────────────────────────────────────────>│  Opens agsd.db                         │
         │  1. Connect & HMAC Auth                    │  Restores ControlPlane                 │
         │                                            │  (Events 1 and 2 exist in SQLite)      │
         │<───────────────────────────────────────────│                                        │
         │  AuthSuccess                               │                                        │
         │                                            │                                        │
         │───────────────────────────────────────────>│                                        │
         │  2. SubscribeEvents { from_sequence: 2 }   │                                        │
         │                                            │  Checks ReplayBuffer:                  │
         │                                            │  buffer.is_empty() == true             │
         │                                            │  last_sequence == None                 │
         │                                            │  from_sequence (2) != 1                │
         │                                            │                                        │
         │<───────────────────────────────────────────│                                        │
         │  3. SequenceGapRecoverable {               │  Rejects subscription with error!      │
         │       expected: 1,                         │  Ignores valid SQLite events on disk.   │
         │       found: 2                             │                                        │
         │     }                                      │                                        │
         │                                            │                                        │
         │  ❌ Fatal client sync failure              │                                        │
```

---

## 3. Detailed Root Causes

### Root Cause 1: Lack of Initial Sequence Watermark Hydration
`ReplayBuffer` was completely decoupled from `ControlPlane` initialization. Even though `ControlPlane::restore_from_store` replayed and counted all persisted events, it did not pass the highest committed sequence number to `IpcServerBuilder`, leaving `ReplayBuffer.last_sequence` as `None`.

### Root Cause 2: False SequenceGapRecoverable on Empty Buffers
When `from_sequence < expected` (e.g., buffer expected 5 because it was initialized with last_sequence = 4, but client requested 2), `ReplayBuffer` returned `SequenceGapRecoverable` rather than `SequenceGapUnrecoverable`. 
- `SequenceGapRecoverable` implies the client sent a future sequence that has not occurred yet.
- `SequenceGapUnrecoverable` implies the requested sequence is in the past and has been evicted from the buffer window.
Because it returned `SequenceGapRecoverable`, the connection layer assumed the client was requesting invalid future state rather than falling back to historical storage.

### Root Cause 3: Premature Monotonicity Rejection in Hydration
During initial implementation of tail hydration, `hydrate_tail` called `self.push(event)`. If `with_initial_sequence(capacity, 4)` was called, `last_sequence` was set to 4. Subsequent pushes of historical events (e.g., sequences 1, 2, 3, 4) were rejected because `event.sequence (1) <= last_sequence (4)`. Hydration monotonicity must be enforced against `self.buffer.back()`, not the terminal watermark `self.last_sequence`.

---

## 4. Architectural Solution Strategy

To resolve the mismatch permanently, M15-05 establishes a **Two-Tiered Hybrid Event Engine**:
1. **Tier 1 (Fast-Path In-Memory Ring Buffer)**: Retains the bounded tail (e.g., last 1,000 events) for microsecond-latency replays and fast-moving UI client updates.
2. **Tier 2 (Durable Persistent SQLite Fallback)**: Serves deep historical backfills for clients reconnecting after long offline periods or daemon restarts where events have rolled off memory.
3. **Bridge Logic**: When an event sequence spans both SQLite and in-memory buffers, the server seamlessly stitches the historical store slice with the memory buffer tail before promoting the connection to the live broadcast feed.
