use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use agent_studios_control_plane::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::store::InMemoryStore;
use agent_studios_internal_agent::{
    AgentExecutionResult, AgentStudiosSupervisor, ControlPlaneActor, CoordinatorDecision,
    InternalAgentSpec, InternalTeamSpec, MockAgentExecutor, PlannedTask, WorkspaceAccessMode,
    WorkspacePolicyArbitrator,
};
use agent_studios_protocol::id::AgentId;
use agent_studios_provider::id::{ModelId, ProviderInstanceId};
use agent_studios_provider::model::ModelRef;
use std::sync::Mutex;

async fn setup_parallel_test_env(
    max_parallel: usize,
) -> (
    AgentStudiosSupervisor<MockAgentExecutor>,
    MockAgentExecutor,
    agent_studios_internal_agent::ControlPlaneHandle,
) {
    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    let (cp_handle, _task) = ControlPlaneActor::spawn(cp);

    let dummy_model = ModelRef::new(
        ProviderInstanceId::new(),
        ModelId::new("test-model").unwrap(),
    );
    let coord = InternalAgentSpec::new(AgentId::new(), "Coord", "Coordinator", dummy_model.clone());
    let worker1 = InternalAgentSpec::new(AgentId::new(), "Worker1", "Worker", dummy_model.clone())
        .with_workspace_access(WorkspaceAccessMode::Mutating);
    let worker2 = InternalAgentSpec::new(AgentId::new(), "Worker2", "Worker", dummy_model.clone())
        .with_workspace_access(WorkspaceAccessMode::ReadOnly);
    let worker3 = InternalAgentSpec::new(AgentId::new(), "Worker3", "Worker", dummy_model)
        .with_workspace_access(WorkspaceAccessMode::Mutating);

    let arbitrator = WorkspacePolicyArbitrator::new();
    let executor = MockAgentExecutor::new();

    let studio = cp_handle.create_studio("Parallel Studio").await.unwrap();

    let team = InternalTeamSpec::new(studio.id, "parallel-team", coord)
        .with_max_parallel_agents(max_parallel)
        .add_agent("w1", worker1)
        .unwrap()
        .add_agent("w2", worker2)
        .unwrap()
        .add_agent("w3", worker3)
        .unwrap();

    let supervisor =
        AgentStudiosSupervisor::new(cp_handle.clone(), team, arbitrator, executor.clone());

    (supervisor, executor, cp_handle)
}

#[tokio::test]
async fn test_max_parallel_agents_concurrency_limit() {
    let (supervisor, executor, _cp) = setup_parallel_test_env(2).await;

    // 4 independent tasks
    let plan = CoordinatorDecision::Plan {
        tasks: vec![
            PlannedTask {
                task_key: "t1".to_string(),
                title: "Task 1".to_string(),
                description: None,
                assigned_alias: "w1".to_string(),
                depends_on: vec![],
                workspace_access: Some(WorkspaceAccessMode::ReadOnly),
                priority: None,
            },
            PlannedTask {
                task_key: "t2".to_string(),
                title: "Task 2".to_string(),
                description: None,
                assigned_alias: "w2".to_string(),
                depends_on: vec![],
                workspace_access: Some(WorkspaceAccessMode::ReadOnly),
                priority: None,
            },
            PlannedTask {
                task_key: "t3".to_string(),
                title: "Task 3".to_string(),
                description: None,
                assigned_alias: "w1".to_string(),
                depends_on: vec![],
                workspace_access: Some(WorkspaceAccessMode::ReadOnly),
                priority: None,
            },
            PlannedTask {
                task_key: "t4".to_string(),
                title: "Task 4".to_string(),
                description: None,
                assigned_alias: "w2".to_string(),
                depends_on: vec![],
                workspace_access: Some(WorkspaceAccessMode::ReadOnly),
                priority: None,
            },
        ],
    };

    let current_concurrency = Arc::new(AtomicUsize::new(0));
    let max_concurrency = Arc::new(AtomicUsize::new(0));

    let cur_c = Arc::clone(&current_concurrency);
    let max_c = Arc::clone(&max_concurrency);

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
            let active = cur_c.fetch_add(1, Ordering::SeqCst) + 1;
            max_c.fetch_max(active, Ordering::SeqCst);
            // Brief work simulation
            std::thread::sleep(Duration::from_millis(50));
            cur_c.fetch_sub(1, Ordering::SeqCst);

            Ok(AgentExecutionResult {
                output: format!("Completed {}", ctx.agent_spec.display_name),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 0,
                success: true,
            })
        }
    });

    let summary = supervisor.run("Run parallel batch").await.unwrap();

    assert_eq!(summary.total_tasks, 4);
    assert_eq!(summary.completed_tasks, 4);
    assert_eq!(summary.failed_tasks, 0);

    let observed_max = max_concurrency.load(Ordering::SeqCst);
    assert!(
        observed_max <= 2,
        "Observed concurrency {} exceeded max_parallel_agents 2",
        observed_max
    );
    assert!(
        observed_max >= 1,
        "Concurrency should be at least 1, got {}",
        observed_max
    );
}

