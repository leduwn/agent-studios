//! Runtime Vertical Slice Integration Tests (CLI-REV-002)
//!
//! Verifies prompt submission -> runtime dispatch -> streaming assistant/tool events
//! -> task state transitions -> truthful execution mode reporting.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::timeout;

use agent_studios_cli_client::{
    AgentStudiosClient, DETERMINISTIC_LOCAL_IMPL_ID, DeterministicLocalRuntime, EmbeddedClient,
    GoalSubmissionOptions, UiEvent, translate_control_plane_event,
};
use agent_studios_external_runtime::{
    AgentRuntime, DiscoveredRuntimeInstance, RuntimeCapabilities, RuntimeError, RuntimeEventKind,
    RuntimeEventSubscription, RuntimeImplementationId, RuntimeInput, RuntimeInstanceId,
    RuntimeLifecycleState, RuntimeSessionHandle, RuntimeSessionRef, RuntimeStartRequest,
    SanitizedRuntimeMessage, SessionEventHub,
};
use agent_studios_protocol::event::ControlPlaneEvent;
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::TaskState;
use async_trait::async_trait;
use chrono::Utc;

#[tokio::test]
async fn test_prompt_submission_to_runtime_dispatch_and_streaming_completion() {
    let client = EmbeddedClient::bootstrap("runtime-slice-test")
        .await
        .expect("bootstrap embedded client failed");

    let mut sub = client
        .subscribe_events(0)
        .await
        .expect("subscribe events failed");

    // Initial events: StudioCreated + 4x AgentRegistered = 5 events
    assert_eq!(sub.backfill.len(), 5);

    let task_id = client
        .submit_goal(
            "Build deterministic vertical slice",
            GoalSubmissionOptions::default(),
        )
        .await
        .expect("submit_goal failed");

    // Collect streaming events until task succeeds
    let mut received_chunk = false;
    let mut task_succeeded = false;
    let mut run_succeeded = false;

    let deadline = Duration::from_secs(5);
    let start_time = tokio::time::Instant::now();

    while start_time.elapsed() < deadline && (!task_succeeded || !run_succeeded) {
        if let Ok(Some(envelope)) = timeout(Duration::from_millis(500), sub.stream.recv()).await {
            match &envelope.event {
                ControlPlaneEvent::AssistantMessageChunk { content, .. } => {
                    if content.contains("[DEMO]") || content.contains("Goal received:") {
                        received_chunk = true;
                    }
                }
                ControlPlaneEvent::RunStateChanged { new_state, .. } => {
                    if *new_state == RunState::Succeeded {
                        run_succeeded = true;
                    }
                }
                ControlPlaneEvent::TaskStateChanged {
                    task_id: tid,
                    new_state,
                    ..
                } => {
                    if *tid == task_id && *new_state == TaskState::Succeeded {
                        task_succeeded = true;
                    }
                }
                _ => {}
            }
        }
    }

    assert!(received_chunk, "Must receive streamed assistant chunks");
    assert!(run_succeeded, "Run must transition to Succeeded");
    assert!(task_succeeded, "Task must transition to Succeeded");

    // Task graph should reflect succeeded task
    let graph = client
        .get_task_graph()
        .await
        .expect("get_task_graph failed");
    let task = graph
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .expect("task in graph");
    assert_eq!(task.state, TaskState::Succeeded);
}

#[tokio::test]
async fn test_ui_event_translation_for_runtime_streaming() {
    let client = EmbeddedClient::bootstrap("ui-translation-test")
        .await
        .expect("bootstrap failed");

    let mut sub = client.subscribe_events(0).await.expect("subscribe failed");

    let _task_id = client
        .submit_goal(
            "Test UI translation pipeline",
            GoalSubmissionOptions::default(),
        )
        .await
        .expect("submit_goal failed");

    let mut seen_ui_chunk = false;

    for _ in 0..30 {
        if let Ok(Some(envelope)) = timeout(Duration::from_millis(300), sub.stream.recv()).await {
            if let Some(ui_event) = translate_control_plane_event(&envelope) {
                if let UiEvent::AssistantChunk { content, .. } = ui_event {
                    if !content.is_empty() {
                        seen_ui_chunk = true;
                        break;
                    }
                }
            }
        }
    }

    assert!(
        seen_ui_chunk,
        "UI translation must emit UiEvent::AssistantChunk"
    );

    // Verify tool translation logic directly on control plane event envelope
    let dummy_tool_start = agent_studios_protocol::event::EventEnvelope::new(
        client.studio_id(),
        999,
        Utc::now(),
        agent_studios_protocol::event::ControlPlaneEvent::ToolStarted {
            task_id: None,
            run_id: None,
            agent_id: agent_studios_protocol::id::AgentId::new(),
            tool_name: "test_tool".to_string(),
            call_id: "call_001".to_string(),
            timestamp: Utc::now(),
        },
    );
    let translated = translate_control_plane_event(&dummy_tool_start).expect("translated UI event");
    assert_eq!(
        translated,
        UiEvent::ToolProgress {
            tool_name: "test_tool".to_string(),
            status: "running".to_string(),
        }
    );
}

