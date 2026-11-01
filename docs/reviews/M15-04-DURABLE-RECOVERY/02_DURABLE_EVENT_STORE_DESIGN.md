# M15-04 Architecture & Design: Durable SQLite Event Store

## 1. Overview & Objectives

In Milestone M15-04, Agent Studios transitions from an ephemeral in-memory event log to an industrial, production-grade SQLite durable storage engine (`SqliteEventStore`). The design guarantees:
1. **Zero Data Loss on Crash or Restart:** State mutations for Studios, Tasks, Agents, Runs, Approvals, Artifacts, and Worktrees are fully serialized and committed to disk before IPC responses acknowledge success.
2. **ACID Transactions via WAL Mode:** Write-Ahead Logging (`PRAGMA journal_mode = WAL;`) provides non-blocking concurrent readers with serialized single-writer transactions (`TransactionBehavior::Immediate`).
3. **Monotonic Sequence Enforcement per Studio:** Every event envelope must have `sequence = latest + 1`. Any gap or regression rolls back the entire batch atomically.
4. **Pre-Flight Corruption Check:** On initialization, `PRAGMA quick_check;` detects database physical corruption and fails fast rather than serving corrupted data.
5. **Non-Duplicating Replay:** `ControlPlane::restore_from_store` replays existing committed events directly into the in-memory graph without re-appending them to the database.

---

## 2. SQLite Schema & Constraints

The durable database maintains a versioned schema managed via `schema_migrations`:

```sql
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at DATETIME NOT NULL
);

CREATE TABLE IF NOT EXISTS events (
    event_id TEXT PRIMARY KEY,
    schema_version INTEGER NOT NULL,
    studio_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    timestamp TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_studio_sequence UNIQUE (studio_id, sequence)
);

CREATE INDEX IF NOT EXISTS idx_events_studio_sequence 
    ON events (studio_id, sequence ASC);
```

### Safety Properties:
- `event_id TEXT PRIMARY KEY`: Rejects duplicates globally.
- `CONSTRAINT uq_studio_sequence UNIQUE (studio_id, sequence)`: Hardware/kernel level prevention of conflicting sequence numbers within the same studio.
- `payload TEXT NOT NULL`: Canonical JSON representation of `ControlPlaneEvent`.

---

## 3. PRAGMA Configuration & Durability

Upon opening the database connection in `SqliteEventStore::open`:
- `PRAGMA journal_mode = WAL;`: Enables write-ahead logging, preventing read/write contention.
- `PRAGMA synchronous = NORMAL;`: Ensures WAL frames are flushed before transaction commits are acknowledged, providing crash durability with optimal NVMe performance.
- `PRAGMA foreign_keys = ON;`: Enforces relational integrity.
- `PRAGMA busy_timeout = 5000;`: Waits up to 5 seconds for lock acquisition during transient lock contention.
- `PRAGMA quick_check;`: Evaluates B-tree structure, cell pointers, and freelist integrity before replay begins.

---

## 4. Single-Writer Atomicity & Sequence Invariants

`append_batch(&mut self, events: &[EventEnvelope]) -> Result<(), StoreError>` executes inside an explicit SQLite transaction with `TransactionBehavior::Immediate`:
1. **Empty Batch Handling:** Returns `Ok(())` immediately.
2. **In-Batch Duplicate Check:** A `HashSet<EventId>` catches duplicate IDs inside the same incoming batch.
3. **Pre-Existing Duplicate Check:** Queries `SELECT 1 FROM events WHERE event_id = ?` to prevent duplicate persistence attempts.
4. **Per-Studio Monotonic Sequence Validation:**
   - Tracks current `latest_sequence` per studio.
   - For each event in the batch:
     - If `sequence <= latest`: Returns `StoreError::SequenceRegression`.
     - If `sequence != latest + 1`: Returns `StoreError::SequenceGap`.
     - Advances tracked `latest` to `sequence`.
5. **Atomic Insertion:** Prepares `INSERT INTO events (...) VALUES (...)` and executes for all batch members.
6. **Commit / Rollback:** `tx.commit()` writes the transaction. If any step fails, the transaction is dropped and rolled back automatically by RAII, guaranteeing zero partial writes.

---

## 5. Non-Duplicating State Reconstruction

Prior in-memory engines called `commit_transaction` which wrote to the store during mutations. When replaying on startup:
```rust
pub fn restore_from_store(
    clock: C,
    store: S,
) -> Result<Self, ControlPlaneError> {
    let all_events = store.all_events()?;
    let mut state = ControlPlaneState::default();
    let mut studio_sequences: HashMap<StudioId, u64> = HashMap::new();

    for envelope in all_events {
        // Direct in-memory replay without store re-append
        state.apply_committed_event(&envelope)?;
        studio_sequences.insert(envelope.studio_id, envelope.sequence);
    }

    Ok(Self {
        clock,
        store,
        state,
        studio_sequences,
    })
}
```
This cleanly decouples state reconstruction from event publication, preventing `StoreError::DuplicateEventId` on every restart.
