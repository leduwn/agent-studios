use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::error::ProtocolError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcEvent {
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub payload: serde_json::Value,
}

impl IpcEvent {
    pub fn new(sequence: u64, event_type: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            sequence,
            timestamp: Utc::now(),
            event_type: event_type.into(),
            payload,
        }
    }
}

pub struct ReplayBuffer {
    buffer: VecDeque<IpcEvent>,
    capacity: usize,
    last_sequence: Option<u64>,
}

impl ReplayBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
            last_sequence: None,
        }
    }

    pub fn push(&mut self, event: IpcEvent) -> Result<(), ProtocolError> {
        if let Some(last) = self.last_sequence
            && event.sequence <= last
        {
            return Err(ProtocolError::InvalidState(format!(
                "Non-monotonic sequence number: incoming {} <= last {}",
                event.sequence, last
            )));
        }

        self.last_sequence = Some(event.sequence);

        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }

        self.buffer.push_back(event);
        Ok(())
    }

    pub fn oldest_sequence(&self) -> Option<u64> {
        self.buffer.front().map(|e| e.sequence)
    }

    pub fn latest_sequence(&self) -> Option<u64> {
        self.buffer.back().map(|e| e.sequence)
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn replay_from(
        &self,
        from_sequence: u64,
        limit: Option<usize>,
    ) -> Result<Vec<IpcEvent>, ProtocolError> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }

        let oldest = self.buffer.front().unwrap().sequence;
        if from_sequence < oldest {
            return Err(ProtocolError::SequenceGapUnrecoverable {
                expected: from_sequence,
                found: oldest,
            });
        }

        let max_events = limit.unwrap_or(usize::MAX);
        let events: Vec<IpcEvent> = self
            .buffer
            .iter()
            .filter(|e| e.sequence >= from_sequence)
            .take(max_events)
            .cloned()
            .collect();

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replay_buffer_push_and_replay() {
        let mut buffer = ReplayBuffer::new(5);

        for seq in 1..=5 {
            buffer
                .push(IpcEvent::new(
                    seq,
                    "task_progress",
                    serde_json::json!({ "percent": seq * 20 }),
                ))
                .unwrap();
        }

        assert_eq!(buffer.len(), 5);
        assert_eq!(buffer.oldest_sequence(), Some(1));
        assert_eq!(buffer.latest_sequence(), Some(5));

        // Replay from sequence 3
        let replayed = buffer.replay_from(3, None).unwrap();
        assert_eq!(replayed.len(), 3);
        assert_eq!(replayed[0].sequence, 3);
        assert_eq!(replayed[2].sequence, 5);
    }

    #[test]
    fn test_replay_buffer_capacity_eviction_and_unrecoverable_gap() {
        let mut buffer = ReplayBuffer::new(3);

        for seq in 1..=5 {
            buffer
                .push(IpcEvent::new(
                    seq,
                    "chunk",
                    serde_json::json!({ "seq": seq }),
                ))
                .unwrap();
        }

        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.oldest_sequence(), Some(3));
        assert_eq!(buffer.latest_sequence(), Some(5));

        // Replaying seq 1 should fail with SequenceGapUnrecoverable
        let gap_err = buffer.replay_from(1, None);
        assert!(gap_err.is_err());
        assert!(matches!(
            gap_err.unwrap_err(),
            ProtocolError::SequenceGapUnrecoverable {
                expected: 1,
                found: 3
            }
        ));

        // Replaying seq 3 succeeds
        let valid = buffer.replay_from(3, None).unwrap();
        assert_eq!(valid.len(), 3);
    }

    #[test]
    fn test_non_monotonic_sequence_rejected() {
        let mut buffer = ReplayBuffer::new(5);
        buffer
            .push(IpcEvent::new(10, "init", serde_json::json!({})))
            .unwrap();

        let err = buffer.push(IpcEvent::new(10, "dup", serde_json::json!({})));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Non-monotonic"));

        let err2 = buffer.push(IpcEvent::new(9, "retrograde", serde_json::json!({})));
        assert!(err2.is_err());
    }
}