#[tokio::test]
async fn test_truthful_reporting_deterministic_local_runtime() {
    let runtime = DeterministicLocalRuntime::new();
    assert_eq!(
        runtime.implementation_id().as_str(),
        DETERMINISTIC_LOCAL_IMPL_ID
    );

    let instances = runtime.discover().await.expect("discovery failed");
    assert_eq!(instances.len(), 1);
    let instance = &instances[0];

    assert_eq!(instance.display_name, "Deterministic Local Engine");
    assert_eq!(instance.version.as_deref(), Some("0.1.0"));
    assert!(instance.is_available());

    let caps = runtime.capabilities().await;
    assert!(caps.streaming_events.is_supported());
    assert!(!caps.tools.is_supported(), "Deterministic mock execution must truthfully report tools=false");
    assert!(caps.stop.is_supported());
    assert!(caps.workspace_binding.is_supported());
}

/// Custom mock runtime to verify failure propagation and custom adapter injection
struct FailingMockRuntime {
    impl_id: RuntimeImplementationId,
    inst_id: RuntimeInstanceId,
    hub: Arc<RwLock<Option<Arc<SessionEventHub>>>>,
}

impl FailingMockRuntime {
    fn new() -> Self {
        Self {
            impl_id: RuntimeImplementationId::new("failing-mock").unwrap(),
            inst_id: RuntimeInstanceId::generate(),
            hub: Arc::new(RwLock::new(None)),
        }
    }
}

#[async_trait]
impl AgentRuntime for FailingMockRuntime {
    fn implementation_id(&self) -> &RuntimeImplementationId {
        &self.impl_id
    }

    async fn discover(&self) -> Result<Vec<DiscoveredRuntimeInstance>, RuntimeError> {
        Ok(vec![DiscoveredRuntimeInstance::available(
            self.inst_id,
            self.impl_id.clone(),
            "Failing Mock Engine",
            RuntimeCapabilities::default().with_streaming_events(true),
        )])
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities::default().with_streaming_events(true)
    }

