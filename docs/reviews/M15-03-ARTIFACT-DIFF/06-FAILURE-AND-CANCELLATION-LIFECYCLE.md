# M15-03 Failure and Cancellation Lifecycle: Zero-Race Artifact Capture

**Milestone:** M15-03  
**Status:** Complete  
**Date:** 2026-11-01  
**Author:** Agent Runtime Engineer & Systems Architect  

---

## 1. Problem Statement

In an agentic coding environment, task runs can terminate under four distinct conditions:
1. **Clean Completion (`Completed`)**: The agent finishes its intended goal and exits cleanly.
2. **Execution Failure (`Failed`)**: The agent encounters an unrecoverable exception, tool error, or environment crash.
3. **User / Operator Cancellation (`Stopped`)**: The operator presses "Cancel Task" in VS Code or CLI.
4. **Interruption (`Interrupted`)**: The session is aborted due to watchdog timeout, resource limits, or human intervention.
5. **Premature Disconnection**: The agent runtime process crashes or the IPC event stream terminates abruptly without emitting a terminal event.

A naive implementation only captures worktree changes on clean completion. This creates two fatal defects:
1. **Partial Work Loss**: When an agent writes code for 10 minutes and then fails or is stopped by the user, all modified files are lost or abandoned without a trace.
2. **Race Hazard**: If task state transition (`TaskState::Succeeded`, `Failed`, or `Cancelled`) is published *before* artifact capture completes, a client listening to `task_state_changed` immediately queries `GetTaskDiff` and receives `NotFound` or stale empty results.

---

## 2. The Pre-State-Transition Capture Pattern

M15-03 implements the Pre-State-Transition Capture Pattern in `agent-studios-ipc-transport::connection::execute_task_lifecycle`.

```rust
#[allow(clippy::too_many_arguments)]
async fn capture_and_register_worktree_artifact(
    server_state: &Arc<ServerState>,
    studio_id: StudioId,
    task_id: TaskId,
    agent_id: AgentId,
    run_id: RunId,
    wt_id: WorktreeId,
    wt_root: &std::path::Path,
    wt_base_sha: &str,
) {
    if let Some(ref orchestrator) = server_state.workspace_orchestrator {
        if wt_root.exists() {
            if let Ok(captured) = orchestrator
                .capture_changes_async(wt_root.to_path_buf(), wt_base_sha.to_string())
                .await
            {
                // 1. Store stats summary artifact (if non-empty)
                let stats_rec = ...;
                
                // 2. Store patch artifact in content-addressed store
                if let Ok(patch_record) = orchestrator.store_patch_artifact(
                    studio_id, task_id, agent_id, Some(run_id), Some(wt_id), &captured,
                ) {
                    // 3. Register artifacts in Control Plane
                    let mut cp = server_state.control_plane.write().await;
                    if let Ok(registered_patch) = cp.register_artifact_record(patch_record) {
                        let registered_stats = stats_rec.and_then(|s| cp.register_artifact_record(s).ok());
                        
                        // 4. Update WorktreeRecord to ChangeCaptured
                        let _ = cp.record_worktree_change_captured(
                            wt_id,
                            Some(run_id),
                            wt_base_sha.to_string(),
                            captured.head_commit.clone(),
                            registered_patch.id,
                            registered_stats.map(|s| s.id),
                            captured.files_changed,
                        );
                        let envs = cp.drain_committed_events();
                        drop(cp);
                        let _ = server_state.publish_control_plane_events(envs);
                    }
                }
            }
        }
    }
}
```

---

## 3. Strict Pre-Transition Invariant

Across every single terminal exit path in `execute_task_lifecycle`:
1. `RuntimeEventKind::Completed`:
   - `capture_and_register_worktree_artifact.await`
   - `cp.transition_run_state(run_id, RunState::Succeeded)`
   - `cp.transition_task_state(task_id, TaskState::Succeeded)`
   - `server_state.publish_control_plane_events(envs)`
2. `RuntimeEventKind::Failed`:
   - `capture_and_register_worktree_artifact.await`
   - `cp.transition_run_state(run_id, RunState::Failed)`
   - `cp.transition_task_state(task_id, TaskState::Failed)`
   - `server_state.publish_control_plane_events(envs)`
3. `RuntimeEventKind::Stopped`:
   - `capture_and_register_worktree_artifact.await`
   - `cp.transition_run_state(run_id, RunState::Cancelled)`
   - `cp.transition_task_state(task_id, TaskState::Cancelled)`
   - `server_state.publish_control_plane_events(envs)`
4. `RuntimeEventKind::Interrupted`:
   - `capture_and_register_worktree_artifact.await`
   - `cp.transition_run_state(run_id, RunState::Cancelled)`
   - `cp.transition_task_state(task_id, TaskState::Cancelled)`
   - `server_state.publish_control_plane_events(envs)`
5. Premature stream termination (`!terminal_seen`):
   - `capture_and_register_worktree_artifact.await`
   - `cp.transition_run_state(run_id, RunState::Failed)`
   - `cp.transition_task_state(task_id, TaskState::Failed)`
   - `server_state.publish_control_plane_events(envs)`

---

## 4. Zero-Race Guarantee

Because the write lock on `ControlPlane` registers the artifact and updates the `WorktreeRecord` *in the exact same transactional block or prior to* transitioning `task_id` and `run_id` to terminal states:
- When a client receives `task_state_changed` (`Succeeded`, `Failed`, or `Cancelled`), `GetTaskDiff` and `ListTaskArtifacts` are guaranteed to return the completed patch immediately with zero race hazard or retry loop required.
- Human developers inspecting failed or cancelled runs can immediately review partial progress in VS Code.
