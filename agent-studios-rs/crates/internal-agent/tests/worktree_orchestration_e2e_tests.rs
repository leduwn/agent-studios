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
    AgentExecutionContext, AgentExecutionResult, AgentExecutor, AgentStudiosCodexRuntimeFactory,
    AgentStudiosSupervisor, CodexAgentExecutor, ControlPlaneActor, CoordinatorDecision,
    FailurePolicy, InternalAgentError, InternalAgentSpec, InternalTeamSpec, MockAgentExecutor,
    PlannedTask, WorkspaceAccessMode, WorkspacePolicyArbitrator,
    build_agent_studios_extension_builder,
};
use agent_studios_protocol::id::{AgentId, WorktreeId};
use agent_studios_protocol::reconciliation::ReconciliationState;
use agent_studios_protocol::task::{DependencyOutputPolicy, TaskState};
use agent_studios_protocol::worktree::{ExecutionWorkspace, WorktreeState};
use agent_studios_provider::ProviderCatalog;
use agent_studios_provider::ProviderDefinition;
use agent_studios_provider::auth::AuthenticationScheme;
use agent_studios_provider::capabilities::ModelCapabilities;
use agent_studios_provider::id::{ModelId, ProviderId, ProviderInstanceId};
use agent_studios_provider::instance::ProviderInstance;
use agent_studios_provider::model::{ModelDescriptor, ModelLimits, ModelRef};
use agent_studios_provider::protocol::ProtocolFamily;
use agent_studios_provider::secret::{SecretBackend, SecretReference};
use agent_studios_runtime_session::AgentStudiosRuntimeSessionFactory;
use agent_studios_runtime_transport::{ContinuationManager, InMemorySecretResolver};
use agent_studios_workspace::{WorkspaceError, WorkspaceOrchestrator};
use codex_core::config::Config;
use codex_extension_api::{ExtensionData, ToolCall, ToolContributor, ToolName};
use codex_login::{AuthManager, CodexAuth};
use codex_protocol::protocol::SessionSource;
use codex_tools::{
    JsonToolOutput, ResponsesApiTool, ToolExecutor, ToolExecutorFuture, ToolExposure, ToolOutput,
    ToolPayload, ToolSpec, parse_tool_input_schema,
};
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
                            ..Default::default()
                        },
                        PlannedTask {
                            task_key: "t2".to_string(),
                            title: "Task 2 Mutate".to_string(),
                            description: None,
                            assigned_alias: "w2".to_string(),
                            depends_on: vec![],
                            workspace_access: Some(WorkspaceAccessMode::Mutating),
                            priority: None,
                            ..Default::default()
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

        let ws = ctx.execution_workspace.cwd().to_path_buf();
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
                    ..Default::default()
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

        let ws = ctx.execution_workspace.cwd().to_path_buf();
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
    let sentinel_content = fs::read(preserved_path.join("dirty_state.log")).unwrap();
    assert_eq!(sentinel_content, b"partial crash log\n");

    let cp_state = cp_handle.get_state().await.unwrap();
    let retained_wt = cp_state
        .worktrees
        .values()
        .find(|w| w.root == preserved_path)
        .expect("worktree record in control plane");
    assert_eq!(retained_wt.state, WorktreeState::Retained);
    assert_ne!(retained_wt.state, WorktreeState::Removed);
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
                        ..Default::default()
                    },
                    PlannedTask {
                        task_key: "t2".to_string(),
                        title: "Task 2".to_string(),
                        description: None,
                        assigned_alias: "w2".to_string(),
                        depends_on: vec!["t1".to_string()],
                        workspace_access: Some(WorkspaceAccessMode::Mutating),
                        priority: Some(5),
                        ..Default::default()
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

        let ws = ctx.execution_workspace.cwd().to_path_buf();
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

struct BodyContainsMatcher(&'static str);

impl wiremock::Match for BodyContainsMatcher {
    fn matches(&self, request: &wiremock::Request) -> bool {
        let body_str = String::from_utf8_lossy(&request.body);
        body_str.contains(self.0)
    }
}

fn run_with_large_stack<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(fut)
        })
        .unwrap()
        .join()
        .unwrap()
}

fn register_provider_and_instance(
    catalog: &mut ProviderCatalog,
    provider_slug: &str,
    provider_name: &str,
    protocol: ProtocolFamily,
    base_url: &str,
    auth_scheme: AuthenticationScheme,
) -> ProviderInstanceId {
    let def_id = ProviderId::new(provider_slug).unwrap();
    let def =
        ProviderDefinition::new(def_id.clone(), provider_name, vec![protocol.clone()]).unwrap();
    let _ = catalog.register_provider_definition(def);
    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        def_id,
        provider_name,
        protocol,
        agent_studios_provider::endpoint::EndpointProfile::new(base_url).unwrap(),
        auth_scheme,
    )
    .unwrap();
    catalog.register_provider_instance(instance).unwrap();
    instance_id
}

#[derive(Clone, Default)]
struct TestWorkspaceFileWriterTool {
    barrier: Option<Arc<tokio::sync::Barrier>>,
    invoked_cwds: Arc<std::sync::Mutex<Vec<PathBuf>>>,
}

impl ToolContributor for TestWorkspaceFileWriterTool {
    fn tools(
        &self,
        _session_store: &ExtensionData,
        _thread_store: &ExtensionData,
    ) -> Vec<Arc<dyn for<'call> ToolExecutor<ToolCall<'call>>>> {
        vec![Arc::new(self.clone())]
    }
}

impl<'call> ToolExecutor<ToolCall<'call>> for TestWorkspaceFileWriterTool {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("write_workspace_file")
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::Function(ResponsesApiTool {
            name: "write_workspace_file".to_string(),
            description: "Writes a file to the active managed worktree".to_string(),
            strict: false,
            defer_loading: None,
            parameters: parse_tool_input_schema(&serde_json::json!({
                "type": "object",
                "properties": {
                    "filename": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["filename", "content"]
            }))
            .unwrap(),
            output_schema: None,
        })
    }

    fn exposure(&self) -> ToolExposure {
        ToolExposure::Direct
    }

    fn handle<'a>(&'a self, call: ToolCall<'call>) -> ToolExecutorFuture<'a>
    where
        'call: 'a,
    {
        let barrier = self.barrier.clone();
        let invoked_cwds = self.invoked_cwds.clone();
        Box::pin(async move {
            let (filename, content) = match &call.payload {
                ToolPayload::Function { arguments } => {
                    let parsed: serde_json::Value =
                        serde_json::from_str(arguments).unwrap_or_default();
                    let file = parsed["filename"]
                        .as_str()
                        .unwrap_or("default.txt")
                        .to_string();
                    let cont = parsed["content"].as_str().unwrap_or("").to_string();
                    (file, cont)
                }
                _ => ("default.txt".to_string(), String::new()),
            };

            let target_dir = call
                .environments
                .first()
                .map(|env| env.cwd.to_path_buf())
                .ok_or_else(|| {
                    codex_tools::FunctionCallError::RespondToModel(
                        "No execution environment cwd found".to_string(),
                    )
                })?;

            invoked_cwds.lock().unwrap().push(target_dir.clone());

            if let Some(b) = barrier {
                b.wait().await;
            }

            let target_file = target_dir.join(&filename);
            std::fs::write(&target_file, &content).map_err(|e| {
                codex_tools::FunctionCallError::RespondToModel(format!(
                    "Failed to write file {}: {e}",
                    target_file.display()
                ))
            })?;

            Ok(Box::new(JsonToolOutput::new(serde_json::json!({
                "status": "success",
                "filename": filename,
                "target_file": target_file.display().to_string()
            }))) as Box<dyn ToolOutput>)
        })
    }
}

#[derive(Clone, Default)]
struct TestWorkspaceFileReaderTool {
    read_cwds: Arc<std::sync::Mutex<Vec<PathBuf>>>,
}

impl ToolContributor for TestWorkspaceFileReaderTool {
    fn tools(
        &self,
        _session_store: &ExtensionData,
        _thread_store: &ExtensionData,
    ) -> Vec<Arc<dyn for<'call> ToolExecutor<ToolCall<'call>>>> {
        vec![Arc::new(self.clone())]
    }
}

impl<'call> ToolExecutor<ToolCall<'call>> for TestWorkspaceFileReaderTool {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("read_workspace_file")
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::Function(ResponsesApiTool {
            name: "read_workspace_file".to_string(),
            description: "Reads a file from the active managed worktree".to_string(),
            strict: false,
            defer_loading: None,
            parameters: parse_tool_input_schema(&serde_json::json!({
                "type": "object",
                "properties": {
                    "filename": { "type": "string" }
                },
                "required": ["filename"]
            }))
            .unwrap(),
            output_schema: None,
        })
    }

    fn exposure(&self) -> ToolExposure {
        ToolExposure::Direct
    }

    fn handle<'a>(&'a self, call: ToolCall<'call>) -> ToolExecutorFuture<'a>
    where
        'call: 'a,
    {
        let read_cwds = self.read_cwds.clone();
        Box::pin(async move {
            let filename = match &call.payload {
                ToolPayload::Function { arguments } => {
                    let parsed: serde_json::Value =
                        serde_json::from_str(arguments).unwrap_or_default();
                    parsed["filename"]
                        .as_str()
                        .unwrap_or("default.txt")
                        .to_string()
                }
                _ => "default.txt".to_string(),
            };

            let target_dir = call
                .environments
                .first()
                .map(|env| env.cwd.to_path_buf())
                .ok_or_else(|| {
                    codex_tools::FunctionCallError::RespondToModel(
                        "No execution environment cwd found".to_string(),
                    )
                })?;

            read_cwds.lock().unwrap().push(target_dir.clone());

            let target_file = target_dir.join(&filename);
            let content = std::fs::read_to_string(&target_file).map_err(|e| {
                codex_tools::FunctionCallError::RespondToModel(format!(
                    "Failed to read file {}: {e}",
                    target_file.display()
                ))
            })?;

            Ok(Box::new(JsonToolOutput::new(serde_json::json!({
                "status": "success",
                "filename": filename,
                "content": content
            }))) as Box<dyn ToolOutput>)
        })
    }
}

