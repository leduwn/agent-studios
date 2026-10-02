use std::collections::HashMap;

use agent_studios_protocol::event::EventEnvelope;
use agent_studios_protocol::id::StudioId;

use crate::error::StoreError;

/// Storage abstraction for persisting and retrieving domain event streams.
pub trait EventStore: Send + Sync {
    /// Appends a new event envelope to the store.
    /// Sequence must be strictly `latest_sequence + 1` for the target studio.
    fn append(&mut self, envelope: EventEnvelope) -> Result<(), StoreError>;

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
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl EventStore for InMemoryStore {
    fn append(&mut self, envelope: EventEnvelope) -> Result<(), StoreError> {
        let studio_id = envelope.studio_id;
        let latest = self.latest_sequence(studio_id)?;
        let expected = latest + 1;

        if envelope.sequence < expected {
            return Err(StoreError::SequenceRegression {
                studio_id,
                latest,
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

        self.all_events.push(envelope.clone());
        self.events_by_studio
            .entry(studio_id)
            .or_default()
            .push(envelope);

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
}
