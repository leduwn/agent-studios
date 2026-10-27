use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent_studios_claude_code_runtime::approval::{
    ApprovalAuthority, ApprovalDecision, CanUseToolRequest, parse_max_budget_usd,
    sanitize_diagnostic, validate_max_budget_usd,
};
use agent_studios_claude_code_runtime::sandbox::{
    validate_session_workspace, validate_workspace_path,
};
use agent_studios_external_runtime::WorkspaceAccessMode;
use agent_studios_protocol::id::WorktreeId;
use agent_studios_protocol::worktree::ExecutionWorkspace;

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
        bearer_sanitized.contains("[REDACTED_BEARER_TOKEN]") || bearer_sanitized.contains("[REDACTED_TOKEN]")
    );

    // 3. GitHub personal access tokens (ghp_...)
    let gh_canary = "Git clone failed with token ghp_ABCDEF1234567890abcdefghijklmnopqrstuv";
    let gh_sanitized = sanitize_diagnostic(gh_canary);
    assert!(
        !gh_sanitized.contains("ghp_ABCDEF1234567890abcdefghijklmnopqrstuv"),
        "Plaintext GitHub token must be redacted"
    );

    // 4. Basic Auth URI with embedded credentials (https://user:password@domain)
    let uri_canary = "Proxying via https://agent_studios:SuperSecretPassword123@api.anthropic.com/v1/messages";
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
    let escaping_res =
        validate_session_workspace(&escaping_managed, WorkspaceAccessMode::Mutating);
    assert!(
        escaping_res.is_err(),
        "Managed workspace with cwd outside root must be rejected"
    );
}