#[test]
fn test_real_codex_worktree_orchestration_e2e() {
    run_with_large_stack(async {
        let temp = tempdir().expect("tempdir");
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).expect("create repo dir");
        let base_commit = init_test_git_repo(&repo_dir);

        let managed_root = temp.path().join("managed_worktrees");
        let artifact_root = temp.path().join("artifacts");
        let orchestrator = Arc::new(
            WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator"),
        );

        let integration_wt = orchestrator
            .create_worktree(&repo_dir, Some(&base_commit))
            .expect("create integration wt");

        let server = MockServer::start().await;
        let secret_resolver = InMemorySecretResolver::new();
        secret_resolver.insert(
            SecretReference {
                backend: SecretBackend::EnvironmentVariable,
                locator: "TEST_API_KEY".to_string(),
            },
            "dummy-api-key",
        );
        let secret_resolver = Arc::new(secret_resolver);

        let mut catalog = ProviderCatalog::new();
        let provider_inst_id = register_provider_and_instance(
            &mut catalog,
            "real-codex-provider",
            "Real Codex Provider",
            ProtocolFamily::OpenAiChatCompletions,
            &server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "TEST_API_KEY".to_string(),
                },
            },
        );

        let coord_model_id = ModelId::new("coord-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    provider_inst_id,
                    coord_model_id.clone(),
                    "Coord Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let worker_model_id = ModelId::new("worker-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    provider_inst_id,
                    worker_model_id.clone(),
                    "Worker Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let catalog = Arc::new(catalog);
        let continuation_manager = Arc::new(ContinuationManager::new());
        let factory = Arc::new(AgentStudiosRuntimeSessionFactory::new(
            catalog,
            secret_resolver,
            continuation_manager,
        ));

        let codex_home = temp.path().join("codex_home");
        let config = AgentStudiosCodexRuntimeFactory::create_test_config(&codex_home)
            .await
            .expect("test config");
        let auth_manager = AuthManager::from_auth_for_testing_with_home(
            CodexAuth::from_api_key("dummy"),
            config.codex_home.to_path_buf(),
        );

        let writer_tool = Arc::new(TestWorkspaceFileWriterTool::default());

        let mut builder = build_agent_studios_extension_builder::<Config>();
        builder.tool_contributor(writer_tool);
        let extensions = Arc::new(builder.build());

        let thread_manager = AgentStudiosCodexRuntimeFactory::build_thread_manager_with_extensions(
            &config,
            auth_manager,
            Some(SessionSource::Exec),
            extensions,
        )
        .await
        .expect("build thread manager");
        let thread_manager = Arc::new(thread_manager);

        let clock = SystemClock;
        let store = InMemoryStore::new();
        let cp = ControlPlane::new(clock, store);
        let (cp_handle, _actor_task) = ControlPlaneActor::spawn(cp);

        let studio = cp_handle
            .create_studio("Real Codex Worktree Studio")
            .await
            .unwrap();
        let studio_id = studio.id;

        let coord_agent_id = AgentId::new();
        let worker_agent_id = AgentId::new();

        let coord_spec = InternalAgentSpec::new(
            coord_agent_id,
            "Coordinator",
            "coordinator",
            ModelRef::new(provider_inst_id, coord_model_id),
        );

        let worker_spec = InternalAgentSpec::new(
            worker_agent_id,
            "Worker 1",
            "worker",
            ModelRef::new(provider_inst_id, worker_model_id),
        )
        .with_workspace_access(WorkspaceAccessMode::Mutating);

        let team_spec = InternalTeamSpec::new(studio_id, "codex-wt-team", coord_spec)
            .add_agent("worker-1", worker_spec)
            .unwrap()
            .with_max_parallel_agents(2);

        let executor = CodexAgentExecutor::try_new(
            factory,
            thread_manager,
            Arc::new(config),
            cp_handle.clone(),
        )
        .unwrap()
        .with_workspace_orchestrator(orchestrator.clone());

        let plan_decision = serde_json::json!({
            "action": "plan",
            "tasks": [
                {
                    "task_key": "task-codex-1",
                    "title": "Codex Mutating Task",
                    "description": "Mutates worktree",
                    "assigned_alias": "worker-1",
                    "depends_on": [],
                    "workspace_access": "mutating",
                    "priority": 1
                }
            ]
        });
        let plan_str = serde_json::to_string(&plan_decision).unwrap();
        let escaped_plan = plan_str.replace('"', "\\\"");

        let coord_plan_sse = format!(
            "data: {{\"id\":\"chat-coord\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{}\"}},\"finish_reason\":null}}]}}\n\n\
             data: {{\"id\":\"chat-coord\",\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":10,\"completion_tokens\":10,\"total_tokens\":20}}}}\n\n\
             data: [DONE]\n\n",
            escaped_plan
        );

        let coord_review_sse = "data: {\"id\":\"chat-coord-review\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Final review: Codex mutation task succeeded and was reconciled.\"},\"finish_reason\":null}]}\n\n\
                                data: {\"id\":\"chat-coord-review\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{{\"prompt_tokens\":10,\"completion_tokens\":10,\"total_tokens\":20}}\n\n\
                                data: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("Available team members:"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(coord_plan_sse),
            )
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("Review the executed tasks"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(coord_review_sse),
            )
            .mount(&server)
            .await;

        let sse_tool = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-t1",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "tool_calls": [{
                            "index": 0,
                            "id": "call_wt_tool_1",
                            "type": "function",
                            "function": {
                                "name": "write_workspace_file",
                                "arguments": serde_json::to_string(&serde_json::json!({
                                    "filename": "feature_real_codex.rs",
                                    "content": "pub fn real_codex() -> bool { true }\n"
                                })).unwrap()
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 10,
                    "total_tokens": 20
                }
            })
        );

        let sse_worker_done = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-t2",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "content": "File written successfully."
                    },
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 10,
                    "total_tokens": 20
                }
            })
        );

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("Codex Mutating Task"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_tool),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("write_workspace_file"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_worker_done),
            )
            .mount(&server)
            .await;

        let arbitrator = WorkspacePolicyArbitrator::new();
        let supervisor =
            AgentStudiosSupervisor::new(cp_handle.clone(), team_spec, arbitrator, executor)
                .with_workspace_orchestrator(
                    orchestrator.clone(),
                    repo_dir.clone(),
                    base_commit.clone(),
                )
                .with_integration_worktree(integration_wt.root.clone());

        let summary = supervisor
            .run("Execute real Codex worktree workflow")
            .await
            .unwrap();

        assert_eq!(summary.total_tasks, 1);
        assert_eq!(summary.completed_tasks, 1);
        assert_eq!(summary.failed_tasks, 0);

        let reconciled_file = integration_wt.root.join("feature_real_codex.rs");
        assert!(
            reconciled_file.exists(),
            "Reconciled file must exist in integration worktree"
        );
        let content = fs::read_to_string(&reconciled_file).unwrap();
        assert_eq!(
            content.replace("\r\n", "\n"),
            "pub fn real_codex() -> bool { true }\n"
        );

        let cp_state = cp_handle.get_state().await.unwrap();
        let tasks: Vec<_> = cp_state.task_graph.all_tasks().collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].state, TaskState::Succeeded);

        let recons: Vec<_> = cp_state.reconciliations.values().collect();
        assert_eq!(recons.len(), 1);
        assert_eq!(recons[0].state, ReconciliationState::Applied);

        let worktrees: Vec<_> = cp_state.worktrees.values().collect();
        assert_eq!(worktrees.len(), 2);
        let task_worktree = worktrees
            .iter()
            .find(|w| w.name != "integration-worktree")
            .expect("Task worktree must exist");
        let patch_id = task_worktree
            .patch_artifact_id
            .expect("Patch artifact ID must be recorded");
        let patch_artifact = cp_state
            .artifacts
            .get(&patch_id)
            .expect("Artifact record must exist");
        assert!(patch_artifact.location.starts_with("blobs/sha256/"));
    });
}

