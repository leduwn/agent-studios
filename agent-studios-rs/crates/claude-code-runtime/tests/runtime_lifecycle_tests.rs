use std::path::PathBuf;

use agent_studios_claude_code_runtime::approval::ApprovalAuthority;
use agent_studios_claude_code_runtime::discovery::{
    CLAUDE_CODE_RUNTIME_ID, ClaudeCodeDiscoveryConfig,
};
use agent_studios_claude_code_runtime::protocol::{
    CanUseToolRequest as ProtocolCanUseToolRequest, ControlRequest, ControlRequestPayload,
};
use agent_studios_claude_code_runtime::runtime::ClaudeCodeRuntime;

use agent_studios_external_runtime::{
    AgentRuntime, DEFAULT_EVENT_RETENTION, RuntimeCapability,
    RuntimeError, RuntimeEventKind, RuntimeImplementationId, RuntimeInstanceId,
    RuntimeInstanceRef, RuntimeLifecycleState, RuntimeSessionId, RuntimeStartRequest,
    SessionEventHub, WorkspaceAccessMode,
};
use agent_studios_protocol::id::WorktreeId;
use agent_studios_protocol::worktree::ExecutionWorkspace;
use tempfile::tempdir;

fn test_workspace() -> (tempfile::TempDir, ExecutionWorkspace) {
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

// ============================================================================
// 1. Adapter Instantiation, Capabilities & Discovery
// ============================================================================

#[tokio::test]
async fn test_runtime_instantiation_and_capabilities() {
    let runtime = ClaudeCodeRuntime::new().expect("ClaudeCodeRuntime::new() should succeed");
    assert_eq!(runtime.implementation_id().as_str(), "claude-code");
    assert_eq!(runtime.active_session_count().await, 0);

    let caps = runtime.capabilities().await;
    assert!(caps.supports(RuntimeCapability::StreamingEvents));
    assert!(caps.supports(RuntimeCapability::Approvals));
    assert!(caps.supports(RuntimeCapability::Interrupt));
    assert!(caps.supports(RuntimeCapability::Resume));
    assert!(caps.supports(RuntimeCapability::PersistentSession));
}

#[tokio::test]
async fn test_runtime_discovery_with_explicit_config() {
    let temp = tempdir().unwrap();
    let dummy_bin = temp.path().join("fake_claude.exe");
    std::fs::write(&dummy_bin, b"fake binary").unwrap();

    let config = ClaudeCodeDiscoveryConfig {
        custom_binary_path: Some(dummy_bin.clone()),
    };

    let runtime = ClaudeCodeRuntime::new()
        .unwrap()
        .with_discovery_config(config);

    let discovered = runtime.discover().await.expect("discover should succeed");
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].implementation_id.as_str(), "claude-code");
    let expected_canonical = std::fs::canonicalize(&dummy_bin).unwrap_or(dummy_bin);
    assert_eq!(discovered[0].binary_path.as_ref(), Some(&expected_canonical));
}

// ============================================================================
// 2. Pre-Spawn Start Request Validation (Fail-Closed)
// ============================================================================

