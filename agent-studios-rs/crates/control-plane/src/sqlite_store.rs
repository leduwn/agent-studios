//! Production SQLite Durable Event Store Implementation (M15-04)
//!
//! Provides durable, transactional, and crash-safe storage for Agent Studios
//! Control Plane domain event streams.
//!
//! Guarantees:
//! 1. Atomic `append_batch`: Transactions commit ALL events or NONE.
//! 2. Per-studio monotonic sequence enforcement (detects and rejects gaps or regressions).
//! 3. Duplicate `EventId` detection and rejection across batch and database.
//! 4. Schema versioning and migration validation.
//! 5. SQLite WAL mode (`PRAGMA journal_mode = WAL`) and `PRAGMA synchronous = NORMAL`.
//! 6. Pre-flight corruption detection via `PRAGMA quick_check`.
//! 7. Single-writer concurrency safety via connection mutex and busy timeout.
//! 8. Bounded read and replay ordering (physical rowid and per-studio sequence order).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};

use agent_studios_protocol::event::{ControlPlaneEvent, EventEnvelope};
use agent_studios_protocol::id::{EventId, StudioId};

use crate::error::StoreError;
use crate::store::EventStore;

/// Current database schema version for Control Plane SQLite store.
pub const CURRENT_STORE_SCHEMA_VERSION: i64 = 1;

/// Durability configuration mode for SQLite event store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DurabilityMode {
    #[default]
    Full,
    Normal,
}

impl DurabilityMode {
    /// Returns the validated PRAGMA synchronous value string.
    pub const fn as_pragma_str(&self) -> &'static str {
        match self {
            Self::Full => "FULL",
            Self::Normal => "NORMAL",
        }
    }
}

/// Durable SQLite-backed event store.
pub struct SqliteEventStore {
    conn: Mutex<Connection>,
    db_path: Option<PathBuf>,
    durability: DurabilityMode,
}

