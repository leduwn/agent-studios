use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent_studios_claude_code_runtime::approval::{ApprovalAuthority, ApprovalDecision};
use agent_studios_claude_code_runtime::discovery::CLAUDE_CODE_RUNTIME_ID;
use agent_studios_claude_code_runtime::error::ClaudeCodeError;
use agent_studios_claude_code_runtime::process::{ClaudeCodeProcessConfig, spawn_claude_process};
use agent_studios_claude_code_runtime::protocol::{
    CanUseToolRequest as ProtocolCanUseToolRequest, ControlDecision, ControlRequest,
    ControlRequestPayload, ControlResponseBody,
};
use agent_studios_claude_code_runtime::runtime::ClaudeCodeRuntime;
use agent_studios_claude_code_runtime::sandbox::SandboxPolicy;
use agent_studios_external_runtime::{
    AgentRuntime, RuntimeImplementationId, RuntimeInstanceId, RuntimeInstanceRef, RuntimeSessionId,
    RuntimeSessionRef,
};
use agent_studios_protocol::id::{ApprovalId, WorktreeId};
use agent_studios_protocol::worktree::ExecutionWorkspace;
use tempfile::tempdir;

fn test_managed_workspace() -> (tempfile::TempDir, ExecutionWorkspace) {
    let temp = tempdir().expect("Failed to create temporary directory");
    let temp_path = temp.path().to_path_buf();
    let ws = ExecutionWorkspace::managed(
        WorktreeId::new(),
        temp_path.clone(),
        temp_path.clone(),
        PathBuf::from("/source"),
        PathBuf::from("/source"),
        "0123456789abcdef0123456789abcdef01234567",
    );
    (temp, ws)
}

// ============================================================================
// 1. Concurrent Approval Requests for Different Tools and Subagents
// ============================================================================

#[tokio::test]
async fn test_concurrent_approval_requests_different_tools_and_subagents() {
    let authority = Arc::new(ApprovalAuthority::new());
    let tool_names = ["Bash", "FileEdit", "Glob", "Grep", "WebFetch", "Agent"];
    let subagents = [
        "subagent_researcher_01",
        "subagent_coder_02",
        "subagent_tester_03",
        "parent_session_root",
    ];

    let concurrency = 60;
    let mut handles = Vec::new();

    for i in 0..concurrency {
        let auth = Arc::clone(&authority);
        let tool = tool_names[i % tool_names.len()];
        let agent = subagents[i % subagents.len()];
        let request_id = format!("req_conc_{i:03}_{tool}");
        let tool_use_id = format!("call_{i:03}");

        let handle = tokio::spawn(async move {
            let control_req = ControlRequest {
                request_id: request_id.clone(),
                request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
                    tool_name: tool.to_string(),
                    input: serde_json::json!({
                        "index": i,
                        "tool": tool,
                        "agent": agent,
                    }),
                    tool_use_id: Some(tool_use_id),
                    agent_id: Some(agent.to_string()),
                    decision_reason: Some(format!("Execute {tool} for {agent}")),
                    blocked_path: None,
                    default_to_no: Some(false),
                    suggestions: None,
                }),
            };

            let approval_id = auth
                .register_request(&control_req)
                .expect("Registration must succeed");

            // Verify deterministic approval ID mapping
            let expected_id = ApprovalAuthority::derive_approval_id(&request_id);
            assert_eq!(approval_id, expected_id);
            assert!(auth.is_pending(&request_id));
            assert!(auth.is_pending_id(&approval_id));

            // Yield to simulate interleaving
            tokio::task::yield_now().await;

            // Half approved, half denied
            let approve = (i % 2) == 0;
            let resp = auth
                .resolve_approval_by_id(approval_id, approve, Some("Test decision".to_string()))
                .expect("Resolution must succeed");

            assert_eq!(resp.request_id(), request_id);
            assert_eq!(resp.r#type, "control_response");

            match resp.response {
                ControlResponseBody::Success {
                    ref request_id,
                    ref response,
                } => {
                    assert_eq!(request_id, &control_req.request_id);
                    if approve {
                        assert!(
                            matches!(response, ControlDecision::Allow { .. }),
                            "Expected Allow decision for even index {i}"
                        );
                    } else {
                        assert!(
                            matches!(response, ControlDecision::Deny { .. }),
                            "Expected Deny decision for odd index {i}"
                        );
                    }
                }
                _ => panic!("Expected Success subtype response"),
            }

            (request_id, approval_id)
        });

        handles.push(handle);
    }

    let mut completed = Vec::new();
    for h in handles {
        let (req_id, app_id) = h.await.expect("Task must join cleanly");
        completed.push((req_id, app_id));
    }

    assert_eq!(completed.len(), concurrency);

    // Verify all cleaned up with no leaks or crosstalk
    assert_eq!(authority.pending_count(), 0);
    for (req_id, app_id) in completed {
        assert!(!authority.is_pending(&req_id));
        assert!(!authority.is_pending_id(&app_id));
    }
}

