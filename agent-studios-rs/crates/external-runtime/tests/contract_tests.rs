use std::path::PathBuf;
use std::sync::Arc;

use agent_studios_external_runtime::*;
use agent_studios_protocol::agent::AgentExecutionBudget;
use agent_studios_protocol::id::{AgentId, ApprovalId, RunId, StudioId, TaskId, WorktreeId};
use agent_studios_protocol::worktree::ExecutionWorkspace;
use agent_studios_provider::{ModelId, ModelRef, ProviderInstanceId};
use chrono::Utc;
use uuid::Uuid;

// ============================================================================
// Matrix A: Lifecycle State Machine Matrix
// ============================================================================

#[tokio::test]
async fn test_matrix_a_lifecycle_normal_completion() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));
    let req = RuntimeStartRequest::new(
        RuntimeInstanceId::generate(),
        runtime.implementation_id().clone(),
        ws,
    );

    let handle = runtime.start(req).await.unwrap();
    assert_eq!(
        runtime.status(&handle.session_id).await.unwrap(),
        RuntimeLifecycleState::Running
    );

    runtime
        .transition_session(&handle.session_id, RuntimeLifecycleState::Completed)
        .await
        .unwrap();
    assert_eq!(
        runtime.status(&handle.session_id).await.unwrap(),
        RuntimeLifecycleState::Completed
    );
    assert!(
        runtime
            .status(&handle.session_id)
            .await
            .unwrap()
            .is_terminal()
    );
}

#[tokio::test]
async fn test_matrix_a_lifecycle_interrupt_resume_completed() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));
    let req = RuntimeStartRequest::new(
        RuntimeInstanceId::generate(),
        runtime.implementation_id().clone(),
        ws,
    );

    let handle = runtime.start(req).await.unwrap();
    assert_eq!(
        runtime.status(&handle.session_id).await.unwrap(),
        RuntimeLifecycleState::Running
    );

    runtime.interrupt(&handle.session_id).await.unwrap();
    assert_eq!(
        runtime.status(&handle.session_id).await.unwrap(),
        RuntimeLifecycleState::Interrupted
    );

    runtime.resume(&handle.session_id, None).await.unwrap();
    assert_eq!(
        runtime.status(&handle.session_id).await.unwrap(),
        RuntimeLifecycleState::Running
    );

    runtime
        .transition_session(&handle.session_id, RuntimeLifecycleState::Completed)
        .await
        .unwrap();
    assert_eq!(
        runtime.status(&handle.session_id).await.unwrap(),
        RuntimeLifecycleState::Completed
    );
}

#[tokio::test]
async fn test_matrix_a_lifecycle_stopped_and_failed_terminals() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

    // Test Stopped terminal
    let req1 = RuntimeStartRequest::new(
        RuntimeInstanceId::generate(),
        runtime.implementation_id().clone(),
        ws.clone(),
    );
    let handle1 = runtime.start(req1).await.unwrap();
    runtime.stop(&handle1.session_id).await.unwrap();
    assert_eq!(
        runtime.status(&handle1.session_id).await.unwrap(),
        RuntimeLifecycleState::Stopped
    );
    assert!(
        runtime
            .status(&handle1.session_id)
            .await
            .unwrap()
            .is_terminal()
    );

    // Test Failed terminal
    let req2 = RuntimeStartRequest::new(
        RuntimeInstanceId::generate(),
        runtime.implementation_id().clone(),
        ws,
    );
    let handle2 = runtime.start(req2).await.unwrap();
    runtime
        .transition_session(&handle2.session_id, RuntimeLifecycleState::Failed)
        .await
        .unwrap();
    assert_eq!(
        runtime.status(&handle2.session_id).await.unwrap(),
        RuntimeLifecycleState::Failed
    );
    assert!(
        runtime
            .status(&handle2.session_id)
            .await
            .unwrap()
            .is_terminal()
    );
}

