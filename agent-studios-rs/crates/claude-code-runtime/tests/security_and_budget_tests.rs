use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent_studios_claude_code_runtime::approval::{
    ApprovalAuthority, ApprovalDecision, CanUseToolRequest, parse_max_budget_usd,
    sanitize_diagnostic, validate_max_budget_usd,
};
use agent_studios_claude_code_runtime::discovery::{
    CLAUDE_CODE_RUNTIME_ID, ClaudeCodeDiscoveryConfig,
};
use agent_studios_claude_code_runtime::protocol::{
    CanUseToolRequest as ProtocolCanUseToolRequest, ControlDecision, ControlRequest,
    ControlRequestPayload, ControlResponseBody,
};
use agent_studios_claude_code_runtime::runtime::ClaudeCodeRuntime;
use agent_studios_claude_code_runtime::sandbox::{
    validate_session_workspace, validate_workspace_path,
};
use agent_studios_external_runtime::{
    AgentRuntime, NonSecretValue, RuntimeConfigRef, RuntimeError, RuntimeImplementationId,
    RuntimeInstanceId, RuntimeInstanceRef, RuntimeStartRequest, WorkspaceAccessMode,
};
use agent_studios_protocol::id::WorktreeId;
use agent_studios_protocol::worktree::ExecutionWorkspace;
use tempfile::tempdir;

// ============================================================================
// 1. Tool Approval Denial Fail-Closed Behavior on Timeout & Cancellation
// ============================================================================

#[tokio::test]
async fn test_tool_approval_denial_fail_closed_on_timeout() {
    let authority = ApprovalAuthority::new();
    let req = CanUseToolRequest::new(
        "req_timeout_test_01",
        "Bash",
        serde_json::json!({"command": "rm -rf /"}),
    );

    assert_eq!(authority.pending_count(), 0);

    // Request approval with 35ms timeout without resolving it
    let decision = authority
        .request_approval(req, Duration::from_millis(35))
        .await
        .expect("request_approval should return Ok(Deny) on timeout");

    assert_eq!(decision, ApprovalDecision::Deny);
    assert!(decision.is_denied());
    assert!(!decision.is_allowed());
    assert_eq!(decision.behavior(), "deny");

    // Must be cleaned up from pending map after timeout
    assert_eq!(authority.pending_count(), 0);
    assert!(!authority.is_pending("req_timeout_test_01"));
}

#[tokio::test]
async fn test_tool_approval_denial_fail_closed_on_cancellation() {
    let authority = Arc::new(ApprovalAuthority::new());
    let req = CanUseToolRequest::new(
        "req_cancel_test_01",
        "Bash",
        serde_json::json!({"command": "drop database"}),
    );

    let auth_clone = authority.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(15)).await;
        auth_clone.cancel_all_pending();
    });

    let decision = authority
        .request_approval(req, Duration::from_millis(500))
        .await
        .expect("request_approval should return Ok(Deny) on cancellation");

    assert_eq!(decision, ApprovalDecision::Deny);
    assert!(decision.is_denied());
}

#[tokio::test]
async fn test_tool_approval_resolve_allow_and_frame_formatting() {
    let authority = Arc::new(ApprovalAuthority::new());
    let req = CanUseToolRequest::new(
        "req_allow_test_01",
        "Bash",
        serde_json::json!({"command": "cargo test"}),
    )
    .with_tool_use_id("toolu_bash_01")
    .with_decision_reason("Read-only test command");

    assert_eq!(req.tool_name, "Bash");
    assert_eq!(req.tool_use_id, Some("toolu_bash_01".into()));

    let auth_clone = authority.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        auth_clone
            .resolve_approval("req_allow_test_01", ApprovalDecision::Allow)
            .expect("resolve_approval should succeed");
    });

    let decision = authority
        .request_approval(req, Duration::from_millis(500))
        .await
        .expect("request_approval should resolve to Allow");

    assert_eq!(decision, ApprovalDecision::Allow);
    assert!(decision.is_allowed());
    assert!(!decision.is_denied());
    assert_eq!(decision.behavior(), "allow");

    // Verify Claude Code stdin control_response NDJSON structure
    let val = decision.to_control_response_value("req_allow_test_01");
    assert_eq!(val["type"], "control_response");
    assert_eq!(val["response"]["subtype"], "success");
    assert_eq!(val["response"]["request_id"], "req_allow_test_01");
    assert_eq!(val["response"]["response"]["behavior"], "allow");

    let line = decision.to_control_response("req_allow_test_01");
    assert!(!line.contains('\n'));

    let frame = decision.to_control_response_frame("req_allow_test_01");
    assert!(frame.ends_with('\n'));

    // Test deny formatting as well
    let deny_val = ApprovalDecision::Deny.to_control_response_value("req_deny_01");
    assert_eq!(deny_val["response"]["response"]["behavior"], "deny");
}

