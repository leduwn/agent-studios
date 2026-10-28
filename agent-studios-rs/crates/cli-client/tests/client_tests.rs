use std::fs;
use std::process::Command;
use std::sync::Arc;
use tempfile::tempdir;

use agent_studios_cli_client::{
    AgentStudiosClient, ClientError, EmbeddedClient, ExecutionMode, GoalSubmissionOptions,
};
use agent_studios_protocol::id::{ReconciliationId, WorktreeId};
use agent_studios_workspace::WorkspaceOrchestrator;

fn init_git_repo(path: &std::path::Path) -> String {
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(path)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("Failed to run git {:?}: {e}", args));
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    run(&["init"]);
    run(&["config", "user.name", "Agent Studios Test"]);
    run(&["config", "user.email", "test@agent-studios.internal"]);
    run(&["config", "commit.gpgsign", "false"]);

    fs::write(path.join("file_in_main.txt"), "Initial main content\n").unwrap();
    let nested = path.join("agent-studios-rs");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("README.md"), "Nested workspace\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-m", "Initial commit"]);

    run(&["rev-parse", "HEAD"])
}

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
    let source_dir = tempdir().expect("source tempdir");
    let base_sha = init_git_repo(source_dir.path());
    assert!(!base_sha.is_empty());

    // Nested workspace directory inside source repo (e.g. agent-studios-rs)
    let nested_dir = source_dir.path().join("agent-studios-rs");

    // Add untracked dirty file in source to verify dirty source preservation
    let dirty_file = source_dir.path().join("dirty_untracked.txt");
    fs::write(&dirty_file, "uncommitted source state\n").expect("write dirty file");

    let orchestrator_dir = tempdir().expect("orchestrator tempdir");
    let wt_root = orchestrator_dir.path().join("worktrees");
    let art_root = orchestrator_dir.path().join("artifacts");

    let orchestrator = Arc::new(
        WorkspaceOrchestrator::new(wt_root, art_root).expect("orchestrator initialization"),
    );

    let client = EmbeddedClient::bootstrap("test-workspace")
        .await
        .expect("bootstrap failed")
        .with_orchestrator(orchestrator)
        .with_working_dir(&nested_dir);

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

    let worktrees = client
        .list_worktrees()
        .await
        .expect("list_worktrees failed");
    assert_eq!(worktrees.len(), 1);

    // Verify dirty source preservation invariant
    assert!(
        dirty_file.exists(),
        "Untracked dirty file in source must be preserved"
    );
    let dirty_content = fs::read_to_string(&dirty_file).expect("read dirty file");
    assert_eq!(dirty_content, "uncommitted source state\n");

    // Verify tracked file in source unchanged
    let main_content =
        fs::read_to_string(source_dir.path().join("file_in_main.txt")).expect("read main file");
    assert_eq!(main_content, "Initial main content\n");
}