#[tokio::test]
async fn test_matrix_a_lifecycle_invalid_transition_rejection() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));
    let req = RuntimeStartRequest::new(
        RuntimeInstanceId::generate(),
        runtime.implementation_id().clone(),
        ws,
    );
    let handle = runtime.start(req).await.unwrap();

    runtime.stop(&handle.session_id).await.unwrap();

    // Invariant: Terminal states cannot transition to anything
    let err = runtime
        .transition_session(&handle.session_id, RuntimeLifecycleState::Running)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        RuntimeError::InvalidLifecycleTransition { .. }
    ));

    // Cannot transition terminal to completed
    let err = runtime
        .transition_session(&handle.session_id, RuntimeLifecycleState::Completed)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        RuntimeError::InvalidLifecycleTransition { .. }
    ));
}

#[tokio::test]
async fn test_matrix_a_interrupted_is_distinct_from_terminals() {
    assert!(!RuntimeLifecycleState::Interrupted.is_terminal());
    assert!(RuntimeLifecycleState::Interrupted.is_active());

    assert!(RuntimeLifecycleState::Stopped.is_terminal());
    assert!(RuntimeLifecycleState::Completed.is_terminal());
    assert!(RuntimeLifecycleState::Failed.is_terminal());

    assert!(!RuntimeLifecycleState::Stopped.is_active());
    assert!(!RuntimeLifecycleState::Completed.is_active());
    assert!(!RuntimeLifecycleState::Failed.is_active());
}

// ============================================================================
// Matrix B: Capability Enforcement
// ============================================================================

#[tokio::test]
async fn test_matrix_b_capability_enforcement_interrupt_and_resume() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    // Strip interrupt and resume capabilities
    runtime
        .set_capabilities(
            RuntimeCapabilities::default()
                .with_interrupt(false)
                .with_resume(false)
                .with_stop(true),
        )
        .await;

    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));
    let req = RuntimeStartRequest::new(
        RuntimeInstanceId::generate(),
        runtime.implementation_id().clone(),
        ws,
    );
    let handle = runtime.start(req).await.unwrap();

    // Interrupt must fail with UnsupportedCapability
    let err = runtime.interrupt(&handle.session_id).await.unwrap_err();
    match err {
        RuntimeError::UnsupportedCapability { capability, .. } => {
            assert_eq!(capability, RuntimeCapability::Interrupt);
        }
        other => panic!("Expected UnsupportedCapability, got: {:?}", other),
    }

    // Resume must fail with UnsupportedCapability
    let err = runtime.resume(&handle.session_id, None).await.unwrap_err();
    match err {
        RuntimeError::UnsupportedCapability { capability, .. } => {
            assert_eq!(capability, RuntimeCapability::Resume);
        }
        other => panic!("Expected UnsupportedCapability, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_matrix_b_tristate_capabilities_behavior() {
    let caps = RuntimeCapabilities::default()
        .with_tools(RuntimeCapabilitySupport::Supported)
        .with_approvals(RuntimeCapabilitySupport::Unsupported)
        .with_mcp(RuntimeCapabilitySupport::Unknown);

    assert!(caps.supports(RuntimeCapability::Tools));
    assert!(!caps.supports(RuntimeCapability::Approvals));
    assert!(!caps.supports(RuntimeCapability::Mcp));

    assert_eq!(
        caps.check_support(RuntimeCapability::Tools),
        RuntimeCapabilitySupport::Supported
    );
    assert_eq!(
        caps.check_support(RuntimeCapability::Approvals),
        RuntimeCapabilitySupport::Unsupported
    );
    assert_eq!(
        caps.check_support(RuntimeCapability::Mcp),
        RuntimeCapabilitySupport::Unknown
    );

    assert!(caps.ensure_supported(RuntimeCapability::Tools).is_ok());
    assert!(caps.ensure_supported(RuntimeCapability::Approvals).is_err());
    assert!(caps.ensure_supported(RuntimeCapability::Mcp).is_err());
}

// ============================================================================
// Matrix C: Session Isolation
// ============================================================================

#[tokio::test]
async fn test_matrix_c_session_isolation() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

    let handle_a = runtime
        .start(RuntimeStartRequest::new(
            RuntimeInstanceId::generate(),
            runtime.implementation_id().clone(),
            ws.clone(),
        ))
        .await
        .unwrap();

    let handle_b = runtime
        .start(RuntimeStartRequest::new(
            RuntimeInstanceId::generate(),
            runtime.implementation_id().clone(),
            ws.clone(),
        ))
        .await
        .unwrap();

    let mut events_a = runtime.events(&handle_a.session_id).await.unwrap();
    let mut events_b = runtime.events(&handle_b.session_id).await.unwrap();

    // Send input to session A
    runtime
        .send(&handle_a.session_id, RuntimeInput::text("Input for A"))
        .await
        .unwrap();

    let event_a = events_a.recv().await.unwrap();
    assert_eq!(event_a.session_id, handle_a.session_id);

    // Stop session A
    runtime.stop(&handle_a.session_id).await.unwrap();
    assert_eq!(
        runtime.status(&handle_a.session_id).await.unwrap(),
        RuntimeLifecycleState::Stopped
    );

    // Session B remains Running and unaffected
    assert_eq!(
        runtime.status(&handle_b.session_id).await.unwrap(),
        RuntimeLifecycleState::Running
    );

    // Send input to session B
    runtime
        .send(&handle_b.session_id, RuntimeInput::text("Input for B"))
        .await
        .unwrap();
    let event_b = events_b.recv().await.unwrap();
    assert_eq!(event_b.session_id, handle_b.session_id);
}

