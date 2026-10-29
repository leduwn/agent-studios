use std::sync::Arc;
use std::time::Duration;

use agent_studios_provider::auth::AuthenticationScheme;
use agent_studios_provider::discovery::{DiscoveryConfig, DiscoveryError, ModelDiscoveryClient};
use agent_studios_provider::endpoint::EndpointProfile;
use agent_studios_provider::id::{ProviderId, ProviderInstanceId};
use agent_studios_provider::instance::ProviderInstance;
use agent_studios_provider::model::ModelMetadataSource;
use agent_studios_provider::protocol::ProtocolFamily;
use agent_studios_provider::secret::SecretReference;
use agent_studios_provider::store::InMemorySecretStore;

use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_discover_openai_models_success() {
    let server = MockServer::start().await;

    let secret_ref = SecretReference::env("OPENAI_TEST_KEY").unwrap();
    let secret_store = Arc::new(
        InMemorySecretStore::new().with_secret(secret_ref.clone(), "sk-test-token-12345"),
    );

    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Bearer sk-test-token-12345"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "object": "list",
            "data": [
                {
                    "id": "gpt-4o",
                    "object": "model",
                    "created": 1715368132,
                    "owned_by": "system"
                },
                {
                    "id": "gpt-4o-mini",
                    "object": "model",
                    "created": 1721172741,
                    "owned_by": "system"
                }
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "OpenAI Production",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new(server.uri()).unwrap(),
        AuthenticationScheme::BearerToken { secret: secret_ref },
    )
    .unwrap();

    let client =
        ModelDiscoveryClient::new(secret_store, Duration::from_secs(5), Duration::from_secs(60));

    let models = client.discover_models(&instance).await.unwrap();
    assert_eq!(models.len(), 2);

    let m1 = &models[0];
    assert_eq!(m1.provider_instance_id, instance_id);
    assert_eq!(m1.id.as_str(), "gpt-4o");
    assert_eq!(m1.display_name, "gpt-4o");
    assert_eq!(m1.limits.context_window_tokens, Some(128_000));
    assert_eq!(m1.limits.max_output_tokens, Some(16_384));
    assert_eq!(m1.capabilities.tool_calling.is_supported(), true);
    assert_eq!(m1.capabilities.vision_input.is_supported(), true);
    assert_eq!(m1.capabilities.streaming.is_supported(), true);
    assert_eq!(
        m1.metadata_source,
        ModelMetadataSource::ProviderDiscovery
    );

    let m2 = &models[1];
    assert_eq!(m2.id.as_str(), "gpt-4o-mini");
    assert_eq!(m2.limits.context_window_tokens, Some(128_000));
    assert_eq!(m2.capabilities.tool_calling.is_supported(), true);
}

#[tokio::test]
async fn test_discover_anthropic_models_success() {
    let server = MockServer::start().await;

    let secret_ref = SecretReference::env("ANTHROPIC_TEST_KEY").unwrap();
    let secret_store =
        Arc::new(InMemorySecretStore::new().with_secret(secret_ref.clone(), "sk-ant-test-key"));

    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("x-api-key", "sk-ant-test-key"))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {
                    "type": "model",
                    "id": "claude-3-5-sonnet-20241022",
                    "display_name": "Claude 3.5 Sonnet",
                    "created_at": "2024-10-22T00:00:00Z"
                }
            ],
            "has_more": false
        })))
        .expect(1)
        .mount(&server)
        .await;

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("anthropic").unwrap(),
        "Anthropic Instance",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new(server.uri()).unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: secret_ref,
        },
    )
    .unwrap();

    let client =
        ModelDiscoveryClient::new(secret_store, Duration::from_secs(5), Duration::from_secs(60));

    let models = client.discover_models(&instance).await.unwrap();
    assert_eq!(models.len(), 1);

    let m = &models[0];
    assert_eq!(m.id.as_str(), "claude-3-5-sonnet-20241022");
    assert_eq!(m.display_name, "Claude 3.5 Sonnet");
    assert_eq!(m.limits.context_window_tokens, Some(200_000));
    assert_eq!(m.limits.max_output_tokens, Some(8_192));
    assert_eq!(m.capabilities.tool_calling.is_supported(), true);
    assert_eq!(m.capabilities.vision_input.is_supported(), true);
    assert_eq!(m.capabilities.streaming.is_supported(), true);
    assert_eq!(m.metadata_source, ModelMetadataSource::ProviderDiscovery);
}

