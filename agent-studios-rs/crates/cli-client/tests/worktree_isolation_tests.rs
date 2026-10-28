use std::fs;
use std::process::Command;
use std::sync::Arc;
use tempfile::tempdir;

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
    run(&["add", "file_in_main.txt"]);
    run(&["commit", "-m", "Initial commit"]);

    run(&["rev-parse", "HEAD"])
}

#[tokio::test]
async fn test_physical_worktree_isolation_and_dirty_source_preservation() {
    let source_dir = tempdir().expect("source tempdir");
    let base_sha = init_git_repo(source_dir.path());

    // Create uncommitted dirty state in source repo
    let dirty_file = source_dir.path().join("dirty_file.txt");
    fs::write(&dirty_file, "uncommitted dirty content\n").unwrap();

    let tracked_file = source_dir.path().join("file_in_main.txt");
    fs::write(&tracked_file, "modified uncommitted content\n").unwrap();

    // Verify source is dirty
    let status_out = Command::new("git")
        .current_dir(source_dir.path())
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    let status_str = String::from_utf8_lossy(&status_out.stdout);
    assert!(
        status_str.contains("dirty_file.txt"),
        "Source repository must report untracked dirty file"
    );
    assert!(
        status_str.contains("file_in_main.txt"),
        "Source repository must report modified dirty file"
    );

    // Provision isolated physical worktree via WorkspaceOrchestrator
    let orchestrator_dir = tempdir().expect("orchestrator tempdir");
    let wt_root = orchestrator_dir.path().join("worktrees");
    let art_root = orchestrator_dir.path().join("artifacts");

    let orchestrator =
        WorkspaceOrchestrator::new(wt_root, art_root).expect("orchestrator initialization");

    let managed = orchestrator
        .create_worktree_async(source_dir.path().to_path_buf(), None)
        .await
        .expect("worktree creation must succeed");

    // 1. Invariant: physical worktree root != source root
    assert_ne!(
        managed.root, managed.source_root,
        "Physical worktree root must be distinct from source repository root"
    );

    // 2. Invariant: base SHA matches initial clean commit
    assert_eq!(
        managed.head_sha, base_sha,
        "Isolated worktree head SHA must match base commit"
    );

    // 3. Invariant: isolated worktree does NOT contain uncommitted dirty files
    assert!(
        !managed.root.join("dirty_file.txt").exists(),
        "Isolated worktree must not contain dirty uncommitted files from source"
    );

    // 4. Invariant: isolated worktree contains clean tracked file at base SHA
    let wt_content =
        fs::read_to_string(managed.root.join("file_in_main.txt")).expect("read file in worktree");
    assert_eq!(
        wt_content.trim(),
        "Initial main content",
        "Tracked file in worktree must reflect clean base commit, not dirty modification"
    );

    // 5. Invariant: original source repo dirty changes are untouched and preserved
    let dirty_source_content = fs::read_to_string(&dirty_file).expect("read dirty file in source");
    assert_eq!(
        dirty_source_content.trim(),
        "uncommitted dirty content",
        "Dirty file in source repository must be preserved without data loss"
    );

    let modified_source_content =
        fs::read_to_string(&tracked_file).expect("read modified file in source");
    assert_eq!(
        modified_source_content.trim(),
        "modified uncommitted content",
        "Dirty modification in source repository must be preserved without reversion"
    );

    // 6. Invariant: Git recognizes the allocated directory as a genuine physical worktree
    let rev_parse_out = Command::new("git")
        .current_dir(&managed.root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .expect("git rev-parse");
    assert_eq!(
        String::from_utf8_lossy(&rev_parse_out.stdout).trim(),
        "true",
        "Allocated directory must be recognized as a valid Git worktree"
    );
}

#[tokio::test]
async fn test_worktree_lease_ownership_and_retention() {
    let source_dir = tempdir().expect("source tempdir");
    let _base_sha = init_git_repo(source_dir.path());

    let orchestrator_dir = tempdir().expect("orchestrator tempdir");
    let wt_root = orchestrator_dir.path().join("worktrees");
    let art_root = orchestrator_dir.path().join("artifacts");

    let orchestrator = Arc::new(
        WorkspaceOrchestrator::new(wt_root, art_root).expect("orchestrator initialization"),
    );

    let managed = orchestrator
        .create_worktree_async(source_dir.path().to_path_buf(), None)
        .await
        .expect("worktree creation must succeed");

    // Initially unowned
    let owner = orchestrator.get_owner(&managed.root).expect("owner query");
    assert!(
        owner.is_none(),
        "New worktree must not have an active owner"
    );

    // Bind thread
    orchestrator
        .bind_thread(&managed.root, "task-thread-101")
        .expect("bind thread");
    let owner = orchestrator.get_owner(&managed.root).expect("owner query");
    assert_eq!(
        owner.as_deref(),
        Some("task-thread-101"),
        "Owner must match bound thread ID"
    );

    // Releasing while owned must fail closed
    let release_err = orchestrator.release_worktree(
        source_dir.path(),
        &managed.root,
        false,
        Some("task finished"),
    );
    assert!(
        release_err.is_err(),
        "Releasing actively owned worktree must fail closed"
    );

    // Retention: retaining leaves directory on disk
    orchestrator
        .release_worktree(
            source_dir.path(),
            &managed.root,
            true,
            Some("retain for inspection"),
        )
        .expect("retention must succeed");

    assert!(
        managed.root.exists(),
        "Retained worktree directory must remain intact on disk"
    );
}

#[tokio::test]
async fn test_embedded_client_provisions_real_worktree_record() {
    use agent_studios_cli_client::{AgentStudiosClient, EmbeddedClient, GoalSubmissionOptions};

    let source_dir = tempdir().expect("source tempdir");
    let base_sha = init_git_repo(source_dir.path());

    let orchestrator_dir = tempdir().expect("orchestrator tempdir");
    let wt_root = orchestrator_dir.path().join("worktrees");
    let art_root = orchestrator_dir.path().join("artifacts");

    let orchestrator = Arc::new(
        WorkspaceOrchestrator::new(wt_root, art_root).expect("orchestrator initialization"),
    );

    // Set working directory to source_dir for submit_goal cwd resolution
    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(source_dir.path()).unwrap();

    let client = EmbeddedClient::bootstrap("wt-client-test")
        .await
        .expect("bootstrap client")
        .with_orchestrator(orchestrator);

    let task_id = client
        .submit_goal(
            "build feature in isolated worktree",
            GoalSubmissionOptions::default(),
        )
        .await
        .expect("submit goal");

    // Restore cwd
    std::env::set_current_dir(original_cwd).unwrap();

    let worktrees = client.list_worktrees().await.expect("list worktrees");
    assert_eq!(
        worktrees.len(),
        1,
        "Exactly one managed worktree must be recorded in Control Plane"
    );

    let wt = &worktrees[0];
    assert_eq!(wt.rel_path, format!("worktree-{task_id}"));
    assert_eq!(wt.state, "InUse");

    // Verify Control Plane record holds base_sha and root != source
    let cp = client.control_plane().lock().await;
    let cp_wt = cp.get_worktree(wt.worktree_id).expect("worktree in CP");
    assert_eq!(cp_wt.base_commit, base_sha);
    assert_ne!(cp_wt.root, source_dir.path());
    assert!(cp_wt.root.exists());
}
