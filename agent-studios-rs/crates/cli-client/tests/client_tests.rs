use agent_studios_cli_client::{
    AgentStudiosClient, ClientError, EmbeddedClient, ExecutionMode, GoalSubmissionOptions,
};
use agent_studios_protocol::id::{ReconciliationId, WorktreeId};

#[tokio::test]
async fn test_reconcile_patch_fails_closed_unsupported() {
    let client = EmbeddedClient::bootstrap("test-workspace")
        .await
        .expect("bootstrap failed");

    let rec_id = ReconciliationId::new();
    let result = client.reconcile_patch(rec_id).await;

    match result {
        Err(ClientError::Unsupported(msg)) => {
            assert_eq!(
                msg,
                "Git reconciliation engine is not attached in embedded mode; requires external workspace reconciler"
            );
        }
        other => panic!("expected Err(ClientError::Unsupported), got {other:?}"),
    }
}

#[tokio::test]
async fn test_get_worktree_diff_fails_closed_unsupported() {
    let client = EmbeddedClient::bootstrap("test-workspace")
        .await
        .expect("bootstrap failed");

    let wt_id = WorktreeId::new();
    let result = client.get_worktree_diff(wt_id).await;

    match result {
        Err(ClientError::Unsupported(msg)) => {
            assert_eq!(
                msg,
                "Worktree diff inspection is not attached in embedded mode"
            );
        }
        other => panic!("expected Err(ClientError::Unsupported), got {other:?}"),
    }
}

#[tokio::test]
async fn test_workspace_name_and_info() {
    let client = EmbeddedClient::bootstrap("custom-workspace")
        .await
        .expect("bootstrap failed");

    assert_eq!(client.workspace_name(), "custom-workspace");
    assert_eq!(client.execution_mode(), ExecutionMode::Embedded);

    let info = client
        .get_workspace_info()
        .await
        .expect("get_workspace_info failed");

    assert_eq!(info.project_name, "custom-workspace");
    assert!(!info.cwd.is_empty());
    // Since we are running in a git repository, branch_name should be detected
    assert!(info.branch_name.is_some());
}

#[tokio::test]
async fn test_submit_goal_and_task_graph() {
    let client = EmbeddedClient::bootstrap("test-workspace")
        .await
        .expect("bootstrap failed");

    let task_id = client
        .submit_goal("implement feature A", GoalSubmissionOptions::default())
        .await
        .expect("submit_goal failed");

    let graph = client
        .get_task_graph()
        .await
        .expect("get_task_graph failed");
    assert_eq!(graph.tasks.len(), 1);
    assert!(graph.tasks.iter().any(|t| t.id == task_id));

    let agents = client
        .get_agent_summaries()
        .await
        .expect("get_agent_summaries failed");
    assert_eq!(agents.len(), 4);
}
