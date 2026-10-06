use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use agent_studios_control_plane::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::store::InMemoryStore;
use agent_studios_internal_agent::{
    AgentExecutionContext, AgentExecutionResult, AgentStudiosSupervisor, ControlPlaneActor,
    CoordinatorDecision, FailurePolicy, InternalAgentSpec, InternalTeamSpec, MockAgentExecutor,
    PlannedTask, WorkspaceAccessMode, WorkspacePolicyArbitrator,
};
use agent_studios_protocol::id::AgentId;
use agent_studios_protocol::task::TaskState;
use agent_studios_provider::id::{ModelId, ProviderInstanceId};
use agent_studios_provider::model::ModelRef;
use agent_studios_workspace::WorkspaceOrchestrator;
use tempfile::tempdir;

fn init_test_git_repo(repo_dir: &Path) -> String {
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

fn dummy_model() -> ModelRef {
    ModelRef::new(
        ProviderInstanceId::new(),
        ModelId::new("test-model").unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_parallel_mutating_tasks_with_distinct_worktrees() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        Arc::new(WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator"));

    let integration_wt = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create integration wt");

    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    let (cp_handle, _task) = ControlPlaneActor::spawn(cp);

    let studio = cp_handle
        .create_studio("Mutating E2E Studio")
        .await
        .unwrap();

    let coord = InternalAgentSpec::new(AgentId::new(), "Coord", "Coordinator", dummy_model());
    let worker1 = InternalAgentSpec::new(AgentId::new(), "Worker1", "Worker", dummy_model())
        .with_workspace_access(WorkspaceAccessMode::Mutating);
    let worker2 = InternalAgentSpec::new(AgentId::new(), "Worker2", "Worker", dummy_model())
        .with_workspace_access(WorkspaceAccessMode::Mutating);

    let team = InternalTeamSpec::new(studio.id, "mutating-team", coord)
        .with_max_parallel_agents(4)
        .add_agent("w1", worker1)
        .unwrap()
        .add_agent("w2", worker2)
        .unwrap();

    let executor = MockAgentExecutor::new();
    let active_workers = Arc::new(AtomicUsize::new(0));
    let peak_concurrency = Arc::new(AtomicUsize::new(0));

    let active_clone = Arc::clone(&active_workers);
    let peak_clone = Arc::clone(&peak_concurrency);

    executor.set_handler(move |ctx: AgentExecutionContext| {
        if ctx.agent_spec.role == "Coordinator" {
            if ctx.task_id.is_none() && ctx.prompt.starts_with("Objective:") {
                let plan = CoordinatorDecision::Plan {
                    tasks: vec![
                        PlannedTask {
                            task_key: "t1".to_string(),
                            title: "Task 1 Mutate".to_string(),
                            description: None,
                            assigned_alias: "w1".to_string(),
                            depends_on: vec![],
                            workspace_access: Some(WorkspaceAccessMode::Mutating),
                            priority: None,
                        },
                        PlannedTask {
                            task_key: "t2".to_string(),
                            title: "Task 2 Mutate".to_string(),
                            description: None,
                            assigned_alias: "w2".to_string(),
                            depends_on: vec![],
                            workspace_access: Some(WorkspaceAccessMode::Mutating),
                            priority: None,
                        },
                    ],
                };
                let json = serde_json::to_string(&plan).unwrap();
                return Ok(AgentExecutionResult {
                    output: json,
                    turns_used: 1,
                    tool_calls_used: 0,
                    duration_secs: 1,
                    success: true,
                });
            }
            return Ok(AgentExecutionResult {
                output: "Review done".to_string(),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 1,
                success: true,
            });
        }

        // Mutating worker execution
        let current = active_clone.fetch_add(1, Ordering::SeqCst) + 1;
        eprintln!(
            "HANDLER start: {} task={:?} active={}",
            ctx.agent_spec.display_name, ctx.task_id, current
        );
        let mut prev_peak = peak_clone.load(Ordering::SeqCst);
        while current > prev_peak {
            match peak_clone.compare_exchange_weak(
                prev_peak,
                current,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(actual) => prev_peak = actual,
            }
        }

        let ws = ctx.workspace_path.expect("worker must have workspace_path");
        if ctx.agent_spec.display_name == "Worker1" {
            fs::write(
                ws.join("feature_one.rs"),
                "pub fn feature_one() -> i32 { 1 }\n",
            )
            .expect("write feature_one");
        } else {
            fs::write(
                ws.join("feature_two.rs"),
                "pub fn feature_two() -> i32 { 2 }\n",
            )
            .expect("write feature_two");
        }

        // Wait up to 5 seconds for peer mutating worker to also become active concurrently
        for _ in 0..100 {
            if active_clone.load(Ordering::SeqCst) >= 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        eprintln!(
            "HANDLER end: {} task={:?} peak={}",
            ctx.agent_spec.display_name,
            ctx.task_id,
            peak_clone.load(Ordering::SeqCst)
        );

        active_clone.fetch_sub(1, Ordering::SeqCst);

        Ok(AgentExecutionResult {
            output: "Mutated worktree successfully".to_string(),
            turns_used: 2,
            tool_calls_used: 1,
            duration_secs: 1,
            success: true,
        })
    });

    let arbitrator = WorkspacePolicyArbitrator::new();
    let supervisor = AgentStudiosSupervisor::new(cp_handle.clone(), team, arbitrator, executor)
        .with_workspace_orchestrator(orchestrator.clone(), repo_dir.clone(), base_commit.clone())
        .with_integration_worktree(integration_wt.root.clone());

    let summary = supervisor.run("Run parallel mutating tasks").await.unwrap();

    assert_eq!(summary.total_tasks, 2);
    assert_eq!(summary.completed_tasks, 2);
    assert_eq!(summary.failed_tasks, 0);

    // Verified: Peak concurrency was 2 (both mutating workers executed concurrently on distinct worktrees!)
    assert_eq!(peak_concurrency.load(Ordering::SeqCst), 2);

    // Verified: Both changes were reconciled into the integration worktree
    assert!(integration_wt.root.join("feature_one.rs").exists());
    assert!(integration_wt.root.join("feature_two.rs").exists());
}

#[tokio::test]
async fn test_dirty_worktree_retained_on_failure() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        Arc::new(WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator"));

    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    let (cp_handle, _task) = ControlPlaneActor::spawn(cp);

    let studio = cp_handle.create_studio("Retention Studio").await.unwrap();

    let coord = InternalAgentSpec::new(AgentId::new(), "Coord", "Coordinator", dummy_model());
    let worker = InternalAgentSpec::new(AgentId::new(), "WorkerFail", "Worker", dummy_model())
        .with_workspace_access(WorkspaceAccessMode::Mutating);

    let team = InternalTeamSpec::new(studio.id, "retention-team", coord)
        .add_agent("w", worker)
        .unwrap();

    let failed_worktree_root = Arc::new(std::sync::Mutex::new(None::<PathBuf>));
    let wt_clone = Arc::clone(&failed_worktree_root);

    let executor = MockAgentExecutor::new();
    executor.set_handler(move |ctx: AgentExecutionContext| {
        if ctx.agent_spec.role == "Coordinator" {
            let plan = CoordinatorDecision::Plan {
                tasks: vec![PlannedTask {
                    task_key: "t_fail".to_string(),
                    title: "Failing Task".to_string(),
                    description: None,
                    assigned_alias: "w".to_string(),
                    depends_on: vec![],
                    workspace_access: Some(WorkspaceAccessMode::Mutating),
                    priority: None,
                }],
            };
            return Ok(AgentExecutionResult {
                output: serde_json::to_string(&plan).unwrap(),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 1,
                success: true,
            });
        }

        let ws = ctx.workspace_path.expect("workspace path");
        *wt_clone.lock().unwrap() = Some(ws.clone());

        // Worker leaves dirty file in worktree then fails
        fs::write(ws.join("dirty_state.log"), "partial crash log\n").expect("write dirty");

        Ok(AgentExecutionResult {
            output: "Worker crashed midway".to_string(),
            turns_used: 1,
            tool_calls_used: 1,
            duration_secs: 1,
            success: false,
        })
    });

    let arbitrator = WorkspacePolicyArbitrator::new();
    let supervisor = AgentStudiosSupervisor::new(cp_handle.clone(), team, arbitrator, executor)
        .with_failure_policy(FailurePolicy::ContinueIndependent)
        .with_workspace_orchestrator(orchestrator, repo_dir, base_commit);

    let summary = supervisor.run("Run failing task").await.unwrap();
    assert_eq!(summary.failed_tasks, 1);

    // Invariant: Dirty worktree MUST NOT be deleted on failure
    let preserved_path = failed_worktree_root.lock().unwrap().clone().unwrap();
    assert!(preserved_path.exists());
    assert!(preserved_path.join("dirty_state.log").exists());
}

#[tokio::test]
async fn test_reconciliation_conflict_does_not_fail_task() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        Arc::new(WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator"));

    let integration_wt = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create integration wt");

    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    let (cp_handle, _task) = ControlPlaneActor::spawn(cp);

    let studio = cp_handle.create_studio("Conflict Studio").await.unwrap();

    let coord = InternalAgentSpec::new(AgentId::new(), "Coord", "Coordinator", dummy_model());
    let worker1 = InternalAgentSpec::new(AgentId::new(), "Worker1", "Worker", dummy_model())
        .with_workspace_access(WorkspaceAccessMode::Mutating);
    let worker2 = InternalAgentSpec::new(AgentId::new(), "Worker2", "Worker", dummy_model())
        .with_workspace_access(WorkspaceAccessMode::Mutating);

    // Sequential tasks so t1 reconciles first, then t2 conflicts with t1
    let team = InternalTeamSpec::new(studio.id, "conflict-team", coord)
        .add_agent("w1", worker1)
        .unwrap()
        .add_agent("w2", worker2)
        .unwrap();

    let executor = MockAgentExecutor::new();
    executor.set_handler(move |ctx: AgentExecutionContext| {
        if ctx.agent_spec.role == "Coordinator" {
            let plan = CoordinatorDecision::Plan {
                tasks: vec![
                    PlannedTask {
                        task_key: "t1".to_string(),
                        title: "Task 1".to_string(),
                        description: None,
                        assigned_alias: "w1".to_string(),
                        depends_on: vec![],
                        workspace_access: Some(WorkspaceAccessMode::Mutating),
                        priority: Some(10),
                    },
                    PlannedTask {
                        task_key: "t2".to_string(),
                        title: "Task 2".to_string(),
                        description: None,
                        assigned_alias: "w2".to_string(),
                        depends_on: vec!["t1".to_string()],
                        workspace_access: Some(WorkspaceAccessMode::Mutating),
                        priority: Some(5),
                    },
                ],
            };
            return Ok(AgentExecutionResult {
                output: serde_json::to_string(&plan).unwrap(),
                turns_used: 1,
                tool_calls_used: 0,
                duration_secs: 1,
                success: true,
            });
        }

        let ws = ctx.workspace_path.expect("workspace path");
        let readme = ws.join("README.md");
        if ctx.agent_spec.display_name == "Worker1" {
            fs::write(&readme, "# Header Alpha\nLine from worker 1\n").expect("write readme 1");
        } else {
            // Worker 2 produces conflicting change on the same base commit
            fs::write(&readme, "# Header Beta\nLine from worker 2\n").expect("write readme 2");
        }

        Ok(AgentExecutionResult {
            output: "Modified readme".to_string(),
            turns_used: 1,
            tool_calls_used: 1,
            duration_secs: 1,
            success: true,
        })
    });

    let arbitrator = WorkspacePolicyArbitrator::new();
    let supervisor = AgentStudiosSupervisor::new(cp_handle.clone(), team, arbitrator, executor)
        .with_workspace_orchestrator(orchestrator, repo_dir, base_commit)
        .with_integration_worktree(integration_wt.root);

    let summary = supervisor.run("Run conflicting tasks").await.unwrap();

    // Invariant: Both tasks succeed! Conflict is NOT task failure!
    assert_eq!(summary.total_tasks, 2);
    assert_eq!(summary.completed_tasks, 2);
    assert_eq!(summary.failed_tasks, 0);

    let tasks = cp_handle.get_studio_tasks(studio.id).await.unwrap();
    assert_eq!(tasks.len(), 2);
    assert!(tasks.iter().all(|t| t.state == TaskState::Succeeded));
}
