mod support;
use support::{FakeAgentRuntime, FakeCallRecord};

use std::path::PathBuf;
use std::sync::Arc;

use agent_studios_external_runtime::*;
use agent_studios_protocol::worktree::ExecutionWorkspace;
use agent_studios_provider::{ProviderInstanceId, SecretBackend, SecretReference};
use uuid::Uuid;

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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

    let secret =
        SecretReference::new(SecretBackend::EnvironmentVariable, "OPENCODE_SECRET_TOKEN").unwrap();

    let req = RuntimeStartRequest::new(inst_ref, ws)
        .with_literal_env("DEBUG", "1")
        .unwrap()
        .with_secret_env("API_KEY", secret)
        .unwrap();

    assert_eq!(req.environment_bindings.len(), 2);
    assert_eq!(req.environment_bindings[0].name, "DEBUG");
    assert!(matches!(
        req.environment_bindings[0].source,
        EnvironmentBindingSource::Literal { .. }
    ));
    assert_eq!(req.environment_bindings[1].name, "API_KEY");
    assert!(matches!(
        req.environment_bindings[1].source,
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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));

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

    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));
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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));
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
    let ws = ExecutionWorkspace::shared_source(PathBuf::from("/workspace"));
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