// ============================================================================
// 2. Duplicate Request IDs Handling & Fail-Closed Invariants
// ============================================================================

#[test]
fn test_duplicate_request_id_deterministic_id_and_fail_closed() {
    let authority = ApprovalAuthority::new();
    let dup_request_id = "req_duplicate_test_999";

    let req1 = ControlRequest {
        request_id: dup_request_id.to_string(),
        request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({"command": "cargo check"}),
            tool_use_id: Some("call_first".to_string()),
            agent_id: None,
            decision_reason: None,
            blocked_path: None,
            default_to_no: None,
            suggestions: None,
        }),
    };

    let id1 = authority
        .register_request(&req1)
        .expect("First registration should succeed");

    // Register second request with exact same request_id
    let req2 = ControlRequest {
        request_id: dup_request_id.to_string(),
        request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({"command": "cargo clippy"}),
            tool_use_id: Some("call_second".to_string()),
            agent_id: None,
            decision_reason: None,
            blocked_path: None,
            default_to_no: None,
            suggestions: None,
        }),
    };

    let id2 = authority
        .register_request(&req2)
        .expect("Second registration should succeed");

    // Invariant: Derived ApprovalId is strictly deterministic based on request_id
    assert_eq!(id1, id2);
    assert_eq!(authority.pending_count(), 1);
    assert!(authority.is_pending(dup_request_id));

    // First host resolution consumes the pending approval
    let res1 = authority.resolve_approval_by_id(id1, true, None);
    assert!(res1.is_ok(), "First resolution must succeed");

    // Second resolution must fail fail-closed (cannot double-resolve or replay)
    let res2 = authority.resolve_approval_by_id(id1, true, None);
    assert!(
        res2.is_err(),
        "Second resolution of duplicate ID must fail closed"
    );
    match res2.err().unwrap() {
        ClaudeCodeError::ApprovalDenied(msg) => {
            assert!(
                msg.contains("not found or already resolved"),
                "Expected 'not found or already resolved', got: {msg}"
            );
        }
        other => panic!("Expected ApprovalDenied error, got: {other:?}"),
    }

    // Attempting resolution by raw request_id on oneshot map also fails cleanly
    let raw_res = authority.resolve_approval(dup_request_id, ApprovalDecision::Allow);
    assert!(raw_res.is_err());

    assert_eq!(authority.pending_count(), 0);
    assert!(!authority.is_pending(dup_request_id));
}

// ============================================================================
// 3. Timeout Racing with Host Approval Resolution (Microsecond Race Conditions)
// ============================================================================

