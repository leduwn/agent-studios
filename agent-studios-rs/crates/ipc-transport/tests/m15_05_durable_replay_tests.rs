use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

use agent_studios_control_plane::sqlite_store::SqliteEventStore;
use agent_studios_control_plane::store::{AnyEventStore, EventEnvelope, EventStore};
use agent_studios_control_plane::{ControlPlane, SystemClock};
use agent_studios_ipc_protocol::RecoveryStatusReport;
use agent_studios_ipc_transport::{ClientConfig, IpcClient, IpcServer};
use agent_studios_protocol::agent::{AgentDescriptor, AgentKind};
use agent_studios_protocol::event::ControlPlaneEvent;
use agent_studios_protocol::id::StudioId;
use agent_studios_protocol::studio::Studio;
use chrono::Utc;

const TEST_KEY_ID: &str = "m15-05-replay-key";
const TEST_SECRET: &[u8] = b"m15-05-replay-secret-abcdef";
const TEST_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn test_durable_replay_after_daemon_restart_from_persisted_sqlite() {
    let temp_dir = TempDir::new().expect("Failed to create tempdir");
    let db_path = temp_dir.path().join("durable_replay_restart.db");

    // Phase 1: Start Server 1, create 2 studios (sequences 1 and 2), client subscribes to 1, then crashes
    {
        let store = SqliteEventStore::open(&db_path).expect("Failed to open SQLite store");
        let cp = ControlPlane::restore_from_store(SystemClock, AnyEventStore::Sqlite(store))
            .expect("Failed to restore control plane");

        let server1 = IpcServer::builder()
            .with_auth_key(TEST_KEY_ID, TEST_SECRET.to_vec())
            .with_control_plane(cp)
            .build()
            .expect("Server 1 failed to build");

        let (client_stream, server_stream) = tokio::io::duplex(65536);
        server1.handle_connection(server_stream);

        let config = ClientConfig::new(TEST_KEY_ID, TEST_SECRET.to_vec());
        let mut client1 = IpcClient::connect_stream(client_stream, config)
            .await
            .expect("Client 1 handshake failed");

        let _studio1 = client1
            .create_studio("Replay Studio One")
            .await
            .expect("Create studio 1 failed");

        let _studio2 = client1
            .create_studio("Replay Studio Two")
            .await
            .expect("Create studio 2 failed");

        let mut sub1 = client1
            .subscribe_events(1)
            .await
            .expect("Subscribe events failed");

        let ev1 = timeout(TEST_TIMEOUT, sub1.next_event())
            .await
            .expect("Timeout on ev1")
            .expect("Error on ev1");
        assert_eq!(ev1.sequence, 1);
        assert_eq!(client1.last_received_sequence(), Some(1));

        // Client 1 disconnected at sequence 1; Server 1 shuts down
        drop(sub1);
        client1.disconnect().await;
    }

    assert!(db_path.exists());

    // Phase 2: Start Server 2 from the same SQLite database
    {
        let store = SqliteEventStore::open(&db_path).expect("Failed to re-open SQLite store");
        let cp = ControlPlane::restore_from_store(SystemClock, AnyEventStore::Sqlite(store))
            .expect("Failed to restore control plane");

        let total_events = cp.all_events().map(|v| v.len()).unwrap_or(0);
        assert_eq!(total_events, 2, "Persisted events must be 2");

        let recovery_status = RecoveryStatusReport {
            last_recovery_at: Some(Utc::now()),
            interrupted_tasks_count: 0,
            interrupted_runs_count: 0,
            worktrees_reconciled: 0,
            worktrees_missing: 0,
            worktrees_foreign: 0,
            database_healthy: true,
            database_path: Some(db_path.to_string_lossy().to_string()),
            total_persisted_events: total_events,
        };

        let server2 = IpcServer::builder()
            .with_auth_key(TEST_KEY_ID, TEST_SECRET.to_vec())
            .with_control_plane(cp)
            .with_recovery_status(recovery_status)
            .build()
            .expect("Server 2 failed to build");

        // Client 2 connects to Server 2
        let (client_stream, server_stream) = tokio::io::duplex(65536);
        server2.handle_connection(server_stream);

        let config = ClientConfig::new(TEST_KEY_ID, TEST_SECRET.to_vec());
        let mut client2 = IpcClient::connect_stream(client_stream, config.clone())
            .await
            .expect("Client 2 handshake failed");

        // Client 2 resumes from sequence K + 1 = 2 (missed event 2 while offline)
        let mut sub2 = client2
            .subscribe_events(2)
            .await
            .expect("Subscribe events from 2 must NOT fail with SequenceGapRecoverable");

        let ev2 = timeout(TEST_TIMEOUT, sub2.next_event())
            .await
            .expect("Timeout waiting for replayed event 2")
            .expect("Failed to receive replayed event 2");
        assert_eq!(ev2.sequence, 2);
        assert_eq!(client2.last_received_sequence(), Some(2));

        // Connect Client 3 to perform live action on Server 2
        let (client3_stream, server3_stream) = tokio::io::duplex(65536);
        server2.handle_connection(server3_stream);

        let mut client3 = IpcClient::connect_stream(client3_stream, config)
            .await
            .expect("Client 3 handshake failed");

        let _studio3 = client3
            .create_studio("Replay Studio Three Live")
            .await
            .expect("Create studio 3 failed");

        // Client 2 receives live broadcast event 3
        let ev3 = timeout(TEST_TIMEOUT, sub2.next_event())
            .await
            .expect("Timeout waiting for live broadcast event 3")
            .expect("Failed to receive live broadcast event 3");
        assert_eq!(ev3.sequence, 3);
        assert_eq!(client2.last_received_sequence(), Some(3));

        drop(sub2);
        client2.disconnect().await;
        client3.disconnect().await;
    }
}

