use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use agent_studios_protocol_adapters::anthropic::continuation::AnthropicContinuationState;
use agent_studios_protocol_adapters::gemini::continuation::GeminiContinuationState;

use crate::error::TransportError;

/// Committed and staged continuation state for a single thread.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThreadContinuation {
    pub anthropic: AnthropicContinuationState,
    pub gemini: GeminiContinuationState,
}

impl ThreadContinuation {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Active staging transaction for a turn.
///
/// If dropped before `commit()`, automatically rolls back all staged changes.
pub struct ContinuationTransaction {
    thread_id: String,
    staged: ThreadContinuation,
    committed: bool,
    manager: Arc<ContinuationManager>,
}

impl ContinuationTransaction {
    /// Mutably borrows the staged Anthropic continuation state.
    pub fn anthropic_mut(&mut self) -> &mut AnthropicContinuationState {
        &mut self.staged.anthropic
    }

    /// Mutably borrows the staged Gemini continuation state.
    pub fn gemini_mut(&mut self) -> &mut GeminiContinuationState {
        &mut self.staged.gemini
    }

    /// Read-only reference to current staged state.
    pub fn staged(&self) -> &ThreadContinuation {
        &self.staged
    }

    /// Commits the staged continuation state into durable thread storage.
    pub fn commit(mut self) -> Result<(), TransportError> {
        self.manager
            .apply_commit(&self.thread_id, self.staged.clone())?;
        self.committed = true;
        Ok(())
    }

    /// Explicitly rolls back and discards staged continuation changes.
    pub fn rollback(mut self) {
        self.committed = true; // Mark as resolved so Drop doesn't re-log or re-trigger
    }
}

impl Drop for ContinuationTransaction {
    fn drop(&mut self) {
        if !self.committed {
            tracing::warn!(
                thread_id = %self.thread_id,
                "ContinuationTransaction dropped without commit; staged state rolled back"
            );
        }
    }
}

/// Thread-safe manager for multi-turn provider continuation states.
#[derive(Debug, Default)]
pub struct ContinuationManager {
    states: RwLock<HashMap<String, ThreadContinuation>>,
}

impl ContinuationManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Retrieves a clone of the committed continuation state for a thread.
    pub fn get(&self, thread_id: &str) -> ThreadContinuation {
        let read = self.states.read().unwrap();
        read.get(thread_id).cloned().unwrap_or_default()
    }

    /// Begins a transactional turn, cloning committed state into staging.
    pub fn begin_transaction(
        self: &Arc<Self>,
        thread_id: impl Into<String>,
    ) -> ContinuationTransaction {
        let tid = thread_id.into();
        let base_state = self.get(&tid);

        ContinuationTransaction {
            thread_id: tid,
            staged: base_state,
            committed: false,
            manager: Arc::clone(self),
        }
    }

    /// Applies a committed transaction state into the thread table.
    fn apply_commit(
        &self,
        thread_id: &str,
        state: ThreadContinuation,
    ) -> Result<(), TransportError> {
        let mut write = self.states.write().unwrap();
        write.insert(thread_id.to_string(), state);
        Ok(())
    }

    /// Clears continuation state for a completed or aborted thread.
    pub fn clear(&self, thread_id: &str) {
        let mut write = self.states.write().unwrap();
        write.remove(thread_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_commit_and_rollback() {
        let manager = Arc::new(ContinuationManager::new());
        let thread_id = "test-thread-1";

        // Initial state is empty
        let initial = manager.get(thread_id);
        assert!(initial.anthropic.is_empty());

        // Stage changes in transaction
        let mut tx = manager.begin_transaction(thread_id);
        tx.anthropic_mut().message_id = Some("msg-123".to_string());
        // Rollback explicitly
        tx.rollback();

        // State remains uncommitted
        assert_eq!(manager.get(thread_id).anthropic.message_id, None);

        // Stage and commit
        let mut tx2 = manager.begin_transaction(thread_id);
        tx2.anthropic_mut().message_id = Some("msg-456".to_string());
        tx2.commit().unwrap();

        // State is now committed
        assert_eq!(
            manager.get(thread_id).anthropic.message_id.as_deref(),
            Some("msg-456")
        );

        // Dropping without commit rolls back
        {
            let mut tx3 = manager.begin_transaction(thread_id);
            tx3.anthropic_mut().message_id = Some("msg-789".to_string());
            // Drops here
        }
        assert_eq!(
            manager.get(thread_id).anthropic.message_id.as_deref(),
            Some("msg-456")
        );
    }
}
