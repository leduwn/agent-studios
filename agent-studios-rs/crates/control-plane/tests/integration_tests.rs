use chrono::Utc;
use std::collections::HashSet;

use agent_studios_control_plane::clock::FixedClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::error::{ControlPlaneError, ReplayError};
use agent_studios_control_plane::store::InMemoryStore;
use agent_studios_protocol::agent::AgentKind;
use agent_studios_protocol::approval::{ApprovalKind, ApprovalState};
use agent_studios_protocol::artifact::ArtifactKind;
use agent_studios_protocol::cancellation::CancellationScope;
use agent_studios_protocol::id::StudioId;
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::TaskState;

#[test]
fn test_control_plane_full_lifecycle_and_invariants() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    // 1. Create Studio
    let studio = cp.create_studio("Studio Alpha").unwrap();
    assert_eq!(studio.name, "Studio Alpha");
    let studio_id = studio.id;

    // 2. Register Agents
    let agent = cp
        .register_agent(
            studio_id,
            "Coder Agent",
            AgentKind::Internal,
            Some("Fullstack Developer".into()),
        )
        .unwrap();
    let agent_id = agent.id;

    // 3. Create Tasks with Dependency
    let task1 = cp
        .create_task(
            studio_id,
            "Design Schema",
            "Define database schema",
            None,
            Some(agent_id),
            vec![],
        )
        .unwrap();
    assert_eq!(task1.state, TaskState::Ready);
    let task1_id = task1.id;

    let task2 = cp
        .create_task(
            studio_id,
            "Implement Migrations",
            "Generate SQL migrations",
            Some(task1_id),
            Some(agent_id),
            vec![task1_id],
        )
        .unwrap();
    // Task 2 depends on Task 1, so it must start Blocked
    assert_eq!(task2.state, TaskState::Blocked);
    let task2_id = task2.id;

    // Invariant check: cannot depend on task from another studio
    let foreign_studio = cp.create_studio("Foreign Studio").unwrap();
    let foreign_task = cp
        .create_task(
            foreign_studio.id,
            "Foreign Task",
            "Alien work",
            None,
            None,
            vec![],
        )
        .unwrap();

    let err_foreign = cp
        .add_task_dependency(task1_id, foreign_task.id)
        .unwrap_err();
    assert!(matches!(
        err_foreign,
        ControlPlaneError::StudioMismatch { .. }
    ));

    // 4. Create Run for Task 1
    let run = cp.create_run(task1_id, agent_id).unwrap();
    assert_eq!(run.state, RunState::Queued);
    let run_id = run.id;

    // 5. Request & Resolve Approval
    let approval = cp
        .request_approval(
            studio_id,
            task1_id,
            agent_id,
            ApprovalKind::FileWrite,
            "Allow schema modifications",
        )
        .unwrap();
    assert_eq!(approval.state, ApprovalState::Pending);
    let approval_id = approval.id;

    // Resolve approval
    cp.resolve_approval(approval_id, ApprovalState::Approved)
        .unwrap();
    assert_eq!(
        cp.get_approval(approval_id).unwrap().state,
        ApprovalState::Approved
    );

    // Double resolution rejected
    let err_double = cp
        .resolve_approval(approval_id, ApprovalState::Denied)
        .unwrap_err();
    assert!(matches!(err_double, ControlPlaneError::Transition(_)));

    // 6. Transition Run and Task through lifecycle
    cp.transition_run_state(run_id, RunState::Starting).unwrap();
    cp.transition_run_state(run_id, RunState::Running).unwrap();
    cp.transition_task_state(task1_id, TaskState::Running)
        .unwrap();

    cp.transition_run_state(run_id, RunState::Succeeded)
        .unwrap();
    cp.transition_task_state(task1_id, TaskState::Succeeded)
        .unwrap();

    // Verification: Task 2 should now automatically unblock and become Ready!
    let task2_after = cp.get_task(task2_id).unwrap();
    assert_eq!(task2_after.state, TaskState::Ready);
    assert!(cp.is_task_ready(task2_id).unwrap());

    // 7. Register Artifact
    let artifact = cp
        .register_artifact(
            studio_id,
            task1_id,
            agent_id,
            ArtifactKind::Patch,
            "migrations.sql",
            Some("sha256-abc123hash".into()),
            "artifacts/migrations.sql",
        )
        .unwrap();
    assert_eq!(artifact.logical_name, "migrations.sql");
    assert_eq!(cp.get_artifact(artifact.id).unwrap().task_id, task1_id);

    // 8. Event sequence checks
    let events = cp.events_for_studio(studio_id, 1).unwrap();
    assert!(!events.is_empty());
    for (i, env) in events.iter().enumerate() {
        assert_eq!(env.sequence, (i + 1) as u64);
        assert_eq!(env.studio_id, studio_id);
    }
}