#[test]
fn test_tool_approval_unknown_request_resolution_fails() {
    let authority = ApprovalAuthority::new();
    let res = authority.resolve_approval("non_existent_req_id", ApprovalDecision::Allow);
    assert!(res.is_err(), "Resolving unprompted request must fail");
}

// ============================================================================
// 2. Budget Ceiling `--max-budget-usd` Parsing & Validation
// ============================================================================

#[test]
fn test_budget_ceiling_parsing_and_validation() {
    // Valid non-negative finite amounts
    assert_eq!(validate_max_budget_usd(0.0).unwrap(), 0.0);
    assert_eq!(validate_max_budget_usd(1.0).unwrap(), 1.0);
    assert_eq!(validate_max_budget_usd(25.50).unwrap(), 25.50);
    assert_eq!(validate_max_budget_usd(500.0).unwrap(), 500.0);

    assert_eq!(parse_max_budget_usd("0").unwrap(), 0.0);
    assert_eq!(parse_max_budget_usd("10").unwrap(), 10.0);
    assert_eq!(parse_max_budget_usd(" 42.75 ").unwrap(), 42.75);

    // Reject negative numbers
    assert!(
        validate_max_budget_usd(-0.01).is_err(),
        "Negative budget amounts must be rejected"
    );
    assert!(
        validate_max_budget_usd(-100.0).is_err(),
        "Negative budget amounts must be rejected"
    );
    assert!(
        parse_max_budget_usd("-5.00").is_err(),
        "Negative budget string must be rejected"
    );

    // Reject NaN
    assert!(
        validate_max_budget_usd(f64::NAN).is_err(),
        "NaN budget amounts must be rejected"
    );
    assert!(
        parse_max_budget_usd("NaN").is_err(),
        "NaN budget string must be rejected"
    );
    assert!(
        parse_max_budget_usd("nan").is_err(),
        "nan budget string must be rejected"
    );

    // Reject Infinite values
    assert!(
        validate_max_budget_usd(f64::INFINITY).is_err(),
        "Infinity budget amounts must be rejected"
    );
    assert!(
        validate_max_budget_usd(f64::NEG_INFINITY).is_err(),
        "-Infinity budget amounts must be rejected"
    );
    assert!(
        parse_max_budget_usd("inf").is_err(),
        "inf budget string must be rejected"
    );
    assert!(
        parse_max_budget_usd("-infinity").is_err(),
        "-infinity budget string must be rejected"
    );

    // Reject non-numeric and empty inputs
    assert!(parse_max_budget_usd("").is_err());
    assert!(parse_max_budget_usd("   ").is_err());
    assert!(parse_max_budget_usd("unlimited").is_err());
    assert!(parse_max_budget_usd("$10.00").is_err());
}

// ============================================================================
// 3. Sensitive Canary Scrubbing in Diagnostics & Debug Outputs
// ============================================================================

