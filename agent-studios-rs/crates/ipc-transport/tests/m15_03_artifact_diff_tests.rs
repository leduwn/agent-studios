use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_studios_external_runtime::capabilities::RuntimeCapabilities;
use agent_studios_external_runtime::discovery::DiscoveredRuntimeInstance;
use agent_studios_external_runtime::error::{RuntimeError, SanitizedRuntimeMessage};
use agent_studios_external_runtime::event::{
    RuntimeEventKind, RuntimeEventSubscription, SessionEventHub,
};
use agent_studios_external_runtime::handle::{
    RuntimeSessionHandle, RuntimeSessionRef, RuntimeStartRequest,
};
use agent_studios_external_runtime::id::{
    RuntimeImplementationId, RuntimeInstanceId, RuntimeSessionId,
};
use agent_studios_external_runtime::input::RuntimeInput;
use agent_studios_external_runtime::lifecycle::RuntimeLifecycleState;
use agent_studios_external_runtime::traits::AgentRuntime;

use agent_studios_ipc_transport::{ClientConfig, IpcClient, IpcServer};
use agent_studios_protocol::artifact::ArtifactKind;
use agent_studios_protocol::id::{ArtifactId, StudioId, TaskId};
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::{BatchTaskSpec, DependencyOutputPolicy, TaskExecutionSpec};
use agent_studios_protocol::worktree::WorktreeState;
use agent_studios_provider::{
    AuthenticationScheme, EndpointProfile, ModelCapabilities, ModelDescriptor, ModelId,
    ModelLimits, ProtocolFamily, ProviderCatalog, ProviderId, ProviderInstance, ProviderInstanceId,
};

#[derive(Clone)]
enum MutationBehavior {
    AddModifyDelete,
    LargeDiff { line_count: usize },
    FailAfterMutate { error: String },
}

#[derive(Clone)]
struct MutatingRuntimeMock {
    impl_id: RuntimeImplementationId,
    instances: Vec<DiscoveredRuntimeInstance>,
    behavior: MutationBehavior,
    hubs: Arc<Mutex<HashMap<RuntimeSessionId, Arc<SessionEventHub>>>>,
}

