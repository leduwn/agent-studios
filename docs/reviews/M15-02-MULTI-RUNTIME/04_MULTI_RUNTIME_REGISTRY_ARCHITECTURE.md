# M15-02 Multi-Runtime Registry Architecture Specification

**Milestone**: M15-02  
**Component**: `agent-studios-external-runtime` / `agent-studios-ipc-transport`  
**Author**: Principal Systems Architect (Claude 1)  
**Date**: 2026-10-31  

---

## 1. Architectural Motivation

In prior iterations, the IPC server stored a single optional runtime adapter:
```rust
// Legacy M15-01 ServerState
pub runtime: Option<Arc<dyn AgentRuntime>>,
```
This prohibited the simultaneous availability of multiple runtime engines (e.g. OpenCode runtime for local open weights, Claude Code runtime for Anthropic CLI tasks, and mock adapters for test automation).

M15-02 introduces the `RuntimeRegistry` abstraction in `agent-studios-external-runtime`, transitioning `ServerState` to:
```rust
// M15-02 ServerState
pub runtime_registry: RuntimeRegistry,
```

---

## 2. Component Design & Concurrency Model

### 2.1 Registry Storage & Ordering Invariant
`RuntimeRegistry` encapsulates an `Arc<RwLock<BTreeMap<RuntimeImplementationId, Arc<dyn AgentRuntime>>>>`:
- **Deterministic Keying**: Indexed strictly by `RuntimeImplementationId`.
- **Alphabetical Traversal**: `BTreeMap` guarantees deterministic iteration order during readiness aggregation and candidate evaluation across platforms.
- **Duplicate Prevention**: Rejects duplicate registrations with `RuntimeError::DuplicateRuntime`.

```rust
#[derive(Clone, Default)]
pub struct RuntimeRegistry {
    runtimes: Arc<RwLock<BTreeMap<RuntimeImplementationId, Arc<dyn AgentRuntime>>>>,
}
```

### 2.2 Synchronous Builder Compatibility
A critical design requirement was preserving the synchronous contract of `IpcServerBuilder::build(self) -> Result<IpcServer, TransportError>` without spawning nested tokio runtimes or blocking on async locks.

To satisfy this, `RuntimeRegistry` exposes synchronous initializers:
```rust
pub fn from_runtimes(
    runtimes: impl IntoIterator<Item = Arc<dyn AgentRuntime>>,
) -> Result<Self, RuntimeError> {
    let mut map = BTreeMap::new();
    for rt in runtimes {
        let id = rt.implementation_id().clone();
        if map.contains_key(&id) {
            return Err(RuntimeError::DuplicateRuntime {
                implementation_id: id,
            });
        }
        map.insert(id, rt);
    }
    Ok(Self {
        runtimes: Arc::new(RwLock::new(map)),
    })
}

pub fn register_sync(&self, runtime: Arc<dyn AgentRuntime>) -> Result<(), RuntimeError> {
    let mut guard = self
        .runtimes
        .try_write()
        .map_err(|_| RuntimeError::Internal {
            reason: SanitizedRuntimeMessage::new(
                "Runtime registry lock unavailable for sync registration",
            ),
        })?;
    let id = runtime.implementation_id().clone();
    if guard.contains_key(&id) {
        return Err(RuntimeError::DuplicateRuntime {
            implementation_id: id,
        });
    }
    guard.insert(id, runtime);
    Ok(())
}
```

### 2.3 Lock Discipline
To prevent lock starvation or deadlocks:
1. Locks on `RuntimeRegistry::runtimes` are held only for pointer clones (`Arc<dyn AgentRuntime>`).
2. No `.await` operations are ever executed while holding a read or write lock on the registry map.
3. Runtimes retrieved from the registry are returned as reference-counted handles (`Arc<dyn AgentRuntime>`) and invoked independently outside the registry critical section.