// ============================================================================
// Matrix D: Multi-Runtime Registry
// ============================================================================

#[tokio::test]
async fn test_matrix_d_multi_runtime_registry() {
    let registry = RuntimeRegistry::new();
    let rt1 = Arc::new(FakeAgentRuntime::new("opencode-preview"));
    let rt2 = Arc::new(FakeAgentRuntime::new("claude-code-preview"));

    // Register both
    registry.register(rt1.clone()).await.unwrap();
    registry.register(rt2.clone()).await.unwrap();
    assert_eq!(registry.count().await, 2);

    // Reject duplicate registration
    let err = registry.register(rt1.clone()).await.unwrap_err();
    assert!(matches!(err, RuntimeError::DuplicateRuntime { .. }));

    // Both discoverable independently
    let lookup1 = registry
        .get(&RuntimeImplementationId::new("opencode-preview").unwrap())
        .await
        .unwrap();
    assert_eq!(lookup1.implementation_id(), rt1.implementation_id());
    let lookup2 = registry
        .get(&RuntimeImplementationId::new("claude-code-preview").unwrap())
        .await
        .unwrap();
    assert_eq!(lookup2.implementation_id(), rt2.implementation_id());

    // Missing runtime lookup returns typed RuntimeError::RuntimeNotFound
    let missing_result = registry
        .get(&RuntimeImplementationId::new("nonexistent").unwrap())
        .await;
    match missing_result {
        Err(RuntimeError::RuntimeNotFound { .. }) => {}
        Err(other) => panic!("Expected RuntimeNotFound, got {:?}", other),
        Ok(_) => panic!("Expected Err, got Ok"),
    }

    // Unregister works correctly
    assert!(
        registry
            .unregister(&RuntimeImplementationId::new("opencode-preview").unwrap())
            .await
    );
    assert_eq!(registry.count().await, 1);
    assert!(
        !registry
            .contains(&RuntimeImplementationId::new("opencode-preview").unwrap())
            .await
    );
}

// ============================================================================
// Matrix E: Event Stream Integrity
// ============================================================================