#[test]
fn test_real_codex_parallel_mutating_workers_use_distinct_worktrees() {
    run_with_large_stack(async {
        let temp = tempdir().expect("tempdir");
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).expect("create repo dir");
        let web_dir = repo_dir.join("apps").join("web");
        fs::create_dir_all(&web_dir).expect("create web dir");

        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&repo_dir)
            .output()
            .expect("git init");
        Command::new("git")
            .args(["config", "user.name", "Test Agent"])
            .current_dir(&repo_dir)
            .output()
            .expect("git config user.name");
        Command::new("git")
            .args(["config", "user.email", "test@agentstudios.local"])
            .current_dir(&repo_dir)
            .output()
            .expect("git config user.email");
        Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(&repo_dir)
            .output()
            .expect("git config commit.gpgsign false");

        let base_file = web_dir.join("base.txt");
        fs::write(&base_file, "Initial base in apps/web\n").expect("write base.txt");

        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_dir)
            .output()
            .expect("git add");
        Command::new("git")
            .args(["commit", "-m", "Initial commit with apps/web/base.txt"])
            .current_dir(&repo_dir)
            .output()
            .expect("git commit");

        let rev_out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo_dir)
            .output()
            .expect("git rev-parse HEAD");
        let base_commit = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();

        let managed_root = temp.path().join("managed_worktrees");
        let artifact_root = temp.path().join("artifacts");
        let orchestrator = Arc::new(
            WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator"),
        );

        let integration_wt = orchestrator
            .create_worktree(&web_dir, Some(&base_commit))
            .expect("create integration wt");

        let server = MockServer::start().await;
        let secret_resolver = InMemorySecretResolver::new();
        secret_resolver.insert(
            SecretReference {
                backend: SecretBackend::EnvironmentVariable,
                locator: "TEST_API_KEY".to_string(),
            },
            "dummy-api-key",
        );
        let secret_resolver = Arc::new(secret_resolver);

        let mut catalog = ProviderCatalog::new();
        let provider_inst_id = register_provider_and_instance(
            &mut catalog,
            "real-codex-provider-parallel",
            "Real Codex Provider Parallel",
            ProtocolFamily::OpenAiChatCompletions,
            &server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "TEST_API_KEY".to_string(),
                },
            },
        );

        let coord_model_id = ModelId::new("coord-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    provider_inst_id,
                    coord_model_id.clone(),
                    "Coord Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let worker_model_id = ModelId::new("worker-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    provider_inst_id,
                    worker_model_id.clone(),
                    "Worker Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let catalog = Arc::new(catalog);
        let continuation_manager = Arc::new(ContinuationManager::new());
        let factory = Arc::new(AgentStudiosRuntimeSessionFactory::new(
            catalog,
            secret_resolver,
            continuation_manager,
        ));

        let codex_home = temp.path().join("codex_home");
        let config = AgentStudiosCodexRuntimeFactory::create_test_config(&codex_home)
            .await
            .expect("test config");
        let auth_manager = AuthManager::from_auth_for_testing_with_home(
            CodexAuth::from_api_key("dummy"),
            config.codex_home.to_path_buf(),
        );

        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let invoked_cwds = Arc::new(std::sync::Mutex::new(Vec::new()));
        let writer_tool = Arc::new(TestWorkspaceFileWriterTool {
            barrier: Some(barrier),
            invoked_cwds: invoked_cwds.clone(),
        });

        let mut builder = build_agent_studios_extension_builder::<Config>();
        builder.tool_contributor(writer_tool);
        let extensions = Arc::new(builder.build());

        let thread_manager = AgentStudiosCodexRuntimeFactory::build_thread_manager_with_extensions(
            &config,
            auth_manager,
            Some(SessionSource::Exec),
            extensions,
        )
        .await
        .expect("build thread manager");
        let thread_manager = Arc::new(thread_manager);

        let clock = SystemClock;
        let store = InMemoryStore::new();
        let cp = ControlPlane::new(clock, store);
        let (cp_handle, _actor_task) = ControlPlaneActor::spawn(cp);

        let studio = cp_handle
            .create_studio("Real Codex Parallel Worktree Studio")
            .await
            .unwrap();
        let studio_id = studio.id;

        let coord_agent_id = AgentId::new();
        let worker_a_agent_id = AgentId::new();
        let worker_b_agent_id = AgentId::new();

        let coord_spec = InternalAgentSpec::new(
            coord_agent_id,
            "Coordinator",
            "coordinator",
            ModelRef::new(provider_inst_id, coord_model_id),
        );

        let worker_a_spec = InternalAgentSpec::new(
            worker_a_agent_id,
            "Worker A",
            "worker",
            ModelRef::new(provider_inst_id, worker_model_id.clone()),
        )
        .with_workspace_access(WorkspaceAccessMode::Mutating);

        let worker_b_spec = InternalAgentSpec::new(
            worker_b_agent_id,
            "Worker B",
            "worker",
            ModelRef::new(provider_inst_id, worker_model_id),
        )
        .with_workspace_access(WorkspaceAccessMode::Mutating);

        let team_spec = InternalTeamSpec::new(studio_id, "parallel-codex-team", coord_spec)
            .add_agent("worker-a", worker_a_spec)
            .unwrap()
            .add_agent("worker-b", worker_b_spec)
            .unwrap()
            .with_max_parallel_agents(2);

        let executor = CodexAgentExecutor::try_new(
            factory,
            thread_manager,
            Arc::new(config),
            cp_handle.clone(),
        )
        .unwrap()
        .with_workspace_orchestrator(orchestrator.clone());

        let plan_decision = serde_json::json!({
            "action": "plan",
            "tasks": [
                {
                    "task_key": "task-a",
                    "title": "Codex Mutating Task A",
                    "description": "Mutates worktree A",
                    "assigned_alias": "worker-a",
                    "depends_on": [],
                    "workspace_access": "mutating",
                    "priority": 1
                },
                {
                    "task_key": "task-b",
                    "title": "Codex Mutating Task B",
                    "description": "Mutates worktree B",
                    "assigned_alias": "worker-b",
                    "depends_on": [],
                    "workspace_access": "mutating",
                    "priority": 1
                }
            ]
        });
        let plan_str = serde_json::to_string(&plan_decision).unwrap();
        let escaped_plan = plan_str.replace('"', "\\\"");

        let coord_plan_sse = format!(
            "data: {{\"id\":\"chat-coord-p\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{}\"}},\"finish_reason\":null}}]}}\n\n\
             data: {{\"id\":\"chat-coord-p\",\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":10,\"completion_tokens\":10,\"total_tokens\":20}}}}\n\n\
             data: [DONE]\n\n",
            escaped_plan
        );

        let coord_review_sse = "data: {\"id\":\"chat-coord-review-p\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Final review: Both parallel mutating tasks succeeded.\"},\"finish_reason\":null}]}\n\n\
                                data: {\"id\":\"chat-coord-review-p\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{{\"prompt_tokens\":10,\"completion_tokens\":10,\"total_tokens\":20}}\n\n\
                                data: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("Available team members:"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(coord_plan_sse),
            )
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("Review the executed tasks"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(coord_review_sse),
            )
            .mount(&server)
            .await;

        let sse_tool_a = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-tool-a",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "tool_calls": [{
                            "index": 0,
                            "id": "call_wt_tool_a",
                            "type": "function",
                            "function": {
                                "name": "write_workspace_file",
                                "arguments": serde_json::to_string(&serde_json::json!({
                                    "filename": "a.txt",
                                    "content": "pub fn worker_a() -> bool { true }\n"
                                })).unwrap()
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": { "prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20 }
            })
        );

        let sse_tool_b = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-tool-b",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "tool_calls": [{
                            "index": 0,
                            "id": "call_wt_tool_b",
                            "type": "function",
                            "function": {
                                "name": "write_workspace_file",
                                "arguments": serde_json::to_string(&serde_json::json!({
                                    "filename": "b.txt",
                                    "content": "pub fn worker_b() -> bool { true }\n"
                                })).unwrap()
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": { "prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20 }
            })
        );

        let sse_worker_done_a = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-done-a",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "content": "File a.txt written."
                    },
                    "finish_reason": "stop"
                }],
                "usage": { "prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20 }
            })
        );

        let sse_worker_done_b = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-done-b",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "content": "File b.txt written."
                    },
                    "finish_reason": "stop"
                }],
                "usage": { "prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20 }
            })
        );

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("Codex Mutating Task A"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_tool_a),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("Codex Mutating Task B"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_tool_b),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("call_wt_tool_a"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_worker_done_a),
            )
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("call_wt_tool_b"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_worker_done_b),
            )
            .mount(&server)
            .await;

        let arbitrator = WorkspacePolicyArbitrator::new();
        let supervisor =
            AgentStudiosSupervisor::new(cp_handle.clone(), team_spec, arbitrator, executor)
                .with_workspace_orchestrator(
                    orchestrator.clone(),
                    web_dir.clone(),
                    base_commit.clone(),
                )
                .with_integration_worktree(integration_wt.root.clone());

        let summary = supervisor
            .run("Execute real Codex parallel mutating workflow")
            .await
            .unwrap();

        assert_eq!(summary.total_tasks, 2);
        assert_eq!(summary.completed_tasks, 2);
        assert_eq!(summary.failed_tasks, 0);

        let cwds = invoked_cwds.lock().unwrap().clone();
        assert_eq!(cwds.len(), 2, "Both workers must execute tool calls");
        assert_ne!(
            cwds[0], cwds[1],
            "Worker A and Worker B must execute in distinct cwds"
        );

        let cp_state = cp_handle.get_state().await.unwrap();
        let tasks = cp_handle.get_studio_tasks(studio_id).await.unwrap();
        let task_a = tasks
            .iter()
            .find(|t| t.title == "Codex Mutating Task A")
            .unwrap();
        let task_b = tasks
            .iter()
            .find(|t| t.title == "Codex Mutating Task B")
            .unwrap();
        assert_eq!(task_a.state, TaskState::Succeeded);
        assert_eq!(task_b.state, TaskState::Succeeded);

        let worktrees: Vec<_> = cp_state
            .worktrees
            .values()
            .filter(|w| w.name != "integration-worktree")
            .collect();
        assert_eq!(worktrees.len(), 2);

        let wt_a = worktrees
            .iter()
            .find(|w| w.assigned_task_id == Some(task_a.id))
            .expect("Task A worktree must exist");
        let wt_b = worktrees
            .iter()
            .find(|w| w.assigned_task_id == Some(task_b.id))
            .expect("Task B worktree must exist");

        // 1. Worker cwds match worktree cwds
        let norm_path = |p: &std::path::Path| -> PathBuf {
            let c = fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
            let s = c.to_string_lossy();
            if let Some(stripped) = s.strip_prefix(r"\\?\") {
                PathBuf::from(stripped)
            } else {
                c
            }
        };
        let canon_wt_a_cwd = norm_path(&wt_a.cwd);
        let canon_wt_b_cwd = norm_path(&wt_b.cwd);
        let canon_cwds: Vec<_> = cwds.iter().map(|c| norm_path(c)).collect();
        assert!(canon_cwds.contains(&canon_wt_a_cwd));
        assert!(canon_cwds.contains(&canon_wt_b_cwd));

        // 2. Distinct thread IDs
        let thread_a = wt_a.bound_thread_id.as_ref().expect("Thread A bound");
        let thread_b = wt_b.bound_thread_id.as_ref().expect("Thread B bound");
        assert_ne!(thread_a, thread_b);

        // 3. Upstream owner equals actual ThreadId
        assert_eq!(
            orchestrator.get_owner(&wt_a.root).unwrap(),
            Some(thread_a.clone())
        );
        assert_eq!(
            orchestrator.get_owner(&wt_b.root).unwrap(),
            Some(thread_b.clone())
        );

        // 4. File isolation
        assert!(
            wt_a.cwd.join("a.txt").exists(),
            "Worktree A must contain a.txt"
        );
        assert!(
            !wt_a.cwd.join("b.txt").exists(),
            "Worktree A must NOT contain b.txt"
        );

        assert!(
            wt_b.cwd.join("b.txt").exists(),
            "Worktree B must contain b.txt"
        );
        assert!(
            !wt_b.cwd.join("a.txt").exists(),
            "Worktree B must NOT contain a.txt"
        );

        // 5. Source checkout untouched
        assert!(
            !web_dir.join("a.txt").exists(),
            "Source checkout must NOT contain a.txt"
        );
        assert!(
            !web_dir.join("b.txt").exists(),
            "Source checkout must NOT contain b.txt"
        );
        assert!(
            !repo_dir.join("a.txt").exists(),
            "Source root must NOT contain a.txt"
        );
        assert!(
            !repo_dir.join("b.txt").exists(),
            "Source root must NOT contain b.txt"
        );

        // 6. Both worktrees remain intact
        assert!(wt_a.root.exists());
        assert!(wt_b.root.exists());
        assert_eq!(wt_a.state, WorktreeState::Retained);
        assert_eq!(wt_b.state, WorktreeState::Retained);

        // 7. No accidental write in worktree root
        assert!(!wt_a.root.join("a.txt").exists());
        assert!(!wt_b.root.join("b.txt").exists());
    });
}

