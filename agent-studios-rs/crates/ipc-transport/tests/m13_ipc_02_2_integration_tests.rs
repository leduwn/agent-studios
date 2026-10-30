use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::timeout;

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

use agent_studios_ipc_protocol::{ApprovalDecision, ApprovalStatus};
use agent_studios_ipc_transport::{
    ClientConfig, DaemonAdminBootstrapInfo, DaemonBootstrapInfo, IpcClient, IpcServer,
};
use agent_studios_protocol::agent::AgentKind;
use agent_studios_protocol::approval::{ApprovalKind, ApprovalState};
use agent_studios_protocol::task::{BatchTaskSpec, DependencyOutputPolicy, TaskState};
use agent_studios_provider::{
    AuthenticationScheme, EndpointProfile, ModelCapabilities, ModelDescriptor, ModelId,
    ModelLimits, ModelRef, ProtocolFamily, ProviderCatalog, ProviderId, ProviderInstance,
    ProviderInstanceId,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct TestMockRuntime {
    impl_id: RuntimeImplementationId,
    instance_id: RuntimeInstanceId,
    hubs: Arc<Mutex<HashMap<RuntimeSessionId, Arc<SessionEventHub>>>>,
    received_inputs: Arc<Mutex<Vec<(RuntimeSessionRef, RuntimeInput)>>>,
    stopped_sessions: Arc<Mutex<Vec<RuntimeSessionRef>>>,
    emit_approval_on_start: bool,
    runtime_approval_id: agent_studios_protocol::id::ApprovalId,
    fail_stop: Arc<AtomicBool>,
    fail_send: Arc<AtomicBool>,
}

impl TestMockRuntime {
    fn new(emit_approval_on_start: bool) -> Self {
        let impl_id = RuntimeImplementationId::new("mock-test-runtime").unwrap();
        let instance_id = RuntimeInstanceId::generate();
        Self {
            impl_id,
            instance_id,
            hubs: Arc::new(Mutex::new(HashMap::new())),
            received_inputs: Arc::new(Mutex::new(Vec::new())),
            stopped_sessions: Arc::new(Mutex::new(Vec::new())),
            emit_approval_on_start,
            runtime_approval_id: agent_studios_protocol::id::ApprovalId::new(),
            fail_stop: Arc::new(AtomicBool::new(false)),
            fail_send: Arc::new(AtomicBool::new(false)),
        }
    }

    fn set_fail_stop(&self, fail: bool) {
        self.fail_stop.store(fail, Ordering::SeqCst);
    }

    fn set_fail_send(&self, fail: bool) {
        self.fail_send.store(fail, Ordering::SeqCst);
    }

    async fn emit_session_event(&self, session_id: RuntimeSessionId, event: RuntimeEventKind) {
        let hub_opt = self.hubs.lock().unwrap().get(&session_id).cloned();
        if let Some(hub) = hub_opt {
            let _ = hub.emit(event).await;
        }
    }

    fn received_inputs(&self) -> Vec<(RuntimeSessionRef, RuntimeInput)> {
        self.received_inputs.lock().unwrap().clone()
    }

    fn stopped_sessions(&self) -> Vec<RuntimeSessionRef> {
        self.stopped_sessions.lock().unwrap().clone()
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
            .with_stop(true)
            .with_approvals(true);
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
            .with_approvals(true)
    }

    async fn start(
        &self,
        request: RuntimeStartRequest,
    ) -> Result<RuntimeSessionHandle, RuntimeError> {
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
        session: &RuntimeSessionRef,
        input: RuntimeInput,
    ) -> Result<(), RuntimeError> {
        if self.fail_send.load(Ordering::SeqCst) {
            return Err(RuntimeError::Internal {
                reason: SanitizedRuntimeMessage::new("Simulated runtime delivery network failure"),
            });
        }

        self.received_inputs
            .lock()
            .unwrap()
            .push((session.clone(), input.clone()));

        // If approval response received, emit completed to finish session cleanly
        if let RuntimeInput::ApprovalResponse { .. } = &input {
            let hub_opt = self.hubs.lock().unwrap().get(&session.session_id).cloned();
            if let Some(hub) = hub_opt {
                let _ = hub
                    .emit(RuntimeEventKind::Completed {
                        summary: Some("Completed after approval response".to_string()),
                    })
                    .await;
            }
        }
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
        if self.fail_stop.load(Ordering::SeqCst) {
            return Err(RuntimeError::Internal {
                reason: SanitizedRuntimeMessage::new("Simulated runtime stop failure"),
            });
        }

        self.stopped_sessions.lock().unwrap().push(session.clone());
        let hub_opt = self.hubs.lock().unwrap().get(&session.session_id).cloned();
        if let Some(hub) = hub_opt {
            let _ = hub.emit(RuntimeEventKind::Stopped).await;
        }
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

            if self.emit_approval_on_start {
                let _ = new_hub
                    .emit(RuntimeEventKind::ApprovalRequested {
                        approval_id: self.runtime_approval_id,
                        description: "Format disk for clean build".into(),
                        command: Some("mkfs.ext4 /dev/sda1".into()),
                    })
                    .await;
            }
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
    _head_commit: String,
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

        let rev_out = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo_path)
            .output()
            .expect("git rev-parse HEAD");

        let head_commit = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();

        Self {
            _dir: dir,
            path: repo_path,
            _head_commit: head_commit,
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

#[tokio::test]
async fn test_bootstrap_credential_segregation_and_untracked_approval_role_gate() {
    let temp_dir = tempfile::tempdir().unwrap();
    let client_path = temp_dir.path().join("agsd.json");
    let admin_path = temp_dir.path().join("agsd-admin.json");

    let client_key_id = "test-client-key-id";
    let client_key = "client-secret-999".to_string();
    let admin_key_id = "test-admin-key-id";
    let admin_key = "admin-secret-888".to_string();

    let client_boot = DaemonBootstrapInfo {
        endpoint: "test-endpoint".to_string(),
        key_id: client_key_id.to_string(),
        client_key: client_key.clone(),
        admin_key: None,
        pid: 1001,
        created_at: 1700000000,
    };
    client_boot.save_to_file(&client_path).unwrap();

    let admin_boot = DaemonAdminBootstrapInfo {
        endpoint: "test-endpoint".to_string(),
        key_id: admin_key_id.to_string(),
        admin_key: admin_key.clone(),
        pid: 1001,
        created_at: 1700000000,
    };
    admin_boot.save_to_file(&admin_path).unwrap();

    // Verify raw JSON contents
    let client_json = std::fs::read_to_string(&client_path).unwrap();
    assert!(client_json.contains("client_key"));
    assert!(!client_json.contains("admin_key"));

    let admin_json = std::fs::read_to_string(&admin_path).unwrap();
    assert!(admin_json.contains("admin_key"));
    assert!(!admin_json.contains("client_key"));

    // Verify redaction in Debug
    assert!(format!("{:?}", client_boot).contains("[REDACTED]"));
    assert!(format!("{:?}", admin_boot).contains("[REDACTED]"));

    // Build server with both roles
    let server = IpcServer::builder()
        .with_auth_key(client_key_id, client_key.as_bytes().to_vec())
        .with_admin_auth_key(admin_key_id, admin_key.as_bytes().to_vec())
        .build()
        .expect("Server failed to build");

    let cp_lock = server.control_plane();

    // Create a studio and a task with an approval in the Control Plane directly (untracked origin)
    let approval_id = {
        let mut cp = cp_lock.write().await;
        let studio = cp.create_studio("Approval Role Gate Studio").unwrap();
        let agent = cp
            .register_agent(
                studio.id,
                "Gate Agent",
                AgentKind::External,
                Some("Specialist gate agent".to_string()),
            )
            .unwrap();
        let task = cp
            .create_task(
                studio.id,
                "Privileged Task",
                "Execute command",
                None,
                Some(agent.id),
                Vec::new(),
            )
            .unwrap();
        let app = cp
            .request_approval(
                studio.id,
                task.id,
                agent.id,
                ApprovalKind::CommandExecution,
                "Execute dangerous operation",
            )
            .unwrap();
        app.id
    };

    // Connect standard client
    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(client_key_id, client_key.as_bytes().to_vec());
    let mut standard_client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Standard client handshake failed");

    // Standard client attempting to resolve untracked/foreign approval must fail closed with permission error
    let std_res = standard_client
        .resolve_approval(
            approval_id.to_string(),
            ApprovalDecision::Approved {
                modified_details: None,
                user_note: Some("Standard approval attempt".into()),
            },
        )
        .await;

    assert!(std_res.is_err());
    let err_msg = format!("{:?}", std_res);
    assert!(
        err_msg.contains("Permission denied") || err_msg.contains("foreign approval resolution"),
        "Expected permission denied message, got: {err_msg}"
    );

    // Connect admin client
    let (admin_client_stream, admin_server_stream) = tokio::io::duplex(65536);
    server.handle_connection(admin_server_stream);

    let admin_config = ClientConfig::new(admin_key_id, admin_key.as_bytes().to_vec());
    let mut admin_client = IpcClient::connect_stream(admin_client_stream, admin_config)
        .await
        .expect("Admin client handshake failed");

    // Admin client is authorized to resolve untracked/foreign approval
    let admin_res = admin_client
        .resolve_approval(
            approval_id.to_string(),
            ApprovalDecision::Approved {
                modified_details: None,
                user_note: Some("Admin operator authorization granted".into()),
            },
        )
        .await
        .expect("Admin approval resolution failed");

    assert_eq!(admin_res, ApprovalStatus::Authorized);

    // Verify Control Plane state is Approved
    {
        let cp = cp_lock.read().await;
        let app = cp.get_approval(approval_id).unwrap();
        assert_eq!(app.state, ApprovalState::Approved);
    }
}

#[tokio::test]
async fn test_runtime_approval_delivery_and_cancellation() {
    let mock_runtime = Arc::new(TestMockRuntime::new(true));
    let expected_runtime_approval_id = mock_runtime.runtime_approval_id;

    let key_id = "test-runtime-key";
    let secret = b"secret-runtime-key-123".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    // Connect subscriber client (dedicated to event subscription)
    let (sub_stream, sub_server_stream) = tokio::io::duplex(65536);
    server.handle_connection(sub_server_stream);
    let sub_config = ClientConfig::new(key_id, secret.clone());
    let mut sub_client = IpcClient::connect_stream(sub_stream, sub_config)
        .await
        .expect("Sub client handshake failed");
    let mut event_sub = sub_client
        .subscribe_events(1)
        .await
        .expect("Subscribe failed");

    // Connect command client
    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);
    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Runtime Test Studio").await.unwrap();

    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(
                studio.id,
                "Test Agent",
                AgentKind::External,
                Some("Testing agent".to_string()),
            )
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-runtime-1".to_string(),
        title: "Test Runtime Task".to_string(),
        description: "Task that triggers approval request".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client.submit_task(studio.id, spec).await.unwrap();
    assert_eq!(task_ids.len(), 1);

    // Await approval_requested event from IPC stream
    let mut detected_approval_id: Option<agent_studios_protocol::id::ApprovalId> = None;
    for _ in 0..100 {
        if let Ok(Ok(ev)) = timeout(TEST_TIMEOUT, event_sub.next_event()).await
            && ev.event_type == "approval_requested"
            && let Some(aid) = ev.payload.get("approval_id").and_then(|v| v.as_str())
            && let Ok(id) = aid.parse::<agent_studios_protocol::id::ApprovalId>()
        {
            detected_approval_id = Some(id);
            break;
        }
    }

    let cp_approval_id =
        detected_approval_id.expect("Timed out waiting for approval_requested event");

    // Resolve approval from client
    let status = client
        .resolve_approval(
            cp_approval_id.to_string(),
            ApprovalDecision::Approved {
                modified_details: None,
                user_note: Some("Operator confirms disk format".into()),
            },
        )
        .await
        .expect("Resolve approval failed");

    assert_eq!(status, ApprovalStatus::Authorized);

    // Verify runtime received RuntimeInput::ApprovalResponse with the correct runtime_approval_id
    let mut found_response = false;
    for _ in 0..50 {
        let inputs = mock_runtime.received_inputs();
        for (_, input) in &inputs {
            if let RuntimeInput::ApprovalResponse {
                approval_id,
                approved,
                reason,
            } = input
                && *approval_id == expected_runtime_approval_id
            {
                assert!(*approved);
                assert_eq!(*reason, Some("Operator confirms disk format".to_string()));
                found_response = true;
                break;
            }
        }
        if found_response {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        found_response,
        "Expected RuntimeInput::ApprovalResponse to be delivered to runtime"
    );
}

#[tokio::test]
async fn test_runtime_cancellation_stops_session() {
    let mock_runtime = Arc::new(TestMockRuntime::new(false));

    let key_id = "test-cancel-key";
    let secret = b"secret-cancel-key-123".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Cancellation Studio").await.unwrap();

    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(
                studio.id,
                "Cancel Agent",
                AgentKind::External,
                Some("Cancellation test agent".to_string()),
            )
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-cancel-1".to_string(),
        title: "Long Running Task".to_string(),
        description: "Task to be cancelled".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client.submit_task(studio.id, spec).await.unwrap();
    let task_id = task_ids[0];

    // Wait until runtime session is registered in active_runtime_sessions
    let mut session_active = false;
    let state = server.state();
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let sessions = state.active_runtime_sessions.read().await;
        if sessions.contains_key(&task_id) {
            session_active = true;
            break;
        }
        let cp = cp_lock.read().await;
        if let Some(t) = cp.get_task(task_id)
            && t.state.is_terminal()
        {
            panic!("Task entered terminal state prematurely: {:?}", t.state);
        }
    }
    assert!(
        session_active,
        "Runtime session should be registered in active_runtime_sessions"
    );

    // Cancel task from client
    client
        .cancel_task(studio.id, task_id, "User requested cancellation")
        .await
        .expect("Cancel task failed");

    // Verify mock runtime was stopped
    let stopped = mock_runtime.stopped_sessions();
    assert!(
        !stopped.is_empty(),
        "Runtime session should have been stopped via runtime.stop()"
    );

    // Verify task state in Control Plane is Cancelled
    {
        let cp = cp_lock.read().await;
        let task = cp.get_task(task_id).unwrap();
        assert_eq!(task.state, TaskState::Cancelled);
    }
}

#[tokio::test]
async fn test_worktree_orchestrator_fail_closed_on_invalid_repo() {
    let temp_dir = tempfile::tempdir().unwrap();
    let wt_root = temp_dir.path().join("worktrees");
    let art_root = temp_dir.path().join("artifacts");

    let orchestrator = agent_studios_workspace::WorkspaceOrchestrator::new(wt_root, art_root)
        .expect("Failed to create orchestrator");

    // Test that creating worktree with an invalid, non-git source_cwd fails closed
    let non_git_dir = temp_dir.path().join("non-existent-repo-path");
    let res = orchestrator.create_worktree_async(non_git_dir, None).await;

    assert!(
        res.is_err(),
        "Expected worktree creation to fail on non-git directory"
    );
}

#[tokio::test]
async fn test_cancel_task_with_wrong_studio_id_fails_closed_without_stopping_runtime() {
    let mock_runtime = Arc::new(TestMockRuntime::new(false));
    let key_id = "test-cancel-wrong-studio";
    let secret = b"secret-cancel-wrong-studio-123".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    let studio1 = client.create_studio("Studio 1").await.unwrap();
    let studio2 = client.create_studio("Studio 2").await.unwrap();

    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(
                studio1.id,
                "Agent 1",
                AgentKind::External,
                Some("Test agent".to_string()),
            )
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-wrong-studio".to_string(),
        title: "Task in Studio 1".to_string(),
        description: "Task for wrong studio cancellation test".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client.submit_task(studio1.id, spec).await.unwrap();
    let task_id = task_ids[0];

    // Wait until runtime session is registered in active_runtime_sessions
    let state = server.state();
    let mut session_active = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let sessions = state.active_runtime_sessions.read().await;
        if sessions.contains_key(&task_id) {
            session_active = true;
            break;
        }
    }
    assert!(session_active, "Session must be active");

    // Attempt cancellation using studio2 (mismatched studio ID)
    let cancel_res = client
        .cancel_task(studio2.id, task_id, "Attempt cancel from wrong studio")
        .await;

    assert!(
        cancel_res.is_err(),
        "Cancellation with wrong studio ID must fail"
    );
    let err_str = format!("{:?}", cancel_res);
    assert!(
        err_str.contains("Studio mismatch"),
        "Expected Studio mismatch error, got: {err_str}"
    );

    // Verify runtime was NOT stopped
    let stopped = mock_runtime.stopped_sessions();
    assert!(
        stopped.is_empty(),
        "Runtime session must NOT be stopped when studio ID is mismatched"
    );

    // Verify task in Control Plane is still non-terminal
    {
        let cp = cp_lock.read().await;
        let task = cp.get_task(task_id).unwrap();
        assert!(
            !task.state.is_terminal(),
            "Task must not enter terminal state on rejected cancellation"
        );
    }
}

#[tokio::test]
async fn test_cross_session_cancellation_rejected_for_standard_clients() {
    let mock_runtime = Arc::new(TestMockRuntime::new(false));
    let key1 = "client-user-1";
    let secret1 = b"secret-client-user-1-abc".to_vec();
    let key2 = "client-user-2";
    let secret2 = b"secret-client-user-2-xyz".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    let server = IpcServer::builder()
        .with_auth_key(key1, secret1.clone())
        .with_auth_key(key2, secret2.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    // Connect Client 1 (owner)
    let (c1_stream, s1_stream) = tokio::io::duplex(65536);
    server.handle_connection(s1_stream);
    let mut client1 = IpcClient::connect_stream(c1_stream, ClientConfig::new(key1, secret1))
        .await
        .expect("Client 1 handshake failed");

    // Connect Client 2 (foreign)
    let (c2_stream, s2_stream) = tokio::io::duplex(65536);
    server.handle_connection(s2_stream);
    let mut client2 = IpcClient::connect_stream(c2_stream, ClientConfig::new(key2, secret2))
        .await
        .expect("Client 2 handshake failed");

    let studio = client1.create_studio("Shared Studio").await.unwrap();

    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio.id, "Cross Session Agent", AgentKind::External, None)
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-cross-session".to_string(),
        title: "Cross Session Task".to_string(),
        description: "Task submitted by Client 1".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client1.submit_task(studio.id, spec).await.unwrap();
    let task_id = task_ids[0];

    // Wait until runtime session is registered in active_runtime_sessions
    let state = server.state();
    let mut session_active = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let sessions = state.active_runtime_sessions.read().await;
        if sessions.contains_key(&task_id) {
            session_active = true;
            break;
        }
    }
    assert!(session_active, "Session must be active");

    // Client 2 (standard role) attempts to cancel Client 1's task
    let cancel_res = client2
        .cancel_task(studio.id, task_id, "Malicious cancel attempt")
        .await;

    assert!(
        cancel_res.is_err(),
        "Cross-session cancellation must be rejected for standard clients"
    );
    let err_str = format!("{:?}", cancel_res);
    assert!(
        err_str.contains("cross-session cancellation requires admin role"),
        "Expected admin role requirement error, got: {err_str}"
    );

    // Verify runtime was NOT stopped
    let stopped = mock_runtime.stopped_sessions();
    assert!(
        stopped.is_empty(),
        "Runtime session must NOT be stopped when unauthorized client attempts cancellation"
    );

    // Verify task state in Control Plane is not cancelled
    {
        let cp = cp_lock.read().await;
        let task = cp.get_task(task_id).unwrap();
        assert_ne!(task.state, TaskState::Cancelled);
    }
}

