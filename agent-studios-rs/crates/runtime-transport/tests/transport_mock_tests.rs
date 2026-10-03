use futures::StreamExt;
use std::sync::Arc;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use agent_studios_provider::auth::AuthenticationScheme;
use agent_studios_provider::endpoint::EndpointProfile;
use agent_studios_provider::id::{ProviderId, ProviderInstanceId};
use agent_studios_provider::instance::ProviderInstance;
use agent_studios_provider::protocol::ProtocolFamily;
use agent_studios_provider::secret::{SecretBackend, SecretReference};

use agent_studios_runtime_transport::{ContinuationManager, InMemorySecretResolver, RuntimeRouter};
use codex_api::{ResponseEvent, ResponsesApiRequest};
use codex_model_provider::{ModelInferenceBackend, ModelInferenceContext};
use codex_protocol::models::{ContentItem, ResponseItem};

fn create_test_request(model: &str) -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: model.to_string(),
        stream: true,
        service_tier: None,
        instructions: "You are a helpful assistant.".to_string(),
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "Hello test".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        }],
        tools: None,
        tool_choice: "auto".to_string(),
        parallel_tool_calls: false,
        reasoning: None,
        store: false,
        stream_options: None,
        include: Vec::new(),
        prompt_cache_key: None,
        text: None,
        client_metadata: None,
        access_programs: None,
    }
}

#[tokio::test]
async fn test_chat_completions_wiremock() {
    let mock_server = MockServer::start().await;

    let sse_body = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\n\
                    data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello from mock chat!\"},\"finish_reason\":null}]}\n\n\
                    data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n\
                    data: [DONE]\n\n";

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body),
        )
        .mount(&mock_server)
        .await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_OPENAI_KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_OPENAI_KEY", "sk-mock-key"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager);

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "Mock OpenAI Instance",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::BearerToken { secret: sec_ref },
    )
    .unwrap();

    router.register_instance(instance);
    router.set_active_instance(instance_id);

    let request = create_test_request("gpt-4o-mini");
    let context = ModelInferenceContext {
        thread_id: "thread-chat-1".to_string(),
        turn_id: Some("turn-1".to_string()),
    };

    let mut stream = router.stream(request, context).await.unwrap();
    let mut text_accum = String::new();
    let mut got_completed = false;

    while let Some(event_res) = stream.next().await {
        let event = event_res.unwrap();
        match event {
            ResponseEvent::OutputTextDelta(delta) => {
                text_accum.push_str(&delta);
            }
            ResponseEvent::Completed { .. } => {
                got_completed = true;
            }
            _ => {}
        }
    }

    assert_eq!(text_accum, "Hello from mock chat!");
    assert!(got_completed);
}

#[tokio::test]
async fn test_anthropic_wiremock() {
    let mock_server = MockServer::start().await;

    let sse_body = "event: message_start\n\
                    data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_123\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-3-5-sonnet\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":20,\"output_tokens\":0}}}\n\n\
                    event: content_block_start\n\
                    data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
                    event: content_block_delta\n\
                    data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello from Anthropic!\"}}\n\n\
                    event: content_block_stop\n\
                    data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
                    event: message_delta\n\
                    data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":10}}\n\n\
                    event: message_stop\n\
                    data: {\"type\":\"message_stop\"}\n\n";

    Mock::given(method("POST"))
        .and(path("/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body),
        )
        .mount(&mock_server)
        .await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_ANTHROPIC_KEY".to_string(),
    };
    let secret_resolver = Arc::new(
        InMemorySecretResolver::new().with_env_secret("TEST_ANTHROPIC_KEY", "ant-mock-key"),
    );
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, Arc::clone(&continuation_manager));

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("anthropic").unwrap(),
        "Mock Anthropic Instance",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: sec_ref,
        },
    )
    .unwrap();

    router.register_instance(instance);
    router.set_active_instance(instance_id);

    let request = create_test_request("claude-3-5-sonnet");
    let thread_id = "thread-ant-1";
    let context = ModelInferenceContext {
        thread_id: thread_id.to_string(),
        turn_id: Some("turn-1".to_string()),
    };

    let mut stream = router.stream(request, context).await.unwrap();
    let mut text_accum = String::new();
    let mut got_completed = false;

    while let Some(event_res) = stream.next().await {
        let event = event_res.unwrap();
        match event {
            ResponseEvent::OutputTextDelta(delta) => {
                text_accum.push_str(&delta);
            }
            ResponseEvent::Completed { .. } => {
                got_completed = true;
            }
            _ => {}
        }
    }

    assert_eq!(text_accum, "Hello from Anthropic!");
    assert!(got_completed);

    // Verify continuation state committed into ContinuationManager
    let cont = continuation_manager.get(thread_id);
    assert_eq!(cont.anthropic.message_id.as_deref(), Some("msg_123"));
}

