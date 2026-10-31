use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
use agent_studios_external_runtime::registry::RuntimeRegistry;
use agent_studios_external_runtime::traits::AgentRuntime;

use agent_studios_ipc_transport::{ClientConfig, IpcClient, IpcServer};
use agent_studios_protocol::id::{StudioId, TaskId};
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::{
    BatchTaskSpec, DependencyOutputPolicy, TaskExecutionSpec, TaskState,
};
use agent_studios_provider::{
    AuthenticationScheme, EndpointProfile, ModelCapabilities, ModelDescriptor, ModelId,
    ModelLimits, ModelRef, ProtocolFamily, ProviderCatalog, ProviderId, ProviderInstance,
    ProviderInstanceId,
};

#[derive(Clone)]
struct MultiRuntimeMock {
    impl_id: RuntimeImplementationId,
    instances: Vec<DiscoveredRuntimeInstance>,
    discovery_error: Arc<Mutex<Option<String>>>,
    started_requests: Arc<Mutex<Vec<RuntimeStartRequest>>>,
    stopped_sessions: Arc<Mutex<Vec<RuntimeSessionRef>>>,
    sent_inputs: Arc<Mutex<Vec<(RuntimeSessionRef, RuntimeInput)>>>,
    hubs: Arc<Mutex<HashMap<RuntimeSessionId, Arc<SessionEventHub>>>>,
}

