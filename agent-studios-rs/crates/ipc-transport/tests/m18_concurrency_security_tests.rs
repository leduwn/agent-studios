//! Phase M18 Concurrency Race Safety, IPC Security Hardening, and Stream Backpressure Tests
//!
//! Validates:
//! 1. Concurrent approval resolution (`ResolveApproval`) and task cancellation (`CancelTask`) race safety.
//! 2. HMAC nonce replay rejection (`NonceTracker`), role privilege enforcement (`Shutdown`),
//!    and workspace path traversal (`..`) / unauthorized root rejection.
//! 3. High-throughput IPC event stream (2,000+ rapid `IpcEvent` frames) framing integrity,
//!    monotonic sequence continuity, and replay buffer backfill.

use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

use agent_studios_control_plane::{ControlPlane, InMemoryStore, SystemClock};
use agent_studios_ipc_protocol::{
    ApprovalDecision, ApprovalStatus, AuthBinding, HmacSigner, IpcEvent, ProtocolVersion,
    RecoveryStatusReport, SessionId,
};
use agent_studios_ipc_transport::{
    ClientConfig, DEFAULT_MAX_FRAME_SIZE, IpcClient, IpcCommand, IpcResponse, IpcServer,
    TransportError, recv_msg, send_msg,
};
use agent_studios_protocol::agent::AgentKind;
use agent_studios_protocol::approval::{ApprovalKind, ApprovalState};
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::TaskState;

const TEST_TIMEOUT: Duration = Duration::from_secs(15);

async fn connect_authenticated_duplex_client(
    server: &IpcServer,
    key_id: &str,
    secret: &[u8],
) -> IpcClient {
    let (client_stream, server_stream) = tokio::io::duplex(262_144);
    server.handle_connection(server_stream);
    let config = ClientConfig::new(key_id, secret.to_vec());
    IpcClient::connect_stream(client_stream, config)
        .await
        .expect("Failed to connect and authenticate duplex IPC client")
}