#[tokio::test]
async fn test_gemini_wiremock() {
    let mock_server = MockServer::start().await;

    let sse_body = "data: {\"responseId\":\"resp_gem_123\",\"candidates\":[{\"index\":0,\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"Hello from Gemini!\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":10,\"candidatesTokenCount\":5,\"totalTokenCount\":15}}\n\n";

    Mock::given(method("POST"))
        .and(path_regex(r"^/models/.*:streamGenerateContent$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body),
        )
        .mount(&mock_server)
        .await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_GEMINI_KEY".to_string(),
    };
    let secret_resolver = Arc::new(
        InMemorySecretResolver::new().with_env_secret("TEST_GEMINI_KEY", "gemini-mock-key"),
    );
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, Arc::clone(&continuation_manager));

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("google").unwrap(),
        "Mock Gemini Instance",
        ProtocolFamily::GeminiGenerateContent,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::QueryParameter {
            parameter_name: "key".to_string(),
            secret: sec_ref,
        },
    )
    .unwrap();

    router.register_instance(instance);
    router.set_active_instance(instance_id);

    let request = create_test_request("gemini-2.5-flash");
    let thread_id = "thread-gem-1";
    let context = ModelInferenceContext {
        thread_id: thread_id.to_string(),
        turn_id: Some("turn-1".to_string()),
    };

    let mut stream = router.stream(request, context).await.unwrap();
    let mut text_accum = String::new();
    let mut got_completed = false;

    while let Some(event_res) = stream.next().await {
        let event = event_res.unwrap();
        match event {
            ResponseEvent::OutputTextDelta(delta) => {
                text_accum.push_str(&delta);
            }
            ResponseEvent::Completed { .. } => {
                got_completed = true;
            }
            _ => {}
        }
    }

    assert_eq!(text_accum, "Hello from Gemini!");
    assert!(got_completed);
}

#[tokio::test]
async fn test_transactional_rollback_on_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_string("Internal Server Error from upstream provider"),
        )
        .mount(&mock_server)
        .await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_KEY", "sk-mock"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, Arc::clone(&continuation_manager));

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "Mock Error Instance",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::BearerToken { secret: sec_ref },
    )
    .unwrap();

    router.register_instance(instance);
    router.set_active_instance(instance_id);

    let request = create_test_request("gpt-4o");
    let thread_id = "thread-error-test";
    let context = ModelInferenceContext {
        thread_id: thread_id.to_string(),
        turn_id: None,
    };

    let result = router.stream(request, context).await;
    assert!(result.is_err());

    // Verify continuation state remains unpolluted
    let cont = continuation_manager.get(thread_id);
    assert!(cont.anthropic.is_empty());
    assert_eq!(cont.gemini, Default::default());
}

#[tokio::test]
async fn test_cancellation_propagation() {
    let mock_server = MockServer::start().await;

    // Send one chunk then close
    let sse_body = "data: {\"id\":\"chatcmpl-cancel\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Chunk 1\"},\"finish_reason\":null}]}\n\n";

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body),
        )
        .mount(&mock_server)
        .await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_KEY", "sk-mock"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager);

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "Mock Cancel Instance",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::BearerToken { secret: sec_ref },
    )
    .unwrap();

    router.register_instance(instance);
    router.set_active_instance(instance_id);

    let request = create_test_request("gpt-4o");
    let context = ModelInferenceContext {
        thread_id: "thread-cancel".to_string(),
        turn_id: None,
    };

    let mut stream = router.stream(request, context).await.unwrap();
    // Consume first event
    let first = stream.next().await;
    assert!(first.is_some());

    // Trigger interrupt
    if let Some(interrupt) = stream.interrupt.take() {
        let _ = interrupt.send(());
    }

    // Stream should complete / terminate gracefully
    let _ = stream.next().await;
}

#[tokio::test]
async fn test_concurrent_thread_isolation() {
    let mock_server = MockServer::start().await;

    let sse_body_a = "event: message_start\n\
                      data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_aaa\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-3-5-sonnet\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n\
                      event: message_delta\n\
                      data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n\
                      event: message_stop\n\
                      data: {\"type\":\"message_stop\"}\n\n";

    let sse_body_b = "event: message_start\n\
                      data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_bbb\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-3-5-sonnet\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n\
                      event: message_delta\n\
                      data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n\
                      event: message_stop\n\
                      data: {\"type\":\"message_stop\"}\n\n";

    Mock::given(method("POST"))
        .and(path("/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body_a),
        )
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body_b),
        )
        .mount(&mock_server)
        .await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_KEY", "ant-key"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = Arc::new(RuntimeRouter::new(
        secret_resolver,
        Arc::clone(&continuation_manager),
    ));

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("anthropic").unwrap(),
        "Mock Isolation Instance",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: sec_ref,
        },
    )
    .unwrap();

    router.register_instance(instance);
    router.set_active_instance(instance_id);

    let r1 = Arc::clone(&router);
    let handle_a = tokio::spawn(async move {
        let req = create_test_request("claude-3-5-sonnet");
        let ctx = ModelInferenceContext {
            thread_id: "thread-iso-A".to_string(),
            turn_id: None,
        };
        let mut stream = r1.stream(req, ctx).await.unwrap();
        while let Some(ev) = stream.next().await {
            let _ = ev;
        }
    });

    let r2 = Arc::clone(&router);
    let handle_b = tokio::spawn(async move {
        let req = create_test_request("claude-3-5-sonnet");
        let ctx = ModelInferenceContext {
            thread_id: "thread-iso-B".to_string(),
            turn_id: None,
        };
        let mut stream = r2.stream(req, ctx).await.unwrap();
        while let Some(ev) = stream.next().await {
            let _ = ev;
        }
    });

    let (res_a, res_b) = tokio::join!(handle_a, handle_b);
    res_a.unwrap();
    res_b.unwrap();

    let cont_a = continuation_manager.get("thread-iso-A");
    let cont_b = continuation_manager.get("thread-iso-B");

    assert_eq!(cont_a.anthropic.message_id.as_deref(), Some("msg_aaa"));
    assert_eq!(cont_b.anthropic.message_id.as_deref(), Some("msg_bbb"));
}
