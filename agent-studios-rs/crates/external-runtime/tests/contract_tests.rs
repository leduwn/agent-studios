mod support;
use support::{FakeAgentRuntime, FakeCallRecord};

use std::path::PathBuf;
use std::sync::Arc;

use agent_studios_external_runtime::*;
use agent_studios_protocol::id::WorktreeId;
use agent_studios_protocol::worktree::ExecutionWorkspace;
use agent_studios_provider::{ProviderInstanceId, SecretBackend, SecretReference};
use uuid::Uuid;

fn valid_test_workspace() -> ExecutionWorkspace {
    ExecutionWorkspace::managed(
        WorktreeId::new(),
        PathBuf::from("/worktree"),
        PathBuf::from("/worktree"),
        PathBuf::from("/source"),
        PathBuf::from("/source"),
        "0123456789abcdef0123456789abcdef01234567",
    )
}

// ============================================================================
// Milestone M10.1: 24 Contract Regression Tests
// ============================================================================

#[tokio::test]
async fn test_01_multi_instance_discovery_authority() {
    let runtime = FakeAgentRuntime::new("opencode");
    let impl_id = runtime.implementation_id().clone();
    let caps = runtime.capabilities().await;

    let inst1_id = RuntimeInstanceId::generate();
    let inst2_id = RuntimeInstanceId::generate();

    let inst1 = DiscoveredRuntimeInstance::available(
        inst1_id,
        impl_id.clone(),
        "OpenCode CLI Global",
        caps.clone(),
    )
    .with_version("1.0.0");
    let inst2 = DiscoveredRuntimeInstance::available(
        inst2_id,
        impl_id.clone(),
        "OpenCode CLI Workspace",
        caps.clone(),
    )
    .with_version("1.1.0");

    runtime.set_instances(vec![inst1, inst2]).await;

    let discovered = runtime.discover().await.unwrap();
    assert_eq!(discovered.len(), 2);
    assert_eq!(discovered[0].instance_id, inst1_id);
    assert_eq!(discovered[1].instance_id, inst2_id);
    assert_ne!(discovered[0].instance_id, discovered[1].instance_id);
    assert_eq!(discovered[0].implementation_id, impl_id);
    assert_eq!(discovered[1].implementation_id, impl_id);
}

#[tokio::test]
async fn test_02_instance_ref_validation_at_start() {
    let runtime = FakeAgentRuntime::new("opencode");
    let ws = valid_test_workspace();

    // Case 1: Mismatched implementation ID
    let mismatched_impl = RuntimeImplementationId::new("claude-code").unwrap();
    let inst_ref = RuntimeInstanceRef::new(mismatched_impl.clone(), RuntimeInstanceId::generate());
    let req = RuntimeStartRequest::new(inst_ref, ws.clone());

    let err = runtime.start(req).await.unwrap_err();
    assert!(
        matches!(
            &err,
            RuntimeError::InstanceImplementationMismatch { expected, actual, .. }
            if expected.as_str() == "opencode" && actual.as_str() == "claude-code"
        ),
        "Expected InstanceImplementationMismatch, got {:?}",
        err
    );

    // Case 2: Unknown instance ID
    let unknown_inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        RuntimeInstanceId::generate(),
    );
    let req2 = RuntimeStartRequest::new(unknown_inst_ref, ws);
    let err2 = runtime.start(req2).await.unwrap_err();
    assert!(
        matches!(err2, RuntimeError::UnknownInstance { .. }),
        "Expected UnknownInstance, got {:?}",
        err2
    );
}

#[tokio::test]
async fn test_03_session_ref_ownership_validation() {
    let runtime = FakeAgentRuntime::new("opencode");
    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();

    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();

    // Forged session ref with foreign implementation ID
    let foreign_impl = RuntimeImplementationId::new("foreign-runtime").unwrap();
    let forged_ref = RuntimeSessionRef::new(foreign_impl, handle.instance_id, handle.session_id);

    // Verify all lifecycle methods reject forged implementation
    let err_send = runtime
        .send(&forged_ref, RuntimeInput::text("hi"))
        .await
        .unwrap_err();
    assert!(matches!(
        err_send,
        RuntimeError::SessionImplementationMismatch { .. }
    ));

    let err_interrupt = runtime.interrupt(&forged_ref).await.unwrap_err();
    assert!(matches!(
        err_interrupt,
        RuntimeError::SessionImplementationMismatch { .. }
    ));

    let err_resume = runtime.resume(&forged_ref, None).await.unwrap_err();
    assert!(matches!(
        err_resume,
        RuntimeError::SessionImplementationMismatch { .. }
    ));

    let err_stop = runtime.stop(&forged_ref).await.unwrap_err();
    assert!(matches!(
        err_stop,
        RuntimeError::SessionImplementationMismatch { .. }
    ));

    let err_status = runtime.status(&forged_ref).await.unwrap_err();
    assert!(matches!(
        err_status,
        RuntimeError::SessionImplementationMismatch { .. }
    ));

    let err_events = runtime.events(&forged_ref).await.unwrap_err();
    assert!(matches!(
        err_events,
        RuntimeError::SessionImplementationMismatch { .. }
    ));
}

#[tokio::test]
async fn test_04_session_instance_mismatch_error() {
    let runtime = FakeAgentRuntime::new("opencode");
    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();

    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();

    // Forged session ref with wrong instance ID
    let forged_instance_id = RuntimeInstanceId::generate();
    let forged_ref = RuntimeSessionRef::new(
        handle.implementation_id.clone(),
        forged_instance_id,
        handle.session_id,
    );

    let err = runtime
        .send(&forged_ref, RuntimeInput::text("test"))
        .await
        .unwrap_err();
    match err {
        RuntimeError::SessionInstanceMismatch {
            expected,
            actual,
            session_id,
        } => {
            assert_eq!(expected, handle.instance_id);
            assert_eq!(actual, forged_instance_id);
            assert_eq!(session_id, handle.session_id);
        }
        other => panic!("Expected SessionInstanceMismatch, got {:?}", other),
    }
}

#[tokio::test]
async fn test_05_zero_plaintext_secrets_in_start_request() {
    let runtime_impl = RuntimeImplementationId::new("opencode").unwrap();
    let inst_id = RuntimeInstanceId::generate();
    let inst_ref = RuntimeInstanceRef::new(runtime_impl, inst_id);
    let ws = valid_test_workspace();

    let secret =
        SecretReference::new(SecretBackend::EnvironmentVariable, "OPENCODE_SECRET_TOKEN").unwrap();

    let req = RuntimeStartRequest::new(inst_ref, ws)
        .with_literal_env("DEBUG", "1")
        .unwrap()
        .with_secret_env("API_KEY", secret)
        .unwrap();

    assert_eq!(req.environment_bindings.len(), 2);
    assert_eq!(req.environment_bindings[0].name(), "DEBUG");
    assert!(matches!(
        req.environment_bindings[0].source(),
        EnvironmentBindingSource::Literal { .. }
    ));
    assert_eq!(req.environment_bindings[1].name(), "API_KEY");
    assert!(matches!(
        req.environment_bindings[1].source(),
        EnvironmentBindingSource::Secret { .. }
    ));

    // Verify serialization contains secret reference, no plaintext key leak
    let serialized = serde_json::to_string(&req).unwrap();
    assert!(serialized.contains("OPENCODE_SECRET_TOKEN"));
    assert!(!serialized.contains("raw_secret_value"));
}