impl MultiRuntimeMock {
    fn new(impl_name: &str, instance_ids: Vec<RuntimeInstanceId>) -> Self {
        let impl_id = RuntimeImplementationId::new(impl_name).unwrap();
        let caps = RuntimeCapabilities::default()
            .with_streaming_events(true)
            .with_stop(true);
        let instances = instance_ids
            .into_iter()
            .enumerate()
            .map(|(idx, id)| {
                DiscoveredRuntimeInstance::available(
                    id,
                    impl_id.clone(),
                    format!("{impl_name} instance {idx}"),
                    caps.clone(),
                )
            })
            .collect();

        Self {
            impl_id,
            instances,
            discovery_error: Arc::new(Mutex::new(None)),
            started_requests: Arc::new(Mutex::new(Vec::new())),
            stopped_sessions: Arc::new(Mutex::new(Vec::new())),
            sent_inputs: Arc::new(Mutex::new(Vec::new())),
            hubs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn with_unavailable_instance(impl_name: &str, inst_id: RuntimeInstanceId) -> Self {
        let impl_id = RuntimeImplementationId::new(impl_name).unwrap();
        let caps = RuntimeCapabilities::default()
            .with_streaming_events(true)
            .with_stop(true);
        let inst = DiscoveredRuntimeInstance::unavailable(
            inst_id,
            impl_id.clone(),
            format!("{impl_name} unavailable"),
            caps,
            "Service offline or not installed",
        );
        Self {
            impl_id,
            instances: vec![inst],
            discovery_error: Arc::new(Mutex::new(None)),
            started_requests: Arc::new(Mutex::new(Vec::new())),
            stopped_sessions: Arc::new(Mutex::new(Vec::new())),
            sent_inputs: Arc::new(Mutex::new(Vec::new())),
            hubs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn set_discovery_error(&self, err: impl Into<String>) {
        *self.discovery_error.lock().unwrap() = Some(err.into());
    }

    fn started_requests(&self) -> Vec<RuntimeStartRequest> {
        self.started_requests.lock().unwrap().clone()
    }

    fn stopped_sessions(&self) -> Vec<RuntimeSessionRef> {
        self.stopped_sessions.lock().unwrap().clone()
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

    async fn wait_for_stopped_sessions(&self, count: usize) -> Vec<RuntimeSessionRef> {
        let start = std::time::Instant::now();
        loop {
            let stops = self.stopped_sessions();
            if stops.len() >= count {
                return stops;
            }
            if start.elapsed() > Duration::from_secs(5) {
                panic!(
                    "Timed out waiting for {} stopped sessions; have {}",
                    count,
                    stops.len()
                );
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

#[async_trait]
impl AgentRuntime for MultiRuntimeMock {
    fn implementation_id(&self) -> &RuntimeImplementationId {
        &self.impl_id
    }

    async fn discover(&self) -> Result<Vec<DiscoveredRuntimeInstance>, RuntimeError> {
        if let Some(err) = self.discovery_error.lock().unwrap().as_ref() {
            return Err(RuntimeError::DiscoveryFailed {
                reason: agent_studios_external_runtime::error::SanitizedRuntimeMessage::new(
                    err.clone(),
                ),
            });
        }
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
        self.started_requests.lock().unwrap().push(request.clone());
        let session_id = RuntimeSessionId::generate();
        let instance_id = *request.instance_id();
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
        self.hubs.lock().unwrap().insert(session_id, hub);

        Ok(handle)
    }

    async fn send(
        &self,
        session: &RuntimeSessionRef,
        input: RuntimeInput,
    ) -> Result<(), RuntimeError> {
        self.sent_inputs
            .lock()
            .unwrap()
            .push((session.clone(), input));
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

    async fn stop(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        self.stopped_sessions.lock().unwrap().push(session.clone());
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
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo_path)
            .output()
            .expect("git config user.name");

        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo_path)
            .output()
            .expect("git config user.email");

        std::fs::write(repo_path.join("README.md"), "# Test Repo\n").expect("write file");

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
        ModelCapabilities::default(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(model1).unwrap();

    let prov2_id = ProviderInstanceId::new();
    let secret2 = agent_studios_provider::secret::SecretReference::new(
        agent_studios_provider::secret::SecretBackend::EnvironmentVariable,
        "OPENAI_API_KEY_SECRET_REF_456",
    )
    .unwrap();
    let prov2 = ProviderInstance::new(
        prov2_id,
        ProviderId::new("openai").unwrap(),
        "OpenAI Provider",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("https://api.openai.com").unwrap(),
        AuthenticationScheme::BearerToken { secret: secret2 },
    )
    .unwrap();
    catalog.register_provider_instance(prov2).unwrap();
    let model2_id = ModelId::new("gpt-4o").unwrap();
    let model2 = ModelDescriptor::new(
        prov2_id,
        model2_id.clone(),
        "GPT-4o",
        ModelCapabilities::default(),
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
) -> agent_studios_protocol::run::RunRecord {
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
        if start.elapsed() > Duration::from_secs(5) {
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
async fn test_01_builder_multi_runtime_registration_and_duplicate_rejection() {
    let inst_a = RuntimeInstanceId::generate();
    let inst_b = RuntimeInstanceId::generate();
    let rt_a: Arc<dyn AgentRuntime> =
        Arc::new(MultiRuntimeMock::new("runtime-alpha", vec![inst_a]));
    let rt_b: Arc<dyn AgentRuntime> = Arc::new(MultiRuntimeMock::new("runtime-beta", vec![inst_b]));

    // Registering distinct runtimes succeeds
    let server = IpcServer::builder()
        .with_runtimes(vec![rt_a.clone(), rt_b.clone()])
        .build();
    assert!(server.is_ok(), "Expected valid multi-runtime server build");

    // Registering duplicate runtime implementation ID via builder fails closed
    let duplicate_rt: Arc<dyn AgentRuntime> =
        Arc::new(MultiRuntimeMock::new("runtime-alpha", vec![inst_a]));
    let err_server = IpcServer::builder()
        .with_runtimes(vec![rt_a.clone(), duplicate_rt])
        .build();
    assert!(
        err_server.is_err(),
        "Expected duplicate runtime registration to fail"
    );

    // RuntimeRegistry::from_runtimes with duplicates fails closed
    let duplicate_rt2: Arc<dyn AgentRuntime> =
        Arc::new(MultiRuntimeMock::new("runtime-alpha", vec![inst_a]));
    let reg_res = RuntimeRegistry::from_runtimes(vec![rt_a.clone(), duplicate_rt2]);
    assert!(matches!(
        reg_res,
        Err(RuntimeError::DuplicateRuntime { .. })
    ));

    // Registering via with_runtime_registry
    let custom_reg = RuntimeRegistry::new();
    custom_reg.register(rt_a.clone()).await.unwrap();
    custom_reg.register(rt_b.clone()).await.unwrap();
    let reg_server = IpcServer::builder()
        .with_runtime_registry(custom_reg)
        .build();
    assert!(
        reg_server.is_ok(),
        "Expected server build with registry to succeed"
    );
}

#[tokio::test]
async fn test_02_readiness_aggregation_and_fault_isolation() {
    let key_id = "test-readiness-key";
    let secret = b"secret-m15-02-test-abc".to_vec();
    let repo = IsolatedTestRepo::new();
    let (catalog, prov1_id, model1_id, _, _) = create_test_catalog();

    let inst_a1 = RuntimeInstanceId::generate();
    let inst_a2 = RuntimeInstanceId::generate();
    let inst_b1 = RuntimeInstanceId::generate();

    let rt_a = Arc::new(MultiRuntimeMock::new(
        "runtime-alpha",
        vec![inst_a1, inst_a2],
    ));
    let rt_b = Arc::new(MultiRuntimeMock::new("runtime-beta", vec![inst_b1]));

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtimes(vec![
            rt_a.clone() as Arc<dyn AgentRuntime>,
            rt_b.clone() as Arc<dyn AgentRuntime>,
        ])
        .with_provider_catalog(catalog)
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id))
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .build()
        .expect("Server failed to build");

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .expect("Client handshake failed");

    // Without a configured default instance, multiple healthy runtimes mean readiness is false (ambiguous default)
    let readiness = client
        .get_execution_readiness()
        .await
        .expect("GetExecutionReadiness failed");
    assert!(!readiness.is_ready);
    assert_eq!(readiness.available_runtimes.len(), 2);
    assert_eq!(readiness.runtime_instances.len(), 3);
    assert!(
        readiness
            .blocking_reasons
            .iter()
            .any(|b| b.contains("Multiple healthy runtimes available"))
    );

    // When configured_instance_id is set to inst_a1, check readiness
    let server_configured = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtimes(vec![
            rt_a.clone() as Arc<dyn AgentRuntime>,
            rt_b.clone() as Arc<dyn AgentRuntime>,
        ])
        .with_configured_instance_id(inst_a1)
        .with_provider_catalog(ProviderCatalog::with_builtin_definitions())
        .with_default_model_ref(ModelRef::new(
            prov1_id,
            ModelId::new("claude-3-7-sonnet").unwrap(),
        ))
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .build()
        .expect("Server configured build");

    let (c_stream2, s_stream2) = tokio::io::duplex(65536);
    server_configured.handle_connection(s_stream2);
    let mut client2 =
        IpcClient::connect_stream(c_stream2, ClientConfig::new(key_id, secret.clone()))
            .await
            .expect("Client handshake failed");
    let r2 = client2.get_execution_readiness().await.unwrap();
    assert!(
        r2.runtime_instances
            .iter()
            .any(|i| i.instance_id == inst_a1.to_string() && i.healthy)
    );

    // Fault isolation: runtime B fails discovery, runtime A's instances remain visible
    rt_b.set_discovery_error("Daemon process died unexpectedly");
    let r_fault = client.get_execution_readiness().await.unwrap();
    assert_eq!(
        r_fault.runtime_instances.len(),
        2,
        "Runtime A's instances must survive Runtime B's discovery failure"
    );
    assert!(
        r_fault
            .blocking_reasons
            .iter()
            .any(|b| b.contains("Failed to discover instances for runtime 'runtime-beta'"))
    );
    assert_eq!(
        r_fault.available_runtimes,
        vec!["runtime-alpha"],
        "Failed implementations must not be advertised as available"
    );
}

#[tokio::test]
async fn test_03_deterministic_routing_explicit_preference_and_instance() {
    let key_id = "test-routing-key";
    let secret = b"secret-m15-02-test-def".to_vec();
    let repo = IsolatedTestRepo::new();
    let (catalog, prov1_id, model1_id, _, _) = create_test_catalog();

    let inst_a = RuntimeInstanceId::generate();
    let inst_b = RuntimeInstanceId::generate();
    let rt_a = Arc::new(MultiRuntimeMock::new("runtime-alpha", vec![inst_a]));
    let rt_b = Arc::new(MultiRuntimeMock::new("runtime-beta", vec![inst_b]));

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtimes(vec![
            rt_a.clone() as Arc<dyn AgentRuntime>,
            rt_b.clone() as Arc<dyn AgentRuntime>,
        ])
        .with_auto_execute(true)
        .with_workspace_orchestrator(repo.create_orchestrator())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .with_provider_catalog(catalog)
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id))
        .build()
        .expect("Server failed to build");

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Test Studio").await.unwrap();
    let studio_id = studio.id;

    // Submit task explicitly requesting runtime-beta and inst_b
    let task_spec = make_task_spec(
        "task-beta-pinned",
        "Target Beta Instance Task",
        repo.path(),
        Some("runtime-beta".to_string()),
        Some(inst_b.to_string()),
    );

    let tasks = client.submit_task(studio_id, task_spec).await.unwrap();
    let task_id = tasks[0];

    // Wait for start request on rt_b
    let reqs_b = rt_b.wait_for_started_requests(1).await;
    assert_eq!(reqs_b.len(), 1);
    assert_eq!(*reqs_b[0].instance_id(), inst_b);

    // Verify rt_a never received any request
    assert_eq!(rt_a.started_requests().len(), 0);

    // Verify RunRecord.execution records both runtime_preference and runtime_instance_id
    let run = wait_for_run_active(&mut client, studio_id, task_id).await;
    let exec = run.execution.as_ref().expect("Run execution spec");
    assert_eq!(exec.runtime_preference.as_deref(), Some("runtime-beta"));
    assert_eq!(
        exec.runtime_instance_id.as_deref(),
        Some(inst_b.to_string().as_str())
    );
}

#[tokio::test]
async fn test_04_deterministic_routing_explicit_instance_resolves_owning_runtime() {
    let key_id = "test-routing-key-2";
    let secret = b"secret-m15-02-test-ghi".to_vec();
    let repo = IsolatedTestRepo::new();
    let (catalog, prov1_id, model1_id, _, _) = create_test_catalog();

    let inst_a = RuntimeInstanceId::generate();
    let inst_b = RuntimeInstanceId::generate();
    let rt_a = Arc::new(MultiRuntimeMock::new("runtime-alpha", vec![inst_a]));
    let rt_b = Arc::new(MultiRuntimeMock::new("runtime-beta", vec![inst_b]));

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtimes(vec![
            rt_a.clone() as Arc<dyn AgentRuntime>,
            rt_b.clone() as Arc<dyn AgentRuntime>,
        ])
        .with_auto_execute(true)
        .with_workspace_orchestrator(repo.create_orchestrator())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .with_provider_catalog(catalog)
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id))
        .build()
        .expect("Server failed to build");

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Test Studio").await.unwrap();
    let studio_id = studio.id;

    // Submit task with runtime_instance_id = inst_a and NO runtime_preference
    let task_spec = make_task_spec(
        "task-alpha-inst-only",
        "Target Alpha Instance Only",
        repo.path(),
        None,
        Some(inst_a.to_string()),
    );

    let tasks = client.submit_task(studio_id, task_spec).await.unwrap();
    let task_id = tasks[0];

    // Wait for start request on rt_a
    let reqs_a = rt_a.wait_for_started_requests(1).await;
    assert_eq!(reqs_a.len(), 1);
    assert_eq!(*reqs_a[0].instance_id(), inst_a);

    // Verify rt_b received nothing
    assert_eq!(rt_b.started_requests().len(), 0);

    // Verify RunRecord.execution records effective runtime_preference = "runtime-alpha"
    let run = wait_for_run_active(&mut client, studio_id, task_id).await;
    let exec = run.execution.as_ref().expect("Run execution spec");
    assert_eq!(exec.runtime_preference.as_deref(), Some("runtime-alpha"));
    assert_eq!(
        exec.runtime_instance_id.as_deref(),
        Some(inst_a.to_string().as_str())
    );
}

#[tokio::test]
async fn test_05_deterministic_routing_explicit_preference_resolves_instance() {
    let key_id = "test-routing-key-3";
    let secret = b"secret-m15-02-test-jkl".to_vec();
    let repo = IsolatedTestRepo::new();
    let (catalog, prov1_id, model1_id, _, _) = create_test_catalog();

    let inst_a = RuntimeInstanceId::generate();
    let inst_b = RuntimeInstanceId::generate();
    let rt_a = Arc::new(MultiRuntimeMock::new("runtime-alpha", vec![inst_a]));
    let rt_b = Arc::new(MultiRuntimeMock::new("runtime-beta", vec![inst_b]));

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtimes(vec![
            rt_a.clone() as Arc<dyn AgentRuntime>,
            rt_b.clone() as Arc<dyn AgentRuntime>,
        ])
        .with_auto_execute(true)
        .with_workspace_orchestrator(repo.create_orchestrator())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .with_provider_catalog(catalog)
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id))
        .build()
        .expect("Server failed to build");

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Test Studio").await.unwrap();
    let studio_id = studio.id;

    // Submit task with runtime_preference = "runtime-alpha" and NO runtime_instance_id
    let task_spec = make_task_spec(
        "task-alpha-pref-only",
        "Target Preference Only",
        repo.path(),
        Some("runtime-alpha".to_string()),
        None,
    );

    let _ = client.submit_task(studio_id, task_spec).await.unwrap();

    // Wait for start request on rt_a
    let reqs_a = rt_a.wait_for_started_requests(1).await;
    assert_eq!(reqs_a.len(), 1);
    assert_eq!(*reqs_a[0].instance_id(), inst_a);
    assert_eq!(rt_b.started_requests().len(), 0);
}

#[tokio::test]
async fn test_06_deterministic_routing_mismatched_preference_and_instance_fails_closed() {
    let key_id = "test-routing-key-4";
    let secret = b"secret-m15-02-test-mno".to_vec();
    let repo = IsolatedTestRepo::new();
    let (catalog, prov1_id, model1_id, _, _) = create_test_catalog();

    let inst_a = RuntimeInstanceId::generate();
    let inst_b = RuntimeInstanceId::generate();
    let rt_a = Arc::new(MultiRuntimeMock::new("runtime-alpha", vec![inst_a]));
    let rt_b = Arc::new(MultiRuntimeMock::new("runtime-beta", vec![inst_b]));

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtimes(vec![
            rt_a.clone() as Arc<dyn AgentRuntime>,
            rt_b.clone() as Arc<dyn AgentRuntime>,
        ])
        .with_auto_execute(true)
        .with_workspace_orchestrator(repo.create_orchestrator())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .with_provider_catalog(catalog)
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id))
        .build()
        .expect("Server failed to build");

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Test Studio").await.unwrap();
    let studio_id = studio.id;

    // Preference "runtime-alpha" but instance belongs to "runtime-beta"
    let task_spec = make_task_spec(
        "task-mismatch",
        "Mismatched Task",
        repo.path(),
        Some("runtime-alpha".to_string()),
        Some(inst_b.to_string()),
    );

    let tasks = client.submit_task(studio_id, task_spec).await.unwrap();
    let task_id = tasks[0];

    // Neither runtime should have been started
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(rt_a.started_requests().len(), 0);
    assert_eq!(rt_b.started_requests().len(), 0);

    // Verify task failed closed
    let run = wait_for_run_terminal_state(&mut client, studio_id, task_id).await;
    assert_eq!(run.state, RunState::Failed);
    let task = client.get_task(studio_id, task_id).await.unwrap().unwrap();
    assert_eq!(task.state, TaskState::Failed);
}

#[tokio::test]
async fn test_07_deterministic_routing_unknown_runtime_fails_closed() {
    let key_id = "test-routing-key-5";
    let secret = b"secret-m15-02-test-pqr".to_vec();
    let repo = IsolatedTestRepo::new();
    let (catalog, prov1_id, model1_id, _, _) = create_test_catalog();

    let inst_a = RuntimeInstanceId::generate();
    let rt_a = Arc::new(MultiRuntimeMock::new("runtime-alpha", vec![inst_a]));

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(rt_a.clone() as Arc<dyn AgentRuntime>)
        .with_auto_execute(true)
        .with_workspace_orchestrator(repo.create_orchestrator())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .with_provider_catalog(catalog)
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id))
        .build()
        .expect("Server failed to build");

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Test Studio").await.unwrap();
    let studio_id = studio.id;

    let task_spec = make_task_spec(
        "task-unknown-rt",
        "Unknown Runtime Task",
        repo.path(),
        Some("unregistered-runtime-xyz".to_string()),
        None,
    );

    let tasks = client.submit_task(studio_id, task_spec).await.unwrap();
    let task_id = tasks[0];

    let run = wait_for_run_terminal_state(&mut client, studio_id, task_id).await;
    assert_eq!(run.state, RunState::Failed);
    let task = client.get_task(studio_id, task_id).await.unwrap().unwrap();
    assert_eq!(task.state, TaskState::Failed);
}

#[tokio::test]
async fn test_08_deterministic_routing_unhealthy_instance_fails_closed() {
    let key_id = "test-routing-key-6";
    let secret = b"secret-m15-02-test-stu".to_vec();
    let repo = IsolatedTestRepo::new();
    let (catalog, prov1_id, model1_id, _, _) = create_test_catalog();

    let unavail_inst = RuntimeInstanceId::generate();
    let rt_unavail = Arc::new(MultiRuntimeMock::with_unavailable_instance(
        "runtime-offline",
        unavail_inst,
    ));

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(rt_unavail.clone() as Arc<dyn AgentRuntime>)
        .with_auto_execute(true)
        .with_workspace_orchestrator(repo.create_orchestrator())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .with_provider_catalog(catalog)
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id))
        .build()
        .expect("Server failed to build");

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Test Studio").await.unwrap();
    let studio_id = studio.id;

    let task_spec = make_task_spec(
        "task-unavail",
        "Unhealthy Task",
        repo.path(),
        Some("runtime-offline".to_string()),
        Some(unavail_inst.to_string()),
    );

    let tasks = client.submit_task(studio_id, task_spec).await.unwrap();
    let task_id = tasks[0];

    let run = wait_for_run_terminal_state(&mut client, studio_id, task_id).await;
    assert_eq!(run.state, RunState::Failed);
    let task = client.get_task(studio_id, task_id).await.unwrap().unwrap();
    assert_eq!(task.state, TaskState::Failed);
}