#[test]
fn test_dependency_output_propagation_in_worktree_pipeline() {
    run_with_large_stack(async {
        let temp = tempdir().expect("tempdir");
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).expect("create repo dir");

        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&repo_dir)
            .output()
            .expect("git init");
        Command::new("git")
            .args(["config", "user.name", "Test Agent"])
            .current_dir(&repo_dir)
            .output()
            .expect("git config user.name");
        Command::new("git")
            .args(["config", "user.email", "test@agentstudios.local"])
            .current_dir(&repo_dir)
            .output()
            .expect("git config user.email");
        Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(&repo_dir)
            .output()
            .expect("git config commit.gpgsign false");

        let shared_file = repo_dir.join("shared.rs");
        fs::write(&shared_file, "pub fn shared_feature() -> i32 { 10 }\n")
            .expect("write shared.rs");

        Command::new("git")
            .args(["add", "shared.rs"])
            .current_dir(&repo_dir)
            .output()
            .expect("git add");
        Command::new("git")
            .args(["commit", "-m", "Initial commit with shared.rs"])
            .current_dir(&repo_dir)
            .output()
            .expect("git commit");

        let rev_out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo_dir)
            .output()
            .expect("git rev-parse HEAD");
        let base_commit = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();

        let managed_root = temp.path().join("managed_worktrees");
        let artifact_root = temp.path().join("artifacts");
        let orchestrator = Arc::new(
            WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator"),
        );

        let integration_wt = orchestrator
            .create_worktree(&repo_dir, Some(&base_commit))
            .expect("create integration wt");

        let server = MockServer::start().await;
        let secret_resolver = InMemorySecretResolver::new();
        secret_resolver.insert(
            SecretReference {
                backend: SecretBackend::EnvironmentVariable,
                locator: "TEST_API_KEY".to_string(),
            },
            "dummy-api-key",
        );
        let secret_resolver = Arc::new(secret_resolver);

        let mut catalog = ProviderCatalog::new();
        let provider_inst_id = register_provider_and_instance(
            &mut catalog,
            "real-codex-provider-dep",
            "Real Codex Provider Dep",
            ProtocolFamily::OpenAiChatCompletions,
            &server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "TEST_API_KEY".to_string(),
                },
            },
        );

        let coord_model_id = ModelId::new("coord-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    provider_inst_id,
                    coord_model_id.clone(),
                    "Coord Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let worker_model_id = ModelId::new("worker-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    provider_inst_id,
                    worker_model_id.clone(),
                    "Worker Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let catalog = Arc::new(catalog);
        let continuation_manager = Arc::new(ContinuationManager::new());
        let factory = Arc::new(AgentStudiosRuntimeSessionFactory::new(
            catalog,
            secret_resolver,
            continuation_manager,
        ));

        let codex_home = temp.path().join("codex_home");
        let config = AgentStudiosCodexRuntimeFactory::create_test_config(&codex_home)
            .await
            .expect("test config");
        let auth_manager = AuthManager::from_auth_for_testing_with_home(
            CodexAuth::from_api_key("dummy"),
            config.codex_home.to_path_buf(),
        );

        let writer_tool = Arc::new(TestWorkspaceFileWriterTool::default());
        let reader_tool = Arc::new(TestWorkspaceFileReaderTool::default());

        let mut builder = build_agent_studios_extension_builder::<Config>();
        builder.tool_contributor(writer_tool);
        builder.tool_contributor(reader_tool);
        let extensions = Arc::new(builder.build());

        let thread_manager = AgentStudiosCodexRuntimeFactory::build_thread_manager_with_extensions(
            &config,
            auth_manager,
            Some(SessionSource::Exec),
            extensions,
        )
        .await
        .expect("build thread manager");
        let thread_manager = Arc::new(thread_manager);

        let clock = SystemClock;
        let store = InMemoryStore::new();
        let cp = ControlPlane::new(clock, store);
        let (cp_handle, _actor_task) = ControlPlaneActor::spawn(cp);

        let studio = cp_handle
            .create_studio("Real Codex Dependency Propagation Studio")
            .await
            .unwrap();
        let studio_id = studio.id;

        let coord_agent_id = AgentId::new();
        let worker_1_agent_id = AgentId::new();
        let worker_2_agent_id = AgentId::new();

        let coord_spec = InternalAgentSpec::new(
            coord_agent_id,
            "Coordinator",
            "coordinator",
            ModelRef::new(provider_inst_id, coord_model_id),
        );

        let worker_1_spec = InternalAgentSpec::new(
            worker_1_agent_id,
            "Worker 1",
            "worker",
            ModelRef::new(provider_inst_id, worker_model_id.clone()),
        )
        .with_workspace_access(WorkspaceAccessMode::Mutating);

        let worker_2_spec = InternalAgentSpec::new(
            worker_2_agent_id,
            "Worker 2",
            "worker",
            ModelRef::new(provider_inst_id, worker_model_id),
        )
        .with_workspace_access(WorkspaceAccessMode::Mutating);

        let team_spec = InternalTeamSpec::new(studio_id, "dep-codex-team", coord_spec)
            .add_agent("worker-1", worker_1_spec)
            .unwrap()
            .add_agent("worker-2", worker_2_spec)
            .unwrap();

        let executor = CodexAgentExecutor::try_new(
            factory,
            thread_manager,
            Arc::new(config),
            cp_handle.clone(),
        )
        .unwrap()
        .with_workspace_orchestrator(orchestrator.clone());

        let plan_decision = serde_json::json!({
            "action": "plan",
            "tasks": [
                {
                    "task_key": "task-a",
                    "title": "Task A Mutate Shared",
                    "description": "Mutates shared.rs",
                    "assigned_alias": "worker-1",
                    "depends_on": [],
                    "workspace_access": "mutating",
                    "priority": 10
                },
                {
                    "task_key": "task-b",
                    "title": "Task B Inspect Reconciled",
                    "description": "Reads reconciled shared.rs",
                    "assigned_alias": "worker-2",
                    "depends_on": ["task-a"],
                    "dependency_output_policy": "reconciled_output",
                    "workspace_access": "mutating",
                    "priority": 5
                }
            ]
        });
        let plan_str = serde_json::to_string(&plan_decision).unwrap();
        let escaped_plan = plan_str.replace('"', "\\\"");

        let coord_plan_sse = format!(
            "data: {{\"id\":\"chat-coord-dep\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{}\"}},\"finish_reason\":null}}]}}\n\n\
             data: {{\"id\":\"chat-coord-dep\",\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":10,\"completion_tokens\":10,\"total_tokens\":20}}}}\n\n\
             data: [DONE]\n\n",
            escaped_plan
        );

        let coord_review_sse = "data: {\"id\":\"chat-coord-review-dep\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Final review: Dependency pipeline succeeded.\"},\"finish_reason\":null}]}\n\n\
                                data: {\"id\":\"chat-coord-review-dep\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{{\"prompt_tokens\":10,\"completion_tokens\":10,\"total_tokens\":20}}\n\n\
                                data: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("Available team members:"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(coord_plan_sse),
            )
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("Review the executed tasks"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(coord_review_sse),
            )
            .mount(&server)
            .await;

        let sse_tool_a = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-dep-a",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "tool_calls": [{
                            "index": 0,
                            "id": "call_wt_tool_dep_a",
                            "type": "function",
                            "function": {
                                "name": "write_workspace_file",
                                "arguments": serde_json::to_string(&serde_json::json!({
                                    "filename": "shared.rs",
                                    "content": "pub fn shared_feature() -> i32 { 42 }\n"
                                })).unwrap()
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": { "prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20 }
            })
        );

        let sse_worker_done_a = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-done-dep-a",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "content": "Updated shared.rs"
                    },
                    "finish_reason": "stop"
                }],
                "usage": { "prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20 }
            })
        );

        let sse_tool_b = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-dep-b",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "tool_calls": [{
                            "index": 0,
                            "id": "call_wt_tool_dep_b",
                            "type": "function",
                            "function": {
                                "name": "read_workspace_file",
                                "arguments": serde_json::to_string(&serde_json::json!({
                                    "filename": "shared.rs"
                                })).unwrap()
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": { "prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20 }
            })
        );

        let sse_worker_done_b = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-done-dep-b",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "content": "Read reconciled shared.rs"
                    },
                    "finish_reason": "stop"
                }],
                "usage": { "prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20 }
            })
        );

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("Task A Mutate Shared"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_tool_a),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("call_wt_tool_dep_a"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_worker_done_a),
            )
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("Task B Inspect Reconciled"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_tool_b),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("call_wt_tool_dep_b"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_worker_done_b),
            )
            .mount(&server)
            .await;

        let arbitrator = WorkspacePolicyArbitrator::new();
        let supervisor =
            AgentStudiosSupervisor::new(cp_handle.clone(), team_spec, arbitrator, executor)
                .with_workspace_orchestrator(
                    orchestrator.clone(),
                    repo_dir.clone(),
                    base_commit.clone(),
                )
                .with_integration_worktree(integration_wt.root.clone());

        eprintln!(
            ">>> STARTING SUPERVISOR RUN in test_dependency_output_propagation_in_worktree_pipeline"
        );
        let summary = supervisor
            .run("Execute dependency propagation pipeline")
            .await
            .unwrap();
        eprintln!(
            ">>> FINISHED SUPERVISOR RUN in test_dependency_output_propagation_in_worktree_pipeline"
        );

        assert_eq!(summary.total_tasks, 2);
        assert_eq!(summary.completed_tasks, 2);
        assert_eq!(summary.failed_tasks, 0);

        // 1. Integration worktree received Task A's modification
        let int_shared = fs::read_to_string(integration_wt.root.join("shared.rs")).unwrap();
        assert_eq!(
            int_shared.replace("\r\n", "\n"),
            "pub fn shared_feature() -> i32 { 42 }\n"
        );

        let int_head = orchestrator
            .resolve_head_commit_async(integration_wt.root.clone())
            .await
            .unwrap();
        assert_ne!(int_head, base_commit);

        // 2. Task B worktree branched from integration HEAD and had A's reconciled modification
        let cp_state = cp_handle.get_state().await.unwrap();
        let tasks = cp_handle.get_studio_tasks(studio_id).await.unwrap();
        let task_a = tasks
            .iter()
            .find(|t| t.title == "Task A Mutate Shared")
            .unwrap();
        let task_b = tasks
            .iter()
            .find(|t| t.title == "Task B Inspect Reconciled")
            .unwrap();

        let wt_a = cp_state
            .worktrees
            .values()
            .find(|w| w.assigned_task_id == Some(task_a.id))
            .unwrap();
        let wt_b = cp_state
            .worktrees
            .values()
            .find(|w| w.assigned_task_id == Some(task_b.id))
            .unwrap();

        assert_eq!(
            wt_b.base_commit, int_head,
            "Task B base commit must match integration HEAD"
        );
        assert_ne!(
            wt_b.root, wt_a.root,
            "Task B must execute in separate worktree from Task A"
        );

        let b_shared = fs::read_to_string(wt_b.cwd.join("shared.rs")).unwrap();
        assert_eq!(
            b_shared.replace("\r\n", "\n"),
            "pub fn shared_feature() -> i32 { 42 }\n",
            "Task B cwd must contain Task A's reconciled change"
        );
    });
}

