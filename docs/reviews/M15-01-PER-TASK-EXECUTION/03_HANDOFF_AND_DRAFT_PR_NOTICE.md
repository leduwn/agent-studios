# Claude 2 Cross-Team Handoff: M15-01 Real AI Coding Agent Execution Contract & Per-Task Routing

**Date**: 2026-10-31  
**Target Pull Requests**: PR #23 (`feat/m14-ipc-03-connected-client`) & PR #25 (`feat/m14-mission-control-connected-integration`)  
**Backend Branch**: `feat/m15-01-per-task-execution-contract` (stacked on `feat/m13-ipc-02-control-plane` / PR #19)  
**Associated Draft PR**: PR #26 (https://github.com/leduwn/agent-studios/pull/26)  
**Author**: Principal Systems Architect (Claude 1)  
**Recipient**: Principal Frontend / Extension Architect (Claude 2)  
**Status**: AUTHORITATIVE CONTRACT SPECIFICATION & BACKEND READINESS AUDIT  

---

## 1. Overview for Claude 2

Milestone M15-01 extends the Agent Studios IPC contract to empower frontend clients (Mission Control, ConnectedClient, Code-OSS extension) to dynamically query backend execution readiness and submit coding tasks with granular per-task execution configurations.

### Key Capabilities Enabled for Claude 2:
1. **Query Execution Readiness**: Call `get_execution_readiness()` on daemon connect to truthfully populate UI runtime selectors, available model lists, and authorized workspace dropdowns without exposing secrets or bearer tokens.
2. **Per-Task Execution Routing**: When submitting tasks via `submit_task()` / `BatchTaskSpec`, include optional `TaskExecutionSpec` under `task_spec.execution` to route tasks to specific git repositories, runtime preferences, and models.
3. **Fail-Closed Diagnostics**: Explicit error diagnostics in `RunRecord` if a selected repository is unauthorized, non-git, or if an unknown model/runtime is requested.
4. **Zero Breaking Changes**: If `execution` is omitted or set to `null`/`undefined`, legacy M13 behavior is preserved.

---

## 2. Authoritative Wire Protocol Specification

All IPC frames use length-prefixed JSON serialization (`u32` little-endian length prefix followed by JSON payload). The schemas below reflect the authoritative Serde representations validated by Rust golden fixture tests.

### 2.1 Querying Execution Readiness

- **Command**:
  ```json
  {
    "type": "get_execution_readiness"
  }
  ```

- **Response (`execution_readiness`)**:
  ```json
  {
    "type": "execution_readiness",
    "readiness": {
      "is_ready": true,
      "available_runtimes": [
        "opencode"
      ],
      "runtime_instances": [
        {
          "instance_id": "rt-inst-c0a80101-0000-0000-0000-000000000001",
          "implementation": "opencode",
          "healthy": true
        }
      ],
      "available_models": [
        {
          "provider_instance_id": "c0a80101-0000-0000-0000-000000000002",
          "model_id": "claude-3-7-sonnet",
          "display_name": "Claude 3.7 Sonnet"
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

#### Field Semantics & Types:
- `is_ready` (`boolean`): Indicates whether the daemon is ready to execute tasks with **default configuration**. Fails closed (`false`) if any default resource is invalid or missing.
- `available_runtimes` (`string[]`): Unique list of runtime implementation identifiers registered on the daemon (e.g. `["opencode"]`).
- `runtime_instances` (`object[]`): Discovered runtime instances.
  - `instance_id` (`string`): Unique identifier of the runtime instance.
  - `implementation` (`string`): Adapter implementation ID (e.g. `"opencode"`).
  - `healthy` (`boolean`): Whether the runtime instance is currently available and healthy.
- `available_models` (`object[]`): Usable models registered in the daemon provider catalog.
  - `provider_instance_id` (`string`): UUID of the configured provider instance.
  - `model_id` (`string`): Model identifier (e.g. `"claude-3-7-sonnet"`).
  - `display_name` (`string | null`): Human-readable model label.
- `authorized_workspaces` (`string[]`): List of canonicalized repository paths approved for task execution.
- `defaults` (`object`):
  - `default_repo_path` (`string | null`): Default repository path configured on daemon.
  - `default_provider_instance_id` (`string | null`): Default provider instance ID.
  - `default_model_id` (`string | null`): Default model ID.
  - `configured_instance_id` (`string | null`): Explicit runtime instance ID pinned on daemon, if any.
- `blocking_reasons` (`string[]`): Human-readable explanations why `is_ready` is `false`.

#### Readiness Semantics & Fail-Closed Rules:
`is_ready` evaluates to `false` and populates `blocking_reasons` if:
1. **No runtime adapter configured**: No runtime adapter is configured, or discovery failed.
2. **No healthy runtime instances**: Discovered runtime instances are all unhealthy/unavailable.
3. **Configured runtime instance unavailable**: If `configured_instance_id` is set on daemon but does not match any healthy discovered instance.
4. **No models available**: Provider catalog is empty.
5. **No default model with multiple candidates**: Multiple models are registered in catalog but no default model is configured on the daemon (`with_default_model_ref` or `AGENT_STUDIOS_DEFAULT_MODEL` not set).
6. **Configured default model missing**: Default model reference does not exist in catalog.
7. **Default repository invalid / missing**: Default repository path is unset, cannot be canonicalized, missing `.git` (directory or gitlink file), or fails workspace authorization.
8. **No authorized workspaces configured**: Daemon has no allowlisted workspaces.

> **Distinction Between Global Readiness and Per-Task Execution**:  
> `is_ready` reflects whether a task with omitted `execution` defaults can be run immediately. If `is_ready` is `false` solely due to default model or default repository configuration, a client can still submit a task with complete, explicit `execution` parameters (explicit authorized `repository_path`, valid `provider_instance_id`, and valid `model_id`), and that task will execute successfully.

---

### 2.2 Submitting Tasks with `TaskExecutionSpec`

> **CRITICAL WIRE CONTRACT**: The submit command parameter is named **`task_spec`** (NOT `spec`).

- **Command (`submit_task` with execution)**:
  ```json
  {
    "type": "submit_task",
    "studio_id": "c0a80101-0000-0000-0000-000000000001",
    "task_spec": {
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

- **Command Response (`tasks_submitted`)**:
  ```json
  {
    "type": "tasks_submitted",
    "task_ids": [
      "c0a80101-0000-0000-0000-000000000011"
    ]
  }
  ```

- **Legacy M13 Backward Compatible Command (`execution: null`)**:
  ```json
  {
    "type": "submit_task",
    "studio_id": "c0a80101-0000-0000-0000-000000000001",
    "task_spec": {
      "key": "task-legacy",
      "title": "Legacy Task",
      "description": "Executed with daemon defaults",
      "assigned_agent_id": null,
      "parent_task_key": null,
      "parent_task_id": null,
      "dependency_keys": [],
      "dependency_task_ids": [],
      "dependency_output_policy": "task_success",
      "execution": null
    }
  }
  ```

---

### 2.3 Run Record Representation

When querying runs (`get_run`, `list_runs`), the returned `RunRecord` contains the resolved `execution` spec:

- **Run with Execution Spec**:
  ```json
  {
    "type": "run",
    "run": {
      "id": "c0a80101-0000-0000-0000-000000000010",
      "task_id": "c0a80101-0000-0000-0000-000000000011",
      "agent_id": "c0a80101-0000-0000-0000-000000000012",
      "attempt": 1,
      "state": "running",
      "started_at": "2026-10-30T12:00:00Z",
      "finished_at": null,
      "execution": {
        "initial_prompt": "Fix token validation in auth middleware without touching unrelated code.",
        "repository_path": "d:/Projects/my-app",
        "runtime_instance_id": "rt-inst-c0a80101-0000-0000-0000-000000000001",
        "runtime_preference": "opencode",
        "provider_instance_id": "c0a80101-0000-0000-0000-000000000002",
        "model_id": "claude-3-7-sonnet"
      }
    }
  }
  ```

- **Run without Execution Spec (`execution: null`)**:
  ```json
  {
    "type": "run",
    "run": {
      "id": "c0a80101-0000-0000-0000-000000000010",
      "task_id": "c0a80101-0000-0000-0000-000000000011",
      "agent_id": "c0a80101-0000-0000-0000-000000000012",
      "attempt": 1,
      "state": "queued",
      "started_at": null,
      "finished_at": null,
      "execution": null
    }
  }
  ```

---

### 2.4 Error Responses

Any transport or validation failure returns standard `IpcResponse::Error`:
```json
{
  "type": "error",
  "message": "Repository path 'd:/Projects/unauthorized-repo' is not authorized on daemon"
}
```

---

## 3. Workspace Authorization Policies & Containment Rules

The daemon enforces strict containment and validation before any task execution begins:

1. **Containment Policy**:
   - **Hierarchical Containment (Default, `strict_workspace_roots: false`)**:
     A workspace path `canonical` is authorized if it equals an allowlisted root or resides within an allowlisted root (`canonical == root || canonical.starts_with(root)`). This permits approved nested submodules, monorepo packages, and git worktrees within approved parent roots.
   - **Strict Root Matching (`strict_workspace_roots: true`)**:
     Enforces exact match (`canonical == root`). Nested repositories are rejected unless explicitly added to the allowlist.

2. **Path Canonicalization & Reparse Resolution**:
   - Every input path is resolved using `dunce::canonicalize`.
   - On Windows, NTFS junction points, symlinks, directory reparse points, and `..` path traversals are fully resolved to target filesystem locations before authorization checks.
   - Any path resolving outside allowlisted workspace roots fails closed immediately.

3. **Git Repository Identity (`.git` Directory vs Regular File)**:
   - Validated via `.join(".git").exists()`.
   - Supports both:
     - Standard `.git` directories (standalone git repositories).
     - Standard `.git` regular files (gitlinks created by git submodules and `git worktree add`, containing `gitdir: <path>`).
   - Plain non-git directories are rejected and fail closed.

4. **Filesystem Normalization & Edge Cases**:
   - **Windows Case Normalization**: `dunce::canonicalize` normalizes drive letters and file path casing against disk casing.
   - **Deleted / Moved Paths**: Paths that do not exist at check time fail canonicalization and are rejected.

---

## 4. Clarification of Runtime Support & Boundaries

### Supported in M15-01:
- Per-task runtime instance selection (`runtime_instance_id`).
- Per-task runtime preference verification (`runtime_preference`, e.g. ensuring requested implementation matches registered daemon runtime).
- Per-task provider & model resolution (`provider_instance_id`, `model_id`).
- Per-task authorized repository routing (`repository_path`).

### Explicit Future Scope (Not in M15-01):
- **Dynamic Multi-Adapter Registry**: The daemon currently hosts a single configured agent runtime adapter (default: OpenCode). Dynamic runtime discovery across multiple distinct adapter plugins is scheduled for M15-03+.
- **Do Not Advertise Claude Code**: The frontend MUST NOT hardcode or advertise Claude Code as available unless the running daemon explicitly reports `"claude-code"` in `available_runtimes` / `runtime_instances`.

---

## 5. Strict Cross-Team Boundary Rules

1. **Claude 1 (Backend Systems)**:
   - Works strictly on `feat/m15-01-per-task-execution-contract` (Draft PR #26) targeting `feat/m13-ipc-02-control-plane` (PR #19).
   - Never commits to or modifies Claude 2's branches (`feat/m14-ipc-03-connected-client` in PR #23, `feat/m14-mission-control-connected-integration` in PR #25).
   - Subtree `codex-rs/` is strictly 100% untouched (0 diffs, 0 bytes modified).
   - Keeps PR #26 in Draft; no force pushes, no self-merging.

2. **Claude 2 (Frontend & Extensions)**:
   - Consumes the exact wire schemas in Section 2.
   - Implements TypeScript types matching `TaskExecutionSpec`, `ExecutionReadinessReport`, and `RunRecord`.
   - Uses `get_execution_readiness()` on daemon connect to populate workbench controls.