#[test]
fn test_sensitive_canary_scrubbing_in_diagnostics() {
    // 1. Anthropic API Keys (sk-ant-...)
    let ant_canary = "Error sending message to Anthropic: sk-ant-api03-abcdef1234567890qwertyuiop";
    let ant_sanitized = sanitize_diagnostic(ant_canary);
    assert!(
        !ant_sanitized.contains("sk-ant-api03-abcdef1234567890qwertyuiop"),
        "Plaintext Anthropic API key must be redacted"
    );
    assert!(ant_sanitized.contains("[REDACTED_API_KEY]"));

    // 2. Authorization Bearer Headers
    let bearer_canary = "HTTP request failed: Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.sig";
    let bearer_sanitized = sanitize_diagnostic(bearer_canary);
    assert!(
        !bearer_sanitized.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.sig"),
        "Plaintext Bearer token must be redacted"
    );
    assert!(
        bearer_sanitized.contains("[REDACTED_BEARER_TOKEN]")
            || bearer_sanitized.contains("[REDACTED_TOKEN]")
    );

    // 3. GitHub personal access tokens (ghp_...)
    let gh_canary = "Git clone failed with token ghp_ABCDEF1234567890abcdefghijklmnopqrstuv";
    let gh_sanitized = sanitize_diagnostic(gh_canary);
    assert!(
        !gh_sanitized.contains("ghp_ABCDEF1234567890abcdefghijklmnopqrstuv"),
        "Plaintext GitHub token must be redacted"
    );

    // 4. Basic Auth URI with embedded credentials (https://user:password@domain)
    let uri_canary =
        "Proxying via https://agent_studios:SuperSecretPassword123@api.anthropic.com/v1/messages";
    let uri_sanitized = sanitize_diagnostic(uri_canary);
    assert!(
        !uri_sanitized.contains("SuperSecretPassword123"),
        "Plaintext password in URI must be redacted"
    );
    assert!(uri_sanitized.contains("[REDACTED_PASSWORD]"));

    // 5. Query parameter credentials (api_key=... and token=...)
    let query_canary = "GET /v1/models?api_key=secret_api_key_val_9999&token=access_token_val_8888";
    let query_sanitized = sanitize_diagnostic(query_canary);
    assert!(!query_sanitized.contains("secret_api_key_val_9999"));
    assert!(!query_sanitized.contains("access_token_val_8888"));
}

// ============================================================================
// 4. Workspace Boundary Validation & Rejection of Workspace Escapes
// ============================================================================

#[test]
fn test_workspace_boundary_validation_rejection_of_escapes() {
    let temp = tempfile::tempdir().expect("Failed to create temporary directory");
    let temp_root = temp.path().to_path_buf();

    // Create a subfolder inside root
    let subfolder = temp_root.join("project_dir");
    std::fs::create_dir_all(&subfolder).unwrap();

    // 1. Valid workspace path within root
    let valid_res = validate_workspace_path(&temp_root, &subfolder);
    assert!(
        valid_res.is_ok(),
        "Path within root should be valid: {:?}",
        valid_res.err()
    );

    // 2. Path escaping root via parent directory traversal (..)
    let escape_target = temp_root.join("project_dir").join("..").join("..");
    let escape_res = validate_workspace_path(&subfolder, &escape_target);
    assert!(
        escape_res.is_err(),
        "Path escaping subfolder root must fail closed"
    );

    // 3. ExecutionWorkspace::SharedSource must be strictly rejected
    let shared_workspace = ExecutionWorkspace::shared_source(temp_root.clone());
    let shared_res = validate_session_workspace(&shared_workspace, WorkspaceAccessMode::Mutating);
    assert!(
        shared_res.is_err(),
        "SharedSource workspaces must be rejected to protect root repository"
    );

    // 4. ExecutionWorkspace::Managed valid workspace
    let managed_workspace = ExecutionWorkspace::managed(
        WorktreeId::new(),
        temp_root.clone(),
        temp_root.clone(),
        PathBuf::from("/source"),
        PathBuf::from("/source"),
        "0123456789abcdef0123456789abcdef01234567",
    );
    let managed_res = validate_session_workspace(&managed_workspace, WorkspaceAccessMode::Mutating);
    assert!(
        managed_res.is_ok(),
        "Managed workspace within valid root must succeed"
    );

    // 5. ExecutionWorkspace::Managed with non-existent root path
    let non_existent_managed = ExecutionWorkspace::managed(
        WorktreeId::new(),
        PathBuf::from("C:\\definitely\\does\\not\\exist\\path_123456"),
        PathBuf::from("C:\\definitely\\does\\not\\exist\\path_123456"),
        PathBuf::from("/source"),
        PathBuf::from("/source"),
        "0123456789abcdef0123456789abcdef01234567",
    );
    let non_existent_res =
        validate_session_workspace(&non_existent_managed, WorkspaceAccessMode::Mutating);
    assert!(
        non_existent_res.is_err(),
        "Non-existent worktree path must fail closed"
    );

    // 6. ExecutionWorkspace::Managed with cwd outside root
    let outside_temp = tempfile::tempdir().expect("Failed to create outside directory");
    let escaping_managed = ExecutionWorkspace::managed(
        WorktreeId::new(),
        temp_root.clone(),
        outside_temp.path().to_path_buf(),
        PathBuf::from("/source"),
        PathBuf::from("/source"),
        "0123456789abcdef0123456789abcdef01234567",
    );
    let escaping_res = validate_session_workspace(&escaping_managed, WorkspaceAccessMode::Mutating);
    assert!(
        escaping_res.is_err(),
        "Managed workspace with cwd outside root must be rejected"
    );
}