#[tokio::test]
async fn test_reconciliation_conflict_preserves_dependent_task_blocked() {
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

    // Introduce conflicting direct edit in integration worktree
    let conflict_file = integration_wt.root.join("README.md");
    fs::write(
        &conflict_file,
        "# Integration Conflict Header\nDirect change\n",
    )
    .expect("write conflict");
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&integration_wt.root)
        .output()
        .expect("git add in integration");
    Command::new("git")
        .args(["commit", "-m", "Direct conflicting commit in integration"])
        .current_dir(&integration_wt.root)
        .output()
        .expect("git commit in integration");

    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    let (cp_handle, _task) = ControlPlaneActor::spawn(cp);

    let studio = cp_handle
        .create_studio("Reconciliation Conflict Preserves Blocked Studio")
        .await
        .unwrap();

    let coord = InternalAgentSpec::new(AgentId::new(), "Coord", "Coordinator", dummy_model());
    let worker1 = InternalAgentSpec::new(AgentId::new(), "Worker1", "Worker", dummy_model())
        .with_workspace_access(WorkspaceAccessMode::Mutating);
    let worker2 = InternalAgentSpec::new(AgentId::new(), "Worker2", "Worker", dummy_model())
        .with_workspace_access(WorkspaceAccessMode::Mutating);

    let team = InternalTeamSpec::new(studio.id, "conflict-blocked-team", coord)
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
                        title: "Task 1 Conflicting".to_string(),
                        description: None,
                        assigned_alias: "w1".to_string(),
                        depends_on: vec![],
                        workspace_access: Some(WorkspaceAccessMode::Mutating),
                        priority: Some(10),
                        ..Default::default()
                    },
                    PlannedTask {
                        task_key: "t2".to_string(),
                        title: "Task 2 Blocked on ReconciledOutput".to_string(),
                        description: None,
                        assigned_alias: "w2".to_string(),
                        depends_on: vec!["t1".to_string()],
                        dependency_output_policy: DependencyOutputPolicy::ReconciledOutput,
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

        // Worker 1 modifies README.md on the old base_commit which will conflict with integration
        let ws = ctx.execution_workspace.cwd().to_path_buf();
        let readme = ws.join("README.md");
        fs::write(&readme, "# Worker 1 Conflicting Header\nWorker edit\n").expect("write readme");

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
        .with_failure_policy(FailurePolicy::ContinueIndependent)
        .with_workspace_orchestrator(orchestrator, repo_dir, base_commit)
        .with_integration_worktree(integration_wt.root);

    let summary = supervisor.run("Run conflicting pipeline").await.unwrap();

    // Mandatory Invariant from Phase I:
    // Task 1 succeeded, reconciliation conflicted.
    // Task 2 remained Blocked and was NOT Cancelled!
    assert_eq!(summary.total_tasks, 2);
    assert_eq!(summary.completed_tasks, 1);
    assert_eq!(summary.failed_tasks, 0);
    assert_eq!(summary.cancelled_tasks, 0);
    assert_eq!(summary.blocked_tasks, 1);

    let tasks = cp_handle.get_studio_tasks(studio.id).await.unwrap();
    let t1 = tasks
        .iter()
        .find(|t| t.title == "Task 1 Conflicting")
        .unwrap();
    let t2 = tasks
        .iter()
        .find(|t| t.title == "Task 2 Blocked on ReconciledOutput")
        .unwrap();

    assert_eq!(t1.state, TaskState::Succeeded);
    assert_eq!(t2.state, TaskState::Blocked);

    let cp_state = cp_handle.get_state().await.unwrap();
    let recons: Vec<_> = cp_state.reconciliations.values().collect();
    assert_eq!(recons.len(), 1);
    assert_eq!(recons[0].state, ReconciliationState::Conflicted);
}

