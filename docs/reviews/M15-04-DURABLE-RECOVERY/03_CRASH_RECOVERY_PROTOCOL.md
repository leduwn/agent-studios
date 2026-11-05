# M15-04 Lifecycle & Recovery: Interrupted Execution Protocol

## 1. The Crash & Restart Problem

When the Agent Studios daemon (`agsd`) terminates abruptly (kill signal, OS shutdown, power loss, or panic), external processes, child threads, and in-flight operations stop execution immediately.
Upon daemon restart:
- Any task left in `TaskState::Running` is an orphan.
- Any run left in `RunState::Starting` or `RunState::Running` is interrupted mid-stream.
- Pending operator approvals for interrupted tasks are lingering.
- Associated worktrees may be dirty or in unknown physical states.

---

## 2. Invariants & Rules

1. **No Silent Resumption:** The system must NEVER automatically restart or resume an interrupted task or run without operator intent. Models cost tokens, and commands might have irreversible side effects.
2. **No Fabricated Success:** Under no circumstance may an interrupted run or task be marked `Succeeded`. Success requires complete, authoritative evidence and artifacts.
3. **Explicit Interruption Classification:** Interrupted runs must be marked `RunState::Failed` with domain event classification `"daemon_interrupted"`.
4. **Auto-Cancellation of Pending Approvals:** When an in-flight task transitions to `TaskState::Failed`, all pending approvals for that task must be marked `ApprovalState::Cancelled` within the same transaction.
5. **Durable Persistence of Recovery Transitions:** The transition events generated during recovery must be committed to the durable SQLite store so subsequent restarts or downstream clients receive an authoritative history.

---

## 3. Implementation: `reconcile_interrupted_executions`

The recovery logic runs immediately after `ControlPlane::restore_from_store` populates state:

```rust
pub fn reconcile_interrupted_executions(&mut self) -> Result<(usize, usize), ControlPlaneError> {
    // 1. Reconcile in-flight runs
    let in_flight_runs: Vec<(RunId, StudioId, TaskId, AgentId, RunState)> = self
        .state
        .runs
        .values()
        .filter(|r| r.state == RunState::Starting || r.state == RunState::Running)
        .map(|r| {
            let studio_id = self
                .state
                .task_graph
                .get_task(r.task_id)
                .map(|t| t.studio_id)
                .unwrap_or_else(|| {
                    self.state.studios.keys().next().copied().unwrap_or_default()
                });
            (r.id, studio_id, r.task_id, r.agent_id, r.state)
        })
        .collect();

    let mut interrupted_runs_count = 0;
    for (run_id, studio_id, task_id, agent_id, prev_state) in in_flight_runs {
        let candidate_events = vec![
            ControlPlaneEvent::RunStateChanged {
                run_id,
                previous_state: prev_state,
                new_state: RunState::Failed,
            },
            ControlPlaneEvent::RunOutcomeRecorded {
                run_id,
                task_id,
                agent_id,
                classification: "daemon_interrupted".to_string(),
                safe_error_summary: Some(
                    "daemon_interrupted: Daemon restarted during active execution".to_string(),
                ),
            },
        ];
        self.commit_transaction(studio_id, candidate_events)?;
        interrupted_runs_count += 1;
    }

    // 2. Reconcile in-flight tasks
    let in_flight_tasks: Vec<TaskId> = self
        .state
        .task_graph
        .all_tasks()
        .filter(|t| t.state == TaskState::Running)
        .map(|t| t.id)
        .collect();

    let mut interrupted_tasks_count = 0;
    for task_id in in_flight_tasks {
        self.transition_task_state(task_id, TaskState::Failed)?;
        interrupted_tasks_count += 1;
    }

    Ok((interrupted_tasks_count, interrupted_runs_count))
}
```

---

## 4. Verification

Integration tests in `test_crash_recovery_transitions_in_flight_tasks_and_runs` verify:
1. In-flight task is marked `TaskState::Failed`.
2. In-flight run is marked `RunState::Failed`.
3. Associated pending approval is marked `ApprovalState::Cancelled`.
4. Domain event `RunOutcomeRecorded` with `"daemon_interrupted"` is committed to the database.
5. New mutations can be dispatched normally without state corruption.
