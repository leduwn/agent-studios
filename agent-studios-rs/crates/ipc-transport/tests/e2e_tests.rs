use std::time::Duration;
use tokio::time::timeout;

use agent_studios_ipc_protocol::IpcEvent;
use agent_studios_ipc_transport::{ClientConfig, IpcClient, IpcServer, TransportError};

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn test_e2e_connect_authenticate_and_ping() {
    let key_id = "test-key-auth-ping";
    let secret = b"secret-key-1234567890".to_vec();

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

    assert!(client.session_id().await.is_some());

    // Send Ping, verify Pong with matching payload and valid roundtrip latency
    let pong = client
        .ping("hello-agent-studios")
        .await
        .expect("Ping failed");

    assert_eq!(pong.payload, "hello-agent-studios");
    assert!(pong.client_timestamp > 0);
    assert!(pong.server_timestamp > 0);
}

#[tokio::test]
async fn test_e2e_unauthenticated_command_rejected() {
    let key_id = "test-key-unauth";
    let secret = b"secret-key-unauth-test".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    let server_handle = server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    // Connect stream without performing handshake
    let mut client = IpcClient::connect_stream_unauthenticated(client_stream, config);

    // Attempting to send GetHealth without authenticating
    let res = client.get_health().await;
    assert!(
        res.is_err(),
        "Expected unauthenticated command to be rejected by server"
    );

    // Server should terminate connection session
    let server_res = timeout(TEST_TIMEOUT, server_handle)
        .await
        .expect("Server handler timed out")
        .expect("Server task panicked");
    assert!(
        server_res.is_err(),
        "Expected server session to terminate with error"
    );
}

#[tokio::test]
async fn test_e2e_wrong_auth_key_rejected() {
    let key_id = "test-key-auth-fail";
    let correct_secret = b"correct-secret-123".to_vec();
    let wrong_secret = b"wrong-secret-999".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, correct_secret)
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, wrong_secret);
    let client_res = IpcClient::connect_stream(client_stream, config).await;

    assert!(
        client_res.is_err(),
        "Expected authentication to fail with incorrect secret key"
    );
    match client_res {
        Err(TransportError::AuthenticationFailed(msg)) => {
            assert!(
                msg.contains("HMAC") || msg.contains("verification") || msg.contains("auth"),
                "Unexpected auth error message: {}",
                msg
            );
        }
        Err(other) => panic!("Expected AuthenticationFailed error, got {:?}", other),
        Ok(_) => panic!("Expected handshake to fail"),
    }
}

#[tokio::test]
async fn test_e2e_event_subscription_and_live_broadcast() {
    let key_id = "test-key-events";
    let secret = b"secret-events-123".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_replay_capacity(100)
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    // Client subscribes to events starting from sequence 1
    let mut sub = client
        .subscribe_events(1)
        .await
        .expect("Subscription failed");

    // Server publishes 5 events
    for i in 1..=5 {
        let event = IpcEvent::new(
            i,
            "agent_state_changed",
            serde_json::json!({ "step": i, "status": "running" }),
        );
        server
            .publish_event(event)
            .expect("Failed to publish event");
    }

    // Client receives all 5 events in contiguous order (sequences 1..=5)
    let mut received_seqs = Vec::new();
    for _ in 1..=5 {
        let event = timeout(TEST_TIMEOUT, sub.next_event())
            .await
            .expect("Timeout waiting for event")
            .expect("Error receiving event");
        received_seqs.push(event.sequence);
    }

    assert_eq!(
        received_seqs,
        vec![1, 2, 3, 4, 5],
        "Events should arrive in contiguous sequence order"
    );
    assert_eq!(sub.last_sequence(), Some(5));
    assert_eq!(client.last_received_sequence(), Some(5));
}

#[tokio::test]
async fn test_e2e_disconnect_reconnect_with_watermark_recovery() {
    let key_id = "test-key-watermark";
    let secret = b"secret-watermark-123".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_replay_capacity(100)
        .build()
        .expect("Server failed to build");

    // Step 1: Client 1 subscribes, receives events 1..=3
    let (client1_stream, server1_stream) = tokio::io::duplex(65536);
    server.handle_connection(server1_stream);

    let config = ClientConfig::new(key_id, secret.clone());
    let mut client1 = IpcClient::connect_stream(client1_stream, config.clone())
        .await
        .expect("Client 1 handshake failed");

    let mut sub1 = client1
        .subscribe_events(1)
        .await
        .expect("Client 1 subscribe failed");

    for i in 1..=3 {
        let event = IpcEvent::new(i, "initial_batch", serde_json::json!({ "id": i }));
        server.publish_event(event).unwrap();
    }

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

    // Step 2: Client 1 disconnects
    client1.disconnect().await;
    drop(sub1);
    drop(client1);

    // Step 3: Server publishes events 4..=8 into ReplayBuffer
    for i in 4..=8 {
        let event = IpcEvent::new(i, "backfill_batch", serde_json::json!({ "id": i }));
        server.publish_event(event).unwrap();
    }

    // Step 4: Client 2 connects and subscribes with from_sequence = 4
    let (client2_stream, server2_stream) = tokio::io::duplex(65536);
    server.handle_connection(server2_stream);

    let mut client2 = IpcClient::connect_stream(client2_stream, config.clone())
        .await
        .expect("Client 2 handshake failed");

    let mut sub2 = client2
        .subscribe_events(4)
        .await
        .expect("Client 2 subscribe failed");

    // Step 5: Client receives backfill events 4..=8 without loss or duplication
    let mut backfilled_seqs = Vec::new();
    for _ in 4..=8 {
        let event = timeout(TEST_TIMEOUT, sub2.next_event())
            .await
            .expect("Timeout on client 2 backfill event")
            .expect("Error receiving backfill event");
        backfilled_seqs.push(event.sequence);
    }

    assert_eq!(
        backfilled_seqs,
        vec![4, 5, 6, 7, 8],
        "Backfill events should arrive in exact sequence without gaps"
    );
    assert_eq!(sub2.last_sequence(), Some(8));
    assert_eq!(client2.last_received_sequence(), Some(8));
}