#[tokio::test]
async fn test_durable_replay_sqlite_fallback_beyond_ring_buffer_capacity() {
    let temp_dir = TempDir::new().expect("Failed to create tempdir");
    let db_path = temp_dir.path().join("durable_replay_fallback.db");
    let studio_id = StudioId::new();
    let now = Utc::now();

    // 1. Seed SQLite database directly with 6 events (sequences 1..=6)
    {
        let mut store = SqliteEventStore::open(&db_path).expect("Failed to open store");
        let events = vec![
            EventEnvelope::new(
                studio_id,
                1,
                now,
                ControlPlaneEvent::StudioCreated {
                    studio: Studio::with_id(studio_id, "Fallback Studio", now),
                },
            ),
            EventEnvelope::new(
                studio_id,
                2,
                now,
                ControlPlaneEvent::AgentRegistered {
                    agent: AgentDescriptor::new(studio_id, "Agent 1", AgentKind::Internal, None),
                },
            ),
            EventEnvelope::new(
                studio_id,
                3,
                now,
                ControlPlaneEvent::AgentRegistered {
                    agent: AgentDescriptor::new(studio_id, "Agent 2", AgentKind::Internal, None),
                },
            ),
            EventEnvelope::new(
                studio_id,
                4,
                now,
                ControlPlaneEvent::AgentRegistered {
                    agent: AgentDescriptor::new(studio_id, "Agent 3", AgentKind::Internal, None),
                },
            ),
            EventEnvelope::new(
                studio_id,
                5,
                now,
                ControlPlaneEvent::AgentRegistered {
                    agent: AgentDescriptor::new(studio_id, "Agent 4", AgentKind::Internal, None),
                },
            ),
            EventEnvelope::new(
                studio_id,
                6,
                now,
                ControlPlaneEvent::AgentRegistered {
                    agent: AgentDescriptor::new(studio_id, "Agent 5", AgentKind::Internal, None),
                },
            ),
        ];

        store
            .append_batch(&events)
            .expect("Failed to append seed events");
    }

    // 2. Start Server with small replay buffer capacity = 2
    // Replay buffer holds only tail (sequences 5 and 6); sequences 1..4 are evicted from memory
    let store = SqliteEventStore::open(&db_path).expect("Failed to re-open store");
    let cp = ControlPlane::restore_from_store(SystemClock, AnyEventStore::Sqlite(store))
        .expect("Failed to restore control plane");

    let recovery_status = RecoveryStatusReport {
        last_recovery_at: Some(now),
        interrupted_tasks_count: 0,
        interrupted_runs_count: 0,
        worktrees_reconciled: 0,
        worktrees_missing: 0,
        worktrees_foreign: 0,
        database_healthy: true,
        database_path: Some(db_path.to_string_lossy().to_string()),
        total_persisted_events: 6,
    };

    let server = IpcServer::builder()
        .with_auth_key(TEST_KEY_ID, TEST_SECRET.to_vec())
        .with_control_plane(cp)
        .with_recovery_status(recovery_status)
        .with_replay_capacity(2) // Ring buffer holds at most 2 events!
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(TEST_KEY_ID, TEST_SECRET.to_vec());
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    // 3. Client subscribes from sequence 2
    // Since sequence 2 is evicted from the memory ring buffer, server must fall back to SQLite
    let mut sub = client
        .subscribe_events(2)
        .await
        .expect("Subscribe events from 2 must succeed via SQLite fallback");

    let mut received_sequences = Vec::new();
    for _ in 2..=6 {
        let ev = timeout(TEST_TIMEOUT, sub.next_event())
            .await
            .expect("Timeout on historical event")
            .expect("Failed to receive event");
        received_sequences.push(ev.sequence);
    }

    assert_eq!(
        received_sequences,
        vec![2, 3, 4, 5, 6],
        "Client must receive all events 2..=6 via SQLite fallback"
    );
    assert_eq!(client.last_received_sequence(), Some(6));

    client.disconnect().await;
}