impl MutatingRuntimeMock {
    fn new(impl_name: &str, instance_id: RuntimeInstanceId, behavior: MutationBehavior) -> Self {
        let impl_id = RuntimeImplementationId::new(impl_name).unwrap();
        let caps = RuntimeCapabilities::default()
            .with_streaming_events(true)
            .with_stop(true);
        let instances = vec![DiscoveredRuntimeInstance::available(
            instance_id,
            impl_id.clone(),
            format!("{impl_name} instance"),
            caps,
        )];

        Self {
            impl_id,
            instances,
            behavior,
            hubs: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl AgentRuntime for MutatingRuntimeMock {
    fn implementation_id(&self) -> &RuntimeImplementationId {
        &self.impl_id
    }

    async fn discover(&self) -> Result<Vec<DiscoveredRuntimeInstance>, RuntimeError> {
        Ok(self.instances.clone())
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities::default()
            .with_streaming_events(true)
            .with_stop(true)
    }

    async fn start(
        &self,
        request: RuntimeStartRequest,
    ) -> Result<RuntimeSessionHandle, RuntimeError> {
        let session_id = RuntimeSessionId::generate();
        let instance_id = request.instance_ref.instance_id;

        // Perform real mutations in the managed worktree filesystem
        let wt_root = request
            .workspace
            .root()
            .expect("Managed worktree must have root");

        match &self.behavior {
            MutationBehavior::AddModifyDelete => {
                // 1. Create a new file
                let new_file_path = wt_root.join("added.rs");
                std::fs::write(&new_file_path, "pub fn new_func() -> i32 { 42 }\n")
                    .map_err(|e| RuntimeError::startup_failed(e.to_string()))?;

                // 2. Modify an existing file
                let existing_path = wt_root.join("existing.txt");
                std::fs::write(
                    &existing_path,
                    "original content\nappended line 1\nappended line 2\n",
                )
                .map_err(|e| RuntimeError::startup_failed(e.to_string()))?;

                // 3. Delete a file
                let deleted_path = wt_root.join("deleted.txt");
                if deleted_path.exists() {
                    std::fs::remove_file(&deleted_path)
                        .map_err(|e| RuntimeError::startup_failed(e.to_string()))?;
                }
            }
            MutationBehavior::LargeDiff { line_count } => {
                let large_file = wt_root.join("large_generated.txt");
                let mut content = String::with_capacity(line_count * 40);
                for i in 0..*line_count {
                    content.push_str(&format!(
                        "Line {i:05}: generated content for pagination test\n"
                    ));
                }
                std::fs::write(&large_file, content)
                    .map_err(|e| RuntimeError::startup_failed(e.to_string()))?;
            }
            MutationBehavior::FailAfterMutate { .. } => {
                let fail_file = wt_root.join("partial_work.txt");
                std::fs::write(&fail_file, "Work in progress before crash\n")
                    .map_err(|e| RuntimeError::startup_failed(e.to_string()))?;
            }
        }

        let handle = RuntimeSessionHandle::new(
            session_id,
            instance_id,
            self.impl_id.clone(),
            RuntimeLifecycleState::Running,
            request.workspace,
            request.workspace_access_mode,
            self.capabilities().await,
            request.correlation,
            chrono::Utc::now(),
        );

        let hub = Arc::new(SessionEventHub::new(session_id, instance_id, 128));
        let _ = hub
            .emit(RuntimeEventKind::SessionStarted {
                session_id,
                instance_id,
            })
            .await;

        // Emit terminal event according to configured behavior
        match &self.behavior {
            MutationBehavior::AddModifyDelete | MutationBehavior::LargeDiff { .. } => {
                let _ = hub
                    .emit(RuntimeEventKind::Completed {
                        summary: Some("Deterministic mutations complete".to_string()),
                    })
                    .await;
            }
            MutationBehavior::FailAfterMutate { error } => {
                let safe_summary = SanitizedRuntimeMessage::new(error.clone());
                let _ = hub
                    .emit(RuntimeEventKind::Failed {
                        safe_error_summary: safe_summary,
                    })
                    .await;
            }
        }

        self.hubs.lock().unwrap().insert(session_id, hub);
        Ok(handle)
    }

    async fn send(
        &self,
        _session: &RuntimeSessionRef,
        _input: RuntimeInput,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn interrupt(&self, _session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn resume(
        &self,
        _session: &RuntimeSessionRef,
        _input: Option<RuntimeInput>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn stop(&self, _session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn status(
        &self,
        _session: &RuntimeSessionRef,
    ) -> Result<RuntimeLifecycleState, RuntimeError> {
        Ok(RuntimeLifecycleState::Running)
    }

    async fn events_after(
        &self,
        session: &RuntimeSessionRef,
        after_sequence: Option<u64>,
    ) -> Result<RuntimeEventSubscription, RuntimeError> {
        let hub = {
            self.hubs
                .lock()
                .unwrap()
                .get(&session.session_id)
                .cloned()
                .unwrap_or_else(|| {
                    Arc::new(SessionEventHub::new(
                        session.session_id,
                        session.instance_id,
                        128,
                    ))
                })
        };
        hub.subscribe(after_sequence).await
    }
}

struct IsolatedTestRepo {
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
}

impl IsolatedTestRepo {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_path = dir.path().to_path_buf();

        std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&repo_path)
            .output()
            .expect("git init");

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo_path)
            .output()
            .expect("git config user.name");

        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo_path)
            .output()
            .expect("git config user.email");

        std::fs::write(repo_path.join("README.md"), "# Test Repo\n").expect("write README");
        std::fs::write(repo_path.join("existing.txt"), "original content\n")
            .expect("write existing");
        std::fs::write(repo_path.join("deleted.txt"), "content to be deleted\n")
            .expect("write deleted");

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&repo_path)
            .output()
            .expect("git add");

        std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&repo_path)
            .output()
            .expect("git commit");

        Self {
            _dir: dir,
            path: repo_path,
        }
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn create_orchestrator(&self) -> Arc<agent_studios_workspace::WorkspaceOrchestrator> {
        let wt_root = self.path().join("worktrees");
        let art_root = self.path().join("artifacts");
        Arc::new(
            agent_studios_workspace::WorkspaceOrchestrator::new(wt_root, art_root)
                .expect("create workspace orchestrator"),
        )
    }
}

fn create_test_catalog() -> (ProviderCatalog, ProviderInstanceId, ModelId) {
    let mut catalog = ProviderCatalog::with_builtin_definitions();

    let prov_id = ProviderInstanceId::new();
    let secret = agent_studios_provider::secret::SecretReference::new(
        agent_studios_provider::secret::SecretBackend::EnvironmentVariable,
        "TEST_API_KEY_SECRET_REF",
    )
    .unwrap();
    let prov = ProviderInstance::new(
        prov_id,
        ProviderId::new("anthropic").unwrap(),
        "Anthropic Provider",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new("https://api.anthropic.com").unwrap(),
        AuthenticationScheme::BearerToken { secret },
    )
    .unwrap();
    catalog.register_provider_instance(prov).unwrap();

    let model_id = ModelId::new("claude-3-7-sonnet").unwrap();
    let model = ModelDescriptor::new(
        prov_id,
        model_id.clone(),
        "Claude 3.7 Sonnet",
        ModelCapabilities::default(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(model).unwrap();

    (catalog, prov_id, model_id)
}

async fn wait_for_run_terminal_state(
    client: &mut IpcClient,
    studio_id: StudioId,
    task_id: TaskId,
) -> agent_studios_protocol::run::RunRecord {
    let start = std::time::Instant::now();
    loop {
        let runs = client.list_runs(studio_id).await.unwrap();
        if let Some(run) = runs
            .into_iter()
            .find(|r| r.task_id == task_id && r.state.is_terminal())
        {
            return run;
        }
        if start.elapsed() > Duration::from_secs(10) {
            panic!("Timed out waiting for terminal state for task {task_id}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn make_task_spec(
    key: &str,
    title: &str,
    repo_path: &std::path::Path,
    runtime_preference: Option<String>,
    runtime_instance_id: Option<String>,
) -> BatchTaskSpec {
    BatchTaskSpec {
        key: key.to_string(),
        title: title.to_string(),
        description: title.to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
        execution: Some(TaskExecutionSpec {
            initial_prompt: Some(title.to_string()),
            repository_path: Some(repo_path.to_string_lossy().to_string()),
            runtime_preference,
            runtime_instance_id,
            provider_instance_id: None,
            model_id: None,
        }),
    }
}

#[tokio::test]
async fn test_01_full_change_capture_and_diff_review() {
    let repo = IsolatedTestRepo::new();
    let orchestrator = repo.create_orchestrator();
    let (catalog, _, _) = create_test_catalog();

    let inst_id = RuntimeInstanceId::generate();
    let runtime: Arc<dyn AgentRuntime> = Arc::new(MutatingRuntimeMock::new(
        "mutating-runtime",
        inst_id,
        MutationBehavior::AddModifyDelete,
    ));

    let key_id = "test-key-1";
    let secret = b"m15-03-test-secret-key-bytes-long".to_vec();
    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(runtime)
        .with_auto_execute(true)
        .with_provider_catalog(catalog)
        .with_workspace_orchestrator(orchestrator.clone())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .build()
        .unwrap();

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret))
        .await
        .unwrap();

    let studio = client.create_studio("Test Studio 1").await.unwrap();
    let studio_id = studio.id;

    let spec = make_task_spec(
        "task-mutating",
        "Perform add modify delete in worktree",
        repo.path(),
        Some("mutating-runtime".to_string()),
        Some(inst_id.to_string()),
    );

    let task_ids = client.submit_task(studio_id, spec).await.unwrap();
    let task_id = task_ids[0];

    // Wait for task run to reach terminal Succeeded state
    let run = wait_for_run_terminal_state(&mut client, studio_id, task_id).await;
    assert_eq!(run.state, RunState::Succeeded);

    // 1. Query ListTaskArtifacts
    let artifacts = client
        .list_task_artifacts(studio_id, task_id)
        .await
        .unwrap();
    assert!(
        artifacts.len() >= 2,
        "Expected at least patch and stats artifacts, found {}",
        artifacts.len()
    );

    let patch_artifact = artifacts
        .iter()
        .find(|a| a.kind == ArtifactKind::Patch)
        .expect("Patch artifact must be registered");
    assert_eq!(patch_artifact.task_id, task_id);
    assert_eq!(patch_artifact.studio_id, studio_id);
    assert!(patch_artifact.content_hash.is_some());

    let stats_artifact = artifacts
        .iter()
        .find(|a| a.kind == ArtifactKind::Report)
        .expect("Stats artifact must be registered");
    assert_eq!(stats_artifact.task_id, task_id);

    // 2. Query GetArtifact for patch
    let (retrieved_art_opt, patch_content) = client
        .get_artifact(studio_id, patch_artifact.id)
        .await
        .unwrap();
    let retrieved_art = retrieved_art_opt.expect("Artifact record must be present");
    assert_eq!(retrieved_art.id, patch_artifact.id);
    let patch_text = patch_content.expect("Patch content blob must be present");
    assert!(patch_text.contains("diff --git a/added.rs b/added.rs"));
    assert!(patch_text.contains("diff --git a/existing.txt b/existing.txt"));
    assert!(patch_text.contains("diff --git a/deleted.txt b/deleted.txt"));

    // 3. Query GetTaskDiff
    let diff_resp = client
        .get_task_diff(studio_id, task_id, None, None)
        .await
        .unwrap();
    assert_eq!(diff_resp.studio_id, studio_id);
    assert_eq!(diff_resp.task_id, task_id);
    assert_eq!(diff_resp.artifact_id, Some(patch_artifact.id));
    assert_eq!(diff_resp.summary.files_changed, 3);
    assert_eq!(diff_resp.summary.added_files, vec!["added.rs"]);
    assert_eq!(diff_resp.summary.modified_files, vec!["existing.txt"]);
    assert_eq!(diff_resp.summary.deleted_files, vec!["deleted.txt"]);
    assert_eq!(diff_resp.offset, 0);
    assert!(!diff_resp.has_more);
    assert_eq!(diff_resp.patch, patch_text);

    // 4. Verify WorktreeRecord state in Control Plane
    let s_state = server.state();
    let cp = s_state.control_plane.read().await;
    let wt_rec = cp
        .state()
        .worktrees
        .values()
        .next()
        .expect("Worktree record must exist");
    assert_eq!(wt_rec.state, WorktreeState::ChangeCaptured);
    assert_eq!(wt_rec.patch_artifact_id, Some(patch_artifact.id));

    client.disconnect().await;
}

#[tokio::test]
async fn test_02_diff_pagination_and_frame_bounding() {
    let repo = IsolatedTestRepo::new();
    let orchestrator = repo.create_orchestrator();
    let (catalog, _, _) = create_test_catalog();

    let inst_id = RuntimeInstanceId::generate();
    let runtime: Arc<dyn AgentRuntime> = Arc::new(MutatingRuntimeMock::new(
        "large-diff-runtime",
        inst_id,
        MutationBehavior::LargeDiff { line_count: 500 },
    ));

    let key_id = "test-key-2";
    let secret = b"m15-03-test-secret-key-bytes-long".to_vec();
    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(runtime)
        .with_auto_execute(true)
        .with_provider_catalog(catalog)
        .with_workspace_orchestrator(orchestrator.clone())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .build()
        .unwrap();

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret))
        .await
        .unwrap();

    let studio = client.create_studio("Test Studio 2").await.unwrap();
    let studio_id = studio.id;

    let spec = make_task_spec(
        "task-large-diff",
        "Generate 500 lines for pagination test",
        repo.path(),
        Some("large-diff-runtime".to_string()),
        Some(inst_id.to_string()),
    );

    let task_ids = client.submit_task(studio_id, spec).await.unwrap();
    let task_id = task_ids[0];

    let run = wait_for_run_terminal_state(&mut client, studio_id, task_id).await;
    assert_eq!(run.state, RunState::Succeeded);

    // Retrieve full diff first to know total size
    let full_diff = client
        .get_task_diff(studio_id, task_id, None, None)
        .await
        .unwrap();
    let total_bytes = full_diff.total_bytes;
    assert!(
        total_bytes > 2048,
        "Total diff bytes ({total_bytes}) should exceed 2048 for pagination"
    );

    // Request Chunk 1: offset=0, limit=512
    let chunk1 = client
        .get_task_diff(studio_id, task_id, Some(0), Some(512))
        .await
        .unwrap();
    assert_eq!(chunk1.offset, 0);
    assert_eq!(chunk1.limit, 512);
    assert!(chunk1.has_more);
    assert_eq!(chunk1.patch.len(), 512);

    // Request Chunk 2: offset=512, limit=512
    let chunk2 = client
        .get_task_diff(studio_id, task_id, Some(512), Some(512))
        .await
        .unwrap();
    assert_eq!(chunk2.offset, 512);
    assert_eq!(chunk2.limit, 512);
    assert_eq!(chunk2.patch.len(), 512);

    // Reconstructed prefix matches full patch
    let reconstructed = format!("{}{}", chunk1.patch, chunk2.patch);
    assert_eq!(&full_diff.patch[0..1024], &reconstructed);

    // Request with limit exceeding 2 MiB ceiling is clamped safely
    let oversized = client
        .get_task_diff(studio_id, task_id, Some(0), Some(10 * 1024 * 1024))
        .await
        .unwrap();
    assert_eq!(oversized.limit, 2 * 1024 * 1024);

    client.disconnect().await;
}

#[tokio::test]
async fn test_03_artifact_capture_on_failure() {
    let repo = IsolatedTestRepo::new();
    let orchestrator = repo.create_orchestrator();
    let (catalog, _, _) = create_test_catalog();

    let inst_fail = RuntimeInstanceId::generate();
    let rt_fail: Arc<dyn AgentRuntime> = Arc::new(MutatingRuntimeMock::new(
        "fail-runtime",
        inst_fail,
        MutationBehavior::FailAfterMutate {
            error: "Simulated runtime crash".to_string(),
        },
    ));

    let key_id = "test-key-3";
    let secret = b"m15-03-test-secret-key-bytes-long".to_vec();
    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(rt_fail)
        .with_auto_execute(true)
        .with_provider_catalog(catalog)
        .with_workspace_orchestrator(orchestrator.clone())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .build()
        .unwrap();

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret))
        .await
        .unwrap();

    let studio = client.create_studio("Test Studio 3").await.unwrap();
    let studio_id = studio.id;

    let spec = make_task_spec(
        "task-fail",
        "Mutate then fail",
        repo.path(),
        Some("fail-runtime".to_string()),
        Some(inst_fail.to_string()),
    );

    let task_ids = client.submit_task(studio_id, spec).await.unwrap();
    let task_id = task_ids[0];

    // Wait for terminal state (Failed)
    let run = wait_for_run_terminal_state(&mut client, studio_id, task_id).await;
    assert_eq!(run.state, RunState::Failed);

    // Verify partial changes are captured and available even on failure!
    let artifacts = client
        .list_task_artifacts(studio_id, task_id)
        .await
        .unwrap();
    assert!(
        !artifacts.is_empty(),
        "Artifacts should be captured on failure"
    );

    let diff = client
        .get_task_diff(studio_id, task_id, None, None)
        .await
        .unwrap();
    assert_eq!(diff.summary.files_changed, 1);
    assert_eq!(diff.summary.added_files, vec!["partial_work.txt"]);
    assert!(diff.patch.contains("Work in progress before crash"));

    client.disconnect().await;
}

#[tokio::test]
async fn test_04_cross_studio_authorization_and_negative_queries() {
    let repo = IsolatedTestRepo::new();
    let orchestrator = repo.create_orchestrator();
    let (catalog, _, _) = create_test_catalog();

    let inst_id = RuntimeInstanceId::generate();
    let runtime: Arc<dyn AgentRuntime> = Arc::new(MutatingRuntimeMock::new(
        "auth-test-runtime",
        inst_id,
        MutationBehavior::AddModifyDelete,
    ));

    let key_id = "test-key-4";
    let secret = b"m15-03-test-secret-key-bytes-long".to_vec();
    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(runtime)
        .with_auto_execute(true)
        .with_provider_catalog(catalog)
        .with_workspace_orchestrator(orchestrator.clone())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .build()
        .unwrap();

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret))
        .await
        .unwrap();

    let studio = client.create_studio("Test Studio 4").await.unwrap();
    let studio_a = studio.id;
    let studio_b = StudioId::new(); // Unassociated studio

    let spec = make_task_spec(
        "task-auth",
        "Authoritative studio isolation test",
        repo.path(),
        Some("auth-test-runtime".to_string()),
        Some(inst_id.to_string()),
    );

    let task_ids = client.submit_task(studio_a, spec).await.unwrap();
    let task_id = task_ids[0];

    let run = wait_for_run_terminal_state(&mut client, studio_a, task_id).await;
    assert_eq!(run.state, RunState::Succeeded);

    // Cross-studio ListTaskArtifacts with studio_b should fail
    let list_err = client.list_task_artifacts(studio_b, task_id).await;
    assert!(
        list_err.is_err(),
        "Cross-studio list_task_artifacts must fail"
    );

    // Cross-studio GetTaskDiff with studio_b should fail
    let diff_err = client.get_task_diff(studio_b, task_id, None, None).await;
    assert!(diff_err.is_err(), "Cross-studio get_task_diff must fail");

    // Query nonexistent artifact ID returns None
    let fake_art_id = ArtifactId::new();
    let (fake_art, fake_content) = client.get_artifact(studio_a, fake_art_id).await.unwrap();
    assert!(fake_art.is_none());
    assert!(fake_content.is_none());

    client.disconnect().await;
}
