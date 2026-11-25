//! Phase M18 Concurrency Stress, Long-Running Session Profiling, and Crash Recovery Tests
//!
//! Validates:
//! 1. 12 simultaneous mutating workers contending for tasks, runs, and worktrees on SQLite WAL.
//! 2. 1,000+ conversation turns and 100,000+ event envelopes long-running session profiling.
//! 3. Abnormal daemon crash recovery and the `Blocked != Cancelled` state invariant.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Instant;

use chrono::Utc;
use tempfile::TempDir;

use agent_studios_control_plane::{
    AnyEventStore, ControlPlane, DurabilityMode, EventStore, SqliteEventStore, SystemClock,
};
use agent_studios_protocol::agent::{AgentKind, AgentState};
use agent_studios_protocol::approval::{ApprovalKind, ApprovalState};
use agent_studios_protocol::artifact::ArtifactKind;
use agent_studios_protocol::event::{ControlPlaneEvent, EventEnvelope};
use agent_studios_protocol::id::{RunId, TaskId, WorktreeId};
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::TaskState;
use agent_studios_protocol::worktree::WorktreeState;

#[test]
fn test_m18_12_concurrent_mutating_workers_contention() {
    const WORKER_COUNT: usize = 12;
    const TASKS_PER_WORKER: usize = 5;
    const TOTAL_ITEMS: usize = WORKER_COUNT * TASKS_PER_WORKER;

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("m18_contention_wal.db");

    let sqlite_store = SqliteEventStore::open_with_durability(&db_path, DurabilityMode::Normal)
        .expect("Failed to open SQLite store in WAL mode");
    let mut initial_cp = ControlPlane::new(SystemClock, AnyEventStore::Sqlite(sqlite_store));

    let studio = initial_cp
        .create_studio("M18 12-Worker Contention Studio")
        .expect("Failed to create studio");
    let studio_id = studio.id;

    let cp = Arc::new(Mutex::new(initial_cp));
    let barrier = Arc::new(Barrier::new(WORKER_COUNT));

    let mut handles = Vec::with_capacity(WORKER_COUNT);

    for worker_idx in 0..WORKER_COUNT {
        let cp_clone = Arc::clone(&cp);
        let barrier_clone = Arc::clone(&barrier);
        let worker_root = temp_dir.path().join(format!("worker_{worker_idx}"));

        handles.push(thread::spawn(
            move || -> Vec<(TaskId, RunId, WorktreeId, bool)> {
                // Synchronize all 12 workers to maximize lock and SQLite single-writer contention
                barrier_clone.wait();

                let agent_id = {
                    let mut guard = cp_clone.lock().expect("ControlPlane mutex poisoned");
                    let agent = guard
                        .register_agent(
                            studio_id,
                            format!("worker-agent-{worker_idx}"),
                            AgentKind::External,
                            Some(format!("worker_role_{worker_idx}")),
                        )
                        .expect("Failed to register worker agent");
                    guard
                        .update_agent_state(agent.id, AgentState::Starting)
                        .expect("Failed to start worker agent");
                    guard
                        .update_agent_state(agent.id, AgentState::Idle)
                        .expect("Failed to idle worker agent");
                    guard
                        .update_agent_state(agent.id, AgentState::Busy)
                        .expect("Failed to mark worker agent busy");
                    agent.id
                };

                let mut completed_records = Vec::with_capacity(TASKS_PER_WORKER);

                for iter_idx in 0..TASKS_PER_WORKER {
                    let should_succeed = (worker_idx + iter_idx) % 4 != 0;

                    // Step 1: Create Task and Worktree under contention
                    let (task_id, worktree_id) = {
                        let mut guard = cp_clone.lock().expect("ControlPlane mutex poisoned");
                        let task = guard
                            .create_task(
                                studio_id,
                                format!("Task W{worker_idx}-I{iter_idx}"),
                                format!(
                                    "Concurrent workload item {iter_idx} by worker {worker_idx}"
                                ),
                                None,
                                Some(agent_id),
                                vec![],
                            )
                            .expect("Failed to create task");
                        assert_eq!(task.state, TaskState::Ready);

                        let wt_path = worker_root.join(format!("wt_{iter_idx}"));
                        let wt = guard
                            .create_worktree(
                                studio_id,
                                format!("wt-w{worker_idx}-i{iter_idx}"),
                                &worker_root,
                                &wt_path,
                                format!("base-commit-w{worker_idx}-i{iter_idx}"),
                            )
                            .expect("Failed to create worktree");
                        assert_eq!(wt.state, WorktreeState::Creating);

                        (task.id, wt.id)
                    };

                    thread::yield_now();

                    // Step 2: Create Run, Assign Worktree, Bind Thread, and Transition to Running/InUse
                    let run_id = {
                        let mut guard = cp_clone.lock().expect("ControlPlane mutex poisoned");
                        let run = guard
                            .create_run(task_id, agent_id)
                            .expect("Failed to create run");
                        assert_eq!(run.state, RunState::Queued);

                        guard
                            .transition_worktree_state(worktree_id, WorktreeState::Ready)
                            .expect("Failed to transition worktree to Ready");
                        guard
                            .assign_worktree(worktree_id, task_id, agent_id, Some(run.id))
                            .expect("Failed to assign worktree");
                        guard
                            .bind_worktree_thread(
                                worktree_id,
                                format!("thread-w{worker_idx}-i{iter_idx}"),
                            )
                            .expect("Failed to bind worktree thread");
                        guard
                            .transition_worktree_state(worktree_id, WorktreeState::InUse)
                            .expect("Failed to transition worktree to InUse");

                        guard
                            .transition_run_state(run.id, RunState::Starting)
                            .expect("Failed to transition run to Starting");
                        guard
                            .transition_run_state(run.id, RunState::Running)
                            .expect("Failed to transition run to Running");
                        guard
                            .transition_task_state(task_id, TaskState::Running)
                            .expect("Failed to transition task to Running");

                        run.id
                    };

                    thread::yield_now();

                    // Step 3: Emit tool, chunk, and budget telemetry events
                    {
                        let mut guard = cp_clone.lock().expect("ControlPlane mutex poisoned");
                        let now = Utc::now();
                        let call_id = format!("call-w{worker_idx}-i{iter_idx}");
                        guard
                            .record_tool_started(
                                studio_id,
                                agent_id,
                                Some(task_id),
                                Some(run_id),
                                "apply_patch",
                                &call_id,
                                now,
                            )
                            .expect("Failed to record tool started");
                        guard
                            .record_assistant_chunk(
                                studio_id,
                                agent_id,
                                Some(task_id),
                                Some(run_id),
                                format!("Worker {worker_idx} executing step {iter_idx}"),
                                false,
                            )
                            .expect("Failed to record assistant chunk");
                        guard
                            .record_tool_completed(
                                studio_id,
                                agent_id,
                                Some(task_id),
                                Some(run_id),
                                "apply_patch",
                                &call_id,
                                12,
                                now,
                            )
                            .expect("Failed to record tool completed");
                        guard
                            .record_budget_usage_updated(
                                studio_id,
                                agent_id,
                                Some(run_id),
                                (iter_idx + 1) as u32,
                                1,
                                2,
                                0,
                            )
                            .expect("Failed to record budget usage");
                    }

                    thread::yield_now();

                    // Step 4: Finalize Worktree, Run, and Task deterministically
                    {
                        let mut guard = cp_clone.lock().expect("ControlPlane mutex poisoned");
                        if should_succeed {
                            let patch = guard
                                .register_artifact(
                                    studio_id,
                                    task_id,
                                    agent_id,
                                    ArtifactKind::Patch,
                                    format!("patch-w{worker_idx}-i{iter_idx}.diff"),
                                    Some(format!("sha256:w{worker_idx}i{iter_idx}")),
                                    format!("artifacts/w{worker_idx}i{iter_idx}.diff"),
                                )
                                .expect("Failed to register patch artifact");

                            guard
                                .record_worktree_change_captured(
                                    worktree_id,
                                    Some(run_id),
                                    format!("base-commit-w{worker_idx}-i{iter_idx}"),
                                    Some(format!("head-commit-w{worker_idx}-i{iter_idx}")),
                                    patch.id,
                                    None,
                                    3,
                                )
                                .expect("Failed to capture worktree change");

                            guard
                                .transition_run_state(run_id, RunState::Succeeded)
                                .expect("Failed to transition run to Succeeded");
                            guard
                                .record_run_outcome(run_id, "completed", None)
                                .expect("Failed to record run outcome");
                            guard
                                .transition_task_state(task_id, TaskState::Succeeded)
                                .expect("Failed to transition task to Succeeded");
                            guard
                                .complete_worktree_removal(
                                    worktree_id,
                                    Some("clean completion".to_string()),
                                )
                                .expect("Failed to complete worktree removal");
                        } else {
                            guard
                                .record_worktree_retained(
                                    worktree_id,
                                    Some("retained for failure inspection".to_string()),
                                )
                                .expect("Failed to retain worktree");
                            guard
                                .transition_run_state(run_id, RunState::Failed)
                                .expect("Failed to transition run to Failed");
                            guard
                                .record_run_outcome(
                                    run_id,
                                    "execution_error",
                                    Some("Simulated task failure".to_string()),
                                )
                                .expect("Failed to record failed run outcome");
                            guard
                                .transition_task_state(task_id, TaskState::Failed)
                                .expect("Failed to transition task to Failed");
                        }
                    }

                    completed_records.push((task_id, run_id, worktree_id, should_succeed));
                }

                completed_records
            },
        ));
    }

    let mut all_outcomes = Vec::with_capacity(TOTAL_ITEMS);
    for handle in handles {
        let worker_outcomes = handle.join().expect("Worker thread panicked");
        assert_eq!(worker_outcomes.len(), TASKS_PER_WORKER);
        all_outcomes.extend(worker_outcomes);
    }
    assert_eq!(all_outcomes.len(), TOTAL_ITEMS);

    // Verify in-memory state integrity and deterministic transitions
    let guard = cp.lock().expect("ControlPlane mutex poisoned");
    assert_eq!(guard.state().agents.len(), WORKER_COUNT);
    assert_eq!(guard.state().runs.len(), TOTAL_ITEMS);
    assert_eq!(guard.state().worktrees.len(), TOTAL_ITEMS);
    assert_eq!(guard.all_tasks().count(), TOTAL_ITEMS);

    for (task_id, run_id, worktree_id, should_succeed) in &all_outcomes {
        let task = guard.get_task(*task_id).expect("Task missing from state");
        let run = guard.get_run(*run_id).expect("Run missing from state");
        let wt = guard
            .get_worktree(*worktree_id)
            .expect("Worktree missing from state");

        assert_eq!(wt.assigned_task_id, Some(*task_id));
        assert_eq!(wt.assigned_run_id, Some(*run_id));
        assert!(wt.bound_thread_id.is_some());
        assert!(wt.released_at.is_some());

        if *should_succeed {
            assert_eq!(task.state, TaskState::Succeeded);
            assert_eq!(run.state, RunState::Succeeded);
            assert_eq!(wt.state, WorktreeState::Removed);
            assert!(!wt.retained);
            assert!(wt.patch_artifact_id.is_some());
        } else {
            assert_eq!(task.state, TaskState::Failed);
            assert_eq!(run.state, RunState::Failed);
            assert_eq!(wt.state, WorktreeState::Retained);
            assert!(wt.retained);
        }
    }

    // Verify single-writer serialization: monotonic sequences without gaps or duplicates
    let persisted_events = guard
        .events_for_studio(studio_id, 1)
        .expect("Failed to query studio events");
    let total_event_count = guard.event_count().expect("Failed to query event count");
    assert_eq!(persisted_events.len(), total_event_count);

    let mut seen_ids = HashSet::with_capacity(total_event_count);
    for (idx, env) in persisted_events.iter().enumerate() {
        let expected_seq = (idx + 1) as u64;
        assert_eq!(
            env.sequence, expected_seq,
            "Sequence gap or out-of-order event at index {idx}"
        );
        assert_eq!(env.studio_id, studio_id);
        assert!(
            seen_ids.insert(env.event_id),
            "Duplicate EventId detected: {}",
            env.event_id
        );
    }

    // Reopen SQLite store from disk and verify 100% deterministic state reconstruction
    let reopened_store = SqliteEventStore::open(&db_path).expect("Failed to reopen SQLite store");
    let restored_cp =
        ControlPlane::restore_from_store(SystemClock, AnyEventStore::Sqlite(reopened_store))
            .expect("Failed to restore ControlPlane from SQLite WAL store");
    assert_eq!(restored_cp.state(), guard.state());
}