#[test]
fn test_hierarchical_cancellation() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    let studio = cp.create_studio("Cancel Studio").unwrap();
    let studio_id = studio.id;
    let agent = cp
        .register_agent(studio_id, "Worker", AgentKind::Internal, None)
        .unwrap();
    let agent_id = agent.id;

    let parent_task = cp
        .create_task(studio_id, "Parent Task", "", None, Some(agent_id), vec![])
        .unwrap();
    let child_task = cp
        .create_task(
            studio_id,
            "Child Task",
            "",
            Some(parent_task.id),
            Some(agent_id),
            vec![],
        )
        .unwrap();

    let run1 = cp.create_run(parent_task.id, agent_id).unwrap();
    let run2 = cp.create_run(child_task.id, agent_id).unwrap();
    let approval = cp
        .request_approval(
            studio_id,
            parent_task.id,
            agent_id,
            ApprovalKind::CommandExecution,
            "Run worker",
        )
        .unwrap();

    // Cancel at parent task level with include_descendants = true
    let summary = cp
        .request_cancellation(
            CancellationScope::Task {
                task_id: parent_task.id,
                include_descendants: true,
            },
            Some("User cancelled whole tree".into()),
        )
        .unwrap();

    assert!(summary.cancelled_tasks.contains(&parent_task.id));
    assert!(summary.cancelled_tasks.contains(&child_task.id));
    assert!(summary.cancelled_runs.contains(&run1.id));
    assert!(summary.cancelled_runs.contains(&run2.id));
    assert!(summary.cancelled_approvals.contains(&approval.id));

    // Verify entity states
    assert_eq!(
        cp.get_task(parent_task.id).unwrap().state,
        TaskState::Cancelled
    );
    assert_eq!(
        cp.get_task(child_task.id).unwrap().state,
        TaskState::Cancelled
    );
    assert_eq!(cp.get_run(run1.id).unwrap().state, RunState::Cancelled);
    assert_eq!(cp.get_run(run2.id).unwrap().state, RunState::Cancelled);
    assert_eq!(
        cp.get_approval(approval.id).unwrap().state,
        ApprovalState::Cancelled
    );

    // Idempotent cancellation: repeating cancellation on already cancelled entities should succeed with empty summary
    let summary2 = cp
        .request_cancellation(
            CancellationScope::Task {
                task_id: parent_task.id,
                include_descendants: true,
            },
            Some("Redundant cancellation".into()),
        )
        .unwrap();
    assert!(summary2.cancelled_tasks.is_empty());
    assert!(summary2.cancelled_runs.is_empty());
    assert!(summary2.cancelled_approvals.is_empty());
}