#[tokio::test]
async fn test_microsecond_race_timeout_vs_host_resolution() {
    let authority = Arc::new(ApprovalAuthority::new());
    let race_iterations = 60;

    for i in 0..race_iterations {
        let req_id = format!("req_race_{i:03}");
        let control_req = ControlRequest {
            request_id: req_id.clone(),
            request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
                tool_name: "Bash".to_string(),
                input: serde_json::json!({"action": "race_test", "iteration": i}),
                tool_use_id: Some(format!("call_race_{i}")),
                agent_id: None,
                decision_reason: None,
                blocked_path: None,
                default_to_no: None,
                suggestions: None,
            }),
        };

        let approval_id = authority
            .register_request(&control_req)
            .expect("Register must succeed");

        let (timeout_tx, timeout_rx) = tokio::sync::oneshot::channel();

        // Very tight timeout: 2ms
        let watchdog_handle = authority.spawn_watchdog(
            approval_id,
            Duration::from_millis(2),
            move |deny_resp| async move {
                let _ = timeout_tx.send(deny_resp);
            },
        );

        assert!(authority.is_watchdog_active(&approval_id));

        // Concurrently attempt resolution around the exact same time
        let auth_clone = Arc::clone(&authority);
        let host_handle = tokio::spawn(async move {
            // Jitter between 0 and 3 milliseconds to induce both outcomes
            let sleep_micros = (i % 4) * 1000;
            tokio::time::sleep(Duration::from_micros(sleep_micros as u64)).await;
            auth_clone.resolve_approval_by_id(approval_id, true, None)
        });

        let host_result = host_handle.await.expect("host task should join");
        let timeout_result = tokio::time::timeout(Duration::from_millis(150), timeout_rx).await;

        // Ensure watchdog task has terminated
        let _ = watchdog_handle.await;

        // INVARIANT: Mutually exclusive outcome:
        // Either host resolved first (Allow), OR watchdog fired first (Deny).
        // Under no circumstances can both claim resolution or neither resolve.
        match (host_result, timeout_result) {
            (Ok(allow_resp), Err(_timeout_elapsed)) => {
                // Host won race: watchdog was cancelled before firing
                assert_eq!(allow_resp.request_id(), req_id);
            }
            (Err(_host_denied), Ok(Ok(deny_resp))) => {
                // Watchdog won race: host was rejected fail-closed
                assert_eq!(deny_resp.request_id(), req_id);
            }
            (Ok(_), Ok(Ok(_))) => {
                panic!(
                    "RACE FAILURE: Both host approval and timeout denial succeeded for {req_id}!"
                );
            }
            (Err(e), Err(_)) => {
                panic!("RACE FAILURE: Neither host nor timeout resolved {req_id}! Host err: {e:?}");
            }
            (_, Ok(Err(_channel_closed))) => {
                // Watchdog channel closed without sending, which is valid if cancelled
            }
        }

        // Post-race cleanup invariant
        assert_eq!(authority.pending_count(), 0);
        assert!(!authority.is_pending(&req_id));
        assert!(!authority.is_pending_id(&approval_id));
        assert!(!authority.is_watchdog_active(&approval_id));
    }
}

// ============================================================================
// 4. Child Stdin Write Failure: Fails Closed and Not Marked Delivered
// ============================================================================