#[tokio::test]
async fn test_09_deterministic_routing_ambiguity_rejection() {
    let key_id = "test-routing-key-7";
    let secret = b"secret-m15-02-test-vwx".to_vec();
    let repo = IsolatedTestRepo::new();
    let (catalog, prov1_id, model1_id, _, _) = create_test_catalog();

    let inst_a = RuntimeInstanceId::generate();
    let inst_b = RuntimeInstanceId::generate();
    let rt_a = Arc::new(MultiRuntimeMock::new("runtime-alpha", vec![inst_a]));
    let rt_b = Arc::new(MultiRuntimeMock::new("runtime-beta", vec![inst_b]));

    // Multi-runtime with NO default instance configured
    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtimes(vec![
            rt_a.clone() as Arc<dyn AgentRuntime>,
            rt_b.clone() as Arc<dyn AgentRuntime>,
        ])
        .with_auto_execute(true)
        .with_workspace_orchestrator(repo.create_orchestrator())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .with_provider_catalog(catalog)
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id))
        .build()
        .expect("Server failed to build");

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Test Studio").await.unwrap();
    let studio_id = studio.id;

    // Task with NO preference and NO instance
    let task_spec = make_task_spec(
        "task-ambiguous",
        "Ambiguous Routing Task",
        repo.path(),
        None,
        None,
    );

    let tasks = client.submit_task(studio_id, task_spec).await.unwrap();
    let task_id = tasks[0];

    let run = wait_for_run_terminal_state(&mut client, studio_id, task_id).await;
    assert_eq!(run.state, RunState::Failed);
    let task = client.get_task(studio_id, task_id).await.unwrap().unwrap();
    assert_eq!(task.state, TaskState::Failed);
}

