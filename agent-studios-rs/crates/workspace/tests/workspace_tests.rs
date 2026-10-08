use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

use agent_studios_protocol::artifact::ArtifactKind;
use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId, WorktreeId};
use agent_studios_protocol::worktree::WorktreeRecord;
use agent_studios_workspace::{
    ArtifactOptions, ArtifactStore, ReconciliationOutcome, WorkspaceError, WorkspaceOrchestrator,
};
use chrono::Utc;

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

#[test]
fn test_generate_recovery_report() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator new");

    let studio_id = StudioId::new();
    let now = Utc::now();

    // 1. Physical worktree 1: matched
    let wt1 = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create wt1");
    let thread1 = "thread_matched_123";
    orchestrator
        .bind_thread(&wt1.root, thread1)
        .expect("bind thread1");
    let mut rec1 = WorktreeRecord::new(studio_id, "wt-1", &repo_dir, &wt1.root, &base_commit, now);
    rec1.bound_thread_id = Some(thread1.to_string());

    // 2. Physical worktree 2: ownership mismatch
    let wt2 = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create wt2");
    let thread2_actual = "thread_actual_456";
    orchestrator
        .bind_thread(&wt2.root, thread2_actual)
        .expect("bind thread2");
    let mut rec2 = WorktreeRecord::new(studio_id, "wt-2", &repo_dir, &wt2.root, &base_commit, now);
    rec2.bound_thread_id = Some("thread_expected_other".to_string());

    // 3. Physical worktree 3: foreign (no durable record passed)
    let wt3 = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create wt3");

    // 4. Durable record 4: missing physical worktree
    let missing_path = managed_root.join("wt_missing_never_created");
    let rec4 = WorktreeRecord::new(
        studio_id,
        "wt-4-missing",
        &repo_dir,
        &missing_path,
        &base_commit,
        now,
    );

    // Run recovery report
    let report = orchestrator
        .generate_recovery_report(&repo_dir, &[rec1.clone(), rec2.clone(), rec4.clone()])
        .expect("generate recovery report");

    // Assertions
    assert_eq!(report.matched.len(), 1);
    assert_eq!(report.matched[0].id, rec1.id);

    assert_eq!(report.ownership_mismatches.len(), 1);
    assert_eq!(report.ownership_mismatches[0].worktree_id, rec2.id);
    assert_eq!(
        report.ownership_mismatches[0].expected_thread_id,
        Some("thread_expected_other".to_string())
    );
    assert_eq!(
        report.ownership_mismatches[0].actual_thread_id,
        Some(thread2_actual.to_string())
    );

    assert_eq!(report.missing.len(), 1);
    assert_eq!(report.missing[0].id, rec4.id);

    // Foreign list must contain wt3.root
    let canon_wt3 = dunce::canonicalize(&wt3.root).unwrap_or(wt3.root.clone());
    assert!(
        report
            .foreign
            .iter()
            .any(|f| dunce::canonicalize(f).unwrap_or(f.clone()) == canon_wt3),
        "wt3 must be reported as foreign"
    );
}

#[test]
fn test_release_worktree_refuses_dirty_worktree_and_preserves_dir() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator new");

    let managed = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create worktree");

    assert!(managed.root.exists());

    // Create untracked/dirty file in worktree
    let dirty_file = managed.root.join("uncommitted_work.rs");
    fs::write(&dirty_file, "pub fn dirty() -> bool { true }\n").expect("write dirty file");

    // Attempt to release with retain = false (removal requested)
    let res = orchestrator.release_worktree(&repo_dir, &managed.root, false, None);

    // Invariant: Removal MUST be refused when worktree has untracked/dirty changes
    assert!(
        matches!(res, Err(WorkspaceError::WorktreeRemovalFailed(_))),
        "expected WorktreeRemovalFailed on dirty worktree, got {res:?}"
    );

    // Invariant: Directory and dirty file MUST remain intact on disk
    assert!(
        managed.root.exists(),
        "worktree directory must be preserved on disk after refusal"
    );
    assert!(
        dirty_file.exists(),
        "uncommitted file must be preserved on disk after refusal"
    );
}

#[test]
fn test_release_worktree_refuses_actively_owned_worktree_and_preserves_dir() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator new");

    let managed = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create worktree");

    assert!(managed.root.exists());

    // Bind active thread owner
    let active_thread = "thread_active_worker_777";
    orchestrator
        .bind_thread(&managed.root, active_thread)
        .expect("bind thread");

    // Attempt to release with retain = false while owned
    let res = orchestrator.release_worktree(&repo_dir, &managed.root, false, None);

    // Invariant: Removal MUST be refused while actively owned by a thread
    assert!(
        matches!(res, Err(WorkspaceError::InvalidOperation(ref msg)) if msg.contains("actively owned by thread")),
        "expected InvalidOperation on actively owned worktree, got {res:?}"
    );

    // Invariant: Directory MUST remain intact on disk
    assert!(
        managed.root.exists(),
        "worktree directory must be preserved on disk"
    );
}

