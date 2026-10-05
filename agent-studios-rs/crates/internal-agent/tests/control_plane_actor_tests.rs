use agent_studios_control_plane::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::store::InMemoryStore;
use agent_studios_internal_agent::{ControlPlaneActor, ControlPlaneHandle};
use agent_studios_protocol::agent::{AgentKind, AgentState};
use agent_studios_protocol::event::ControlPlaneEvent;
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::{BatchTaskSpec, TaskState};
use chrono::Utc;

fn create_test_actor() -> (ControlPlaneHandle, tokio::task::JoinHandle<()>) {
    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    ControlPlaneActor::spawn(cp)
}

#[tokio::test]
async fn test_actor_lifecycle_and_task_dependency_resolution() {
    let (handle, _task) = create_test_actor();

    let studio = handle.create_studio("Test Studio").await.unwrap();
    let agent1 = handle
        .register_agent(
            studio.id,
            "Coder",
            AgentKind::Internal,
            Some("Coder".into()),
        )
        .await
        .unwrap();

    // Create Task 1 (no dependencies -> Ready)
    let task1 = handle
        .create_task(
            studio.id,
            "Task 1",
            "Initial task",
            None,
            Some(agent1.id),
            vec![],
        )
        .await
        .unwrap();
    assert_eq!(task1.state, TaskState::Ready);

    // Create Task 2 (depends on Task 1 -> Blocked)
    let task2 = handle
        .create_task(
            studio.id,
            "Task 2",
            "Follow-up task",
            None,
            Some(agent1.id),
            vec![task1.id],
        )
        .await
        .unwrap();
    assert_eq!(task2.state, TaskState::Blocked);

    let ready_tasks = handle.get_ready_tasks(studio.id).await.unwrap();
    assert_eq!(ready_tasks.len(), 1);
    assert_eq!(ready_tasks[0].id, task1.id);

    // Execute Task 1: transition Ready -> Running -> Succeeded
    handle
        .transition_task_state(task1.id, TaskState::Running)
        .await
        .unwrap();
    handle
        .transition_task_state(task1.id, TaskState::Succeeded)
        .await
        .unwrap();

    // Task 2 should now be automatically unblocked and Ready!
    let task2_updated = handle.get_task(task2.id).await.unwrap().unwrap();
    assert_eq!(task2_updated.state, TaskState::Ready);

    let ready_tasks_after = handle.get_ready_tasks(studio.id).await.unwrap();
    assert_eq!(ready_tasks_after.len(), 1);
    assert_eq!(ready_tasks_after[0].id, task2.id);
}