#[tokio::test]
async fn test_runtime_stop_failure_fails_closed() {
    let mock_runtime = Arc::new(TestMockRuntime::new(false));
    let key_id = "test-fail-stop-key";
    let secret = b"secret-fail-stop-key-123".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Stop Failure Studio").await.unwrap();

    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio.id, "Stop Fail Agent", AgentKind::External, None)
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-stop-fail".to_string(),
        title: "Task with stop failure".to_string(),
        description: "Task testing runtime.stop() failure".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client.submit_task(studio.id, spec).await.unwrap();
    let task_id = task_ids[0];

    // Wait until session is active
    let state = server.state();
    let mut session_active = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let sessions = state.active_runtime_sessions.read().await;
        if sessions.contains_key(&task_id) {
            session_active = true;
            break;
        }
    }
    assert!(session_active, "Session must be active");

    // Configure mock runtime to fail on stop()
    mock_runtime.set_fail_stop(true);

    // Cancel task from client
    let cancel_res = client
        .cancel_task(studio.id, task_id, "User requested cancellation")
        .await;

    assert!(
        cancel_res.is_err(),
        "Cancellation must fail closed when runtime.stop() fails"
    );
    let err_str = format!("{:?}", cancel_res);
    assert!(
        err_str.contains("Failed to stop runtime session"),
        "Expected stop failure message, got: {err_str}"
    );

    // Verify task state in Control Plane was NOT transitioned to Cancelled
    {
        let cp = cp_lock.read().await;
        let task = cp.get_task(task_id).unwrap();
        assert_ne!(
            task.state,
            TaskState::Cancelled,
            "Task must fail closed and NOT transition to Cancelled when runtime.stop() fails"
        );
    }
}

