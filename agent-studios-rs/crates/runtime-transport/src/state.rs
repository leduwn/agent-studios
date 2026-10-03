use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex, RwLock};

use agent_studios_protocol_adapters::anthropic::continuation::AnthropicContinuationState;
use agent_studios_protocol_adapters::gemini::continuation::GeminiContinuationState;
use agent_studios_provider::id::ProviderInstanceId;

use crate::error::TransportError;

/// Composite key scoping multi-turn continuation and concurrency tracking to a specific provider instance.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ContinuationKey {
    pub provider_instance_id: ProviderInstanceId,
    pub thread_id: String,
}

impl ContinuationKey {
    pub fn new(provider_instance_id: ProviderInstanceId, thread_id: impl Into<String>) -> Self {
        Self {
            provider_instance_id,
            thread_id: thread_id.into(),
        }
    }
}

/// Committed and staged continuation state for a single thread on a provider instance.
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

/// RAII guard holding an active in-flight execution lease for a `ContinuationKey`.
///
/// Automatically releases the lease in `ContinuationManager` upon drop (on commit, rollback, error, cancellation, or panic).
pub struct ContinuationLease {
    key: ContinuationKey,
    manager: Arc<ContinuationManager>,
    active: bool,
}

impl ContinuationLease {
    fn new(key: ContinuationKey, manager: Arc<ContinuationManager>) -> Self {
        Self {
            key,
            manager,
            active: true,
        }
    }

    /// Explicitly releases the lease.
    pub fn release(&mut self) {
        if self.active {
            self.manager.release_in_flight(&self.key);
            self.active = false;
        }
    }
}

impl Drop for ContinuationLease {
    fn drop(&mut self) {
        self.release();
    }
}

/// Active staging transaction for a turn.
///
/// Holds an in-flight execution lease. If dropped before `commit()`, automatically
/// rolls back all staged changes and releases the lease.
pub struct ContinuationTransaction {
    key: ContinuationKey,
    staged: ThreadContinuation,
    committed: bool,
    manager: Arc<ContinuationManager>,
    lease: ContinuationLease,
}

impl fmt::Debug for ContinuationTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContinuationTransaction")
            .field("key", &self.key)
            .field("staged", &self.staged)
            .field("committed", &self.committed)
            .finish()
    }
}

impl ContinuationTransaction {
    /// Returns the continuation key for this transaction.
    pub fn key(&self) -> &ContinuationKey {
        &self.key
    }

    /// Returns the thread ID for this transaction.
    pub fn thread_id(&self) -> &str {
        &self.key.thread_id
    }

    /// Returns the provider instance ID for this transaction.
    pub fn provider_instance_id(&self) -> ProviderInstanceId {
        self.key.provider_instance_id
    }

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

    /// Commits the staged continuation state into durable thread storage and releases the in-flight lease.
    pub fn commit(mut self) -> Result<(), TransportError> {
        self.manager.apply_commit(&self.key, self.staged.clone())?;
        self.committed = true;
        self.lease.release();
        Ok(())
    }

    /// Explicitly rolls back and discards staged continuation changes and releases the in-flight lease.
    pub fn rollback(mut self) {
        self.committed = true; // Mark as resolved so Drop doesn't re-log
        self.lease.release();
    }
}

impl Drop for ContinuationTransaction {
    fn drop(&mut self) {
        if !self.committed {
            tracing::warn!(
                provider_instance_id = %self.key.provider_instance_id,
                thread_id = %self.key.thread_id,
                "ContinuationTransaction dropped without commit; staged state rolled back and in-flight lease released"
            );
        }
        // self.lease drops automatically here, releasing the lease if not already released!
    }
}

/// Thread-safe manager for multi-turn provider continuation states and concurrent execution leases.
#[derive(Debug, Default)]
pub struct ContinuationManager {
    states: RwLock<HashMap<ContinuationKey, ThreadContinuation>>,
    in_flight: Mutex<HashSet<ContinuationKey>>,
}