#[tokio::test]
async fn test_deterministic_candidate_sorting_by_priority() {
    // max_parallel = 1 forces strict serial dispatch so we can verify order exactly
    let (supervisor, executor, _cp) = setup_parallel_test_env(1).await;

    let plan = CoordinatorDecision::Plan {
        tasks: vec![
            PlannedTask {
                task_key: "low_pri".to_string(),
                title: "Low Priority".to_string(),
                description: None,
                assigned_alias: "w1".to_string(),
                depends_on: vec![],
                workspace_access: Some(WorkspaceAccessMode::ReadOnly),
                priority: Some(10),
            },
            PlannedTask {
                task_key: "high_pri".to_string(),
                title: "High Priority".to_string(),
                description: None,
                assigned_alias: "w2".to_string(),
                depends_on: vec![],
                workspace_access: Some(WorkspaceAccessMode::ReadOnly),
                priority: Some(100),
            },
            PlannedTask {
                task_key: "default_pri".to_string(),
                title: "Default Priority".to_string(),
                description: None,
                assigned_alias: "w1".to_string(),
                depends_on: vec![],
                workspace_access: Some(WorkspaceAccessMode::ReadOnly),
                priority: None, // defaults to 0
            },
            PlannedTask {
                task_key: "mid_pri".to_string(),
                title: "Mid Priority".to_string(),
                description: None,
                assigned_alias: "w2".to_string(),
                depends_on: vec![],
                workspace_access: Some(WorkspaceAccessMode::ReadOnly),
                priority: Some(50),
            },
        ],
    };

    let dispatch_order = Arc::new(Mutex::new(Vec::new()));
    let order_clone = Arc::clone(&dispatch_order);

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
            let prompt = ctx.prompt.clone();
            // Capture task title or identifier
            let mut list = order_clone.lock().unwrap();
            list.push(prompt);

            Ok(AgentExecutionResult {
                output: "Done".to_string(),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 0,
                success: true,
            })
        }
    });

    let summary = supervisor.run("Run priority sorted batch").await.unwrap();
    assert_eq!(summary.completed_tasks, 4);

    let recorded = dispatch_order.lock().unwrap().clone();
    assert_eq!(recorded.len(), 4);
    // Order must be: high_pri (100) -> mid_pri (50) -> low_pri (10) -> default_pri (0)
    assert!(recorded[0].contains("High Priority"));
    assert!(recorded[1].contains("Mid Priority"));
    assert!(recorded[2].contains("Low Priority"));
    assert!(recorded[3].contains("Default Priority"));
}

#[tokio::test]
async fn test_workspace_policy_isolation_single_writer_exclusivity() {
    // max_parallel = 4, but tasks both require Mutating access to the workspace
    let (supervisor, executor, _cp) = setup_parallel_test_env(4).await;

    let plan = CoordinatorDecision::Plan {
        tasks: vec![
            PlannedTask {
                task_key: "mutator_1".to_string(),
                title: "Mutator 1".to_string(),
                description: None,
                assigned_alias: "w1".to_string(),
                depends_on: vec![],
                workspace_access: Some(WorkspaceAccessMode::Mutating),
                priority: Some(10),
            },
            PlannedTask {
                task_key: "mutator_2".to_string(),
                title: "Mutator 2".to_string(),
                description: None,
                assigned_alias: "w3".to_string(),
                depends_on: vec![],
                workspace_access: Some(WorkspaceAccessMode::Mutating),
                priority: Some(10),
            },
        ],
    };

    let active_mutators = Arc::new(AtomicUsize::new(0));
    let max_concurrent_mutators = Arc::new(AtomicUsize::new(0));

    let act_m = Arc::clone(&active_mutators);
    let max_m = Arc::clone(&max_concurrent_mutators);

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
            let count = act_m.fetch_add(1, Ordering::SeqCst) + 1;
            max_m.fetch_max(count, Ordering::SeqCst);
            // Simulate work
            std::thread::sleep(Duration::from_millis(60));
            act_m.fetch_sub(1, Ordering::SeqCst);

            Ok(AgentExecutionResult {
                output: "Mutated successfully".to_string(),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 0,
                success: true,
            })
        }
    });

    let summary = supervisor
        .run("Run mutating exclusivity test")
        .await
        .unwrap();

    assert_eq!(summary.total_tasks, 2);
    assert_eq!(summary.completed_tasks, 2);
    assert_eq!(summary.failed_tasks, 0);

    // Because single-writer workspace exclusivity is enforced, max concurrent mutators must never exceed 1
    let observed_max = max_concurrent_mutators.load(Ordering::SeqCst);
    assert_eq!(
        observed_max, 1,
        "Single-writer exclusivity violated! Max concurrent mutators was {}",
        observed_max
    );
}