#[tokio::test]
async fn test_late_runtime_completed_event_ignored_after_cancellation() {
    let mock_runtime = Arc::new(TestMockRuntime::new(false));
    let key_id = "test-late-event-key";
    let secret = b"secret-late-event-key-123".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Late Event Studio").await.unwrap();

    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio.id, "Late Event Agent", AgentKind::External, None)
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-late-event".to_string(),
        title: "Task with late runtime event".to_string(),
        description: "Task testing late runtime event after cancellation".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client.submit_task(studio.id, spec).await.unwrap();
    let task_id = task_ids[0];

    // Wait until session is active and extract session_id
    let state = server.state();
    let mut session_opt = None;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let sessions = state.active_runtime_sessions.read().await;
        if let Some((s_ref, _)) = sessions.get(&task_id) {
            session_opt = Some(s_ref.session_id);
            break;
        }
    }
    let runtime_session_id = session_opt.expect("Session must be active");

    // Cancel task cleanly
    client
        .cancel_task(studio.id, task_id, "User requested cancellation")
        .await
        .expect("Cancellation failed");

    // Verify task is Cancelled
    {
        let cp = cp_lock.read().await;
        let task = cp.get_task(task_id).unwrap();
        assert_eq!(task.state, TaskState::Cancelled);
    }

    // Emit late Completed event from runtime
    mock_runtime
        .emit_session_event(
            runtime_session_id,
            RuntimeEventKind::Completed {
                summary: Some("Late finish after cancellation".into()),
            },
        )
        .await;

    // Allow event processing loop to run
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify task state in Control Plane remains Cancelled, NOT Succeeded
    {
        let cp = cp_lock.read().await;
        let task = cp.get_task(task_id).unwrap();
        assert_eq!(
            task.state,
            TaskState::Cancelled,
            "Late runtime Completed event must not overwrite terminal Cancelled state"
        );
    }
}