impl SqliteEventStore {
    /// Opens or creates a durable SQLite event store at the given filesystem path with default durability (Full).
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::open_with_durability(path, DurabilityMode::Full)
    }

    /// Opens or creates a durable SQLite event store at the given filesystem path with specified durability mode.
    pub fn open_with_durability(
        path: impl AsRef<Path>,
        durability: DurabilityMode,
    ) -> Result<Self, StoreError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                StoreError::StorageFailure(format!(
                    "Failed to create database parent directory: {e}"
                ))
            })?;
        }

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;

        let conn = Connection::open_with_flags(path, flags).map_err(|e| {
            StoreError::StorageFailure(format!(
                "Failed to open SQLite database at {}: {e}",
                path.display()
            ))
        })?;

        let store = Self {
            conn: Mutex::new(conn),
            db_path: Some(path.to_path_buf()),
            durability,
        };

        store.initialize()?;
        Ok(store)
    }

    /// Creates an in-memory SQLite event store (primarily for unit tests and benchmarks).
    pub fn in_memory() -> Result<Self, StoreError> {
        Self::in_memory_with_durability(DurabilityMode::Normal)
    }

    /// Creates an in-memory SQLite event store with specified durability mode.
    pub fn in_memory_with_durability(durability: DurabilityMode) -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory().map_err(|e| {
            StoreError::StorageFailure(format!("Failed to open in-memory SQLite: {e}"))
        })?;

        let store = Self {
            conn: Mutex::new(conn),
            db_path: None,
            durability,
        };

        store.initialize()?;
        Ok(store)
    }

    /// Returns the durability mode configured for this store.
    pub fn durability(&self) -> DurabilityMode {
        self.durability
    }

    /// Returns the database file path if disk-backed.
    pub fn db_path(&self) -> Option<&Path> {
        self.db_path.as_deref()
    }

    /// Configures pragmas, executes integrity checks, and applies schema migrations.
    fn initialize(&self) -> Result<(), StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::StorageFailure("Poisoned connection mutex".to_string()))?;

        // 1. Configure production PRAGMAs
        if self.db_path.is_some() {
            let journal_mode: String = conn
                .query_row("PRAGMA journal_mode = WAL;", [], |row| row.get(0))
                .map_err(|e| {
                    StoreError::StorageFailure(format!("Failed to set WAL journal mode: {e}"))
                })?;
            if !journal_mode.eq_ignore_ascii_case("wal") {
                eprintln!("[WARN] SQLite journal mode is {journal_mode}, not WAL");
            }
        }

        let sync_mode = self.durability.as_pragma_str();
        conn.execute_batch(&format!(
            "PRAGMA synchronous = {sync_mode};
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;"
        ))
        .map_err(|e| StoreError::StorageFailure(format!("Failed to apply SQLite PRAGMAs: {e}")))?;

        // 2. Perform quick corruption check
        let check_res: String = conn
            .query_row("PRAGMA quick_check;", [], |row| row.get(0))
            .map_err(|e| {
                StoreError::StorageFailure(format!("Failed to execute SQLite quick_check: {e}"))
            })?;

        if check_res != "ok" {
            return Err(StoreError::StorageFailure(format!(
                "SQLite database corruption detected during startup: {check_res}"
            )));
        }

        // 3. Schema migrations table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL,
                description TEXT NOT NULL
            );",
            [],
        )
        .map_err(|e| {
            StoreError::StorageFailure(format!("Failed to initialize schema_migrations table: {e}"))
        })?;

        // 4. Verify existing migration versions
        let current_version: Option<i64> = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|e| {
                StoreError::StorageFailure(format!("Failed to query schema migrations: {e}"))
            })?
            .flatten();

        if let Some(version) = current_version
            && version > CURRENT_STORE_SCHEMA_VERSION
        {
            return Err(StoreError::StorageFailure(format!(
                "Unsupported database schema version {version} (max supported is {CURRENT_STORE_SCHEMA_VERSION}). Migration regression prevented."
            )));
        }

        // 5. Apply Schema Version 1 if not already applied
        if current_version.unwrap_or(0) < 1 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS event_store (
                    rowid INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_id TEXT NOT NULL UNIQUE,
                    schema_version INTEGER NOT NULL,
                    studio_id TEXT NOT NULL,
                    sequence INTEGER NOT NULL,
                    timestamp TEXT NOT NULL,
                    event_kind TEXT NOT NULL,
                    event_payload TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    UNIQUE(studio_id, sequence)
                );
                CREATE INDEX IF NOT EXISTS idx_event_store_studio_seq ON event_store(studio_id, sequence);
                INSERT INTO schema_migrations (version, applied_at, description)
                VALUES (1, datetime('now'), 'Initial event_store schema with per-studio monotonic sequences');"
            )
            .map_err(|e| StoreError::StorageFailure(format!("Failed to apply schema version 1 migration: {e}")))?;
        }

        Ok(())
    }

    /// Returns the total count of events persisted in the database.
    pub fn event_count(&self) -> Result<usize, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::StorageFailure("Poisoned connection mutex".to_string()))?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM event_store", [], |row| row.get(0))
            .map_err(|e| StoreError::StorageFailure(e.to_string()))?;
        usize::try_from(count)
            .map_err(|e| StoreError::StorageFailure(format!("Invalid event count {count}: {e}")))
    }

    /// Returns the most recent `limit` events persisted in the database in ascending chronological order.
    pub fn events_tail(&self, limit: usize) -> Result<Vec<EventEnvelope>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::StorageFailure("Poisoned connection mutex".to_string()))?;

        let limit_i64 = i64::try_from(limit)
            .map_err(|e| StoreError::StorageFailure(format!("limit overflow {limit}: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT event_id, schema_version, studio_id, sequence, timestamp, event_payload
                 FROM (
                     SELECT rowid, event_id, schema_version, studio_id, sequence, timestamp, event_payload
                     FROM event_store
                     ORDER BY rowid DESC
                     LIMIT ?1
                 ) sub
                 ORDER BY rowid ASC",
            )
            .map_err(|e| StoreError::StorageFailure(e.to_string()))?;

        let rows = stmt
            .query_map(params![limit_i64], |row| {
                Ok(StoredEventRow {
                    event_id: row.get(0)?,
                    schema_version: row.get(1)?,
                    studio_id: row.get(2)?,
                    sequence: row.get(3)?,
                    timestamp: row.get(4)?,
                    payload: row.get(5)?,
                })
            })
            .map_err(|e| StoreError::StorageFailure(e.to_string()))?;

        let mut envelopes = Vec::new();
        for r in rows {
            let stored = r.map_err(|e| StoreError::StorageFailure(e.to_string()))?;
            envelopes.push(stored.into_envelope()?);
        }

        Ok(envelopes)
    }

    /// Returns a slice of events starting from row offset `offset` up to `limit` in ascending chronological order.
    pub fn events_slice(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<EventEnvelope>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::StorageFailure("Poisoned connection mutex".to_string()))?;

        let limit_i64 = i64::try_from(limit)
            .map_err(|e| StoreError::StorageFailure(format!("limit overflow {limit}: {e}")))?;
        let offset_i64 = i64::try_from(offset)
            .map_err(|e| StoreError::StorageFailure(format!("offset overflow {offset}: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT event_id, schema_version, studio_id, sequence, timestamp, event_payload
                 FROM event_store
                 ORDER BY rowid ASC
                 LIMIT ?1 OFFSET ?2",
            )
            .map_err(|e| StoreError::StorageFailure(e.to_string()))?;

        let rows = stmt
            .query_map(params![limit_i64, offset_i64], |row| {
                Ok(StoredEventRow {
                    event_id: row.get(0)?,
                    schema_version: row.get(1)?,
                    studio_id: row.get(2)?,
                    sequence: row.get(3)?,
                    timestamp: row.get(4)?,
                    payload: row.get(5)?,
                })
            })
            .map_err(|e| StoreError::StorageFailure(e.to_string()))?;

        let mut envelopes = Vec::new();
        for r in rows {
            let stored = r.map_err(|e| StoreError::StorageFailure(e.to_string()))?;
            envelopes.push(stored.into_envelope()?);
        }

        Ok(envelopes)
    }
}

impl EventStore for SqliteEventStore {
    fn append_batch(&mut self, events: &[EventEnvelope]) -> Result<(), StoreError> {
        if events.is_empty() {
            return Ok(());
        }

        let mut conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::StorageFailure("Poisoned connection mutex".to_string()))?;

        // Begin immediate transaction: acquires write lock up front to prevent deadlocks
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| StoreError::StorageFailure(format!("Failed to begin transaction: {e}")))?;

        let mut current_sequences: HashMap<StudioId, u64> = HashMap::new();
        let mut batch_seen_ids = HashSet::new();

        for envelope in events {
            // 1. In-batch duplicate check
            if !batch_seen_ids.insert(envelope.event_id) {
                return Err(StoreError::DuplicateEventId {
                    event_id: envelope.event_id,
                });
            }

            // 2. Database duplicate EventId check
            let exists: bool = tx
                .query_row(
                    "SELECT 1 FROM event_store WHERE event_id = ?1",
                    params![envelope.event_id.to_string()],
                    |_| Ok(true),
                )
                .optional()
                .map_err(|e| StoreError::StorageFailure(e.to_string()))?
                .unwrap_or(false);

            if exists {
                return Err(StoreError::DuplicateEventId {
                    event_id: envelope.event_id,
                });
            }

            // 3. Monotonic sequence check per Studio
            let studio_id = envelope.studio_id;
            let expected = match current_sequences.get(&studio_id) {
                Some(&seq) => seq,
                None => {
                    let latest: i64 = tx
                        .query_row(
                            "SELECT COALESCE(MAX(sequence), 0) FROM event_store WHERE studio_id = ?1",
                            params![studio_id.to_string()],
                            |row| row.get(0),
                        )
                        .map_err(|e| StoreError::StorageFailure(e.to_string()))?;
                    let latest_u64 = u64::try_from(latest).map_err(|e| {
                        StoreError::StorageFailure(format!(
                            "Invalid sequence in database {latest}: {e}"
                        ))
                    })?;
                    latest_u64.checked_add(1).ok_or_else(|| {
                        StoreError::StorageFailure(format!(
                            "Sequence overflow beyond u64::MAX for studio {studio_id}"
                        ))
                    })?
                }
            };

            if envelope.sequence < expected {
                return Err(StoreError::SequenceRegression {
                    studio_id,
                    latest: expected - 1,
                    attempted: envelope.sequence,
                });
            }

            if envelope.sequence > expected {
                return Err(StoreError::SequenceGap {
                    studio_id,
                    expected,
                    actual: envelope.sequence,
                });
            }

            // 4. Serialize event payload
            let payload_json = serde_json::to_string(&envelope.event).map_err(|e| {
                StoreError::StorageFailure(format!("Event serialization failed: {e}"))
            })?;

            let event_kind = match &envelope.event {
                ControlPlaneEvent::StudioCreated { .. } => "studio_created",
                ControlPlaneEvent::AgentRegistered { .. } => "agent_registered",
                ControlPlaneEvent::AgentStateChanged { .. } => "agent_state_changed",
                ControlPlaneEvent::TaskCreated { .. } => "task_created",
                ControlPlaneEvent::TaskStateChanged { .. } => "task_state_changed",
                ControlPlaneEvent::TaskDependencyAdded { .. } => "task_dependency_added",
                ControlPlaneEvent::TaskDependencyRemoved { .. } => "task_dependency_removed",
                ControlPlaneEvent::RunCreated { .. } => "run_created",
                ControlPlaneEvent::RunStateChanged { .. } => "run_state_changed",
                ControlPlaneEvent::ApprovalRequested { .. } => "approval_requested",
                ControlPlaneEvent::ApprovalResolved { .. } => "approval_resolved",
                ControlPlaneEvent::ArtifactRegistered { .. } => "artifact_registered",
                ControlPlaneEvent::CancellationRequested { .. } => "cancellation_requested",
                ControlPlaneEvent::AgentRuntimeBound { .. } => "agent_runtime_bound",
                ControlPlaneEvent::AgentSpawned { .. } => "agent_spawned",
                ControlPlaneEvent::ToolStarted { .. } => "tool_started",
                ControlPlaneEvent::ToolCompleted { .. } => "tool_completed",
                ControlPlaneEvent::ToolFailed { .. } => "tool_failed",
                ControlPlaneEvent::BudgetUsageUpdated { .. } => "budget_usage_updated",
                ControlPlaneEvent::BudgetExceeded { .. } => "budget_exceeded",
                ControlPlaneEvent::TaskRetryScheduled { .. } => "task_retry_scheduled",
                ControlPlaneEvent::RunOutcomeRecorded { .. } => "run_outcome_recorded",
                ControlPlaneEvent::WorktreeCreated { .. } => "worktree_created",
                ControlPlaneEvent::WorktreeAssigned { .. } => "worktree_assigned",
                ControlPlaneEvent::WorktreeThreadBound { .. } => "worktree_thread_bound",
                ControlPlaneEvent::WorktreeStateChanged { .. } => "worktree_state_changed",
                ControlPlaneEvent::WorktreeChangeCaptured { .. } => "worktree_change_captured",
                ControlPlaneEvent::WorktreeReleased { .. } => "worktree_released",
                ControlPlaneEvent::WorktreeNoChangesCaptured { .. } => {
                    "worktree_no_changes_captured"
                }
                ControlPlaneEvent::ReconciliationCreated { .. } => "reconciliation_created",
                ControlPlaneEvent::ReconciliationStateChanged { .. } => {
                    "reconciliation_state_changed"
                }
                ControlPlaneEvent::ReconciliationConflictDetected { .. } => {
                    "reconciliation_conflict_detected"
                }
                ControlPlaneEvent::ReconciliationApplied { .. } => "reconciliation_applied",
                ControlPlaneEvent::ReconciliationFailed { .. } => "reconciliation_failed",
            };

            let schema_version = i64::from(envelope.schema_version);
            let sequence = i64::try_from(envelope.sequence).map_err(|e| {
                StoreError::StorageFailure(format!("Sequence overflow {}: {e}", envelope.sequence))
            })?;

            // 5. Insert event record
            tx.execute(
                "INSERT INTO event_store (
                    event_id, schema_version, studio_id, sequence, timestamp, event_kind, event_payload, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    envelope.event_id.to_string(),
                    schema_version,
                    studio_id.to_string(),
                    sequence,
                    envelope.timestamp.to_rfc3339(),
                    event_kind,
                    payload_json,
                    Utc::now().to_rfc3339(),
                ],
            )
            .map_err(|e| StoreError::StorageFailure(format!("Failed to insert event {}: {e}", envelope.event_id)))?;

            current_sequences.insert(studio_id, expected + 1);
        }

        // Commit transaction atomically
        tx.commit().map_err(|e| {
            StoreError::StorageFailure(format!("Failed to commit batch transaction: {e}"))
        })?;

        Ok(())
    }

    fn events_for_studio(
        &self,
        studio_id: StudioId,
        from_sequence: u64,
    ) -> Result<Vec<EventEnvelope>, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::StorageFailure("Poisoned connection mutex".to_string()))?;

        let from_seq_i64 = i64::try_from(from_sequence).map_err(|e| {
            StoreError::StorageFailure(format!("from_sequence overflow {from_sequence}: {e}"))
        })?;

        let mut stmt = conn
            .prepare(
                "SELECT event_id, schema_version, studio_id, sequence, timestamp, event_payload
                 FROM event_store
                 WHERE studio_id = ?1 AND sequence >= ?2
                 ORDER BY sequence ASC",
            )
            .map_err(|e| StoreError::StorageFailure(e.to_string()))?;

        let rows = stmt
            .query_map(params![studio_id.to_string(), from_seq_i64], |row| {
                Ok(StoredEventRow {
                    event_id: row.get(0)?,
                    schema_version: row.get(1)?,
                    studio_id: row.get(2)?,
                    sequence: row.get(3)?,
                    timestamp: row.get(4)?,
                    payload: row.get(5)?,
                })
            })
            .map_err(|e| StoreError::StorageFailure(e.to_string()))?;

        let mut envelopes = Vec::new();
        for r in rows {
            let stored = r.map_err(|e| StoreError::StorageFailure(e.to_string()))?;
            envelopes.push(stored.into_envelope()?);
        }

        Ok(envelopes)
    }

    fn latest_sequence(&self, studio_id: StudioId) -> Result<u64, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::StorageFailure("Poisoned connection mutex".to_string()))?;

        let seq: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence), 0) FROM event_store WHERE studio_id = ?1",
                params![studio_id.to_string()],
                |row| row.get(0),
            )
            .map_err(|e| StoreError::StorageFailure(e.to_string()))?;

        u64::try_from(seq).map_err(|e| {
            StoreError::StorageFailure(format!("Invalid sequence in database {seq}: {e}"))
        })
    }

    fn all_events(&self) -> Result<Vec<EventEnvelope>, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StoreError::StorageFailure("Poisoned connection mutex".to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT event_id, schema_version, studio_id, sequence, timestamp, event_payload
                 FROM event_store
                 ORDER BY rowid ASC",
            )
            .map_err(|e| StoreError::StorageFailure(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(StoredEventRow {
                    event_id: row.get(0)?,
                    schema_version: row.get(1)?,
                    studio_id: row.get(2)?,
                    sequence: row.get(3)?,
                    timestamp: row.get(4)?,
                    payload: row.get(5)?,
                })
            })
            .map_err(|e| StoreError::StorageFailure(e.to_string()))?;

        let mut envelopes = Vec::new();
        for r in rows {
            let stored = r.map_err(|e| StoreError::StorageFailure(e.to_string()))?;
            envelopes.push(stored.into_envelope()?);
        }

        Ok(envelopes)
    }

    fn event_count(&self) -> Result<usize, StoreError> {
        self.event_count()
    }

    fn events_tail(&self, limit: usize) -> Result<Vec<EventEnvelope>, StoreError> {
        self.events_tail(limit)
    }

    fn events_slice(&self, offset: usize, limit: usize) -> Result<Vec<EventEnvelope>, StoreError> {
        self.events_slice(offset, limit)
    }
}

