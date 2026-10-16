use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent_studios_external_runtime::{
    AgentRuntime, RuntimeInput, RuntimeInstanceRef, RuntimeStartRequest, WorkspaceAccessMode,
};
use agent_studios_opencode_runtime::OpenCodeRuntime;
use agent_studios_protocol::id::{ApprovalId, WorktreeId};
use agent_studios_protocol::worktree::ExecutionWorkspace;
use agent_studios_provider::id::{ModelId, ProviderId, ProviderInstanceId};
use agent_studios_provider::model::ModelRef;
use agent_studios_provider::{
    AuthenticationScheme, EndpointProfile, ProtocolFamily, ProviderCatalog, ProviderInstance,
    SecretReference,
};
use tempfile::tempdir;
use url::Url;
use wiremock::matchers::{body_json, method, path};
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
async fn test_m11_r08_approval_response_dispatched_to_permission_endpoint() {
    let mock_server = MockServer::start().await;
    let server_url = Url::parse(&mock_server.uri()).unwrap();

    // 1. Mock /global/health
    Mock::given(method("GET"))
        .and(path("/global/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok"
        })))
        .mount(&mock_server)
        .await;

    // 2. Mock POST /session
    Mock::given(method("POST"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "opencode-sess-perm-test",
            "status": "ready"
        })))
        .mount(&mock_server)
        .await;

    // 3. Mock GET /event
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("event: output.delta\ndata: {\"text\": \"ready\"}\n\n"),
        )
        .mount(&mock_server)
        .await;

    let approval_id = ApprovalId::new();
    let perm_endpoint_path = format!(
        "/session/opencode-sess-perm-test/permission/{}",
        approval_id
    );

    // 4. Mock POST /session/:id/permission/:permission_id
    Mock::given(method("POST"))
        .and(path(perm_endpoint_path.as_str()))
        .and(body_json(serde_json::json!({
            "granted": true,
            "approved": true,
            "reason": "Authorized by user policy"
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
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

    // Send ApprovalResponse
    let approval_input = RuntimeInput::approval_response(
        approval_id,
        true,
        Some("Authorized by user policy".to_string()),
    );

    let send_res = runtime.send(&session_ref, approval_input).await;
    assert!(
        send_res.is_ok(),
        "Sending approval response must succeed: {:?}",
        send_res.err()
    );

    // mock_server drops and verifies expect(1) on permission endpoint was satisfied
}