#[tokio::test]
async fn test_approval_runtime_delivery_failure_fails_closed_without_modifying_control_plane() {
    let mock_runtime = Arc::new(TestMockRuntime::new(true));
    let key_id = "test-fail-approval-delivery";
    let secret = b"secret-fail-approval-del-123".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    // Connect subscriber client
    let (sub_stream, sub_server_stream) = tokio::io::duplex(65536);
    server.handle_connection(sub_server_stream);
    let mut sub_client =
        IpcClient::connect_stream(sub_stream, ClientConfig::new(key_id, secret.clone()))
            .await
            .expect("Sub client handshake failed");
    let mut event_sub = sub_client
        .subscribe_events(1)
        .await
        .expect("Subscribe failed");

    // Connect command client
    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);
    let mut client = IpcClient::connect_stream(client_stream, ClientConfig::new(key_id, secret))
        .await
        .expect("Client handshake failed");

    let studio = client
        .create_studio("Approval Fail Delivery Studio")
        .await
        .unwrap();

    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio.id, "Approval Agent", AgentKind::External, None)
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-approval-fail-del".to_string(),
        title: "Task with approval delivery failure".to_string(),
        description: "Task testing 2PC approval delivery failure".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client.submit_task(studio.id, spec).await.unwrap();
    assert_eq!(task_ids.len(), 1);

    // Await approval_requested event from IPC stream
    let mut detected_approval_id: Option<agent_studios_protocol::id::ApprovalId> = None;
    for _ in 0..100 {
        if let Ok(Ok(ev)) = timeout(TEST_TIMEOUT, event_sub.next_event()).await
            && ev.event_type == "approval_requested"
            && let Some(aid) = ev.payload.get("approval_id").and_then(|v| v.as_str())
            && let Ok(id) = aid.parse::<agent_studios_protocol::id::ApprovalId>()
        {
            detected_approval_id = Some(id);
            break;
        }
    }
    let cp_approval_id =
        detected_approval_id.expect("Timed out waiting for approval_requested event");

    // Configure mock runtime to fail on send() (simulating delivery network failure)
    mock_runtime.set_fail_send(true);

    // Attempt to resolve approval from client
    let resolve_res = client
        .resolve_approval(
            cp_approval_id.to_string(),
            ApprovalDecision::Approved {
                modified_details: None,
                user_note: Some("Operator confirms action".into()),
            },
        )
        .await;

    assert!(
        resolve_res.is_err(),
        "Resolution must fail closed when runtime delivery fails"
    );
    let err_str = format!("{:?}", resolve_res);
    assert!(
        err_str.contains("Failed to deliver approval decision to runtime"),
        "Expected delivery failure error, got: {err_str}"
    );

    // Verify Control Plane state is STILL Pending (not mutated to Approved)
    {
        let cp = cp_lock.read().await;
        let app = cp.get_approval(cp_approval_id).unwrap();
        assert_eq!(
            app.state,
            ApprovalState::Pending,
            "Approval state must remain Pending when runtime delivery fails"
        );
    }

    // Verify pending runtime approval mapping is preserved in server state for retry
    {
        let state = server.state();
        let pending = state.pending_runtime_approvals.read().await;
        assert!(
            pending.contains_key(&cp_approval_id),
            "Pending runtime approval mapping must be preserved for retry"
        );
    }

    // Restore delivery and retry resolution
    mock_runtime.set_fail_send(false);

    let retry_res = client
        .resolve_approval(
            cp_approval_id.to_string(),
            ApprovalDecision::Approved {
                modified_details: None,
                user_note: Some("Operator retried confirmation".into()),
            },
        )
        .await
        .expect("Retry resolution should succeed");

    assert_eq!(retry_res, ApprovalStatus::Authorized);

    // Verify Control Plane state is now Approved
    {
        let cp = cp_lock.read().await;
        let app = cp.get_approval(cp_approval_id).unwrap();
        assert_eq!(app.state, ApprovalState::Approved);
    }
}