#[tokio::test]
async fn test_m18_concurrent_approval_and_cancellation_race_safety() {
    const RACE_ROUNDS: usize = 16;

    let admin_key = "m18-race-admin-key";
    let admin_secret = b"m18-race-admin-secret-998877".to_vec();

    let server = IpcServer::builder()
        .with_admin_auth_key(admin_key, admin_secret.clone())
        .with_replay_capacity(4096)
        .build()
        .expect("Failed to build IpcServer");

    let mut setup_client =
        connect_authenticated_duplex_client(&server, admin_key, &admin_secret).await;
    let studio = setup_client
        .create_studio("M18 Approval & Cancellation Race Studio")
        .await
        .expect("Failed to create studio");
    let studio_id = studio.id;

    let agent_id = {
        let cp_lock = server.control_plane();
        let mut cp = cp_lock.write().await;
        let agent = cp
            .register_agent(studio_id, "race-worker-agent", AgentKind::Internal, None)
            .expect("Failed to register agent");
        let envs = cp.drain_committed_events();
        drop(cp);
        server
            .state()
            .publish_control_plane_events(envs)
            .expect("Failed to publish agent events");
        agent.id
    };

    for round in 0..RACE_ROUNDS {
        // Seed an active Running task, Running run, and Pending approval in ControlPlane
        let (task_id, run_id, approval_id) = {
            let cp_lock = server.control_plane();
            let mut cp = cp_lock.write().await;

            let task = cp
                .create_task(
                    studio_id,
                    format!("Race Task Round {round}"),
                    "Concurrent approval resolution vs task cancellation target",
                    None,
                    Some(agent_id),
                    vec![],
                )
                .expect("Failed to create task");
            cp.transition_task_state(task.id, TaskState::Running)
                .expect("Failed to transition task to Running");

            let run = cp
                .create_run(task.id, agent_id)
                .expect("Failed to create run");
            cp.transition_run_state(run.id, RunState::Starting)
                .expect("Failed to transition run to Starting");
            cp.transition_run_state(run.id, RunState::Running)
                .expect("Failed to transition run to Running");

            let approval = cp
                .request_approval(
                    studio_id,
                    task.id,
                    agent_id,
                    ApprovalKind::CommandExecution,
                    format!("Approve tool execution round {round}"),
                )
                .expect("Failed to request approval");

            let envs = cp.drain_committed_events();
            drop(cp);
            server
                .state()
                .publish_control_plane_events(envs)
                .expect("Failed to publish round setup events");

            (task.id, run.id, approval.id)
        };

        // Spawn 4 independent authenticated IPC clients to race against the same task & approval
        let mut client_approve =
            connect_authenticated_duplex_client(&server, admin_key, &admin_secret).await;
        let mut client_reject =
            connect_authenticated_duplex_client(&server, admin_key, &admin_secret).await;
        let mut client_cancel_1 =
            connect_authenticated_duplex_client(&server, admin_key, &admin_secret).await;
        let mut client_cancel_2 =
            connect_authenticated_duplex_client(&server, admin_key, &admin_secret).await;

        let approval_id_str_1 = approval_id.to_string();
        let approval_id_str_2 = approval_id.to_string();

        let (approve_res, reject_res, cancel_1_res, cancel_2_res) = tokio::join!(
            async move {
                client_approve
                    .resolve_approval(
                        approval_id_str_1,
                        ApprovalDecision::Approved {
                            modified_details: None,
                            user_note: Some("concurrent approve".to_string()),
                        },
                    )
                    .await
            },
            async move {
                client_reject
                    .resolve_approval(
                        approval_id_str_2,
                        ApprovalDecision::Rejected {
                            reason: "concurrent reject".to_string(),
                        },
                    )
                    .await
            },
            async move {
                client_cancel_1
                    .cancel_task(studio_id, task_id, "concurrent cancel 1")
                    .await
            },
            async move {
                client_cancel_2
                    .cancel_task(studio_id, task_id, "concurrent cancel 2")
                    .await
            },
        );

        // Invariant 1: Approve and Reject cannot BOTH succeed on the same approval
        assert!(
            !(approve_res.is_ok() && reject_res.is_ok()),
            "Round {round}: conflicting Approve and Reject resolutions both succeeded!"
        );

        // Invariant 2: Cancel 1 and Cancel 2 cannot BOTH succeed on the same task
        assert!(
            !(cancel_1_res.is_ok() && cancel_2_res.is_ok()),
            "Round {round}: duplicate concurrent CancelTask calls both succeeded!"
        );

        // Invariant 3: At least one cancellation must succeed (since task was non-terminal Running)
        assert!(
            cancel_1_res.is_ok() || cancel_2_res.is_ok(),
            "Round {round}: expected one CancelTask to succeed, got {cancel_1_res:?} and {cancel_2_res:?}"
        );

        // Invariant 4: Verify deterministic terminal state in ControlPlane
        let cp_lock = server.control_plane();
        let cp = cp_lock.read().await;
        let final_task = cp.get_task(task_id).expect("Task must exist");
        let final_run = cp.get_run(run_id).expect("Run must exist");
        let final_approval = cp.get_approval(approval_id).expect("Approval must exist");

        assert_eq!(
            final_task.state,
            TaskState::Cancelled,
            "Round {round}: task must reach Cancelled terminal state"
        );
        assert_eq!(
            final_run.state,
            RunState::Cancelled,
            "Round {round}: run must reach Cancelled terminal state"
        );
        assert!(
            final_approval.state.is_terminal(),
            "Round {round}: approval must be in a terminal state, got {:?}",
            final_approval.state
        );

        if let Ok(status) = approve_res {
            assert_eq!(status, ApprovalStatus::Authorized);
            assert_eq!(final_approval.state, ApprovalState::Approved);
        } else if let Ok(status) = reject_res {
            assert_eq!(status, ApprovalStatus::Rejected);
            assert_eq!(final_approval.state, ApprovalState::Denied);
        } else {
            // Cancellation won the race before either approval resolution could commit
            assert_eq!(final_approval.state, ApprovalState::Cancelled);
        }
        drop(cp);

        // Invariant 5: Post-race late approval resolution must deterministically fail closed
        let late_res = setup_client
            .resolve_approval(
                approval_id.to_string(),
                ApprovalDecision::Approved {
                    modified_details: None,
                    user_note: Some("late resolution after race".to_string()),
                },
            )
            .await;
        assert!(
            late_res.is_err(),
            "Round {round}: late approval resolution on cancelled task must fail closed"
        );
    }

    // Verify full ControlPlane event stream replays cleanly with zero corruption
    let cp_lock = server.control_plane();
    let cp = cp_lock.read().await;
    let all_events = cp.all_events().expect("Failed to read all events");
    let replayed = ControlPlane::replay_events(&all_events, SystemClock, InMemoryStore::new())
        .expect("Event stream after concurrent race rounds must replay cleanly");
    assert_eq!(replayed.state(), cp.state());
}