#[test]
fn test_m18_1000_turns_and_100k_event_envelopes_profiling() {
    const TOTAL_TURNS: u32 = 1_000;
    const TARGET_TOTAL_EVENTS: u64 = 100_000;
    const SQLITE_BATCH_SIZE: usize = 10_000;

    let start_instant = Instant::now();

    // 1. Setup ControlPlane and execute 1,000+ conversation turns through the engine
    let mut cp = ControlPlane::new_in_memory();
    let studio = cp
        .create_studio("M18 1000-Turn & 100k-Event Profiling Studio")
        .expect("Failed to create studio");
    let studio_id = studio.id;

    let agent = cp
        .register_agent(
            studio_id,
            "long-session-agent",
            AgentKind::External,
            Some("lead_architect".to_string()),
        )
        .expect("Failed to register agent");
    let agent_id = agent.id;

    cp.update_agent_state(agent_id, AgentState::Starting)
        .expect("Failed to start agent");
    cp.update_agent_state(agent_id, AgentState::Idle)
        .expect("Failed to transition agent to Idle");
    cp.update_agent_state(agent_id, AgentState::Busy)
        .expect("Failed to transition agent to Busy");

    let task = cp
        .create_task(
            studio_id,
            "1000-Turn Long-Running Session Task",
            "Profile sustained multi-turn conversation and 100k event envelopes",
            None,
            Some(agent_id),
            vec![],
        )
        .expect("Failed to create task");
    let task_id = task.id;

    let run = cp
        .create_run(task_id, agent_id)
        .expect("Failed to create run");
    let run_id = run.id;

    cp.transition_run_state(run_id, RunState::Starting)
        .expect("Failed to start run");
    cp.transition_run_state(run_id, RunState::Running)
        .expect("Failed to transition run to Running");
    cp.transition_task_state(task_id, TaskState::Running)
        .expect("Failed to transition task to Running");

    // Record 1,000 complete conversation turns via ControlPlane API
    for turn in 1..=TOTAL_TURNS {
        cp.record_assistant_chunk(
            studio_id,
            agent_id,
            Some(task_id),
            Some(run_id),
            format!("Turn {turn} assistant reasoning and response chunk"),
            turn % 2 == 0,
        )
        .expect("Failed to record turn chunk");

        cp.record_budget_usage_updated(
            studio_id,
            agent_id,
            Some(run_id),
            turn,
            turn * 2,
            u64::from(turn),
            0,
        )
        .expect("Failed to record turn budget update");
    }

    let engine_events = cp.drain_committed_events();
    let current_seq = u64::try_from(engine_events.len()).expect("sequence fit u64");
    assert!(
        current_seq > u64::from(TOTAL_TURNS) * 2,
        "Expected at least 2,000 events from 1,000 turns"
    );

    // 2. Generate the remaining stream of high-frequency telemetry envelopes up to 100,000 events
    let now = Utc::now();
    let mut all_envelopes = Vec::with_capacity(TARGET_TOTAL_EVENTS as usize);
    all_envelopes.extend(engine_events);

    for seq in (current_seq + 1)..=(TARGET_TOTAL_EVENTS - 2) {
        let event = if seq % 4 == 0 {
            let turn_num = TOTAL_TURNS + u32::try_from(seq / 4).unwrap_or(0);
            ControlPlaneEvent::BudgetUsageUpdated {
                agent_id,
                run_id: Some(run_id),
                turns_used: turn_num,
                tool_calls_used: turn_num,
                wall_clock_secs: seq / 10,
                child_agents_used: 0,
            }
        } else {
            ControlPlaneEvent::AssistantMessageChunk {
                agent_id,
                task_id: Some(task_id),
                run_id: Some(run_id),
                content: format!("stream-delta-{seq}"),
                is_thought: false,
            }
        };
        all_envelopes.push(EventEnvelope::new(studio_id, seq, now, event));
    }

    // Finalize run and task at sequences 99,999 and 100,000
    all_envelopes.push(EventEnvelope::new(
        studio_id,
        TARGET_TOTAL_EVENTS - 1,
        now,
        ControlPlaneEvent::RunStateChanged {
            run_id,
            previous_state: RunState::Running,
            new_state: RunState::Succeeded,
        },
    ));
    all_envelopes.push(EventEnvelope::new(
        studio_id,
        TARGET_TOTAL_EVENTS,
        now,
        ControlPlaneEvent::TaskStateChanged {
            task_id,
            previous_state: TaskState::Running,
            new_state: TaskState::Succeeded,
        },
    ));

    assert_eq!(all_envelopes.len(), TARGET_TOTAL_EVENTS as usize);

    // 3. Replay all 100,000 envelopes through ControlPlane::replay_events to verify domain state machine
    let replayed_cp = ControlPlane::replay_events(
        &all_envelopes,
        SystemClock,
        agent_studios_control_plane::InMemoryStore::new(),
    )
    .expect("Failed to replay 100,000 event envelopes through ControlPlane");

    assert_eq!(
        replayed_cp.event_count().expect("event_count failed"),
        TARGET_TOTAL_EVENTS as usize
    );
    assert_eq!(
        replayed_cp.get_task(task_id).expect("task missing").state,
        TaskState::Succeeded
    );
    assert_eq!(
        replayed_cp.get_run(run_id).expect("run missing").state,
        RunState::Succeeded
    );

    // 4. Persist the 1,000+ turn prefix batch (10,000 envelopes) into SqliteEventStore
    // to verify durable SQLite batch persistence and indexed SQL pagination within bounded time
    let mut sqlite_store = SqliteEventStore::in_memory_with_durability(DurabilityMode::Normal)
        .expect("Failed to create SQLite event store");

    let sqlite_batch = &all_envelopes[..SQLITE_BATCH_SIZE];
    sqlite_store
        .append_batch(sqlite_batch)
        .expect("Failed to append 10,000-event batch to SQLite event store");

    assert_eq!(
        sqlite_store
            .event_count()
            .expect("SQLite event_count failed"),
        SQLITE_BATCH_SIZE
    );
    assert_eq!(
        sqlite_store
            .latest_sequence(studio_id)
            .expect("SQLite latest_sequence failed"),
        SQLITE_BATCH_SIZE as u64
    );

    // 5. Verify monotonic sequence continuity (1..=100_000) and zero sequence gaps across all 100k events
    let full_studio_events = replayed_cp
        .events_for_studio(studio_id, 1)
        .expect("Failed to query full studio events");
    assert_eq!(full_studio_events.len(), TARGET_TOTAL_EVENTS as usize);

    let mut recorded_turns = 0usize;
    for (idx, env) in full_studio_events.iter().enumerate() {
        let expected_seq = (idx + 1) as u64;
        assert_eq!(env.sequence, expected_seq, "Sequence gap at {expected_seq}");
        if matches!(env.event, ControlPlaneEvent::BudgetUsageUpdated { .. }) {
            recorded_turns += 1;
        }
    }
    assert!(
        recorded_turns >= TOTAL_TURNS as usize,
        "Expected at least 1,000 recorded turn updates, found {recorded_turns}"
    );

    // 6. Verify accurate events_for_studio(studio_id, after_sequence) backfill and pagination
    let query_start = Instant::now();
    let backfill_from_99501 = replayed_cp
        .events_for_studio(studio_id, 99_501)
        .expect("Failed to query ControlPlane events_for_studio(99_501)");
    let sqlite_backfill_from_9501 = sqlite_store
        .events_for_studio(studio_id, 9_501)
        .expect("Failed to query SQLite events_for_studio(9_501)");
    let query_elapsed = query_start.elapsed();

    assert_eq!(backfill_from_99501.len(), 500);
    assert_eq!(backfill_from_99501.first().unwrap().sequence, 99_501);
    assert_eq!(
        backfill_from_99501.last().unwrap().sequence,
        TARGET_TOTAL_EVENTS
    );

    assert_eq!(sqlite_backfill_from_9501.len(), 500);
    assert_eq!(sqlite_backfill_from_9501.first().unwrap().sequence, 9_501);
    assert_eq!(
        sqlite_backfill_from_9501.last().unwrap().sequence,
        SQLITE_BATCH_SIZE as u64
    );

    let mid_slice = replayed_cp
        .events_slice(50_000, 250)
        .expect("Failed to query ControlPlane events_slice");
    assert_eq!(mid_slice.len(), 250);
    assert_eq!(mid_slice.first().unwrap().sequence, 50_001);
    assert_eq!(mid_slice.last().unwrap().sequence, 50_250);

    let tail_events = replayed_cp
        .events_tail(100)
        .expect("Failed to query ControlPlane events_tail");
    assert_eq!(tail_events.len(), 100);
    assert_eq!(tail_events.first().unwrap().sequence, 99_901);
    assert_eq!(tail_events.last().unwrap().sequence, TARGET_TOTAL_EVENTS);

    let sqlite_tail = sqlite_store
        .events_tail(100)
        .expect("Failed to query SQLite events_tail");
    assert_eq!(sqlite_tail.len(), 100);
    assert_eq!(sqlite_tail.first().unwrap().sequence, 9_901);
    assert_eq!(
        sqlite_tail.last().unwrap().sequence,
        SQLITE_BATCH_SIZE as u64
    );

    // Bounded latency assertions
    assert!(
        query_elapsed.as_secs() < 5,
        "Backfill query latency exceeded bound: {query_elapsed:?}"
    );
    assert!(
        start_instant.elapsed().as_secs() < 15,
        "100k event profiling test exceeded time budget: {:?}",
        start_instant.elapsed()
    );
}