#[test]
fn test_topology_canonical_repo_and_cwd_separation() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let _base_commit = init_test_git_repo(&repo_dir);

    // Create a subdirectory inside repo
    let sub_cwd = repo_dir.join("apps").join("web");
    fs::create_dir_all(&sub_cwd).expect("create sub cwd");
    let app_file = sub_cwd.join("page.tsx");
    fs::write(
        &app_file,
        "export default function Page() { return null; }\n",
    )
    .expect("write page");

    // Commit sub_cwd file
    let _ = Command::new("git")
        .current_dir(&repo_dir)
        .args(["add", "."])
        .output()
        .expect("git add");
    let commit_out = Command::new("git")
        .current_dir(&repo_dir)
        .args(["commit", "-m", "add apps/web/page.tsx"])
        .output()
        .expect("git commit");
    assert!(commit_out.status.success());

    let rev_out = Command::new("git")
        .current_dir(&repo_dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("rev-parse");
    let new_base = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator new");

    // Create worktree pointing source_cwd to repo/apps/web
    let managed = orchestrator
        .create_worktree(&sub_cwd, Some(&new_base))
        .expect("create worktree with sub cwd");

    // Regression Test H: Validate canonical repo vs repo/apps/web root/cwd separation
    let canon_repo = dunce::canonicalize(&repo_dir).expect("canon repo");
    let canon_sub = dunce::canonicalize(&sub_cwd).expect("canon sub");

    assert_eq!(
        managed.source_root, canon_repo,
        "source_root must be top-level repo"
    );
    assert_eq!(
        managed.source_cwd, canon_sub,
        "source_cwd must be repo/apps/web"
    );
    assert!(
        managed.root.starts_with(&managed_root),
        "root must be under managed_root"
    );
    assert_eq!(
        managed.cwd,
        managed.root.join("apps").join("web"),
        "cwd must be managed_root/<id>/apps/web"
    );
    assert_ne!(
        managed.cwd, managed.root,
        "cwd and root must be decoupled and distinct"
    );
    assert!(managed.cwd.exists(), "managed cwd must exist physically");
    assert!(
        managed.cwd.join("page.tsx").exists(),
        "page.tsx must exist in managed cwd"
    );

    // Clean up
    orchestrator
        .release_worktree(&sub_cwd, &managed.root, false, None)
        .expect("release worktree");
    assert!(!managed.root.exists());
}

#[tokio::test]
async fn test_repository_root_resolution_failure_fails_closed() {
    let temp = tempdir().expect("tempdir");
    let non_git_dir = temp.path().join("not_a_git_repo");
    fs::create_dir_all(&non_git_dir).expect("create non-git dir");

    let ceiling = dunce::canonicalize(temp.path()).unwrap_or(temp.path().to_path_buf());
    unsafe {
        std::env::set_var("GIT_CEILING_DIRECTORIES", &ceiling);
    }

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator new");

    // Regression Test I: Synchronous resolution failure propagates typed GitError
    let sync_res = orchestrator.resolve_repository_root(&non_git_dir);
    assert!(
        matches!(sync_res, Err(WorkspaceError::GitError(ref msg)) if msg.contains("git rev-parse --show-toplevel failed")),
        "expected GitError on non-git dir, got {sync_res:?}"
    );

    // Regression Test I: Asynchronous resolution failure propagates fail-closed error
    let async_res = orchestrator
        .resolve_repository_root_async(non_git_dir.clone())
        .await;
    assert!(
        async_res.is_err(),
        "expected fail-closed error from async resolution, got {async_res:?}"
    );

    unsafe {
        std::env::remove_var("GIT_CEILING_DIRECTORIES");
    }
}