#[tokio::test]
async fn test_caught_up_client_reconnect_after_restart() {
    let temp_dir = TempDir::new().expect("Failed to create tempdir");
    let db_path = temp_dir.path().join("durable_replay_caught_up.db");
    let studio_id = StudioId::new();
    let now = Utc::now();

    // 1. Seed 3 events in SQLite (sequences 1, 2, 3)
    {
        let mut store = SqliteEventStore::open(&db_path).expect("Failed to open store");
        let events = vec![
            EventEnvelope::new(
                studio_id,
                1,
                now,
                ControlPlaneEvent::StudioCreated {
                    studio: Studio::with_id(studio_id, "Caught Up Studio", now),
                },
            ),
            EventEnvelope::new(
                studio_id,
                2,
                now,
                ControlPlaneEvent::AgentRegistered {
                    agent: AgentDescriptor::new(
                        studio_id,
                        "Agent Alpha",
                        AgentKind::Internal,
                        None,
                    ),
                },
            ),
            EventEnvelope::new(
                studio_id,
                3,
                now,
                ControlPlaneEvent::AgentRegistered {
                    agent: AgentDescriptor::new(studio_id, "Agent Beta", AgentKind::Internal, None),
                },
            ),
        ];
        store.append_batch(&events).expect("Failed to append");
    }

    // 2. Start server
    let store = SqliteEventStore::open(&db_path).expect("Failed to re-open store");
    let cp = ControlPlane::restore_from_store(SystemClock, AnyEventStore::Sqlite(store))
        .expect("Failed to restore control plane");

    let recovery_status = RecoveryStatusReport {
        last_recovery_at: Some(now),
        interrupted_tasks_count: 0,
        interrupted_runs_count: 0,
        worktrees_reconciled: 0,
        worktrees_missing: 0,
        worktrees_foreign: 0,
        database_healthy: true,
        database_path: Some(db_path.to_string_lossy().to_string()),
        total_persisted_events: 3,
    };

    let server = IpcServer::builder()
        .with_auth_key(TEST_KEY_ID, TEST_SECRET.to_vec())
        .with_control_plane(cp)
        .with_recovery_status(recovery_status)
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(TEST_KEY_ID, TEST_SECRET.to_vec());
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    // 3. Client is caught up: requests from_sequence = 4 (latest + 1)
    let mut sub = client
        .subscribe_events(4)
        .await
        .expect("Subscribe from latest + 1 must succeed cleanly with empty backfill");

    // 4. Server emits live event 4
    let emitted = server
        .state()
        .emit_event(
            "live_event_phase",
            serde_json::json!({ "status": "active" }),
        )
        .expect("Failed to emit event");
    assert_eq!(emitted.sequence, 4);

    // 5. Client receives live event 4 without any historical replay delay or error
    let ev = timeout(TEST_TIMEOUT, sub.next_event())
        .await
        .expect("Timeout on live event")
        .expect("Failed to receive live event");
    assert_eq!(ev.sequence, 4);

    client.disconnect().await;
}

