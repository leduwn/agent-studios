# Milestone M15 UI-01.2 Review: Authoritative Wire Contract Audit

**Milestone**: M15 UI-01.2 — Real Rust Execution Integration & Final Safety Gates  
**Area**: Rust IPC Wire Types, Serde Parity & Serialization Boundaries  
**Verdict**: APPROVED  

---

## 1. Executive Summary

Milestone M15 UI-01.2 establishes strict wire parity between the VS Code Extension Host TypeScript contracts and the authoritative Rust Serde protocol in PR #26 (`a5dbbfc5ebada254727db9a788c0ee40e3035ea8`). Synthetic, fabricated, and client-invented fields have been eliminated.

---

## 2. Wire Protocol Schema Parity

### 2.1 Execution Readiness Protocol

Rust Command: `IpcCommand::GetExecutionReadiness`  
Rust Response: `IpcResponse::ExecutionReadiness { report: RustExecutionReadinessReport }`

```typescript
export interface RustExecutionReadinessReport {
  is_ready: boolean;
  available_runtimes: string[];
  runtime_instances: Array<{
    instance_id: string;
    implementation: string;
    version?: string;
    status: string;
  }>;
  available_models: Array<{
    model_id: string;
    provider_id: string;
    display_name: string;
    context_window?: number;
  }>;
  authorized_workspaces: string[];
  defaults: {
    default_repo_path: string | null;
    default_provider_instance_id: string | null;
    default_model_id: string | null;
    configured_instance_id: string | null;
  };
  blocking_reasons: string[];
}
```

### 2.2 Task Execution Spec & Run Record

Rust Command: `IpcCommand::SubmitTask { studio_id, task_spec, execution }`  
Rust Record: `RustRunRecord { run_id, studio_id, state, execution?: RustTaskExecutionSpec | null, ... }`

```typescript
export interface RustTaskExecutionSpec {
  runtime: string;
  model: string;
  max_steps?: number;
  budget_tokens?: number;
}

export interface RustBatchTaskSpec {
  key?: string;
  title: string;
  description: string;
  prompt?: string;
  execution?: RustTaskExecutionSpec | null;
  state?: string;
  assigned_agent_id?: string;
  dependency_keys?: string[];
  dependency_task_ids?: string[];
}
```

### 2.3 Task Cancellation Wire Contract

Rust Command: `IpcCommand::CancelTask { studio_id, task_id, reason }`  
Rust Response: `IpcResponse::TaskCancelled { studio_id, task_id, previous_state }`

```typescript
export interface RustCancelTaskCommand {
  type: 'cancel_task';
  studio_id: string;
  task_id: string;
  reason?: string;
}
```

---

## 3. Wire Security Invariants

1. **Snake Case Serialization**: All wire payloads transmitted to daemon use Rust Serde `snake_case` field names (`studio_id`, `task_spec`, `runtime_preference`, `authorized_workspaces`, `default_repo_path`).
2. **Nullable Semantics**: Missing daemon values serialize to explicit `null`, not undefined or synthetic client defaults.
3. **No Fabricated Token Counts**: Metrics fields are absent or marked `isReported: false` until the daemon emits authoritative telemetry in event payloads.