#[tokio::test]
async fn test_cancel_task_missing_origin_fails_closed_for_standard_allowed_for_admin() {
    let mock_runtime = Arc::new(TestMockRuntime::new(false));
    let standard_key = "standard-client-origin";
    let standard_secret = b"standard-secret-origin-123".to_vec();
    let admin_key = "admin-client-origin";
    let admin_secret = b"admin-secret-origin-456".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    let server = IpcServer::builder()
        .with_auth_key(standard_key, standard_secret.clone())
        .with_admin_auth_key(admin_key, admin_secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (std_c_stream, std_s_stream) = tokio::io::duplex(65536);
    server.handle_connection(std_s_stream);
    let mut std_client = IpcClient::connect_stream(
        std_c_stream,
        ClientConfig::new(standard_key, standard_secret),
    )
    .await
    .expect("Standard client handshake failed");

    let (adm_c_stream, adm_s_stream) = tokio::io::duplex(65536);
    server.handle_connection(adm_s_stream);
    let mut adm_client =
        IpcClient::connect_stream(adm_c_stream, ClientConfig::new(admin_key, admin_secret))
            .await
            .expect("Admin client handshake failed");

    let studio = std_client.create_studio("Origin Studio").await.unwrap();
    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio.id, "Origin Agent", AgentKind::External, None)
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-missing-origin".to_string(),
        title: "Missing Origin Task".to_string(),
        description: "Task with stripped origin".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = std_client.submit_task(studio.id, spec).await.unwrap();
    let task_id = task_ids[0];

    // Wait until runtime session is registered in active_runtime_sessions
    let state = server.state();
    let mut session_active = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let sessions = state.active_runtime_sessions.read().await;
        if sessions.contains_key(&task_id) {
            session_active = true;
            break;
        }
    }
    assert!(session_active, "Session must be active");

    // Clear task origin from server state to simulate untracked/missing origin
    {
        let mut origins = state.task_origins.lock().unwrap();
        origins.remove(&task_id);
    }

    // Standard client attempt must FAIL CLOSED
    let cancel_res = std_client
        .cancel_task(
            studio.id,
            task_id,
            "Standard client cancelling untracked task",
        )
        .await;
    assert!(
        cancel_res.is_err(),
        "Untracked task cancellation must fail closed for standard client"
    );
    let err_str = format!("{:?}", cancel_res);
    assert!(
        err_str.contains("cross-session cancellation requires admin role"),
        "Expected admin requirement error, got: {err_str}"
    );

    // Runtime was not stopped by failed standard cancellation
    assert!(mock_runtime.stopped_sessions().is_empty());

    // Admin client attempt on untracked origin must SUCCEED
    let adm_cancel_res = adm_client
        .cancel_task(studio.id, task_id, "Admin emergency cancellation")
        .await;
    assert!(
        adm_cancel_res.is_ok(),
        "Untracked task cancellation must succeed for admin client"
    );

    // Verify task is cancelled and runtime stopped
    let stopped = mock_runtime.stopped_sessions();
    assert_eq!(
        stopped.len(),
        1,
        "Runtime session should be stopped by admin"
    );
    {
        let cp = cp_lock.read().await;
        let task = cp.get_task(task_id).unwrap();
        assert_eq!(task.state, TaskState::Cancelled);
    }
}

