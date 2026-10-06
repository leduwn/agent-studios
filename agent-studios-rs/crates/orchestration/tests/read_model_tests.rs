use agent_studios_control_plane::engine::ControlPlaneState;
use agent_studios_orchestration::{
    SequenceError, SequenceTracker, TimelineItemKind, build_artifact_index,
    build_task_graph_snapshot, build_worktree_snapshot, project_agent_summary,
    project_artifact_index, project_run_summary, project_task_timeline, project_worktree_snapshot,
};
use agent_studios_protocol::agent::{AgentDescriptor, AgentExecutionBudget, AgentKind, AgentState};
use agent_studios_protocol::artifact::{ArtifactKind, ArtifactRecord};
use agent_studios_protocol::event::{ControlPlaneEvent, EventEnvelope};
use agent_studios_protocol::id::{
    AgentId, ArtifactId, EventId, RunId, StudioId, TaskId, WorktreeId,
};
use agent_studios_protocol::reconciliation::ReconciliationRecord;
use agent_studios_protocol::run::{RunRecord, RunState};
use agent_studios_protocol::studio::Studio;
use agent_studios_protocol::task::{TaskRecord, TaskState};
use agent_studios_protocol::worktree::{WorktreeRecord, WorktreeState};
use chrono::Utc;

#[test]
fn test_agent_summary_projection() {
    let studio_id = StudioId::new();
    let agent_id = AgentId::new();
    let parent_id = AgentId::new();
    let now = Utc::now();

    let mut agent = AgentDescriptor::new(
        studio_id,
        "CoderAgent",
        AgentKind::Internal,
        Some("Engineer".to_string()),
    );
    agent.id = agent_id;

    let budget = AgentExecutionBudget::unlimited().with_tool_calls(10);

    let events = vec![
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 1,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::AgentRegistered { agent },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 2,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::AgentStateChanged {
                agent_id,
                previous_state: AgentState::Registered,
                new_state: AgentState::Starting,
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 3,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::AgentRuntimeBound {
                agent_id,
                runtime_kind: "codex".to_string(),
                provider_instance_id: "anthropic".to_string(),
                model_id: "claude-sonnet-5".to_string(),
                protocol: "anthropic_messages".to_string(),
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 4,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::AgentSpawned {
                agent_id,
                thread_id: "thread_xyz".to_string(),
                parent_agent_id: Some(parent_id),
                parent_thread_id: None,
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 5,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::ToolStarted {
                agent_id,
                task_id: None,
                run_id: None,
                tool_name: "read_file".to_string(),
                call_id: "call_1".to_string(),
                timestamp: now,
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 6,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::ToolCompleted {
                agent_id,
                task_id: None,
                run_id: None,
                tool_name: "read_file".to_string(),
                call_id: "call_1".to_string(),
                duration_ms: 50,
                timestamp: now,
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 7,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::BudgetExceeded {
                agent_id,
                run_id: None,
                dimension: "tool_calls".to_string(),
                limit: 10,
                actual: 11,
            },
        },
    ];

    let summary = project_agent_summary(agent_id, &events, Some(budget)).unwrap();
    assert_eq!(summary.agent_id, agent_id);
    assert_eq!(summary.agent_name, "CoderAgent");
    assert_eq!(summary.role.as_deref(), Some("Engineer"));
    assert_eq!(summary.state, AgentState::Starting);
    assert!(summary.runtime_bound);
    assert_eq!(summary.provider.as_deref(), Some("anthropic"));
    assert_eq!(summary.model.as_deref(), Some("claude-sonnet-5"));
    assert_eq!(summary.wire_protocol.as_deref(), Some("anthropic_messages"));
    assert_eq!(summary.codex_thread_id.as_deref(), Some("thread_xyz"));
    assert_eq!(summary.parent_agent_id, Some(parent_id));
    assert_eq!(summary.spawn_depth, 1);
    assert_eq!(summary.tool_calls_total, 1);
    assert_eq!(summary.tool_calls_active, 0);
    assert_eq!(summary.budget.max_tool_calls, Some(10));
    assert!(summary.budget_exceeded);
    assert_eq!(
        summary.budget_exceeded_dimension.as_deref(),
        Some("tool_calls")
    );
}

#[test]
fn test_task_graph_snapshot() {
    let studio_id = StudioId::new();
    let mut state = ControlPlaneState::default();

    let t1_id = TaskId::new();
    let t2_id = TaskId::new();
    let t3_id = TaskId::new();

    let mut t1 = TaskRecord::new(
        studio_id,
        "Task 1",
        "Desc 1",
        None,
        None,
        vec![],
        Utc::now(),
    );
    t1.id = t1_id;
    t1.state = TaskState::Succeeded;

    let mut t2 = TaskRecord::new(
        studio_id,
        "Task 2",
        "Desc 2",
        None,
        None,
        vec![t1_id],
        Utc::now(),
    );
    t2.id = t2_id;
    t2.state = TaskState::Ready;

    let mut t3 = TaskRecord::new(
        studio_id,
        "Task 3",
        "Desc 3",
        None,
        None,
        vec![t2_id],
        Utc::now(),
    );
    t3.id = t3_id;
    t3.state = TaskState::Retrying;
    t3.retry_count = 1;

    state.task_graph.add_task(t1).unwrap();
    state.task_graph.add_task(t2).unwrap();
    state.task_graph.add_task(t3).unwrap();

    let snapshot = build_task_graph_snapshot(studio_id, &state);
    assert_eq!(snapshot.tasks.len(), 3);
    assert_eq!(snapshot.succeeded_tasks, vec![t1_id]);
    assert_eq!(snapshot.ready_tasks, vec![t2_id]);
    assert_eq!(snapshot.retrying_tasks, vec![t3_id]);
    assert_eq!(snapshot.adjacency.get(&t2_id).unwrap(), &vec![t1_id]);
    assert_eq!(snapshot.adjacency.get(&t3_id).unwrap(), &vec![t2_id]);
}

#[test]
fn test_run_summary_projection() {
    let studio_id = StudioId::new();
    let task_id = TaskId::new();
    let agent_id = AgentId::new();
    let run = RunRecord::new(task_id, agent_id, 1);
    let run_id = run.id;
    let t0 = Utc::now();
    let t1 = t0 + chrono::Duration::milliseconds(250);

    let events = vec![
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 1,
            studio_id,
            timestamp: t0,
            event: ControlPlaneEvent::RunCreated { run },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 2,
            studio_id,
            timestamp: t0,
            event: ControlPlaneEvent::RunStateChanged {
                run_id,
                previous_state: RunState::Queued,
                new_state: RunState::Running,
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 3,
            studio_id,
            timestamp: t1,
            event: ControlPlaneEvent::RunStateChanged {
                run_id,
                previous_state: RunState::Running,
                new_state: RunState::Succeeded,
            },
        },
    ];

    let summary = project_run_summary(run_id, &events).unwrap();
    assert_eq!(summary.run_id, run_id);
    assert_eq!(summary.task_id, task_id);
    assert_eq!(summary.agent_id, agent_id);
    assert_eq!(summary.state, RunState::Succeeded);
    assert_eq!(summary.started_at, Some(t0));
    assert_eq!(summary.completed_at, Some(t1));
    assert_eq!(summary.duration_ms, Some(250));
    assert!(summary.error.is_none());
}

#[test]
fn test_sequence_tracker_monotonicity_and_gap_detection() {
    let mut tracker = SequenceTracker::new();
    let studio_id = StudioId::new();
    let now = Utc::now();

    let env1 = EventEnvelope {
        event_id: EventId::new(),
        schema_version: 1,
        sequence: 1,
        studio_id,
        timestamp: now,
        event: ControlPlaneEvent::StudioCreated {
            studio: Studio::new("S1", now),
        },
    };
    let env2 = EventEnvelope {
        event_id: EventId::new(),
        schema_version: 1,
        sequence: 2,
        studio_id,
        timestamp: now,
        event: ControlPlaneEvent::StudioCreated {
            studio: Studio::new("S1", now),
        },
    };
    let env4 = EventEnvelope {
        event_id: EventId::new(),
        schema_version: 1,
        sequence: 4,
        studio_id,
        timestamp: now,
        event: ControlPlaneEvent::StudioCreated {
            studio: Studio::new("S1", now),
        },
    };

    assert!(tracker.observe(&env1).is_ok());
    assert_eq!(tracker.last_seen_sequence(), Some(1));
    assert!(tracker.observe(&env2).is_ok());
    assert_eq!(tracker.last_seen_sequence(), Some(2));

    // Envelope 4 introduces a gap (expected 3)
    let err = tracker.observe(&env4).unwrap_err();
    assert_eq!(
        err,
        SequenceError::GapOrOutdated {
            expected: 3,
            actual: 4
        }
    );
}

#[test]
fn test_recursive_spawn_depth_and_cycle_protection() {
    let studio_id = StudioId::new();
    let root_id = AgentId::new();
    let child_id = AgentId::new();
    let grandchild_id = AgentId::new();
    let now = Utc::now();

    let mut root_agent = AgentDescriptor::new(
        studio_id,
        "RootCoordinator",
        AgentKind::Internal,
        Some("Coordinator".to_string()),
    );
    root_agent.id = root_id;
    let mut child_agent = AgentDescriptor::new(
        studio_id,
        "ChildCoder",
        AgentKind::Internal,
        Some("Coder".to_string()),
    );
    child_agent.id = child_id;
    let mut grandchild_agent = AgentDescriptor::new(
        studio_id,
        "GrandchildReviewer",
        AgentKind::Internal,
        Some("Reviewer".to_string()),
    );
    grandchild_agent.id = grandchild_id;

    let events = vec![
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 1,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::AgentRegistered { agent: root_agent },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 2,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::AgentSpawned {
                agent_id: root_id,
                thread_id: "thread_root".to_string(),
                parent_agent_id: None,
                parent_thread_id: None,
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 3,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::AgentRegistered { agent: child_agent },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 4,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::AgentSpawned {
                agent_id: child_id,
                thread_id: "thread_child".to_string(),
                parent_agent_id: Some(root_id),
                parent_thread_id: Some("thread_root".to_string()),
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 5,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::AgentRegistered {
                agent: grandchild_agent,
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 6,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::AgentSpawned {
                agent_id: grandchild_id,
                thread_id: "thread_grandchild".to_string(),
                parent_agent_id: Some(child_id),
                parent_thread_id: Some("thread_child".to_string()),
            },
        },
    ];

    let root_summary = project_agent_summary(root_id, &events, None).unwrap();
    assert_eq!(root_summary.spawn_depth, 0);

    let child_summary = project_agent_summary(child_id, &events, None).unwrap();
    assert_eq!(child_summary.spawn_depth, 1);
    assert_eq!(child_summary.parent_agent_id, Some(root_id));
    assert_eq!(
        child_summary.parent_thread_id.as_deref(),
        Some("thread_root")
    );

    let grandchild_summary = project_agent_summary(grandchild_id, &events, None).unwrap();
    assert_eq!(grandchild_summary.spawn_depth, 2);
    assert_eq!(grandchild_summary.parent_agent_id, Some(child_id));
    assert_eq!(
        grandchild_summary.parent_thread_id.as_deref(),
        Some("thread_child")
    );
}

#[test]
fn test_agent_operational_state_and_current_task_id() {
    use agent_studios_orchestration::AgentOperationalState;

    let studio_id = StudioId::new();
    let agent_id = AgentId::new();
    let task_id = TaskId::new();
    let run = RunRecord::new(task_id, agent_id, 1);
    let run_id = run.id;
    let now = Utc::now();

    let mut agent = AgentDescriptor::new(studio_id, "Worker", AgentKind::Internal, None);
    agent.id = agent_id;

    let mut events = vec![
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 1,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::AgentRegistered { agent },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 2,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::RunCreated { run },
        },
    ];

    let s1 = project_agent_summary(agent_id, &events, None).unwrap();
    assert_eq!(s1.operational_state, AgentOperationalState::Idle);
    assert_eq!(s1.current_task_id, None);

    // Run starts
    events.push(EventEnvelope {
        event_id: EventId::new(),
        schema_version: 1,
        sequence: 3,
        studio_id,
        timestamp: now,
        event: ControlPlaneEvent::RunStateChanged {
            run_id,
            previous_state: RunState::Queued,
            new_state: RunState::Running,
        },
    });

    let s2 = project_agent_summary(agent_id, &events, None).unwrap();
    assert_eq!(s2.operational_state, AgentOperationalState::Running);
    assert_eq!(s2.current_task_id, Some(task_id));

    // Run completes
    events.push(EventEnvelope {
        event_id: EventId::new(),
        schema_version: 1,
        sequence: 4,
        studio_id,
        timestamp: now,
        event: ControlPlaneEvent::RunStateChanged {
            run_id,
            previous_state: RunState::Running,
            new_state: RunState::Succeeded,
        },
    });

    let s3 = project_agent_summary(agent_id, &events, None).unwrap();
    assert_eq!(s3.operational_state, AgentOperationalState::Idle);
    assert_eq!(s3.current_task_id, None);
}

#[test]
fn test_run_summary_with_outcome_recorded() {
    let studio_id = StudioId::new();
    let task_id = TaskId::new();
    let agent_id = AgentId::new();
    let run = RunRecord::new(task_id, agent_id, 2);
    let run_id = run.id;
    let now = Utc::now();

    let events = vec![
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 1,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::RunCreated { run },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 2,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::RunOutcomeRecorded {
                run_id,
                task_id,
                agent_id,
                classification: "budget_exceeded".to_string(),
                safe_error_summary: Some("tool_calls limit reached".to_string()),
            },
        },
    ];

    let summary = project_run_summary(run_id, &events).unwrap();
    assert_eq!(summary.attempt, 2);
    assert_eq!(
        summary.failure_classification.as_deref(),
        Some("budget_exceeded")
    );
    assert_eq!(
        summary.safe_error_summary.as_deref(),
        Some("tool_calls limit reached")
    );
    assert_eq!(summary.error.as_deref(), Some("tool_calls limit reached"));
}

#[test]
fn test_artifact_index_projection_and_build() {
    let studio_id = StudioId::new();
    let task1_id = TaskId::new();
    let task2_id = TaskId::new();
    let agent_id = AgentId::new();
    let wt_id = WorktreeId::new();
    let now = Utc::now();

    let art1 = ArtifactRecord::new(
        studio_id,
        task1_id,
        agent_id,
        ArtifactKind::Patch,
        "patch.diff",
        Some("sha256:abc".to_string()),
        "blobs/sha256/abc",
        now,
    )
    .with_worktree_id(Some(wt_id))
    .with_size_bytes(Some(100));

    let art2 = ArtifactRecord::new(
        studio_id,
        task1_id,
        agent_id,
        ArtifactKind::Log,
        "execution.log",
        Some("sha256:def".to_string()),
        "blobs/sha256/def",
        now,
    )
    .with_worktree_id(Some(wt_id))
    .with_size_bytes(Some(250));

    let art3 = ArtifactRecord::new(
        studio_id,
        task2_id,
        agent_id,
        ArtifactKind::Report,
        "summary.json",
        Some("sha256:123".to_string()),
        "blobs/sha256/123",
        now,
    )
    .with_size_bytes(Some(500));

    let events = vec![
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 1,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::ArtifactRegistered {
                artifact: art1.clone(),
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 2,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::ArtifactRegistered {
                artifact: art2.clone(),
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 3,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::ArtifactRegistered {
                artifact: art3.clone(),
            },
        },
    ];

    let index = project_artifact_index(studio_id, &events);
    assert_eq!(index.artifacts.len(), 3);
    assert_eq!(index.total_bytes, 850);
    assert_eq!(index.artifacts_for_task(&task1_id).len(), 2);
    assert_eq!(index.artifacts_for_task(&task2_id).len(), 1);
    assert_eq!(index.artifacts_for_worktree(&wt_id).len(), 2);
    assert_eq!(index.artifacts_of_kind(ArtifactKind::Patch).len(), 1);
    assert_eq!(index.get_artifact(&art1.id), Some(&art1));

    // Test build_artifact_index from ControlPlaneState
    let mut state = ControlPlaneState::default();
    state.artifacts.insert(art1.id, art1);
    state.artifacts.insert(art2.id, art2);
    state.artifacts.insert(art3.id, art3);

    let state_index = build_artifact_index(studio_id, &state);
    assert_eq!(state_index.artifacts.len(), 3);
    assert_eq!(state_index.total_bytes, 850);
    assert_eq!(state_index.artifacts_for_task(&task1_id).len(), 2);
}

#[test]
fn test_worktree_snapshot_projection_and_build() {
    let studio_id = StudioId::new();
    let task_id = TaskId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();
    let now = Utc::now();

    let mut wt = WorktreeRecord::new(
        studio_id,
        "wt_feature",
        "/repos/main",
        "/managed/wt_feature",
        "commit_abc",
        now,
    );
    let wt_id = wt.id;

    let patch_art_id = ArtifactId::new();

    let events = vec![
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 1,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::WorktreeCreated {
                worktree: wt.clone(),
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 2,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::WorktreeAssigned {
                worktree_id: wt_id,
                task_id,
                agent_id,
                run_id: Some(run_id),
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 3,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::WorktreeThreadBound {
                worktree_id: wt_id,
                thread_id: "thread_alpha".to_string(),
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 4,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::WorktreeStateChanged {
                worktree_id: wt_id,
                previous_state: WorktreeState::Creating,
                new_state: WorktreeState::InUse,
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 5,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::WorktreeChangeCaptured {
                worktree_id: wt_id,
                run_id: Some(run_id),
                base_commit: "commit_abc".to_string(),
                head_commit: Some("commit_def".to_string()),
                patch_artifact_id: patch_art_id,
                stats_artifact_id: None,
                files_changed: 3,
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 6,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::WorktreeReleased {
                worktree_id: wt_id,
                retained: true,
                reason: Some("Retained for inspection".to_string()),
            },
        },
    ];

    let snapshot = project_worktree_snapshot(studio_id, &events);
    assert_eq!(snapshot.worktrees.len(), 1);
    assert_eq!(snapshot.retained_worktrees, vec![wt_id]);
    assert_eq!(snapshot.worktree_for_task(&task_id).unwrap().id, wt_id);
    assert_eq!(
        snapshot.worktree_for_thread("thread_alpha").unwrap().id,
        wt_id
    );

    let projected_wt = snapshot.get_worktree(&wt_id).unwrap();
    assert_eq!(projected_wt.patch_artifact_id, Some(patch_art_id));
    assert!(projected_wt.retained);
    assert_eq!(
        projected_wt.retained_reason.as_deref(),
        Some("Retained for inspection")
    );

    // Test build_worktree_snapshot from ControlPlaneState
    wt.assigned_task_id = Some(task_id);
    wt.bound_thread_id = Some("thread_alpha".to_string());
    wt.retained = true;
    let mut state = ControlPlaneState::default();
    state.worktrees.insert(wt_id, wt);

    let state_snapshot = build_worktree_snapshot(studio_id, &state);
    assert_eq!(state_snapshot.worktrees.len(), 1);
    assert_eq!(state_snapshot.retained_worktrees, vec![wt_id]);
    assert_eq!(
        state_snapshot.worktree_for_task(&task_id).unwrap().id,
        wt_id
    );
}

#[test]
fn test_task_timeline_projection() {
    let studio_id = StudioId::new();
    let task_id = TaskId::new();
    let agent_id = AgentId::new();
    let wt_id = WorktreeId::new();
    let art_id = ArtifactId::new();
    let run = RunRecord::new(task_id, agent_id, 1);
    let run_id = run.id;
    let now = Utc::now();

    let mut task = TaskRecord::new(
        studio_id,
        "Implement Feature X",
        "Details",
        None,
        Some(agent_id),
        vec![],
        now,
    );
    task.id = task_id;

    let rec = ReconciliationRecord::new(
        studio_id,
        wt_id,
        task_id,
        Some(run_id),
        art_id,
        "/managed/integration",
        "commit_base",
        now,
    );
    let rec_id = rec.id;

    let events = vec![
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 1,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::TaskCreated { task },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 2,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::WorktreeAssigned {
                worktree_id: wt_id,
                task_id,
                agent_id,
                run_id: None,
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 3,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::WorktreeThreadBound {
                worktree_id: wt_id,
                thread_id: "thread_123".to_string(),
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 4,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::RunCreated { run },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 5,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::TaskStateChanged {
                task_id,
                previous_state: TaskState::Ready,
                new_state: TaskState::Running,
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 6,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::WorktreeChangeCaptured {
                worktree_id: wt_id,
                run_id: Some(run_id),
                base_commit: "commit_base".to_string(),
                head_commit: Some("commit_head".to_string()),
                patch_artifact_id: art_id,
                stats_artifact_id: None,
                files_changed: 2,
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 7,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::ReconciliationCreated {
                reconciliation: rec,
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 8,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::ReconciliationApplied {
                reconciliation_id: rec_id,
                merge_commit: Some("commit_merge".to_string()),
            },
        },
        EventEnvelope {
            event_id: EventId::new(),
            schema_version: 1,
            sequence: 9,
            studio_id,
            timestamp: now,
            event: ControlPlaneEvent::TaskStateChanged {
                task_id,
                previous_state: TaskState::Running,
                new_state: TaskState::Succeeded,
            },
        },
    ];

    let timeline = project_task_timeline(task_id, &events).unwrap();
    assert_eq!(timeline.task_id, task_id);
    assert_eq!(timeline.studio_id, studio_id);
    assert_eq!(timeline.current_state, TaskState::Succeeded);
    assert_eq!(timeline.assigned_worktree_id, Some(wt_id));
    assert_eq!(timeline.bound_thread_id.as_deref(), Some("thread_123"));
    assert_eq!(timeline.artifact_ids, vec![art_id]);
    assert_eq!(timeline.items.len(), 9);
    assert_eq!(timeline.items[0].kind, TimelineItemKind::TaskCreated);
    assert_eq!(timeline.items[1].kind, TimelineItemKind::WorktreeAssigned);
    assert_eq!(
        timeline.items[2].kind,
        TimelineItemKind::WorktreeThreadBound
    );
    assert_eq!(timeline.items[3].kind, TimelineItemKind::RunCreated);
    assert_eq!(timeline.items[4].kind, TimelineItemKind::TaskStateChanged);
    assert_eq!(
        timeline.items[5].kind,
        TimelineItemKind::WorktreeChangeCaptured
    );
    assert_eq!(
        timeline.items[6].kind,
        TimelineItemKind::ReconciliationCreated
    );
    assert_eq!(
        timeline.items[7].kind,
        TimelineItemKind::ReconciliationApplied
    );
    assert_eq!(timeline.items[8].kind, TimelineItemKind::TaskStateChanged);
}

#[test]
fn test_enriched_task_graph_snapshot() {
    let studio_id = StudioId::new();
    let mut state = ControlPlaneState::default();

    let t1_id = TaskId::new();
    let mut t1 = TaskRecord::new(
        studio_id,
        "Mutating Task",
        "Desc",
        None,
        None,
        vec![],
        Utc::now(),
    );
    t1.id = t1_id;
    state.task_graph.add_task(t1).unwrap();

    let mut wt = WorktreeRecord::new(
        studio_id,
        "wt_1",
        "/repo",
        "/managed/wt_1",
        "base_c",
        Utc::now(),
    );
    wt.assigned_task_id = Some(t1_id);
    let wt_id = wt.id;
    state.worktrees.insert(wt_id, wt);

    let art = ArtifactRecord::new(
        studio_id,
        t1_id,
        AgentId::new(),
        ArtifactKind::Patch,
        "patch.diff",
        None,
        "location",
        Utc::now(),
    );
    let art_id = art.id;
    state.artifacts.insert(art_id, art);

    let rec = ReconciliationRecord::new(
        studio_id,
        wt_id,
        t1_id,
        None,
        art_id,
        "/integration",
        "base_c",
        Utc::now(),
    );
    let rec_id = rec.id;
    state.reconciliations.insert(rec_id, rec);

    let snapshot = build_task_graph_snapshot(studio_id, &state);
    assert_eq!(snapshot.task_worktrees.get(&t1_id), Some(&wt_id));
    assert_eq!(snapshot.task_artifacts.get(&t1_id), Some(&vec![art_id]));
    assert_eq!(
        snapshot.task_reconciliations.get(&t1_id),
        Some(&vec![rec_id])
    );
}
