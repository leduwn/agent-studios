use agent_studios_control_plane::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::store::InMemoryStore;
use agent_studios_internal_agent::{ControlPlaneActor, ControlPlaneHandle};
use agent_studios_protocol::agent::AgentKind;
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::TaskState;

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