#[tokio::test]
async fn test_matrix_e_event_stream_integrity() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));
    let req = RuntimeStartRequest::new(
        RuntimeInstanceId::generate(),
        runtime.implementation_id().clone(),
        ws,
    )
    .with_initial_prompt("Hello test");

    let handle = runtime.start(req).await.unwrap();
    let recorded = runtime
        .get_recorded_events(&handle.session_id)
        .await
        .unwrap();

    // Verify monotonic sequence numbering
    for (idx, event) in recorded.iter().enumerate() {
        assert_eq!(event.sequence, (idx + 1) as u64);
        assert_eq!(event.session_id, handle.session_id);
    }

    // Verify timestamps are non-decreasing
    for window in recorded.windows(2) {
        assert!(window[0].timestamp <= window[1].timestamp);
    }

    // Stop session
    runtime.stop(&handle.session_id).await.unwrap();

    // Late input to stopped session fails closed
    let send_err = runtime
        .send(&handle.session_id, RuntimeInput::text("Late message"))
        .await
        .unwrap_err();
    assert!(matches!(send_err, RuntimeError::SendFailed { .. }));
}

// ============================================================================
// Matrix F: Workspace Binding
// ============================================================================

#[tokio::test]
async fn test_matrix_f_workspace_binding() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let worktree_id = WorktreeId::new();
    let ws_root = PathBuf::from("/test/workspace/root");
    let ws_cwd = PathBuf::from("/test/workspace/root/src");
    let src_root = PathBuf::from("/test/repo");
    let src_cwd = PathBuf::from("/test/repo/src");
    let base_sha = "abc1234".to_string();

    let ws = ExecutionWorkspace::managed(
        worktree_id,
        ws_root.clone(),
        ws_cwd.clone(),
        src_root.clone(),
        src_cwd.clone(),
        base_sha.clone(),
    );

    let req = RuntimeStartRequest::new(
        RuntimeInstanceId::generate(),
        runtime.implementation_id().clone(),
        ws.clone(),
    );
    let handle = runtime.start(req).await.unwrap();

    assert!(handle.workspace.is_managed());
    assert_eq!(handle.workspace.worktree_id(), Some(worktree_id));
    assert_eq!(handle.workspace.root(), Some(ws_root.as_path()));
    assert_eq!(handle.workspace.cwd(), ws_cwd.as_path());
    assert_eq!(handle.workspace.source_root(), Some(src_root.as_path()));
    assert_eq!(handle.workspace.base_sha(), Some("abc1234"));

    // Also test shared_source variant
    let shared_ws = ExecutionWorkspace::shared_source(PathBuf::from("/shared/dir"));
    let shared_req = RuntimeStartRequest::new(
        RuntimeInstanceId::generate(),
        runtime.implementation_id().clone(),
        shared_ws,
    );
    let shared_handle = runtime.start(shared_req).await.unwrap();
    assert!(!shared_handle.workspace.is_managed());
    assert_eq!(
        shared_handle.workspace.cwd(),
        std::path::Path::new("/shared/dir")
    );
}

// ============================================================================
// Matrix G: Error Sanitization
// ============================================================================

#[tokio::test]
async fn test_matrix_g_error_sanitization() {
    let raw_error = "Failed to connect: sk-ant-api03-secret12345678901234567890 with Bearer eyJhbGciOiJIUzI1NiJ9 and token ghp_ABCDEF1234567890";
    let sanitized = sanitize_error_message(raw_error);

    assert!(!sanitized.contains("sk-ant-api03-"));
    assert!(!sanitized.contains("eyJhbGciOiJIUzI1NiJ9"));
    assert!(!sanitized.contains("ghp_ABCDEF"));
    assert!(sanitized.contains("[REDACTED_API_KEY]"));
    assert!(sanitized.contains("[REDACTED_BEARER_TOKEN]"));
    assert!(sanitized.contains("[REDACTED_GITHUB_TOKEN]"));

    let err = RuntimeError::startup_failed(raw_error);
    match err {
        RuntimeError::StartupFailed { reason } => {
            assert!(!reason.contains("sk-ant-api03-"));
            assert!(reason.contains("[REDACTED_API_KEY]"));
        }
        _ => panic!("Expected StartupFailed"),
    }
}

// ============================================================================
// Matrix H: Correlation Integrity
// ============================================================================