#[tokio::test]
async fn test_10_concurrent_execution_across_independent_runtimes() {
    let key_id = "test-routing-key-8";
    let secret = b"secret-m15-02-test-yz0".to_vec();
    let repo = IsolatedTestRepo::new();
    let (catalog, prov1_id, model1_id, _, _) = create_test_catalog();

    let inst_a = RuntimeInstanceId::generate();
    let inst_b = RuntimeInstanceId::generate();
    let rt_a = Arc::new(MultiRuntimeMock::new("runtime-alpha", vec![inst_a]));
    let rt_b = Arc::new(MultiRuntimeMock::new("runtime-beta", vec![inst_b]));

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtimes(vec![
            rt_a.clone() as Arc<dyn AgentRuntime>,
            rt_b.clone() as Arc<dyn AgentRuntime>,
        ])
        .with_auto_execute(true)
        .with_workspace_orchestrator(repo.create_orchestrator())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .with_provider_catalog(catalog)
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id))
        .build()
        .expect("Server failed to build");

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Test Studio").await.unwrap();
    let studio_id = studio.id;

    // Dispatch Task A to runtime-alpha
    let task_spec_a = make_task_spec(
        "task-concurrent-a",
        "Concurrent Task A",
        repo.path(),
        Some("runtime-alpha".to_string()),
        Some(inst_a.to_string()),
    );
    let _ = client.submit_task(studio_id, task_spec_a).await.unwrap();

    // Dispatch Task B to runtime-beta
    let task_spec_b = make_task_spec(
        "task-concurrent-b",
        "Concurrent Task B",
        repo.path(),
        Some("runtime-beta".to_string()),
        Some(inst_b.to_string()),
    );
    let _ = client.submit_task(studio_id, task_spec_b).await.unwrap();

    // Both runtimes should independently start without blocking each other
    let reqs_a = rt_a.wait_for_started_requests(1).await;
    let reqs_b = rt_b.wait_for_started_requests(1).await;

    assert_eq!(reqs_a.len(), 1);
    assert_eq!(reqs_b.len(), 1);
    assert_eq!(*reqs_a[0].instance_id(), inst_a);
    assert_eq!(*reqs_b[0].instance_id(), inst_b);
}

