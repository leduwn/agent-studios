# 03 — Durable Event Replay Architecture

## 1. High-Level Design & Component Hierarchy

The durable event replay architecture bridges the persistent, append-only SQLite domain store (`SqliteEventStore`) and the real-time in-memory event distributor (`ReplayBuffer` / `broadcast::Sender`).

```
                    ┌────────────────────────────────────────────────────────┐
                    │                   IpcServer / agsd                     │
                    │                                                        │
                    │   ┌─────────────────────┐    ┌─────────────────────┐   │
                    │   │   ControlPlane      │    │    ReplayBuffer     │   │
                    │   │  (SqliteEventStore) │    │  (In-Memory Tail)   │   │
                    │   └──────────┬──────────┘    └──────────┬──────────┘   │
                    └──────────────┼──────────────────────────┼──────────────┘
                                   │                          │
                                   ▼                          ▼
                          Tier 2: Deep Replay        Tier 1: Fast Tail
                        (Historical Store Slice)   (In-Memory Ring Buffer)
                                   │                          │
                                   └────────────┬─────────────┘
                                                │
                                                ▼
                                    ┌───────────────────────┐
                                    │ Connection Handler    │
                                    │ (SubscribeEvents Loop)│
                                    └───────────┬───────────┘
                                                │
                                                ▼
                                    Live Broadcast Stream
```

---

## 2. Store Layer Enhancements (`agent-studios-control-plane`)

In `crates/control-plane/src/store.rs`, the `EventStore` trait defines two new zero-copy query methods:
```rust
pub trait EventStore: Send + Sync {
    // ...
    /// Returns the most recent `limit` events across all studios in ascending chronological order.
    fn events_tail(&self, limit: usize) -> Result<Vec<EventEnvelope>, StoreError>;

    /// Returns a slice of events starting at chronological index `offset` (0-indexed) up to `limit`.
    fn events_slice(&self, offset: usize, limit: usize) -> Result<Vec<EventEnvelope>, StoreError>;
}
```

### SQLite Query Implementation (`crates/control-plane/src/sqlite_store.rs`):
- **Tail Query (`events_tail`)**:
  Retrieves the most recent `limit` rows sorted by sequence descending, then reverses them into ascending chronological order for monotonic replay:
  ```sql
  SELECT sequence, payload FROM events ORDER BY sequence DESC LIMIT ?;
  ```
- **Slice Query (`events_slice`)**:
  Performs index-backed sequential range retrieval using `LIMIT` and `OFFSET`:
  ```sql
  SELECT sequence, payload FROM events ORDER BY sequence ASC LIMIT ? OFFSET ?;
  ```
- **Durability Modes**:
  Controlled via `DurabilityMode`:
  - `ProcessCrashOnly`: `PRAGMA synchronous = NORMAL` (WAL recovery on process termination).
  - `FullPowerLossSafe`: `PRAGMA synchronous = FULL` (acknowledged commits fsynced to disk).

---

## 3. Protocol Layer Watermark & Hydration (`agent-studios-ipc-protocol`)

In `crates/ipc-protocol/src/replay.rs`:
- **`with_initial_sequence(capacity, initial_sequence)`**:
  Constructs a buffer initialized to a non-zero watermark, setting `last_sequence = Some(initial_sequence)`.
- **`hydrate_tail(events)`**:
  Populates the bounded ring buffer from persistent store events upon daemon startup. Monotonicity validation is performed strictly against the predecessor in the buffer (`self.buffer.back()`), ensuring historical events are accepted while rejecting duplicate or out-of-order records:
  ```rust
  pub fn hydrate_tail(&mut self, events: impl IntoIterator<Item = IpcEvent>) -> Result<(), ProtocolError> {
      for event in events {
          match self.buffer.back() {
              Some(back) if self.contiguous_only && event.sequence != back.sequence + 1 => {
                  return Err(ProtocolError::SequenceGapRecoverable {
                      expected: back.sequence + 1,
                      found: event.sequence,
                  });
              }
              Some(back) if !self.contiguous_only && event.sequence <= back.sequence => {
                  return Err(ProtocolError::InvalidState(format!(
                      "Non-monotonic sequence number: incoming {} <= last in buffer {}",
                      event.sequence, back.sequence
                  )));
              }
              _ => {}
          }

          if self.last_sequence.is_none_or(|l| event.sequence > l) {
              self.last_sequence = Some(event.sequence);
          }

          if self.buffer.len() >= self.capacity {
              self.buffer.pop_front();
          }

          self.buffer.push_back(event);
      }
      Ok(())
  }
  ```
- **`replay_from` Fallback Contract**:
  When `from_sequence < expected`, `replay_from` returns `ProtocolError::SequenceGapUnrecoverable { earliest_available, requested: from_sequence }`. This signal instructs the transport layer to fall back to the durable SQLite store.

---

## 4. Server Lifecycle & Bootstrapping (`agent-studios-ipc-transport`)

In `crates/ipc-transport/src/server.rs`:
When `IpcServerBuilder::build` initializes the server:
1. It queries the persisted event count from `control_plane`.
2. If `persisted_count > 0`:
   - It retrieves `cp.events_tail(replay_capacity)`.
   - Converts `EventEnvelope` items to `IpcEvent` instances.
   - Initialises `ReplayBuffer::with_initial_sequence(replay_capacity, persisted_count)`.
   - Hydrates the buffer with the tail events via `replay_buffer.hydrate_tail(tail_events)`.
3. If `persisted_count == 0`:
   - Initialises standard empty `ReplayBuffer::new(replay_capacity)`.

Result: `ReplayBuffer` is immediately warmed and ready to serve reconnecting clients from memory for recent events, with full awareness of the committed watermark.