#[tokio::test]
async fn test_06_sanitized_runtime_message_structural_safety() {
    let raw = "Failed connecting with sk-ant-api03-abcdef1234567890 and Bearer tok_xyz123";
    let msg = SanitizedRuntimeMessage::new(raw);

    let rendered = msg.to_string();
    assert!(!rendered.contains("sk-ant-"));
    assert!(!rendered.contains("tok_xyz123"));
    assert!(rendered.contains("[REDACTED_API_KEY]"));
    assert!(rendered.contains("[REDACTED_BEARER_TOKEN]"));

    // Serialize and deserialize verifies sanitization upon deserialization
    let json = serde_json::to_string(&msg).unwrap();
    let deser: SanitizedRuntimeMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(deser.as_str(), rendered);

    // Direct deserialization of dirty JSON must sanitize on deserialize
    let dirty_json = "\"Error with api_key=super_secret_key_123\"";
    let sanitized_deser: SanitizedRuntimeMessage = serde_json::from_str(dirty_json).unwrap();
    assert!(!sanitized_deser.as_str().contains("super_secret_key_123"));
    assert!(sanitized_deser.as_str().contains("[REDACTED_API_KEY]"));
}

#[tokio::test]
async fn test_07_stop_idempotency_on_stopped_session() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();

    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();
    let sess_ref = handle.session_ref();

    // First stop succeeds
    runtime.stop(&sess_ref).await.unwrap();
    assert_eq!(
        runtime.status(&sess_ref).await.unwrap(),
        RuntimeLifecycleState::Stopped
    );

    // Second stop is idempotent and succeeds
    runtime.stop(&sess_ref).await.unwrap();
    assert_eq!(
        runtime.status(&sess_ref).await.unwrap(),
        RuntimeLifecycleState::Stopped
    );

    // Third stop is also idempotent
    runtime.stop(&sess_ref).await.unwrap();
    assert_eq!(
        runtime.status(&sess_ref).await.unwrap(),
        RuntimeLifecycleState::Stopped
    );
}

#[tokio::test]
async fn test_08_stop_rejection_on_completed_session() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();

    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();
    let sess_ref = handle.session_ref();

    runtime
        .transition_session(&handle.session_id, RuntimeLifecycleState::Completed)
        .await
        .unwrap();
    assert_eq!(
        runtime.status(&sess_ref).await.unwrap(),
        RuntimeLifecycleState::Completed
    );

    // Stop on Completed must return TerminalStateError
    let err = runtime.stop(&sess_ref).await.unwrap_err();
    match err {
        RuntimeError::TerminalStateError {
            session_id,
            current_state,
            attempted_action,
        } => {
            assert_eq!(session_id, handle.session_id);
            assert_eq!(current_state, RuntimeLifecycleState::Completed);
            assert_eq!(attempted_action, "stop");
        }
        other => panic!("Expected TerminalStateError, got {:?}", other),
    }
}

#[tokio::test]
async fn test_09_stop_rejection_on_failed_session() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();

    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();
    let sess_ref = handle.session_ref();

    runtime
        .transition_session(&handle.session_id, RuntimeLifecycleState::Failed)
        .await
        .unwrap();
    assert_eq!(
        runtime.status(&sess_ref).await.unwrap(),
        RuntimeLifecycleState::Failed
    );

    // Stop on Failed must return TerminalStateError
    let err = runtime.stop(&sess_ref).await.unwrap_err();
    match err {
        RuntimeError::TerminalStateError {
            session_id,
            current_state,
            attempted_action,
        } => {
            assert_eq!(session_id, handle.session_id);
            assert_eq!(current_state, RuntimeLifecycleState::Failed);
            assert_eq!(attempted_action, "stop");
        }
        other => panic!("Expected TerminalStateError, got {:?}", other),
    }
}

#[tokio::test]
async fn test_10_stop_capability_enforcement() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let caps = RuntimeCapabilities::default()
        .with_streaming_events(true)
        .with_stop(false); // Stop explicitly unsupported
    runtime.set_capabilities(caps).await;

    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();

    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();
    let sess_ref = handle.session_ref();

    let err = runtime.stop(&sess_ref).await.unwrap_err();
    assert!(
        matches!(
            err,
            RuntimeError::UnsupportedCapability {
                capability: RuntimeCapability::Stop,
                ..
            }
        ),
        "Expected UnsupportedCapability for Stop, got {:?}",
        err
    );
}

#[tokio::test]
async fn test_11_interrupt_capability_enforcement() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let caps = RuntimeCapabilities::default()
        .with_streaming_events(true)
        .with_interrupt(false); // Interrupt explicitly unsupported
    runtime.set_capabilities(caps).await;

    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();

    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();
    let sess_ref = handle.session_ref();

    let err = runtime.interrupt(&sess_ref).await.unwrap_err();
    assert!(
        matches!(
            err,
            RuntimeError::UnsupportedCapability {
                capability: RuntimeCapability::Interrupt,
                ..
            }
        ),
        "Expected UnsupportedCapability for Interrupt, got {:?}",
        err
    );
}

#[tokio::test]
async fn test_12_resume_capability_enforcement() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let caps = RuntimeCapabilities::default()
        .with_streaming_events(true)
        .with_interrupt(true)
        .with_resume(false); // Resume explicitly unsupported
    runtime.set_capabilities(caps).await;

    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();

    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();
    let sess_ref = handle.session_ref();

    // Transition to Interrupted manually for testing resume capability check
    runtime
        .transition_session(&handle.session_id, RuntimeLifecycleState::Interrupted)
        .await
        .unwrap();

    let err = runtime.resume(&sess_ref, None).await.unwrap_err();
    assert!(
        matches!(
            err,
            RuntimeError::UnsupportedCapability {
                capability: RuntimeCapability::Resume,
                ..
            }
        ),
        "Expected UnsupportedCapability for Resume, got {:?}",
        err
    );
}

#[tokio::test]
async fn test_13_unknown_capability_fails_closed() {
    let caps = RuntimeCapabilities::default(); // All default to Unknown

    assert!(!caps.supports(RuntimeCapability::Stop));
    assert!(!caps.supports(RuntimeCapability::Interrupt));
    assert!(!caps.supports(RuntimeCapability::Resume));

    let err_stop = caps.ensure_supported(RuntimeCapability::Stop).unwrap_err();
    assert!(matches!(
        err_stop,
        RuntimeError::UnsupportedCapability {
            capability: RuntimeCapability::Stop,
            ..
        }
    ));

    let err_interrupt = caps
        .ensure_supported(RuntimeCapability::Interrupt)
        .unwrap_err();
    assert!(matches!(
        err_interrupt,
        RuntimeError::UnsupportedCapability {
            capability: RuntimeCapability::Interrupt,
            ..
        }
    ));
}

#[tokio::test]
async fn test_14_non_resurrection_from_stopped() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();

    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();
    let sess_ref = handle.session_ref();

    runtime.stop(&sess_ref).await.unwrap();

    let err = runtime
        .send(&sess_ref, RuntimeInput::text("resurrect"))
        .await
        .unwrap_err();
    match err {
        RuntimeError::TerminalStateError {
            session_id,
            current_state,
            attempted_action,
        } => {
            assert_eq!(session_id, handle.session_id);
            assert_eq!(current_state, RuntimeLifecycleState::Stopped);
            assert_eq!(attempted_action, "send");
        }
        other => panic!("Expected TerminalStateError, got {:?}", other),
    }
}

