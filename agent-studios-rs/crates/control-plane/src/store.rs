use std::collections::{HashMap, HashSet};

pub use agent_studios_protocol::event::EventEnvelope;
use agent_studios_protocol::id::{EventId, StudioId};

pub use crate::error::StoreError;

/// Storage abstraction for persisting and retrieving domain event streams.
pub trait EventStore: Send + Sync {
    /// Appends a batch of event envelopes atomically.
    /// ALL events must be persisted or ZERO if any validation fails.
    fn append_batch(&mut self, events: &[EventEnvelope]) -> Result<(), StoreError>;

    /// Appends a single event envelope to the store.
    /// Delegates to `append_batch`.
    fn append(&mut self, envelope: EventEnvelope) -> Result<(), StoreError> {
        self.append_batch(&[envelope])
    }

    /// Retrieves events for a specific studio starting from `from_sequence` (inclusive).
    fn events_for_studio(
        &self,
        studio_id: StudioId,
        from_sequence: u64,
    ) -> Result<Vec<EventEnvelope>, StoreError>;

    /// Returns the highest assigned sequence for the studio, or 0 if no events exist.
    fn latest_sequence(&self, studio_id: StudioId) -> Result<u64, StoreError>;

    /// Returns all recorded events across all studios in chronological order.
    fn all_events(&self) -> Result<Vec<EventEnvelope>, StoreError>;

    /// Returns the total count of events across all studios without loading all events into memory.
    fn event_count(&self) -> Result<usize, StoreError>;

    /// Returns the most recent `limit` events across all studios in ascending chronological order.
    fn events_tail(&self, limit: usize) -> Result<Vec<EventEnvelope>, StoreError> {
        let all = self.all_events()?;
        if all.len() <= limit {
            Ok(all)
        } else {
            let start = all.len() - limit;
            Ok(all[start..].to_vec())
        }
    }

    /// Returns a slice of events starting from chronological index `offset` (0-indexed) up to `limit`.
    fn events_slice(&self, offset: usize, limit: usize) -> Result<Vec<EventEnvelope>, StoreError> {
        let all = self.all_events()?;
        if offset >= all.len() {
            return Ok(Vec::new());
        }
        let end = (offset + limit).min(all.len());
        Ok(all[offset..end].to_vec())
    }
}