// ============================================================================
// 5. M12-REV-003: Budget Validation Bypass Regression Tests
// ============================================================================

fn test_managed_workspace() -> (tempfile::TempDir, ExecutionWorkspace) {
    let temp = tempdir().unwrap();
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

#[tokio::test]
async fn test_start_rejects_malformed_max_budget_usd_without_launching_process() {
    let runtime = ClaudeCodeRuntime::new().unwrap();
    let instance_ref = RuntimeInstanceRef::new(
        RuntimeImplementationId::new(CLAUDE_CODE_RUNTIME_ID).unwrap(),
        RuntimeInstanceId::from_uuid(uuid::Uuid::new_v4()),
    );
    let (_temp_ws, ws) = test_managed_workspace();

    let malformed_budgets = vec![
        "invalid",
        "-5.0",
        "-0.01",
        "NaN",
        "nan",
        "inf",
        "-infinity",
        "",
        "   ",
        "unlimited",
        "$10.00",
    ];

    for bad_budget in malformed_budgets {
        let mut req = RuntimeStartRequest::new(instance_ref.clone(), ws.clone());
        req.metadata.insert(
            "max_budget_usd".to_string(),
            NonSecretValue::new(bad_budget).unwrap(),
        );

        let res = runtime.start(req).await;
        assert!(
            res.is_err(),
            "start() must reject malformed budget ceiling '{bad_budget}'"
        );

        match res.err().unwrap() {
            RuntimeError::InvalidConfiguration { reason } => {
                assert!(
                    reason.as_str().contains("Invalid max_budget_usd"),
                    "Expected 'Invalid max_budget_usd' error for '{bad_budget}', got: {}",
                    reason.as_str()
                );
            }
            other => panic!("Expected InvalidConfiguration for '{bad_budget}', got: {other:?}"),
        }
    }

    // Invariant: Zero sessions created or active
    assert_eq!(runtime.active_session_count().await, 0);
}

#[tokio::test]
async fn test_start_accepts_valid_budget_and_passes_to_config() {
    let temp = tempdir().unwrap();
    let dummy_bin = temp.path().join("fake_claude.exe");
    std::fs::write(&dummy_bin, b"fake binary").unwrap();

    let config = ClaudeCodeDiscoveryConfig {
        custom_binary_path: Some(dummy_bin),
    };
    let runtime = ClaudeCodeRuntime::new()
        .unwrap()
        .with_discovery_config(config);

    let instance_ref = RuntimeInstanceRef::new(
        RuntimeImplementationId::new(CLAUDE_CODE_RUNTIME_ID).unwrap(),
        RuntimeInstanceId::from_uuid(uuid::Uuid::new_v4()),
    );
    let (_temp_ws, ws) = test_managed_workspace();

    let mut req = RuntimeStartRequest::new(instance_ref, ws);
    req.metadata.insert(
        "max_budget_usd".to_string(),
        NonSecretValue::new("10.50").unwrap(),
    );

    // Verify build_process_config parses and accepts valid budget ceiling
    let process_config = runtime
        .build_process_config(&req)
        .expect("build_process_config must succeed for valid budget");

    assert_eq!(
        process_config.max_budget_usd,
        Some(10.50),
        "Budget ceiling '10.50' must be passed to process configuration"
    );

    // Verify start() passes budget validation (it reaches spawn_claude_process rather than failing budget check)
    let start_res = runtime.start(req).await;
    // On Windows, dummy binary cannot be executed as Win32 binary, returning ProcessSpawn / StartupFailed,
    // NEVER InvalidConfiguration.
    if let Err(RuntimeError::InvalidConfiguration { reason }) = start_res {
        panic!("start() must not fail with InvalidConfiguration for valid budget: {reason}");
    }
}

#[test]
fn test_config_ref_budget_validation_and_rejection() {
    let temp = tempdir().unwrap();
    let dummy_bin = temp.path().join("fake_claude.exe");
    std::fs::write(&dummy_bin, b"fake binary").unwrap();

    let config = ClaudeCodeDiscoveryConfig {
        custom_binary_path: Some(dummy_bin),
    };
    let runtime = ClaudeCodeRuntime::new()
        .unwrap()
        .with_discovery_config(config);

    let (_temp_ws, ws) = test_managed_workspace();

    // 1. Valid budget in JSON config_ref
    let valid_inst = RuntimeInstanceRef::new(
        RuntimeImplementationId::new(CLAUDE_CODE_RUNTIME_ID).unwrap(),
        RuntimeInstanceId::from_uuid(uuid::Uuid::new_v4()),
    )
    .with_config_ref(RuntimeConfigRef::new(r#"{"max_budget_usd": 25.50}"#).unwrap());

    let req_valid = RuntimeStartRequest::new(valid_inst, ws.clone());
    let cfg = runtime
        .build_process_config(&req_valid)
        .expect("Valid JSON config_ref budget must succeed");
    assert_eq!(cfg.max_budget_usd, Some(25.50));

    // 2. Valid budget in prefix config_ref
    let prefix_inst = RuntimeInstanceRef::new(
        RuntimeImplementationId::new(CLAUDE_CODE_RUNTIME_ID).unwrap(),
        RuntimeInstanceId::from_uuid(uuid::Uuid::new_v4()),
    )
    .with_config_ref(RuntimeConfigRef::new("max_budget_usd=15.00").unwrap());

    let req_prefix = RuntimeStartRequest::new(prefix_inst, ws.clone());
    let cfg_prefix = runtime
        .build_process_config(&req_prefix)
        .expect("Valid prefix config_ref budget must succeed");
    assert_eq!(cfg_prefix.max_budget_usd, Some(15.00));

    // 3. Malformed negative budget in config_ref must fail closed
    let bad_inst = RuntimeInstanceRef::new(
        RuntimeImplementationId::new(CLAUDE_CODE_RUNTIME_ID).unwrap(),
        RuntimeInstanceId::from_uuid(uuid::Uuid::new_v4()),
    )
    .with_config_ref(RuntimeConfigRef::new(r#"{"max_budget_usd": -10.0}"#).unwrap());

    let req_bad = RuntimeStartRequest::new(bad_inst, ws.clone());
    let bad_res = runtime.build_process_config(&req_bad);
    assert!(
        bad_res.is_err(),
        "Negative budget in config_ref must fail closed"
    );
    match bad_res.err().unwrap() {
        RuntimeError::InvalidConfiguration { reason } => {
            assert!(reason.as_str().contains("Invalid max_budget_usd"));
        }
        other => panic!("Expected InvalidConfiguration, got: {other:?}"),
    }

    // 4. Malformed NaN budget in config_ref must fail closed
    let nan_inst = RuntimeInstanceRef::new(
        RuntimeImplementationId::new(CLAUDE_CODE_RUNTIME_ID).unwrap(),
        RuntimeInstanceId::from_uuid(uuid::Uuid::new_v4()),
    )
    .with_config_ref(RuntimeConfigRef::new(r#"{"max_budget_usd": "NaN"}"#).unwrap());

    let req_nan = RuntimeStartRequest::new(nan_inst, ws);
    let nan_res = runtime.build_process_config(&req_nan);
    assert!(
        nan_res.is_err(),
        "NaN budget in config_ref must fail closed"
    );
}

// ============================================================================
// 6. M12-REV-004: Approval Timeout Enforcement & Watchdog Tests
// ============================================================================

#[tokio::test]
async fn test_approval_timeout_watchdog_fires_and_produces_deny_response() {
    let authority = Arc::new(ApprovalAuthority::new());
    let control_req = ControlRequest {
        request_id: "req_timeout_watchdog_01".to_string(),
        request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({"command": "rm -rf /"}),
            tool_use_id: Some("call_timeout_01".to_string()),
            agent_id: None,
            decision_reason: Some("Dangerous tool execution".to_string()),
            blocked_path: None,
            default_to_no: Some(false),
            suggestions: None,
        }),
    };

    let approval_id = authority
        .register_request(&control_req)
        .expect("should register approval request");

    assert_eq!(authority.pending_count(), 1);
    assert!(authority.is_pending("req_timeout_watchdog_01"));

    let (timeout_tx, timeout_rx) = tokio::sync::oneshot::channel();
    // Spawn watchdog with short 35ms timeout
    authority.spawn_watchdog(
        approval_id,
        Duration::from_millis(35),
        move |deny_resp| async move {
            let _ = timeout_tx.send(deny_resp);
        },
    );

    assert!(authority.is_watchdog_active(&approval_id));

    // Await watchdog firing
    let deny_resp = tokio::time::timeout(Duration::from_millis(500), timeout_rx)
        .await
        .expect("watchdog must fire within 500ms")
        .expect("channel should receive deny response");

    // Verify formatted NDJSON structure of deny response
    assert_eq!(deny_resp.request_id(), "req_timeout_watchdog_01");
    assert_eq!(deny_resp.r#type, "control_response");
    match &deny_resp.response {
        ControlResponseBody::Success {
            request_id,
            response,
        } => {
            assert_eq!(request_id, "req_timeout_watchdog_01");
            match response {
                ControlDecision::Deny { message, interrupt } => {
                    assert_eq!(*interrupt, Some(false));
                    let msg = message.as_ref().expect("deny response must have message");
                    assert!(
                        msg.contains("Approval timed out"),
                        "Message must state approval timed out: '{msg}'"
                    );
                    assert!(
                        msg.contains("fail-closed"),
                        "Message must emphasize fail-closed: '{msg}'"
                    );
                }
                _ => panic!("Expected ControlDecision::Deny, got {response:?}"),
            }
        }
        _ => panic!("Expected Success subtype with Deny decision"),
    }

    // Verify approval is removed from pending authority
    assert_eq!(authority.pending_count(), 0);
    assert!(!authority.is_pending("req_timeout_watchdog_01"));
    assert!(!authority.is_watchdog_active(&approval_id));

    // Attempting to resolve by host afterwards must fail fail-closed
    let late_res = authority.resolve_approval_by_id(approval_id, true, None);
    assert!(
        late_res.is_err(),
        "Late resolution after timeout must fail closed"
    );
}

#[tokio::test]
async fn test_approval_resolved_before_timeout_cancels_watchdog() {
    let authority = Arc::new(ApprovalAuthority::new());
    let control_req = ControlRequest {
        request_id: "req_resolve_cancel_01".to_string(),
        request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({"command": "cargo check"}),
            tool_use_id: Some("call_resolve_01".to_string()),
            agent_id: None,
            decision_reason: Some("Safe command".to_string()),
            blocked_path: None,
            default_to_no: Some(false),
            suggestions: None,
        }),
    };

    let approval_id = authority
        .register_request(&control_req)
        .expect("should register approval request");

    let (timeout_tx, mut timeout_rx) = tokio::sync::oneshot::channel();
    // Watchdog with 250ms timeout
    let watchdog_handle = authority.spawn_watchdog(
        approval_id,
        Duration::from_millis(250),
        move |deny_resp| async move {
            let _ = timeout_tx.send(deny_resp);
        },
    );

    assert!(authority.is_watchdog_active(&approval_id));

    // Host resolves approval before timeout (after 20ms)
    tokio::time::sleep(Duration::from_millis(20)).await;
    let allow_resp = authority
        .resolve_approval_by_id(approval_id, true, None)
        .expect("resolve_approval_by_id should succeed");

    assert_eq!(allow_resp.request_id(), "req_resolve_cancel_01");

    // Watchdog must be removed from cancellations immediately
    assert!(!authority.is_watchdog_active(&approval_id));

    // Wait for watchdog task to finish
    let _ = watchdog_handle.await;

    // Verify on_timeout callback was NEVER called
    assert!(
        timeout_rx.try_recv().is_err(),
        "Timeout callback must not be invoked after host resolution"
    );

    // Authority is clean
    assert_eq!(authority.pending_count(), 0);
    assert!(!authority.is_pending("req_resolve_cancel_01"));
}

#[tokio::test]
async fn test_approval_watchdog_cancelled_on_session_termination() {
    let authority = Arc::new(ApprovalAuthority::new());
    let control_req = ControlRequest {
        request_id: "req_cancel_termination_01".to_string(),
        request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({"command": "cargo test"}),
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
        .expect("should register approval request");

    let (timeout_tx, mut timeout_rx) = tokio::sync::oneshot::channel();
    let watchdog_handle = authority.spawn_watchdog(
        approval_id,
        Duration::from_millis(300),
        move |deny_resp| async move {
            let _ = timeout_tx.send(deny_resp);
        },
    );

    tokio::time::sleep(Duration::from_millis(20)).await;
    // Session termination cancels all pending approvals
    authority.cancel_all_pending();

    let _ = watchdog_handle.await;
    assert!(
        timeout_rx.try_recv().is_err(),
        "Timeout callback must not fire after session termination cancel"
    );
    assert_eq!(authority.pending_count(), 0);
    assert!(!authority.is_watchdog_active(&approval_id));
}