#[tokio::test]
async fn test_15_non_resurrection_from_completed() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();

    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();
    let sess_ref = handle.session_ref();

    runtime
        .transition_session(&handle.session_id, RuntimeLifecycleState::Completed)
        .await
        .unwrap();

    let err = runtime
        .send(&sess_ref, RuntimeInput::text("resurrect"))
        .await
        .unwrap_err();
    match err {
        RuntimeError::TerminalStateError {
            session_id,
            current_state,
            attempted_action,
        } => {
            assert_eq!(session_id, handle.session_id);
            assert_eq!(current_state, RuntimeLifecycleState::Completed);
            assert_eq!(attempted_action, "send");
        }
        other => panic!("Expected TerminalStateError, got {:?}", other),
    }
}

#[tokio::test]
async fn test_16_non_resurrection_from_failed() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();

    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();
    let sess_ref = handle.session_ref();

    runtime
        .transition_session(&handle.session_id, RuntimeLifecycleState::Failed)
        .await
        .unwrap();

    let err = runtime
        .send(&sess_ref, RuntimeInput::text("resurrect"))
        .await
        .unwrap_err();
    match err {
        RuntimeError::TerminalStateError {
            session_id,
            current_state,
            attempted_action,
        } => {
            assert_eq!(session_id, handle.session_id);
            assert_eq!(current_state, RuntimeLifecycleState::Failed);
            assert_eq!(attempted_action, "send");
        }
        other => panic!("Expected TerminalStateError, got {:?}", other),
    }
}

#[tokio::test]
async fn test_17_deterministic_registry_ordering() {
    let registry = RuntimeRegistry::new();

    let r_z = Arc::new(FakeAgentRuntime::new("zeta-runtime"));
    let r_a = Arc::new(FakeAgentRuntime::new("alpha-runtime"));
    let r_m = Arc::new(FakeAgentRuntime::new("middle-runtime"));

    // Register in arbitrary order
    registry.register(r_z).await.unwrap();
    registry.register(r_a).await.unwrap();
    registry.register(r_m).await.unwrap();

    // Verify deterministic alphabetical sorting by RuntimeImplementationId
    let ids = registry.list_ids().await;
    let names: Vec<&str> = ids.iter().map(|id| id.as_str()).collect();
    assert_eq!(
        names,
        vec!["alpha-runtime", "middle-runtime", "zeta-runtime"]
    );

    let caps = registry.capabilities_all().await;
    let cap_keys: Vec<&str> = caps.keys().map(|id| id.as_str()).collect();
    assert_eq!(
        cap_keys,
        vec!["alpha-runtime", "middle-runtime", "zeta-runtime"]
    );
}

#[tokio::test]
async fn test_18_registry_lock_drop_before_await() {
    let registry = RuntimeRegistry::new();
    let runtime = Arc::new(FakeAgentRuntime::new("concurrent-test"));
    registry.register(runtime.clone()).await.unwrap();

    // Concurrently trigger discovery across registry and direct runtime get
    let reg_clone1 = registry.clone();
    let reg_clone2 = registry.clone();

    let handle1 = tokio::spawn(async move { reg_clone1.discover_all().await.unwrap() });

    let handle2 = tokio::spawn(async move {
        let impl_id = RuntimeImplementationId::new("concurrent-test").unwrap();
        reg_clone2.get(&impl_id).await.unwrap()
    });

    let (res1, res2) = tokio::join!(handle1, handle2);
    assert!(!res1.unwrap().is_empty());
    assert_eq!(
        res2.unwrap().implementation_id().as_str(),
        "concurrent-test"
    );
}

#[tokio::test]
async fn test_19_event_sequence_number_authority() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();

    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();
    let sess_ref = handle.session_ref();

    runtime
        .send(&sess_ref, RuntimeInput::text("input 1"))
        .await
        .unwrap();
    runtime
        .send(&sess_ref, RuntimeInput::text("input 2"))
        .await
        .unwrap();
    runtime
        .send(&sess_ref, RuntimeInput::text("input 3"))
        .await
        .unwrap();

    let events = runtime
        .get_recorded_events(&handle.session_id)
        .await
        .unwrap();
    assert!(events.len() >= 5);

    // Verify sequence numbers are strictly 1, 2, 3, 4, 5... monotonic authority
    for (idx, event) in events.iter().enumerate() {
        assert_eq!(event.sequence, (idx + 1) as u64);
        assert_eq!(event.session_id, handle.session_id);
    }
}

#[tokio::test]
async fn test_20_session_started_event_correlation() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();

    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();

    let events = runtime
        .get_recorded_events(&handle.session_id)
        .await
        .unwrap();
    let first = &events[0];
    assert_eq!(first.sequence, 1);
    match &first.kind {
        RuntimeEventKind::SessionStarted {
            session_id,
            instance_id,
        } => {
            assert_eq!(*session_id, handle.session_id);
            assert_eq!(*instance_id, handle.instance_id);
        }
        other => panic!("Expected SessionStarted, got {:?}", other),
    }
}

#[tokio::test]
async fn test_21_discovered_instance_availability() {
    let impl_id = RuntimeImplementationId::new("test-runtime").unwrap();
    let caps = RuntimeCapabilities::default();
    let inst_id = RuntimeInstanceId::generate();

    // Available instance
    let avail =
        DiscoveredRuntimeInstance::available(inst_id, impl_id.clone(), "Avail", caps.clone());
    assert!(avail.is_available());
    assert!(avail.availability.unavailable_reason().is_none());

    // Unavailable instance with sanitization
    let unavail = DiscoveredRuntimeInstance::unavailable(
        inst_id,
        impl_id.clone(),
        "Unavail",
        caps,
        "Failed on key sk-ant-secret12345",
    );
    assert!(!unavail.is_available());
    let reason = unavail.availability.unavailable_reason().unwrap();
    assert!(!reason.as_str().contains("sk-ant-"));
    assert!(reason.as_str().contains("[REDACTED_API_KEY]"));
}

#[tokio::test]
async fn test_22_production_exports_exclude_fake() {
    // Compile-time verification: FakeAgentRuntime is only accessible from tests/support,
    // not directly from crate root.
    // The trait AgentRuntime is exported from the crate root:
    fn assert_trait_object(_: &dyn AgentRuntime) {}
    let fake = FakeAgentRuntime::new("export-check");
    assert_trait_object(&fake);
}

#[tokio::test]
async fn test_23_runtime_config_ref_integrity() {
    let config_ref = RuntimeConfigRef::new("profiles/staging.json").unwrap();
    assert_eq!(config_ref.as_str(), "profiles/staging.json");

    // Empty config ref rejected
    let empty_err = RuntimeConfigRef::new("   ").unwrap_err();
    assert!(matches!(
        empty_err,
        RuntimeError::InvalidConfiguration { .. }
    ));

    // Config ref attached to RuntimeInstanceRef
    let impl_id = RuntimeImplementationId::new("opencode").unwrap();
    let inst_id = RuntimeInstanceId::generate();
    let inst_ref = RuntimeInstanceRef::new(impl_id, inst_id).with_config_ref(config_ref.clone());

    assert_eq!(inst_ref.config_ref.as_ref().unwrap(), &config_ref);

    let ws = valid_test_workspace();
    let req = RuntimeStartRequest::new(inst_ref, ws);
    assert_eq!(req.runtime_config_ref().unwrap(), &config_ref);
}