#[test]
fn test_m18_abnormal_daemon_crash_recovery_and_blocked_not_cancelled() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("m18_crash_recovery_wal.db");
    let existing_wt_root = temp_dir.path().join("worktrees").join("wt_existing");
    let missing_wt_root = temp_dir.path().join("worktrees").join("wt_missing");
    std::fs::create_dir_all(&existing_wt_root).expect("Failed to create existing worktree dir");

    let studio_id;
    let agent_id;
    let running_task_id;
    let blocked_task_id;
    let ready_task_id;
    let running_run_id;
    let pending_approval_id;
    let existing_wt_id;
    let missing_wt_id;
    let pre_crash_sequence;

    // Phase 1: Populate active Running, Blocked, and Ready tasks with runs and worktrees, then drop abruptly
    {
        let store = SqliteEventStore::open_with_durability(&db_path, DurabilityMode::Full)
            .expect("Failed to open SQLite WAL database");
        let mut cp = ControlPlane::new(SystemClock, AnyEventStore::Sqlite(store));

        let studio = cp
            .create_studio("M18 Crash Recovery Studio")
            .expect("Failed to create studio");
        studio_id = studio.id;

        let agent = cp
            .register_agent(
                studio_id,
                "crash-test-agent",
                AgentKind::External,
                Some("executor".to_string()),
            )
            .expect("Failed to register agent");
        agent_id = agent.id;

        // 1. Running task with active run, pending approval, and two worktrees (one on disk, one missing)
        let running_task = cp
            .create_task(
                studio_id,
                "In-Flight Running Task",
                "Active task interrupted by hard daemon crash",
                None,
                Some(agent_id),
                vec![],
            )
            .expect("Failed to create running task");
        running_task_id = running_task.id;
        assert_eq!(running_task.state, TaskState::Ready);

        // 2. Blocked task depending on running_task (must start and remain Blocked, NOT Cancelled!)
        let blocked_task = cp
            .create_task(
                studio_id,
                "Downstream Blocked Task",
                "Blocked waiting on In-Flight Running Task",
                Some(running_task_id),
                Some(agent_id),
                vec![running_task_id],
            )
            .expect("Failed to create blocked task");
        blocked_task_id = blocked_task.id;
        assert_eq!(blocked_task.state, TaskState::Blocked);

        // 3. Independent Ready task
        let ready_task = cp
            .create_task(
                studio_id,
                "Queued Ready Task",
                "Ready task not yet started before crash",
                None,
                Some(agent_id),
                vec![],
            )
            .expect("Failed to create ready task");
        ready_task_id = ready_task.id;
        assert_eq!(ready_task.state, TaskState::Ready);

        // Start Run and transition running_task to Running
        let run = cp
            .create_run(running_task_id, agent_id)
            .expect("Failed to create run");
        running_run_id = run.id;
        cp.transition_run_state(running_run_id, RunState::Starting)
            .expect("Failed to transition run to Starting");
        cp.transition_run_state(running_run_id, RunState::Running)
            .expect("Failed to transition run to Running");
        cp.transition_task_state(running_task_id, TaskState::Running)
            .expect("Failed to transition task to Running");

        // Create a pending approval on the running task
        let approval = cp
            .request_approval(
                studio_id,
                running_task_id,
                agent_id,
                ApprovalKind::CommandExecution,
                "Approve privileged test command",
            )
            .expect("Failed to request approval");
        pending_approval_id = approval.id;
        assert_eq!(approval.state, ApprovalState::Pending);

        // Create two active worktrees: one existing on disk, one whose directory vanishes before recovery
        let wt1 = cp
            .create_worktree(
                studio_id,
                "wt-existing",
                temp_dir.path(),
                &existing_wt_root,
                "commit-abc111",
            )
            .expect("Failed to create existing worktree");
        existing_wt_id = wt1.id;
        cp.transition_worktree_state(existing_wt_id, WorktreeState::Ready)
            .expect("Failed to transition existing worktree to Ready");
        cp.assign_worktree(
            existing_wt_id,
            running_task_id,
            agent_id,
            Some(running_run_id),
        )
        .expect("Failed to assign existing worktree");
        cp.transition_worktree_state(existing_wt_id, WorktreeState::InUse)
            .expect("Failed to transition existing worktree to InUse");

        let wt2 = cp
            .create_worktree(
                studio_id,
                "wt-missing",
                temp_dir.path(),
                &missing_wt_root,
                "commit-def222",
            )
            .expect("Failed to create missing worktree");
        missing_wt_id = wt2.id;
        cp.transition_worktree_state(missing_wt_id, WorktreeState::Ready)
            .expect("Failed to transition missing worktree to Ready");
        cp.transition_worktree_state(missing_wt_id, WorktreeState::InUse)
            .expect("Failed to transition missing worktree to InUse");

        pre_crash_sequence = cp
            .store()
            .latest_sequence(studio_id)
            .expect("Failed to read pre-crash latest sequence");
        assert!(pre_crash_sequence >= 12);

        // Hard daemon crash simulated by dropping ControlPlane and SqliteEventStore here without shutdown
    }

    // Phase 2: Reopen ControlPlane::restore_from_store on the same SQLite WAL database
    let reopened_store = SqliteEventStore::open_with_durability(&db_path, DurabilityMode::Full)
        .expect("Failed to reopen SQLite WAL store after crash");
    let mut restored_cp =
        ControlPlane::restore_from_store(SystemClock, AnyEventStore::Sqlite(reopened_store))
            .expect("Failed to restore ControlPlane from SQLite WAL store");

    // Verify pre-reconciliation restored states match exact pre-crash snapshot
    assert_eq!(
        restored_cp.get_task(running_task_id).unwrap().state,
        TaskState::Running
    );
    assert_eq!(
        restored_cp.get_task(blocked_task_id).unwrap().state,
        TaskState::Blocked
    );
    assert_eq!(
        restored_cp.get_task(ready_task_id).unwrap().state,
        TaskState::Ready
    );
    assert_eq!(
        restored_cp.get_run(running_run_id).unwrap().state,
        RunState::Running
    );

    // Phase 3: Run reconcile_interrupted_executions() and reconcile_missing_worktrees()
    let (interrupted_tasks, interrupted_runs) = restored_cp
        .reconcile_interrupted_executions()
        .expect("reconcile_interrupted_executions failed");
    assert_eq!(
        interrupted_tasks, 1,
        "Exactly 1 Running task must be reconciled"
    );
    assert_eq!(
        interrupted_runs, 1,
        "Exactly 1 Running run must be reconciled"
    );

    let (wt_reconciled, wt_missing) = restored_cp
        .reconcile_missing_worktrees()
        .expect("reconcile_missing_worktrees failed");
    assert_eq!(
        wt_reconciled, 1,
        "Existing worktree on disk must be counted as reconciled"
    );
    assert_eq!(
        wt_missing, 1,
        "Missing worktree directory must be transitioned to Failed"
    );

    // Phase 4: Verify Running -> Failed and Blocked != Cancelled invariant
    let running_task_after = restored_cp
        .get_task(running_task_id)
        .expect("Running task must exist");
    assert_eq!(
        running_task_after.state,
        TaskState::Failed,
        "Interrupted Running task must transition to Failed"
    );

    let running_run_after = restored_cp
        .get_run(running_run_id)
        .expect("Running run must exist");
    assert_eq!(
        running_run_after.state,
        RunState::Failed,
        "Interrupted Running run must transition to Failed"
    );

    let approval_after = restored_cp
        .get_approval(pending_approval_id)
        .expect("Pending approval must exist");
    assert_eq!(
        approval_after.state,
        ApprovalState::Cancelled,
        "Dangling pending approval on interrupted task must be auto-cancelled"
    );

    let blocked_task_after = restored_cp
        .get_task(blocked_task_id)
        .expect("Blocked task must exist");
    assert_eq!(
        blocked_task_after.state,
        TaskState::Blocked,
        "Blocked task must remain Blocked after daemon crash recovery"
    );
    assert_ne!(
        blocked_task_after.state,
        TaskState::Cancelled,
        "CRITICAL INVARIANT: Blocked != Cancelled"
    );

    let ready_task_after = restored_cp
        .get_task(ready_task_id)
        .expect("Ready task must exist");
    assert_eq!(
        ready_task_after.state,
        TaskState::Ready,
        "Ready task must remain Ready after crash recovery"
    );

    assert_eq!(
        restored_cp.get_worktree(existing_wt_id).unwrap().state,
        WorktreeState::InUse
    );
    assert_eq!(
        restored_cp.get_worktree(missing_wt_id).unwrap().state,
        WorktreeState::Failed
    );

    // Phase 5: Resume mutations post-recovery and verify sequence continuity without duplicate or lost events
    restored_cp
        .transition_task_state(ready_task_id, TaskState::Running)
        .expect("Failed to start ready task after recovery");
    restored_cp
        .transition_task_state(ready_task_id, TaskState::Succeeded)
        .expect("Failed to complete ready task after recovery");

    let all_events = restored_cp
        .events_for_studio(studio_id, 1)
        .expect("Failed to fetch studio events post-recovery");
    let post_recovery_sequence = restored_cp
        .store()
        .latest_sequence(studio_id)
        .expect("Failed to query latest sequence");

    assert!(post_recovery_sequence > pre_crash_sequence);
    assert_eq!(all_events.len(), post_recovery_sequence as usize);

    let mut seen_event_ids = HashSet::with_capacity(all_events.len());
    let mut classifications_by_run: HashMap<RunId, String> = HashMap::new();

    for (idx, envelope) in all_events.iter().enumerate() {
        let expected_seq = (idx + 1) as u64;
        assert_eq!(
            envelope.sequence, expected_seq,
            "Monotonic sequence continuity broken after crash recovery at index {idx}"
        );
        assert!(
            seen_event_ids.insert(envelope.event_id),
            "Duplicate event_id detected after crash recovery: {}",
            envelope.event_id
        );
        if let ControlPlaneEvent::RunOutcomeRecorded {
            run_id,
            classification,
            ..
        } = &envelope.event
        {
            classifications_by_run.insert(*run_id, classification.clone());
        }
    }

    assert_eq!(
        classifications_by_run
            .get(&running_run_id)
            .map(String::as_str),
        Some("daemon_interrupted"),
        "Interrupted run must record 'daemon_interrupted' outcome classification"
    );
}
