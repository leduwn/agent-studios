# Claude 2 Cross-Team Handoff: M15-02 Multi-Runtime Adapter Registry & Routing

**Date**: 2026-10-31  
**Target Pull Requests**: PR #23 (`feat/m14-ipc-03-connected-client`), PR #25 (`feat/m14-mission-control-connected-integration`), PR #26 (`feat/m15-01-per-task-execution-contract`)  
**Backend Branch**: `feat/m15-02-multi-runtime-registry` (stacked on `feat/m15-01-per-task-execution-contract`)  
**Author**: Principal Systems Architect (Claude 1)  
**Recipient**: Principal Frontend / Extension Architect (Claude 2) & Release Engineering  
**Status**: AUTHORITATIVE HANDOFF & DRAFT PR NOTICE  

---

## 1. Overview for Claude 2

Milestone M15-02 unlocks true multi-runtime execution support across Agent Studios. Where M15-01 supported per-task configuration against a single server runtime, M15-02 allows multiple heterogeneous runtimes (e.g., `opencode`, `claude-code`, custom test mock adapters) to be registered and queried simultaneously.

### Key Capabilities Enabled for Claude 2:
1. **Multi-Runtime Execution Readiness**:
   - `get_execution_readiness()` now returns the aggregate list of all healthy runtimes in `available_runtimes` (e.g., `["opencode", "claude-code"]`) and all discovered instances across all registered runtimes in `runtime_instances`.
   - Single runtime discovery failures are isolated: healthy runtimes remain usable, while failing runtimes are detailed in `blocking_reasons`.
2. **Deterministic Multi-Runtime Routing**:
   - Tasks submitted with `task_spec.execution.runtime_preference` route directly to the matching runtime implementation.
   - Tasks submitted with `task_spec.execution.runtime_instance_id` route to whichever runtime owns that instance.
   - If both are supplied, the backend cross-validates them and fails closed if mismatched.
3. **Session Ownership Invariants**:
   - Cancellations (`cancel_task`) and approval resolutions (`resolve_approval`) automatically route directly to the owning runtime adapter that spawned the task's session.
4. **Ambiguity Prevention**:
   - If multiple runtimes exist and the task specifies neither preference nor instance, and the server has no configured default instance, the run fails closed with an unambiguous error instructing the client to select a runtime.

---

## 2. Wire Protocol Samples & Schemas

### 2.1 Execution Readiness Response with Multiple Runtimes

```json
{
  "type": "execution_readiness",
  "readiness": {
    "is_ready": true,
    "available_runtimes": [
      "claude-code",
      "opencode"
    ],
    "runtime_instances": [
      {
        "instance_id": "rt-inst-c0a80101-0000-0000-0000-000000000001",
        "implementation": "claude-code",
        "healthy": true
      },
      {
        "instance_id": "rt-inst-c0a80101-0000-0000-0000-000000000002",
        "implementation": "opencode",
        "healthy": true
      }
    ],
    "available_models": [
      {
        "provider_instance_id": "c0a80101-0000-0000-0000-000000000003",
        "model_id": "claude-3-7-sonnet",
        "display_name": "Claude 3.7 Sonnet"
      }
    ],
    "authorized_workspaces": [
      "d:/Projects/agent-studios"
    ],
    "defaults": {
      "default_repo_path": "d:/Projects/agent-studios",
      "default_provider_instance_id": "c0a80101-0000-0000-0000-000000000003",
      "default_model_id": "claude-3-7-sonnet",
      "configured_instance_id": "rt-inst-c0a80101-0000-0000-0000-000000000001"
    },
    "blocking_reasons": []
  }
}
```

### 2.2 Submitting Tasks with Runtime Preference

```json
{
  "type": "submit_task",
  "studio_id": "c0a80101-0000-0000-0000-000000000010",
  "task_spec": {
    "title": "Implement cache layer",
    "description": "Add memory cache to HTTP client",
    "execution": {
      "runtime_preference": "opencode",
      "runtime_instance_id": "rt-inst-c0a80101-0000-0000-0000-000000000002",
      "model_ref": {
        "model_id": "claude-3-7-sonnet"
      }
    }
  }
}
```

---

## 3. Backward Compatibility & Migration Guidance

- **Existing M13/M15-01 Callers**: Zero changes required. If `runtime_preference` and `runtime_instance_id` are omitted, the daemon dispatches to the single registered runtime or the server's configured default instance.
- **Server Builder API**:
  - `IpcServerBuilder::with_runtime(Arc<dyn AgentRuntime>)` remains available for single runtime initialization.
  - `IpcServerBuilder::with_runtimes(Vec<Arc<dyn AgentRuntime>>)` and `with_runtime_registry(RuntimeRegistry)` added.
  - All builder methods remain 100% synchronous (`build() -> Result<IpcServer, TransportError>`).