#[tokio::test]
async fn test_11_lifecycle_session_ownership_cancellation_dispatch() {
    let key_id = "test-routing-key-9";
    let secret = b"secret-m15-02-test-123".to_vec();
    let repo = IsolatedTestRepo::new();
    let (catalog, prov1_id, model1_id, _, _) = create_test_catalog();

    let inst_a = RuntimeInstanceId::generate();
    let inst_b = RuntimeInstanceId::generate();
    let rt_a = Arc::new(MultiRuntimeMock::new("runtime-alpha", vec![inst_a]));
    let rt_b = Arc::new(MultiRuntimeMock::new("runtime-beta", vec![inst_b]));

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtimes(vec![
            rt_a.clone() as Arc<dyn AgentRuntime>,
            rt_b.clone() as Arc<dyn AgentRuntime>,
        ])
        .with_auto_execute(true)
        .with_workspace_orchestrator(repo.create_orchestrator())
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .with_provider_catalog(catalog)
        .with_default_model_ref(ModelRef::new(prov1_id, model1_id))
        .build()
        .expect("Server failed to build");

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret.clone()))
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Test Studio").await.unwrap();
    let studio_id = studio.id;

    // Start Task on runtime-beta
    let task_spec = make_task_spec(
        "task-cancellable",
        "Cancellable Beta Task",
        repo.path(),
        Some("runtime-beta".to_string()),
        Some(inst_b.to_string()),
    );
    let tasks = client.submit_task(studio_id, task_spec).await.unwrap();
    let task_id = tasks[0];
    let _ = rt_b.wait_for_started_requests(1).await;

    // Cancel the task
    let cancel_res = client
        .cancel_task(studio_id, task_id, "User requested cancellation")
        .await;
    assert!(cancel_res.is_ok(), "Expected task cancellation to succeed");

    // Verify rt_b's stop method was called
    let stops_b = rt_b.wait_for_stopped_sessions(1).await;
    assert_eq!(stops_b.len(), 1);
    assert_eq!(stops_b[0].instance_id, inst_b);

    // Verify rt_a received ZERO stop calls
    assert_eq!(rt_a.stopped_sessions().len(), 0);
}

