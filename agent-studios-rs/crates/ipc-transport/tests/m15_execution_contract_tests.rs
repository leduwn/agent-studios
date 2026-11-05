use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::timeout;

use agent_studios_external_runtime::capabilities::RuntimeCapabilities;
use agent_studios_external_runtime::discovery::DiscoveredRuntimeInstance;
use agent_studios_external_runtime::error::RuntimeError;
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

use agent_studios_ipc_transport::framing::{recv_msg, send_msg};
use agent_studios_ipc_transport::{ClientConfig, IpcClient, IpcCommand, IpcResponse, IpcServer};
use agent_studios_protocol::id::{StudioId, TaskId};
use agent_studios_protocol::run::{RunRecord, RunState};
use agent_studios_protocol::task::{
    BatchTaskSpec, DependencyOutputPolicy, TaskExecutionSpec, TaskState,
};
use agent_studios_provider::{
    AuthenticationScheme, EndpointProfile, ModelCapabilities, ModelDescriptor, ModelId,
    ModelLimits, ModelRef, ProtocolFamily, ProviderCatalog, ProviderId, ProviderInstance,
    ProviderInstanceId,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct TestMockRuntime {
    impl_id: RuntimeImplementationId,
    instance_id: RuntimeInstanceId,
    started_requests: Arc<Mutex<Vec<RuntimeStartRequest>>>,
    hubs: Arc<Mutex<HashMap<RuntimeSessionId, Arc<SessionEventHub>>>>,
}

impl TestMockRuntime {
    fn new(impl_name: &str) -> Self {
        Self {
            impl_id: RuntimeImplementationId::new(impl_name).unwrap(),
            instance_id: RuntimeInstanceId::generate(),
            started_requests: Arc::new(Mutex::new(Vec::new())),
            hubs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn started_requests(&self) -> Vec<RuntimeStartRequest> {
        self.started_requests.lock().unwrap().clone()
    }

    async fn wait_for_started_requests(&self, count: usize) -> Vec<RuntimeStartRequest> {
        let start = std::time::Instant::now();
        loop {
            let reqs = self.started_requests();
            if reqs.len() >= count {
                return reqs;
            }
            if start.elapsed() > Duration::from_secs(5) {
                panic!(
                    "Timed out waiting for {} started requests; have {}",
                    count,
                    reqs.len()
                );
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

#[async_trait]
impl AgentRuntime for TestMockRuntime {
    fn implementation_id(&self) -> &RuntimeImplementationId {
        &self.impl_id
    }

    async fn discover(&self) -> Result<Vec<DiscoveredRuntimeInstance>, RuntimeError> {
        let caps = RuntimeCapabilities::default()
            .with_streaming_events(true)
            .with_stop(true);
        Ok(vec![DiscoveredRuntimeInstance::available(
            self.instance_id,
            self.impl_id.clone(),
            "Mock Test Instance",
            caps,
        )])
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
        self.started_requests.lock().unwrap().push(request.clone());
        let session_id = RuntimeSessionId::generate();
        Ok(RuntimeSessionHandle::new(
            session_id,
            self.instance_id,
            self.impl_id.clone(),
            RuntimeLifecycleState::Running,
            request.workspace,
            request.workspace_access_mode,
            self.capabilities().await,
            request.correlation,
            chrono::Utc::now(),
        ))
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
        let existing = { self.hubs.lock().unwrap().get(&session.session_id).cloned() };
        let hub = if let Some(existing) = existing {
            existing
        } else {
            let new_hub = Arc::new(SessionEventHub::new(
                session.session_id,
                session.instance_id,
                128,
            ));
            let _ = new_hub
                .emit(RuntimeEventKind::SessionStarted {
                    session_id: session.session_id,
                    instance_id: session.instance_id,
                })
                .await;
            self.hubs
                .lock()
                .unwrap()
                .insert(session.session_id, new_hub.clone());
            new_hub
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
            .args(["config", "user.name", "Test Agent"])
            .current_dir(&repo_path)
            .output()
            .expect("git config user.name");

        std::process::Command::new("git")
            .args(["config", "user.email", "test@agentstudios.local"])
            .current_dir(&repo_path)
            .output()
            .expect("git config user.email");

        std::process::Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(&repo_path)
            .output()
            .expect("git config commit.gpgsign");

        let readme = repo_path.join("README.md");
        std::fs::write(&readme, "# Isolated Test Repo\n").expect("write README");

        std::process::Command::new("git")
            .args(["add", "README.md"])
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

fn create_test_catalog() -> (
    ProviderCatalog,
    ProviderInstanceId,
    ModelId,
    ProviderInstanceId,
    ModelId,
) {
    let mut catalog = ProviderCatalog::with_builtin_definitions();

    let prov1_id = ProviderInstanceId::new();
    let secret1 = agent_studios_provider::secret::SecretReference::new(
        agent_studios_provider::secret::SecretBackend::EnvironmentVariable,
        "ANTHROPIC_API_KEY_SECRET_REF_123",
    )
    .unwrap();
    let prov1 = ProviderInstance::new(
        prov1_id,
        ProviderId::new("anthropic").unwrap(),
        "Anthropic Provider",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new("https://api.anthropic.com").unwrap(),
        AuthenticationScheme::BearerToken { secret: secret1 },
    )
    .unwrap();
    catalog.register_provider_instance(prov1).unwrap();
    let model1_id = ModelId::new("claude-3-7-sonnet").unwrap();
    let model1 = ModelDescriptor::new(
        prov1_id,
        model1_id.clone(),
        "Claude 3.7 Sonnet",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(model1).unwrap();

    let prov2_id = ProviderInstanceId::new();
    let secret2 = agent_studios_provider::secret::SecretReference::new(
        agent_studios_provider::secret::SecretBackend::EnvironmentVariable,
        "OPENAI_API_KEY_SECRET_REF_999",
    )
    .unwrap();
    let prov2 = ProviderInstance::new(
        prov2_id,
        ProviderId::new("openai").unwrap(),
        "OpenAI Provider",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("https://api.openai.com").unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "api-key".into(),
            secret: secret2,
        },
    )
    .unwrap();
    catalog.register_provider_instance(prov2).unwrap();
    let model2_id = ModelId::new("gpt-4o").unwrap();
    let model2 = ModelDescriptor::new(
        prov2_id,
        model2_id.clone(),
        "GPT-4o",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(model2).unwrap();

    (catalog, prov1_id, model1_id, prov2_id, model2_id)
}

async fn wait_for_run_active(
    client: &mut IpcClient,
    studio_id: StudioId,
    task_id: TaskId,
) -> RunRecord {
    let start = std::time::Instant::now();
    loop {
        let runs = client.list_runs(studio_id).await.unwrap();
        if let Some(run) = runs.into_iter().find(|r| r.task_id == task_id) {
            let has_active_exec = run
                .execution
                .as_ref()
                .and_then(|e| e.runtime_instance_id.as_ref())
                .is_some();
            if has_active_exec || run.state.is_terminal() {
                return run;
            }
        }
        if start.elapsed() > Duration::from_secs(5) {
            panic!("Timed out waiting for active run for task {task_id}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_for_run_terminal_state(
    client: &mut IpcClient,
    studio_id: StudioId,
    task_id: TaskId,
) -> RunRecord {
    let start = std::time::Instant::now();
    loop {
        let runs = client.list_runs(studio_id).await.unwrap();
        if let Some(run) = runs
            .into_iter()
            .find(|r| r.task_id == task_id && r.state.is_terminal())
        {
            return run;
        }
        if start.elapsed() > Duration::from_secs(5) {
            panic!("Timed out waiting for terminal state for task {task_id}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn test_execution_readiness_report_truthful_and_secret_free() {
    let mock_runtime = Arc::new(TestMockRuntime::new("mock-test-runtime"));
    let (catalog, prov1_id, model1_id, _prov2_id, model2_id) = create_test_catalog();

    let repo_a = IsolatedTestRepo::new();
    let repo_b = IsolatedTestRepo::new();

    let key_id = "test-readiness-key";
    let secret = b"secret-readiness-key-123".to_vec();

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_provider_catalog(catalog)
        .with_default_repo_path(repo_a.path())
        .with_authorized_workspace(repo_a.path())
        .with_authorized_workspace(repo_b.path())
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id.clone()))
        .with_configured_instance_id(mock_runtime.instance_id)
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    let readiness = timeout(TEST_TIMEOUT, client.get_execution_readiness())
        .await
        .expect("Timeout")
        .expect("GetExecutionReadiness failed");

    // 1. Verify truthfulness
    assert!(readiness.is_ready);
    assert_eq!(readiness.available_runtimes, vec!["mock-test-runtime"]);
    assert_eq!(readiness.runtime_instances.len(), 1);
    assert_eq!(
        readiness.runtime_instances[0].instance_id,
        mock_runtime.instance_id.to_string()
    );
    assert!(readiness.runtime_instances[0].healthy);

    assert_eq!(readiness.available_models.len(), 2);
    let model_ids: Vec<&str> = readiness
        .available_models
        .iter()
        .map(|m| m.model_id.as_str())
        .collect();
    assert!(model_ids.contains(&model1_id.as_str()));
    assert!(model_ids.contains(&model2_id.as_str()));

    assert_eq!(readiness.authorized_workspaces.len(), 2);
    assert_eq!(
        readiness.defaults.default_model_id.as_deref(),
        Some(model1_id.as_str())
    );
    assert_eq!(
        readiness.defaults.default_provider_instance_id.as_deref(),
        Some(prov1_id.to_string().as_str())
    );
    assert_eq!(
        readiness.defaults.configured_instance_id.as_deref(),
        Some(mock_runtime.instance_id.to_string().as_str())
    );
    assert!(readiness.blocking_reasons.is_empty());

    // 2. Critical Security Audit: Verify zero secret exposure
    let json_str = serde_json::to_string(&readiness).unwrap();
    assert!(
        !json_str.contains("secret-anthropic-token-12345"),
        "CRITICAL: Secret token leaked in readiness report!"
    );
    assert!(
        !json_str.contains("secret-openai-key-99999"),
        "CRITICAL: API key leaked in readiness report!"
    );
    assert!(
        !json_str.contains("secret-readiness-key-123"),
        "CRITICAL: HMAC auth key leaked in readiness report!"
    );
}

#[tokio::test]
async fn test_unauthenticated_client_readiness_rejected() {
    let mock_runtime = Arc::new(TestMockRuntime::new("mock-test-runtime"));
    let key_id = "test-unauth-key";
    let secret = b"secret-unauth-key-123".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret)
        .with_runtime(mock_runtime)
        .build()
        .expect("Server failed to build");

    let (mut client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    // Attempt GetExecutionReadiness without handshake
    send_msg(&mut client_stream, &IpcCommand::GetExecutionReadiness)
        .await
        .expect("Send failed");

    let resp: IpcResponse = recv_msg(&mut client_stream, 65536)
        .await
        .expect("Recv failed");

    match resp {
        IpcResponse::Error { message } => {
            assert!(
                message.contains("Unauthenticated") || message.contains("must authenticate"),
                "Expected authentication error, got: {message}"
            );
        }
        other => panic!("Expected Error response, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_per_task_authorized_workspaces_and_unauthorized_rejection() {
    let mock_runtime = Arc::new(TestMockRuntime::new("mock-test-runtime"));
    let repo_a = IsolatedTestRepo::new();
    let repo_b = IsolatedTestRepo::new();
    let repo_unauth = IsolatedTestRepo::new();

    let orchestrator = repo_a.create_orchestrator();

    let key_id = "test-workspace-key";
    let secret = b"secret-workspace-key-123".to_vec();

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(repo_a.path())
        .with_authorized_workspace(repo_a.path())
        .with_authorized_workspace(repo_b.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    let studio = client
        .create_studio("Per-Task Workspace Studio")
        .await
        .expect("Create studio failed");

    // 1. Submit task with authorized repository A
    let spec_a = BatchTaskSpec {
        key: "task-repo-a".to_string(),
        title: "Task in Repo A".to_string(),
        description: "Executing in Repo A".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
        execution: Some(TaskExecutionSpec {
            initial_prompt: Some("Run in Repo A".to_string()),
            repository_path: Some(repo_a.path().to_string_lossy().to_string()),
            runtime_instance_id: None,
            runtime_preference: None,
            provider_instance_id: None,
            model_id: None,
        }),
    };

    let task_ids_a = client.submit_task(studio.id, spec_a).await.unwrap();
    assert_eq!(task_ids_a.len(), 1);

    // Wait for task A execution to kick off
    let run_a = wait_for_run_active(&mut client, studio.id, task_ids_a[0]).await;
    assert_ne!(run_a.state, RunState::Failed, "Task A should not fail");
    assert!(run_a.execution.is_some());

    // 2. Submit task with authorized repository B
    let spec_b = BatchTaskSpec {
        key: "task-repo-b".to_string(),
        title: "Task in Repo B".to_string(),
        description: "Executing in Repo B".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
        execution: Some(TaskExecutionSpec {
            initial_prompt: Some("Run in Repo B".to_string()),
            repository_path: Some(repo_b.path().to_string_lossy().to_string()),
            runtime_instance_id: None,
            runtime_preference: None,
            provider_instance_id: None,
            model_id: None,
        }),
    };

    let task_ids_b = client.submit_task(studio.id, spec_b).await.unwrap();
    assert_eq!(task_ids_b.len(), 1);

    let run_b = wait_for_run_active(&mut client, studio.id, task_ids_b[0]).await;
    assert_ne!(run_b.state, RunState::Failed, "Task B should not fail");

    // 3. Submit task with unauthorized repository (fails closed)
    let spec_unauth = BatchTaskSpec {
        key: "task-repo-unauth".to_string(),
        title: "Task in Unauth Repo".to_string(),
        description: "Executing in Unauthorized Repo".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
        execution: Some(TaskExecutionSpec {
            initial_prompt: Some("Run in Unauth Repo".to_string()),
            repository_path: Some(repo_unauth.path().to_string_lossy().to_string()),
            runtime_instance_id: None,
            runtime_preference: None,
            provider_instance_id: None,
            model_id: None,
        }),
    };

    let task_ids_unauth = client.submit_task(studio.id, spec_unauth).await.unwrap();
    assert_eq!(task_ids_unauth.len(), 1);

    let run_unauth = wait_for_run_terminal_state(&mut client, studio.id, task_ids_unauth[0]).await;
    assert_eq!(
        run_unauth.state,
        RunState::Failed,
        "Unauthorized repo must fail closed"
    );

    let tasks_all = client.list_tasks(studio.id).await.unwrap();
    let task_unauth = tasks_all
        .iter()
        .find(|t| t.id == task_ids_unauth[0])
        .unwrap();
    assert_eq!(
        task_unauth.state,
        TaskState::Failed,
        "Task for unauthorized repo must fail closed"
    );
}

#[tokio::test]
async fn test_per_task_non_git_and_traversal_workspace_fails_closed() {
    let mock_runtime = Arc::new(TestMockRuntime::new("mock-test-runtime"));
    let repo_valid = IsolatedTestRepo::new();
    let non_git_dir = tempfile::tempdir().unwrap();

    let orchestrator = repo_valid.create_orchestrator();
    let key_id = "test-sec-key";
    let secret = b"secret-sec-key-123".to_vec();

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime)
        .with_workspace_orchestrator(orchestrator)
        .with_authorized_workspace(repo_valid.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Sec Studio").await.unwrap();

    // 1. Non-git workspace path
    let spec_non_git = BatchTaskSpec {
        key: "task-non-git".to_string(),
        title: "Non-git task".to_string(),
        description: "Non-git".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
        execution: Some(TaskExecutionSpec {
            initial_prompt: Some("Testing non-git".to_string()),
            repository_path: Some(non_git_dir.path().to_string_lossy().to_string()),
            runtime_instance_id: None,
            runtime_preference: None,
            provider_instance_id: None,
            model_id: None,
        }),
    };

    let task_ids = client.submit_task(studio.id, spec_non_git).await.unwrap();
    let run = wait_for_run_terminal_state(&mut client, studio.id, task_ids[0]).await;
    assert_eq!(
        run.state,
        RunState::Failed,
        "Non-git directory must fail closed"
    );

    // 2. Traversal attempt
    let traversal_path = format!("{}/../../etc", repo_valid.path().display());
    let spec_traversal = BatchTaskSpec {
        key: "task-traversal".to_string(),
        title: "Traversal task".to_string(),
        description: "Traversal".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
        execution: Some(TaskExecutionSpec {
            initial_prompt: Some("Testing traversal".to_string()),
            repository_path: Some(traversal_path),
            runtime_instance_id: None,
            runtime_preference: None,
            provider_instance_id: None,
            model_id: None,
        }),
    };

    let task_ids2 = client.submit_task(studio.id, spec_traversal).await.unwrap();
    let run2 = wait_for_run_terminal_state(&mut client, studio.id, task_ids2[0]).await;
    assert_eq!(
        run2.state,
        RunState::Failed,
        "Path traversal must fail closed"
    );
}

#[tokio::test]
async fn test_per_task_model_routing_and_prompt_integrity() {
    let mock_runtime = Arc::new(TestMockRuntime::new("mock-test-runtime"));
    let (catalog, prov1_id, model1_id, prov2_id, model2_id) = create_test_catalog();

    let repo = IsolatedTestRepo::new();
    let orchestrator = repo.create_orchestrator();
    let key_id = "test-model-key";
    let secret = b"secret-model-key-123".to_vec();

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_provider_catalog(catalog)
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Model Studio").await.unwrap();

    // 1. Explicit model routing for model 1 + prompt integrity test
    let complex_prompt = "Fix bug in auth middleware.\n\n```rust\nfn check_token() -> bool { false }\n```\nKiểm tra mã xác thực UTF-8.";
    let spec1 = BatchTaskSpec {
        key: "task-model-1".to_string(),
        title: "Model 1 Task".to_string(),
        description: "Synthesized description that should NOT appear".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
        execution: Some(TaskExecutionSpec {
            initial_prompt: Some(complex_prompt.to_string()),
            repository_path: None, // Uses default
            runtime_instance_id: None,
            runtime_preference: Some("mock-test-runtime".to_string()),
            provider_instance_id: Some(prov1_id.to_string()),
            model_id: Some(model1_id.to_string()),
        }),
    };

    let task_ids1 = client.submit_task(studio.id, spec1).await.unwrap();
    let run1 = wait_for_run_active(&mut client, studio.id, task_ids1[0]).await;
    assert_ne!(run1.state, RunState::Failed);

    let reqs = mock_runtime.wait_for_started_requests(1).await;
    assert_eq!(reqs.len(), 1);
    assert_eq!(
        reqs[0].initial_prompt.as_deref(),
        Some(complex_prompt),
        "Initial prompt must be preserved verbatim without title/description concatenation"
    );
    assert_eq!(
        reqs[0].model_ref.as_ref(),
        Some(&ModelRef::new(prov1_id, model1_id.clone()))
    );

    // 2. Unambiguous model routing (specify only model_id)
    let spec2 = BatchTaskSpec {
        key: "task-model-2".to_string(),
        title: "Model 2 Task".to_string(),
        description: "Test".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
        execution: Some(TaskExecutionSpec {
            initial_prompt: Some("Run model 2".to_string()),
            repository_path: None,
            runtime_instance_id: None,
            runtime_preference: None,
            provider_instance_id: None,
            model_id: Some(model2_id.to_string()),
        }),
    };

    let task_ids2 = client.submit_task(studio.id, spec2).await.unwrap();
    let run2 = wait_for_run_active(&mut client, studio.id, task_ids2[0]).await;
    assert_ne!(run2.state, RunState::Failed);

    let reqs2 = mock_runtime.wait_for_started_requests(2).await;
    assert_eq!(reqs2.len(), 2);
    assert_eq!(
        reqs2[1].model_ref.as_ref(),
        Some(&ModelRef::new(prov2_id, model2_id.clone()))
    );

    // 3. Unknown model ID fails closed
    let spec_unknown = BatchTaskSpec {
        key: "task-model-unknown".to_string(),
        title: "Unknown Model Task".to_string(),
        description: "Test".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
        execution: Some(TaskExecutionSpec {
            initial_prompt: Some("Run nonexistent model".to_string()),
            repository_path: None,
            runtime_instance_id: None,
            runtime_preference: None,
            provider_instance_id: None,
            model_id: Some("nonexistent-super-model".to_string()),
        }),
    };

    let task_ids_unk = client.submit_task(studio.id, spec_unknown).await.unwrap();
    let run_unk = wait_for_run_terminal_state(&mut client, studio.id, task_ids_unk[0]).await;
    assert_eq!(
        run_unk.state,
        RunState::Failed,
        "Unknown model must fail closed"
    );

    // 4. Runtime preference mismatch fails closed
    let spec_mismatch = BatchTaskSpec {
        key: "task-runtime-mismatch".to_string(),
        title: "Runtime Mismatch Task".to_string(),
        description: "Test".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
        execution: Some(TaskExecutionSpec {
            initial_prompt: Some("Run with mismatched runtime".to_string()),
            repository_path: None,
            runtime_instance_id: None,
            runtime_preference: Some("claude-code".to_string()), // Server only has mock-test-runtime
            provider_instance_id: None,
            model_id: None,
        }),
    };

    let task_ids_mis = client.submit_task(studio.id, spec_mismatch).await.unwrap();
    let run_mis = wait_for_run_terminal_state(&mut client, studio.id, task_ids_mis[0]).await;
    assert_eq!(
        run_mis.state,
        RunState::Failed,
        "Runtime preference mismatch must fail closed"
    );
}

#[tokio::test]
async fn test_backward_compatible_m13_task_submission() {
    let mock_runtime = Arc::new(TestMockRuntime::new("mock-test-runtime"));
    let (catalog, prov1_id, model1_id, _, _) = create_test_catalog();

    let repo = IsolatedTestRepo::new();
    let orchestrator = repo.create_orchestrator();
    let key_id = "test-compat-key";
    let secret = b"secret-compat-key-123".to_vec();

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_provider_catalog(catalog)
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id.clone()))
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Compat Studio").await.unwrap();

    // M13 SubmitTask payload: execution is None
    let spec = BatchTaskSpec {
        key: "task-legacy".to_string(),
        title: "Legacy Task Title".to_string(),
        description: "Legacy Task Description".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
        execution: None,
    };

    let task_ids = client.submit_task(studio.id, spec).await.unwrap();
    let run = wait_for_run_active(&mut client, studio.id, task_ids[0]).await;
    assert_ne!(run.state, RunState::Failed);

    let reqs = mock_runtime.wait_for_started_requests(1).await;
    assert_eq!(reqs.len(), 1);
    // Legacy synthesis: "Title: Description"
    assert_eq!(
        reqs[0].initial_prompt.as_deref(),
        Some("Legacy Task Title: Legacy Task Description")
    );
    assert_eq!(
        reqs[0].model_ref.as_ref(),
        Some(&ModelRef::new(prov1_id, model1_id))
    );
}

#[tokio::test]
async fn test_readiness_semantics_blocking_reasons_and_edge_cases() {
    let mock_runtime = Arc::new(TestMockRuntime::new("opencode"));
    let repo = IsolatedTestRepo::new();
    let key_id = "test-readiness-edge-key";
    let secret = b"secret-readiness-edge-123".to_vec();

    // 1. No runtime configured on daemon
    let server_no_rt = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .build()
        .expect("build server");
    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server_no_rt.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .unwrap();
    let r_no_rt = client.get_execution_readiness().await.unwrap();
    assert!(!r_no_rt.is_ready);
    assert!(
        r_no_rt
            .blocking_reasons
            .iter()
            .any(|b| b.contains("No agent runtime configured")
                || b.contains("No healthy runtime instances available"))
    );

    // 2. Empty model catalog
    let server_no_model = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_provider_catalog(ProviderCatalog::new())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .build()
        .expect("build server");
    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server_no_model.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .unwrap();
    let r_no_model = client.get_execution_readiness().await.unwrap();
    assert!(!r_no_model.is_ready);
    assert!(
        r_no_model
            .blocking_reasons
            .iter()
            .any(|b| b.contains("No models available in provider catalog"))
    );

    // 3. Multiple models without default model configured
    let (catalog_multi, prov1_id, model1_id, _, _) = create_test_catalog();
    let server_multi_no_def = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_provider_catalog(catalog_multi.clone())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .build()
        .expect("build server");
    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server_multi_no_def.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .unwrap();
    let r_multi = client.get_execution_readiness().await.unwrap();
    assert!(!r_multi.is_ready);
    assert!(
        r_multi
            .blocking_reasons
            .iter()
            .any(|b| b.contains("no default model configured"))
    );

    // 4. Configured default model does not exist in catalog
    let missing_model_ref = ModelRef::new(prov1_id, ModelId::new("phantom-model").unwrap());
    let server_missing_model = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_provider_catalog(catalog_multi.clone())
        .with_default_model_ref(missing_model_ref)
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .build()
        .expect("build server");
    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server_missing_model.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .unwrap();
    let r_missing_model = client.get_execution_readiness().await.unwrap();
    assert!(!r_missing_model.is_ready);
    assert!(
        r_missing_model
            .blocking_reasons
            .iter()
            .any(|b| b.contains("not found in provider catalog"))
    );

    // 5. Configured runtime instance unavailable / non-matching
    let unavail_inst_id = RuntimeInstanceId::generate();
    let server_unavail_rt = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_provider_catalog(catalog_multi.clone())
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id.clone()))
        .with_configured_instance_id(unavail_inst_id)
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .build()
        .expect("build server");
    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server_unavail_rt.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .unwrap();
    let r_unavail_rt = client.get_execution_readiness().await.unwrap();
    assert!(!r_unavail_rt.is_ready);
    assert!(
        r_unavail_rt
            .blocking_reasons
            .iter()
            .any(|b| b.contains("not available or unhealthy"))
    );

    // 6. Default workspace is not a valid Git repo
    let non_git_dir = tempfile::tempdir().unwrap();
    let server_non_git = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_provider_catalog(catalog_multi.clone())
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id.clone()))
        .with_configured_instance_id(mock_runtime.instance_id)
        .with_default_repo_path(non_git_dir.path())
        .with_authorized_workspace(non_git_dir.path())
        .build()
        .expect("build server");
    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server_non_git.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .unwrap();
    let r_non_git = client.get_execution_readiness().await.unwrap();
    assert!(!r_non_git.is_ready);
    assert!(
        r_non_git
            .blocking_reasons
            .iter()
            .any(|b| b.contains("is not a valid Git repository (.git not found)"))
    );

    // 7. Default workspace path does not exist (canonicalization failure)
    let non_existent_path = non_git_dir.path().join("deleted-repo");
    let server_missing_dir = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_provider_catalog(catalog_multi.clone())
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id.clone()))
        .with_configured_instance_id(mock_runtime.instance_id)
        .with_default_repo_path(&non_existent_path)
        .with_authorized_workspace(&non_existent_path)
        .build()
        .expect("build server");
    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server_missing_dir.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .unwrap();
    let r_missing_dir = client.get_execution_readiness().await.unwrap();
    assert!(!r_missing_dir.is_ready);
    assert!(
        r_missing_dir
            .blocking_reasons
            .iter()
            .any(|b| b.contains("cannot be canonicalized"))
    );

    // 8. No default workspace configured on daemon
    let server_no_default_ws = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_provider_catalog(catalog_multi.clone())
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id.clone()))
        .with_configured_instance_id(mock_runtime.instance_id)
        .build()
        .expect("build server");
    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server_no_default_ws.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .unwrap();
    let r_no_default_ws = client.get_execution_readiness().await.unwrap();
    assert!(!r_no_default_ws.is_ready);
    assert!(
        r_no_default_ws
            .blocking_reasons
            .iter()
            .any(|b| b.contains("No authorized workspaces configured"))
    );
}

#[tokio::test]
async fn test_workspace_containment_and_authorization_policies() {
    let temp_root = tempfile::tempdir().unwrap();
    let root_path = dunce::canonicalize(temp_root.path()).unwrap();

    // 1. Create main repository
    let main_repo_dir = root_path.join("main-repo");
    std::fs::create_dir_all(&main_repo_dir).unwrap();
    std::process::Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(&main_repo_dir)
        .output()
        .expect("git init main");
    let canon_main = dunce::canonicalize(&main_repo_dir).unwrap();

    // 2. Create nested repository inside main repository
    let nested_repo_dir = main_repo_dir.join("nested-sub");
    std::fs::create_dir_all(&nested_repo_dir).unwrap();
    std::process::Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(&nested_repo_dir)
        .output()
        .expect("git init nested");
    let canon_nested = dunce::canonicalize(&nested_repo_dir).unwrap();

    // 3. Create outside repository
    let outside_repo_dir = root_path.join("outside-repo");
    std::fs::create_dir_all(&outside_repo_dir).unwrap();
    std::process::Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(&outside_repo_dir)
        .output()
        .expect("git init outside");
    let canon_outside = dunce::canonicalize(&outside_repo_dir).unwrap();

    // 4. Create repository with .git as a regular file (submodule/worktree gitdir reference)
    let file_git_repo_dir = root_path.join("file-git-repo");
    std::fs::create_dir_all(&file_git_repo_dir).unwrap();
    std::fs::write(file_git_repo_dir.join(".git"), "gitdir: ../somewhere\n").unwrap();
    let canon_file_git = dunce::canonicalize(&file_git_repo_dir).unwrap();

    // 5. Create non-git directory
    let non_git_dir = root_path.join("plain-folder");
    std::fs::create_dir_all(&non_git_dir).unwrap();

    // Build ServerState with hierarchical containment (default: strict_workspace_roots = false)
    let server_hierarchical = IpcServer::builder()
        .with_authorized_workspace(&canon_main)
        .with_authorized_workspace(&canon_file_git)
        .build()
        .unwrap();

    // Check 1: Exact approved repository succeeds
    let res_exact = server_hierarchical
        .state
        .verify_workspace_authorization(canon_main.to_str().unwrap());
    assert_eq!(res_exact.unwrap(), canon_main);

    // Check 2: Nested repository succeeds under hierarchical containment
    let res_nested_hier = server_hierarchical
        .state
        .verify_workspace_authorization(canon_nested.to_str().unwrap());
    assert_eq!(res_nested_hier.unwrap(), canon_nested);

    // Check 3: Outside repository rejected
    let res_outside = server_hierarchical
        .state
        .verify_workspace_authorization(canon_outside.to_str().unwrap());
    assert!(res_outside.is_err());
    assert!(
        res_outside
            .unwrap_err()
            .contains("not in the daemon authorized workspaces allowlist")
    );

    // Check 4: Non-git directory rejected
    let res_non_git = server_hierarchical
        .state
        .verify_workspace_authorization(non_git_dir.to_str().unwrap());
    assert!(res_non_git.is_err());
    assert!(
        res_non_git
            .unwrap_err()
            .contains("is not a valid Git repository (.git not found)")
    );

    // Check 5: .git file repository (submodule/worktree) succeeds
    let res_file_git = server_hierarchical
        .state
        .verify_workspace_authorization(canon_file_git.to_str().unwrap());
    assert_eq!(res_file_git.unwrap(), canon_file_git);

    // Check 6: Strict mode rejects nested repository
    let server_strict = IpcServer::builder()
        .with_authorized_workspace(&canon_main)
        .with_strict_workspace_roots(true)
        .build()
        .unwrap();

    let res_nested_strict = server_strict
        .state
        .verify_workspace_authorization(canon_nested.to_str().unwrap());
    assert!(
        res_nested_strict.is_err(),
        "Strict mode must reject nested repositories not explicitly allowlisted"
    );
    assert!(
        res_nested_strict
            .unwrap_err()
            .contains("not in the daemon authorized workspaces allowlist")
    );

    // Check 7: Deleted/moved path fails canonicalization
    let res_deleted = server_hierarchical
        .state
        .verify_workspace_authorization("non/existent/path/987654");
    assert!(res_deleted.is_err());
    assert!(res_deleted.unwrap_err().contains("Failed to canonicalize"));
}
