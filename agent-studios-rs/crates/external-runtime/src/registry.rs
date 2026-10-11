use std::collections::BTreeMap;
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinSet;

use serde::{Deserialize, Serialize};

use crate::capabilities::RuntimeCapabilities;
use crate::discovery::DiscoveredRuntimeInstance;
use crate::error::{RuntimeError, SanitizedRuntimeMessage};
use crate::id::RuntimeImplementationId;
use crate::traits::AgentRuntime;

/// Typed outcome of discovering runtime instances across all registered implementations.
/// Preserves successful instance discoveries while isolating and reporting individual runtime failures.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RegistryDiscoveryOutcome {
    /// Discovered runtime instances ordered deterministically by implementation ID.
    pub instances: Vec<DiscoveredRuntimeInstance>,
    /// Failures encountered during discovery, keyed by implementation ID.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub failures: BTreeMap<RuntimeImplementationId, SanitizedRuntimeMessage>,
}

impl RegistryDiscoveryOutcome {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty() && self.failures.is_empty()
    }

    pub fn is_success(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }

    /// Unwraps the discovered instances if no runtime discovery failures occurred,
    /// or returns an error summarizing discovery failures.
    pub fn into_result(self) -> Result<Vec<DiscoveredRuntimeInstance>, RuntimeError> {
        if let Some((failed_id, reason)) = self.failures.into_iter().next() {
            return Err(RuntimeError::DiscoveryFailed {
                reason: SanitizedRuntimeMessage::new(format!(
                    "Discovery failed for runtime '{}': {}",
                    failed_id, reason
                )),
            });
        }
        Ok(self.instances)
    }

    /// Convenience unwrapper for tests and callers that expect no failures.
    pub fn unwrap(self) -> Vec<DiscoveredRuntimeInstance> {
        self.into_result()
            .expect("Registry discovery encountered unexpected runtime failures")
    }
}

impl Deref for RegistryDiscoveryOutcome {
    type Target = [DiscoveredRuntimeInstance];

    fn deref(&self) -> &Self::Target {
        &self.instances
    }
}

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
    /// Preserves deterministic alphabetical ordering and isolates individual discovery failures.
    pub async fn discover_all(&self) -> RegistryDiscoveryOutcome {
        let entries: Vec<(RuntimeImplementationId, Arc<dyn AgentRuntime>)> = {
            let guard = self.runtimes.read().await;
            guard
                .iter()
                .map(|(k, v)| (k.clone(), Arc::clone(v)))
                .collect()
        }; // Lock dropped before await

        let mut task_map =
            std::collections::HashMap::<tokio::task::Id, RuntimeImplementationId>::new();
        let mut join_set = JoinSet::new();

        for (id, runtime) in entries.clone() {
            let impl_id = id.clone();
            let handle = join_set.spawn(async move {
                let res = runtime.discover().await;
                (impl_id, res)
            });
            task_map.insert(handle.id(), id);
        }

        let mut collected: BTreeMap<
            RuntimeImplementationId,
            Result<Vec<DiscoveredRuntimeInstance>, RuntimeError>,
        > = BTreeMap::new();
        let mut worker_failures: BTreeMap<RuntimeImplementationId, SanitizedRuntimeMessage> =
            BTreeMap::new();

        while let Some(join_res) = join_set.join_next().await {
            match join_res {
                Ok((id, discover_res)) => {
                    task_map.retain(|_, v| v != &id);
                    collected.insert(id, discover_res);
                }
                Err(panic_err) => {
                    let task_id = panic_err.id();
                    let attributed_id = task_map.remove(&task_id);
                    let action = if panic_err.is_panic() {
                        "panicked"
                    } else {
                        "was cancelled"
                    };
                    let log_msg = match &attributed_id {
                        Some(id) => {
                            format!("Runtime discovery worker for implementation '{id}' {action}")
                        }
                        None => format!("Runtime discovery worker {action}"),
                    };
                    tracing::error!("{log_msg}");
                    if let Some(id) = attributed_id {
                        worker_failures.insert(id, SanitizedRuntimeMessage::new(log_msg));
                    }
                }
            }
        }

        let mut outcome = RegistryDiscoveryOutcome::new();

        // Attribute any remaining unaccounted-for implementations from entries
        for (id, _) in entries {
            if !collected.contains_key(&id) && !worker_failures.contains_key(&id) {
                worker_failures.insert(
                    id.clone(),
                    SanitizedRuntimeMessage::new(
                        "Runtime discovery task failed unexpectedly without result",
                    ),
                );
            }
        }

        // Iterate in deterministic sorted BTreeMap key order
        for (id, res) in collected {
            match res {
                Ok(instances) => {
                    outcome.instances.extend(instances);
                }
                Err(err) => {
                    outcome
                        .failures
                        .insert(id, SanitizedRuntimeMessage::new(err.to_string()));
                }
            }
        }

        for (id, failure) in worker_failures {
            outcome.failures.insert(id, failure);
        }

        outcome
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