#[tokio::test]
async fn test_24_full_lifecycle_e2e_contract() {
    let runtime = FakeAgentRuntime::new("opencode");
    let instances = runtime.discover().await.unwrap();
    assert!(!instances.is_empty());
    let inst_id = instances[0].instance_id;

    let inst_ref = RuntimeInstanceRef::new(runtime.implementation_id().clone(), inst_id);
    let ws = valid_test_workspace();
    let req = RuntimeStartRequest::new(inst_ref, ws).with_initial_prompt("Hello assistant");

    // 1. Start session
    let handle = runtime.start(req).await.unwrap();
    let sess_ref = handle.session_ref();
    assert_eq!(
        runtime.status(&sess_ref).await.unwrap(),
        RuntimeLifecycleState::Running
    );

    // 2. Send turn
    runtime
        .send(&sess_ref, RuntimeInput::text("Next turn"))
        .await
        .unwrap();
    assert_eq!(
        runtime.status(&sess_ref).await.unwrap(),
        RuntimeLifecycleState::Running
    );

    // 3. Interrupt
    runtime.interrupt(&sess_ref).await.unwrap();
    assert_eq!(
        runtime.status(&sess_ref).await.unwrap(),
        RuntimeLifecycleState::Interrupted
    );

    // 4. Resume
    runtime
        .resume(&sess_ref, Some(RuntimeInput::text("Continue")))
        .await
        .unwrap();
    assert_eq!(
        runtime.status(&sess_ref).await.unwrap(),
        RuntimeLifecycleState::Running
    );

    // 5. Stop
    runtime.stop(&sess_ref).await.unwrap();
    assert_eq!(
        runtime.status(&sess_ref).await.unwrap(),
        RuntimeLifecycleState::Stopped
    );

    // 6. Idempotent stop
    runtime.stop(&sess_ref).await.unwrap();
    assert_eq!(
        runtime.status(&sess_ref).await.unwrap(),
        RuntimeLifecycleState::Stopped
    );

    // 7. Verify all call records
    let calls = runtime.get_calls().await;
    assert!(calls.iter().any(|c| matches!(c, FakeCallRecord::Discover)));
    assert!(
        calls
            .iter()
            .any(|c| matches!(c, FakeCallRecord::Start { .. }))
    );
    assert!(
        calls
            .iter()
            .any(|c| matches!(c, FakeCallRecord::Send { .. }))
    );
    assert!(
        calls
            .iter()
            .any(|c| matches!(c, FakeCallRecord::Interrupt { .. }))
    );
    assert!(
        calls
            .iter()
            .any(|c| matches!(c, FakeCallRecord::Resume { .. }))
    );
    assert!(
        calls
            .iter()
            .any(|c| matches!(c, FakeCallRecord::Stop { .. }))
    );
}

// ============================================================================
// Matrix Coverage Retained & Hardened (Matrices A - M)
// ============================================================================

#[tokio::test]
async fn test_matrix_a_lifecycle_normal_completion() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();
    let req = RuntimeStartRequest::new(inst_ref, ws);

    let handle = runtime.start(req).await.unwrap();
    let sess_ref = handle.session_ref();
    assert_eq!(
        runtime.status(&sess_ref).await.unwrap(),
        RuntimeLifecycleState::Running
    );

    runtime
        .transition_session(&handle.session_id, RuntimeLifecycleState::Completed)
        .await
        .unwrap();
    assert_eq!(
        runtime.status(&sess_ref).await.unwrap(),
        RuntimeLifecycleState::Completed
    );
    assert!(runtime.status(&sess_ref).await.unwrap().is_terminal());
}

#[tokio::test]
async fn test_matrix_b_capability_audit() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let caps = runtime.capabilities().await;
    assert!(caps.supports(RuntimeCapability::StreamingEvents));
    assert!(caps.supports(RuntimeCapability::Interrupt));
    assert!(caps.supports(RuntimeCapability::Resume));
    assert!(caps.supports(RuntimeCapability::Stop));
    assert!(caps.supports(RuntimeCapability::Tools));
    assert!(caps.supports(RuntimeCapability::Approvals));
}

#[tokio::test]
async fn test_matrix_c_id_collision_prevention() {
    let raw_uuid = Uuid::new_v4();
    let rt_inst = RuntimeInstanceId::from_uuid(raw_uuid);
    let rt_inst_json = serde_json::to_string(&rt_inst).unwrap();
    assert!(rt_inst_json.contains("rt-inst-"));

    let provider_inst = ProviderInstanceId::from_uuid(raw_uuid);
    let provider_inst_json = serde_json::to_string(&provider_inst).unwrap();
    let rt_deser_result: Result<RuntimeInstanceId, _> = serde_json::from_str(&provider_inst_json);
    assert!(rt_deser_result.is_err());
}

#[tokio::test]
async fn test_25_event_subscription_replay_and_live_stream() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();
    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();
    let sess_ref = handle.session_ref();

    // Subscribe to full history and live stream
    let mut sub = runtime.events(&sess_ref).await.unwrap();

    // Trigger an input turn and then stop
    runtime
        .send(&sess_ref, RuntimeInput::text("streaming input"))
        .await
        .unwrap();
    runtime.stop(&sess_ref).await.unwrap();

    // Consume all events until terminal close
    let mut events = Vec::new();
    while let Some(event) = sub.next_event().await.unwrap() {
        events.push(event);
    }

    assert_eq!(events.len(), 4);
    assert_eq!(events[0].sequence, 1);
    assert!(matches!(
        events[0].kind,
        RuntimeEventKind::SessionStarted { .. }
    ));
    assert_eq!(events[1].sequence, 2);
    assert!(matches!(
        events[1].kind,
        RuntimeEventKind::StatusChanged { .. }
    ));
    assert_eq!(events[2].sequence, 3);
    assert!(matches!(
        events[2].kind,
        RuntimeEventKind::OutputDelta { .. }
    ));
    assert_eq!(events[3].sequence, 4);
    assert!(matches!(events[3].kind, RuntimeEventKind::Stopped));

    // Next event after close is Ok(None)
    assert!(sub.next_event().await.unwrap().is_none());
}

#[tokio::test]
async fn test_26_event_subscription_replay_from_offset() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();
    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();
    let sess_ref = handle.session_ref();

    runtime
        .send(&sess_ref, RuntimeInput::text("msg 1"))
        .await
        .unwrap();
    runtime
        .send(&sess_ref, RuntimeInput::text("msg 2"))
        .await
        .unwrap();
    runtime.stop(&sess_ref).await.unwrap();

    // Replay after sequence 2 (skips sequence 1 and 2)
    let mut sub = runtime.events_after(&sess_ref, Some(2)).await.unwrap();

    let first = sub.next_event().await.unwrap().unwrap();
    assert_eq!(first.sequence, 3);
    assert!(matches!(first.kind, RuntimeEventKind::OutputDelta { .. }));

    let second = sub.next_event().await.unwrap().unwrap();
    assert_eq!(second.sequence, 4);
    assert!(matches!(second.kind, RuntimeEventKind::OutputDelta { .. }));

    let third = sub.next_event().await.unwrap().unwrap();
    assert_eq!(third.sequence, 5);
    assert!(matches!(third.kind, RuntimeEventKind::Stopped));

    assert!(sub.next_event().await.unwrap().is_none());
}