#[tokio::test]
async fn test_actor_run_lifecycle() {
    let (handle, _task) = create_test_actor();

    let studio = handle.create_studio("Studio Runs").await.unwrap();
    let agent = handle
        .register_agent(studio.id, "Runner", AgentKind::Internal, None)
        .await
        .unwrap();
    let task = handle
        .create_task(studio.id, "Run Task", "", None, Some(agent.id), vec![])
        .await
        .unwrap();

    let run = handle.create_run(task.id, agent.id).await.unwrap();
    assert_eq!(run.state, RunState::Queued);

    handle
        .transition_run_state(run.id, RunState::Starting)
        .await
        .unwrap();
    handle
        .transition_run_state(run.id, RunState::Running)
        .await
        .unwrap();
    handle
        .transition_run_state(run.id, RunState::Succeeded)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_actor_concurrent_handle_access() {
    let (handle, _task) = create_test_actor();
    let studio = handle.create_studio("Concurrent Studio").await.unwrap();

    let mut handles = Vec::new();
    for i in 0..10 {
        let h = handle.clone();
        let sid = studio.id;
        handles.push(tokio::spawn(async move {
            let agent = h
                .register_agent(sid, format!("Agent_{i}"), AgentKind::Internal, None)
                .await
                .unwrap();
            h.create_task(sid, format!("Task_{i}"), "", None, Some(agent.id), vec![])
                .await
                .unwrap();
        }));
    }

    for jh in handles {
        jh.await.unwrap();
    }

    let all_tasks = handle.get_studio_tasks(studio.id).await.unwrap();
    assert_eq!(all_tasks.len(), 10);
}

#[tokio::test]
async fn test_actor_agent_state_transition() {
    let (handle, _task) = create_test_actor();
    let studio = handle.create_studio("Agent State Studio").await.unwrap();
    let agent = handle
        .register_agent(studio.id, "WorkerAgent", AgentKind::Internal, None)
        .await
        .unwrap();

    assert_eq!(agent.state, AgentState::Registered);

    handle
        .transition_agent_state(agent.id, AgentState::Starting)
        .await
        .unwrap();
    handle
        .transition_agent_state(agent.id, AgentState::Idle)
        .await
        .unwrap();
    handle
        .transition_agent_state(agent.id, AgentState::Busy)
        .await
        .unwrap();

    let state = handle.get_state().await.unwrap();
    let updated_agent = state.agents.get(&agent.id).unwrap();
    assert_eq!(updated_agent.state, AgentState::Busy);
}

#[tokio::test]
async fn test_actor_task_batch_and_live_event_subscription() {
    let (handle, _task) = create_test_actor();
    let studio = handle.create_studio("Subscription Studio").await.unwrap();

    // 1. Initial event subscription: historical should have studio created (seq 1)
    let (historical, mut live_rx) = handle.subscribe_events(studio.id, 1).await.unwrap();
    assert_eq!(historical.len(), 1);
    assert_eq!(historical[0].sequence, 1);

    // 2. Dispatch CreateTaskBatch
    let batch = vec![
        BatchTaskSpec {
            key: "step_1".to_string(),
            title: "Step 1".to_string(),
            description: "First step".to_string(),
            assigned_agent_id: None,
            parent_task_key: None,
            parent_task_id: None,
            dependency_keys: vec![],
            dependency_task_ids: vec![],
        },
        BatchTaskSpec {
            key: "step_2".to_string(),
            title: "Step 2".to_string(),
            description: "Second step".to_string(),
            assigned_agent_id: None,
            parent_task_key: None,
            parent_task_id: None,
            dependency_keys: vec!["step_1".to_string()],
            dependency_task_ids: vec![],
        },
    ];

    let tasks = handle.create_task_batch(studio.id, batch).await.unwrap();
    assert_eq!(tasks.len(), 2);

    // 3. Receive live events from subscription
    let env1 = live_rx.recv().await.unwrap();
    let env2 = live_rx.recv().await.unwrap();

    assert_eq!(env1.sequence, 2);
    assert_eq!(env2.sequence, 3);
    assert!(matches!(env1.event, ControlPlaneEvent::TaskCreated { .. }));
    assert!(matches!(env2.event, ControlPlaneEvent::TaskCreated { .. }));
}

#[tokio::test]
async fn test_actor_record_observability_events() {
    let (handle, _task) = create_test_actor();
    let studio = handle.create_studio("Observability Studio").await.unwrap();
    let agent = handle
        .register_agent(studio.id, "ObsAgent", AgentKind::Internal, None)
        .await
        .unwrap();

    let now = Utc::now();

    handle
        .record_runtime_bound(
            studio.id,
            agent.id,
            "codex",
            "provider_1",
            "model_1",
            "openai_responses",
        )
        .await
        .unwrap();

    handle
        .record_agent_spawned(studio.id, agent.id, "thread_123", None, None)
        .await
        .unwrap();

    handle
        .record_tool_started(studio.id, agent.id, None, None, "read_file", "call_1", now)
        .await
        .unwrap();

    handle
        .record_tool_completed(
            studio.id,
            agent.id,
            None,
            None,
            "read_file",
            "call_1",
            42,
            now,
        )
        .await
        .unwrap();

    handle
        .record_budget_usage_updated(studio.id, agent.id, None, 5, 2, 10, 1)
        .await
        .unwrap();

    handle
        .record_budget_exceeded(studio.id, agent.id, None, "tool_calls", 100, 101)
        .await
        .unwrap();

    let (events, _) = handle.subscribe_events(studio.id, 1).await.unwrap();
    // StudioCreated(1) + AgentRegistered(2) + RuntimeBound(3) + Spawned(4) + ToolStarted(5) + ToolCompleted(6) + BudgetUpdated(7) + BudgetExceeded(8)
    assert_eq!(events.len(), 8);
}
