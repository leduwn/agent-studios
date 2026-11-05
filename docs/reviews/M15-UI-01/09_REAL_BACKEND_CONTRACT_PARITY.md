# Milestone M15 UI-01.1 Review: Real Backend Contract Parity

**Authoritative Backend PR**: PR #26 (`agent-studios-rs`, branch `feat/m15-01-per-task-execution-contract`)  
**Commit**: `49319b29d376972fa4079eba69f536a209c4ee28`  
**Reference Files**:
- `agent-studios-rs/src/ipc/readiness.rs`
- `agent-studios-rs/src/ipc/task.rs`
- `agent-studios-rs/src/ipc/framing.rs`
- `agent-studios-rs/src/ipc/connection.rs`
- `agent-studios-rs/tests/golden_fixture_tests.rs`

---

## 1. IPC Wire Protocol Alignment

The backend serialization format uses Serde tagged enum representations with `snake_case` naming: `#[serde(tag = "type", rename_all = "snake_case")]`.

### 1.1 IpcCommand::GetExecutionReadiness
- **Wire Command**:
  ```json
  { "type": "get_execution_readiness" }
  ```
- **Authoritative Daemon Response (`IpcResponse::ExecutionReadiness`)**:
  ```json
  {
    "type": "execution_readiness",
    "readiness": {
      "is_ready": true,
      "available_runtimes": ["claude_code"],
      "runtime_instances": [
        {
          "instance_id": "claude-code-primary",
          "implementation": "claude_code",
          "healthy": true
        }
      ],
      "available_models": [
        {
          "provider_instance_id": "anthropic-primary",
          "model_id": "claude-3-7-sonnet",
          "display_name": "Claude 3.7 Sonnet"
        }
      ],
      "authorized_workspaces": ["/workspace/agent-studios"],
      "defaults": {
        "default_repo_path": "/workspace/agent-studios",
        "default_provider_instance_id": "anthropic-primary",
        "default_model_id": "claude-3-7-sonnet",
        "configured_instance_id": "claude_code"
      },
      "blocking_reasons": []
    }
  }
  ```

### 1.2 TaskExecutionSpec in BatchTaskSpec
Inside `BatchTaskSpec.execution`, the backend expects per-task runtime and model execution properties:
- `initial_prompt`: string (nullable)
- `repository_path`: string (nullable)
- `runtime_instance_id`: string (nullable)
- `runtime_preference`: string (nullable)
- `provider_instance_id`: string (nullable)
- `model_id`: string (nullable)

TypeScript mapping in `RustIpcProtocolTypes.ts`:
```typescript
export interface RustTaskExecutionSpec {
  initial_prompt?: string | null;
  repository_path?: string | null;
  runtime_instance_id?: string | null;
  runtime_preference?: string | null;
  provider_instance_id?: string | null;
  model_id?: string | null;
}
```

### 1.3 Cancellation Command
- **Wire Command**:
  ```json
  {
    "type": "cancel_task",
    "studio_id": "<uuid>",
    "task_id": "<uuid>",
    "reason": "Cancelled by operator"
  }
  ```
- **Daemon Response**:
  ```json
  {
    "type": "task_cancelled",
    "studio_id": "<uuid>",
    "task_id": "<uuid>",
    "cancelled": true
  }
  ```

---

## 2. Bridge Interface Compliance
All three bridge implementations (`AgentStudiosBridge`, `DaemonConnectedBridge`, and `FixtureBridge`) implement:
- `getExecutionReadiness(): Promise<RustExecutionReadinessReport>`
- `submitTask(req: SubmitTaskRequest): Promise<SubmitTaskResult>`
- `cancelTask(taskId: string, reason?: string): Promise<CancelTaskResult>`