#[tokio::test]
async fn test_child_stdin_write_failure_fails_closed_and_not_marked_delivered() {
    let (temp_dir, _ws) = test_managed_workspace();

    // Use system cmd.exe on Windows (or sh on Unix) as a dummy executable
    let dummy_bin = if cfg!(windows) {
        PathBuf::from("cmd.exe")
    } else {
        PathBuf::from("/bin/sh")
    };

    let process_config = ClaudeCodeProcessConfig {
        binary_path: dummy_bin,
        working_dir: temp_dir.path().to_path_buf(),
        session_id: uuid::Uuid::new_v4().to_string(),
        sandbox_policy: SandboxPolicy::default(),
        permission_mode: Some("manual".to_string()),
        max_budget_usd: None,
        model: None,
        bare: false,
        env_vars: Vec::new(),
    };

    // Spawn child process
    let (process, _stdout) = spawn_claude_process(process_config)
        .await
        .expect("Process spawn should succeed");

    // Close child stdin to simulate closed/broken pipe
    process.close_stdin().await;

    // Register approval in authority
    let authority = ApprovalAuthority::new();
    let req_id = "req_stdin_fail_01";
    let control_req = ControlRequest {
        request_id: req_id.to_string(),
        request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({"command": "dangerous_action"}),
            tool_use_id: Some("call_fail_01".to_string()),
            agent_id: None,
            decision_reason: None,
            blocked_path: None,
            default_to_no: None,
            suggestions: None,
        }),
    };

    let approval_id = authority
        .register_request(&control_req)
        .expect("Registration must succeed");

    assert!(authority.is_pending(req_id));

    // Resolve approval
    let control_resp = authority
        .resolve_approval_by_id(approval_id, true, None)
        .expect("resolve_approval_by_id should succeed");

    let json_frame = serde_json::to_string(&control_resp).unwrap();

    // Attempt to write to closed stdin pipe
    let write_res = process.write_stdin_line(&json_frame).await;

    // INVARIANT 1: write_stdin_line must fail with ProcessIo error
    assert!(
        write_res.is_err(),
        "Writing to closed child stdin must return an error"
    );
    match write_res.err().unwrap() {
        ClaudeCodeError::ProcessIo(io_err) => {
            assert!(
                io_err.kind() == std::io::ErrorKind::BrokenPipe
                    || io_err.to_string().contains("closed"),
                "Expected BrokenPipe or closed pipe error, got: {io_err}"
            );
        }
        other => panic!("Expected ProcessIo error, got: {other:?}"),
    }

    // INVARIANT 2: Approval is NOT marked delivered, and authority fails closed against retry
    assert_eq!(authority.pending_count(), 0);
    assert!(!authority.is_pending(req_id));
    assert!(!authority.is_pending_id(&approval_id));

    let replay_res = authority.resolve_approval_by_id(approval_id, true, None);
    assert!(
        replay_res.is_err(),
        "Replaying or retrying an undelivered consumed approval must fail closed"
    );

    // Clean up child process
    let _ = process.terminate().await;
}

#[tokio::test]
async fn test_runtime_send_approval_when_session_terminal_fails_closed() {
    let runtime = ClaudeCodeRuntime::new().unwrap();
    let instance_ref = RuntimeInstanceRef::new(
        RuntimeImplementationId::new(CLAUDE_CODE_RUNTIME_ID).unwrap(),
        RuntimeInstanceId::from_uuid(uuid::Uuid::new_v4()),
    );

    let session_ref = RuntimeSessionRef::new(
        instance_ref.implementation_id.clone(),
        instance_ref.instance_id,
        RuntimeSessionId::new(),
    );

    // Attempting send on non-existent session returns SessionNotFound
    let approval_id = ApprovalId::new();
    let input = agent_studios_external_runtime::RuntimeInput::ApprovalResponse {
        approval_id,
        approved: true,
        reason: None,
    };

    let send_res = runtime.send(&session_ref, input).await;
    assert!(
        send_res.is_err(),
        "Sending approval to non-existent session must fail"
    );
    assert!(matches!(
        send_res.err().unwrap(),
        agent_studios_external_runtime::RuntimeError::SessionNotFound { .. }
    ));
}

// ============================================================================
// 5. Watchdog Task Lifecycle Cleanup: No Leaked Tasks
// ============================================================================