#[test]
fn test_event_replay_reconstructs_identical_domain_state() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut original_cp = ControlPlane::new(clock, store);

    // Build rich domain state across two studios
    let s1 = original_cp.create_studio("Studio 1").unwrap();
    let s2 = original_cp.create_studio("Studio 2").unwrap();

    let a1 = original_cp
        .register_agent(s1.id, "Agent 1", AgentKind::Internal, None)
        .unwrap();
    let a2 = original_cp
        .register_agent(s2.id, "Agent 2", AgentKind::External, None)
        .unwrap();

    let t1 = original_cp
        .create_task(s1.id, "Task 1", "T1", None, Some(a1.id), vec![])
        .unwrap();
    let t2 = original_cp
        .create_task(s1.id, "Task 2", "T2", Some(t1.id), Some(a1.id), vec![t1.id])
        .unwrap();

    let r1 = original_cp.create_run(t1.id, a1.id).unwrap();
    let app1 = original_cp
        .request_approval(
            s1.id,
            t1.id,
            a1.id,
            ApprovalKind::CommandExecution,
            "Run bash",
        )
        .unwrap();
    original_cp
        .resolve_approval(app1.id, ApprovalState::Approved)
        .unwrap();

    original_cp
        .transition_run_state(r1.id, RunState::Starting)
        .unwrap();
    original_cp
        .transition_run_state(r1.id, RunState::Running)
        .unwrap();
    original_cp
        .transition_task_state(t1.id, TaskState::Running)
        .unwrap();
    original_cp
        .transition_run_state(r1.id, RunState::Succeeded)
        .unwrap();
    original_cp
        .transition_task_state(t1.id, TaskState::Succeeded)
        .unwrap();

    let art1 = original_cp
        .register_artifact(
            s1.id,
            t1.id,
            a1.id,
            ArtifactKind::Log,
            "execution.log",
            None,
            "logs/exec.log",
        )
        .unwrap();

    // Verify task2 unblocked in original
    assert_eq!(original_cp.get_task(t2.id).unwrap().state, TaskState::Ready);

    // Collect all event envelopes emitted by original
    let all_events = original_cp.all_events().unwrap();
    assert!(!all_events.is_empty());

    // Replay into a completely blank instance
    let replay_clock = FixedClock::new(now);
    let replay_store = InMemoryStore::new();
    let replayed_cp = ControlPlane::replay_events(&all_events, replay_clock, replay_store).unwrap();

    // Assert exact state parity
    // 1. Studios
    let orig_studios: HashSet<_> = original_cp.all_studios().map(|s| s.id).collect();
    let replayed_studios: HashSet<_> = replayed_cp.all_studios().map(|s| s.id).collect();
    assert_eq!(orig_studios, replayed_studios);
    assert_eq!(
        original_cp.get_studio(s1.id).unwrap().name,
        replayed_cp.get_studio(s1.id).unwrap().name
    );
    assert_eq!(
        original_cp.get_studio(s2.id).unwrap().name,
        replayed_cp.get_studio(s2.id).unwrap().name
    );

    // 2. Agents
    let orig_agents: HashSet<_> = original_cp.all_agents().map(|a| a.id).collect();
    let replayed_agents: HashSet<_> = replayed_cp.all_agents().map(|a| a.id).collect();
    assert_eq!(orig_agents, replayed_agents);
    assert_eq!(
        original_cp.get_agent(a1.id).unwrap(),
        replayed_cp.get_agent(a1.id).unwrap()
    );
    assert_eq!(
        original_cp.get_agent(a2.id).unwrap(),
        replayed_cp.get_agent(a2.id).unwrap()
    );

    // 3. Tasks & Graph
    let orig_tasks: HashSet<_> = original_cp.all_tasks().map(|t| t.id).collect();
    let replayed_tasks: HashSet<_> = replayed_cp.all_tasks().map(|t| t.id).collect();
    assert_eq!(orig_tasks, replayed_tasks);
    assert_eq!(
        original_cp.get_task(t1.id).unwrap(),
        replayed_cp.get_task(t1.id).unwrap()
    );
    assert_eq!(
        original_cp.get_task(t2.id).unwrap(),
        replayed_cp.get_task(t2.id).unwrap()
    );
    assert_eq!(replayed_cp.get_task(t2.id).unwrap().state, TaskState::Ready);

    // 4. Runs
    let orig_runs: HashSet<_> = original_cp.all_runs().map(|r| r.id).collect();
    let replayed_runs: HashSet<_> = replayed_cp.all_runs().map(|r| r.id).collect();
    assert_eq!(orig_runs, replayed_runs);
    assert_eq!(
        original_cp.get_run(r1.id).unwrap(),
        replayed_cp.get_run(r1.id).unwrap()
    );

    // 5. Approvals
    let orig_apps: HashSet<_> = original_cp.all_approvals().map(|a| a.id).collect();
    let replayed_apps: HashSet<_> = replayed_cp.all_approvals().map(|a| a.id).collect();
    assert_eq!(orig_apps, replayed_apps);
    assert_eq!(
        original_cp.get_approval(app1.id).unwrap(),
        replayed_cp.get_approval(app1.id).unwrap()
    );

    // 6. Artifacts
    let orig_arts: HashSet<_> = original_cp.all_artifacts().map(|a| a.id).collect();
    let replayed_arts: HashSet<_> = replayed_cp.all_artifacts().map(|a| a.id).collect();
    assert_eq!(orig_arts, replayed_arts);
    assert_eq!(
        original_cp.get_artifact(art1.id).unwrap(),
        replayed_cp.get_artifact(art1.id).unwrap()
    );

    // 7. Store parity: replayed store events match original exactly
    let orig_store_events = original_cp.all_events().unwrap();
    let replayed_store_events = replayed_cp.all_events().unwrap();
    assert_eq!(orig_store_events, replayed_store_events);
}

#[test]
fn test_replay_sequence_gap_rejected() {
    let now = Utc::now();
    let studio_id = StudioId::new();

    let dummy_event = agent_studios_protocol::event::ControlPlaneEvent::StudioCreated {
        studio: agent_studios_protocol::studio::Studio::with_id(studio_id, "Gap Test", now),
    };

    let bad_events = vec![
        agent_studios_protocol::event::EventEnvelope::new(studio_id, 1, now, dummy_event.clone()),
        // Sequence gap: 3 instead of 2
        agent_studios_protocol::event::EventEnvelope::new(studio_id, 3, now, dummy_event),
    ];

    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let result = ControlPlane::replay_events(&bad_events, clock, store);

    assert!(matches!(
        result,
        Err(ReplayError::InvalidSequence {
            expected: 2,
            actual: 3
        })
    ));
}
