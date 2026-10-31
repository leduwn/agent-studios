# M15-02 Deterministic Per-Task Routing Specification

**Milestone**: M15-02  
**Component**: `agent-studios-ipc-transport::connection::auto_execute_task`  
**Author**: Principal Systems Architect (Claude 1)  
**Date**: 2026-10-31  

---

## 1. Specification Overview

When a task transitions into automatic execution via `auto_execute_task`, the daemon resolves the target `(Arc<dyn AgentRuntime>, DiscoveredRuntimeInstance)` based on `TaskExecutionSpec`:
```rust
pub struct TaskExecutionSpec {
    pub repository_path: Option<String>,
    pub initial_prompt: Option<String>,
    pub runtime_preference: Option<String>,
    pub runtime_instance_id: Option<RuntimeInstanceId>,
    pub model_ref: Option<ModelReference>,
}
```

The resolution algorithm evaluates an exhaustive 4-case deterministic decision tree. Any violation or ambiguity triggers an immediate fail-closed transition of the `Run` and `Task` into `Failed`.

---

## 2. Decision Tree Specification

```
                                  [TaskExecutionSpec]
                                           |
               +---------------------------+---------------------------+
               |                                                       |
        preference = Some                                       preference = None
               |                                                       |
       +-------+-------+                                       +-------+-------+
       |               |                                       |               |
instance = Some  instance = None                       instance = Some  instance = None
   [CASE 1]        [CASE 3]                                [CASE 2]        [CASE 4]
```

### Case 1: Explicit Preference & Explicit Instance (`Some(pref)`, `Some(instance_id)`)
1. Lookup runtime implementation where `rt.implementation_id().as_str() == pref`.
   - If not found: **Fail Closed** (`UnknownRuntimePreference`).
2. Run discovery on the selected runtime to retrieve available instances.
3. Verify that `instance_id` exists in the discovered instance list.
   - If not found or belongs to another runtime: **Fail Closed** (`MismatchedRuntimePreferenceAndInstance`).
4. Verify that instance `healthy == true`.
   - If unhealthy: **Fail Closed** (`RuntimeInstanceUnhealthy`).
5. **Dispatch**: Select `(runtime, instance)`.

### Case 2: Explicit Instance Only (`None`, `Some(instance_id)`)
1. Scan all registered runtimes via `runtime_registry.list()`.
2. Discover instances across all registered runtimes.
3. Identify candidate runtimes where `instances.any(|i| i.instance_id == instance_id)`.
   - If 0 matches: **Fail Closed** (`RuntimeInstanceNotFound`).
   - If >1 matches: **Fail Closed** (`AmbiguousRuntimeRouting`).
4. Verify the matching instance is `healthy == true`.
   - If unhealthy: **Fail Closed** (`RuntimeInstanceUnhealthy`).
5. **Dispatch**: Select `(owning_runtime, matching_instance)`.

### Case 3: Explicit Preference Only (`Some(pref)`, `None`)
1. Lookup runtime implementation where `rt.implementation_id().as_str() == pref`.
   - If not found: **Fail Closed** (`UnknownRuntimePreference`).
2. Discover instances on the selected runtime. Filter healthy instances (`healthy == true`).
   - If 0 healthy instances: **Fail Closed** (`NoHealthyRuntimeInstances`).
3. If server has a `configured_instance_id` belonging to this runtime, select it.
4. Otherwise, if exactly 1 healthy instance exists, select it.
   - If >1 healthy instances exist without configured default: **Fail Closed** (`AmbiguousRuntimeRouting`).
5. **Dispatch**: Select `(runtime, selected_instance)`.

### Case 4: Default / Unspecified (`None`, `None`)
1. If server has a `configured_instance_id`:
   - Discover across all runtimes to locate the configured instance.
   - If found and healthy: **Dispatch** `(owning_runtime, configured_instance)`.
   - If unhealthy: **Fail Closed** (`RuntimeInstanceUnhealthy`).
2. If no `configured_instance_id` exists:
   - Query all registered runtimes.
   - If total registered runtimes == 0: **Fail Closed** (`NoRuntimeRegistered`).
   - If total registered runtimes == 1:
     - Discover instances on that single runtime. Filter healthy instances.
     - If exactly 1 healthy instance: **Dispatch** `(single_runtime, single_instance)`.
     - If 0 healthy: **Fail Closed** (`NoHealthyRuntimeInstances`).
     - If >1 healthy: **Fail Closed** (`AmbiguousRuntimeRouting`).
   - If total registered runtimes > 1:
     - Check total healthy instances across all runtimes.
     - If exactly 1 healthy instance exists across all runtimes: **Dispatch** `(owning_runtime, single_instance)`.
     - If >1 healthy instances exist: **Fail Closed** (`AmbiguousRuntimeRouting`).

---

## 3. RunRecord Persistence Guarantee

Upon successful resolution of `(selected_runtime, selected_instance)`:
- `RunRecord.execution` is updated authoritatively:
  ```rust
  run_record.execution = Some(TaskExecutionSpec {
      repository_path: Some(canonical_repo_path),
      initial_prompt: effective_prompt,
      runtime_preference: Some(selected_runtime.implementation_id().to_string()),
      runtime_instance_id: Some(selected_instance.instance_id),
      model_ref: Some(resolved_model_ref),
  });
  ```
- Downstream clients inspecting task runs are guaranteed to read the exact, resolved execution parameters.