#[tokio::test]
async fn test_m18_ipc_hmac_replay_role_privilege_and_path_traversal_hardening() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create authorized git workspace, unauthorized sibling git workspace, and non-git directory
    let authorized_repo = temp_dir.path().join("workspaces").join("authorized_repo");
    let unauthorized_repo = temp_dir.path().join("workspaces").join("unauthorized_repo");
    let non_git_dir = temp_dir.path().join("workspaces").join("plain_folder");

    std::fs::create_dir_all(authorized_repo.join(".git"))
        .expect("Failed to create authorized .git dir");
    std::fs::create_dir_all(unauthorized_repo.join(".git"))
        .expect("Failed to create unauthorized .git dir");
    std::fs::create_dir_all(&non_git_dir).expect("Failed to create non-git dir");

    let admin_key = "m18-sec-admin";
    let admin_secret = b"m18-sec-admin-secret-key-001".to_vec();
    let standard_key = "m18-sec-standard";
    let standard_secret = b"m18-sec-standard-secret-key-002".to_vec();

    let recovery_report = RecoveryStatusReport {
        last_recovery_at: Some(chrono::Utc::now()),
        interrupted_tasks_count: 1,
        interrupted_runs_count: 1,
        worktrees_reconciled: 2,
        worktrees_missing: 0,
        worktrees_foreign: 0,
        database_healthy: true,
        database_path: Some("/var/lib/agent-studios/sensitive-state.db".to_string()),
        total_persisted_events: 42,
    };

    let server = IpcServer::builder()
        .with_admin_auth_key(admin_key, admin_secret.clone())
        .with_auth_key(standard_key, standard_secret.clone())
        .with_authorized_workspace(&authorized_repo)
        .with_strict_workspace_roots(true)
        .with_recovery_status(recovery_report)
        .build()
        .expect("Failed to build IpcServer");

    // =========================================================================
    // 1. HMAC Nonce Replay Rejection over Live Duplex Transport & NonceTracker
    // =========================================================================
    let caps = vec![
        "worktree_isolation".to_string(),
        "approval_routing".to_string(),
    ];
    let replay_session_id = SessionId::generate();
    let replay_nonce = "m18-replay-nonce-0001";
    let now_ms = chrono::Utc::now().timestamp_millis();

    let binding = AuthBinding::new(
        standard_key,
        replay_session_id,
        replay_nonce,
        now_ms,
        b"",
        ProtocolVersion::CURRENT,
        Some(caps.clone()),
    );
    let signed_header =
        HmacSigner::sign(&standard_secret, &binding).expect("Failed to sign HMAC header");

    // Connection 1: legitimate authentication using signed_header succeeds
    {
        let (client_stream, server_stream) = tokio::io::duplex(65_536);
        server.handle_connection(server_stream);
        let (mut reader, mut writer) = tokio::io::split(client_stream);

        send_msg(
            &mut writer,
            &IpcCommand::NegotiateCapabilities {
                protocol_version: ProtocolVersion::CURRENT,
                capabilities: caps.clone(),
            },
        )
        .await
        .expect("Failed to send NegotiateCapabilities");

        let neg_resp: IpcResponse = recv_msg(&mut reader, DEFAULT_MAX_FRAME_SIZE)
            .await
            .expect("Failed to receive CapabilitiesNegotiated");
        assert!(matches!(
            neg_resp,
            IpcResponse::CapabilitiesNegotiated { .. }
        ));

        send_msg(
            &mut writer,
            &IpcCommand::Authenticate {
                header: signed_header.clone(),
                payload: None,
            },
        )
        .await
        .expect("Failed to send Authenticate");

        let auth_resp: IpcResponse = recv_msg(&mut reader, DEFAULT_MAX_FRAME_SIZE)
            .await
            .expect("Failed to receive Authenticated");
        assert!(
            matches!(auth_resp, IpcResponse::Authenticated { .. }),
            "First authentication with valid nonce must succeed, got {auth_resp:?}"
        );
    }

    // Connection 2: replay attack reusing identical signed_header must be rejected
    {
        let (client_stream, server_stream) = tokio::io::duplex(65_536);
        server.handle_connection(server_stream);
        let (mut reader, mut writer) = tokio::io::split(client_stream);

        send_msg(
            &mut writer,
            &IpcCommand::NegotiateCapabilities {
                protocol_version: ProtocolVersion::CURRENT,
                capabilities: caps.clone(),
            },
        )
        .await
        .expect("Failed to send NegotiateCapabilities on replay connection");

        let neg_resp: IpcResponse = recv_msg(&mut reader, DEFAULT_MAX_FRAME_SIZE)
            .await
            .expect("Failed to receive CapabilitiesNegotiated on replay connection");
        assert!(matches!(
            neg_resp,
            IpcResponse::CapabilitiesNegotiated { .. }
        ));

        send_msg(
            &mut writer,
            &IpcCommand::Authenticate {
                header: signed_header.clone(),
                payload: None,
            },
        )
        .await
        .expect("Failed to send replayed Authenticate");

        let replay_resp: IpcResponse = recv_msg(&mut reader, DEFAULT_MAX_FRAME_SIZE)
            .await
            .expect("Failed to receive response for replayed Authenticate");

        match replay_resp {
            IpcResponse::Error { message } => {
                assert!(
                    message.contains("Replay attack detected"),
                    "Expected replay detection error message, got: {message}"
                );
                assert!(
                    !message.contains(replay_nonce),
                    "Error message must not echo untrusted nonce string"
                );
            }
            other => panic!("Expected IpcResponse::Error on nonce replay, got {other:?}"),
        }
    }

    // Direct NonceTracker verification confirms duplicate rejection
    {
        let state_ref = server.state();
        let tracker = state_ref.nonce_tracker.lock().unwrap();
        let direct_replay = HmacSigner::verify(
            &standard_secret,
            &signed_header,
            b"",
            Some(&caps[..]),
            60_000,
            &tracker,
        );
        assert!(direct_replay.is_err());
    }

    // =========================================================================
    // 2. Role Privilege Enforcement: Standard Client Rejected on Admin Shutdown
    // =========================================================================
    let mut std_client =
        connect_authenticated_duplex_client(&server, standard_key, &standard_secret).await;

    // Standard client must receive redacted database_path in recovery status
    let std_recovery = std_client
        .get_recovery_status()
        .await
        .expect("Standard client get_recovery_status failed");
    assert_eq!(
        std_recovery.database_path, None,
        "Standard client must not see filesystem database_path"
    );

    // Standard client attempting Admin Shutdown must be rejected with PermissionDenied
    let shutdown_err = std_client
        .shutdown()
        .await
        .expect_err("Standard client must not be allowed to trigger daemon Shutdown");
    match shutdown_err {
        TransportError::PermissionDenied(msg) => {
            assert!(
                msg.contains("Permission denied: shutdown requires admin role"),
                "Unexpected permission denied message: {msg}"
            );
        }
        other => panic!("Expected TransportError::PermissionDenied, got {other:?}"),
    }
    assert!(
        !server.shutdown_token().is_cancelled(),
        "Server shutdown_token must NOT be cancelled after rejected standard client shutdown"
    );

    // Verify server and standard client remain operational after rejected shutdown
    let health = std_client
        .get_health()
        .await
        .expect("Server must remain healthy after rejected shutdown");
    assert_eq!(health.status, "healthy");

    // =========================================================================
    // 3. Workspace Authorization & Path Traversal (`..`) Hardening
    // =========================================================================
    let state = server.state();

    // Authorized workspace root succeeds
    let auth_canon = state
        .verify_workspace_authorization(&authorized_repo.to_string_lossy())
        .expect("Authorized git repository must pass verification");
    assert_eq!(
        auth_canon,
        dunce::canonicalize(&authorized_repo).expect("canonicalize authorized_repo")
    );

    // Path traversal (`..`) escaping from authorized_repo into sibling unauthorized_repo must fail
    let traversal_path = authorized_repo.join("..").join("unauthorized_repo");
    let traversal_err = state
        .verify_workspace_authorization(&traversal_path.to_string_lossy())
        .expect_err("Path traversal via '..' to unauthorized repo must be rejected");
    assert!(
        traversal_err.contains("not in the daemon authorized workspaces allowlist"),
        "Unexpected traversal rejection message: {traversal_err}"
    );

    // Deep relative path traversal (`sub/../../unauthorized_repo`) must fail
    let sub_dir = authorized_repo.join("sub");
    std::fs::create_dir_all(&sub_dir).expect("Failed to create subdirectory");
    let deep_traversal = sub_dir.join("..").join("..").join("unauthorized_repo");
    let deep_traversal_err = state
        .verify_workspace_authorization(&deep_traversal.to_string_lossy())
        .expect_err("Deep path traversal via '../..' must be rejected");
    assert!(
        deep_traversal_err.contains("not in the daemon authorized workspaces allowlist"),
        "Unexpected deep traversal rejection message: {deep_traversal_err}"
    );

    // Direct access to unauthorized git repository must fail
    let direct_unauth_err = state
        .verify_workspace_authorization(&unauthorized_repo.to_string_lossy())
        .expect_err("Direct unauthorized repository access must be rejected");
    assert!(
        direct_unauth_err.contains("not in the daemon authorized workspaces allowlist"),
        "Unexpected unauthorized repo rejection message: {direct_unauth_err}"
    );

    // Directory without `.git` must fail git repository identity check
    let non_git_err = state
        .verify_workspace_authorization(&non_git_dir.to_string_lossy())
        .expect_err("Directory without .git must be rejected");
    assert!(
        non_git_err.contains("not a valid Git repository (.git not found)"),
        "Unexpected non-git rejection message: {non_git_err}"
    );

    // Strict workspace root policy: nested git repository inside authorized_repo must be rejected
    let nested_git_repo = authorized_repo.join("nested_subrepo");
    std::fs::create_dir_all(nested_git_repo.join(".git"))
        .expect("Failed to create nested git repo");
    let nested_strict_err = state
        .verify_workspace_authorization(&nested_git_repo.to_string_lossy())
        .expect_err("Strict workspace root policy must reject nested sub-repositories");
    assert!(
        nested_strict_err.contains("not in the daemon authorized workspaces allowlist"),
        "Unexpected strict root rejection message: {nested_strict_err}"
    );

    // Finally, verify an Admin client CAN see unredacted recovery status and authorize Shutdown
    let mut admin_client =
        connect_authenticated_duplex_client(&server, admin_key, &admin_secret).await;
    let admin_recovery = admin_client
        .get_recovery_status()
        .await
        .expect("Admin get_recovery_status failed");
    assert_eq!(
        admin_recovery.database_path.as_deref(),
        Some("/var/lib/agent-studios/sensitive-state.db")
    );

    admin_client
        .shutdown()
        .await
        .expect("Admin client must be authorized to shut down server");
    assert!(server.shutdown_token().is_cancelled());
}