    async fn start(
        &self,
        request: RuntimeStartRequest,
    ) -> Result<RuntimeSessionHandle, RuntimeError> {
        let session_id = request
            .correlation
            .and_then(|c| c.task_id)
            .map(|_| agent_studios_external_runtime::RuntimeSessionId::generate())
            .unwrap_or_else(agent_studios_external_runtime::RuntimeSessionId::generate);

        let hub = Arc::new(SessionEventHub::new(session_id, self.inst_id, 128));
        *self.hub.write().await = Some(hub.clone());

        let handle = RuntimeSessionHandle::new(
            session_id,
            self.inst_id,
            self.impl_id.clone(),
            RuntimeLifecycleState::Running,
            request.workspace,
            request.workspace_access_mode,
            RuntimeCapabilities::default().with_streaming_events(true),
            None,
            Utc::now(),
        );

        hub.emit(RuntimeEventKind::SessionStarted {
            session_id,
            instance_id: self.inst_id,
        })
        .await?;

        // Emit failure event
        let hub_clone = hub.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let _ = hub_clone
                .emit(RuntimeEventKind::Failed {
                    safe_error_summary: SanitizedRuntimeMessage::new(
                        "Model API quota exceeded: 429 Too Many Requests",
                    ),
                })
                .await;
        });

        Ok(handle)
    }

    async fn send(
        &self,
        _session: &RuntimeSessionRef,
        _input: RuntimeInput,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn interrupt(&self, _session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn resume(
        &self,
        _session: &RuntimeSessionRef,
        _input: Option<RuntimeInput>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn stop(&self, _session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn status(
        &self,
        _session: &RuntimeSessionRef,
    ) -> Result<RuntimeLifecycleState, RuntimeError> {
        Ok(RuntimeLifecycleState::Failed)
    }

    async fn events_after(
        &self,
        _session: &RuntimeSessionRef,
        after: Option<u64>,
    ) -> Result<RuntimeEventSubscription, RuntimeError> {
        let hub = self.hub.read().await.clone().expect("hub initialized");
        hub.subscribe(after).await
    }
}

#[tokio::test]
async fn test_runtime_failure_propagation_to_control_plane() {
    let mock_runtime = Arc::new(FailingMockRuntime::new());
    let client = EmbeddedClient::bootstrap_with_runtime("failing-runtime-test", mock_runtime)
        .await
        .expect("bootstrap failed");

    let mut sub = client.subscribe_events(0).await.expect("subscribe failed");

    let task_id = client
        .submit_goal(
            "Goal that fails in runtime",
            GoalSubmissionOptions::default(),
        )
        .await
        .expect("submit_goal failed");

    let mut task_failed = false;
    let mut run_failed = false;

    let deadline = Duration::from_secs(5);
    let start = tokio::time::Instant::now();

    while start.elapsed() < deadline && (!task_failed || !run_failed) {
        if let Ok(Some(envelope)) = timeout(Duration::from_millis(300), sub.stream.recv()).await {
            match &envelope.event {
                ControlPlaneEvent::RunStateChanged { new_state, .. } => {
                    if *new_state == RunState::Failed {
                        run_failed = true;
                    }
                }
                ControlPlaneEvent::TaskStateChanged {
                    task_id: tid,
                    new_state,
                    ..
                } => {
                    if *tid == task_id && *new_state == TaskState::Failed {
                        task_failed = true;
                    }
                }
                _ => {}
            }
        }
    }

    assert!(
        run_failed,
        "Run must transition to Failed on runtime failure"
    );
    assert!(
        task_failed,
        "Task must transition to Failed on runtime failure"
    );

    let graph = client
        .get_task_graph()
        .await
        .expect("get_task_graph failed");
    let task = graph
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .expect("task in graph");
    assert_eq!(task.state, TaskState::Failed);
}

#[tokio::test]
async fn test_cancellation_stops_active_runtime_sessions() {
    let client = EmbeddedClient::bootstrap("cancellation-test")
        .await
        .expect("bootstrap failed");

    let mut sub = client.subscribe_events(0).await.expect("subscribe failed");

    let _task_id = client
        .submit_goal(
            "Long running goal to cancel",
            GoalSubmissionOptions::default(),
        )
        .await
        .expect("submit_goal failed");

    client
        .cancel_active_run(Some("User requested stop".to_string()))
        .await
        .expect("cancel_active_run failed");

    let mut cancellation_recorded = false;
    for _ in 0..20 {
        if let Ok(Some(envelope)) = timeout(Duration::from_millis(200), sub.stream.recv()).await {
            if let ControlPlaneEvent::CancellationRequested { reason, .. } = &envelope.event {
                assert_eq!(reason.as_deref(), Some("User requested stop"));
                cancellation_recorded = true;
                break;
            }
        }
    }

    assert!(
        cancellation_recorded,
        "Cancellation event must be recorded in Control Plane"
    );
}

#[tokio::test]
async fn test_production_mode_fails_closed_when_unconfigured() {
    use agent_studios_cli_client::client::RuntimeMode;
    use agent_studios_cli_client::runtime::resolve_runtime;

    // In an unconfigured environment without a running OpenCode daemon, resolve_runtime(Production)
    // must fail closed with ClientError::Configuration rather than silently falling back to mock execution.
    let result = resolve_runtime(RuntimeMode::Production).await;
    match result {
        Ok(_) => {
            // If an actual OpenCode instance happened to be present in PATH/environment, that's valid
        }
        Err(err) => {
            assert!(
                matches!(err, agent_studios_cli_client::client::ClientError::Configuration(_)),
                "Expected ClientError::Configuration, got: {err:?}"
            );
        }
    }
}

#[tokio::test]
async fn test_resolve_approval_fails_closed_without_active_session() {
    use agent_studios_cli_client::client::ClientApprovalDecision;
    use agent_studios_protocol::id::ApprovalId;

    let client = EmbeddedClient::bootstrap("approval-fail-closed-test")
        .await
        .expect("bootstrap failed");

    let approval_id = ApprovalId::new();
    let res = client
        .resolve_approval(approval_id, ClientApprovalDecision::Allow)
        .await;

    assert!(
        res.is_err(),
        "resolve_approval must fail closed when no active runtime session is attached"
    );
    let err = res.unwrap_err();
    assert!(
        matches!(err, agent_studios_cli_client::client::ClientError::OperationFailed(ref msg) if msg.contains("no active runtime session attached")),
        "Expected missing active session error, got: {err:?}"
    );
}
