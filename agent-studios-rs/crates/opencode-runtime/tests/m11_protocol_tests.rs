use url::Url;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use agent_studios_opencode_runtime::{
    CreateSessionRequest, MessagePart, ModelSpec, OpenCodeError, OpenCodeHttpClient,
    OpenCodeMessageRequest, PermissionDecisionRequest,
};

#[test]
fn test_client_debug_redacts_credentials_and_url() {
    let base_url = Url::parse("http://secret_user:super_secret@127.0.0.1:4096/").unwrap();
    let client = OpenCodeHttpClient::with_credentials(
        base_url,
        "custom_user",
        "super_secret_password_12345",
    )
    .unwrap();

    let debug_str = format!("{client:?}");
    assert!(
        !debug_str.contains("super_secret_password_12345"),
        "Client debug output leaked password"
    );
    assert!(
        !debug_str.contains("super_secret"),
        "Client debug output leaked URL password"
    );
    assert!(
        debug_str.contains("[REDACTED]"),
        "Client debug output missing [REDACTED] tag"
    );
}

#[tokio::test]
async fn test_client_basic_auth_header_injection() {
    let mock_server = MockServer::start().await;
    let server_url = Url::parse(&mock_server.uri()).unwrap();
    let password = "opencode_secret_token";

    // Expected Basic Auth: base64("opencode:opencode_secret_token")
    let expected_auth = format!(
        "Basic {}",
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            format!("opencode:{password}"),
        )
    );

    Mock::given(method("GET"))
        .and(path("/global/health"))
        .and(header("authorization", expected_auth.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = OpenCodeHttpClient::new(server_url, password).unwrap();
    assert!(client.health_check().await.is_ok());
}

#[tokio::test]
async fn test_client_rejects_redirects_no_credential_leakage() {
    let mock_server = MockServer::start().await;
    let server_url = Url::parse(&mock_server.uri()).unwrap();

    // 302 redirect should NOT be followed by the client
    Mock::given(method("GET"))
        .and(path("/global/health"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", "http://external-leak-target.com/health"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = OpenCodeHttpClient::new(server_url, "test-password").unwrap();
    let result = client.health_check().await;
    assert!(
        result.is_err(),
        "Client must not silently follow redirect without error"
    );
}

#[tokio::test]
async fn test_create_session_success_and_invalid_schema() {
    let mock_server = MockServer::start().await;
    let server_url = Url::parse(&mock_server.uri()).unwrap();

    // Successful create session
    Mock::given(method("POST"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "sess-alpha-99",
            "title": "Unit Test Session",
            "status": "ready"
        })))
        .mount(&mock_server)
        .await;

    let client = OpenCodeHttpClient::new(server_url.clone(), "test-pass").unwrap();
    let session = client
        .create_session(CreateSessionRequest {
            title: Some("Unit Test Session".to_string()),
            directory: None,
        })
        .await
        .expect("create_session failed");

    assert_eq!(session.id, "sess-alpha-99");
    assert_eq!(session.title.as_deref(), Some("Unit Test Session"));

    // Missing ID must fail deserialization
    let mock_server2 = MockServer::start().await;
    let server_url2 = Url::parse(&mock_server2.uri()).unwrap();

    Mock::given(method("POST"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "title": "Invalid Session With No ID"
        })))
        .mount(&mock_server2)
        .await;

    let client2 = OpenCodeHttpClient::new(server_url2, "test-pass").unwrap();
    let err = client2
        .create_session(CreateSessionRequest::default())
        .await;
    assert!(
        err.is_err(),
        "create_session must reject payload missing mandatory 'id'"
    );
}

#[tokio::test]
async fn test_send_message_typed_request_and_prompt_response() {
    let mock_server = MockServer::start().await;
    let server_url = Url::parse(&mock_server.uri()).unwrap();

    Mock::given(method("POST"))
        .and(path("/session/sess-123/message"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "success": true,
            "messageID": "msg-888",
            "status": "running"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = OpenCodeHttpClient::new(server_url, "test-pass").unwrap();
    let req = OpenCodeMessageRequest {
        parts: vec![MessagePart::text("Refactor authentication module")],
        model: Some(ModelSpec {
            provider_id: "anthropic".to_string(),
            model_id: "claude-3-5-sonnet".to_string(),
        }),
        content: None,
        provider: None,
    };

    let resp = client
        .send_message("sess-123", req)
        .await
        .expect("send_message failed");

    assert!(resp.success);
    assert_eq!(resp.message_id.as_deref(), Some("msg-888"));
    assert_eq!(resp.status.as_deref(), Some("running"));
}

#[tokio::test]
async fn test_respond_permission_decision() {
    let mock_server = MockServer::start().await;
    let server_url = Url::parse(&mock_server.uri()).unwrap();

    Mock::given(method("POST"))
        .and(path("/session/sess-123/permission/perm-456"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "accepted"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = OpenCodeHttpClient::new(server_url, "test-pass").unwrap();
    let res = client
        .respond_permission(
            "sess-123",
            "perm-456",
            PermissionDecisionRequest::with_reason(true, "User explicitly approved bash execution"),
        )
        .await;

    assert!(res.is_ok(), "Permission decision must succeed on 200 OK");
}

#[tokio::test]
async fn test_abort_session_and_status() {
    let mock_server = MockServer::start().await;
    let server_url = Url::parse(&mock_server.uri()).unwrap();

    Mock::given(method("POST"))
        .and(path("/session/sess-123/abort"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "aborted"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/sess-123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "aborted",
            "is_running": false
        })))
        .mount(&mock_server)
        .await;

    let client = OpenCodeHttpClient::new(server_url, "test-pass").unwrap();
    assert!(client.abort_session("sess-123").await.is_ok());

    let status = client
        .get_session_status("sess-123")
        .await
        .expect("get_session_status failed");
    assert_eq!(status.status, "aborted");
    assert!(!status.is_running);
}

#[tokio::test]
async fn test_http_status_code_mapping_and_error_sanitization() {
    let mock_server = MockServer::start().await;
    let server_url = Url::parse(&mock_server.uri()).unwrap();

    // 401 Unauthorized returns typed Http error
    Mock::given(method("POST"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized access"))
        .mount(&mock_server)
        .await;

    let client = OpenCodeHttpClient::new(server_url.clone(), "wrong-pass").unwrap();
    let err = client
        .create_session(CreateSessionRequest::default())
        .await
        .unwrap_err();

    match err {
        OpenCodeError::ApiError { status, .. } => {
            assert_eq!(status, 401, "Expected 401 status in error response");
        }
        other => panic!("Expected OpenCodeError::ApiError on 401, got {other:?}"),
    }

    // 404 Session Not Found
    let mock_server2 = MockServer::start().await;
    let server_url2 = Url::parse(&mock_server2.uri()).unwrap();

    Mock::given(method("POST"))
        .and(path("/session/sess-missing/abort"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Session does not exist"))
        .mount(&mock_server2)
        .await;

    let client2 = OpenCodeHttpClient::new(server_url2, "test-pass").unwrap();
    let err2 = client2.abort_session("sess-missing").await.unwrap_err();

    match err2 {
        OpenCodeError::SessionNotFound(sid) => {
            assert_eq!(sid, "sess-missing");
        }
        other => panic!("Expected OpenCodeError::SessionNotFound, got {other:?}"),
    }
}