#[tokio::test]
async fn test_matrix_h_correlation_integrity() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

    let studio_id = StudioId::new();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let agent_id = AgentId::new();

    let correlation = RuntimeCorrelation::new()
        .with_studio_id(studio_id)
        .with_run_id(run_id)
        .with_task_id(task_id)
        .with_agent_id(agent_id);

    let req = RuntimeStartRequest::new(
        RuntimeInstanceId::generate(),
        runtime.implementation_id().clone(),
        ws,
    )
    .with_correlation(correlation.clone());

    let handle = runtime.start(req).await.unwrap();

    assert!(handle.correlation.is_some());
    let h_corr = handle.correlation.unwrap();
    assert_eq!(h_corr.studio_id, Some(studio_id));
    assert_eq!(h_corr.run_id, Some(run_id));
    assert_eq!(h_corr.task_id, Some(task_id));
    assert_eq!(h_corr.agent_id, Some(agent_id));
}

// ============================================================================
// Matrix I: Interrupt vs Stop Semantic Distinction
// ============================================================================

#[tokio::test]
async fn test_matrix_i_interrupt_vs_stop_semantics() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));
    let req = RuntimeStartRequest::new(
        RuntimeInstanceId::generate(),
        runtime.implementation_id().clone(),
        ws,
    );
    let handle = runtime.start(req).await.unwrap();

    // 1. Interrupt pauses execution, session remains resumable
    runtime.interrupt(&handle.session_id).await.unwrap();
    assert_eq!(
        runtime.status(&handle.session_id).await.unwrap(),
        RuntimeLifecycleState::Interrupted
    );

    // Resuming from interrupted state succeeds
    runtime.resume(&handle.session_id, None).await.unwrap();
    assert_eq!(
        runtime.status(&handle.session_id).await.unwrap(),
        RuntimeLifecycleState::Running
    );

    // 2. Stop terminates process, session becomes terminal
    runtime.stop(&handle.session_id).await.unwrap();
    assert_eq!(
        runtime.status(&handle.session_id).await.unwrap(),
        RuntimeLifecycleState::Stopped
    );

    // 3. Resuming after Stop fails with InvalidLifecycleTransition
    let resume_err = runtime.resume(&handle.session_id, None).await.unwrap_err();
    assert!(matches!(
        resume_err,
        RuntimeError::InvalidLifecycleTransition { .. }
    ));
}

// ============================================================================
// Matrix J: Re-entrancy & Concurrency
// ============================================================================

#[tokio::test]
async fn test_matrix_j_reentrancy_and_concurrency() {
    let runtime = Arc::new(FakeAgentRuntime::new("test-runtime"));
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));
    let req = RuntimeStartRequest::new(
        RuntimeInstanceId::generate(),
        runtime.implementation_id().clone(),
        ws,
    );
    let handle = runtime.start(req).await.unwrap();

    // Multiple subscribers to events
    let mut sub1 = runtime.events(&handle.session_id).await.unwrap();
    let mut sub2 = runtime.events(&handle.session_id).await.unwrap();

    // Concurrent senders
    let mut tasks = Vec::new();
    for i in 0..10 {
        let rt = runtime.clone();
        let sid = handle.session_id;
        tasks.push(tokio::spawn(async move {
            rt.send(&sid, RuntimeInput::text(format!("Concurrent msg {}", i)))
                .await
                .unwrap();
            let _ = rt.status(&sid).await.unwrap();
        }));
    }

    for task in tasks {
        task.await.unwrap();
    }

    // Both subscribers receive events without deadlocks
    let ev1 = sub1.recv().await.unwrap();
    let ev2 = sub2.recv().await.unwrap();
    assert_eq!(ev1.session_id, handle.session_id);
    assert_eq!(ev2.session_id, handle.session_id);
}

// ============================================================================
// Matrix K: Fake Runtime Determinism
// ============================================================================