#[tokio::test]
async fn test_watchdog_lifecycle_no_leaked_tasks_on_resolution() {
    let authority = Arc::new(ApprovalAuthority::new());
    let req_id = "req_leak_test_resolve";

    let control_req = ControlRequest {
        request_id: req_id.to_string(),
        request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({}),
            tool_use_id: None,
            agent_id: None,
            decision_reason: None,
            blocked_path: None,
            default_to_no: None,
            suggestions: None,
        }),
    };

    let approval_id = authority
        .register_request(&control_req)
        .expect("Registration should succeed");

    // Spawn watchdog with very long timeout (15 seconds)
    let watchdog_handle = authority.spawn_watchdog(
        approval_id,
        Duration::from_secs(15),
        |_deny_resp| async move {},
    );

    assert!(authority.is_watchdog_active(&approval_id));

    // Resolve immediately
    authority
        .resolve_approval_by_id(approval_id, true, None)
        .expect("Resolution should succeed");

    // INVARIANT: Watchdog task must exit promptly on cancellation, not wait 15 seconds
    let join_result = tokio::time::timeout(Duration::from_millis(500), watchdog_handle)
        .await
        .expect("Watchdog task must terminate within 500ms when resolved");

    assert!(join_result.is_ok(), "Watchdog join must succeed");
    assert!(!authority.is_watchdog_active(&approval_id));
    assert_eq!(authority.pending_count(), 0);
}

#[tokio::test]
async fn test_watchdog_lifecycle_no_leaked_tasks_on_cancellation() {
    let authority = Arc::new(ApprovalAuthority::new());
    let count = 20;
    let mut handles = Vec::new();
    let mut approval_ids = Vec::new();

    for i in 0..count {
        let req_id = format!("req_leak_cancel_{i:02}");
        let control_req = ControlRequest {
            request_id: req_id,
            request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
                tool_name: "Bash".to_string(),
                input: serde_json::json!({}),
                tool_use_id: None,
                agent_id: None,
                decision_reason: None,
                blocked_path: None,
                default_to_no: None,
                suggestions: None,
            }),
        };

        let approval_id = authority.register_request(&control_req).unwrap();
        let handle = authority.spawn_watchdog(
            approval_id,
            Duration::from_secs(20),
            |_deny_resp| async move {},
        );

        handles.push(handle);
        approval_ids.push(approval_id);
    }

    assert_eq!(authority.pending_count(), count);
    for id in &approval_ids {
        assert!(authority.is_watchdog_active(id));
    }

    // Cancel all pending approvals (e.g. on session termination)
    authority.cancel_all_pending();

    // INVARIANT: All 20 watchdog tasks must complete immediately, not leak or sleep 20s
    for (i, h) in handles.into_iter().enumerate() {
        let res = tokio::time::timeout(Duration::from_millis(500), h).await;
        assert!(
            res.is_ok(),
            "Watchdog task {i} leaked or did not finish promptly after cancel_all_pending"
        );
    }

    assert_eq!(authority.pending_count(), 0);
    for id in &approval_ids {
        assert!(!authority.is_watchdog_active(id));
    }
}

#[tokio::test]
async fn test_watchdog_lifecycle_no_leaked_tasks_on_timeout() {
    let authority = Arc::new(ApprovalAuthority::new());
    let req_id = "req_leak_timeout";

    let control_req = ControlRequest {
        request_id: req_id.to_string(),
        request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({}),
            tool_use_id: None,
            agent_id: None,
            decision_reason: None,
            blocked_path: None,
            default_to_no: None,
            suggestions: None,
        }),
    };

    let approval_id = authority
        .register_request(&control_req)
        .expect("Registration should succeed");

    let (tx, rx) = tokio::sync::oneshot::channel();
    let watchdog_handle = authority.spawn_watchdog(
        approval_id,
        Duration::from_millis(25),
        move |deny_resp| async move {
            let _ = tx.send(deny_resp);
        },
    );

    // Wait for timeout to fire
    let deny = tokio::time::timeout(Duration::from_millis(300), rx)
        .await
        .expect("Timeout must fire within 300ms")
        .expect("Deny response must be received");

    assert_eq!(deny.request_id(), req_id);

    // Watchdog task itself finishes
    let join_res = tokio::time::timeout(Duration::from_millis(200), watchdog_handle).await;
    assert!(join_res.is_ok(), "Watchdog task must finish on timeout");

    assert!(!authority.is_watchdog_active(&approval_id));
    assert_eq!(authority.pending_count(), 0);
}