/// Thread-safe in-memory event store for development, testing, and replays.
#[derive(Clone, Debug, Default)]
pub struct InMemoryStore {
    events_by_studio: HashMap<StudioId, Vec<EventEnvelope>>,
    all_events: Vec<EventEnvelope>,
    seen_event_ids: HashSet<EventId>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl EventStore for InMemoryStore {
    fn append_batch(&mut self, events: &[EventEnvelope]) -> Result<(), StoreError> {
        if events.is_empty() {
            return Ok(());
        }

        // Phase 1: Strict validation of entire batch before modifying any state.
        let mut batch_seen_ids = HashSet::new();
        let mut expected_sequences: HashMap<StudioId, u64> = HashMap::new();

        for envelope in events {
            // Check for duplicate EventId in store or batch
            if self.seen_event_ids.contains(&envelope.event_id)
                || !batch_seen_ids.insert(envelope.event_id)
            {
                return Err(StoreError::DuplicateEventId {
                    event_id: envelope.event_id,
                });
            }

            let studio_id = envelope.studio_id;
            let expected = match expected_sequences.get(&studio_id) {
                Some(&seq) => seq,
                None => self.latest_sequence(studio_id)? + 1,
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

            expected_sequences.insert(studio_id, expected + 1);
        }

        // Phase 2: All validation passed - commit all events atomically.
        for envelope in events {
            self.seen_event_ids.insert(envelope.event_id);
            self.all_events.push(envelope.clone());
            self.events_by_studio
                .entry(envelope.studio_id)
                .or_default()
                .push(envelope.clone());
        }

        Ok(())
    }

    fn events_for_studio(
        &self,
        studio_id: StudioId,
        from_sequence: u64,
    ) -> Result<Vec<EventEnvelope>, StoreError> {
        let events = self
            .events_by_studio
            .get(&studio_id)
            .map(|list| {
                list.iter()
                    .filter(|e| e.sequence >= from_sequence)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        Ok(events)
    }

    fn latest_sequence(&self, studio_id: StudioId) -> Result<u64, StoreError> {
        let latest = self
            .events_by_studio
            .get(&studio_id)
            .and_then(|list| list.last())
            .map(|e| e.sequence)
            .unwrap_or(0);
        Ok(latest)
    }

    fn all_events(&self) -> Result<Vec<EventEnvelope>, StoreError> {
        Ok(self.all_events.clone())
    }

    fn event_count(&self) -> Result<usize, StoreError> {
        Ok(self.all_events.len())
    }

    fn events_tail(&self, limit: usize) -> Result<Vec<EventEnvelope>, StoreError> {
        let count = self.all_events.len();
        if count <= limit {
            Ok(self.all_events.clone())
        } else {
            let start = count - limit;
            Ok(self.all_events[start..].to_vec())
        }
    }

    fn events_slice(&self, offset: usize, limit: usize) -> Result<Vec<EventEnvelope>, StoreError> {
        let count = self.all_events.len();
        if offset >= count {
            return Ok(Vec::new());
        }
        let end = (offset + limit).min(count);
        Ok(self.all_events[offset..end].to_vec())
    }
}

/// Type alias for backward compatibility and explicit naming.
pub type InMemoryEventStore = InMemoryStore;

impl EventStore for Box<dyn EventStore> {
    fn append_batch(&mut self, events: &[EventEnvelope]) -> Result<(), StoreError> {
        (**self).append_batch(events)
    }

    fn events_for_studio(
        &self,
        studio_id: StudioId,
        from_sequence: u64,
    ) -> Result<Vec<EventEnvelope>, StoreError> {
        (**self).events_for_studio(studio_id, from_sequence)
    }

    fn latest_sequence(&self, studio_id: StudioId) -> Result<u64, StoreError> {
        (**self).latest_sequence(studio_id)
    }

    fn all_events(&self) -> Result<Vec<EventEnvelope>, StoreError> {
        (**self).all_events()
    }

    fn event_count(&self) -> Result<usize, StoreError> {
        (**self).event_count()
    }

    fn events_tail(&self, limit: usize) -> Result<Vec<EventEnvelope>, StoreError> {
        (**self).events_tail(limit)
    }

    fn events_slice(&self, offset: usize, limit: usize) -> Result<Vec<EventEnvelope>, StoreError> {
        (**self).events_slice(offset, limit)
    }
}

/// Unified event store backend container supporting in-memory, SQLite, or custom backends.
pub enum AnyEventStore {
    InMemory(InMemoryStore),
    Sqlite(crate::sqlite_store::SqliteEventStore),
    Custom(Box<dyn EventStore>),
}

impl Default for AnyEventStore {
    fn default() -> Self {
        Self::InMemory(InMemoryStore::new())
    }
}

impl EventStore for AnyEventStore {
    fn append_batch(&mut self, events: &[EventEnvelope]) -> Result<(), StoreError> {
        match self {
            Self::InMemory(s) => s.append_batch(events),
            Self::Sqlite(s) => s.append_batch(events),
            Self::Custom(s) => s.append_batch(events),
        }
    }

    fn events_for_studio(
        &self,
        studio_id: StudioId,
        from_sequence: u64,
    ) -> Result<Vec<EventEnvelope>, StoreError> {
        match self {
            Self::InMemory(s) => s.events_for_studio(studio_id, from_sequence),
            Self::Sqlite(s) => s.events_for_studio(studio_id, from_sequence),
            Self::Custom(s) => s.events_for_studio(studio_id, from_sequence),
        }
    }

    fn latest_sequence(&self, studio_id: StudioId) -> Result<u64, StoreError> {
        match self {
            Self::InMemory(s) => s.latest_sequence(studio_id),
            Self::Sqlite(s) => s.latest_sequence(studio_id),
            Self::Custom(s) => s.latest_sequence(studio_id),
        }
    }

    fn all_events(&self) -> Result<Vec<EventEnvelope>, StoreError> {
        match self {
            Self::InMemory(s) => s.all_events(),
            Self::Sqlite(s) => s.all_events(),
            Self::Custom(s) => s.all_events(),
        }
    }

    fn event_count(&self) -> Result<usize, StoreError> {
        match self {
            Self::InMemory(s) => s.event_count(),
            Self::Sqlite(s) => s.event_count(),
            Self::Custom(s) => s.event_count(),
        }
    }

    fn events_tail(&self, limit: usize) -> Result<Vec<EventEnvelope>, StoreError> {
        match self {
            Self::InMemory(s) => s.events_tail(limit),
            Self::Sqlite(s) => s.events_tail(limit),
            Self::Custom(s) => s.events_tail(limit),
        }
    }

    fn events_slice(&self, offset: usize, limit: usize) -> Result<Vec<EventEnvelope>, StoreError> {
        match self {
            Self::InMemory(s) => s.events_slice(offset, limit),
            Self::Sqlite(s) => s.events_slice(offset, limit),
            Self::Custom(s) => s.events_slice(offset, limit),
        }
    }
}

impl From<InMemoryStore> for AnyEventStore {
    fn from(s: InMemoryStore) -> Self {
        Self::InMemory(s)
    }
}

impl From<crate::sqlite_store::SqliteEventStore> for AnyEventStore {
    fn from(s: crate::sqlite_store::SqliteEventStore) -> Self {
        Self::Sqlite(s)
    }
}

impl From<Box<dyn EventStore>> for AnyEventStore {
    fn from(s: Box<dyn EventStore>) -> Self {
        Self::Custom(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_studios_protocol::event::ControlPlaneEvent;
    use agent_studios_protocol::studio::Studio;
    use chrono::Utc;

    #[test]
    fn test_monotonic_sequence_enforcement() {
        let mut store = InMemoryStore::new();
        let studio_id = StudioId::new();
        let now = Utc::now();

        let event = ControlPlaneEvent::StudioCreated {
            studio: Studio::with_id(studio_id, "Test Studio", now),
        };

        // First event must be sequence 1
        let env1 = EventEnvelope::new(studio_id, 1, now, event.clone());
        assert!(store.append(env1).is_ok());
        assert_eq!(store.latest_sequence(studio_id).unwrap(), 1);

        // Gap rejected (e.g. sequence 3 instead of 2)
        let env_gap = EventEnvelope::new(studio_id, 3, now, event.clone());
        let err_gap = store.append(env_gap).unwrap_err();
        assert_eq!(
            err_gap,
            StoreError::SequenceGap {
                studio_id,
                expected: 2,
                actual: 3
            }
        );

        // Regression rejected (e.g. sequence 1 again)
        let env_reg = EventEnvelope::new(studio_id, 1, now, event.clone());
        let err_reg = store.append(env_reg).unwrap_err();
        assert_eq!(
            err_reg,
            StoreError::SequenceRegression {
                studio_id,
                latest: 1,
                attempted: 1
            }
        );

        // Sequence 2 succeeds
        let env2 = EventEnvelope::new(studio_id, 2, now, event);
        assert!(store.append(env2).is_ok());
        assert_eq!(store.latest_sequence(studio_id).unwrap(), 2);
    }

    #[test]
    fn test_atomic_batch_all_or_zero() {
        let mut store = InMemoryStore::new();
        let studio_id = StudioId::new();
        let now = Utc::now();

        let event = ControlPlaneEvent::StudioCreated {
            studio: Studio::with_id(studio_id, "Batch Studio", now),
        };

        let env1 = EventEnvelope::new(studio_id, 1, now, event.clone());
        let env2 = EventEnvelope::new(studio_id, 2, now, event.clone());
        // env3 has sequence gap (4 instead of 3)
        let env3_gap = EventEnvelope::new(studio_id, 4, now, event.clone());

        let batch = vec![env1, env2, env3_gap];
        let result = store.append_batch(&batch);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            StoreError::SequenceGap {
                studio_id,
                expected: 3,
                actual: 4
            }
        );

        // ZERO events must be persisted
        assert_eq!(store.latest_sequence(studio_id).unwrap(), 0);
        assert!(store.all_events().unwrap().is_empty());

        // Valid batch persists ALL events
        let env1_valid = EventEnvelope::new(studio_id, 1, now, event.clone());
        let env2_valid = EventEnvelope::new(studio_id, 2, now, event.clone());
        let env3_valid = EventEnvelope::new(studio_id, 3, now, event.clone());
        assert!(
            store
                .append_batch(&[env1_valid, env2_valid.clone(), env3_valid])
                .is_ok()
        );
        assert_eq!(store.latest_sequence(studio_id).unwrap(), 3);
        assert_eq!(store.all_events().unwrap().len(), 3);

        // Duplicate event ID in batch or store rejected
        let dup_batch = vec![env2_valid];
        let dup_res = store.append_batch(&dup_batch);
        assert!(matches!(dup_res, Err(StoreError::DuplicateEventId { .. })));
        assert_eq!(store.event_count().unwrap(), 3);

        let any_store: AnyEventStore = store.into();
        assert_eq!(any_store.event_count().unwrap(), 3);
    }

    #[test]
    fn test_in_memory_store_events_tail_and_slice() {
        let mut store = InMemoryStore::new();
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

        // events_tail(3) returns sequences [8, 9, 10]
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

        // AnyEventStore delegation
        let any_store: AnyEventStore = store.into();
        assert_eq!(any_store.events_tail(2).unwrap().len(), 2);
        assert_eq!(any_store.events_slice(0, 2).unwrap().len(), 2);
    }
}