#[tokio::test]
async fn test_foreign_owner_preflight_blocks_execution_before_inference_and_tools() {
    run_with_large_stack(async {
        let temp = tempdir().expect("tempdir");
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).expect("create repo dir");
        let base_commit = init_test_git_repo(&repo_dir);

        let managed_root = temp.path().join("managed_worktrees");
        let artifact_root = temp.path().join("artifacts");
        let orchestrator = Arc::new(
            WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator"),
        );

        let managed_wt = orchestrator
            .create_worktree(&repo_dir, Some(&base_commit))
            .expect("create managed worktree");

        // Foreign owner actively binds the worktree checkout
        orchestrator
            .bind_thread(&managed_wt.root, "foreign-thread-999")
            .expect("bind foreign thread");

        let server = MockServer::start().await;
        // Mount an expectation of 0 calls on wiremock: any POST /chat/completions fails
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;

        let secret_resolver = InMemorySecretResolver::new();
        secret_resolver.insert(
            SecretReference {
                backend: SecretBackend::EnvironmentVariable,
                locator: "TEST_API_KEY".to_string(),
            },
            "dummy-api-key",
        );
        let secret_resolver = Arc::new(secret_resolver);

        let mut catalog = ProviderCatalog::new();
        let provider_inst_id = register_provider_and_instance(
            &mut catalog,
            "foreign-preflight-provider",
            "Foreign Preflight Provider",
            ProtocolFamily::OpenAiChatCompletions,
            &server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "TEST_API_KEY".to_string(),
                },
            },
        );

        let worker_model_id = ModelId::new("worker-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    provider_inst_id,
                    worker_model_id.clone(),
                    "Worker Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let catalog = Arc::new(catalog);
        let continuation_manager = Arc::new(ContinuationManager::new());
        let factory = Arc::new(AgentStudiosRuntimeSessionFactory::new(
            catalog,
            secret_resolver,
            continuation_manager,
        ));

        let codex_home = temp.path().join("codex_home");
        let config = AgentStudiosCodexRuntimeFactory::create_test_config(&codex_home)
            .await
            .expect("test config");
        let auth_manager = AuthManager::from_auth_for_testing_with_home(
            CodexAuth::from_api_key("dummy"),
            config.codex_home.to_path_buf(),
        );

        let writer_tool = Arc::new(TestWorkspaceFileWriterTool::default());

        let mut builder = build_agent_studios_extension_builder::<Config>();
        builder.tool_contributor(writer_tool.clone());
        let extensions = Arc::new(builder.build());

        let thread_manager = AgentStudiosCodexRuntimeFactory::build_thread_manager_with_extensions(
            &config,
            auth_manager,
            Some(SessionSource::Exec),
            extensions,
        )
        .await
        .expect("build thread manager");
        let thread_manager = Arc::new(thread_manager);

        let clock = SystemClock;
        let store = InMemoryStore::new();
        let cp = ControlPlane::new(clock, store);
        let (cp_handle, _actor_task) = ControlPlaneActor::spawn(cp);

        let studio = cp_handle
            .create_studio("Foreign Owner Studio")
            .await
            .unwrap();

        let worker_agent_id = AgentId::new();
        let worker_spec = InternalAgentSpec::new(
            worker_agent_id,
            "Worker 1",
            "worker",
            ModelRef::new(provider_inst_id, worker_model_id),
        )
        .with_workspace_access(WorkspaceAccessMode::Mutating);

        let executor = CodexAgentExecutor::try_new(
            factory,
            thread_manager,
            Arc::new(config),
            cp_handle.clone(),
        )
        .unwrap()
        .with_workspace_orchestrator(orchestrator.clone());

        let mut context = AgentExecutionContext::new(
            studio.id,
            worker_spec,
            "Should be blocked before inference or tool dispatch",
        );
        let wt_id = WorktreeId::new();
        context.execution_workspace = ExecutionWorkspace::managed(
            wt_id,
            managed_wt.root.clone(),
            managed_wt.cwd.clone(),
            repo_dir.clone(),
            repo_dir.clone(),
            base_commit.clone(),
        );

        let res = executor.execute_agent(context).await;

        // Invariant: Fails closed with typed WorktreeOwnershipConflict before inference and tools
        match res {
            Err(InternalAgentError::WorktreeOwnershipConflict {
                agent_id,
                worktree_root,
                owner_thread_id,
            }) => {
                assert_eq!(agent_id, worker_agent_id);
                assert_eq!(worktree_root, managed_wt.root);
                assert_eq!(owner_thread_id, "foreign-thread-999");
            }
            other => panic!("expected WorktreeOwnershipConflict, got {other:?}"),
        }

        // Invariant: Zero tool calls executed
        assert!(writer_tool.invoked_cwds.lock().unwrap().is_empty());
        // Invariant: Zero inference requests dispatched
        server.verify().await;
    })
}

#[tokio::test]
async fn test_ownership_query_failure_fails_closed_before_inference_and_tools() {
    run_with_large_stack(async {
        let temp = tempdir().expect("tempdir");
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).expect("create repo dir");
        let base_commit = init_test_git_repo(&repo_dir);

        let managed_root = temp.path().join("managed_worktrees");
        let artifact_root = temp.path().join("artifacts");
        let orchestrator = Arc::new(
            WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator"),
        );

        let server = MockServer::start().await;
        // Mock server expects zero calls
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;

        let secret_resolver = InMemorySecretResolver::new();
        secret_resolver.insert(
            SecretReference {
                backend: SecretBackend::EnvironmentVariable,
                locator: "TEST_API_KEY".to_string(),
            },
            "dummy-api-key",
        );
        let secret_resolver = Arc::new(secret_resolver);

        let mut catalog = ProviderCatalog::new();
        let provider_inst_id = register_provider_and_instance(
            &mut catalog,
            "query-failure-provider",
            "Query Failure Provider",
            ProtocolFamily::OpenAiChatCompletions,
            &server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "TEST_API_KEY".to_string(),
                },
            },
        );

        let worker_model_id = ModelId::new("worker-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    provider_inst_id,
                    worker_model_id.clone(),
                    "Worker Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let catalog = Arc::new(catalog);
        let continuation_manager = Arc::new(ContinuationManager::new());
        let factory = Arc::new(AgentStudiosRuntimeSessionFactory::new(
            catalog,
            secret_resolver,
            continuation_manager,
        ));

        let codex_home = temp.path().join("codex_home");
        let config = AgentStudiosCodexRuntimeFactory::create_test_config(&codex_home)
            .await
            .expect("test config");
        let auth_manager = AuthManager::from_auth_for_testing_with_home(
            CodexAuth::from_api_key("dummy"),
            config.codex_home.to_path_buf(),
        );

        let writer_tool = Arc::new(TestWorkspaceFileWriterTool::default());

        let mut builder = build_agent_studios_extension_builder::<Config>();
        builder.tool_contributor(writer_tool.clone());
        let extensions = Arc::new(builder.build());

        let thread_manager = AgentStudiosCodexRuntimeFactory::build_thread_manager_with_extensions(
            &config,
            auth_manager,
            Some(SessionSource::Exec),
            extensions,
        )
        .await
        .expect("build thread manager");
        let thread_manager = Arc::new(thread_manager);

        let clock = SystemClock;
        let store = InMemoryStore::new();
        let cp = ControlPlane::new(clock, store);
        let (cp_handle, _actor_task) = ControlPlaneActor::spawn(cp);

        let studio = cp_handle
            .create_studio("Query Failure Studio")
            .await
            .unwrap();

        let worker_agent_id = AgentId::new();
        let worker_spec = InternalAgentSpec::new(
            worker_agent_id,
            "Worker 1",
            "worker",
            ModelRef::new(provider_inst_id, worker_model_id),
        )
        .with_workspace_access(WorkspaceAccessMode::Mutating);

        let executor = CodexAgentExecutor::try_new(
            factory,
            thread_manager,
            Arc::new(config),
            cp_handle.clone(),
        )
        .unwrap()
        .with_workspace_orchestrator(orchestrator.clone());

        // Target an unresolvable/non-existent managed worktree path
        let invalid_root = temp.path().join("does_not_exist_wt");
        let mut context = AgentExecutionContext::new(
            studio.id,
            worker_spec,
            "Should fail-closed before inference or tools",
        );
        let wt_id = WorktreeId::new();
        context.execution_workspace = ExecutionWorkspace::managed(
            wt_id,
            invalid_root.clone(),
            invalid_root.clone(),
            repo_dir.clone(),
            repo_dir.clone(),
            base_commit.clone(),
        );

        let res = executor.execute_agent(context).await;

        // Invariant: Query failure fails closed
        assert!(matches!(res, Err(InternalAgentError::Workspace(_))));
        assert!(writer_tool.invoked_cwds.lock().unwrap().is_empty());
        server.verify().await;

        // Invariant: Orchestrator release also fails closed on query failure
        let release_res = orchestrator.release_worktree(&repo_dir, &invalid_root, false, None);
        assert!(matches!(
            release_res,
            Err(WorkspaceError::InvalidOperation(_))
        ));
    })
}

