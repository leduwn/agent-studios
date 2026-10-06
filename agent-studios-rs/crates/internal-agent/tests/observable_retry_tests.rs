use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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

#[tokio::test]
async fn test_observable_retry_state_transition_and_events() {
    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    let (cp_handle, _actor_task) = ControlPlaneActor::spawn(cp);

    let dummy_model = ModelRef::new(
        ProviderInstanceId::new(),
        ModelId::new("test-model").unwrap(),
    );
    let coord = InternalAgentSpec::new(AgentId::new(), "Coord", "Coordinator", dummy_model.clone());
    let coder = InternalAgentSpec::new(AgentId::new(), "Coder", "Developer", dummy_model)
        .with_workspace_access(WorkspaceAccessMode::Mutating);

    let arbitrator = WorkspacePolicyArbitrator::new();
    let executor = MockAgentExecutor::new();

    let studio = cp_handle.create_studio("Retry Studio").await.unwrap();

    let team = InternalTeamSpec::new(studio.id, "retry-team", coord)
        .add_agent("coder", coder)
        .unwrap();

    let (historical, mut event_rx) = cp_handle.subscribe_events(studio.id, 1).await.unwrap();

    let supervisor =
        AgentStudiosSupervisor::new(cp_handle.clone(), team, arbitrator, executor.clone())
            .with_failure_policy(FailurePolicy::RetryTask(2));

    let plan = CoordinatorDecision::Plan {
        tasks: vec![PlannedTask {
            task_key: "flaky_task".to_string(),
            title: "Flaky Task".to_string(),
            description: None,
            assigned_alias: "coder".to_string(),
            depends_on: vec![],
            workspace_access: Some(WorkspaceAccessMode::Mutating),
            priority: Some(10),
            ..Default::default()
        }],
    };

    let attempt_counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = Arc::clone(&attempt_counter);

    executor.set_handler(move |ctx| {
        if ctx.agent_spec.role == "Coordinator" {
            Ok(AgentExecutionResult {
                output: serde_json::to_string(&plan).unwrap(),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 0,
                success: true,
            })
        } else {
            let attempt = counter_clone.fetch_add(1, Ordering::SeqCst);
            if attempt == 0 {
                // Fail first attempt
                Ok(AgentExecutionResult {
                    output: "Network blip on first attempt".to_string(),
                    turns_used: 1,
                    tool_calls_used: 0,
                    duration_secs: 0,
                    success: false,
                })
            } else {
                // Succeed on retry
                Ok(AgentExecutionResult {
                    output: "Success on second attempt".to_string(),
                    turns_used: 1,
                    tool_calls_used: 0,
                    duration_secs: 0,
                    success: true,
                })
            }
        }
    });

    let summary = supervisor.run("Run flaky pipeline").await.unwrap();

    assert_eq!(summary.total_tasks, 1);
    assert_eq!(summary.completed_tasks, 1);
    assert_eq!(summary.failed_tasks, 0);
    assert_eq!(attempt_counter.load(Ordering::SeqCst), 2);

    // Verify observable events in stream
    let mut saw_task_retrying = false;
    let mut saw_task_retry_scheduled = false;
    let mut recorded_retry_count = 0;

    let mut all_events: Vec<ControlPlaneEvent> = historical.into_iter().map(|e| e.event).collect();
    while let Ok(envelope) = event_rx.try_recv() {
        all_events.push(envelope.event);
    }

    for event in all_events {
        match event {
            ControlPlaneEvent::TaskStateChanged {
                new_state: TaskState::Retrying,
                ..
            } => {
                saw_task_retrying = true;
            }
            ControlPlaneEvent::TaskRetryScheduled { attempt, .. } => {
                saw_task_retry_scheduled = true;
                recorded_retry_count = attempt;
            }
            _ => {}
        }
    }

    assert!(
        saw_task_retrying,
        "TaskStateTransition to Retrying missing from event stream"
    );
    assert!(
        saw_task_retry_scheduled,
        "TaskRetryScheduled event missing from event stream"
    );
    assert_eq!(recorded_retry_count, 1, "Expected retry count 1 in event");
}

#[tokio::test]
async fn test_bounded_retry_count_exhaustion() {
    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    let (cp_handle, _actor_task) = ControlPlaneActor::spawn(cp);

    let dummy_model = ModelRef::new(
        ProviderInstanceId::new(),
        ModelId::new("test-model").unwrap(),
    );
    let coord = InternalAgentSpec::new(AgentId::new(), "Coord", "Coordinator", dummy_model.clone());
    let coder = InternalAgentSpec::new(AgentId::new(), "Coder", "Developer", dummy_model)
        .with_workspace_access(WorkspaceAccessMode::Mutating);

    let studio = cp_handle.create_studio("Exhaustion Studio").await.unwrap();

    let team = InternalTeamSpec::new(studio.id, "exhaustion-team", coord)
        .add_agent("coder", coder)
        .unwrap();

    let arbitrator = WorkspacePolicyArbitrator::new();
    let executor = MockAgentExecutor::new();

    let supervisor =
        AgentStudiosSupervisor::new(cp_handle.clone(), team, arbitrator, executor.clone())
            .with_failure_policy(FailurePolicy::RetryTask(2));

    let plan = CoordinatorDecision::Plan {
        tasks: vec![PlannedTask {
            task_key: "permanent_fail".to_string(),
            title: "Permanent Fail".to_string(),
            description: None,
            assigned_alias: "coder".to_string(),
            depends_on: vec![],
            workspace_access: Some(WorkspaceAccessMode::Mutating),
            priority: Some(10),
            ..Default::default()
        }],
    };

    let attempt_counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = Arc::clone(&attempt_counter);

    executor.set_handler(move |ctx| {
        if ctx.agent_spec.role == "Coordinator" {
            Ok(AgentExecutionResult {
                output: serde_json::to_string(&plan).unwrap(),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 0,
                success: true,
            })
        } else {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            // Always fails
            Ok(AgentExecutionResult {
                output: "Permanent failure".to_string(),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 0,
                success: false,
            })
        }
    });

    let summary = supervisor
        .run("Run permanently failing task")
        .await
        .unwrap();

    assert_eq!(summary.total_tasks, 1);
    assert_eq!(summary.completed_tasks, 0);
    assert_eq!(summary.failed_tasks, 1);
    // Initial attempt + 2 retries = 3 attempts total
    assert_eq!(
        attempt_counter.load(Ordering::SeqCst),
        3,
        "Expected exactly 3 attempts (1 initial + 2 retries)"
    );
}