#[tokio::test]
async fn test_12_duplicate_instance_identity_fails_closed_at_registry_boundary() {
    let shared_id = RuntimeInstanceId::generate();
    let alpha = Arc::new(MultiRuntimeMock::new("runtime-alpha", vec![shared_id]));
    let beta = Arc::new(MultiRuntimeMock::new("runtime-beta", vec![shared_id]));
    let registry = RuntimeRegistry::from_runtimes(vec![
        alpha as Arc<dyn AgentRuntime>,
        beta as Arc<dyn AgentRuntime>,
    ])
    .expect("registration with distinct implementation IDs");

    let outcome = registry.discover_all().await;
    assert!(outcome.instances.is_empty(), "Ambiguous identity must not resolve");
    assert!(outcome.failures.contains_key(
        &RuntimeImplementationId::new("runtime-alpha").unwrap()
    ));
    assert!(outcome.failures.contains_key(
        &RuntimeImplementationId::new("runtime-beta").unwrap()
    ));
}

#[tokio::test]
async fn test_13_foreign_implementation_identity_rejected_at_discovery() {
    let instance_id = RuntimeInstanceId::generate();
    let mut spoof = MultiRuntimeMock::new("runtime-spoof", vec![instance_id]);
    spoof.instances[0].implementation_id =
        RuntimeImplementationId::new("runtime-trusted").unwrap();
    let registry =
        RuntimeRegistry::from_runtimes(vec![Arc::new(spoof) as Arc<dyn AgentRuntime>])
            .expect("register spoofed implementation");

    let outcome = registry.discover_all().await;
    assert!(outcome.instances.is_empty(), "Foreign identity must be rejected");
    assert!(outcome.failures.contains_key(
        &RuntimeImplementationId::new("runtime-spoof").unwrap()
    ));
}

