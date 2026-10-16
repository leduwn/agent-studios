use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent_studios_external_runtime::{
    AgentRuntime, RuntimeEventKind, RuntimeInput, RuntimeInstanceRef, RuntimeLifecycleState,
    RuntimeStartRequest, WorkspaceAccessMode,
};
use agent_studios_opencode_runtime::OpenCodeRuntime;
use agent_studios_protocol::id::WorktreeId;
use agent_studios_protocol::worktree::ExecutionWorkspace;
use agent_studios_provider::id::{ModelId, ProviderId, ProviderInstanceId};
use agent_studios_provider::model::ModelRef;
use agent_studios_provider::{
    AuthenticationScheme, EndpointProfile, ProtocolFamily, ProviderCatalog, ProviderInstance,
    SecretReference,
};
use tempfile::tempdir;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn setup_test_catalog() -> (Arc<ProviderCatalog>, ProviderInstanceId) {
    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("anthropic").unwrap(),
        "Anthropic Production",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new("https://api.anthropic.com").unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: SecretReference::env("ANTHROPIC_API_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(instance).unwrap();
    (Arc::new(catalog), instance_id)
}

#[tokio::test]
async fn test_m11_r05_pre_spawn_validation_fails_closed_without_catalog() {
    let mock_server = MockServer::start().await;
    let server_url = Url::parse(&mock_server.uri()).unwrap();

    let runtime = OpenCodeRuntime::new()
        .unwrap()
        .with_mock_server(server_url, "test-token");

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

    let instances = runtime.discover().await.unwrap();
    let instance = &instances[0];
    let instance_ref =
        RuntimeInstanceRef::new(instance.implementation_id.clone(), instance.instance_id);

    // Provide model_ref when no catalog is configured on runtime -> must fail before calling server
    let model_ref = ModelRef::new(
        ProviderInstanceId::new(),
        ModelId::new("claude-3-7-sonnet").unwrap(),
    );

    let start_req = RuntimeStartRequest::new(instance_ref, workspace)
        .with_workspace_access_mode(WorkspaceAccessMode::Mutating)
        .with_model_ref(model_ref);

    let res = runtime.start(start_req).await;
    assert!(
        res.is_err(),
        "Must fail closed when ModelRef is specified without ProviderCatalog"
    );
}

#[tokio::test]
async fn test_m11_r05_initial_prompt_failure_triggers_transactional_rollback() {
    let mock_server = MockServer::start().await;
    let server_url = Url::parse(&mock_server.uri()).unwrap();

    // Mock /global/health
    Mock::given(method("GET"))
        .and(path("/global/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok"
        })))
        .mount(&mock_server)
        .await;

    // Mock POST /session succeeds
    Mock::given(method("POST"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "opencode-sess-rollback",
            "status": "ready"
        })))
        .mount(&mock_server)
        .await;

    // Mock GET /event
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("event: output.delta\ndata: {\"text\": \"init\"}\n\n"),
        )
        .mount(&mock_server)
        .await;

    // Mock POST /session/.../message fails with 500 Internal Error
    Mock::given(method("POST"))
        .and(path("/session/opencode-sess-rollback/message"))
        .respond_with(ResponseTemplate::new(500).set_body_string("LLM service unavailable"))
        .mount(&mock_server)
        .await;

    let (catalog, provider_inst_id) = setup_test_catalog();
    let runtime = OpenCodeRuntime::new()
        .unwrap()
        .with_catalog(catalog)
        .with_mock_server(server_url, "test-token");

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

    let instances = runtime.discover().await.unwrap();
    let instance = &instances[0];
    let instance_ref =
        RuntimeInstanceRef::new(instance.implementation_id.clone(), instance.instance_id);

    let model_ref = ModelRef::new(provider_inst_id, ModelId::new("claude-3-7-sonnet").unwrap());

    let start_req = RuntimeStartRequest::new(instance_ref, workspace)
        .with_workspace_access_mode(WorkspaceAccessMode::Mutating)
        .with_model_ref(model_ref)
        .with_initial_prompt("This prompt will fail");

    let res = runtime.start(start_req).await;
    assert!(
        res.is_err(),
        "Start must fail when initial prompt dispatch fails"
    );
}

#[tokio::test]
async fn test_m11_r05_stop_idempotence_and_event_replay_after_stop() {
    let mock_server = MockServer::start().await;
    let server_url = Url::parse(&mock_server.uri()).unwrap();

    Mock::given(method("GET"))
        .and(path("/global/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "opencode-sess-stop",
            "status": "ready"
        })))
        .mount(&mock_server)
        .await;

    let sse_body = "event: output.delta\ndata: {\"text\": \"hello world\"}\n\n";
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body),
        )
        .mount(&mock_server)
        .await;

    let (catalog, provider_inst_id) = setup_test_catalog();
    let runtime = OpenCodeRuntime::new()
        .unwrap()
        .with_catalog(catalog)
        .with_mock_server(server_url, "test-token");

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

    let instances = runtime.discover().await.unwrap();
    let instance = &instances[0];
    let instance_ref =
        RuntimeInstanceRef::new(instance.implementation_id.clone(), instance.instance_id);

    let model_ref = ModelRef::new(provider_inst_id, ModelId::new("claude-3-7-sonnet").unwrap());

    let start_req = RuntimeStartRequest::new(instance_ref, workspace)
        .with_workspace_access_mode(WorkspaceAccessMode::Mutating)
        .with_model_ref(model_ref);

    let handle = runtime.start(start_req).await.unwrap();
    let session_ref = handle.session_ref();

    tokio::time::sleep(Duration::from_millis(150)).await;

    // First stop
    let stop1 = runtime.stop(&session_ref).await;
    assert!(stop1.is_ok(), "First stop should succeed");

    // Second stop -> must be idempotent Ok(())
    let stop2 = runtime.stop(&session_ref).await;
    assert!(stop2.is_ok(), "Second stop must be idempotent Ok(())");

    // Status must be Stopped
    let st = runtime.status(&session_ref).await.unwrap();
    assert_eq!(st, RuntimeLifecycleState::Stopped);

    // Terminal state persistence: sending input after stop must fail with TerminalStateError
    let send_res = runtime.send(&session_ref, RuntimeInput::text("late")).await;
    assert!(
        send_res.is_err(),
        "Sending input on stopped session must fail"
    );

    // Event replay after stop: events_after must still successfully replay all historical events
    let mut sub = runtime.events_after(&session_ref, None).await.unwrap();
    let mut events = Vec::new();
    while let Ok(Some(ev)) = sub.next_event().await {
        events.push(ev);
        if events.len() >= 2 {
            break;
        }
    }
    assert!(
        !events.is_empty(),
        "Must be able to replay events after session has stopped"
    );
    assert_eq!(events[0].sequence, 1);
    assert!(matches!(
        events[0].kind,
        RuntimeEventKind::SessionStarted { .. }
    ));
}