#[test]
fn test_artifact_lineage_deterministic_progression() {
    let temp = tempdir().expect("tempdir");
    let artifact_root = temp.path().join("artifacts");
    let store = ArtifactStore::new(&artifact_root).expect("store new");

    let studio_id = StudioId::new();
    let task_id = TaskId::new();
    let agent_id = AgentId::new();

    // Regression Test J: v1 -> v2 -> v3 deterministic lineage progression
    // v1: parent_id / supersedes is None
    let v1_opt = ArtifactOptions::new().with_version(1);
    let v1 = store
        .store_artifact(
            studio_id,
            task_id,
            agent_id,
            ArtifactKind::Patch,
            "pipeline.diff",
            b"diff --git a/a b/a\n+version 1\n",
            v1_opt,
        )
        .expect("store v1");
    assert_eq!(v1.version, 1);
    assert_eq!(v1.supersedes, None, "v1 supersedes must be None");

    // v2: supersedes Some(v1.id)
    let v2_opt = ArtifactOptions::new()
        .with_version(2)
        .with_supersedes(Some(v1.id));
    let v2 = store
        .store_artifact(
            studio_id,
            task_id,
            agent_id,
            ArtifactKind::Patch,
            "pipeline.diff",
            b"diff --git a/a b/a\n+version 2\n",
            v2_opt,
        )
        .expect("store v2");
    assert_eq!(v2.version, 2);
    assert_eq!(v2.supersedes, Some(v1.id), "v2 must supersede v1");

    // v3: supersedes Some(v2.id)
    let v3_opt = ArtifactOptions::new()
        .with_version(3)
        .with_supersedes(Some(v2.id));
    let v3 = store
        .store_artifact(
            studio_id,
            task_id,
            agent_id,
            ArtifactKind::Patch,
            "pipeline.diff",
            b"diff --git a/a b/a\n+version 3\n",
            v3_opt,
        )
        .expect("store v3");
    assert_eq!(v3.version, 3);
    assert_eq!(v3.supersedes, Some(v2.id), "v3 must supersede v2");
}

#[test]
fn test_transactional_reconciliation_failure_preserves_untracked_sentinel() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator new");

    // Create integration worktree
    let integration_wt = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create integration wt");

    // Track a file
    let tracked_file = integration_wt.root.join("tracked.rs");
    fs::write(&tracked_file, "pub fn initial() -> i32 { 42 }\n").expect("write tracked");
    let _ = Command::new("git")
        .current_dir(&integration_wt.root)
        .args(["add", "tracked.rs"])
        .output()
        .expect("git add");
    let commit_out = Command::new("git")
        .current_dir(&integration_wt.root)
        .args(["commit", "-m", "commit tracked.rs"])
        .output()
        .expect("git commit");
    assert!(commit_out.status.success());

    let rev_out = Command::new("git")
        .current_dir(&integration_wt.root)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("rev-parse");
    let pre_head = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();

    // Create untracked sentinel file
    let sentinel_file = integration_wt.root.join("untracked_sentinel.txt");
    let sentinel_bytes = b"sentinel bytes 99999\n";
    fs::write(&sentinel_file, sentinel_bytes).expect("write sentinel");

    // Invalid patch that fails to apply cleanly
    let invalid_patch = b"diff --git a/tracked.rs b/tracked.rs\n--- a/tracked.rs\n+++ b/tracked.rs\n@@ -99,6 +99,6 @@\n-nonexistent line\n+new line\n";

    // Regression Test K: Attempt to apply invalid patch directly
    let outcome = agent_studios_workspace::apply_patch(
        &integration_wt.root,
        invalid_patch,
        Some("failing commit"),
    )
    .expect("apply_patch call");

    // Invariant: Produces ReconciliationOutcome::Conflicted or ReconciliationOutcome::Failed
    match outcome {
        ReconciliationOutcome::Conflicted { ref reason, .. } => {
            assert!(!reason.is_empty(), "conflict reason should not be empty");
        }
        ReconciliationOutcome::Failed { ref error } => {
            assert!(!error.is_empty(), "failure error should not be empty");
        }
        other => panic!("expected Conflicted or Failed outcome, got {other:?}"),
    }

    // Invariant: pre-HEAD is preserved
    let post_rev_out = Command::new("git")
        .current_dir(&integration_wt.root)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("rev-parse");
    let post_head = String::from_utf8_lossy(&post_rev_out.stdout)
        .trim()
        .to_string();
    assert_eq!(
        post_head, pre_head,
        "HEAD must match pre_head after rollback"
    );

    // Invariant: Tracked bytes are preserved
    let tracked_content = fs::read_to_string(&tracked_file).expect("read tracked");
    assert_eq!(
        tracked_content, "pub fn initial() -> i32 { 42 }\n",
        "tracked file bytes must be preserved"
    );

    // Invariant: Untracked sentinel file MUST be preserved (never deleted by clean -fd)
    assert!(
        sentinel_file.exists(),
        "untracked sentinel file must remain intact"
    );
    let post_sentinel_bytes = fs::read(&sentinel_file).expect("read sentinel");
    assert_eq!(
        post_sentinel_bytes, sentinel_bytes,
        "sentinel file bytes must remain exact"
    );

    // Clean up
    orchestrator
        .release_worktree(&repo_dir, &integration_wt.root, true, Some("test cleanup"))
        .expect("release");
}

