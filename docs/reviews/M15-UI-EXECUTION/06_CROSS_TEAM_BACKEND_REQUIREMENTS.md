# M15 — Cross-Team Backend Contract Requirements (For Claude 1)

**Document Reference**: `docs/reviews/M15-UI-EXECUTION/06_CROSS_TEAM_BACKEND_REQUIREMENTS.md`  
**Milestone**: M14 Final Integration Freeze & M15 UI Execution Preparation  
**Target Recipient**: Backend Engineer / Claude 1 (`agent-studios-rs`)  
**Author**: Principal VS Code Extension Engineer  
**Date**: 2026-10-30  
**Status**: SUBMITTED BACKEND REQUIREMENTS  

---

## 1. Executive Summary

Based on the architectural audit of `agent-studios-rs` (pinned commit `ce0396c1f165760ea271110eaf9bfd6dc84a67a7`), the VS Code UI extension has implemented client-side adapters and capability gates for all existing functionality.

To unlock full interactive multi-agent execution, the following 5 backend pull requests are requested from the backend team:

---

## 2. Prioritized Backend Work Items

### PR-M15-01: Per-Task Model and Provider Selection
- **Problem**: `BatchTaskSpec` currently lacks `model_ref` and `provider_instance_id`. All tasks inherit daemon-level default model (`AGENT_STUDIOS_DEFAULT_MODEL`).
- **Required Changes in `agent-studios-protocol::task`**:
  ```rust
  pub struct BatchTaskSpec {
      pub key: String,
      pub title: String,
      pub description: String,
      pub assigned_agent_id: Option<AgentId>,
      pub model_ref: Option<String>,                   // <-- NEW
      pub provider_instance_id: Option<String>,       // <-- NEW
      // ... existing fields ...
  }
  ```
- **Required Changes in `agent-studios-ipc-transport::connection`**:
  In `execute_task_lifecycle`, check `spec.model_ref` before falling back to `server_state.default_model_ref`.

---

### PR-M15-02: Dynamic Runtime Adapter Registry
- **Problem**: `IpcServerBuilder` configures a singleton runtime adapter (`server_state.runtime: Option<Arc<dyn AgentRuntime>>`). The daemon cannot execute tasks across different runtimes (e.g. `opencode-runtime` vs. `claude-code-runtime`) dynamically.
- **Required Changes in `agent-studios-ipc-transport::server`**:
  Replace singleton with a registry:
  ```rust
  pub struct ServerState {
      pub runtimes: HashMap<String, Arc<dyn AgentRuntime>>, // <-- NEW
      // ...
  }
  ```

---

### PR-M15-03: Artifact Diff Capture over IPC
- **Problem**: `ArtifactRecord` exists in `protocol`, but `capture_changes` is not wired into the task execution lifecycle in `connection.rs`, and no `IpcCommand::GetTaskDiff` exists.
- **Required Changes**:
  1. Add `IpcCommand::GetTaskDiff { task_id: TaskId }` returning `IpcResponse::TaskDiff { diff: String }`.
  2. In `connection.rs` after task success, execute `workspace.capture_changes()` and store the resulting diff artifact.

---

### PR-M15-04: Conversational Follow-Up Prompts (`SendPrompt`)
- **Problem**: `IpcCommand` only provides `SubmitTask` (initial task submission). Multi-turn follow-up prompts within an active task session lack an IPC command.
- **Required Changes**:
  Add `IpcCommand::SendPrompt { task_id: TaskId, message: String }` that dispatches to the active `AgentRuntime` instance.

---

### PR-M15-05: Control Plane Persistent Storage (SQLite)
- **Problem**: `agent-studios-control-plane` exclusively uses `InMemoryStore`. Restarting the daemon permanently discards all tasks, runs, and session records.
- **Required Changes**:
  Provide an optional SQLite storage engine implementing `ControlPlaneStore` with WAL mode enabled.