#[tokio::test]
async fn test_same_workspace_worker_reuse_preserves_thread_id() {
    run_with_large_stack(async {
        let temp = tempdir().expect("tempdir");
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).expect("create repo dir");
        let base_commit = init_test_git_repo(&repo_dir);

        let managed_root = temp.path().join("managed_worktrees");
        let artifact_root = temp.path().join("artifacts");
        let orchestrator = Arc::new(
            WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator"),
        );

        let managed_wt = orchestrator
            .create_worktree(&repo_dir, Some(&base_commit))
            .expect("create managed worktree");

        let server = MockServer::start().await;
        let turn_sse = "data: {\"id\":\"chat-turn\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Turn complete\"},\"finish_reason\":null}]}\n\n\
                        data: {\"id\":\"chat-turn\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":10,\"total_tokens\":20}}\n\n\
                        data: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(turn_sse),
            )
            .mount(&server)
            .await;

        let secret_resolver = InMemorySecretResolver::new();
        secret_resolver.insert(
            SecretReference {
                backend: SecretBackend::EnvironmentVariable,
                locator: "TEST_API_KEY".to_string(),
            },
            "dummy-api-key",
        );
        let secret_resolver = Arc::new(secret_resolver);

        let mut catalog = ProviderCatalog::new();
        let provider_inst_id = register_provider_and_instance(
            &mut catalog,
            "reuse-worker-provider",
            "Reuse Worker Provider",
            ProtocolFamily::OpenAiChatCompletions,
            &server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "TEST_API_KEY".to_string(),
                },
            },
        );

        let worker_model_id = ModelId::new("worker-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    provider_inst_id,
                    worker_model_id.clone(),
                    "Worker Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let catalog = Arc::new(catalog);
        let continuation_manager = Arc::new(ContinuationManager::new());
        let factory = Arc::new(AgentStudiosRuntimeSessionFactory::new(
            catalog,
            secret_resolver,
            continuation_manager,
        ));

        let codex_home = temp.path().join("codex_home");
        let config = AgentStudiosCodexRuntimeFactory::create_test_config(&codex_home)
            .await
            .expect("test config");
        let auth_manager = AuthManager::from_auth_for_testing_with_home(
            CodexAuth::from_api_key("dummy"),
            config.codex_home.to_path_buf(),
        );

        let writer_tool = Arc::new(TestWorkspaceFileWriterTool::default());

        let mut builder = build_agent_studios_extension_builder::<Config>();
        builder.tool_contributor(writer_tool);
        let extensions = Arc::new(builder.build());

        let thread_manager = AgentStudiosCodexRuntimeFactory::build_thread_manager_with_extensions(
            &config,
            auth_manager,
            Some(SessionSource::Exec),
            extensions,
        )
        .await
        .expect("build thread manager");
        let thread_manager = Arc::new(thread_manager);

        let clock = SystemClock;
        let store = InMemoryStore::new();
        let cp = ControlPlane::new(clock, store);
        let (cp_handle, _actor_task) = ControlPlaneActor::spawn(cp);

        let studio = cp_handle
            .create_studio("Reuse Worker Studio")
            .await
            .unwrap();

        let worker_agent_id = AgentId::new();
        let worker_spec = InternalAgentSpec::new(
            worker_agent_id,
            "Worker 1",
            "worker",
            ModelRef::new(provider_inst_id, worker_model_id),
        )
        .with_workspace_access(WorkspaceAccessMode::Mutating);

        let executor = CodexAgentExecutor::try_new(
            factory,
            thread_manager,
            Arc::new(config),
            cp_handle.clone(),
        )
        .unwrap()
        .with_workspace_orchestrator(orchestrator.clone());

        let wt_record = cp_handle
            .create_worktree(
                studio.id,
                "reuse-wt",
                repo_dir.clone(),
                managed_wt.root.clone(),
                base_commit.clone(),
            )
            .await
            .unwrap();
        cp_handle
            .transition_worktree_state(wt_record.id, WorktreeState::Ready)
            .await
            .unwrap();

        let workspace = ExecutionWorkspace::managed(
            wt_record.id,
            managed_wt.root.clone(),
            managed_wt.cwd.clone(),
            repo_dir.clone(),
            repo_dir.clone(),
            base_commit.clone(),
        );

        // Turn 1
        let mut ctx1 = AgentExecutionContext::new(studio.id, worker_spec.clone(), "Turn 1");
        ctx1.execution_workspace = workspace.clone();
        let res1 = executor.execute_agent(ctx1).await.unwrap();
        assert!(res1.success);

        let thread_id_1 = {
            let running = executor.running_agents();
            let agents = running.read().await;
            agents.get(&worker_agent_id).unwrap().thread_id
        };

        // Turn 2 on the exact same workspace
        let mut ctx2 = AgentExecutionContext::new(studio.id, worker_spec.clone(), "Turn 2");
        ctx2.execution_workspace = workspace;
        let res2 = executor.execute_agent(ctx2).await.unwrap();
        assert!(res2.success);

        let thread_id_2 = {
            let running = executor.running_agents();
            let agents = running.read().await;
            agents.get(&worker_agent_id).unwrap().thread_id
        };

        // Invariant: Same workspace reuses existing ThreadId without unbinding or replacement
        assert_eq!(thread_id_1, thread_id_2);
    })
}

#[tokio::test]
async fn test_same_workspace_worker_reuse_revalidates_owner_and_fails_closed_on_foreign_owner() {
    run_with_large_stack(async {
        let temp = tempdir().expect("tempdir");
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).expect("create repo dir");
        let base_commit = init_test_git_repo(&repo_dir);

        let managed_root = temp.path().join("managed_worktrees");
        let artifact_root = temp.path().join("artifacts");
        let orchestrator = Arc::new(
            WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator"),
        );

        let managed_wt = orchestrator
            .create_worktree(&repo_dir, Some(&base_commit))
            .expect("create managed worktree");

        let server = MockServer::start().await;
        let turn_sse = "data: {\"id\":\"chat-turn\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Turn complete\"},\"finish_reason\":null}]}\n\n\
                        data: {\"id\":\"chat-turn\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":10,\"total_tokens\":20}}\n\n\
                        data: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(turn_sse),
            )
            .mount(&server)
            .await;

        let secret_resolver = InMemorySecretResolver::new();
        secret_resolver.insert(
            SecretReference {
                backend: SecretBackend::EnvironmentVariable,
                locator: "TEST_API_KEY".to_string(),
            },
            "dummy-api-key",
        );
        let secret_resolver = Arc::new(secret_resolver);

        let mut catalog = ProviderCatalog::new();
        let provider_inst_id = register_provider_and_instance(
            &mut catalog,
            "reuse-worker-provider-foreign",
            "Reuse Worker Provider Foreign",
            ProtocolFamily::OpenAiChatCompletions,
            &server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "TEST_API_KEY".to_string(),
                },
            },
        );

        let worker_model_id = ModelId::new("worker-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    provider_inst_id,
                    worker_model_id.clone(),
                    "Worker Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let catalog = Arc::new(catalog);
        let continuation_manager = Arc::new(ContinuationManager::new());
        let factory = Arc::new(AgentStudiosRuntimeSessionFactory::new(
            catalog,
            secret_resolver,
            continuation_manager,
        ));

        let codex_home = temp.path().join("codex_home");
        let config = AgentStudiosCodexRuntimeFactory::create_test_config(&codex_home)
            .await
            .expect("test config");
        let auth_manager = AuthManager::from_auth_for_testing_with_home(
            CodexAuth::from_api_key("dummy"),
            config.codex_home.to_path_buf(),
        );

        let writer_tool = Arc::new(TestWorkspaceFileWriterTool::default());

        let mut builder = build_agent_studios_extension_builder::<Config>();
        builder.tool_contributor(writer_tool);
        let extensions = Arc::new(builder.build());

        let thread_manager = AgentStudiosCodexRuntimeFactory::build_thread_manager_with_extensions(
            &config,
            auth_manager,
            Some(SessionSource::Exec),
            extensions,
        )
        .await
        .expect("build thread manager");
        let thread_manager = Arc::new(thread_manager);

        let clock = SystemClock;
        let store = InMemoryStore::new();
        let cp = ControlPlane::new(clock, store);
        let (cp_handle, _actor_task) = ControlPlaneActor::spawn(cp);

        let studio = cp_handle
            .create_studio("Reuse Worker Studio Foreign")
            .await
            .unwrap();

        let worker_agent_id = AgentId::new();
        let worker_spec = InternalAgentSpec::new(
            worker_agent_id,
            "Worker 1",
            "worker",
            ModelRef::new(provider_inst_id, worker_model_id),
        )
        .with_workspace_access(WorkspaceAccessMode::Mutating);

        let executor = CodexAgentExecutor::try_new(
            factory,
            thread_manager,
            Arc::new(config),
            cp_handle.clone(),
        )
        .unwrap()
        .with_workspace_orchestrator(orchestrator.clone());

        let wt_record = cp_handle
            .create_worktree(
                studio.id,
                "reuse-wt-foreign",
                repo_dir.clone(),
                managed_wt.root.clone(),
                base_commit.clone(),
            )
            .await
            .unwrap();
        cp_handle
            .transition_worktree_state(wt_record.id, WorktreeState::Ready)
            .await
            .unwrap();

        let workspace = ExecutionWorkspace::managed(
            wt_record.id,
            managed_wt.root.clone(),
            managed_wt.cwd.clone(),
            repo_dir.clone(),
            repo_dir.clone(),
            base_commit.clone(),
        );

        // Turn 1 succeeds
        let mut ctx1 = AgentExecutionContext::new(studio.id, worker_spec.clone(), "Turn 1");
        ctx1.execution_workspace = workspace.clone();
        let res1 = executor.execute_agent(ctx1).await.unwrap();
        assert!(res1.success);

        // Mutate upstream worktree ownership metadata to simulate foreign ownership
        let git_path_out = std::process::Command::new("git")
            .current_dir(&managed_wt.root)
            .args(["rev-parse", "--git-path", "codex-thread.json"])
            .output()
            .expect("rev-parse --git-path");
        let meta_rel = String::from_utf8_lossy(&git_path_out.stdout)
            .trim()
            .to_string();
        let meta_file = managed_wt.root.join(meta_rel);
        fs::write(
            &meta_file,
            r#"{"version":1,"ownerThreadId":"foreign-thread-999"}"#,
        )
        .expect("overwrite owner metadata");

        // Turn 2 on the exact same workspace must fail closed with WorktreeOwnershipConflict
        let mut ctx2 = AgentExecutionContext::new(studio.id, worker_spec.clone(), "Turn 2");
        ctx2.execution_workspace = workspace;
        let res2 = executor.execute_agent(ctx2).await;

        match res2 {
            Err(InternalAgentError::WorktreeOwnershipConflict {
                agent_id,
                worktree_root,
                owner_thread_id,
            }) => {
                assert_eq!(agent_id, worker_agent_id);
                assert_eq!(worktree_root, managed_wt.root);
                assert_eq!(owner_thread_id, "foreign-thread-999");
            }
            other => panic!("expected WorktreeOwnershipConflict, got {other:?}"),
        }
    })
}