#[test]
fn test_release_worktree_ownership_query_failure_fails_closed() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator new");

    let managed = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create worktree");

    assert!(managed.root.exists());

    // Corrupt codex-thread.json by writing invalid JSON bytes
    let git_path_out = Command::new("git")
        .current_dir(&managed.root)
        .args(["rev-parse", "--git-path", "codex-thread.json"])
        .output()
        .expect("rev-parse --git-path");
    let rel_git_path = String::from_utf8_lossy(&git_path_out.stdout)
        .trim()
        .to_string();
    let thread_meta_path = managed.root.join(rel_git_path);
    fs::create_dir_all(thread_meta_path.parent().unwrap()).expect("parent dir");
    fs::write(&thread_meta_path, b"{ corrupted invalid json").expect("write corrupt meta");

    // Regression Test B (unit): Ownership query failure MUST fail closed and refuse removal
    let res = orchestrator.release_worktree(&repo_dir, &managed.root, false, None);

    assert!(
        matches!(res, Err(WorkspaceError::InvalidOperation(ref msg)) if msg.contains("ownership query failed")),
        "expected InvalidOperation on corrupted metadata, got {res:?}"
    );

    // Invariant: Zero physical deletion
    assert!(
        managed.root.exists(),
        "worktree directory must be preserved on disk when ownership query fails"
    );
}

#[test]
fn test_release_worktree_clean_removal_verifies_root_removed() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator new");

    let managed = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create worktree");

    assert!(managed.root.exists());

    // Regression Test E (unit): Clean worktree with retain=false removes physical directory
    orchestrator
        .release_worktree(&repo_dir, &managed.root, false, None)
        .expect("clean release");

    assert!(
        !managed.root.exists(),
        "worktree directory must be removed from disk"
    );
}

#[test]
fn test_isolated_reconciliation_preserves_untracked_sentinel_on_success() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator new");

    let integration_wt = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create integration worktree");

    // Create untracked sentinel file
    let sentinel_file = integration_wt.root.join("untracked_sentinel.txt");
    let sentinel_bytes = b"important user data - do not delete or stage\n";
    fs::write(&sentinel_file, sentinel_bytes).expect("write sentinel");

    // Create valid patch modifying README.md
    let valid_patch = b"diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1 +1,2 @@\n # Initial Workspace\n+New line added by patch\n";

    let outcome = agent_studios_workspace::apply_patch(
        &integration_wt.root,
        valid_patch,
        Some("Apply valid patch cleanly"),
    )
    .expect("apply_patch");

    // Invariant: Applied with merge_commit sha
    match outcome {
        ReconciliationOutcome::Applied { merge_commit } => {
            assert!(merge_commit.is_some(), "expected merge_commit SHA");
        }
        other => panic!("expected Applied outcome, got {other:?}"),
    }

    // Invariant: Untracked sentinel file exists with identical content
    assert!(
        sentinel_file.exists(),
        "untracked sentinel must be preserved"
    );
    let read_sentinel = fs::read(&sentinel_file).expect("read sentinel");
    assert_eq!(read_sentinel, sentinel_bytes);

    // Invariant: Untracked sentinel file is NOT in the new HEAD tree
    let ls_tree_out = Command::new("git")
        .current_dir(&integration_wt.root)
        .args(["ls-tree", "-r", "HEAD", "--name-only"])
        .output()
        .expect("git ls-tree");
    let ls_tree_str = String::from_utf8_lossy(&ls_tree_out.stdout);
    assert!(
        !ls_tree_str.contains("untracked_sentinel.txt"),
        "untracked sentinel must never be staged into commit"
    );

    // Invariant: README.md working tree is updated
    let readme_content =
        fs::read_to_string(integration_wt.root.join("README.md")).expect("read readme");
    assert!(readme_content.contains("New line added by patch"));
}

