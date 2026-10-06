use std::time::Duration;

use agent_studios_control_plane::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::store::InMemoryStore;
use agent_studios_internal_agent::{
    AgentExecutionResult, AgentStudiosSupervisor, ControlPlaneActor, CoordinatorDecision,
    FailurePolicy, InternalAgentSpec, InternalTeamSpec, MockAgentExecutor, PlannedTask,
    WorkspaceAccessMode, WorkspacePolicyArbitrator,
};
use agent_studios_protocol::event::ControlPlaneEvent;
use agent_studios_protocol::id::AgentId;
use agent_studios_protocol::task::TaskState;
use agent_studios_provider::id::{ModelId, ProviderInstanceId};
use agent_studios_provider::model::ModelRef;

#[tokio::test(flavor = "multi_thread")]
async fn test_fail_fast_cancels_active_turns_and_marks_non_terminal_tasks() {
    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    let (cp_handle, _actor_task) = ControlPlaneActor::spawn(cp);

    let dummy_model = ModelRef::new(
        ProviderInstanceId::new(),
        ModelId::new("test-model").unwrap(),
    );
    let coord = InternalAgentSpec::new(AgentId::new(), "Coord", "Coordinator", dummy_model.clone());
    let worker1 = InternalAgentSpec::new(AgentId::new(), "Worker1", "Worker1", dummy_model.clone())
        .with_workspace_access(WorkspaceAccessMode::ReadOnly);
    let worker2 = InternalAgentSpec::new(AgentId::new(), "Worker2", "Worker2", dummy_model.clone())
        .with_workspace_access(WorkspaceAccessMode::ReadOnly);
    let worker3 = InternalAgentSpec::new(AgentId::new(), "Worker3", "Worker3", dummy_model)
        .with_workspace_access(WorkspaceAccessMode::ReadOnly);

    let worker1_id = worker1.agent_id;
    let worker2_id = worker2.agent_id;

    let arbitrator = WorkspacePolicyArbitrator::new();
    let executor = MockAgentExecutor::new();

    let studio = cp_handle.create_studio("Cancel Studio").await.unwrap();

    let team = InternalTeamSpec::new(studio.id, "cancellation-team", coord)
        .with_max_parallel_agents(2)
        .add_agent("w1", worker1)
        .unwrap()
        .add_agent("w2", worker2)
        .unwrap()
        .add_agent("w3", worker3)
        .unwrap();

    let (historical, mut event_rx) = cp_handle.subscribe_events(studio.id, 1).await.unwrap();

    let supervisor =
        AgentStudiosSupervisor::new(cp_handle.clone(), team, arbitrator, executor.clone())
            .with_failure_policy(FailurePolicy::FailFast);

    let plan = CoordinatorDecision::Plan {
        tasks: vec![
            PlannedTask {
                task_key: "failing_task".to_string(),
                title: "Failing Task".to_string(),
                description: None,
                assigned_alias: "w1".to_string(),
                depends_on: vec![],
                workspace_access: Some(WorkspaceAccessMode::ReadOnly),
                priority: Some(20),
                ..Default::default()
            },
            PlannedTask {
                task_key: "slow_concurrent_task".to_string(),
                title: "Slow Concurrent Task".to_string(),
                description: None,
                assigned_alias: "w2".to_string(),
                depends_on: vec![],
                workspace_access: Some(WorkspaceAccessMode::ReadOnly),
                priority: Some(10),
                ..Default::default()
            },
            PlannedTask {
                task_key: "queued_dependent_task".to_string(),
                title: "Queued Dependent Task".to_string(),
                description: None,
                assigned_alias: "w3".to_string(),
                depends_on: vec!["failing_task".to_string()],
                workspace_access: Some(WorkspaceAccessMode::ReadOnly),
                priority: Some(5),
                ..Default::default()
            },
        ],
    };

    executor.set_handler(move |ctx| {
        if ctx.agent_spec.role == "Coordinator" {
            Ok(AgentExecutionResult {
                output: serde_json::to_string(&plan).unwrap(),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 0,
                success: true,
            })
        } else if ctx.agent_spec.display_name == "Worker1" {
            // Task 1 fails fast immediately
            Ok(AgentExecutionResult {
                output: "Fatal unrecoverable error".to_string(),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 0,
                success: false,
            })
        } else if ctx.agent_spec.display_name == "Worker2" {
            // Task 2 runs slower and will be interrupted
            std::thread::sleep(Duration::from_millis(500));
            Ok(AgentExecutionResult {
                output: "Finished slowly".to_string(),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 0,
                success: true,
            })
        } else {
            Ok(AgentExecutionResult {
                output: "Worker3 output".to_string(),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 0,
                success: true,
            })
        }
    });

    let summary = supervisor.run("Run fail fast test").await.unwrap();

    // Verification 1: Summary counters
    assert_eq!(summary.total_tasks, 3);
    assert_eq!(summary.failed_tasks, 1);
    assert_eq!(summary.cancelled_tasks, 2); // both concurrent running task and queued task are cancelled

    // Verification 2: Check executor received cancellation for the running agent
    let cancelled = executor.cancelled_agents();
    assert!(
        cancelled.contains(&worker2_id) || cancelled.contains(&worker1_id),
        "Active running agents were not interrupted upon FailFast: {:?}",
        cancelled
    );

    // Verification 3: Verify subscription stream captured cancellation event
    let mut saw_cancellation_requested = false;
    let mut saw_task_cancelled = false;

    let mut all_events: Vec<ControlPlaneEvent> = historical.into_iter().map(|e| e.event).collect();
    while let Ok(envelope) = event_rx.try_recv() {
        all_events.push(envelope.event);
    }

    for event in all_events {
        match event {
            ControlPlaneEvent::CancellationRequested { .. } => {
                saw_cancellation_requested = true;
            }
            ControlPlaneEvent::TaskStateChanged {
                new_state: TaskState::Cancelled,
                ..
            } => {
                saw_task_cancelled = true;
            }
            _ => {}
        }
    }

    assert!(
        saw_cancellation_requested,
        "CancellationRequested event missing from event stream"
    );
    assert!(
        saw_task_cancelled,
        "TaskStateTransition to Cancelled missing from event stream"
    );
}