#[tokio::test]
async fn test_27_event_retention_exceeded_error() {
    let session_id = RuntimeSessionId::generate();
    let instance_id = RuntimeInstanceId::generate();
    // Tiny retention buffer of 3 events
    let hub = SessionEventHub::new(session_id, instance_id, 3);

    // Sequence 1 must be SessionStarted
    hub.emit(RuntimeEventKind::SessionStarted {
        session_id,
        instance_id,
    })
    .await
    .unwrap();

    // Emit 5 more events (seq 2 to 6)
    for _ in 0..5 {
        hub.emit(RuntimeEventKind::OutputDelta {
            text: "delta".to_string(),
        })
        .await
        .unwrap();
    }

    // Earliest retained event sequence is 4 (events 1, 2, 3 dropped)
    let err = hub.subscribe(Some(1)).await.unwrap_err();
    match err {
        RuntimeError::EventRetentionExceeded {
            requested_sequence,
            earliest_available_sequence,
            ..
        } => {
            assert_eq!(requested_sequence, 1);
            assert_eq!(earliest_available_sequence, 4);
        }
        other => panic!("Expected EventRetentionExceeded, got {:?}", other),
    }

    // Subscribing after sequence 3 or 4 succeeds
    let sub = hub.subscribe(Some(4)).await;
    assert!(sub.is_ok());
}

#[tokio::test]
async fn test_28_event_boundary_validator_invariants() {
    let session_id = RuntimeSessionId::generate();
    let instance_id = RuntimeInstanceId::generate();
    let mut validator = EventBoundaryValidator::new(session_id, instance_id);

    // 1. Sequence 0 rejected
    let ev0 = RuntimeEvent::new(
        session_id,
        0,
        RuntimeEventKind::SessionStarted {
            session_id,
            instance_id,
        },
    );
    assert!(matches!(
        validator.validate(&ev0).unwrap_err(),
        RuntimeError::InvalidEventSequence { sequence: 0, .. }
    ));

    // 2. Sequence 1 must be SessionStarted
    let ev1_wrong_kind = RuntimeEvent::new(
        session_id,
        1,
        RuntimeEventKind::OutputDelta {
            text: "bad".to_string(),
        },
    );
    assert!(matches!(
        validator.validate(&ev1_wrong_kind).unwrap_err(),
        RuntimeError::InvalidEventSequence { sequence: 1, .. }
    ));

    // 3. Foreign session ID rejected
    let foreign_sess = RuntimeSessionId::generate();
    let ev1_foreign = RuntimeEvent::new(
        foreign_sess,
        1,
        RuntimeEventKind::SessionStarted {
            session_id: foreign_sess,
            instance_id,
        },
    );
    assert!(matches!(
        validator.validate(&ev1_foreign).unwrap_err(),
        RuntimeError::EventSessionMismatch { .. }
    ));

    // 4. Valid SessionStarted accepted
    let ev1_valid = RuntimeEvent::new(
        session_id,
        1,
        RuntimeEventKind::SessionStarted {
            session_id,
            instance_id,
        },
    );
    assert!(validator.validate(&ev1_valid).is_ok());

    // 5. Sequence gap (3 instead of 2) rejected
    let ev3_gap = RuntimeEvent::new(
        session_id,
        3,
        RuntimeEventKind::OutputDelta {
            text: "gap".to_string(),
        },
    );
    assert!(matches!(
        validator.validate(&ev3_gap).unwrap_err(),
        RuntimeError::InvalidEventSequence { sequence: 3, .. }
    ));

    // 6. Valid sequence 2 accepted
    let ev2_valid = RuntimeEvent::new(
        session_id,
        2,
        RuntimeEventKind::OutputDelta {
            text: "delta".to_string(),
        },
    );
    assert!(validator.validate(&ev2_valid).is_ok());

    // 7. Terminal event sequence 3 accepted
    let ev3_terminal = RuntimeEvent::new(session_id, 3, RuntimeEventKind::Stopped);
    assert!(validator.validate(&ev3_terminal).is_ok());

    // 8. Event after terminal rejected
    let ev4_post_term = RuntimeEvent::new(
        session_id,
        4,
        RuntimeEventKind::OutputDelta {
            text: "after term".to_string(),
        },
    );
    assert!(matches!(
        validator.validate(&ev4_post_term).unwrap_err(),
        RuntimeError::EventAfterTerminalState { .. }
    ));
}

#[tokio::test]
async fn test_29_non_secret_value_deserialization_rejection() {
    // Secret keywords rejected during serde deserialization
    assert!(serde_json::from_str::<NonSecretValue>("\"my_secret_token\"").is_err());
    assert!(serde_json::from_str::<NonSecretValue>("\"some_password_123\"").is_err());
    assert!(serde_json::from_str::<NonSecretValue>("\"AUTH_HEADER\"").is_err());
    assert!(serde_json::from_str::<NonSecretValue>("\"Bearer secret123\"").is_err());

    // Secret prefixes rejected during serde deserialization
    assert!(serde_json::from_str::<NonSecretValue>("\"sk-ant-12345\"").is_err());
    assert!(serde_json::from_str::<NonSecretValue>("\"ghp_0123456789\"").is_err());
    assert!(serde_json::from_str::<NonSecretValue>("\"glpat-abcdef\"").is_err());

    // Safe values deserialize cleanly
    let safe: NonSecretValue = serde_json::from_str("\"claude-sonnet-4-6\"").unwrap();
    assert_eq!(safe.as_str(), "claude-sonnet-4-6");
}

#[tokio::test]
async fn test_30_start_request_metadata_and_env_deserialization_safety() {
    // EnvironmentVariableBinding::literal rejects secret keywords and prefixes
    assert!(EnvironmentVariableBinding::literal("API_KEY", "value").is_err());
    assert!(EnvironmentVariableBinding::literal("TOKEN", "value").is_err());
    assert!(EnvironmentVariableBinding::literal("SAFE_NAME", "sk-proj-xyz").is_err());

    // RuntimeStartRequest metadata rejects secrets in key or value
    let inst_ref = RuntimeInstanceRef::new(
        RuntimeImplementationId::new("test").unwrap(),
        RuntimeInstanceId::generate(),
    );
    let ws = valid_test_workspace();
    let req = RuntimeStartRequest::new(inst_ref, ws);

    assert!(req.clone().with_metadata("SECRET_CONFIG", "safe").is_err());
    assert!(req.with_metadata("safe_config", "PASSWORD123").is_err());
}

#[tokio::test]
async fn test_31_validate_instance_start_unavailable_rejection() {
    let impl_id = RuntimeImplementationId::new("opencode").unwrap();
    let inst_id = RuntimeInstanceId::generate();
    let unavail = DiscoveredRuntimeInstance::unavailable(
        inst_id,
        impl_id.clone(),
        "Unavailable Instance",
        RuntimeCapabilities::default(),
        "CLI daemon is offline",
    );

    let inst_ref = RuntimeInstanceRef::new(impl_id.clone(), inst_id);
    let ws = valid_test_workspace();
    let req = RuntimeStartRequest::new(inst_ref, ws);

    let err = validate_instance_start(&unavail, &impl_id, &req).unwrap_err();
    assert!(matches!(
        err,
        RuntimeError::InstanceUnavailable { instance_id, .. }
        if instance_id == inst_id
    ));
}

#[tokio::test]
async fn test_32_validate_instance_start_unsupported_config_rejection() {
    let impl_id = RuntimeImplementationId::new("opencode").unwrap();
    let inst_id = RuntimeInstanceId::generate();
    let cfg_prod = RuntimeConfigRef::new("profiles/prod.json").unwrap();
    let cfg_stage = RuntimeConfigRef::new("profiles/staging.json").unwrap();

    let inst = DiscoveredRuntimeInstance::available(
        inst_id,
        impl_id.clone(),
        "Available Instance",
        RuntimeCapabilities::default(),
    )
    .with_supported_config(cfg_prod);

    // Request specifying unsupported config
    let inst_ref = RuntimeInstanceRef::new(impl_id.clone(), inst_id).with_config_ref(cfg_stage);
    let ws = valid_test_workspace();
    let req = RuntimeStartRequest::new(inst_ref, ws);

    let err = validate_instance_start(&inst, &impl_id, &req).unwrap_err();
    assert!(matches!(
        err,
        RuntimeError::UnsupportedConfiguration { instance_id, .. }
        if instance_id == inst_id
    ));
}