#[tokio::test]
async fn test_14_readiness_only_advertises_healthy_implementations() {
    let secret = b"secret-m15-02-healthy-filter".to_vec();
    let key_id = "healthy-filter-client";
    let repo = IsolatedTestRepo::new();
    let (catalog, provider_id, model_id, _, _) = create_test_catalog();
    let alpha_id_1 = RuntimeInstanceId::generate();
    let alpha_id_2 = RuntimeInstanceId::generate();
    let beta_id = RuntimeInstanceId::generate();
    let alpha = Arc::new(MultiRuntimeMock::new(
        "runtime-alpha",
        vec![alpha_id_1, alpha_id_2],
    ));
    let beta = Arc::new(MultiRuntimeMock::with_unavailable_instance(
        "runtime-beta",
        beta_id,
    ));

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtimes(vec![
            alpha as Arc<dyn AgentRuntime>,
            beta as Arc<dyn AgentRuntime>,
        ])
        .with_provider_catalog(catalog)
        .with_default_model_ref(ModelRef::new(provider_id, model_id))
        .with_default_repo_path(repo.path())
        .with_authorized_workspace(repo.path())
        .build()
        .expect("build multi-runtime server");
    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);
    let mut client = IpcClient::connect_stream(
        client_stream,
        ClientConfig::new(key_id, secret),
    )
    .await
    .expect("connect IPC client");

    let readiness = client.get_execution_readiness().await.unwrap();
    assert_eq!(readiness.available_runtimes, vec!["runtime-alpha"]);
    assert_eq!(readiness.runtime_instances.len(), 3);
    assert!(readiness.runtime_instances.iter().any(|instance| {
        instance.instance_id == beta_id.to_string() && !instance.healthy
    }));
    assert!(
        !readiness.is_ready,
        "Two healthy instances from one implementation still need a default"
    );
    assert!(readiness.blocking_reasons.iter().any(|reason| {
        reason.contains("Multiple healthy runtimes available")
    }));
}