#[tokio::test]
async fn test_workspace_change_replacement_waits_for_termination_and_spawns_distinct_thread() {
    run_with_large_stack(async {
        let temp = tempdir().expect("tempdir");
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).expect("create repo dir");
        let base_commit = init_test_git_repo(&repo_dir);

        let managed_root = temp.path().join("managed_worktrees");
        let artifact_root = temp.path().join("artifacts");
        let orchestrator = Arc::new(
            WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator"),
        );

        let managed_wt_1 = orchestrator
            .create_worktree(&repo_dir, Some(&base_commit))
            .expect("create managed worktree 1");
        let managed_wt_2 = orchestrator
            .create_worktree(&repo_dir, Some(&base_commit))
            .expect("create managed worktree 2");

        let server = MockServer::start().await;
        let turn_sse = "data: {\"id\":\"chat-turn\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Turn complete\"},\"finish_reason\":null}]}\n\n\
                        data: {\"id\":\"chat-turn\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":10,\"total_tokens\":20}}\n\n\
                        data: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(turn_sse),
            )
            .mount(&server)
            .await;

        let secret_resolver = InMemorySecretResolver::new();
        secret_resolver.insert(
            SecretReference {
                backend: SecretBackend::EnvironmentVariable,
                locator: "TEST_API_KEY".to_string(),
            },
            "dummy-api-key",
        );
        let secret_resolver = Arc::new(secret_resolver);

        let mut catalog = ProviderCatalog::new();
        let provider_inst_id = register_provider_and_instance(
            &mut catalog,
            "replace-worker-provider",
            "Replace Worker Provider",
            ProtocolFamily::OpenAiChatCompletions,
            &server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "TEST_API_KEY".to_string(),
                },
            },
        );

        let worker_model_id = ModelId::new("worker-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    provider_inst_id,
                    worker_model_id.clone(),
                    "Worker Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let catalog = Arc::new(catalog);
        let continuation_manager = Arc::new(ContinuationManager::new());
        let factory = Arc::new(AgentStudiosRuntimeSessionFactory::new(
            catalog,
            secret_resolver,
            continuation_manager,
        ));

        let codex_home = temp.path().join("codex_home");
        let config = AgentStudiosCodexRuntimeFactory::create_test_config(&codex_home)
            .await
            .expect("test config");
        let auth_manager = AuthManager::from_auth_for_testing_with_home(
            CodexAuth::from_api_key("dummy"),
            config.codex_home.to_path_buf(),
        );

        let writer_tool = Arc::new(TestWorkspaceFileWriterTool::default());

        let mut builder = build_agent_studios_extension_builder::<Config>();
        builder.tool_contributor(writer_tool);
        let extensions = Arc::new(builder.build());

        let thread_manager = AgentStudiosCodexRuntimeFactory::build_thread_manager_with_extensions(
            &config,
            auth_manager,
            Some(SessionSource::Exec),
            extensions,
        )
        .await
        .expect("build thread manager");
        let thread_manager = Arc::new(thread_manager);

        let clock = SystemClock;
        let store = InMemoryStore::new();
        let cp = ControlPlane::new(clock, store);
        let (cp_handle, _actor_task) = ControlPlaneActor::spawn(cp);

        let studio = cp_handle
            .create_studio("Replace Worker Studio")
            .await
            .unwrap();

        let worker_agent_id = AgentId::new();
        let worker_spec = InternalAgentSpec::new(
            worker_agent_id,
            "Worker 1",
            "worker",
            ModelRef::new(provider_inst_id, worker_model_id),
        )
        .with_workspace_access(WorkspaceAccessMode::Mutating);

        let executor = CodexAgentExecutor::try_new(
            factory,
            thread_manager,
            Arc::new(config),
            cp_handle.clone(),
        )
        .unwrap()
        .with_workspace_orchestrator(orchestrator.clone());

        let wt_record_1 = cp_handle
            .create_worktree(
                studio.id,
                "replace-wt-1",
                repo_dir.clone(),
                managed_wt_1.root.clone(),
                base_commit.clone(),
            )
            .await
            .unwrap();
        cp_handle
            .transition_worktree_state(wt_record_1.id, WorktreeState::Ready)
            .await
            .unwrap();

        let wt_record_2 = cp_handle
            .create_worktree(
                studio.id,
                "replace-wt-2",
                repo_dir.clone(),
                managed_wt_2.root.clone(),
                base_commit.clone(),
            )
            .await
            .unwrap();
        cp_handle
            .transition_worktree_state(wt_record_2.id, WorktreeState::Ready)
            .await
            .unwrap();

        let workspace_1 = ExecutionWorkspace::managed(
            wt_record_1.id,
            managed_wt_1.root.clone(),
            managed_wt_1.cwd.clone(),
            repo_dir.clone(),
            repo_dir.clone(),
            base_commit.clone(),
        );

        // Turn 1 on Workspace 1
        let mut ctx1 = AgentExecutionContext::new(studio.id, worker_spec.clone(), "Turn 1");
        ctx1.execution_workspace = workspace_1;
        let res1 = executor.execute_agent(ctx1).await.unwrap();
        assert!(res1.success);

        let (thread_id_1, thread_1) = {
            let running = executor.running_agents();
            let agents = running.read().await;
            let agent = agents.get(&worker_agent_id).unwrap();
            (agent.thread_id, agent.thread.clone())
        };

        let workspace_2 = ExecutionWorkspace::managed(
            wt_record_2.id,
            managed_wt_2.root.clone(),
            managed_wt_2.cwd.clone(),
            repo_dir.clone(),
            repo_dir.clone(),
            base_commit.clone(),
        );

        // Turn 2 on Workspace 2 (different workspace triggers replacement)
        let mut ctx2 = AgentExecutionContext::new(studio.id, worker_spec.clone(), "Turn 2");
        ctx2.execution_workspace = workspace_2;
        let res2 = executor.execute_agent(ctx2).await.unwrap();
        assert!(res2.success);

        let thread_id_2 = {
            let running = executor.running_agents();
            let agents = running.read().await;
            agents.get(&worker_agent_id).unwrap().thread_id
        };

        // Invariant: Workspace change waits for old thread termination and creates distinct ThreadId
        assert_ne!(thread_id_1, thread_id_2);
        // Verify thread 1 is terminated
        thread_1.wait_until_terminated().await;
    })
}

#[tokio::test]
async fn test_physical_removal_failure_preserves_root_and_leaves_durable_state_non_removed() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).expect("create repo dir");
    let base_commit = init_test_git_repo(&repo_dir);

    let managed_root = temp.path().join("managed_worktrees");
    let artifact_root = temp.path().join("artifacts");
    let orchestrator =
        Arc::new(WorkspaceOrchestrator::new(&managed_root, &artifact_root).expect("orchestrator"));

    let managed_wt = orchestrator
        .create_worktree(&repo_dir, Some(&base_commit))
        .expect("create managed worktree");

    // Add an ignored local file so git worktree remove safely fails upstream
    fs::write(managed_wt.root.join(".gitignore"), "ignored.log\n").unwrap();
    fs::write(
        managed_wt.root.join("ignored.log"),
        "cannot delete ignored files\n",
    )
    .unwrap();

    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    let (cp_handle, _actor_task) = ControlPlaneActor::spawn(cp);

    let studio = cp_handle
        .create_studio("Physical Removal Studio")
        .await
        .unwrap();
    let wt_record = cp_handle
        .create_worktree(
            studio.id,
            "Physical Removal",
            repo_dir.clone(),
            managed_wt.root.clone(),
            base_commit.clone(),
        )
        .await
        .unwrap();
    cp_handle
        .transition_worktree_state(wt_record.id, WorktreeState::Ready)
        .await
        .unwrap();

    // Verify orchestrator release_worktree fails cleanly without deleting root
    let release_res = orchestrator.release_worktree(&repo_dir, &managed_wt.root, false, None);
    assert!(matches!(
        release_res,
        Err(WorkspaceError::WorktreeRemovalFailed(_))
    ));
    assert!(managed_wt.root.exists());

    // When physical removal fails, record retained, NEVER record removed
    if release_res.is_ok() {
        cp_handle
            .complete_worktree_removal(wt_record.id, None)
            .await
            .unwrap();
    } else {
        cp_handle
            .record_worktree_retained(wt_record.id, Some("Physical removal failed".to_string()))
            .await
            .unwrap();
    }

    let cp_state = cp_handle.get_state().await.unwrap();
    let wt = cp_state.worktrees.get(&wt_record.id).unwrap();
    assert_eq!(wt.state, WorktreeState::Retained);
    assert_ne!(wt.state, WorktreeState::Removed);
    assert!(managed_wt.root.exists());
    assert!(managed_wt.root.join("ignored.log").exists());
}
