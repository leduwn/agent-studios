use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

use agent_studios_protocol::artifact::ArtifactKind;
use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId, WorktreeId};
use agent_studios_workspace::{
    ArtifactOptions, ArtifactStore, ReconciliationOutcome, WorkspaceOrchestrator,
};

fn init_test_git_repo(repo_dir: &Path) -> String {
    // Configure local git repo in temp directory
    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo_dir)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.name", "Test Agent"])
        .current_dir(repo_dir)
        .output()
        .expect("git config user.name");
    Command::new("git")
        .args(["config", "user.email", "test@agentstudios.local"])
        .current_dir(repo_dir)
        .output()
        .expect("git config user.email");
    Command::new("git")
        .args(["config", "commit.gpgsign", "false"])
        .current_dir(repo_dir)
        .output()
        .expect("git config commit.gpgsign false");

    // Create initial commit
    let file_path = repo_dir.join("README.md");
    fs::write(&file_path, "# Initial Workspace\n").expect("write README");

    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(repo_dir)
        .output()
        .expect("git add");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_dir)
        .output()
        .expect("git commit");

    let rev_out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_dir)
        .output()
        .expect("git rev-parse HEAD");

    String::from_utf8_lossy(&rev_out.stdout).trim().to_string()
}

#[test]
fn test_artifact_store_atomic_write_and_deduplication() {
    let temp = tempdir().expect("tempdir");
    let store = ArtifactStore::new(temp.path()).expect("ArtifactStore new");

    let data1 = b"Hello Agent Studios Artifact 1";
    let (hash1, size1) = store.store_bytes(data1).expect("store data1");
    assert_eq!(size1, data1.len());
    assert!(store.has_blob(&hash1));

    // Deduplication test: storing exact same content returns same hash and does not fail
    let (hash2, size2) = store.store_bytes(data1).expect("store data1 again");
    assert_eq!(hash1, hash2);
    assert_eq!(size1, size2);

    // Read bytes test
    let read_data = store.read_bytes(&hash1).expect("read data1");
    assert_eq!(read_data, data1);

    // High level artifact record helper
    let studio_id = StudioId::new();
    let task_id = TaskId::new();
    let agent_id = AgentId::new();
    let worktree_id = WorktreeId::new();
    let run_id = RunId::new();

    let options = ArtifactOptions::new()
        .with_worktree_id(Some(worktree_id))
        .with_run_id(Some(run_id))
        .with_version(1);

    let record = store
        .store_artifact(
            studio_id,
            task_id,
            agent_id,
            ArtifactKind::Patch,
            "patch.diff",
            b"diff --git a/foo b/foo",
            options,
        )
        .expect("store_artifact");

    assert_eq!(record.version, 1);
    assert_eq!(record.worktree_id, Some(worktree_id));
    assert_eq!(record.run_id, Some(run_id));
    assert_eq!(record.size_bytes, Some(22));
    assert!(record.location.starts_with("blobs/sha256/"));
}

#[test]
fn test_orchestrator_path_validation() {
    let temp = tempdir().expect("tempdir");
    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");

    let orchestrator =
        WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator new");

    // Path escaping managed root must be rejected
    let outside_root = temp.path().join("outside_worktree");
    fs::create_dir_all(&outside_root).expect("create outside dir");
    assert!(
        orchestrator
            .validate_worktree_path(&outside_root, &outside_root)
            .is_err()
    );

    // Path inside managed root with cwd escaping root must be rejected
    let valid_root = managed_root.join("wt_1");
    fs::create_dir_all(&valid_root).expect("create valid root");
    assert!(
        orchestrator
            .validate_worktree_path(&valid_root, &outside_root)
            .is_err()
    );
}

#[test]
fn test_worktree_lifecycle_capture_and_reconciliation_full() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator new");

    // 1. Create managed worktree
    let managed = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create worktree");

    assert!(managed.root.exists());
    assert!(managed.root.starts_with(&managed_root));

    // 2. Thread binding test
    let thread_1 = "thread_alpha_123";
    orchestrator
        .bind_thread(&managed.root, thread_1)
        .expect("bind thread 1");
    assert_eq!(
        orchestrator.get_owner(&managed.root).unwrap(),
        Some(thread_1.to_string())
    );

    // Rebinding same thread succeeds idempotently
    orchestrator
        .bind_thread(&managed.root, thread_1)
        .expect("rebind same thread");

    // Rebinding different thread fails with conflict
    let thread_2 = "thread_beta_456";
    let conflict_err = orchestrator.bind_thread(&managed.root, thread_2);
    assert!(conflict_err.is_err());

    // 3. Make changes in the isolated worktree
    let new_file = managed.root.join("feature.rs");
    fs::write(&new_file, "pub fn compute() -> i32 { 42 }\n").expect("write new file");

    let readme = managed.root.join("README.md");
    fs::write(&readme, "# Updated Workspace\nFeature active.\n").expect("update readme");

    // 4. Capture changes with ephemeral index
    let captured = orchestrator
        .capture_changes(&managed.root, &base_commit)
        .expect("capture changes");

    assert_eq!(captured.files_changed, 2);
    assert!(captured.changed_files.contains(&"feature.rs".to_string()));
    assert!(captured.changed_files.contains(&"README.md".to_string()));
    assert!(!captured.patch_bytes.is_empty());

    // 5. Store patch artifact
    let studio_id = StudioId::new();
    let task_id = TaskId::new();
    let agent_id = AgentId::new();
    let worktree_id = WorktreeId::new();
    let patch_record = orchestrator
        .store_patch_artifact(
            studio_id,
            task_id,
            agent_id,
            None,
            Some(worktree_id),
            &captured,
        )
        .expect("store patch artifact");

    assert!(
        orchestrator.artifact_store().has_blob(
            patch_record
                .content_hash
                .as_deref()
                .unwrap()
                .strip_prefix("sha256:")
                .unwrap()
        )
    );

    // 6. Reconciliation: create dedicated integration worktree
    let integration_wt = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create integration worktree");

    // Clean reconciliation succeeds
    let outcome = orchestrator
        .reconcile_patch(
            &integration_wt.root,
            &captured.patch_bytes,
            Some("Merge feature patch"),
        )
        .expect("reconcile patch");

    match outcome {
        ReconciliationOutcome::Applied { merge_commit } => {
            assert!(merge_commit.is_some());
            assert!(integration_wt.root.join("feature.rs").exists());
        }
        other => panic!("expected Applied, got {other:?}"),
    }

    // 7. Conflicting reconciliation test:
    // Try to apply the same patch again to the integration worktree (which now already has the changes)
    let conflict_outcome = orchestrator
        .reconcile_patch(
            &integration_wt.root,
            &captured.patch_bytes,
            Some("Duplicate patch apply"),
        )
        .expect("reconcile conflicting patch");

    match conflict_outcome {
        ReconciliationOutcome::Conflicted {
            conflicted_files,
            reason,
        } => {
            assert!(!conflicted_files.is_empty() || !reason.is_empty());
        }
        other => panic!("expected Conflicted, got {other:?}"),
    }

    // 8. Release worktrees:
    // Retain managed worktree 1: directory stays on disk!
    orchestrator
        .release_worktree(
            &repo_dir,
            &managed.root,
            true,
            Some("Retained for inspection"),
        )
        .expect("release retain");
    assert!(managed.root.exists());

    // Release integration worktree with retain = false: cleaned up!
    orchestrator
        .release_worktree(&repo_dir, &integration_wt.root, false, None)
        .expect("release remove");
    assert!(!integration_wt.root.exists());
}