impl ContinuationManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Retrieves a clone of the committed continuation state for a given continuation key.
    pub fn get(&self, key: &ContinuationKey) -> ThreadContinuation {
        let read = self.states.read().unwrap();
        read.get(key).cloned().unwrap_or_default()
    }

    /// Checks if a request is currently in-flight for the given continuation key.
    pub fn is_in_flight(&self, key: &ContinuationKey) -> bool {
        let guard = self.in_flight.lock().unwrap();
        guard.contains(key)
    }

    /// Begins a transactional turn for the given continuation key.
    ///
    /// Acquires an exclusive in-flight execution lease. If another request is currently in-flight
    /// for the same key, deterministically returns `Err(TransportError::ConcurrentThreadInference)`.
    pub fn begin_transaction(
        self: &Arc<Self>,
        key: ContinuationKey,
    ) -> Result<ContinuationTransaction, TransportError> {
        {
            let mut guard = self.in_flight.lock().unwrap();
            if guard.contains(&key) {
                return Err(TransportError::ConcurrentThreadInference {
                    provider_instance_id: key.provider_instance_id,
                    thread_id: key.thread_id.clone(),
                });
            }
            guard.insert(key.clone());
        }

        let base_state = self.get(&key);
        let lease = ContinuationLease::new(key.clone(), Arc::clone(self));

        Ok(ContinuationTransaction {
            key,
            staged: base_state,
            committed: false,
            manager: Arc::clone(self),
            lease,
        })
    }

    /// Releases an active in-flight lease.
    fn release_in_flight(&self, key: &ContinuationKey) {
        let mut guard = self.in_flight.lock().unwrap();
        guard.remove(key);
    }

    /// Applies a committed transaction state into the table.
    fn apply_commit(
        &self,
        key: &ContinuationKey,
        state: ThreadContinuation,
    ) -> Result<(), TransportError> {
        let mut write = self.states.write().unwrap();
        write.insert(key.clone(), state);
        Ok(())
    }

    /// Clears continuation state for a completed or aborted thread.
    pub fn clear(&self, key: &ContinuationKey) {
        let mut write = self.states.write().unwrap();
        write.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_commit_and_rollback() {
        let manager = Arc::new(ContinuationManager::new());
        let instance_id = ProviderInstanceId::new();
        let key = ContinuationKey::new(instance_id, "test-thread-1");

        // Initial state is empty
        let initial = manager.get(&key);
        assert!(initial.anthropic.is_empty());

        // Stage changes in transaction
        let mut tx = manager.begin_transaction(key.clone()).unwrap();
        assert!(manager.is_in_flight(&key));

        tx.anthropic_mut().message_id = Some("msg-123".to_string());
        // Rollback explicitly
        tx.rollback();
        assert!(!manager.is_in_flight(&key));

        // State remains uncommitted
        assert_eq!(manager.get(&key).anthropic.message_id, None);

        // Stage and commit
        let mut tx2 = manager.begin_transaction(key.clone()).unwrap();
        assert!(manager.is_in_flight(&key));
        tx2.anthropic_mut().message_id = Some("msg-456".to_string());
        tx2.commit().unwrap();
        assert!(!manager.is_in_flight(&key));

        // State is now committed
        assert_eq!(
            manager.get(&key).anthropic.message_id.as_deref(),
            Some("msg-456")
        );

        // Dropping without commit rolls back and releases in-flight lease
        {
            let mut tx3 = manager.begin_transaction(key.clone()).unwrap();
            assert!(manager.is_in_flight(&key));
            tx3.anthropic_mut().message_id = Some("msg-789".to_string());
            // Drops here
        }
        assert!(!manager.is_in_flight(&key));
        assert_eq!(
            manager.get(&key).anthropic.message_id.as_deref(),
            Some("msg-456")
        );
    }

    #[test]
    fn test_same_thread_concurrency_guard() {
        let manager = Arc::new(ContinuationManager::new());
        let instance_id = ProviderInstanceId::new();
        let key = ContinuationKey::new(instance_id, "thread-concurrent");

        let tx1 = manager.begin_transaction(key.clone()).unwrap();
        assert!(manager.is_in_flight(&key));

        // Second simultaneous request for the same key must fail deterministically
        let err = manager.begin_transaction(key.clone()).unwrap_err();
        match err {
            TransportError::ConcurrentThreadInference {
                provider_instance_id,
                thread_id,
            } => {
                assert_eq!(provider_instance_id, instance_id);
                assert_eq!(thread_id, "thread-concurrent");
            }
            other => panic!("Unexpected error: {other:?}"),
        }

        // Another thread ID or different instance ID succeeds
        let instance_id_2 = ProviderInstanceId::new();
        let key_other_instance = ContinuationKey::new(instance_id_2, "thread-concurrent");
        let tx_other = manager.begin_transaction(key_other_instance).unwrap();

        let key_other_thread = ContinuationKey::new(instance_id, "thread-different");
        let tx_diff = manager.begin_transaction(key_other_thread).unwrap();

        // Dropping tx1 releases lease
        drop(tx1);
        assert!(!manager.is_in_flight(&key));

        // Now can acquire again
        let tx_retry = manager.begin_transaction(key.clone()).unwrap();
        drop(tx_retry);
        drop(tx_other);
        drop(tx_diff);
    }
}