#[tokio::test]
async fn test_race_free_sqlite_replay_and_concurrent_broadcast_handshake() {
    let temp_dir = TempDir::new().expect("Failed to create tempdir");
    let db_path = temp_dir.path().join("durable_replay_race_free.db");
    let studio_id = StudioId::new();
    let now = Utc::now();

    // 1. Seed 4 historical events (sequences 1..=4)
    {
        let mut store = SqliteEventStore::open(&db_path).expect("Failed to open store");
        let events = vec![
            EventEnvelope::new(
                studio_id,
                1,
                now,
                ControlPlaneEvent::StudioCreated {
                    studio: Studio::with_id(studio_id, "Race-Free Studio", now),
                },
            ),
            EventEnvelope::new(
                studio_id,
                2,
                now,
                ControlPlaneEvent::AgentRegistered {
                    agent: AgentDescriptor::new(studio_id, "Agent 1", AgentKind::Internal, None),
                },
            ),
            EventEnvelope::new(
                studio_id,
                3,
                now,
                ControlPlaneEvent::AgentRegistered {
                    agent: AgentDescriptor::new(studio_id, "Agent 2", AgentKind::Internal, None),
                },
            ),
            EventEnvelope::new(
                studio_id,
                4,
                now,
                ControlPlaneEvent::AgentRegistered {
                    agent: AgentDescriptor::new(studio_id, "Agent 3", AgentKind::Internal, None),
                },
            ),
        ];
        store.append_batch(&events).expect("Failed to append");
    }

    // 2. Start server
    let store = SqliteEventStore::open(&db_path).expect("Failed to re-open store");
    let cp = ControlPlane::restore_from_store(SystemClock, AnyEventStore::Sqlite(store))
        .expect("Failed to restore control plane");

    let recovery_status = RecoveryStatusReport {
        last_recovery_at: Some(now),
        interrupted_tasks_count: 0,
        interrupted_runs_count: 0,
        worktrees_reconciled: 0,
        worktrees_missing: 0,
        worktrees_foreign: 0,
        database_healthy: true,
        database_path: Some(db_path.to_string_lossy().to_string()),
        total_persisted_events: 4,
    };

    let server = IpcServer::builder()
        .with_auth_key(TEST_KEY_ID, TEST_SECRET.to_vec())
        .with_control_plane(cp)
        .with_recovery_status(recovery_status)
        .build()
        .expect("Server failed to build");

    let (client_stream, server_stream) = tokio::io::duplex(65536);
    server.handle_connection(server_stream);

    let config = ClientConfig::new(TEST_KEY_ID, TEST_SECRET.to_vec());
    let mut client = IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Client handshake failed");

    // 3. Client subscribes from sequence 2
    let mut sub = client
        .subscribe_events(2)
        .await
        .expect("Subscribe events failed");

    // 4. Immediately emit live events 5 and 6
    let ev5 = server
        .state()
        .emit_event("live_e5", serde_json::json!({ "seq": 5 }))
        .expect("Emit 5 failed");
    let ev6 = server
        .state()
        .emit_event("live_e6", serde_json::json!({ "seq": 6 }))
        .expect("Emit 6 failed");
    assert_eq!(ev5.sequence, 5);
    assert_eq!(ev6.sequence, 6);

    // 5. Client must receive events 2, 3, 4 (historical backfill) + 5, 6 (live broadcast)
    // in strict monotonic sequence order with zero gaps and zero duplicates
    let mut received_seqs = Vec::new();
    for _ in 2..=6 {
        let ev = timeout(TEST_TIMEOUT, sub.next_event())
            .await
            .expect("Timeout on event")
            .expect("Failed to receive event");
        received_seqs.push(ev.sequence);
    }

    assert_eq!(
        received_seqs,
        vec![2, 3, 4, 5, 6],
        "Client must receive exactly [2, 3, 4, 5, 6] in monotonic order"
    );

    client.disconnect().await;
}