#[tokio::test]
async fn test_discover_ollama_tags_success() {
    let server = MockServer::start().await;

    let secret_store = Arc::new(InMemorySecretStore::new());

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": [
                {
                    "name": "llama3:latest",
                    "model": "llama3:latest",
                    "modified_at": "2024-05-01T00:00:00Z",
                    "size": 4661224676i64
                }
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("ollama").unwrap(),
        "Ollama Local",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(server.uri()).unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    let client =
        ModelDiscoveryClient::new(secret_store, Duration::from_secs(5), Duration::from_secs(60));

    let models = client.discover_models(&instance).await.unwrap();
    assert_eq!(models.len(), 1);

    let m = &models[0];
    assert_eq!(m.id.as_str(), "llama3:latest");
    assert_eq!(m.display_name, "llama3:latest");
    assert_eq!(m.limits.context_window_tokens, Some(8_192));
    assert_eq!(m.capabilities.tool_calling.is_supported(), true);
    assert_eq!(m.capabilities.streaming.is_supported(), true);
    assert_eq!(m.metadata_source, ModelMetadataSource::ProviderDiscovery);
}

#[tokio::test]
async fn test_discover_authentication_failure_401() {
    let server = MockServer::start().await;

    let secret_ref = SecretReference::env("BAD_KEY").unwrap();
    let secret_store =
        Arc::new(InMemorySecretStore::new().with_secret(secret_ref.clone(), "invalid-token"));

    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Invalid authentication key"))
        .expect(1)
        .mount(&server)
        .await;

    let instance = ProviderInstance::new(
        ProviderInstanceId::new(),
        ProviderId::new("openai").unwrap(),
        "OpenAI Test",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new(server.uri()).unwrap(),
        AuthenticationScheme::BearerToken { secret: secret_ref },
    )
    .unwrap();

    let client =
        ModelDiscoveryClient::new(secret_store, Duration::from_secs(5), Duration::from_secs(60));

    let err = client.discover_models(&instance).await.unwrap_err();
    match err {
        DiscoveryError::AuthenticationFailed(msg) => {
            assert!(msg.contains("Invalid authentication key"));
        }
        other => panic!("Expected AuthenticationFailed, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_discover_rate_limited_429() {
    let server = MockServer::start().await;

    let secret_ref = SecretReference::env("RATE_LIMIT_KEY").unwrap();
    let secret_store =
        Arc::new(InMemorySecretStore::new().with_secret(secret_ref.clone(), "valid-key"));

    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "30")
                .set_body_string("Rate limit exceeded. Please wait."),
        )
        .expect(1)
        .mount(&server)
        .await;

    let instance = ProviderInstance::new(
        ProviderInstanceId::new(),
        ProviderId::new("openai").unwrap(),
        "OpenAI Test",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new(server.uri()).unwrap(),
        AuthenticationScheme::BearerToken { secret: secret_ref },
    )
    .unwrap();

    let client =
        ModelDiscoveryClient::new(secret_store, Duration::from_secs(5), Duration::from_secs(60));

    let err = client.discover_models(&instance).await.unwrap_err();
    match err {
        DiscoveryError::RateLimited {
            retry_after_secs,
            message,
        } => {
            assert_eq!(retry_after_secs, Some(30));
            assert!(message.contains("Rate limit exceeded"));
        }
        other => panic!("Expected RateLimited, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_discovery_cache_ttl_and_refresh() {
    let server = MockServer::start().await;

    let secret_store = Arc::new(InMemorySecretStore::new());

    // Expect exactly 2 requests across 3 calls
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                { "id": "cached-model-1" }
            ]
        })))
        .expect(2)
        .mount(&server)
        .await;

    let instance = ProviderInstance::new(
        ProviderInstanceId::new(),
        ProviderId::new("lm-studio").unwrap(),
        "LM Studio Test",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(server.uri()).unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    let client =
        ModelDiscoveryClient::new(secret_store, Duration::from_secs(5), Duration::from_secs(60));

    // Call 1: misses cache, executes HTTP request #1
    let models1 = client.discover_models(&instance).await.unwrap();
    assert_eq!(models1.len(), 1);
    assert_eq!(models1[0].id.as_str(), "cached-model-1");

    // Call 2: hits cache within TTL, does NOT execute HTTP request
    let models2 = client.discover_models(&instance).await.unwrap();
    assert_eq!(models2.len(), 1);
    assert_eq!(models2[0].id.as_str(), "cached-model-1");

    // Call 3: forces refresh, executes HTTP request #2
    let models3 = client.refresh_models(&instance).await.unwrap();
    assert_eq!(models3.len(), 1);
    assert_eq!(models3[0].id.as_str(), "cached-model-1");
}