#[tokio::test]
async fn test_m18_high_throughput_ipc_event_stream_backpressure() {
    const TOTAL_EVENTS: u64 = 2_500;
    const PRE_BACKFILL_EVENTS: u64 = 500;

    let key_id = "m18-stream-key";
    let secret = b"m18-stream-secret-556677".to_vec();

    // Configure replay buffer capacity large enough to satisfy burst backpressure & late backfill
    let server = IpcServer::builder()
        .with_auth_key(key_id, secret.clone())
        .with_replay_capacity(4096)
        .build()
        .expect("Failed to build IpcServer");

    // Client 1 subscribes before any events are published (live high-throughput stream with backpressure recovery)
    let mut live_client = connect_authenticated_duplex_client(&server, key_id, &secret).await;
    let mut live_stream = live_client
        .subscribe_events(1)
        .await
        .expect("Live client subscribe_events(1) failed");

    // Publish first 500 events
    let state = server.state();
    for seq in 1..=PRE_BACKFILL_EVENTS {
        let ev = state
            .emit_event(
                "assistant_message_chunk",
                serde_json::json!({
                    "seq": seq,
                    "phase": "pre_backfill",
                    "chunk": format!("high-throughput-frame-payload-{seq}"),
                }),
            )
            .expect("Failed to emit pre-backfill event");
        assert_eq!(ev.sequence, seq);
    }

    // Client 2 connects after 500 events and subscribes from sequence 1 (tests replay buffer backfill + live transition)
    let mut backfill_client = connect_authenticated_duplex_client(&server, key_id, &secret).await;
    let mut backfill_stream = backfill_client
        .subscribe_events(1)
        .await
        .expect("Backfill client subscribe_events(1) failed");

    // Rapidly publish remaining 2,000 events (exceeds broadcast channel capacity 1024 to exercise lag recovery)
    let publisher_state = state.clone();
    let publisher_handle = tokio::spawn(async move {
        for seq in (PRE_BACKFILL_EVENTS + 1)..=TOTAL_EVENTS {
            let ev = IpcEvent::new(
                seq,
                "assistant_message_chunk",
                serde_json::json!({
                    "seq": seq,
                    "phase": "rapid_burst",
                    "chunk": format!("high-throughput-frame-payload-{seq}"),
                }),
            );
            publisher_state
                .publish_event(ev)
                .expect("Failed to publish burst event");
            if seq % 250 == 0 {
                tokio::task::yield_now().await;
            }
        }
    });

    // Concurrently consume all 2,500 events on both live_stream and backfill_stream
    let consume_live = timeout(TEST_TIMEOUT, async move {
        for expected_seq in 1..=TOTAL_EVENTS {
            let ev = live_stream
                .next_event()
                .await
                .unwrap_or_else(|e| panic!("Live stream failed at seq {expected_seq}: {e:?}"));
            assert_eq!(
                ev.sequence, expected_seq,
                "Live stream monotonic sequence mismatch"
            );
            assert_eq!(ev.event_type, "assistant_message_chunk");
            assert_eq!(ev.payload["seq"], serde_json::json!(expected_seq));
            assert_eq!(
                ev.payload["chunk"],
                serde_json::json!(format!("high-throughput-frame-payload-{expected_seq}"))
            );
        }
        assert_eq!(live_stream.last_sequence(), Some(TOTAL_EVENTS));
    });

    let consume_backfill = timeout(TEST_TIMEOUT, async move {
        for expected_seq in 1..=TOTAL_EVENTS {
            let ev = backfill_stream
                .next_event()
                .await
                .unwrap_or_else(|e| panic!("Backfill stream failed at seq {expected_seq}: {e:?}"));
            assert_eq!(
                ev.sequence, expected_seq,
                "Backfill stream monotonic sequence mismatch"
            );
            assert_eq!(ev.event_type, "assistant_message_chunk");
            assert_eq!(ev.payload["seq"], serde_json::json!(expected_seq));
        }
        assert_eq!(backfill_stream.last_sequence(), Some(TOTAL_EVENTS));
    });

    let (pub_res, live_res, backfill_res) =
        tokio::join!(publisher_handle, consume_live, consume_backfill);
    pub_res.expect("Publisher task panicked");
    live_res.expect("Live stream timed out receiving 2,500 events");
    backfill_res.expect("Backfill stream timed out receiving 2,500 events");

    // Client 3 subscribes purely for historical tail replay from sequence 2,001..=2,500
    let mut tail_client = connect_authenticated_duplex_client(&server, key_id, &secret).await;
    let mut tail_stream = tail_client
        .subscribe_events(2_001)
        .await
        .expect("Tail client subscribe_events(2001) failed");

    timeout(TEST_TIMEOUT, async move {
        for expected_seq in 2_001..=TOTAL_EVENTS {
            let ev = tail_stream
                .next_event()
                .await
                .expect("Tail stream failed to backfill event");
            assert_eq!(ev.sequence, expected_seq);
            assert_eq!(ev.payload["seq"], serde_json::json!(expected_seq));
        }
        assert_eq!(tail_stream.last_sequence(), Some(TOTAL_EVENTS));
    })
    .await
    .expect("Tail replay stream timed out");
}
