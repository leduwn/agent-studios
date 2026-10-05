use agent_studios_control_plane::engine::ControlPlaneState;
use agent_studios_orchestration::{
    SequenceError, SequenceTracker, build_task_graph_snapshot, project_agent_summary,
    project_run_summary,
};
use agent_studios_protocol::agent::{AgentDescriptor, AgentExecutionBudget, AgentKind, AgentState};
use agent_studios_protocol::event::{ControlPlaneEvent, EventEnvelope};
use agent_studios_protocol::id::{AgentId, EventId, StudioId, TaskId};
use agent_studios_protocol::run::{RunRecord, RunState};
use agent_studios_protocol::studio::Studio;
use agent_studios_protocol::task::{TaskRecord, TaskState};
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