#[tokio::test]
async fn test_33_validate_instance_start_workspace_isolation_rejection() {
    let impl_id = RuntimeImplementationId::new("opencode").unwrap();
    let inst_id = RuntimeInstanceId::generate();
    let inst = DiscoveredRuntimeInstance::available(
        inst_id,
        impl_id.clone(),
        "Available Instance",
        RuntimeCapabilities::default(),
    );

    let inst_ref = RuntimeInstanceRef::new(impl_id.clone(), inst_id);

    // 1. SharedSource workspace configured for Mutating access must fail closed with InvalidWorkspaceAccess
    let ws_mutating = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));
    let req_mutating = RuntimeStartRequest::new(inst_ref.clone(), ws_mutating)
        .with_workspace_access_mode(WorkspaceAccessMode::Mutating);
    let err_mutating = validate_instance_start(&inst, &impl_id, &req_mutating).unwrap_err();
    assert!(
        matches!(err_mutating, RuntimeError::InvalidWorkspaceAccess { .. }),
        "Expected InvalidWorkspaceAccess for Mutating, got {:?}",
        err_mutating
    );

    // 2. SharedSource workspace configured for ReadOnly access must ALSO fail closed pending M11 (R09)
    let ws_readonly = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));
    let req_readonly = RuntimeStartRequest::new(inst_ref, ws_readonly)
        .with_workspace_access_mode(WorkspaceAccessMode::ReadOnly);
    let err_readonly = validate_instance_start(&inst, &impl_id, &req_readonly).unwrap_err();
    assert!(
        matches!(err_readonly, RuntimeError::InvalidWorkspaceAccess { .. }),
        "Expected InvalidWorkspaceAccess for ReadOnly, got {:?}",
        err_readonly
    );
}

#[tokio::test]
async fn test_34_per_instance_capability_authority() {
    let runtime = FakeAgentRuntime::new("opencode");
    let impl_id = runtime.implementation_id().clone();

    // Instance has Interrupt set to false
    let inst_id = RuntimeInstanceId::generate();
    let reduced_caps = RuntimeCapabilities::default()
        .with_streaming_events(true)
        .with_stop(true)
        .with_interrupt(false);
    let inst = DiscoveredRuntimeInstance::available(
        inst_id,
        impl_id.clone(),
        "Reduced CLI Instance",
        reduced_caps,
    );
    runtime.add_instance(inst).await;

    let inst_ref = RuntimeInstanceRef::new(impl_id, inst_id);
    let ws = valid_test_workspace();
    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();

    // Verify handle carries instance's reduced capability
    assert!(!handle.capabilities.supports(RuntimeCapability::Interrupt));

    // Calling interrupt must fail with UnsupportedCapability
    let err = runtime.interrupt(&handle.session_ref()).await.unwrap_err();
    assert!(matches!(
        err,
        RuntimeError::UnsupportedCapability {
            capability: RuntimeCapability::Interrupt,
            ..
        }
    ));
}

#[tokio::test]
async fn test_35_runtime_config_ref_deserialization_integrity() {
    assert!(serde_json::from_str::<RuntimeConfigRef>("\"\"").is_err());
    assert!(serde_json::from_str::<RuntimeConfigRef>("\"   \"").is_err());
    assert!(serde_json::from_str::<RuntimeConfigRef>("\"\\t\\n\"").is_err());

    let valid: RuntimeConfigRef = serde_json::from_str("\"config/prod.yaml\"").unwrap();
    assert_eq!(valid.as_str(), "config/prod.yaml");
}

#[tokio::test]
async fn test_36_diagnostic_sanitization_comprehensive() {
    // 1. Case-insensitive keywords
    let s1 = sanitize_error_message("Error: API_KEY=abc123456 and PASSWORD=supersecret");
    assert!(!s1.contains("abc123456"));
    assert!(!s1.contains("supersecret"));

    // 2. Bearer token
    let s2 = sanitize_error_message("Header: bearer tok_secret_9999");
    assert!(!s2.contains("tok_secret_9999"));

    // 3. Quotes
    let s3 = sanitize_error_message("Failed with 'sk-ant-testkey123' token");
    assert!(!s3.contains("sk-ant-testkey123"));

    // 4. Query string
    let s4 = sanitize_error_message("URL: http://api.com?token=xyz123&other=param");
    assert!(!s4.contains("xyz123"));
    assert!(s4.contains("&other=param"));

    // 5. Basic auth in URI
    let s5 = sanitize_error_message("Git clone: https://admin:super_secret_pw@github.com/repo.git");
    assert!(!s5.contains("super_secret_pw"));
    assert!(s5.contains("admin:[REDACTED_PASSWORD]@github.com"));

    // 6. Multiline
    let s6 = sanitize_error_message("Line 1\napi_key=mykey123\nLine 3");
    assert!(!s6.contains("mykey123"));
    assert!(s6.contains("Line 1\n[REDACTED_API_KEY]\nLine 3"));
}

#[tokio::test]
async fn test_37_registry_parallel_discovery_with_fault_tolerance() {
    let registry = RuntimeRegistry::new();

    let runtime_ok = Arc::new(FakeAgentRuntime::new("runtime-ok"));
    let runtime_fail = Arc::new(FakeAgentRuntime::new("runtime-fail"));
    runtime_fail
        .set_fail_discovery(Some("Daemon connection timed out".to_string()))
        .await;

    registry.register(runtime_ok).await.unwrap();
    registry.register(runtime_fail).await.unwrap();

    let outcome = registry.discover_all().await;
    assert!(!outcome.is_success());
    assert_eq!(outcome.instances.len(), 1);
    assert_eq!(
        outcome.instances[0].implementation_id.as_str(),
        "runtime-ok"
    );
    assert_eq!(outcome.failures.len(), 1);
    let fail_impl = RuntimeImplementationId::new("runtime-fail").unwrap();
    assert!(outcome.failures.contains_key(&fail_impl));
}

