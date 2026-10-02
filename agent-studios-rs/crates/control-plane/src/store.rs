use std::collections::{HashMap, HashSet};

use agent_studios_protocol::event::EventEnvelope;
use agent_studios_protocol::id::{EventId, StudioId};

use crate::error::StoreError;

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
    }
}
