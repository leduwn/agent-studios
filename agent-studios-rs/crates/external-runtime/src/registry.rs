use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::capabilities::RuntimeCapabilities;
use crate::discovery::DiscoveredRuntime;
use crate::error::RuntimeError;
use crate::id::RuntimeImplementationId;
use crate::traits::AgentRuntime;

/// Thread-safe registry managing registered external agent runtime implementations.
#[derive(Clone, Default)]
pub struct RuntimeRegistry {
    runtimes: Arc<RwLock<HashMap<RuntimeImplementationId, Arc<dyn AgentRuntime>>>>,
}

impl RuntimeRegistry {
    /// Creates a new empty runtime registry.
    pub fn new() -> Self {
        Self {
            runtimes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers a runtime implementation. Fails if an implementation with the same ID is already registered.
    pub async fn register(&self, runtime: Arc<dyn AgentRuntime>) -> Result<(), RuntimeError> {
        let mut guard = self.runtimes.write().await;
        let id = runtime.implementation_id().clone();
        if guard.contains_key(&id) {
            return Err(RuntimeError::DuplicateRuntime {
                implementation_id: id,
            });
        }
        guard.insert(id, runtime);
        Ok(())
    }

    /// Unregisters a runtime implementation by ID. Returns true if removed, false if not found.
    pub async fn unregister(&self, id: &RuntimeImplementationId) -> bool {
        let mut guard = self.runtimes.write().await;
        guard.remove(id).is_some()
    }

    /// Looks up a runtime implementation by ID, returning a typed `RuntimeNotFound` error if missing.
    pub async fn get(
        &self,
        id: &RuntimeImplementationId,
    ) -> Result<Arc<dyn AgentRuntime>, RuntimeError> {
        let guard = self.runtimes.read().await;
        guard
            .get(id)
            .cloned()
            .ok_or_else(|| RuntimeError::RuntimeNotFound { id: id.to_string() })
    }

    /// Checks if a runtime with the given ID is registered.
    pub async fn contains(&self, id: &RuntimeImplementationId) -> bool {
        let guard = self.runtimes.read().await;
        guard.contains_key(id)
    }

    /// Lists all registered runtime implementation IDs.
    pub async fn list_ids(&self) -> Vec<RuntimeImplementationId> {
        let guard = self.runtimes.read().await;
        guard.keys().cloned().collect()
    }

    /// Discovers all registered runtimes concurrently.
    pub async fn discover_all(&self) -> Result<Vec<DiscoveredRuntime>, RuntimeError> {
        let guard = self.runtimes.read().await;
        let mut results = Vec::with_capacity(guard.len());
        for runtime in guard.values() {
            results.push(runtime.discover().await?);
        }
        Ok(results)
    }

    /// Queries capabilities for all registered runtimes.
    pub async fn capabilities_all(&self) -> HashMap<RuntimeImplementationId, RuntimeCapabilities> {
        let guard = self.runtimes.read().await;
        let mut results = HashMap::with_capacity(guard.len());
        for (id, runtime) in guard.iter() {
            results.insert(id.clone(), runtime.capabilities().await);
        }
        results
    }

    /// Returns the number of registered runtimes.
    pub async fn count(&self) -> usize {
        let guard = self.runtimes.read().await;
        guard.len()
    }
}