#[tokio::test]
async fn test_cancel_task_reconnected_owner_session_rebinding_succeeds() {
    let mock_runtime = Arc::new(TestMockRuntime::new(false));
    let owner_key = "reconnecting-owner-key";
    let owner_secret = b"reconnecting-secret-123".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    let server = IpcServer::builder()
        .with_auth_key(owner_key, owner_secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (c1_stream, s1_stream) = tokio::io::duplex(65536);
    server.handle_connection(s1_stream);
    let mut client1 = IpcClient::connect_stream(
        c1_stream,
        ClientConfig::new(owner_key, owner_secret.clone()),
    )
    .await
    .expect("Client 1 handshake failed");

    let studio = client1.create_studio("Rebind Studio").await.unwrap();
    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio.id, "Rebind Agent", AgentKind::External, None)
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-rebind".to_string(),
        title: "Rebinding Task".to_string(),
        description: "Task submitted by initial session".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client1.submit_task(studio.id, spec).await.unwrap();
    let task_id = task_ids[0];

    // Wait until runtime session is registered in active_runtime_sessions
    let state = server.state();
    let mut session_active = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let sessions = state.active_runtime_sessions.read().await;
        if sessions.contains_key(&task_id) {
            session_active = true;
            break;
        }
    }
    assert!(session_active, "Session must be active");

    let initial_session_id = {
        let origins = state.task_origins.lock().unwrap();
        let (s_id, k_id) = origins.get(&task_id).unwrap();
        assert_eq!(k_id, owner_key);
        s_id.clone()
    };

    // Client 1 disconnects / terminates
    drop(client1);

    // Reconnect as Client 2 with SAME key_id but new ephemeral connection/session
    let (c2_stream, s2_stream) = tokio::io::duplex(65536);
    server.handle_connection(s2_stream);
    let mut client2 =
        IpcClient::connect_stream(c2_stream, ClientConfig::new(owner_key, owner_secret))
            .await
            .expect("Client 2 handshake failed");

    // Client 2 cancels the task
    let cancel_res = client2
        .cancel_task(studio.id, task_id, "Reconnected owner cancellation")
        .await;

    assert!(
        cancel_res.is_ok(),
        "Cancellation from reconnected owner with same key_id must succeed"
    );

    // Verify origin session was rebound
    {
        let origins = state.task_origins.lock().unwrap();
        let (new_s_id, k_id) = origins.get(&task_id).unwrap();
        assert_eq!(k_id, owner_key);
        assert_ne!(
            new_s_id, &initial_session_id,
            "Session ID must be rebound to new connection"
        );
    }

    // Verify runtime was stopped
    let stopped = mock_runtime.stopped_sessions();
    assert_eq!(stopped.len(), 1, "Runtime session should be stopped");

    // Verify task state in Control Plane
    {
        let cp = cp_lock.read().await;
        let task = cp.get_task(task_id).unwrap();
        assert_eq!(task.state, TaskState::Cancelled);
    }
}

#[tokio::test]
async fn test_cancel_task_duplicate_on_terminal_state_rejected() {
    let mock_runtime = Arc::new(TestMockRuntime::new(false));
    let key_id = "test-dup-cancel";
    let secret = b"secret-dup-cancel-123".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret))
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Dup Cancel Studio").await.unwrap();
    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio.id, "Dup Agent", AgentKind::External, None)
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-dup".to_string(),
        title: "Duplicate Cancel Task".to_string(),
        description: "Task to test duplicate cancellation".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client.submit_task(studio.id, spec).await.unwrap();
    let task_id = task_ids[0];

    // Wait until runtime session is registered in active_runtime_sessions
    let state = server.state();
    let mut session_active = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let sessions = state.active_runtime_sessions.read().await;
        if sessions.contains_key(&task_id) {
            session_active = true;
            break;
        }
    }
    assert!(session_active, "Session must be active");

    // First cancellation succeeds
    let first_cancel = client
        .cancel_task(studio.id, task_id, "First cancellation")
        .await;
    assert!(first_cancel.is_ok(), "First cancellation should succeed");

    assert_eq!(mock_runtime.stopped_sessions().len(), 1);

    // Second cancellation on terminal state must be rejected
    let second_cancel = client
        .cancel_task(studio.id, task_id, "Second cancellation")
        .await;
    assert!(
        second_cancel.is_err(),
        "Duplicate cancellation on terminal state must fail"
    );
    let err_str = format!("{:?}", second_cancel);
    assert!(
        err_str.contains("already in terminal state Cancelled")
            || err_str.contains("Cannot cancel task in terminal state"),
        "Expected terminal state error, got: {err_str}"
    );

    // Stop session not called again
    assert_eq!(mock_runtime.stopped_sessions().len(), 1);
}

