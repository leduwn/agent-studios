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
use agent_studios_protocol::id::AgentId;
use agent_studios_provider::id::{ModelId, ProviderInstanceId};
use agent_studios_provider::model::ModelRef;

async fn setup_test_supervisor() -> (AgentStudiosSupervisor<MockAgentExecutor>, MockAgentExecutor) {
    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    let (cp_handle, _task) = ControlPlaneActor::spawn(cp);

    let dummy_model = ModelRef::new(
        ProviderInstanceId::new(),
        ModelId::new("test-model").unwrap(),
    );
    let coord = InternalAgentSpec::new(AgentId::new(), "Coord", "Coordinator", dummy_model.clone());
    let coder = InternalAgentSpec::new(AgentId::new(), "Coder", "Developer", dummy_model.clone())
        .with_workspace_access(WorkspaceAccessMode::Mutating);
    let reviewer = InternalAgentSpec::new(AgentId::new(), "Reviewer", "Auditor", dummy_model)
        .with_workspace_access(WorkspaceAccessMode::ReadOnly);

    let arbitrator = WorkspacePolicyArbitrator::new();
    let executor = MockAgentExecutor::new();

    let studio = cp_handle.create_studio("Test Studio").await.unwrap();

    let team = InternalTeamSpec::new(studio.id, "test-team", coord)
        .add_agent("coder", coder)
        .unwrap()
        .add_agent("reviewer", reviewer)
        .unwrap();

    let supervisor = AgentStudiosSupervisor::new(cp_handle, team, arbitrator, executor.clone());

    (supervisor, executor)
}

#[tokio::test]
async fn test_supervisor_full_success_workflow() {
    let (supervisor, executor) = setup_test_supervisor().await;

    // Configure coordinator to return a 2-task plan
    let plan = CoordinatorDecision::Plan {
        tasks: vec![
            PlannedTask {
                task_key: "code".to_string(),
                title: "Write feature code".to_string(),
                description: None,
                assigned_alias: "coder".to_string(),
                depends_on: vec![],
                workspace_access: None,
                priority: None,
                ..Default::default()
            },
            PlannedTask {
                task_key: "review".to_string(),
                title: "Review feature code".to_string(),
                description: None,
                assigned_alias: "reviewer".to_string(),
                depends_on: vec!["code".to_string()],
                workspace_access: None,
                priority: None,
                ..Default::default()
            },
        ],
    };

    executor.set_result_for_role(
        "Coordinator",
        AgentExecutionResult {
            output: serde_json::to_string(&plan).unwrap(),
            turns_used: 1,
            tool_calls_used: 0,
            duration_secs: 1,
            success: true,
        },
    );

    let summary = supervisor
        .run("Implement user authentication")
        .await
        .unwrap();

    assert_eq!(summary.total_tasks, 2);
    assert_eq!(summary.completed_tasks, 2);
    assert_eq!(summary.failed_tasks, 0);
    assert_eq!(summary.cancelled_tasks, 0);
    assert!(summary.coordinator_summary.is_some());
}

#[tokio::test]
async fn test_supervisor_retry_policy() {
    let (supervisor, executor) = setup_test_supervisor().await;
    let supervisor = supervisor.with_failure_policy(FailurePolicy::RetryTask(2));

    let plan = CoordinatorDecision::Plan {
        tasks: vec![PlannedTask {
            task_key: "flaky".to_string(),
            title: "Flaky task".to_string(),
            description: None,
            assigned_alias: "coder".to_string(),
            depends_on: vec![],
            workspace_access: None,
            priority: None,
            ..Default::default()
        }],
    };

    executor.set_result_for_role(
        "Coordinator",
        AgentExecutionResult {
            output: serde_json::to_string(&plan).unwrap(),
            turns_used: 1,
            tool_calls_used: 0,
            duration_secs: 1,
            success: true,
        },
    );

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
                // Fail on first attempt
                Ok(AgentExecutionResult {
                    output: "Flaky network error".to_string(),
                    turns_used: 1,
                    tool_calls_used: 0,
                    duration_secs: 0,
                    success: false,
                })
            } else {
                // Succeed on second attempt
                Ok(AgentExecutionResult {
                    output: "Success on retry".to_string(),
                    turns_used: 1,
                    tool_calls_used: 0,
                    duration_secs: 0,
                    success: true,
                })
            }
        }
    });

    let summary = supervisor.run("Retry task").await.unwrap();
    assert_eq!(summary.total_tasks, 1);
    assert_eq!(summary.completed_tasks, 1);
    assert_eq!(summary.failed_tasks, 0);
    assert_eq!(attempt_counter.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_supervisor_fail_fast_policy() {
    let (supervisor, executor) = setup_test_supervisor().await;
    let supervisor = supervisor.with_failure_policy(FailurePolicy::FailFast);

    let plan = CoordinatorDecision::Plan {
        tasks: vec![
            PlannedTask {
                task_key: "broken".to_string(),
                title: "Broken task".to_string(),
                description: None,
                assigned_alias: "coder".to_string(),
                depends_on: vec![],
                workspace_access: None,
                priority: None,
                ..Default::default()
            },
            PlannedTask {
                task_key: "dependent".to_string(),
                title: "Dependent task".to_string(),
                description: None,
                assigned_alias: "reviewer".to_string(),
                depends_on: vec!["broken".to_string()],
                workspace_access: None,
                priority: None,
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
        } else if ctx.agent_spec.role == "Developer" {
            // Task fails
            Ok(AgentExecutionResult {
                output: "Fatal compile error".to_string(),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 0,
                success: false,
            })
        } else {
            Ok(AgentExecutionResult {
                output: "Reviewer never reached".to_string(),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 0,
                success: true,
            })
        }
    });

    let summary = supervisor.run("Failing pipeline").await.unwrap();
    assert_eq!(summary.total_tasks, 2);
    assert_eq!(summary.completed_tasks, 0);
    assert_eq!(summary.failed_tasks, 1);
    assert_eq!(summary.cancelled_tasks, 1);
}