#[tokio::test]
async fn test_matrix_k_fake_runtime_determinism() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    runtime
        .set_fail_start(Some("Simulated startup failure".to_string()))
        .await;

    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));
    let req = RuntimeStartRequest::new(
        RuntimeInstanceId::generate(),
        runtime.implementation_id().clone(),
        ws,
    );

    let err = runtime.start(req).await.unwrap_err();
    match err {
        RuntimeError::StartupFailed { reason } => {
            assert_eq!(reason, "Simulated startup failure");
        }
        other => panic!("Expected StartupFailed, got: {:?}", other),
    }

    let calls = runtime.get_calls().await;
    assert_eq!(calls.len(), 1);
    assert!(matches!(calls[0], FakeCallRecord::Start { .. }));
}

// ============================================================================
// Matrix L: Wire Format Compatibility
// ============================================================================

#[test]
fn test_matrix_l_wire_format_compatibility() {
    let session_id = RuntimeSessionId::generate();
    let instance_id = RuntimeInstanceId::generate();
    let impl_id = RuntimeImplementationId::new("test-impl").unwrap();
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

    // 1. RuntimeStartRequest
    let req = RuntimeStartRequest::new(instance_id, impl_id.clone(), ws.clone())
        .with_initial_prompt("Prompt")
        .with_budget(
            AgentExecutionBudget::unlimited()
                .with_turns(100)
                .with_tool_calls(20)
                .with_wall_clock_secs(3600),
        );
    let req_json = serde_json::to_string(&req).unwrap();
    let req_deser: RuntimeStartRequest = serde_json::from_str(&req_json).unwrap();
    assert_eq!(req, req_deser);

    // 2. RuntimeSessionHandle
    let handle = RuntimeSessionHandle::new(
        session_id,
        instance_id,
        impl_id.clone(),
        RuntimeLifecycleState::Running,
        ws,
        None,
        Utc::now(),
    );
    let handle_json = serde_json::to_string(&handle).unwrap();
    let handle_deser: RuntimeSessionHandle = serde_json::from_str(&handle_json).unwrap();
    assert_eq!(handle, handle_deser);

    // 3. RuntimeEvent variants
    let event_kinds = vec![
        RuntimeEventKind::SessionStarted {
            session_id,
            instance_id,
        },
        RuntimeEventKind::StatusChanged {
            previous_state: RuntimeLifecycleState::Starting,
            new_state: RuntimeLifecycleState::Running,
        },
        RuntimeEventKind::OutputDelta {
            text: "delta".to_string(),
        },
        RuntimeEventKind::ToolStarted {
            tool_name: "bash".to_string(),
            tool_call_id: "c1".to_string(),
        },
        RuntimeEventKind::ToolCompleted {
            tool_name: "bash".to_string(),
            tool_call_id: "c1".to_string(),
            duration_ms: 42,
            success: true,
        },
        RuntimeEventKind::ApprovalRequested {
            approval_id: ApprovalId::new(),
            description: "run rm".to_string(),
            command: Some("rm -rf".to_string()),
        },
        RuntimeEventKind::ArtifactProduced {
            logical_name: "report".to_string(),
            path: "out.md".to_string(),
        },
        RuntimeEventKind::UsageUpdated {
            prompt_tokens: Some(10),
            completion_tokens: Some(20),
            total_tokens: Some(30),
        },
        RuntimeEventKind::Diagnostic {
            level: "warn".to_string(),
            message: "diag".to_string(),
        },
        RuntimeEventKind::Interrupted {
            reason: "user".to_string(),
        },
        RuntimeEventKind::Failed {
            safe_error_summary: "err".to_string(),
        },
        RuntimeEventKind::Completed {
            summary: Some("done".to_string()),
        },
        RuntimeEventKind::Stopped,
    ];

    for kind in event_kinds {
        let event = RuntimeEvent::new(session_id, 1, kind);
        let event_json = serde_json::to_string(&event).unwrap();
        let event_deser: RuntimeEvent = serde_json::from_str(&event_json).unwrap();
        assert_eq!(event, event_deser);
    }

    // 4. RuntimeInput variants
    let inputs = vec![
        RuntimeInput::text("text msg"),
        RuntimeInput::continuation(Some("ctx".to_string())),
        RuntimeInput::approval_response(ApprovalId::new(), true, Some("ok".to_string())),
    ];
    for inp in inputs {
        let inp_json = serde_json::to_string(&inp).unwrap();
        let inp_deser: RuntimeInput = serde_json::from_str(&inp_json).unwrap();
        assert_eq!(inp, inp_deser);
    }

    // 5. DiscoveredRuntime & RuntimeCapabilities
    let caps = RuntimeCapabilities::default()
        .with_streaming_events(true)
        .with_tools(true);
    let disc = DiscoveredRuntime::available(impl_id.clone(), "Test Display", caps.clone())
        .with_version("1.0.0")
        .with_binary_path(PathBuf::from("/bin/agent"));
    let disc_json = serde_json::to_string(&disc).unwrap();
    let disc_deser: DiscoveredRuntime = serde_json::from_str(&disc_json).unwrap();
    assert_eq!(disc, disc_deser);

    // 6. RuntimeError
    let err = RuntimeError::UnsupportedCapability {
        capability: RuntimeCapability::Resume,
        reason: "not supported".to_string(),
    };
    let err_json = serde_json::to_string(&err).unwrap();
    let err_deser: RuntimeError = serde_json::from_str(&err_json).unwrap();
    assert_eq!(err, err_deser);
}

