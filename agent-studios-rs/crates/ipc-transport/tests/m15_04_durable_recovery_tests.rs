use tempfile::TempDir;

use agent_studios_control_plane::sqlite_store::SqliteEventStore;
use agent_studios_control_plane::store::{AnyEventStore, EventEnvelope, EventStore, StoreError};
use agent_studios_control_plane::{ControlPlane, SystemClock};
use agent_studios_ipc_transport::{ClientConfig, IpcClient, IpcServer};
use agent_studios_protocol::agent::{AgentDescriptor, AgentKind};
use agent_studios_protocol::approval::{ApprovalKind, ApprovalState};
use agent_studios_protocol::event::ControlPlaneEvent;
use agent_studios_protocol::id::{AgentId, EventId, RunId, StudioId, TaskId, WorktreeId};
use agent_studios_protocol::run::{RunRecord, RunState};
use agent_studios_protocol::studio::Studio;
use agent_studios_protocol::task::{BatchTaskSpec, DependencyOutputPolicy, TaskRecord, TaskState};
use agent_studios_protocol::worktree::{WorktreeRecord, WorktreeState};

const TEST_KEY_ID: &str = "m15-04-recovery-key";
const TEST_SECRET: &[u8] = b"m15-04-recovery-secret-98765";

#[tokio::test]
async fn test_sqlite_wal_persistence_survives_daemon_restart() {
    let temp_dir = TempDir::new().expect("Failed to create tempdir");
    let db_path = temp_dir.path().join("durable_state.db");

    let studio_id_captured;
    let task_id_captured;

    // Phase 1: Start Server 1 backed by durable SQLite
    {
        let store = SqliteEventStore::open(&db_path).expect("Failed to open SQLite store");
        let cp = ControlPlane::restore_from_store(SystemClock, AnyEventStore::Sqlite(store))
            .expect("Failed to restore control plane");

        let server = IpcServer::builder()
            .with_auth_key(TEST_KEY_ID, TEST_SECRET.to_vec())
            .with_control_plane(cp)
            .build()
            .expect("Server 1 failed to build");

        let (client_stream, server_stream) = tokio::io::duplex(65536);
        server.handle_connection(server_stream);

        let config = ClientConfig::new(TEST_KEY_ID, TEST_SECRET.to_vec());
        let mut client = IpcClient::connect_stream(client_stream, config)
            .await
            .expect("Client 1 handshake failed");

        // Create Studio
        let studio = client
            .create_studio("M15-04 Persistent Studio")
            .await
            .expect("Create studio failed");
        studio_id_captured = studio.id;

        // Submit Task via IPC
        let task_spec = BatchTaskSpec {
            key: "durable-task-alpha".to_string(),
            title: "Durable Task Alpha".to_string(),
            description: "Task that must survive daemon restart".to_string(),
            assigned_agent_id: None,
            parent_task_key: None,
            parent_task_id: None,
            dependency_keys: vec![],
            dependency_task_ids: vec![],
            dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
            execution: None,
        };

        let task_ids = client
            .submit_task(studio_id_captured, task_spec)
            .await
            .expect("Submit task failed");
        assert_eq!(task_ids.len(), 1);
        task_id_captured = task_ids[0];

        client.disconnect().await;
        // Server 1 dropped here
    }

    assert!(db_path.exists(), "SQLite database file must exist on disk");

    // Phase 2: Start Server 2 from the exact same SQLite database
    {
        let store = SqliteEventStore::open(&db_path).expect("Failed to re-open SQLite store");
        let mut cp = ControlPlane::restore_from_store(SystemClock, AnyEventStore::Sqlite(store))
            .expect("Failed to restore control plane on restart");

        let (interrupted_tasks, interrupted_runs) = cp
            .reconcile_interrupted_executions()
            .expect("Reconcile interrupted executions failed");
        assert_eq!(interrupted_tasks, 0, "No tasks were running");
        assert_eq!(interrupted_runs, 0, "No runs were running");

        let total_events = cp.all_events().map(|v| v.len()).unwrap_or(0);
        assert!(
            total_events >= 2,
            "Expected persisted events for Studio and Task"
        );

        let recovery_status = agent_studios_ipc_protocol::RecoveryStatusReport {
            last_recovery_at: Some(chrono::Utc::now()),
            interrupted_tasks_count: interrupted_tasks,
            interrupted_runs_count: interrupted_runs,
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

        let (client_stream, server_stream) = tokio::io::duplex(65536);
        server2.handle_connection(server_stream);

        let config = ClientConfig::new(TEST_KEY_ID, TEST_SECRET.to_vec());
        let mut client2 = IpcClient::connect_stream(client_stream, config)
            .await
            .expect("Client 2 handshake failed");

        // Verify recovery status over IPC
        let status = client2
            .get_recovery_status()
            .await
            .expect("Failed to get recovery status");
        assert!(status.database_healthy);
        assert_eq!(
            status.database_path.as_deref(),
            Some(db_path.to_string_lossy().as_ref())
        );
        assert_eq!(status.total_persisted_events, total_events);

        // Verify durable Studio exists
        let studios = client2.list_studios().await.expect("List studios failed");
        assert_eq!(studios.len(), 1);
        assert_eq!(studios[0].id, studio_id_captured);
        assert_eq!(studios[0].name, "M15-04 Persistent Studio");

        // Verify durable Task exists
        let task = client2
            .get_task(studio_id_captured, task_id_captured)
            .await
            .expect("Get task failed");
        assert!(task.is_some());
        let task = task.unwrap();
        assert_eq!(task.id, task_id_captured);
        assert_eq!(task.studio_id, studio_id_captured);
        assert_eq!(task.title, "Durable Task Alpha");

        // Verify continuing mutations on Server 2
        let task_spec2 = BatchTaskSpec {
            key: "durable-task-beta".to_string(),
            title: "Durable Task Beta".to_string(),
            description: "Created after daemon restart".to_string(),
            assigned_agent_id: None,
            parent_task_key: None,
            parent_task_id: None,
            dependency_keys: vec![],
            dependency_task_ids: vec![task_id_captured],
            dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
            execution: None,
        };

        let tasks2 = client2
            .submit_task(studio_id_captured, task_spec2)
            .await
            .expect("Submit task 2 failed");
        assert_eq!(tasks2.len(), 1);

        client2.disconnect().await;
    }
}

#[tokio::test]
async fn test_crash_recovery_transitions_in_flight_tasks_and_runs() {
    let temp_dir = TempDir::new().expect("Failed to create tempdir");
    let db_path = temp_dir.path().join("crash_state.db");

    let studio_id = StudioId::new();
    let task_id = TaskId::new();
    let agent_id = AgentId::new();
    let run_id = RunId::new();

    // Seed SQLite store with in-flight running task and run
    {
        let mut store = SqliteEventStore::open(&db_path).expect("Failed to open SQLite store");
        let now = chrono::Utc::now();

        let mut agent = AgentDescriptor::new(studio_id, "Worker Agent", AgentKind::Internal, None);
        agent.id = agent_id;

        let mut task = TaskRecord::new(
            studio_id,
            "In-Flight Running Task",
            "Was running when power cut off",
            None,
            Some(agent_id),
            vec![],
            now,
        );
        task.id = task_id;

        let mut run = RunRecord::new(task_id, agent_id, 1);
        run.id = run_id;

        let events = vec![
            EventEnvelope::new(
                studio_id,
                1,
                now,
                ControlPlaneEvent::StudioCreated {
                    studio: Studio::with_id(studio_id, "Crash Test Studio", now),
                },
            ),
            EventEnvelope::new(
                studio_id,
                2,
                now,
                ControlPlaneEvent::AgentRegistered { agent },
            ),
            EventEnvelope::new(studio_id, 3, now, ControlPlaneEvent::TaskCreated { task }),
            EventEnvelope::new(
                studio_id,
                4,
                now,
                ControlPlaneEvent::TaskStateChanged {
                    task_id,
                    previous_state: TaskState::Ready,
                    new_state: TaskState::Running,
                },
            ),
            EventEnvelope::new(studio_id, 5, now, ControlPlaneEvent::RunCreated { run }),
            EventEnvelope::new(
                studio_id,
                6,
                now,
                ControlPlaneEvent::RunStateChanged {
                    run_id,
                    previous_state: RunState::Queued,
                    new_state: RunState::Starting,
                },
            ),
            EventEnvelope::new(
                studio_id,
                7,
                now,
                ControlPlaneEvent::RunStateChanged {
                    run_id,
                    previous_state: RunState::Starting,
                    new_state: RunState::Running,
                },
            ),
            EventEnvelope::new(
                studio_id,
                8,
                now,
                ControlPlaneEvent::ApprovalRequested {
                    approval: agent_studios_protocol::approval::ApprovalRequest::new(
                        studio_id,
                        task_id,
                        agent_id,
                        ApprovalKind::CommandExecution,
                        "Pending file delete authorization",
                        now,
                    ),
                },
            ),
        ];

        store
            .append_batch(&events)
            .expect("Failed to seed crash events");
        // Daemon crashed without completing run or task!
    }

    // Phase 2: Start new daemon instance pointing at the same SQLite database
    {
        let store =
            SqliteEventStore::open(&db_path).expect("Failed to open SQLite store on restart");
        let mut cp = ControlPlane::restore_from_store(SystemClock, AnyEventStore::Sqlite(store))
            .expect("Failed to restore control plane");

        // Verify pre-reconciliation state
        assert_eq!(cp.get_task(task_id).unwrap().state, TaskState::Running);
        assert_eq!(cp.get_run(run_id).unwrap().state, RunState::Running);

        // Run crash recovery reconciliation
        let (interrupted_tasks, interrupted_runs) = cp
            .reconcile_interrupted_executions()
            .expect("Failed to reconcile interrupted executions");

        assert_eq!(interrupted_tasks, 1, "Exactly one task was running");
        assert_eq!(interrupted_runs, 1, "Exactly one run was running");

        // Verify reconciled state in memory
        let reconciled_task = cp.get_task(task_id).unwrap();
        assert_eq!(
            reconciled_task.state,
            TaskState::Failed,
            "Interrupted task must transition to Failed"
        );

        let reconciled_run = cp.get_run(run_id).unwrap();
        assert_eq!(
            reconciled_run.state,
            RunState::Failed,
            "Interrupted run must transition to Failed"
        );

        // Verify pending approval was auto-cancelled
        let approvals: Vec<_> = cp
            .all_approvals()
            .filter(|a| a.studio_id == studio_id)
            .collect();
        assert_eq!(approvals.len(), 1);
        assert_eq!(
            approvals[0].state,
            ApprovalState::Cancelled,
            "Pending approval must be cancelled when task fails"
        );

        // Verify domain events persisted to SQLite include daemon_interrupted classification
        let all_events = cp.all_events().expect("Failed to retrieve events");
        let interruption_event = all_events.iter().find(|e| {
            if let ControlPlaneEvent::RunOutcomeRecorded {
                run_id: r_id,
                classification,
                ..
            } = &e.event
            {
                *r_id == run_id && classification == "daemon_interrupted"
            } else {
                false
            }
        });
        assert!(
            interruption_event.is_some(),
            "Expected RunOutcomeRecorded with 'daemon_interrupted' classification"
        );

        // Expose via IPC server and verify client view
        let recovery_status = agent_studios_ipc_protocol::RecoveryStatusReport {
            last_recovery_at: Some(chrono::Utc::now()),
            interrupted_tasks_count: interrupted_tasks,
            interrupted_runs_count: interrupted_runs,
            worktrees_reconciled: 0,
            worktrees_missing: 0,
            worktrees_foreign: 0,
            database_healthy: true,
            database_path: Some(db_path.to_string_lossy().to_string()),
            total_persisted_events: all_events.len(),
        };

        let server = IpcServer::builder()
            .with_auth_key(TEST_KEY_ID, TEST_SECRET.to_vec())
            .with_control_plane(cp)
            .with_recovery_status(recovery_status)
            .build()
            .expect("Server failed to build");

        let (client_stream, server_stream) = tokio::io::duplex(65536);
        server2_handle(server, server_stream);

        let config = ClientConfig::new(TEST_KEY_ID, TEST_SECRET.to_vec());
        let mut client = IpcClient::connect_stream(client_stream, config)
            .await
            .expect("Client handshake failed");

        let status = client
            .get_recovery_status()
            .await
            .expect("Failed to get recovery status");
        assert_eq!(status.interrupted_tasks_count, 1);
        assert_eq!(status.interrupted_runs_count, 1);

        let task_via_ipc = client
            .get_task(studio_id, task_id)
            .await
            .expect("Get task failed");
        assert!(task_via_ipc.is_some());
        assert_eq!(task_via_ipc.unwrap().state, TaskState::Failed);

        client.disconnect().await;
    }
}

fn server2_handle(server: IpcServer, stream: tokio::io::DuplexStream) {
    server.handle_connection(stream);
}

#[tokio::test]
async fn test_sqlite_preflight_corruption_detection_and_rejection() {
    let temp_dir = TempDir::new().expect("Failed to create tempdir");
    let corrupt_db_path = temp_dir.path().join("corrupt_database.db");

    // Write corrupted garbage data to the SQLite path
    std::fs::write(
        &corrupt_db_path,
        b"NOT_A_VALID_SQLITE_DATABASE_HEADER_CORRUPTED_BYTES",
    )
    .expect("Failed to write corrupt file");

    let open_result = SqliteEventStore::open(&corrupt_db_path);
    assert!(
        open_result.is_err(),
        "Opening a corrupt database must fail fast"
    );

    match open_result.err().unwrap() {
        StoreError::StorageFailure(msg) => {
            assert!(
                msg.contains("Corrupt")
                    || msg.contains("file is not a database")
                    || msg.contains("quick_check"),
                "Error message should mention database corruption: {msg}"
            );
        }
        other => panic!("Expected StoreError::StorageFailure, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_sqlite_batch_atomicity_and_gap_rejection() {
    let temp_dir = TempDir::new().expect("Failed to create tempdir");
    let db_path = temp_dir.path().join("atomicity_test.db");

    let mut store = SqliteEventStore::open(&db_path).expect("Failed to open SQLite store");
    let studio_id = StudioId::new();
    let now = chrono::Utc::now();

    // 1. Valid batch of 3 events (sequences 1, 2, 3)
    let valid_batch = vec![
        EventEnvelope::new(
            studio_id,
            1,
            now,
            ControlPlaneEvent::StudioCreated {
                studio: Studio::with_id(studio_id, "Studio Atomicity", now),
            },
        ),
        EventEnvelope::new(
            studio_id,
            2,
            now,
            ControlPlaneEvent::AgentRegistered {
                agent: AgentDescriptor::new(studio_id, "Agent A", AgentKind::Internal, None),
            },
        ),
        EventEnvelope::new(
            studio_id,
            3,
            now,
            ControlPlaneEvent::AgentRegistered {
                agent: AgentDescriptor::new(studio_id, "Agent B", AgentKind::Internal, None),
            },
        ),
    ];

    store
        .append_batch(&valid_batch)
        .expect("Valid batch should append successfully");
    assert_eq!(store.latest_sequence(studio_id).unwrap(), 3);

    // 2. Batch with sequence gap (sequence 5 instead of 4) -> must fail and roll back
    let gap_batch = vec![
        EventEnvelope::new(
            studio_id,
            4,
            now,
            ControlPlaneEvent::AgentRegistered {
                agent: AgentDescriptor::new(studio_id, "Agent C", AgentKind::Internal, None),
            },
        ),
        EventEnvelope::new(
            studio_id,
            6, // GAP! Expected 5
            now,
            ControlPlaneEvent::AgentRegistered {
                agent: AgentDescriptor::new(studio_id, "Agent D", AgentKind::Internal, None),
            },
        ),
    ];

    let gap_result = store.append_batch(&gap_batch);
    assert!(matches!(
        gap_result,
        Err(StoreError::SequenceGap {
            studio_id: s_id,
            expected: 5,
            actual: 6
        }) if s_id == studio_id
    ));
    // Verify rollback: latest sequence is still 3, sequence 4 was not persisted
    assert_eq!(store.latest_sequence(studio_id).unwrap(), 3);
    assert_eq!(store.events_for_studio(studio_id, 1).unwrap().len(), 3);

    // 3. Batch with sequence regression (sequence 2 instead of 4) -> must fail and roll back
    let regression_batch = vec![EventEnvelope::new(
        studio_id,
        2, // REGRESSION! Latest is 3
        now,
        ControlPlaneEvent::AgentRegistered {
            agent: AgentDescriptor::new(studio_id, "Agent E", AgentKind::Internal, None),
        },
    )];

    let reg_result = store.append_batch(&regression_batch);
    assert!(matches!(
        reg_result,
        Err(StoreError::SequenceRegression {
            studio_id: s_id,
            latest: 3,
            attempted: 2
        }) if s_id == studio_id
    ));
    assert_eq!(store.latest_sequence(studio_id).unwrap(), 3);

    // 4. Batch with internal duplicate EventId -> must fail and roll back
    let dup_id = EventId::new();
    let mut env1 = EventEnvelope::new(
        studio_id,
        4,
        now,
        ControlPlaneEvent::AgentRegistered {
            agent: AgentDescriptor::new(studio_id, "Agent F", AgentKind::Internal, None),
        },
    );
    env1.event_id = dup_id;

    let mut env2 = EventEnvelope::new(
        studio_id,
        5,
        now,
        ControlPlaneEvent::AgentRegistered {
            agent: AgentDescriptor::new(studio_id, "Agent G", AgentKind::Internal, None),
        },
    );
    env2.event_id = dup_id; // DUPLICATE ID!

    let dup_result = store.append_batch(&[env1, env2]);
    assert!(matches!(
        dup_result,
        Err(StoreError::DuplicateEventId { event_id }) if event_id == dup_id
    ));
    assert_eq!(store.latest_sequence(studio_id).unwrap(), 3);
}

#[tokio::test]
async fn test_worktree_startup_reconciliation_missing_dir() {
    let temp_dir = TempDir::new().expect("Failed to create tempdir");
    let db_path = temp_dir.path().join("worktree_state.db");

    let studio_id = StudioId::new();
    let task_id = TaskId::new();
    let worktree_id = WorktreeId::new();
    let missing_root = temp_dir.path().join("deleted_worktree_dir");

    // Seed SQLite with active worktree pointing to a non-existent directory
    {
        let mut store = SqliteEventStore::open(&db_path).expect("Failed to open SQLite store");
        let now = chrono::Utc::now();

        let mut task = TaskRecord::new(
            studio_id,
            "Task with Worktree",
            "Task holding missing worktree",
            None,
            None,
            vec![],
            now,
        );
        task.id = task_id;

        let mut wt = WorktreeRecord::new(
            studio_id,
            "recovery-worktree",
            temp_dir.path().join("repo"),
            missing_root.clone(),
            "base-commit-hash",
            now,
        );
        wt.id = worktree_id;
        wt.assigned_task_id = Some(task_id);

        let events = vec![
            EventEnvelope::new(
                studio_id,
                1,
                now,
                ControlPlaneEvent::StudioCreated {
                    studio: Studio::with_id(studio_id, "Worktree Studio", now),
                },
            ),
            EventEnvelope::new(studio_id, 2, now, ControlPlaneEvent::TaskCreated { task }),
            EventEnvelope::new(
                studio_id,
                3,
                now,
                ControlPlaneEvent::WorktreeCreated { worktree: wt },
            ),
            EventEnvelope::new(
                studio_id,
                4,
                now,
                ControlPlaneEvent::WorktreeStateChanged {
                    worktree_id,
                    previous_state: WorktreeState::Creating,
                    new_state: WorktreeState::Ready,
                },
            ),
        ];

        store
            .append_batch(&events)
            .expect("Failed to seed worktree events");
    }

    assert!(!missing_root.exists(), "Directory should not exist on disk");

    // Daemon restarts:
    let store = SqliteEventStore::open(&db_path).expect("Failed to open SQLite store");
    let mut cp = ControlPlane::restore_from_store(SystemClock, AnyEventStore::Sqlite(store))
        .expect("Failed to restore control plane");

    assert_eq!(
        cp.get_worktree(worktree_id).unwrap().state,
        WorktreeState::Ready
    );

    let (reconciled, missing) = cp
        .reconcile_missing_worktrees()
        .expect("Failed to reconcile worktrees");

    assert_eq!(reconciled, 0);
    assert_eq!(missing, 1);

    // Verify worktree transitioned to Failed
    let wt = cp.get_worktree(worktree_id).unwrap();
    assert_eq!(wt.state, WorktreeState::Failed);

    // Verify transition was committed to durable store
    let events = cp.all_events().expect("Failed to read all events");
    let failed_event = events.iter().find(|e| {
        if let ControlPlaneEvent::WorktreeStateChanged {
            worktree_id: w_id,
            new_state,
            ..
        } = &e.event
        {
            *w_id == worktree_id && *new_state == WorktreeState::Failed
        } else {
            false
        }
    });
    assert!(
        failed_event.is_some(),
        "WorktreeStateChanged to Failed must be committed"
    );
}
