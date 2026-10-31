# M15-02 Execution Readiness Aggregation & Fault Isolation

**Milestone**: M15-02  
**Component**: `agent-studios-ipc-transport::connection::build_execution_readiness_report`  
**Author**: Principal Systems Architect (Claude 1)  
**Date**: 2026-10-31  

---

## 1. Architectural Challenge

In a multi-runtime environment, runtime implementations may run locally, in external background daemons, or across network boundaries. Discovery against one external runtime must never:
1. Block discovery of other independent runtimes.
2. Panic or abort the readiness query if one runtime is failing, misconfigured, or unreachable.
3. Expose raw credentials, tokens, or unredacted environmental paths.

M15-02 introduces concurrent, fault-isolated discovery fan-out via `RuntimeRegistry::discover_all()` and robust readiness synthesis in `build_execution_readiness_report`.

---

## 2. Discovery Fan-Out & Fault Isolation Design

### 2.1 Concurrent Discovery with Tokio JoinSet
`RuntimeRegistry::discover_all()` spawns concurrent discovery tasks across all registered runtimes:

```rust
pub async fn discover_all(&self) -> MultiRuntimeDiscoveryReport {
    let runtimes = self.list().await;
    let mut join_set = tokio::task::JoinSet::new();

    for rt in runtimes {
        join_set.spawn(async move {
            let id = rt.implementation_id().clone();
            let result = rt.discover().await;
            (id, result)
        });
    }

    let mut instances = Vec::new();
    let mut failures = BTreeMap::new();

    while let Some(res) = join_set.join_next().await {
        match res {
            Ok((id, Ok(discovered))) => {
                instances.extend(discovered);
            }
            Ok((id, Err(err))) => {
                failures.insert(id, err);
            }
            Err(join_err) => {
                // Task panicked or cancelled; handle gracefully
            }
        }
    }

    MultiRuntimeDiscoveryReport {
        instances,
        failures,
    }
}
```

### 2.2 Fault Isolation Properties
- **Containment**: A crash, timeout, or discovery error in `rt-b` does not prevent instances from `rt-a` from appearing in `runtime_instances`.
- **Visibility**: Failures from `failures` are mapped into `ReadinessReport.blocking_reasons` under `RuntimeDiscoveryFailure`:
  ```rust
  for (impl_id, err) in &discovery_report.failures {
      blocking_reasons.push(format!(
          "Runtime discovery failed for implementation '{}': {}",
          impl_id, err
      ));
  }
  ```
- **Readiness Gate**: `is_ready` remains `true` if at least one healthy runtime instance exists and routing is unambiguous, allowing unaffected runtimes to proceed.

---

## 3. Ambiguity Detection in Readiness Reporting

When multiple healthy runtimes are registered:
- If a server default instance (`configured_instance_id`) is set, `is_ready` evaluates to `true` because the fallback path is unambiguous.
- If no default instance is set:
  - If multiple runtimes report healthy instances, `is_ready` evaluates to `false` and a blocking reason is emitted:
    ```
    "Multiple healthy runtime instances discovered across different implementations with no default configured. Clients must specify an explicit runtime_preference or runtime_instance_id."
    ```
  - This informs frontend clients (Mission Control) to require user selection before task dispatch.