// ============================================================================
// Matrix M: Architectural Invariant Checks
// ============================================================================

#[test]
fn test_matrix_m_runtime_vs_provider_wire_collision_protection() {
    let raw_uuid = Uuid::new_v4();
    let rt_inst = RuntimeInstanceId::from_uuid(raw_uuid);

    // Serialized RuntimeInstanceId includes "rt-inst-" prefix
    let rt_inst_json = serde_json::to_string(&rt_inst).unwrap();
    assert!(rt_inst_json.contains("rt-inst-"));

    // Attempting to deserialize RuntimeInstanceId as ProviderInstanceId fails at wire boundary
    let provider_deser_result: Result<ProviderInstanceId, _> = serde_json::from_str(&rt_inst_json);
    assert!(
        provider_deser_result.is_err(),
        "ProviderInstanceId must reject RuntimeInstanceId prefixed string at deserialization"
    );

    // Attempting to deserialize raw ProviderInstanceId JSON as RuntimeInstanceId fails
    let provider_inst = ProviderInstanceId::from_uuid(raw_uuid);
    let provider_inst_json = serde_json::to_string(&provider_inst).unwrap();
    let rt_deser_result: Result<RuntimeInstanceId, _> = serde_json::from_str(&provider_inst_json);
    assert!(
        rt_deser_result.is_err(),
        "RuntimeInstanceId must reject unprefixed ProviderInstanceId string at deserialization"
    );

    // RuntimeSessionId collision protection
    let rt_sess = RuntimeSessionId::from_uuid(raw_uuid);
    let rt_sess_json = serde_json::to_string(&rt_sess).unwrap();
    assert!(rt_sess_json.contains("rt-sess-"));
    let provider_deser_sess: Result<ProviderInstanceId, _> = serde_json::from_str(&rt_sess_json);
    assert!(
        provider_deser_sess.is_err(),
        "ProviderInstanceId must reject RuntimeSessionId prefixed string"
    );
}

#[test]
fn test_matrix_m_model_ref_orthogonal_to_runtime_id() {
    let runtime_impl = RuntimeImplementationId::new("opencode").unwrap();
    let provider_inst = ProviderInstanceId::new();
    let model_id = ModelId::new("claude-sonnet-5").unwrap();
    let model_ref = ModelRef::new(provider_inst, model_id);

    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));
    let req = RuntimeStartRequest::new(RuntimeInstanceId::generate(), runtime_impl.clone(), ws)
        .with_model_ref(model_ref.clone());

    // Identity check: runtime_impl remains orthogonal to model_ref
    assert_eq!(req.implementation_id.as_str(), "opencode");
    assert_eq!(req.model_ref.unwrap().provider_instance_id, provider_inst);
}