#[test]
fn test_isolated_reconciliation_mutation_boundary_race_preserves_worktree() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator new");

    let integration_wt = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create integration worktree");

    let pre_rev_out = Command::new("git")
        .current_dir(&integration_wt.root)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("rev-parse");
    let pre_head = String::from_utf8_lossy(&pre_rev_out.stdout)
        .trim()
        .to_string();

    // Valid patch modifying README.md
    let valid_patch = b"diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1 +1,2 @@\n # Initial Workspace\n+Concurrent patch line\n";

    // Test hook creates concurrent uncommitted modification immediately before promotion
    let outcome = agent_studios_workspace::apply_patch_with_pre_promotion_hook(
        &integration_wt.root,
        valid_patch,
        Some("Concurrent race test"),
        Some(|wt_path: &Path| {
            fs::write(wt_path.join("README.md"), "# Modified concurrently!\n")
                .map_err(|e| WorkspaceError::InvalidOperation(e.to_string()))
        }),
    )
    .expect("apply_patch call");

    // Invariant: Pre-promotion revalidation catches dirty integration worktree and fails
    match outcome {
        ReconciliationOutcome::Failed { error } => {
            assert!(
                error.contains("pre-promotion revalidation failed"),
                "expected pre-promotion revalidation failure, got: {error}"
            );
        }
        other => panic!("expected Failed outcome due to race check, got {other:?}"),
    }

    // Invariant: HEAD unchanged
    let post_rev_out = Command::new("git")
        .current_dir(&integration_wt.root)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("rev-parse");
    let post_head = String::from_utf8_lossy(&post_rev_out.stdout)
        .trim()
        .to_string();
    assert_eq!(post_head, pre_head, "HEAD must not advance on race failure");

    // Invariant: Concurrent working tree changes are preserved
    let concurrent_readme =
        fs::read_to_string(integration_wt.root.join("README.md")).expect("read readme");
    assert_eq!(concurrent_readme, "# Modified concurrently!\n");
}

#[test]
fn test_isolated_reconciliation_candidate_construction_failure_preserves_worktree() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator new");

    let integration_wt = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create integration worktree");

    let untracked = integration_wt.root.join("keeper.txt");
    fs::write(&untracked, "safe file\n").expect("write untracked");

    // Corrupted patch that fails git apply --cached
    let corrupt_patch = b"not a valid git patch header\nrandom corrupt bytes\n";
    let outcome = orchestrator
        .reconcile_patch(&integration_wt.root, corrupt_patch, Some("corrupt"))
        .expect("reconcile_patch");

    assert!(
        matches!(
            outcome,
            ReconciliationOutcome::Conflicted { .. } | ReconciliationOutcome::Failed { .. }
        ),
        "expected Conflicted or Failed for corrupt patch, got {outcome:?}"
    );

    assert!(untracked.exists(), "untracked file must be preserved");
    let content = fs::read_to_string(&untracked).expect("read untracked");
    assert_eq!(content, "safe file\n");
}

#[test]
fn test_safe_removal_of_retired_owned_worktree() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator new");

    // 1. Success case: retired owner matches expected thread
    let wt1 = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create wt1");
    orchestrator
        .bind_thread(&wt1.root, "thread-retired-123")
        .expect("bind thread");
    assert_eq!(
        orchestrator.get_owner(&wt1.root).unwrap(),
        Some("thread-retired-123".to_string())
    );

    orchestrator
        .release_retired_owned_worktree(&repo_dir, &wt1.root, "thread-retired-123")
        .expect("safe release retired owned worktree");
    assert!(
        !wt1.root.exists(),
        "wt1 directory should be removed from disk"
    );

    // 2. Failure case: foreign owner recorded
    let wt2 = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create wt2");
    orchestrator
        .bind_thread(&wt2.root, "thread-other-owner")
        .expect("bind thread");

    let foreign_res =
        orchestrator.release_retired_owned_worktree(&repo_dir, &wt2.root, "thread-expected-owner");
    assert!(
        matches!(foreign_res, Err(WorkspaceError::InvalidOperation(ref msg)) if msg.contains("expected retired owner")),
        "expected InvalidOperation on foreign owner mismatch, got {foreign_res:?}"
    );
    assert!(
        wt2.root.exists(),
        "wt2 directory must remain on disk after mismatch"
    );

    // 3. Failure case: uncommitted changes in worktree (fails safe without deletion)
    let wt3 = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create wt3");
    orchestrator
        .bind_thread(&wt3.root, "thread-retired-dirty")
        .expect("bind thread");
    let dirty_file = wt3.root.join("uncommitted.rs");
    fs::write(&dirty_file, "pub fn dirty() {}\n").expect("write dirty");

    let dirty_res =
        orchestrator.release_retired_owned_worktree(&repo_dir, &wt3.root, "thread-retired-dirty");
    assert!(
        matches!(dirty_res, Err(WorkspaceError::WorktreeRemovalFailed(_))),
        "expected WorktreeRemovalFailed on dirty worktree, got {dirty_res:?}"
    );
    assert!(wt3.root.exists(), "dirty wt3 directory must remain on disk");
    assert!(
        dirty_file.exists(),
        "dirty uncommitted file must be preserved"
    );
}