#[tokio::test]
async fn test_e2e_reconnect_stream_auto_watermark_recovery() {
    let key_id = "test-key-auto-recovery";
    let secret = b"secret-auto-recovery-123".to_vec();

    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_replay_capacity(100)
        .build()
        .expect("Server failed to build");

    let (client_stream1, server_stream1) = tokio::io::duplex(65536);
    server.handle_connection(server_stream1);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream1, config)
        .await
        .expect("Client connect failed");

    let mut sub = client.subscribe_events(1).await.expect("Subscribe failed");

    for i in 1..=3 {
        let event = IpcEvent::new(i, "batch1", serde_json::json!({ "seq": i }));
        server.publish_event(event).unwrap();
    }

    for expected_seq in 1..=3 {
        let ev = timeout(TEST_TIMEOUT, sub.next_event())
            .await
            .expect("Timeout")
            .expect("Error");
        assert_eq!(ev.sequence, expected_seq);
    }
    assert_eq!(client.last_received_sequence(), Some(3));

    // Server publishes 4..=6 while connection is interrupted
    for i in 4..=6 {
        let event = IpcEvent::new(i, "batch2", serde_json::json!({ "seq": i }));
        server.publish_event(event).unwrap();
    }

    // Client reconnects with new stream. Reconnect automatically requests from_sequence = 4
    let (client_stream2, server_stream2) = tokio::io::duplex(65536);
    server.handle_connection(server_stream2);
    client
        .reconnect_stream(client_stream2)
        .await
        .expect("Reconnect failed");

    // Stream automatically continues receiving backfilled events 4..=6
    for expected_seq in 4..=6 {
        let ev = timeout(TEST_TIMEOUT, sub.next_event())
            .await
            .expect("Timeout on recovered stream")
            .expect("Error on recovered stream");
        assert_eq!(ev.sequence, expected_seq);
    }
    assert_eq!(client.last_received_sequence(), Some(6));
}

#[tokio::test]
async fn test_e2e_graceful_shutdown() {
    let key_id = "test-key-shutdown";
    let secret = b"secret-shutdown-123".to_vec();

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    let server_handle = server.handle_connection(server_stream);

    let config = ClientConfig::new(key_id, secret);
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client connect failed");

    // Send Shutdown command
    client.shutdown().await.expect("Shutdown command failed");

    // Server shutdown token should now be cancelled
    assert!(server.shutdown_token().is_cancelled());

    // Connection task should complete cleanly
    let handle_res = timeout(TEST_TIMEOUT, server_handle)
        .await
        .expect("Server handle timed out")
        .expect("Server task panicked");
    assert!(handle_res.is_ok());
}

#[cfg(windows)]
#[tokio::test]
async fn test_e2e_windows_named_pipe_full_lifecycle() {
    let pipe_name = format!(
        r"\\.\pipe\agent-studios-e2e-{}",
        uuid::Uuid::new_v4().simple()
    );
    let key_id = "win-pipe-key";
    let secret = b"win-pipe-secret-123".to_vec();

    let server = IpcServer::builder()
        .with_admin_auth_key(key_id, secret.clone())
        .with_replay_capacity(100)
        .build()
        .expect("Server failed to build");

    let server_pipe_name = pipe_name.clone();
    let server_clone = server.clone();
    let server_task =
        tokio::spawn(async move { server_clone.run_named_pipe(&server_pipe_name).await });

    // Allow named pipe listener to initialize
    tokio::time::sleep(Duration::from_millis(50)).await;

    let config = ClientConfig::new(key_id, secret);

    let mut client = IpcClient::connect_named_pipe(&pipe_name, config)
        .await
        .expect("Failed to connect to Windows Named Pipe");

    // Ping
    let pong = client
        .ping("windows-pipe-ping")
        .await
        .expect("Ping over Named Pipe failed");
    assert_eq!(pong.payload, "windows-pipe-ping");

    // Health
    let health = client
        .get_health()
        .await
        .expect("GetHealth over Named Pipe failed");
    assert_eq!(health.status, "healthy");
    assert!(health.active_clients >= 1);

    // Event subscription & broadcast
    let mut sub = client
        .subscribe_events(1)
        .await
        .expect("Subscribe over Named Pipe failed");

    let event = IpcEvent::new(1, "win_event", serde_json::json!({ "msg": "ok" }));
    server.publish_event(event).unwrap();

    let received = timeout(TEST_TIMEOUT, sub.next_event())
        .await
        .expect("Timeout on Named Pipe event")
        .expect("Error receiving Named Pipe event");
    assert_eq!(received.sequence, 1);

    // Graceful shutdown over Named Pipe
    client.shutdown().await.expect("Shutdown failed");

    // Server accept loop should exit cleanly
    let server_res = timeout(TEST_TIMEOUT, server_task)
        .await
        .expect("Server task did not exit in time")
        .expect("Server task panicked");
    assert!(server_res.is_ok());
    assert!(server.shutdown_token().is_cancelled());
}
