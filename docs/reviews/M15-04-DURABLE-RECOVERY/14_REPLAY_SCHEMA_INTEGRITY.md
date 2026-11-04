# M15-04.1 Replay Integrity & Schema Validation Architecture

## 1. Checked Conversions & Arithmetic Safety

In M15-04, raw integer queries from SQLite were cast using unchecked `as u16` and `as u64`:
```rust
// Legacy unchecked casts
schema_version: row.schema_version as u16,
sequence: row.sequence as u64,
```

### Risks:
- Negative SQL values (e.g. from malicious database tampering or corrupted pages) wrap into large positive unsigned integers.
- Schema version 65537 wraps to 1 (`65537 as u16 == 1`), silently bypassing schema version gates.
- Large sequences wrap silently, violating monotonic sequence order without raising errors.

### M15-04.1 Checked Conversions:
All row-to-envelope translations now strictly enforce checked bounds:
```rust
let schema_version = u16::try_from(self.schema_version).map_err(|e| {
    StoreError::StorageFailure(format!(
        "Invalid schema_version {} for event {}: {e}",
        self.schema_version, self.event_id
    ))
})?;

let sequence = u64::try_from(self.sequence).map_err(|e| {
    StoreError::StorageFailure(format!(
        "Invalid sequence {} for event {}: {e}",
        self.sequence, self.event_id
    ))
})?;
```
Any negative integer or out-of-bounds value immediately halts replay and fails closed without panicking.

---

## 2. Transactional Schema Migrations

- The `schema_migrations` table and all migration steps execute inside immediate transactions (`TransactionBehavior::Immediate`).
- If any migration step fails:
  1. The transaction automatically rolls back.
  2. The original database and historical events remain unmodified.
  3. The daemon fails fast with `StoreError::StorageFailure`.
  4. The daemon **never** silently falls back to `InMemoryStore`.

---

## 3. High-Performance Event Counting

Routine recovery reports and health queries no longer load all historical events into memory to determine event count. `SqliteEventStore::event_count()` executes `SELECT COUNT(*) FROM event_store;` in O(1) time using SQLite's B-Tree header counters, preventing unbounded memory allocations on large event databases.
