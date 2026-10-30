use std::time::Duration;
use tokio::time::timeout;

use agent_studios_ipc_protocol::{ApprovalDecision, ApprovalStatus, IpcEvent};
use agent_studios_ipc_transport::{ClientConfig, IpcClient, IpcServer};
use agent_studios_protocol::agent::AgentKind;
use agent_studios_protocol::approval::{ApprovalKind, ApprovalState};
use agent_studios_protocol::id::{StudioId, TaskId};
use agent_studios_protocol::task::{BatchTaskSpec, DependencyOutputPolicy, TaskState};
use agent_studios_provider::{
    AuthenticationScheme, EndpointProfile, ModelCapabilities, ModelDescriptor, ModelId,
    ModelLimits, ProtocolFamily, ProviderCatalog, ProviderId, ProviderInstance, ProviderInstanceId,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn test_ipc_studio_and_session_discovery() {
    let key_id = "test-key-studio-discovery";
    let secret = b"secret-studio-discovery-123".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    // Verify session ID exists
    assert!(client.session_id().await.is_some());

    // Initially no studios exist
    let initial_studios = client.list_studios().await.expect("List studios failed");
    assert!(initial_studios.is_empty());

    // Create studio
    let studio = client
        .create_studio("Test Discovery Studio Alpha")
        .await
        .expect("Create studio failed");
    assert_eq!(studio.name, "Test Discovery Studio Alpha");

    // List studios should now return exactly the created studio
    let studios = client.list_studios().await.expect("List studios failed");
    assert_eq!(studios.len(), 1);
    assert_eq!(studios[0].id, studio.id);
    assert_eq!(studios[0].name, "Test Discovery Studio Alpha");

    // Get studio by id
    let fetched = client
        .get_studio(studio.id)
        .await
        .expect("Get studio failed");
    assert_eq!(fetched, Some(studio));

    // Get non-existent studio returns None
    let missing = client
        .get_studio(StudioId::new())
        .await
        .expect("Get missing studio failed");
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_ipc_task_submission_and_lifecycle() {
    let key_id = "test-key-task-lifecycle";
    let secret = b"secret-task-lifecycle-123".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    let studio = client
        .create_studio("Task Lifecycle Studio")
        .await
        .expect("Create studio failed");

    // Submit batch task
    let spec = BatchTaskSpec {
        key: "task-codebase-analysis".to_string(),
        title: "Analyze Repository Structure".to_string(),
        description: "Parse crate graph and verify dependency boundaries".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client
        .submit_task(studio.id, spec)
        .await
        .expect("Submit task failed");
    assert_eq!(task_ids.len(), 1);
    let task_id = task_ids[0];

    // List tasks
    let tasks = client
        .list_tasks(studio.id)
        .await
        .expect("List tasks failed");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, task_id);
    assert_eq!(tasks[0].title, "Analyze Repository Structure");
    assert_eq!(tasks[0].state, TaskState::Ready);

    // Get task by id
    let task = client
        .get_task(studio.id, task_id)
        .await
        .expect("Get task failed");
    assert!(task.is_some());
    let task = task.unwrap();
    assert_eq!(task.id, task_id);
    assert_eq!(task.title, "Analyze Repository Structure");
    assert_eq!(task.state, TaskState::Ready);

    // Non-existent task returns None
    let missing_task = client
        .get_task(studio.id, TaskId::new())
        .await
        .expect("Get missing task failed");
    assert!(missing_task.is_none());
}

#[tokio::test]
async fn test_ipc_task_cancellation() {
    let key_id = "test-key-task-cancel";
    let secret = b"secret-task-cancel-123".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    let studio = client
        .create_studio("Task Cancellation Studio")
        .await
        .expect("Create studio failed");

    let spec = BatchTaskSpec {
        key: "long-running-build".to_string(),
        title: "Compile Whole Workspace".to_string(),
        description: "Run cargo build --all-targets across all crates".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client
        .submit_task(studio.id, spec)
        .await
        .expect("Submit task failed");
    let task_id = task_ids[0];

    // Verify task is ready before cancellation
    let initial_task = client
        .get_task(studio.id, task_id)
        .await
        .expect("Get task failed")
        .expect("Task not found");
    assert_eq!(initial_task.state, TaskState::Ready);

    // Cancel the task
    client
        .cancel_task(studio.id, task_id, "User requested cancellation via IPC")
        .await
        .expect("Cancel task failed");

    // Verify task is now Cancelled
    let cancelled_task = client
        .get_task(studio.id, task_id)
        .await
        .expect("Get task failed")
        .expect("Task not found");
    assert_eq!(cancelled_task.state, TaskState::Cancelled);

    // Cancelling a non-existent task fails
    let err = client
        .cancel_task(studio.id, TaskId::new(), "Non-existent task")
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn test_ipc_approval_resolution() {
    let key_id = "test-key-approval-resolution";
    let secret = b"secret-approval-res-123".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    let studio = client
        .create_studio("Approval Resolution Studio")
        .await
        .expect("Create studio failed");

    // Register agent, task, and request approval in Control Plane
    let cp_lock = server.control_plane();
    let (agent_id, approval_id) = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio.id, "worker-agent-1", AgentKind::Internal, None)
            .expect("Register agent failed");
        let task = cp
            .create_task(
                studio.id,
                "Execute schema migration",
                "Runs database migrations",
                None,
                Some(agent.id),
                Vec::new(),
            )
            .expect("Create task failed");
        let approval = cp
            .request_approval(
                studio.id,
                task.id,
                agent.id,
                ApprovalKind::CommandExecution,
                "Operator authorization required for migration",
            )
            .expect("Request approval failed");
        (agent.id, approval.id)
    };

    // Client resolves approval as Authorized
    let status = client
        .resolve_approval(
            approval_id.to_string(),
            ApprovalDecision::Approved {
                modified_details: None,
                user_note: Some("Migration authorized by ops team".to_string()),
            },
        )
        .await
        .expect("Resolve approval failed");
    assert_eq!(status, ApprovalStatus::Authorized);

    // Verify Control Plane state is updated
    {
        let cp = cp_lock.read().await;
        let approval = cp
            .get_approval(approval_id)
            .expect("Approval should exist in Control Plane");
        assert_eq!(approval.state, ApprovalState::Approved);
    }

    // Request second approval and resolve as Rejected
    let approval_id_2 = {
        let mut cp = cp_lock.write().await;
        let task2 = cp
            .create_task(
                studio.id,
                "Drop table",
                "Drop table in database",
                None,
                Some(agent_id),
                Vec::new(),
            )
            .expect("Create task 2 failed");
        let approval2 = cp
            .request_approval(
                studio.id,
                task2.id,
                agent_id,
                ApprovalKind::CommandExecution,
                "Authorize dropping production table",
            )
            .expect("Request approval 2 failed");
        approval2.id
    };

    let status2 = client
        .resolve_approval(
            approval_id_2.to_string(),
            ApprovalDecision::Rejected {
                reason: "Dangerous operation rejected by ops policy".to_string(),
            },
        )
        .await
        .expect("Resolve approval 2 failed");
    assert_eq!(status2, ApprovalStatus::Rejected);

    {
        let cp = cp_lock.read().await;
        let approval2 = cp
            .get_approval(approval_id_2)
            .expect("Approval 2 should exist in Control Plane");
        assert_eq!(approval2.state, ApprovalState::Denied);
    }
}

#[tokio::test]
async fn test_ipc_provider_catalog_query() {
    let key_id = "test-key-provider-catalog";
    let secret = b"secret-provider-cat-123".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_provider_catalog(ProviderCatalog::with_builtin_definitions())
        .build()
        .expect("Server failed to build");

    let inst_id = ProviderInstanceId::new();
    let prov_id = ProviderId::new("openai").unwrap();
    let inst = ProviderInstance::new(
        inst_id,
        prov_id,
        "OpenAI Production Instance",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    let model_desc = ModelDescriptor::new(
        inst_id,
        ModelId::new("gpt-4o-mini").unwrap(),
        "GPT-4o Mini Fast",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();

    {
        let cat_lock = server.provider_catalog();
        let mut catalog = cat_lock.write().await;
        catalog
            .register_provider_instance(inst)
            .expect("Register provider instance failed");
        catalog
            .register_model(model_desc)
            .expect("Register model failed");
    }

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    // List providers
    let providers = client
        .list_providers()
        .await
        .expect("List providers failed");
    assert!(
        providers
            .iter()
            .any(|p| p.id == inst_id && p.display_name == "OpenAI Production Instance")
    );

    // Get specific provider
    let fetched = client
        .get_provider(inst_id)
        .await
        .expect("Get provider failed");
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().id, inst_id);

    // Get non-existent provider
    let missing = client
        .get_provider(ProviderInstanceId::new())
        .await
        .expect("Get missing provider failed");
    assert!(missing.is_none());

    // List all models
    let all_models = client.list_models(None).await.expect("List models failed");
    assert!(all_models.iter().any(|m| m.id.as_str() == "gpt-4o-mini"));

    // List models for specific instance
    let filtered_models = client
        .list_models(Some(inst_id))
        .await
        .expect("List instance models failed");
    assert_eq!(filtered_models.len(), 1);
    assert_eq!(filtered_models[0].id.as_str(), "gpt-4o-mini");

    // List models for non-existent instance
    let empty_models = client
        .list_models(Some(ProviderInstanceId::new()))
        .await
        .expect("List empty models failed");
    assert!(empty_models.is_empty());
}

#[tokio::test]
async fn test_ipc_linearizable_replay_and_reconnect() {
    let key_id = "test-key-replay-reconnect";
    let secret = b"secret-replay-reconnect-123".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_replay_capacity(200)
        .build()
        .expect("Server failed to build");

    let config = ClientConfig::new(key_id, secret.clone());

    // Step 1: Client 1 connects and subscribes starting from sequence 1
    let (client1_stream, server1_stream) = tokio::io::duplex(65536);
    server.handle_connection(server1_stream);

    let mut client1 = IpcClient::connect_stream(client1_stream, config.clone())
        .await
        .expect("Client 1 handshake failed");

    let mut sub1 = client1
        .subscribe_events(1)
        .await
        .expect("Client 1 subscribe failed");

    // Server emits initial 3 events
    for i in 1..=3 {
        let event = IpcEvent::new(
            i,
            "session_lifecycle_event",
            serde_json::json!({ "phase": "initial", "seq": i }),
        );
        server
            .publish_event(event)
            .expect("Failed to publish initial event");
    }

    // Client 1 receives events 1..=3
    let mut client1_seqs = Vec::new();
    for _ in 1..=3 {
        let event = timeout(TEST_TIMEOUT, sub1.next_event())
            .await
            .expect("Timeout on client 1 event")
            .expect("Error receiving client 1 event");
        client1_seqs.push(event.sequence);
    }
    assert_eq!(client1_seqs, vec![1, 2, 3]);
    assert_eq!(client1.last_received_sequence(), Some(3));
    assert_eq!(sub1.last_sequence(), Some(3));

    // Step 2: Client 1 disconnects (drop client1 and sub1)
    drop(sub1);
    drop(client1);

    // Step 3: While disconnected, server emits events 4..=7
    for i in 4..=7 {
        let event = IpcEvent::new(
            i,
            "daemon_offline_event",
            serde_json::json!({ "phase": "disconnected", "seq": i }),
        );
        server
            .publish_event(event)
            .expect("Failed to publish disconnected event");
    }

    // Step 4: Client 2 reconnects using sequence watermark 4 (client1_last_received + 1)
    let (client2_stream, server2_stream) = tokio::io::duplex(65536);
    server.handle_connection(server2_stream);

    let mut client2 = IpcClient::connect_stream(client2_stream, config.clone())
        .await
        .expect("Client 2 handshake failed");

    let watermark = 4;
    let mut sub2 = client2
        .subscribe_events(watermark)
        .await
        .expect("Client 2 subscribe failed");

    // Step 5: While Client 2 is streaming, server emits live events 8..=10
    for i in 8..=10 {
        let event = IpcEvent::new(
            i,
            "live_broadcast_event",
            serde_json::json!({ "phase": "live", "seq": i }),
        );
        server
            .publish_event(event)
            .expect("Failed to publish live event");
    }

    // Step 6: Client 2 receives events 4..=10 (replayed backfill 4..=7 + live broadcast 8..=10)
    let mut client2_seqs = Vec::new();
    for _ in 4..=10 {
        let event = timeout(TEST_TIMEOUT, sub2.next_event())
            .await
            .expect("Timeout on client 2 event")
            .expect("Error receiving client 2 event");
        client2_seqs.push(event.sequence);
    }

    // Verify linearizable delivery: exact sequence order, zero event loss, zero duplicates
    assert_eq!(
        client2_seqs,
        vec![4, 5, 6, 7, 8, 9, 10],
        "Client 2 must receive events 4..=10 in strict linearizable order"
    );
    assert_eq!(client2.last_received_sequence(), Some(10));
    assert_eq!(sub2.last_sequence(), Some(10));

    // Verify zero duplicates: client 2 never received events 1..=3
    assert!(!client2_seqs.iter().any(|seq| *seq < 4));

    // Verify zero loss: union of client 1 and client 2 sequences is contiguous 1..=10
    let mut all_delivered = client1_seqs;
    all_delivered.extend(client2_seqs);
    assert_eq!(
        all_delivered,
        (1..=10).collect::<Vec<u64>>(),
        "All events 1..=10 must be delivered with zero loss and zero duplicates"
    );
}

#[tokio::test]
async fn test_ipc_approval_cannot_resolve_for_terminal_task() {
    let key_id = "test-key-approval-terminal";
    let secret = b"secret-approval-term-123".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    let studio = client
        .create_studio("Approval Terminal Task Studio")
        .await
        .expect("Create studio failed");

    let cp_lock = server.control_plane();
    let (_agent_id, task_id, approval_id) = {
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio.id, "worker-agent-term", AgentKind::Internal, None)
            .expect("Register agent failed");
        let task = cp
            .create_task(
                studio.id,
                "Execute terminal task",
                "Description",
                None,
                Some(agent.id),
                Vec::new(),
            )
            .expect("Create task failed");
        let approval = cp
            .request_approval(
                studio.id,
                task.id,
                agent.id,
                ApprovalKind::CommandExecution,
                "Operation requiring approval",
            )
            .expect("Request approval failed");
        (agent.id, task.id, approval.id)
    };

    // Transition task to Running then Succeeded directly
    {
        let mut cp = cp_lock.write().await;
        cp.transition_task_state(task_id, TaskState::Running)
            .unwrap();
        cp.transition_task_state(task_id, TaskState::Succeeded)
            .unwrap();
        // Verify that pending approval was auto-cancelled on terminal state transition
        let app = cp.get_approval(approval_id).unwrap();
        assert_eq!(app.state, ApprovalState::Cancelled);
    }

    // Resolving approval for the terminal task must fail
    let err = client
        .resolve_approval(
            approval_id.to_string(),
            ApprovalDecision::Approved {
                modified_details: None,
                user_note: None,
            },
        )
        .await;
    assert!(
        err.is_err(),
        "Expected resolve_approval to fail for terminal task"
    );
}

#[tokio::test]
async fn test_ipc_multiple_subscriptions_no_duplicate_events() {
    let key_id = "test-key-multi-sub";
    let secret = b"secret-multi-sub-123".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    // Client subscribes first time
    let _sub1 = client.subscribe_events(1).await.expect("Sub 1 failed");
    // Ensure server processed sub1
    let _ = client.ping("sync1").await.expect("Ping 1 failed");

    // Client subscribes second time on same connection (should replace first subscription)
    let mut sub2 = client.subscribe_events(1).await.expect("Sub 2 failed");
    // Ensure server processed sub2 and aborted sub1 task
    let _ = client.ping("sync2").await.expect("Ping 2 failed");

    // Emit live event 1
    let event = IpcEvent::new(1, "live_event", serde_json::json!({ "v": 1 }));
    server.publish_event(event).unwrap();

    let ev = timeout(TEST_TIMEOUT, sub2.next_event())
        .await
        .expect("Timeout waiting for event")
        .expect("Error receiving event");
    assert_eq!(ev.sequence, 1);

    // Ensure no duplicate event is waiting on the stream
    let no_dup = timeout(Duration::from_millis(100), sub2.next_event()).await;
    assert!(
        no_dup.is_err(),
        "Expected timeout: no duplicate event should arrive"
    );
}

#[tokio::test]
async fn test_ipc_task_auto_execution_lifecycle() {
    let key_id = "test-key-auto-exec";
    let secret = b"secret-auto-exec-12345".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
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
        .create_studio("Auto Execute Studio")
        .await
        .expect("Create studio failed");

    let spec = BatchTaskSpec {
        key: "auto-run-task".to_string(),
        title: "Auto Execution Lifecycle Task".to_string(),
        description: "Task executed automatically via lifecycle runner".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
    };

    let task_ids = client
        .submit_task(studio.id, spec)
        .await
        .expect("Submit task failed");
    assert_eq!(task_ids.len(), 1);
    let task_id = task_ids[0];

    // Wait for auto-execution lifecycle to transition the task to Succeeded
    let cp_lock = server.control_plane();
    let mut succeeded = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let cp = cp_lock.read().await;
        if let Some(t) = cp.get_task(task_id)
            && t.state == TaskState::Succeeded
        {
            succeeded = true;
            break;
        }
    }
    assert!(
        succeeded,
        "Expected task to transition to Succeeded via auto-execution lifecycle"
    );

    // Verify task via client query
    let task = client
        .get_task(studio.id, task_id)
        .await
        .expect("Get task failed")
        .expect("Task should exist");
    assert_eq!(task.state, TaskState::Succeeded);

    // Verify events were emitted into replay buffer
    let state = server.state();
    let replay_len = state.replay_buffer.lock().unwrap().len();
    assert!(
        replay_len > 0,
        "Expected lifecycle progress events in replay buffer, got {}",
        replay_len
    );
}
