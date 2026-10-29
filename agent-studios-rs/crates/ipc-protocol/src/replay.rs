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
    contiguous_only: bool,
}

impl ReplayBuffer {
    pub fn new(capacity: usize) -> Result<Self, ProtocolError> {
        if capacity == 0 {
            return Err(ProtocolError::InvalidCapacity(
                "Buffer capacity must be greater than zero".into(),
            ));
        }

        Ok(Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
            last_sequence: None,
            contiguous_only: false,
        })
    }

    pub fn new_contiguous(capacity: usize) -> Result<Self, ProtocolError> {
        let mut buf = Self::new(capacity)?;
        buf.contiguous_only = true;
        Ok(buf)
    }

    pub fn push(&mut self, event: IpcEvent) -> Result<(), ProtocolError> {
        if self.contiguous_only {
            return self.push_contiguous(event);
        }

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

    pub fn push_contiguous(&mut self, event: IpcEvent) -> Result<(), ProtocolError> {
        if let Some(last) = self.last_sequence
            && event.sequence != last + 1
        {
            return Err(ProtocolError::SequenceGapRecoverable {
                expected: last + 1,
                found: event.sequence,
            });
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

        let latest = self.buffer.back().unwrap().sequence;
        if from_sequence > latest + 1 {
            return Err(ProtocolError::SequenceGapRecoverable {
                expected: latest + 1,
                found: from_sequence,
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
    fn test_replay_buffer_reject_zero_capacity() {
        let err = ReplayBuffer::new(0);
        assert!(matches!(
            err,
            Err(ProtocolError::InvalidCapacity(ref msg)) if msg == "Buffer capacity must be greater than zero"
        ));

        let err_contiguous = ReplayBuffer::new_contiguous(0);
        assert!(matches!(
            err_contiguous,
            Err(ProtocolError::InvalidCapacity(ref msg)) if msg == "Buffer capacity must be greater than zero"
        ));
    }

    #[test]
    fn test_replay_buffer_push_and_replay() {
        let mut buffer = ReplayBuffer::new(5).unwrap();

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

        // Replay with limit
        let limited = buffer.replay_from(2, Some(2)).unwrap();
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].sequence, 2);
        assert_eq!(limited[1].sequence, 3);
    }

    #[test]
    fn test_replay_buffer_capacity_eviction_and_unrecoverable_gap() {
        let mut buffer = ReplayBuffer::new(3).unwrap();

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
        assert_eq!(
            gap_err.unwrap_err(),
            ProtocolError::SequenceGapUnrecoverable {
                expected: 1,
                found: 3
            }
        );

        // Replaying seq 3 succeeds
        let valid = buffer.replay_from(3, None).unwrap();
        assert_eq!(valid.len(), 3);
    }

    #[test]
    fn test_non_monotonic_sequence_rejected() {
        let mut buffer = ReplayBuffer::new(5).unwrap();
        buffer
            .push(IpcEvent::new(10, "init", serde_json::json!({})))
            .unwrap();

        let err = buffer.push(IpcEvent::new(10, "dup", serde_json::json!({})));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Non-monotonic"));

        let err2 = buffer.push(IpcEvent::new(9, "retrograde", serde_json::json!({})));
        assert!(err2.is_err());
    }

    #[test]
    fn test_contiguous_vs_non_contiguous_gaps() {
        // Standard buffer allows non-contiguous gaps on push
        let mut standard = ReplayBuffer::new(5).unwrap();
        assert!(
            standard
                .push(IpcEvent::new(1, "e1", serde_json::json!({})))
                .is_ok()
        );
        assert!(
            standard
                .push(IpcEvent::new(5, "e5", serde_json::json!({})))
                .is_ok()
        );

        // But push_contiguous on standard buffer rejects gap
        let gap_err = standard.push_contiguous(IpcEvent::new(7, "e7", serde_json::json!({})));
        assert_eq!(
            gap_err.unwrap_err(),
            ProtocolError::SequenceGapRecoverable {
                expected: 6,
                found: 7
            }
        );
        assert!(
            standard
                .push_contiguous(IpcEvent::new(6, "e6", serde_json::json!({})))
                .is_ok()
        );

        // Contiguous buffer created with new_contiguous rejects gap on standard push
        let mut contiguous = ReplayBuffer::new_contiguous(5).unwrap();
        assert!(
            contiguous
                .push(IpcEvent::new(10, "start", serde_json::json!({})))
                .is_ok()
        );
        assert!(
            contiguous
                .push(IpcEvent::new(11, "next", serde_json::json!({})))
                .is_ok()
        );
        let gap_err2 = contiguous.push(IpcEvent::new(13, "skip", serde_json::json!({})));
        assert_eq!(
            gap_err2.unwrap_err(),
            ProtocolError::SequenceGapRecoverable {
                expected: 12,
                found: 13
            }
        );
    }

    #[test]
    fn test_replay_gap_unrecoverable_vs_recoverable() {
        let mut buffer = ReplayBuffer::new(3).unwrap();
        buffer
            .push(IpcEvent::new(10, "e10", serde_json::json!({})))
            .unwrap();
        buffer
            .push(IpcEvent::new(11, "e11", serde_json::json!({})))
            .unwrap();
        buffer
            .push(IpcEvent::new(12, "e12", serde_json::json!({})))
            .unwrap();
        buffer
            .push(IpcEvent::new(13, "e13", serde_json::json!({})))
            .unwrap();

        // Oldest is 11, latest is 13
        assert_eq!(buffer.oldest_sequence(), Some(11));
        assert_eq!(buffer.latest_sequence(), Some(13));

        // from_sequence < oldest -> SequenceGapUnrecoverable
        let unrec = buffer.replay_from(10, None);
        assert_eq!(
            unrec.unwrap_err(),
            ProtocolError::SequenceGapUnrecoverable {
                expected: 10,
                found: 11
            }
        );

        // from_sequence == latest + 1 -> Ok(empty) (client caught up)
        let caught_up = buffer.replay_from(14, None).unwrap();
        assert!(caught_up.is_empty());

        // from_sequence > latest + 1 -> SequenceGapRecoverable
        let rec = buffer.replay_from(15, None);
        assert_eq!(
            rec.unwrap_err(),
            ProtocolError::SequenceGapRecoverable {
                expected: 14,
                found: 15
            }
        );
    }

    #[test]
    fn test_boundary_capacity_and_overflow() {
        let mut buffer = ReplayBuffer::new(1).unwrap();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
        assert_eq!(buffer.oldest_sequence(), None);
        assert_eq!(buffer.latest_sequence(), None);

        // Empty replay returns empty vec
        assert!(buffer.replay_from(1, None).unwrap().is_empty());

        // Push 1 item
        buffer
            .push(IpcEvent::new(1, "e1", serde_json::json!({})))
            .unwrap();
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.oldest_sequence(), Some(1));
        assert_eq!(buffer.latest_sequence(), Some(1));

        // Overflow capacity 1
        buffer
            .push(IpcEvent::new(2, "e2", serde_json::json!({})))
            .unwrap();
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.oldest_sequence(), Some(2));
        assert_eq!(buffer.latest_sequence(), Some(2));

        // Replay seq 1 unrecoverable
        assert_eq!(
            buffer.replay_from(1, None).unwrap_err(),
            ProtocolError::SequenceGapUnrecoverable {
                expected: 1,
                found: 2
            }
        );

        // Replay seq 2 returns item
        let events = buffer.replay_from(2, None).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sequence, 2);

        // Replay seq 3 (latest + 1) returns empty
        assert!(buffer.replay_from(3, None).unwrap().is_empty());

        // Replay seq 4 (> latest + 1) returns recoverable gap
        assert_eq!(
            buffer.replay_from(4, None).unwrap_err(),
            ProtocolError::SequenceGapRecoverable {
                expected: 3,
                found: 4
            }
        );
    }
}