/// Internal database row structure.
struct StoredEventRow {
    event_id: String,
    schema_version: i64,
    studio_id: String,
    sequence: i64,
    timestamp: String,
    payload: String,
}

impl StoredEventRow {
    fn into_envelope(self) -> Result<EventEnvelope, StoreError> {
        let event_id = EventId::from_str(&self.event_id)
            .map_err(|e| StoreError::StorageFailure(format!("Invalid EventId in database: {e}")))?;

        let studio_id = StudioId::from_str(&self.studio_id).map_err(|e| {
            StoreError::StorageFailure(format!("Invalid StudioId in database: {e}"))
        })?;

        let timestamp = DateTime::parse_from_rfc3339(&self.timestamp)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                StoreError::StorageFailure(format!("Invalid timestamp in database: {e}"))
            })?;

        let event: ControlPlaneEvent = serde_json::from_str(&self.payload).map_err(|e| {
            StoreError::StorageFailure(format!("Corrupt event JSON in database: {e}"))
        })?;

        let schema_version = u16::try_from(self.schema_version).map_err(|e| {
            StoreError::StorageFailure(format!(
                "Invalid schema_version {}: {e}",
                self.schema_version
            ))
        })?;
        let sequence = u64::try_from(self.sequence).map_err(|e| {
            StoreError::StorageFailure(format!("Invalid sequence {}: {e}", self.sequence))
        })?;

        Ok(EventEnvelope {
            event_id,
            schema_version,
            studio_id,
            sequence,
            timestamp,
            event,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_studios_protocol::studio::Studio;
    use tempfile::tempdir;

    #[test]
    fn test_sqlite_store_monotonic_and_atomicity() {
        let store = SqliteEventStore::in_memory().unwrap();
        let mut store = store;
        let studio_id = StudioId::new();
        let now = Utc::now();

        let event1 = ControlPlaneEvent::StudioCreated {
            studio: Studio::with_id(studio_id, "Studio 1", now),
        };
        let env1 = EventEnvelope::new(studio_id, 1, now, event1.clone());

        assert!(store.append(env1.clone()).is_ok());
        assert_eq!(store.latest_sequence(studio_id).unwrap(), 1);

        // Duplicate event ID must fail
        assert!(store.append(env1.clone()).is_err());

        // Sequence gap must fail
        let env3 = EventEnvelope::new(studio_id, 3, now, event1.clone());
        assert!(matches!(
            store.append(env3),
            Err(StoreError::SequenceGap {
                expected: 2,
                actual: 3,
                ..
            })
        ));

        // Sequence regression must fail
        let env0 = EventEnvelope::new(studio_id, 1, now, event1.clone());
        assert!(matches!(
            store.append(env0),
            Err(StoreError::SequenceRegression {
                latest: 1,
                attempted: 1,
                ..
            })
        ));

        // Valid sequence 2
        let env2 = EventEnvelope::new(studio_id, 2, now, event1.clone());
        assert!(store.append(env2).is_ok());
        assert_eq!(store.latest_sequence(studio_id).unwrap(), 2);
    }

    #[test]
    fn test_sqlite_store_batch_atomicity_rollback() {
        let mut store = SqliteEventStore::in_memory().unwrap();
        let studio_id = StudioId::new();
        let now = Utc::now();

        let event = ControlPlaneEvent::StudioCreated {
            studio: Studio::with_id(studio_id, "Studio 1", now),
        };
        let env1 = EventEnvelope::new(studio_id, 1, now, event.clone());
        // env2 has gap (seq 3 instead of 2)
        let env2 = EventEnvelope::new(studio_id, 3, now, event.clone());

        let batch = vec![env1, env2];
        let res = store.append_batch(&batch);
        assert!(res.is_err());

        // Atomicity guarantee: env1 must NOT have been saved
        assert_eq!(store.latest_sequence(studio_id).unwrap(), 0);
        assert_eq!(store.event_count().unwrap(), 0);
    }

    #[test]
    fn test_sqlite_store_disk_persistence_and_reopen() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("events.db");
        let studio_id = StudioId::new();
        let now = Utc::now();

        {
            let mut store = SqliteEventStore::open(&db_path).unwrap();
            let event = ControlPlaneEvent::StudioCreated {
                studio: Studio::with_id(studio_id, "Disk Studio", now),
            };
            let env = EventEnvelope::new(studio_id, 1, now, event);
            store.append(env).unwrap();
            assert_eq!(store.event_count().unwrap(), 1);
        }

        // Reopen database from disk and verify recovery
        {
            let store = SqliteEventStore::open(&db_path).unwrap();
            assert_eq!(store.event_count().unwrap(), 1);
            let events = store.all_events().unwrap();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].studio_id, studio_id);
            assert_eq!(events[0].sequence, 1);
            assert_eq!(store.latest_sequence(studio_id).unwrap(), 1);
        }
    }

    #[test]
    fn test_sqlite_store_durability_mode_configuration() {
        let dir = tempdir().unwrap();

        // Default open uses DurabilityMode::Full
        let db_full_path = dir.path().join("full.db");
        let store_full = SqliteEventStore::open(&db_full_path).unwrap();
        assert_eq!(store_full.durability(), DurabilityMode::Full);
        let sync_val_full: i64 = store_full
            .conn
            .lock()
            .unwrap()
            .query_row("PRAGMA synchronous;", [], |r| r.get(0))
            .unwrap();
        // SQLite synchronous FULL is 2
        assert_eq!(sync_val_full, 2);

        // Explicit Normal durability
        let db_normal_path = dir.path().join("normal.db");
        let store_normal =
            SqliteEventStore::open_with_durability(&db_normal_path, DurabilityMode::Normal)
                .unwrap();
        assert_eq!(store_normal.durability(), DurabilityMode::Normal);
        let sync_val_normal: i64 = store_normal
            .conn
            .lock()
            .unwrap()
            .query_row("PRAGMA synchronous;", [], |r| r.get(0))
            .unwrap();
        // SQLite synchronous NORMAL is 1
        assert_eq!(sync_val_normal, 1);
    }

    #[test]
    fn test_sqlite_store_events_tail_and_slice() {
        let mut store = SqliteEventStore::in_memory().unwrap();
        let studio_id = StudioId::new();
        let now = Utc::now();

        // Empty store queries
        assert!(store.events_tail(10).unwrap().is_empty());
        assert!(store.events_slice(0, 10).unwrap().is_empty());
        assert!(store.events_tail(0).unwrap().is_empty());
        assert!(store.events_slice(0, 0).unwrap().is_empty());

        // Insert 10 events
        for seq in 1..=10 {
            let event = ControlPlaneEvent::StudioCreated {
                studio: Studio::with_id(studio_id, format!("Studio {seq}"), now),
            };
            let env = EventEnvelope::new(studio_id, seq, now, event);
            store.append(env).unwrap();
        }

        assert_eq!(store.event_count().unwrap(), 10);

        // Limit 0 returns empty
        assert!(store.events_tail(0).unwrap().is_empty());
        assert!(store.events_slice(0, 0).unwrap().is_empty());

        // events_tail(3) returns sequences [8, 9, 10] in ascending order
        let tail_3 = store.events_tail(3).unwrap();
        assert_eq!(tail_3.len(), 3);
        assert_eq!(tail_3[0].sequence, 8);
        assert_eq!(tail_3[1].sequence, 9);
        assert_eq!(tail_3[2].sequence, 10);

        // events_tail(20) returns all 10 events
        let tail_all = store.events_tail(20).unwrap();
        assert_eq!(tail_all.len(), 10);
        assert_eq!(tail_all[0].sequence, 1);
        assert_eq!(tail_all[9].sequence, 10);

        // events_slice(0, 4) returns sequences [1, 2, 3, 4]
        let slice_first = store.events_slice(0, 4).unwrap();
        assert_eq!(slice_first.len(), 4);
        assert_eq!(slice_first[0].sequence, 1);
        assert_eq!(slice_first[3].sequence, 4);

        // events_slice(4, 4) returns sequences [5, 6, 7, 8]
        let slice_mid = store.events_slice(4, 4).unwrap();
        assert_eq!(slice_mid.len(), 4);
        assert_eq!(slice_mid[0].sequence, 5);
        assert_eq!(slice_mid[3].sequence, 8);

        // events_slice(8, 4) returns sequences [9, 10]
        let slice_tail = store.events_slice(8, 4).unwrap();
        assert_eq!(slice_tail.len(), 2);
        assert_eq!(slice_tail[0].sequence, 9);
        assert_eq!(slice_tail[1].sequence, 10);

        // events_slice(10, 4) returns empty vec
        let slice_out_of_bounds = store.events_slice(10, 4).unwrap();
        assert!(slice_out_of_bounds.is_empty());
    }
}
