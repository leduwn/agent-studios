use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::capabilities::RuntimeCapabilities;
use crate::discovery::DiscoveredRuntimeInstance;
use crate::error::RuntimeError;
use crate::id::RuntimeImplementationId;
use crate::traits::AgentRuntime;

/// Thread-safe registry managing registered external agent runtime implementations.
/// Uses BTreeMap for deterministic ordering and strictly releases locks before awaiting async runtime operations.
#[derive(Clone, Default)]
pub struct RuntimeRegistry {
    runtimes: Arc<RwLock<BTreeMap<RuntimeImplementationId, Arc<dyn AgentRuntime>>>>,
}

impl RuntimeRegistry {
    /// Creates a new empty runtime registry.
    pub fn new() -> Self {
        Self {
            runtimes: Arc::new(RwLock::new(BTreeMap::new())),
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

    /// Lists all registered runtime implementation IDs in deterministic sorted order.
    pub async fn list_ids(&self) -> Vec<RuntimeImplementationId> {
        let guard = self.runtimes.read().await;
        guard.keys().cloned().collect()
    }

    /// Discovers all registered runtime instances concurrently across implementations,
    /// explicitly dropping the registry lock before awaiting async calls.
    pub async fn discover_all(&self) -> Result<Vec<DiscoveredRuntimeInstance>, RuntimeError> {
        let runtimes: Vec<Arc<dyn AgentRuntime>> = {
            let guard = self.runtimes.read().await;
            guard.values().cloned().collect()
        }; // Lock dropped before await

        let mut results = Vec::new();
        for runtime in runtimes {
            let instances = runtime.discover().await?;
            results.extend(instances);
        }
        Ok(results)
    }

    /// Queries capabilities for all registered runtimes in deterministic order,
    /// explicitly dropping the registry lock before awaiting async calls.
    pub async fn capabilities_all(&self) -> BTreeMap<RuntimeImplementationId, RuntimeCapabilities> {
        let entries: Vec<(RuntimeImplementationId, Arc<dyn AgentRuntime>)> = {
            let guard = self.runtimes.read().await;
            guard
                .iter()
                .map(|(k, v)| (k.clone(), Arc::clone(v)))
                .collect()
        }; // Lock dropped before await

        let mut results = BTreeMap::new();
        for (id, runtime) in entries {
            let caps = runtime.capabilities().await;
            results.insert(id, caps);
        }
        results
    }

    /// Returns the number of registered runtimes.
    pub async fn count(&self) -> usize {
        let guard = self.runtimes.read().await;
        guard.len()
    }
}