#[tokio::test]
async fn test_concurrent_cancellation_serialized_and_deduplicated() {
    let mock_runtime = Arc::new(TestMockRuntime::new(false));
    let key_id = "test-concurrent-cancel";
    let secret = b"secret-concurrent-cancel-123".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (c1_stream, s1_stream) = tokio::io::duplex(65536);
    server.handle_connection(s1_stream);
    let mut client1 =
        IpcClient::connect_stream(c1_stream, ClientConfig::new(key_id, secret.clone()))
            .await
            .expect("Client 1 handshake failed");

    let (c2_stream, s2_stream) = tokio::io::duplex(65536);
    server.handle_connection(s2_stream);
    let mut client2 = IpcClient::connect_stream(c2_stream, ClientConfig::new(key_id, secret))
        .await
        .expect("Client 2 handshake failed");

    let studio = client1
        .create_studio("Concurrent Cancel Studio")
        .await
        .unwrap();
    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio.id, "Concurrent Agent", AgentKind::External, None)
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-concurrent".to_string(),
        title: "Concurrent Task".to_string(),
        description: "Task for concurrent cancellation".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client1.submit_task(studio.id, spec).await.unwrap();
    let task_id = task_ids[0];

    // Wait until runtime session is registered in active_runtime_sessions
    let state = server.state();
    let mut session_active = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let sessions = state.active_runtime_sessions.read().await;
        if sessions.contains_key(&task_id) {
            session_active = true;
            break;
        }
    }
    assert!(session_active, "Session must be active");

    let studio_id = studio.id;
    let h1 = tokio::spawn(async move {
        client1
            .cancel_task(studio_id, task_id, "Concurrent cancel 1")
            .await
    });
    let h2 = tokio::spawn(async move {
        client2
            .cancel_task(studio_id, task_id, "Concurrent cancel 2")
            .await
    });

    let (res1, res2) = tokio::join!(h1, h2);
    let r1 = res1.unwrap();
    let r2 = res2.unwrap();

    let success_count = (r1.is_ok() as usize) + (r2.is_ok() as usize);
    assert_eq!(
        success_count, 1,
        "Exactly one concurrent cancellation must succeed; r1: {:?}, r2: {:?}",
        r1, r2
    );

    let err_str = if r1.is_err() {
        format!("{:?}", r1)
    } else {
        format!("{:?}", r2)
    };
    assert!(
        err_str.contains("Cancellation already in progress")
            || err_str.contains("already in terminal state")
            || err_str.contains("Cannot cancel task in terminal state"),
        "Losing concurrent cancel must fail closed with clean serialization error: {err_str}"
    );

    // Verify task is terminal Cancelled and runtime was stopped
    assert_eq!(mock_runtime.stopped_sessions().len(), 1);
    {
        let cp = cp_lock.read().await;
        let task = cp.get_task(task_id).unwrap();
        assert_eq!(task.state, TaskState::Cancelled);
    }
}

#[tokio::test]
async fn test_concurrent_approval_resolution_serialized_and_deduplicated() {
    let mock_runtime = Arc::new(TestMockRuntime::new(true)); // emit approval on start
    let key_id = "test-concurrent-approval";
    let secret = b"secret-concurrent-approval-123".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (c1_stream, s1_stream) = tokio::io::duplex(65536);
    server.handle_connection(s1_stream);
    let mut client1 =
        IpcClient::connect_stream(c1_stream, ClientConfig::new(key_id, secret.clone()))
            .await
            .expect("Client 1 handshake failed");

    let (c2_stream, s2_stream) = tokio::io::duplex(65536);
    server.handle_connection(s2_stream);
    let mut client2 = IpcClient::connect_stream(c2_stream, ClientConfig::new(key_id, secret))
        .await
        .expect("Client 2 handshake failed");

    let studio = client1
        .create_studio("Concurrent Approval Studio")
        .await
        .unwrap();
    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio.id, "Approval Agent", AgentKind::External, None)
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-approval-concurrent".to_string(),
        title: "Concurrent Approval Task".to_string(),
        description: "Task for concurrent approval".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client1.submit_task(studio.id, spec).await.unwrap();
    let _task_id = task_ids[0];

    // Wait until pending_runtime_approvals has the mapping
    let state = server.state();
    let mut cp_approval_id_opt = None;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let pending = state.pending_runtime_approvals.read().await;
        if let Some((&aid, _)) = pending.iter().next() {
            cp_approval_id_opt = Some(aid);
            break;
        }
    }
    let cp_approval_id =
        cp_approval_id_opt.expect("Approval should be registered in pending runtime approvals");

    let app_id_str1 = cp_approval_id.to_string();
    let app_id_str2 = cp_approval_id.to_string();

    let h1 = tokio::spawn(async move {
        client1
            .resolve_approval(
                app_id_str1,
                ApprovalDecision::Approved {
                    modified_details: None,
                    user_note: Some("Concurrent approval 1".into()),
                },
            )
            .await
    });
    let h2 = tokio::spawn(async move {
        client2
            .resolve_approval(
                app_id_str2,
                ApprovalDecision::Approved {
                    modified_details: None,
                    user_note: Some("Concurrent approval 2".into()),
                },
            )
            .await
    });

    let (res1, res2) = tokio::join!(h1, h2);
    let r1 = res1.unwrap();
    let r2 = res2.unwrap();

    let success_count = (r1.is_ok() as usize) + (r2.is_ok() as usize);
    assert_eq!(
        success_count, 1,
        "Exactly one concurrent approval resolution must succeed; r1: {:?}, r2: {:?}",
        r1, r2
    );

    let err_str = if r1.is_err() {
        format!("{:?}", r1)
    } else {
        format!("{:?}", r2)
    };
    assert!(
        err_str.contains("Approval resolution already in progress")
            || err_str.contains("already in terminal state")
            || err_str.contains("Approval already resolved"),
        "Losing concurrent approval must fail closed with clean serialization error: {err_str}"
    );

    // Verify Control Plane state is Approved
    {
        let cp = cp_lock.read().await;
        let app = cp.get_approval(cp_approval_id).unwrap();
        assert_eq!(app.state, ApprovalState::Approved);
    }
}