#[tokio::test]
async fn test_runtime_start_validation_rejects_shared_source_workspace() {
    let runtime = ClaudeCodeRuntime::new().unwrap();
    let instance_ref = RuntimeInstanceRef::new(
        RuntimeImplementationId::new(CLAUDE_CODE_RUNTIME_ID).unwrap(),
        RuntimeInstanceId::from_uuid(uuid::Uuid::new_v4()),
    );

    let shared_workspace = ExecutionWorkspace::shared_source(PathBuf::from("/shared/repo"));
    let req = RuntimeStartRequest::new(instance_ref, shared_workspace)
        .with_workspace_access_mode(WorkspaceAccessMode::Mutating);

    let result = runtime.start(req).await;
    assert!(result.is_err(), "Must reject SharedSource workspace");
    match result.err().unwrap() {
        RuntimeError::InvalidWorkspaceAccess { reason } => {
            assert!(reason.as_str().contains("SharedSource is strictly forbidden"));
        }
        other => panic!("Expected InvalidWorkspaceAccess, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_runtime_start_validation_rejects_implementation_mismatch() {
    let runtime = ClaudeCodeRuntime::new().unwrap();
    let wrong_impl_ref = RuntimeInstanceRef::new(
        RuntimeImplementationId::new("opencode").unwrap(),
        RuntimeInstanceId::from_uuid(uuid::Uuid::new_v4()),
    );

    let (_temp, ws) = test_workspace();
    let req = RuntimeStartRequest::new(wrong_impl_ref, ws);

    let result = runtime.start(req).await;
    assert!(result.is_err(), "Must reject implementation mismatch");
    match result.err().unwrap() {
        RuntimeError::InvalidConfiguration { reason } => {
            assert!(reason.as_str().contains("Implementation mismatch"));
        }
        other => panic!("Expected InvalidConfiguration, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_runtime_start_validation_rejects_missing_binary() {
    let config = ClaudeCodeDiscoveryConfig {
        custom_binary_path: Some(PathBuf::from("/nonexistent/path/to/claude")),
    };

    let runtime = ClaudeCodeRuntime::new()
        .unwrap()
        .with_discovery_config(config);

    let instance_ref = RuntimeInstanceRef::new(
        RuntimeImplementationId::new(CLAUDE_CODE_RUNTIME_ID).unwrap(),
        RuntimeInstanceId::from_uuid(uuid::Uuid::new_v4()),
    );

    let (_temp, ws) = test_workspace();
    let req = RuntimeStartRequest::new(instance_ref, ws);

    let result = runtime.start(req).await;
    assert!(result.is_err(), "Must fail when binary is missing");
    match result.err().unwrap() {
        RuntimeError::DiscoveryFailed { reason } => {
            assert!(reason.as_str().contains("binary was not found"));
        }
        other => panic!("Expected DiscoveryFailed, got: {other:?}"),
    }
}

// ============================================================================
// 3. State Machine Invariants & Lifecycle Transitions
// ============================================================================

#[test]
fn test_runtime_lifecycle_state_machine_transitions() {
    use RuntimeLifecycleState::*;

    // Active states
    assert!(Starting.is_active());
    assert!(Running.is_active());
    assert!(Interrupted.is_active());

    // Terminal states
    assert!(Stopped.is_terminal());
    assert!(Completed.is_terminal());
    assert!(Failed.is_terminal());

    // Legal transitions
    assert!(Starting.can_transition_to(Running));
    assert!(Starting.can_transition_to(Stopped));
    assert!(Starting.can_transition_to(Failed));

    assert!(Running.can_transition_to(Interrupted));
    assert!(Running.can_transition_to(Completed));
    assert!(Running.can_transition_to(Stopped));
    assert!(Running.can_transition_to(Failed));

    assert!(Interrupted.can_transition_to(Running)); // Resume
    assert!(Interrupted.can_transition_to(Stopped));
    assert!(Interrupted.can_transition_to(Failed));

    // Illegal transitions (terminal cannot transition)
    assert!(!Stopped.can_transition_to(Running));
    assert!(!Completed.can_transition_to(Running));
    assert!(!Failed.can_transition_to(Running));

    // Validation returns typed error on illegal transition
    assert!(Stopped.validate_transition_to(Running).is_err());
    assert!(Completed.validate_transition_to(Running).is_err());
}

// ============================================================================
// 4. Session Event Hub Monotonic Sequencing & Bounded Replay
// ============================================================================

#[tokio::test]
async fn test_session_event_hub_monotonic_ordering_and_replay() {
    let session_id = RuntimeSessionId::new();
    let instance_id = RuntimeInstanceId::from_uuid(uuid::Uuid::new_v4());
    let hub = SessionEventHub::new(session_id, instance_id, DEFAULT_EVENT_RETENTION);

    // Invariant: First event must be SessionStarted at sequence 1
    let ev1 = hub
        .emit(RuntimeEventKind::SessionStarted {
            session_id,
            instance_id,
        })
        .await
        .expect("sequence 1 must succeed");
    assert_eq!(ev1.sequence, 1);

    // Sequence 2: OutputDelta
    let ev2 = hub
        .emit(RuntimeEventKind::OutputDelta {
            text: "Hello from Claude Code".to_string(),
        })
        .await
        .expect("sequence 2 must succeed");
    assert_eq!(ev2.sequence, 2);

    // Sequence 3: ToolStarted
    let ev3 = hub
        .emit(RuntimeEventKind::ToolStarted {
            tool_name: "Bash".to_string(),
            tool_call_id: "toolu_01".to_string(),
        })
        .await
        .expect("sequence 3 must succeed");
    assert_eq!(ev3.sequence, 3);

    // Replay from beginning (after_sequence = None)
    let mut sub_all = hub.subscribe(None).await.expect("subscribe should succeed");
    let r1 = sub_all.next_event().await.unwrap().unwrap();
    assert_eq!(r1.sequence, 1);
    let r2 = sub_all.next_event().await.unwrap().unwrap();
    assert_eq!(r2.sequence, 2);
    let r3 = sub_all.next_event().await.unwrap().unwrap();
    assert_eq!(r3.sequence, 3);

    // Replay offset (after_sequence = Some(2))
    let mut sub_offset = hub
        .subscribe(Some(2))
        .await
        .expect("subscribe after 2 should succeed");
    let ro1 = sub_offset.next_event().await.unwrap().unwrap();
    assert_eq!(ro1.sequence, 3);

    // Future sequence beyond latest committed returns InvalidReplayOffset
    let err = hub.subscribe(Some(99)).await;
    assert!(err.is_err());
    assert!(matches!(err.err().unwrap(), RuntimeError::InvalidReplayOffset { .. }));
}

#[tokio::test]
async fn test_session_event_hub_rejects_duplicate_session_started() {
    let session_id = RuntimeSessionId::new();
    let instance_id = RuntimeInstanceId::from_uuid(uuid::Uuid::new_v4());
    let hub = SessionEventHub::new(session_id, instance_id, DEFAULT_EVENT_RETENTION);

    hub.emit(RuntimeEventKind::SessionStarted {
        session_id,
        instance_id,
    })
    .await
    .unwrap();

    // Emitting SessionStarted again at sequence 2 must be rejected
    let res = hub
        .emit(RuntimeEventKind::SessionStarted {
            session_id,
            instance_id,
        })
        .await;
    assert!(res.is_err(), "Duplicate SessionStarted must be rejected");
}

// ============================================================================
// 5. Host Delegated Tool Approvals & Fail-Closed Invariants
// ============================================================================

#[test]
fn test_approval_authority_resolve_by_approval_id() {
    let authority = ApprovalAuthority::new();

    let control_req = ControlRequest {
        request_id: "req_test_auth_01".to_string(),
        request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({"command": "cargo build"}),
            tool_use_id: Some("call_01".to_string()),
            agent_id: None,
            decision_reason: Some("Compiling code".to_string()),
            blocked_path: None,
            default_to_no: Some(false),
            suggestions: None,
        }),
    };

    let approval_id = authority
        .register_request(&control_req)
        .expect("should register approval request");

    assert_eq!(authority.pending_count(), 1);
    assert!(authority.is_pending("req_test_auth_01"));

    // Resolve with allow
    let response = authority
        .resolve_approval_by_id(approval_id, true, None)
        .expect("resolve_approval_by_id should succeed");

    assert_eq!(response.request_id, "req_test_auth_01");
    assert_eq!(response.r#type, "control_response");

    // Cleaned up
    assert_eq!(authority.pending_count(), 0);
    assert!(!authority.is_pending("req_test_auth_01"));
}

#[test]
fn test_approval_authority_cancel_all_pending_fails_closed() {
    let authority = ApprovalAuthority::new();

    let req1 = ControlRequest {
        request_id: "req_01".to_string(),
        request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({"command": "echo 1"}),
            tool_use_id: None,
            agent_id: None,
            decision_reason: None,
            blocked_path: None,
            default_to_no: None,
            suggestions: None,
        }),
    };

    let req2 = ControlRequest {
        request_id: "req_02".to_string(),
        request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
            tool_name: "Write".to_string(),
            input: serde_json::json!({"file_path": "/tmp/test"}),
            tool_use_id: None,
            agent_id: None,
            decision_reason: None,
            blocked_path: None,
            default_to_no: None,
            suggestions: None,
        }),
    };

    let id1 = authority.register_request(&req1).unwrap();
    let id2 = authority.register_request(&req2).unwrap();

    assert_eq!(authority.pending_count(), 2);

    authority.cancel_all_pending();

    assert_eq!(authority.pending_count(), 0);
    assert!(authority.resolve_approval_by_id(id1, true, None).is_err());
    assert!(authority.resolve_approval_by_id(id2, true, None).is_err());
}