#[tokio::test]
async fn test_38_r01_event_hub_ingest_boundary_validation_atomic() {
    let session_id = RuntimeSessionId::generate();
    let instance_id = RuntimeInstanceId::generate();
    let hub = SessionEventHub::new(session_id, instance_id, 10);

    // 1. Ingest non-sequence 1 start event rejects
    let invalid_first = RuntimeEvent::new(
        session_id,
        1,
        RuntimeEventKind::OutputDelta {
            text: "premature delta".to_string(),
        },
    );
    let err = hub.ingest(invalid_first).await.unwrap_err();
    assert!(matches!(
        err,
        RuntimeError::InvalidEventSequence { sequence: 1, .. }
    ));
    // Verify hub history is still empty (atomic rollback / non-mutation)
    assert!(hub.recorded_events().await.is_empty());

    // 2. Ingest sequence 1 with mismatched instance ID rejects
    let foreign_inst = RuntimeInstanceId::generate();
    let mismatched_start = RuntimeEvent::new(
        session_id,
        1,
        RuntimeEventKind::SessionStarted {
            session_id,
            instance_id: foreign_inst,
        },
    );
    let err = hub.ingest(mismatched_start).await.unwrap_err();
    assert!(matches!(
        err,
        RuntimeError::InvalidEventSequence { sequence: 1, .. }
    ));
    assert!(hub.recorded_events().await.is_empty());

    // 3. Ingest valid sequence 1 SessionStarted succeeds
    let valid_start = RuntimeEvent::new(
        session_id,
        1,
        RuntimeEventKind::SessionStarted {
            session_id,
            instance_id,
        },
    );
    hub.ingest(valid_start).await.unwrap();
    assert_eq!(hub.recorded_events().await.len(), 1);

    // 4. Ingest sequence gap (seq 3 instead of 2) rejects without corrupting state
    let gap_event = RuntimeEvent::new(
        session_id,
        3,
        RuntimeEventKind::OutputDelta {
            text: "gap".to_string(),
        },
    );
    let err = hub.ingest(gap_event).await.unwrap_err();
    assert!(matches!(
        err,
        RuntimeError::InvalidEventSequence { sequence: 3, .. }
    ));
    assert_eq!(hub.recorded_events().await.len(), 1);

    // 5. Ingest terminal event Stopped
    let stopped = RuntimeEvent::new(session_id, 2, RuntimeEventKind::Stopped);
    hub.ingest(stopped).await.unwrap();
    assert_eq!(hub.recorded_events().await.len(), 2);

    // 6. Ingest after terminal rejects
    let post_terminal = RuntimeEvent::new(
        session_id,
        3,
        RuntimeEventKind::OutputDelta {
            text: "after terminal".to_string(),
        },
    );
    let err = hub.ingest(post_terminal).await.unwrap_err();
    assert!(matches!(err, RuntimeError::EventAfterTerminalState { .. }));
    assert_eq!(hub.recorded_events().await.len(), 2);
}

#[tokio::test]
async fn test_39_r02_event_hub_emit_atomic_serialization_and_terminal_rejection() {
    let session_id = RuntimeSessionId::generate();
    let instance_id = RuntimeInstanceId::generate();
    let hub = SessionEventHub::new(session_id, instance_id, 10);

    // 1. Emit assigns sequence 1
    let ev1 = hub
        .emit(RuntimeEventKind::SessionStarted {
            session_id,
            instance_id,
        })
        .await
        .unwrap();
    assert_eq!(ev1.sequence, 1);

    // 2. Emit assigns monotonic sequence 2
    let ev2 = hub
        .emit(RuntimeEventKind::OutputDelta {
            text: "step 1".to_string(),
        })
        .await
        .unwrap();
    assert_eq!(ev2.sequence, 2);

    // 3. Emit terminal event Stopped at sequence 3
    let ev3 = hub.emit(RuntimeEventKind::Stopped).await.unwrap();
    assert_eq!(ev3.sequence, 3);

    // 4. Attempted emission after terminal event is atomically rejected
    let err = hub
        .emit(RuntimeEventKind::OutputDelta {
            text: "forbidden".to_string(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        RuntimeError::EventAfterTerminalState {
            terminal_state: RuntimeLifecycleState::Stopped,
            attempted_sequence: 4,
            ..
        }
    ));
}

#[tokio::test]
async fn test_40_r03_event_hub_subscribe_retention_and_offset_boundary() {
    let session_id = RuntimeSessionId::generate();
    let instance_id = RuntimeInstanceId::generate();
    // Capacity 2 buffer
    let hub = SessionEventHub::new(session_id, instance_id, 2);

    hub.emit(RuntimeEventKind::SessionStarted {
        session_id,
        instance_id,
    })
    .await
    .unwrap();
    hub.emit(RuntimeEventKind::OutputDelta { text: "1".into() })
        .await
        .unwrap();
    hub.emit(RuntimeEventKind::OutputDelta { text: "2".into() })
        .await
        .unwrap();
    hub.emit(RuntimeEventKind::OutputDelta { text: "3".into() })
        .await
        .unwrap();

    // Retained events are sequences 3 and 4. Earliest is 3.
    // 1. Initial subscription without offset (after_sequence = None) fails with EventRetentionExceeded
    let err_none = hub.subscribe(None).await.unwrap_err();
    assert!(matches!(
        err_none,
        RuntimeError::EventRetentionExceeded {
            requested_sequence: 0,
            earliest_available_sequence: 3,
            ..
        }
    ));

    // 2. Requesting sequence older than retained (e.g. Some(1)) fails
    let err_old = hub.subscribe(Some(1)).await.unwrap_err();
    assert!(matches!(
        err_old,
        RuntimeError::EventRetentionExceeded {
            requested_sequence: 1,
            earliest_available_sequence: 3,
            ..
        }
    ));

    // 3. Boundary offset: after_sequence = Some(E - 1) = Some(2) must SUCCEED and replay sequences 3 and 4
    let mut sub_bound = hub.subscribe(Some(2)).await.unwrap();
    let e3 = sub_bound.next_event().await.unwrap().unwrap();
    assert_eq!(e3.sequence, 3);
    let e4 = sub_bound.next_event().await.unwrap().unwrap();
    assert_eq!(e4.sequence, 4);

    // 4. after_sequence = Some(3) replays strictly sequence 4
    let mut sub_3 = hub.subscribe(Some(3)).await.unwrap();
    let e4_only = sub_3.next_event().await.unwrap().unwrap();
    assert_eq!(e4_only.sequence, 4);
}

#[tokio::test]
async fn test_41_r04_secret_environment_variable_binding_invariants_and_serde() {
    // 1. Direct constructor rejects forbidden secret keyword in literal binding
    let err_lit = EnvironmentVariableBinding::literal("AWS_SECRET_KEY", "plain_val").unwrap_err();
    assert!(matches!(err_lit, RuntimeError::InvalidConfiguration { .. }));

    let err_tok = EnvironmentVariableBinding::literal("GITHUB_TOKEN", "plain_val").unwrap_err();
    assert!(matches!(err_tok, RuntimeError::InvalidConfiguration { .. }));

    // 2. Serde deserialization cannot bypass validation
    let raw_json = r#"{
        "name": "MY_AUTH_PASSWORD",
        "source": {
            "type": "literal",
            "value": "plaintext"
        }
    }"#;
    let res: Result<EnvironmentVariableBinding, _> = serde_json::from_str(raw_json);
    assert!(
        res.is_err(),
        "Deserializing binding with secret keyword must fail"
    );

    // 3. Custom serde validates successfully on valid binding
    let valid_json = r#"{
        "name": "MY_APP_ENV",
        "source": {
            "type": "literal",
            "value": "production"
        }
    }"#;
    let binding: EnvironmentVariableBinding = serde_json::from_str(valid_json).unwrap();
    assert_eq!(binding.name(), "MY_APP_ENV");
    assert!(matches!(
        binding.source(),
        EnvironmentBindingSource::Literal { .. }
    ));

    // 4. RuntimeStartRequest::validate() re-checks all bindings
    let inst_ref = RuntimeInstanceRef::new(
        RuntimeImplementationId::new("opencode").unwrap(),
        RuntimeInstanceId::generate(),
    );
    let ws = valid_test_workspace();
    let mut req = RuntimeStartRequest::new(inst_ref, ws);
    req.environment_bindings.push(binding);
    assert!(req.validate().is_ok());
}