#[tokio::test]
async fn test_deterministic_model_routing_multi_model_unconfigured_fails_closed() {
    let mock_runtime = Arc::new(TestMockRuntime::new(false));
    let key_id = "test-model-routing-multi";
    let secret = b"secret-model-routing-123".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    // Create catalog with 2 models
    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let inst_id = ProviderInstanceId::new();
    let inst = ProviderInstance::new(
        inst_id,
        ProviderId::new("openai").unwrap(),
        "OpenAI Instance",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(inst).unwrap();

    let m1 = ModelDescriptor::new(
        inst_id,
        ModelId::new("gpt-4o").unwrap(),
        "GPT-4o",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(m1).unwrap();

    let m2 = ModelDescriptor::new(
        inst_id,
        ModelId::new("gpt-4o-mini").unwrap(),
        "GPT-4o Mini",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(m2).unwrap();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_provider_catalog(catalog)
        .with_auto_execute(true)
        // default_model_ref is intentionally NOT configured!
        .build()
        .expect("Server failed to build");

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret))
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Multi Model Studio").await.unwrap();
    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio.id, "Routing Agent", AgentKind::External, None)
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-multi-model".to_string(),
        title: "Multi Model Task".to_string(),
        description: "Task under ambiguous catalog".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client.submit_task(studio.id, spec).await.unwrap();
    let task_id = task_ids[0];

    // Wait for task to transition to Failed
    let mut failed = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let cp = cp_lock.read().await;
        if let Some(task) = cp.get_task(task_id)
            && task.state == TaskState::Failed
        {
            failed = true;
            break;
        }
    }
    assert!(
        failed,
        "Task must fail closed when catalog has multiple models without route"
    );

    // Verify run outcome has explicit routing error
    {
        let cp = cp_lock.read().await;
        let run = cp
            .all_runs()
            .find(|r| r.task_id == task_id)
            .expect("Run must be created");
        assert_eq!(run.state, agent_studios_protocol::run::RunState::Failed);
    }
    let events = server
        .state()
        .replay_buffer
        .lock()
        .unwrap()
        .replay_from(1, None)
        .unwrap_or_default();
    let found_error = events.iter().any(|ev| {
        ev.payload.to_string().contains(
            "Multiple models available (2) in catalog but no explicit model route configured",
        )
    });
    assert!(
        found_error,
        "Expected multi-model routing error in replay buffer events"
    );
}

#[tokio::test]
async fn test_deterministic_model_routing_missing_model_fails_closed() {
    let mock_runtime = Arc::new(TestMockRuntime::new(false));
    let key_id = "test-model-routing-missing";
    let secret = b"secret-model-missing-123".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let inst_id = ProviderInstanceId::new();
    let inst = ProviderInstance::new(
        inst_id,
        ProviderId::new("openai").unwrap(),
        "OpenAI Instance",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(inst).unwrap();

    // Configure a model ref that does NOT exist in catalog
    let missing_model_ref = ModelRef::new(inst_id, ModelId::new("claude-non-existent").unwrap());

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_provider_catalog(catalog)
        .with_default_model_ref(missing_model_ref)
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret))
        .await
        .expect("Client handshake failed");

    let studio = client.create_studio("Missing Model Studio").await.unwrap();
    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio.id, "Routing Agent", AgentKind::External, None)
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-missing-model".to_string(),
        title: "Missing Model Task".to_string(),
        description: "Task with missing model ref".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client.submit_task(studio.id, spec).await.unwrap();
    let task_id = task_ids[0];

    // Wait for task to transition to Failed
    let mut failed = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let cp = cp_lock.read().await;
        if let Some(task) = cp.get_task(task_id)
            && task.state == TaskState::Failed
        {
            failed = true;
            break;
        }
    }
    assert!(
        failed,
        "Task must fail closed when configured model is not found in catalog"
    );

    // Verify run outcome has explicit missing model error
    {
        let cp = cp_lock.read().await;
        let run = cp
            .all_runs()
            .find(|r| r.task_id == task_id)
            .expect("Run must be created");
        assert_eq!(run.state, agent_studios_protocol::run::RunState::Failed);
    }
    let events = server
        .state()
        .replay_buffer
        .lock()
        .unwrap()
        .replay_from(1, None)
        .unwrap_or_default();
    let found_error = events.iter().any(|ev| {
        ev.payload
            .to_string()
            .contains("not found in provider catalog")
    });
    assert!(
        found_error,
        "Expected missing model error in replay buffer events"
    );
}

#[tokio::test]
async fn test_deterministic_model_routing_configured_model_succeeds() {
    let mock_runtime = Arc::new(TestMockRuntime::new(false));
    let key_id = "test-model-routing-success";
    let secret = b"secret-model-success-123".to_vec();

    let test_repo = IsolatedTestRepo::new();
    let orchestrator = test_repo.create_orchestrator();

    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let inst_id = ProviderInstanceId::new();
    let inst = ProviderInstance::new(
        inst_id,
        ProviderId::new("openai").unwrap(),
        "OpenAI Instance",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(inst).unwrap();

    let m1 = ModelDescriptor::new(
        inst_id,
        ModelId::new("gpt-4o").unwrap(),
        "GPT-4o",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(m1.clone()).unwrap();

    let m2 = ModelDescriptor::new(
        inst_id,
        ModelId::new("gpt-4o-mini").unwrap(),
        "GPT-4o Mini",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(m2).unwrap();

    let configured_model_ref = m1.model_ref();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
        .with_workspace_orchestrator(orchestrator)
        .with_default_repo_path(test_repo.path())
        .with_provider_catalog(catalog)
        .with_default_model_ref(configured_model_ref)
        .with_auto_execute(true)
        .build()
        .expect("Server failed to build");

    let (c_stream, s_stream) = tokio::io::duplex(65536);
    server.handle_connection(s_stream);
    let mut client = IpcClient::connect_stream(c_stream, ClientConfig::new(key_id, secret))
        .await
        .expect("Client handshake failed");

    let studio = client
        .create_studio("Configured Model Studio")
        .await
        .unwrap();
    let cp_lock = server.control_plane();
    let agent_id = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio.id, "Routing Agent", AgentKind::External, None)
            .unwrap();
        agent.id
    };

    let spec = BatchTaskSpec {
        key: "task-configured-model".to_string(),
        title: "Configured Model Task".to_string(),
        description: "Task with explicit model route".to_string(),
        assigned_agent_id: Some(agent_id),
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client.submit_task(studio.id, spec).await.unwrap();
    let task_id = task_ids[0];

    // Wait until runtime session is registered in active_runtime_sessions
    let state = server.state();
    let mut session_active = false;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let sessions = state.active_runtime_sessions.read().await;
        if sessions.contains_key(&task_id) {
            session_active = true;
            break;
        }
    }
    assert!(
        session_active,
        "Session must be active when model is explicitly routed"
    );

    // Task should be in Running state
    {
        let cp = cp_lock.read().await;
        let task = cp.get_task(task_id).unwrap();
        assert_eq!(task.state, TaskState::Running);
    }
}
