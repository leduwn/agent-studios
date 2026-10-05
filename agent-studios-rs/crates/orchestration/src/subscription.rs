use agent_studios_protocol::event::EventEnvelope;

/// Tracks observed sequence numbers to guarantee gapless monotonic delivery.
#[derive(Debug, Clone, Default)]
pub struct SequenceTracker {
    last_seen_seq: Option<u64>,
}

impl SequenceTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_last_seen(last_seen_seq: Option<u64>) -> Self {
        Self { last_seen_seq }
    }

    pub fn last_seen_sequence(&self) -> Option<u64> {
        self.last_seen_seq
    }

    /// Observes the envelope and enforces that sequence is strictly monotonic and gapless.
    pub fn observe(&mut self, envelope: &EventEnvelope) -> Result<(), SequenceError> {
        if let Some(last) = self.last_seen_seq
            && envelope.sequence != last + 1
        {
            return Err(SequenceError::GapOrOutdated {
                expected: last + 1,
                actual: envelope.sequence,
            });
        }
        self.last_seen_seq = Some(envelope.sequence);
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SequenceError {
    #[error("sequence gap or out of order: expected {expected}, got {actual}")]
    GapOrOutdated { expected: u64, actual: u64 },
}
