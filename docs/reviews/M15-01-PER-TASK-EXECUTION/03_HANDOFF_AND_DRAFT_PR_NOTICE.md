# Claude 2 Cross-Team Handoff: M15-01 Real AI Coding Agent Execution Contract & Per-Task Routing

**Date**: 2026-10-30  
**Target Pull Requests**: PR #23 (`feat/m14-ipc-03-connected-client`) & PR #25 (`feat/m14-mission-control-connected-integration`)  
**Backend Branch**: `feat/m15-01-per-task-execution-contract` (stacked on `feat/m13-ipc-02-control-plane` / PR #19)  
**Author**: Principal Systems Architect (Claude 1)  
**Recipient**: Principal Frontend / Extension Architect (Claude 2)  
**Status**: OFFICIAL CONTRACT RELEASE & CLIENT INTEGRATION SPECIFICATION  

---

## 1. Overview for Claude 2

Milestone M15-01 extends the Agent Studios IPC contract to empower frontend clients (Mission Control, ConnectedClient, Code-OSS extension) to dynamically query backend execution readiness and submit coding tasks with specific execution configurations.

### Key Capabilities Enabled for Claude 2:
1. **Query Execution Readiness**: Call `get_execution_readiness()` on daemon connect to truthfully populate UI runtime selectors, available model lists, and authorized workspace dropdowns without storing or exposing secrets.
2. **Per-Task Execution Routing**: When submitting tasks via `submit_task()` / `BatchTaskSpec`, include optional `TaskExecutionSpec` to route tasks to specific git repositories, runtime preferences, and models.
3. **Fail-Closed Diagnostics**: Clear error diagnostics in `RunRecord` if a selected repository is unauthorized, non-git, or if an unknown model/runtime is requested.
4. **Zero Breaking Changes**: If `execution` is omitted or set to `null`/`undefined`, legacy M13 behavior is preserved.

---

## 2. Wire Protocol Specification

### 2.1 Querying Execution Readiness
- **Command**:
  ```json
  {
    "type": "get_execution_readiness"
  }
  ```
- **Response**:
  ```json
  {
    "type": "execution_readiness",
    "readiness": {
      "is_ready": true,
      "available_runtimes": [
        {
          "instance_id": "rt-inst-c0a80101-0000-0000-0000-000000000001",
          "implementation_id": "opencode",
          "display_name": "OpenCode AI Runtime",
          "is_available": true,
          "capabilities": {
            "supports_streaming_events": true,
            "supports_cancellation": true,
            "supports_approvals": true
          }
        }
      ],
      "available_models": [
        {
          "provider_instance_id": "c0a80101-0000-0000-0000-000000000002",
          "provider_definition_id": "anthropic",
          "model_id": "claude-3-7-sonnet",
          "display_name": "Claude 3.7 Sonnet",
          "context_window_tokens": 200000,
          "supports_streaming": true
        }
      ],
      "authorized_workspaces": [
        "d:/Projects/my-app"
      ],
      "defaults": {
        "default_repo_path": "d:/Projects/my-app",
        "default_provider_instance_id": "c0a80101-0000-0000-0000-000000000002",
        "default_model_id": "claude-3-7-sonnet",
        "configured_instance_id": "rt-inst-c0a80101-0000-0000-0000-000000000001"
      },
      "blocking_reasons": []
    }
  }
  ```

### 2.2 Submitting Tasks with `TaskExecutionSpec`
In `submit_task` command (`spec` object):
```json
{
  "type": "submit_task",
  "studio_id": "c0a80101-0000-0000-0000-000000000001",
  "spec": {
    "key": "task-auth-fix",
    "title": "Fix token validation in auth middleware",
    "description": "Ensure token expiry uses <= not <.",
    "assigned_agent_id": null,
    "parent_task_key": null,
    "parent_task_id": null,
    "dependency_keys": [],
    "dependency_task_ids": [],
    "dependency_output_policy": "task_success",
    "execution": {
      "initial_prompt": "Fix token validation in auth middleware without touching unrelated code.",
      "repository_path": "d:/Projects/my-app",
      "runtime_instance_id": null,
      "runtime_preference": "opencode",
      "provider_instance_id": "c0a80101-0000-0000-0000-000000000002",
      "model_id": "claude-3-7-sonnet"
    }
  }
}
```

---

## 3. Strict Boundary Rules for Both Teams

1. **Claude 1 Boundary**:
   - Claude 1 works exclusively on `feat/m15-01-per-task-execution-contract` targeting base `feat/m13-ipc-02-control-plane`.
   - Claude 1 never commits to or touches Claude 2's branches (`feat/m14-ipc-03-connected-client` in PR #23, `feat/m14-mission-control-connected-integration` in PR #25).
   - Base PR #19 remains untouched in Draft.
   - Subtree `codex-rs/` is strictly 100% untouched.

2. **Claude 2 Boundary**:
   - Claude 2 implements the frontend bindings in TypeScript using the wire schemas above.
   - Claude 2 consumes the read-only `get_execution_readiness` API to populate Mission Control and workbench controls.
