use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent_studios_external_runtime::{
    AgentRuntime, RuntimeCapabilitySupport, RuntimeEventKind, RuntimeInput, RuntimeInstanceRef,
    RuntimeLifecycleState, RuntimeStartRequest, WorkspaceAccessMode,
};
use agent_studios_opencode_runtime::OpenCodeRuntime;
use agent_studios_protocol::id::WorktreeId;
use agent_studios_protocol::worktree::ExecutionWorkspace;
use agent_studios_provider::ProviderCatalog;
use agent_studios_provider::id::{ModelId, ProviderInstanceId};
use agent_studios_provider::model::ModelRef;
use tempfile::tempdir;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_opencode_runtime_discovery_and_capabilities() {
    let runtime = OpenCodeRuntime::new().unwrap();

    let caps = runtime.capabilities().await;
    assert_eq!(caps.streaming_events, RuntimeCapabilitySupport::Supported);
    assert_eq!(caps.interrupt, RuntimeCapabilitySupport::Supported);
    assert_eq!(caps.workspace_binding, RuntimeCapabilitySupport::Supported);
}

#[tokio::test]
async fn test_opencode_runtime_full_lifecycle_with_mock_server() {
    let mock_server = MockServer::start().await;
    let server_url = Url::parse(&mock_server.uri()).unwrap();

    // 1. Mock /global/health
    Mock::given(method("GET"))
        .and(path("/global/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok",
            "version": "1.0.0"
        })))
        .mount(&mock_server)
        .await;

    // 2. Mock POST /session
    Mock::given(method("POST"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "opencode-sess-xyz",
            "status": "ready"
        })))
        .mount(&mock_server)
        .await;

    // 3. Mock GET /event (SSE stream)
    let sse_body = "event: output.delta\ndata: {\"text\": \"Starting operation...\"}\n\n\
                    event: tool.started\ndata: {\"tool\": \"bash\", \"call_id\": \"c1\"}\n\n\
                    event: tool.completed\ndata: {\"tool\": \"bash\", \"call_id\": \"c1\", \"duration_ms\": 10, \"success\": true}\n\n\
                    event: turn.completed\ndata: {\"summary\": \"Finished successfully\"}\n\n";

    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body),
        )
        .mount(&mock_server)
        .await;

    // 4. Mock POST /session/opencode-sess-xyz/message
    Mock::given(method("POST"))
        .and(path("/session/opencode-sess-xyz/message"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "accepted"
        })))
        .mount(&mock_server)
        .await;

    // 5. Mock POST /session/opencode-sess-xyz/abort
    Mock::given(method("POST"))
        .and(path("/session/opencode-sess-xyz/abort"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "aborted"
        })))
        .mount(&mock_server)
        .await;

    let catalog = Arc::new(ProviderCatalog::with_builtin_definitions());
    let runtime = OpenCodeRuntime::new()
        .unwrap()
        .with_catalog(catalog)
        .with_mock_server(server_url, "test-token");

    let instances = runtime.discover().await.unwrap();
    assert_eq!(instances.len(), 1);
    let instance = &instances[0];
    assert_eq!(instance.implementation_id.as_str(), "opencode");

    // Prepare workspace
    let temp = tempdir().unwrap();
    let temp_path = temp.path().to_path_buf();
    let workspace = ExecutionWorkspace::managed(
        WorktreeId::new(),
        temp_path.clone(),
        temp_path.clone(),
        PathBuf::from("/source"),
        PathBuf::from("/source"),
        "0123456789abcdef0123456789abcdef01234567",
    );

    let model_ref = ModelRef::new(
        ProviderInstanceId::new(),
        ModelId::new("claude-3-7-sonnet").unwrap(),
    );

    let instance_ref =
        RuntimeInstanceRef::new(instance.implementation_id.clone(), instance.instance_id);

    let start_req = RuntimeStartRequest::new(instance_ref, workspace)
        .with_workspace_access_mode(WorkspaceAccessMode::Mutating)
        .with_model_ref(model_ref)
        .with_initial_prompt("Hello OpenCode");

    // Start session
    let handle = runtime.start(start_req).await.expect("start failed");
    let session_ref = handle.session_ref();
    assert_eq!(handle.state, RuntimeLifecycleState::Running);

    // Give background SSE loop time to ingest events
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify events via subscription
    let mut sub = runtime.events_after(&session_ref, None).await.unwrap();
    let mut received = Vec::new();
    while let Ok(Some(ev)) = sub.next_event().await {
        received.push(ev);
        if received.len() >= 4 {
            break;
        }
    }

    assert!(
        received.len() >= 4,
        "Expected at least 4 events, got {}",
        received.len()
    );

    // Event 1 must strictly be SessionStarted
    assert_eq!(received[0].sequence, 1);
    assert!(matches!(
        received[0].kind,
        RuntimeEventKind::SessionStarted { .. }
    ));

    // Verify subsequent event kinds
    assert_eq!(received[1].sequence, 2);
    assert!(matches!(
        received[1].kind,
        RuntimeEventKind::OutputDelta { .. }
    ));

    assert_eq!(received[2].sequence, 3);
    assert!(matches!(
        received[2].kind,
        RuntimeEventKind::ToolStarted { .. }
    ));

    assert_eq!(received[3].sequence, 4);
    assert!(matches!(
        received[3].kind,
        RuntimeEventKind::ToolCompleted { .. }
    ));

    // Send input
    let input = RuntimeInput::text("Next instruction");
    let send_res = runtime.send(&session_ref, input).await;
    assert!(send_res.is_ok(), "send failed: {:?}", send_res.err());

    // Interrupt session
    let int_res = runtime.interrupt(&session_ref).await;
    assert!(int_res.is_ok(), "interrupt failed: {:?}", int_res.err());

    // Check status
    let status = runtime.status(&session_ref).await.unwrap();
    assert!(
        status == RuntimeLifecycleState::Running || status == RuntimeLifecycleState::Interrupted
    );

    // Stop session
    let stop_res = runtime.stop(&session_ref).await;
    assert!(stop_res.is_ok(), "stop failed: {:?}", stop_res.err());
}