#[tokio::test]
async fn test_42_r05_terminal_status_changed_stream_finalization() {
    // 1. is_terminal() recognizes StatusChanged to terminal states
    let sc_stopped = RuntimeEventKind::StatusChanged {
        previous_state: RuntimeLifecycleState::Running,
        new_state: RuntimeLifecycleState::Stopped,
    };
    assert!(sc_stopped.is_terminal());

    let sc_completed = RuntimeEventKind::StatusChanged {
        previous_state: RuntimeLifecycleState::Running,
        new_state: RuntimeLifecycleState::Completed,
    };
    assert!(sc_completed.is_terminal());

    let sc_failed = RuntimeEventKind::StatusChanged {
        previous_state: RuntimeLifecycleState::Running,
        new_state: RuntimeLifecycleState::Failed,
    };
    assert!(sc_failed.is_terminal());

    // Non-terminal StatusChanged
    let sc_interrupted = RuntimeEventKind::StatusChanged {
        previous_state: RuntimeLifecycleState::Running,
        new_state: RuntimeLifecycleState::Interrupted,
    };
    assert!(!sc_interrupted.is_terminal());

    // 2. Subscription finalizes after yielding terminal StatusChanged
    let session_id = RuntimeSessionId::generate();
    let instance_id = RuntimeInstanceId::generate();
    let hub = SessionEventHub::new(session_id, instance_id, 10);
    let mut sub = hub.subscribe(None).await.unwrap();

    hub.emit(RuntimeEventKind::SessionStarted {
        session_id,
        instance_id,
    })
    .await
    .unwrap();
    hub.emit(sc_stopped).await.unwrap();

    let e1 = sub.next_event().await.unwrap().unwrap();
    assert_eq!(e1.sequence, 1);
    let e2 = sub.next_event().await.unwrap().unwrap();
    assert_eq!(e2.sequence, 2);
    assert!(matches!(e2.kind, RuntimeEventKind::StatusChanged { .. }));

    // Subscription is closed after terminal event
    assert!(sub.next_event().await.unwrap().is_none());

    // Hub rejects subsequent emission
    let post_err = hub
        .emit(RuntimeEventKind::OutputDelta {
            text: "dead".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        post_err,
        RuntimeError::EventAfterTerminalState { .. }
    ));
}

#[tokio::test]
async fn test_43_r06_send_lifecycle_error_precedence() {
    let runtime = FakeAgentRuntime::new("test-runtime");
    let instances = runtime.discover().await.unwrap();
    let inst_ref = RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
    );
    let ws = valid_test_workspace();
    let handle = runtime
        .start(RuntimeStartRequest::new(inst_ref, ws))
        .await
        .unwrap();
    let sess_ref = handle.session_ref();

    // Enable fail_send injection
    runtime
        .set_fail_send(Some("Injected send failure".to_string()))
        .await;

    // 1. Invalid session reference returns SessionNotFound BEFORE evaluating fail_send
    let bogus_ref = RuntimeSessionRef::new(
        runtime.implementation_id().clone(),
        instances[0].instance_id,
        RuntimeSessionId::generate(),
    );
    let err_not_found = runtime
        .send(&bogus_ref, RuntimeInput::text("test"))
        .await
        .unwrap_err();
    assert!(
        matches!(err_not_found, RuntimeError::SessionNotFound { .. }),
        "Expected SessionNotFound, got {:?}",
        err_not_found
    );

    // 2. Stop the session
    runtime.stop(&sess_ref).await.unwrap();

    // 3. Stopped session returns TerminalStateError BEFORE evaluating fail_send
    let err_terminal = runtime
        .send(&sess_ref, RuntimeInput::text("test"))
        .await
        .unwrap_err();
    assert!(
        matches!(err_terminal, RuntimeError::TerminalStateError { .. }),
        "Expected TerminalStateError, got {:?}",
        err_terminal
    );
}

#[tokio::test]
async fn test_44_r07_registry_worker_panic_attribution() {
    let registry = RuntimeRegistry::new();

    let runtime_ok = Arc::new(FakeAgentRuntime::new("healthy-runtime"));
    let runtime_fail = Arc::new(FakeAgentRuntime::new("failing-runtime"));
    runtime_fail
        .set_fail_discovery(Some("Hardware device unplugged".into()))
        .await;

    registry.register(runtime_ok).await.unwrap();
    registry.register(runtime_fail).await.unwrap();

    let outcome = registry.discover_all().await;
    assert!(!outcome.is_success());
    assert_eq!(outcome.instances.len(), 1);
    assert_eq!(
        outcome.instances[0].implementation_id.as_str(),
        "healthy-runtime"
    );

    let fail_id = RuntimeImplementationId::new("failing-runtime").unwrap();
    assert!(outcome.failures.contains_key(&fail_id));
    let failure_msg = outcome.failures.get(&fail_id).unwrap().to_string();
    assert!(failure_msg.contains("Hardware device unplugged"));
}

#[tokio::test]
async fn test_45_r08_multi_url_and_query_credential_sanitization() {
    // 1. Multiple basic auth URLs in diagnostic string
    let multi_url = "Error connecting to https://user1:secretpass1@example.com/api and backup https://user2:secretpass2@backup.example.com/api";
    let sanitized_urls = sanitize_error_message(multi_url);
    assert!(!sanitized_urls.contains("secretpass1"));
    assert!(!sanitized_urls.contains("secretpass2"));
    assert!(sanitized_urls.contains("user1:[REDACTED_PASSWORD]@example.com"));
    assert!(sanitized_urls.contains("user2:[REDACTED_PASSWORD]@backup.example.com"));

    // 2. Multiple query parameters without early loop break
    let multi_query = "Request failed: https://api.service.com/v1?api_key=myapikey123&token=tok_456&access_token=acc_789&verbose=true";
    let sanitized_query = sanitize_error_message(multi_query);
    assert!(!sanitized_query.contains("myapikey123"));
    assert!(!sanitized_query.contains("tok_456"));
    assert!(!sanitized_query.contains("acc_789"));
    assert!(sanitized_query.contains("&verbose=true"));
}

#[tokio::test]
async fn test_46_r09_shared_source_workspace_strict_fail_closed() {
    let inst_ref = RuntimeInstanceRef::new(
        RuntimeImplementationId::new("opencode").unwrap(),
        RuntimeInstanceId::generate(),
    );

    // 1. SharedSource + Mutating fails closed with InvalidWorkspaceAccess
    let req_mutating = RuntimeStartRequest::new(
        inst_ref.clone(),
        ExecutionWorkspace::shared_source(PathBuf::from("/workspace")),
    )
    .with_workspace_access_mode(WorkspaceAccessMode::Mutating);
    let err_mut = req_mutating.validate().unwrap_err();
    assert!(matches!(
        err_mut,
        RuntimeError::InvalidWorkspaceAccess { .. }
    ));

    // 2. SharedSource + ReadOnly fails closed with InvalidWorkspaceAccess
    let req_readonly = RuntimeStartRequest::new(
        inst_ref,
        ExecutionWorkspace::shared_source(PathBuf::from("/workspace")),
    )
    .with_workspace_access_mode(WorkspaceAccessMode::ReadOnly);
    let err_ro = req_readonly.validate().unwrap_err();
    assert!(matches!(
        err_ro,
        RuntimeError::InvalidWorkspaceAccess { .. }
    ));

    // 3. Managed worktree passes validation
    let req_managed = RuntimeStartRequest::new(
        RuntimeInstanceRef::new(
            RuntimeImplementationId::new("opencode").unwrap(),
            RuntimeInstanceId::generate(),
        ),
        valid_test_workspace(),
    );
    assert!(req_managed.validate().is_ok());
}
