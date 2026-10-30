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

use agent_studios_ipc_protocol::{ApprovalDecision, ApprovalStatus};
use agent_studios_ipc_transport::{
    ClientConfig, DaemonAdminBootstrapInfo, DaemonBootstrapInfo, IpcClient, IpcServer,
};
use agent_studios_protocol::agent::AgentKind;
use agent_studios_protocol::approval::{ApprovalKind, ApprovalState};
use agent_studios_protocol::task::{BatchTaskSpec, DependencyOutputPolicy, TaskState};

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

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
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

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_runtime(mock_runtime.clone())
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