#[tokio::test]
async fn test_secret_store_resolution_failure() {
    let server = MockServer::start().await;

    // Empty secret store with missing secret
    let secret_store = Arc::new(InMemorySecretStore::new());

    let secret_ref = SecretReference::env("MISSING_SECRET_KEY").unwrap();
    let instance = ProviderInstance::new(
        ProviderInstanceId::new(),
        ProviderId::new("openai").unwrap(),
        "OpenAI Missing Secret",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new(server.uri()).unwrap(),
        AuthenticationScheme::BearerToken { secret: secret_ref },
    )
    .unwrap();

    let client =
        ModelDiscoveryClient::new(secret_store, Duration::from_secs(5), Duration::from_secs(60));

    let err = client.discover_models(&instance).await.unwrap_err();
    match err {
        DiscoveryError::SecretResolutionFailed(msg) => {
            assert!(msg.to_lowercase().contains("not found"));
        }
        other => panic!("Expected SecretResolutionFailed, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_discover_openrouter_models_with_explicit_context() {
    let server = MockServer::start().await;

    let secret_ref = SecretReference::env("OR_KEY").unwrap();
    let secret_store =
        Arc::new(InMemorySecretStore::new().with_secret(secret_ref.clone(), "openrouter-key-abc"));

    Mock::given(method("GET"))
        .and(path("/api/v1/models"))
        .and(header("authorization", "Bearer openrouter-key-abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {
                    "id": "anthropic/claude-3.5-sonnet",
                    "name": "Anthropic: Claude 3.5 Sonnet",
                    "context_length": 200000,
                    "top_provider": {
                        "max_completion_tokens": 8192
                    }
                }
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let instance = ProviderInstance::new(
        ProviderInstanceId::new(),
        ProviderId::new("openrouter").unwrap(),
        "OpenRouter Gateway",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(server.uri()).unwrap(),
        AuthenticationScheme::BearerToken { secret: secret_ref },
    )
    .unwrap();

    let client =
        ModelDiscoveryClient::new(secret_store, Duration::from_secs(5), Duration::from_secs(60));

    let models = client.discover_models(&instance).await.unwrap();
    assert_eq!(models.len(), 1);
    let m = &models[0];
    assert_eq!(m.id.as_str(), "anthropic/claude-3.5-sonnet");
    assert_eq!(m.display_name, "Anthropic: Claude 3.5 Sonnet");
    assert_eq!(m.limits.context_window_tokens, Some(200_000));
    assert_eq!(m.limits.max_output_tokens, Some(8_192));
}

#[tokio::test]
async fn test_invalid_json_payload_returns_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>Not JSON</html>"))
        .expect(1)
        .mount(&server)
        .await;

    let instance = ProviderInstance::new(
        ProviderInstanceId::new(),
        ProviderId::new("openai").unwrap(),
        "OpenAI Bad Payload",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new(server.uri()).unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    let client = ModelDiscoveryClient::with_config(
        Arc::new(InMemorySecretStore::new()),
        DiscoveryConfig::default(),
    );

    let err = client.discover_models(&instance).await.unwrap_err();
    match err {
        DiscoveryError::InvalidPayload(msg) => {
            assert!(msg.contains("Failed to parse JSON"));
        }
        other => panic!("Expected InvalidPayload, got: {other:?}"),
    }
}
