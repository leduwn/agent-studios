use futures::StreamExt;
use std::sync::Arc;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use agent_studios_provider::auth::AuthenticationScheme;
use agent_studios_provider::capabilities::{CapabilitySupport, ModelCapabilities};
use agent_studios_provider::endpoint::EndpointProfile;
use agent_studios_provider::id::{ModelId, ProviderId, ProviderInstanceId};
use agent_studios_provider::instance::ProviderInstance;
use agent_studios_provider::model::{ModelDescriptor, ModelLimits};
use agent_studios_provider::protocol::ProtocolFamily;
use agent_studios_provider::secret::{SecretBackend, SecretReference};

use agent_studios_runtime_transport::{
    ContinuationKey, ContinuationManager, DiagnosticLevel, InMemorySecretResolver,
    RecordingRuntimeDiagnosticSink, ResolvedAuth, RuntimeDiagnosticSink, RuntimeModelRoute,
    RuntimeRouter, RuntimeTransportOptions, SecretResolver, SseParser, TransportError,
    sanitize_error_message,
};
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

fn test_descriptor(instance_id: ProviderInstanceId, model: &str) -> ModelDescriptor {
    ModelDescriptor::new(
        instance_id,
        ModelId::new(model).unwrap(),
        model,
        ModelCapabilities::default(),
        ModelLimits::default(),
    )
    .unwrap()
}

fn test_anthropic_descriptor(instance_id: ProviderInstanceId, model: &str) -> ModelDescriptor {
    ModelDescriptor::new(
        instance_id,
        ModelId::new(model).unwrap(),
        model,
        ModelCapabilities::default(),
        ModelLimits {
            context_window_tokens: Some(200_000),
            max_output_tokens: Some(4096),
        },
    )
    .unwrap()
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
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

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

    let desc = test_descriptor(instance_id, "gpt-4o-mini");
    router
        .register_model("gpt-4o-mini", instance_id, desc)
        .unwrap();

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
    let router = RuntimeRouter::new(secret_resolver, Arc::clone(&continuation_manager)).unwrap();

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

    let desc = test_anthropic_descriptor(instance_id, "claude-3-5-sonnet");
    router
        .register_model("claude-3-5-sonnet", instance_id, desc)
        .unwrap();

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
    let cont = continuation_manager.get(&ContinuationKey::new(instance_id, thread_id));
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
    let router = RuntimeRouter::new(secret_resolver, Arc::clone(&continuation_manager)).unwrap();

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

    let desc = test_descriptor(instance_id, "gemini-2.5-flash");
    router
        .register_model("gemini-2.5-flash", instance_id, desc)
        .unwrap();

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
    let router = RuntimeRouter::new(secret_resolver, Arc::clone(&continuation_manager)).unwrap();

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

    let desc = test_descriptor(instance_id, "gpt-4o");
    router.register_model("gpt-4o", instance_id, desc).unwrap();

    let request = create_test_request("gpt-4o");
    let thread_id = "thread-error-test";
    let context = ModelInferenceContext {
        thread_id: thread_id.to_string(),
        turn_id: None,
    };

    let result = router.stream(request, context).await;
    assert!(result.is_err());

    // Verify continuation state remains unpolluted
    let cont = continuation_manager.get(&ContinuationKey::new(instance_id, thread_id));
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
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

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

    let desc = test_descriptor(instance_id, "gpt-4o");
    router.register_model("gpt-4o", instance_id, desc).unwrap();

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
    let router =
        Arc::new(RuntimeRouter::new(secret_resolver, Arc::clone(&continuation_manager)).unwrap());

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

    let desc = test_anthropic_descriptor(instance_id, "claude-3-5-sonnet");
    router
        .register_model("claude-3-5-sonnet", instance_id, desc)
        .unwrap();

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

    let cont_a = continuation_manager.get(&ContinuationKey::new(instance_id, "thread-iso-A"));
    let cont_b = continuation_manager.get(&ContinuationKey::new(instance_id, "thread-iso-B"));

    assert_eq!(cont_a.anthropic.message_id.as_deref(), Some("msg_aaa"));
    assert_eq!(cont_b.anthropic.message_id.as_deref(), Some("msg_bbb"));
}

#[tokio::test]
async fn test_same_thread_concurrency_rejection() {
    let mock_server = MockServer::start().await;

    let sse_body = "data: {\"id\":\"chatcmpl-slow\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"slow\"},\"finish_reason\":null}]}\n\n\
                    data: [DONE]\n\n";

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body)
                .set_delay(std::time::Duration::from_millis(200)),
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
    let router =
        Arc::new(RuntimeRouter::new(secret_resolver, Arc::clone(&continuation_manager)).unwrap());

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "Mock Concurrency Instance",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::BearerToken { secret: sec_ref },
    )
    .unwrap();

    router.register_instance(instance);

    let desc = test_descriptor(instance_id, "gpt-4o");
    router.register_model("gpt-4o", instance_id, desc).unwrap();

    let thread_id = "thread-same-concurrency";
    let r1 = Arc::clone(&router);
    let t_id = thread_id.to_string();
    let handle_1 = tokio::spawn(async move {
        let req = create_test_request("gpt-4o");
        let ctx = ModelInferenceContext {
            thread_id: t_id,
            turn_id: None,
        };
        let mut stream = r1.stream(req, ctx).await.unwrap();
        let mut events = Vec::new();
        while let Some(ev) = stream.next().await {
            events.push(ev);
        }
        events
    });

    // Small sleep to ensure handle_1 has begun transaction
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;

    // Second request to same thread while first is still in-flight
    let req2 = create_test_request("gpt-4o");
    let ctx2 = ModelInferenceContext {
        thread_id: thread_id.to_string(),
        turn_id: None,
    };
    let err2 = match router.stream(req2, ctx2).await {
        Err(e) => e,
        Ok(_) => panic!("Expected error due to concurrent stream"),
    };
    assert!(
        err2.to_string()
            .contains("Concurrent inference request rejected for thread")
    );

    let events1 = handle_1.await.unwrap();
    assert!(!events1.is_empty());

    // After first finishes, new request can acquire the lease again
    let req3 = create_test_request("gpt-4o");
    let ctx3 = ModelInferenceContext {
        thread_id: thread_id.to_string(),
        turn_id: None,
    };
    let stream3 = router.stream(req3, ctx3).await;
    assert!(stream3.is_ok());
}

#[tokio::test]
async fn test_completed_only_commit_invariant() {
    let mock_server = MockServer::start().await;

    // Stream ends abruptly with no [DONE] or completion
    let sse_body = "data: {\"id\":\"chatcmpl-cut\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"cut short\"},\"finish_reason\":null}]}\n\n";

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
    let router = RuntimeRouter::new(secret_resolver, Arc::clone(&continuation_manager)).unwrap();

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "Mock Cut Instance",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::BearerToken { secret: sec_ref },
    )
    .unwrap();

    router.register_instance(instance);

    let desc = test_descriptor(instance_id, "gpt-4o");
    router.register_model("gpt-4o", instance_id, desc).unwrap();

    let thread_id = "thread-completed-invariant";
    let req = create_test_request("gpt-4o");
    let ctx = ModelInferenceContext {
        thread_id: thread_id.to_string(),
        turn_id: None,
    };

    let mut stream = router.stream(req, ctx).await.unwrap();
    let mut got_error = false;
    while let Some(res) = stream.next().await {
        if res.is_err() {
            got_error = true;
        }
    }
    assert!(got_error);

    // Lease was released (not stuck)
    let key = ContinuationKey::new(instance_id, thread_id);
    assert!(!continuation_manager.is_in_flight(&key));
    // Continuation state was NOT committed
    let state = continuation_manager.get(&key);
    assert!(state.anthropic.is_empty());
}

#[tokio::test]
async fn test_anthropic_max_tokens_fail_closed() {
    let mock_server = MockServer::start().await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_KEY", "ant-key"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("anthropic").unwrap(),
        "Mock Anthropic",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: sec_ref,
        },
    )
    .unwrap();

    // Register descriptor with limits.max_output_tokens = None
    let descriptor = ModelDescriptor {
        provider_instance_id: instance_id,
        id: ModelId::new("claude-test-no-limit").unwrap(),
        display_name: "Claude No Limit".to_string(),
        capabilities: Default::default(),
        limits: ModelLimits {
            context_window_tokens: None,
            max_output_tokens: None,
        },
        metadata_source: Default::default(),
    };

    router.register_instance(instance);
    router
        .register_model("claude-test-no-limit", instance_id, descriptor)
        .unwrap();

    let req = create_test_request("claude-test-no-limit");
    let ctx = ModelInferenceContext {
        thread_id: "thread-max-tokens-fail".to_string(),
        turn_id: None,
    };

    let err = match router.stream(req, ctx).await {
        Err(e) => e,
        Ok(_) => panic!("Expected error for missing max_output_tokens"),
    };
    let err_str = err.to_string();
    assert!(err_str.contains("Missing required model limit 'max_output_tokens'"));
}

#[tokio::test]
async fn test_capability_gating_unsupported_rejected_and_unknown_warning() {
    let mock_server = MockServer::start().await;

    let sse_body = "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
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
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_KEY", "sk-test"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let diagnostic_sink = Arc::new(RecordingRuntimeDiagnosticSink::new());

    let router = RuntimeRouter::new_with_options(
        secret_resolver,
        continuation_manager,
        RuntimeTransportOptions::default(),
        Arc::clone(&diagnostic_sink) as Arc<dyn RuntimeDiagnosticSink>,
    )
    .unwrap();

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "Mock Capability Instance",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::BearerToken { secret: sec_ref },
    )
    .unwrap();

    router.register_instance(instance);

    // Model A: streaming Unsupported
    let desc_no_stream = ModelDescriptor {
        provider_instance_id: instance_id,
        id: ModelId::new("model-no-stream").unwrap(),
        display_name: "Model No Stream".to_string(),
        capabilities: ModelCapabilities {
            streaming: CapabilitySupport::Unsupported,
            ..Default::default()
        },
        limits: ModelLimits {
            context_window_tokens: None,
            max_output_tokens: Some(2048),
        },
        metadata_source: Default::default(),
    };
    router
        .register_model("model-no-stream", instance_id, desc_no_stream)
        .unwrap();

    let mut req_stream = create_test_request("model-no-stream");
    req_stream.stream = true;
    let ctx = ModelInferenceContext {
        thread_id: "thread-gate-1".to_string(),
        turn_id: None,
    };
    let err = match router.stream(req_stream, ctx).await {
        Err(e) => e,
        Ok(_) => panic!("Expected error for unsupported streaming"),
    };
    assert!(err.to_string().contains("streaming"));

    // Model B: streaming Unknown -> succeeds with warning in diagnostic sink
    let desc_unknown_stream = ModelDescriptor {
        provider_instance_id: instance_id,
        id: ModelId::new("model-unknown-stream").unwrap(),
        display_name: "Model Unknown Stream".to_string(),
        capabilities: ModelCapabilities {
            streaming: CapabilitySupport::Unknown,
            ..Default::default()
        },
        limits: ModelLimits {
            context_window_tokens: None,
            max_output_tokens: Some(2048),
        },
        metadata_source: Default::default(),
    };
    router
        .register_model("model-unknown-stream", instance_id, desc_unknown_stream)
        .unwrap();

    let mut req_unknown = create_test_request("model-unknown-stream");
    req_unknown.stream = true;
    let ctx2 = ModelInferenceContext {
        thread_id: "thread-gate-2".to_string(),
        turn_id: None,
    };
    let stream_res = router.stream(req_unknown, ctx2).await;
    assert!(stream_res.is_ok());

    let recorded = diagnostic_sink.events();
    assert!(
        recorded
            .iter()
            .any(|d| d.level == DiagnosticLevel::Warning && d.message.contains("streaming"))
    );
}

#[tokio::test]
async fn test_routing_with_model_descriptor() {
    let mock_server = MockServer::start().await;

    let sse_body = "data: {\"id\":\"routed\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"routed response\"},\"finish_reason\":null}]}\n\n\
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
        locator: "TEST_KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_KEY", "sk-test"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "Routed Provider",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::BearerToken { secret: sec_ref },
    )
    .unwrap();

    router.register_instance(instance);

    let descriptor = ModelDescriptor {
        provider_instance_id: instance_id,
        id: ModelId::new("custom-routed-model").unwrap(),
        display_name: "Custom Routed Model".to_string(),
        capabilities: Default::default(),
        limits: ModelLimits {
            context_window_tokens: Some(128_000),
            max_output_tokens: Some(4096),
        },
        metadata_source: Default::default(),
    };

    let route = RuntimeModelRoute::new(instance_id, descriptor);
    router.register_route("custom-routed-model", route).unwrap();

    let req = create_test_request("custom-routed-model");
    let ctx = ModelInferenceContext {
        thread_id: "thread-routed-1".to_string(),
        turn_id: None,
    };

    let mut stream = router.stream(req, ctx).await.unwrap();
    let mut text = String::new();
    while let Some(Ok(ev)) = stream.next().await {
        if let ResponseEvent::OutputTextDelta(d) = ev {
            text.push_str(&d);
        }
    }
    assert_eq!(text, "routed response");
}

#[tokio::test]
async fn test_sensitive_header_marking() {
    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_KEY".to_string(),
    };
    let secret_resolver: Arc<dyn SecretResolver> = Arc::new(
        InMemorySecretResolver::new().with_env_secret("TEST_KEY", "super-secret-auth-value"),
    );
    let scheme = AuthenticationScheme::BearerToken { secret: sec_ref };

    let auth = ResolvedAuth::resolve(&scheme, &secret_resolver)
        .await
        .unwrap();
    let auth_header = auth.headers.get(reqwest::header::AUTHORIZATION).unwrap();
    assert!(auth_header.is_sensitive());
}

#[tokio::test]
async fn test_empty_secret_rejection() {
    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "EMPTY_KEY".to_string(),
    };
    let resolver: Arc<dyn SecretResolver> =
        Arc::new(InMemorySecretResolver::new().with_env_secret("EMPTY_KEY", "   \t\n  "));
    let scheme = AuthenticationScheme::BearerToken { secret: sec_ref };

    let err = ResolvedAuth::resolve(&scheme, &resolver).await.unwrap_err();
    match err {
        TransportError::EmptySecret { locator } => {
            assert_eq!(locator, "EMPTY_KEY");
        }
        other => panic!("Unexpected error: {other:?}"),
    }
}

#[test]
fn test_safe_error_mapping_redacts_keys() {
    let raw_err = "Request failed for https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:streamGenerateContent?key=AIzaSySecretApiKey12345&alt=sse: 403 Forbidden";
    let sanitized = sanitize_error_message(raw_err);
    assert!(!sanitized.contains("AIzaSySecretApiKey12345"));
    assert!(sanitized.contains("key=[REDACTED]"));
    assert!(sanitized.contains("alt=sse"));
}

#[test]
fn test_auth_collision_rejection() {
    let mut auth = ResolvedAuth::default();
    let mut val = reqwest::header::HeaderValue::from_static("secret");
    val.set_sensitive(true);
    auth.headers.insert(reqwest::header::AUTHORIZATION, val);

    let static_headers = ["authorization".to_string()];
    let query_params: Vec<String> = vec![];
    let base_url_queries: Vec<String> = vec![];

    let err = auth
        .check_collisions(
            static_headers.iter(),
            query_params.iter(),
            base_url_queries.iter(),
        )
        .unwrap_err();
    match err {
        TransportError::AuthenticationCollision { name, location } => {
            assert_eq!(name, "authorization");
            assert_eq!(location, "static_headers");
        }
        other => panic!("Unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn test_base_url_query_auth_collision_rejection() {
    let mock_server = MockServer::start().await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "SECRET_KEY".to_string(),
    };
    let secret_resolver = Arc::new(
        InMemorySecretResolver::new().with_env_secret("SECRET_KEY", "super-secret-query-token"),
    );
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let instance_id = ProviderInstanceId::new();
    let base_url = format!("{}/v1?totally_custom_token=STATIC", mock_server.uri());
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "Collision Instance",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(&base_url).unwrap(),
        AuthenticationScheme::QueryParameter {
            parameter_name: "totally_custom_token".to_string(),
            secret: sec_ref,
        },
    )
    .unwrap();

    router.register_instance(instance);
    let desc = test_descriptor(instance_id, "gpt-4o");
    router.register_model("gpt-4o", instance_id, desc).unwrap();

    let req = create_test_request("gpt-4o");
    let ctx = ModelInferenceContext {
        thread_id: "thread-coll-1".to_string(),
        turn_id: None,
    };

    let err = match router.stream(req, ctx).await {
        Err(e) => e,
        Ok(_) => panic!("Expected error, got Ok stream"),
    };
    let err_display = format!("{err}");
    let err_debug = format!("{err:?}");
    assert!(err_display.contains("totally_custom_token"));
    assert!(err_display.contains("base_url_query"));
    assert!(!err_display.contains("super-secret-query-token"));
    assert!(!err_debug.contains("super-secret-query-token"));

    // Verify no HTTP request was sent to the mock server
    let reqs = mock_server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 0);
}

#[tokio::test]
async fn test_base_url_query_auth_collision_case_insensitive() {
    let mock_server = MockServer::start().await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "SECRET_KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("SECRET_KEY", "secret-token-val"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let instance_id = ProviderInstanceId::new();
    let base_url = format!("{}/v1?TOTALLY_CUSTOM_TOKEN=STATIC", mock_server.uri());
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "Collision Case Instance",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(&base_url).unwrap(),
        AuthenticationScheme::QueryParameter {
            parameter_name: "totally_custom_token".to_string(),
            secret: sec_ref,
        },
    )
    .unwrap();

    router.register_instance(instance);
    let desc = test_descriptor(instance_id, "gpt-4o");
    router.register_model("gpt-4o", instance_id, desc).unwrap();

    let req = create_test_request("gpt-4o");
    let ctx = ModelInferenceContext {
        thread_id: "thread-coll-case".to_string(),
        turn_id: None,
    };

    let err = match router.stream(req, ctx).await {
        Err(e) => e,
        Ok(_) => panic!("Expected error, got Ok stream"),
    };
    let err_display = format!("{err}");
    let err_debug = format!("{err:?}");
    assert!(
        err_display.contains("TOTALLY_CUSTOM_TOKEN")
            || err_display.contains("totally_custom_token")
    );
    assert!(err_display.contains("base_url_query"));
    assert!(!err_display.contains("secret-token-val"));
    assert!(!err_debug.contains("secret-token-val"));

    let reqs = mock_server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 0);
}

#[test]
fn test_remote_http_rejection_and_loopback_allowed() {
    let options = RuntimeTransportOptions::default();

    // Insecure remote HTTP -> rejected
    let err = options
        .validate_url("http://api.openai.com/v1/chat/completions")
        .unwrap_err();
    assert!(matches!(
        err,
        TransportError::InsecureRemoteHttpRejected { .. }
    ));

    // Localhost / Loopback HTTP -> allowed
    assert!(
        options
            .validate_url("http://127.0.0.1:8080/v1/chat/completions")
            .is_ok()
    );
    assert!(
        options
            .validate_url("http://localhost:8080/v1/chat/completions")
            .is_ok()
    );
    assert!(
        options
            .validate_url("http://[::1]:8080/v1/chat/completions")
            .is_ok()
    );

    // HTTPS remote -> allowed
    assert!(
        options
            .validate_url("https://api.openai.com/v1/chat/completions")
            .is_ok()
    );
}

#[tokio::test]
async fn test_sse_split_multibyte_utf8() {
    let text = "Việt Nam 🇻🇳 xin chào";
    let json_prefix = format!(
        "{{\"id\":\"1\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{text}\"}},\"finish_reason\":null}}]}}"
    );
    let full_data = format!("data: {json_prefix}\n\n");
    let bytes = full_data.into_bytes();

    let mid = bytes.len() / 2;
    let chunk1 = &bytes[..mid];
    let chunk2 = &bytes[mid..];

    let mut parser = SseParser::default();
    let ev1 = parser.push_chunk(chunk1).unwrap();
    let ev2 = parser.push_chunk(chunk2).unwrap();

    let all_events = [ev1, ev2].concat();
    assert_eq!(all_events.len(), 1);
    assert!(all_events[0].data.contains(text));
}

#[test]
fn test_sse_frame_size_limit() {
    let mut parser = SseParser::new(100);
    let large_line = format!("data: {}\n\n", "A".repeat(150));
    let err = parser.push_chunk(large_line.as_bytes()).unwrap_err();
    match err {
        TransportError::SseFrameTooLarge { size, max_bytes } => {
            assert!(size > 100);
            assert_eq!(max_bytes, 100);
        }
        other => panic!("Unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn test_gemini_url_encoding() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/models/publishers%2Fgoogle%2Fmodels%2Fgemini-2.0-flash:streamGenerateContent",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: {\"responseId\":\"1\",\"candidates\":[{\"index\":0,\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"encoded model response\"}]},\"finishReason\":\"STOP\"}]}\n\n"),
        )
        .mount(&mock_server)
        .await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_KEY", "gem-key"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("google").unwrap(),
        "Mock Gemini Encoded",
        ProtocolFamily::GeminiGenerateContent,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::QueryParameter {
            parameter_name: "key".to_string(),
            secret: sec_ref,
        },
    )
    .unwrap();

    router.register_instance(instance);

    let desc = test_descriptor(instance_id, "publishers/google/models/gemini-2.0-flash");
    router
        .register_model(
            "publishers/google/models/gemini-2.0-flash",
            instance_id,
            desc,
        )
        .unwrap();

    let req = create_test_request("publishers/google/models/gemini-2.0-flash");
    let ctx = ModelInferenceContext {
        thread_id: "thread-gem-encoded".to_string(),
        turn_id: None,
    };

    let mut stream = router.stream(req, ctx).await.unwrap();
    let mut text = String::new();
    while let Some(Ok(ev)) = stream.next().await {
        if let ResponseEvent::OutputTextDelta(d) = ev {
            text.push_str(&d);
        }
    }
    assert_eq!(text, "encoded model response");
}

#[tokio::test]
async fn test_bounded_error_body_truncation() {
    let mock_server = MockServer::start().await;

    let massive_body = "E".repeat(50_000);
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string(massive_body))
        .mount(&mock_server)
        .await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_KEY", "sk-test"));
    let continuation_manager = Arc::new(ContinuationManager::new());

    let options = RuntimeTransportOptions {
        max_error_body_bytes: 256,
        ..Default::default()
    };

    let router = RuntimeRouter::new_with_options(
        secret_resolver,
        continuation_manager,
        options,
        Arc::new(agent_studios_runtime_transport::NoopRuntimeDiagnosticSink),
    )
    .unwrap();

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

    let desc = test_descriptor(instance_id, "gpt-4o");
    router.register_model("gpt-4o", instance_id, desc).unwrap();

    let req = create_test_request("gpt-4o");
    let ctx = ModelInferenceContext {
        thread_id: "thread-err-trunc".to_string(),
        turn_id: None,
    };

    let err = match router.stream(req, ctx).await {
        Err(e) => e,
        Ok(_) => panic!("Expected error for HTTP 500"),
    };
    let err_str = err.to_string();
    assert!(err_str.to_lowercase().contains("[truncated"));
    assert!(err_str.len() < 1000);
}

#[tokio::test]
async fn test_cross_instance_continuation_key_isolation() {
    let continuation_manager = Arc::new(ContinuationManager::new());

    let instance_1 = ProviderInstanceId::new();
    let instance_2 = ProviderInstanceId::new();
    let shared_thread_id = "shared-session-thread-1";

    let key_1 = ContinuationKey::new(instance_1, shared_thread_id);
    let key_2 = ContinuationKey::new(instance_2, shared_thread_id);

    // Acquire lease for instance 1 on thread
    let tx_1 = continuation_manager
        .begin_transaction(key_1.clone())
        .unwrap();

    // Instance 2 on SAME thread ID can concurrently begin transaction because instance IDs differ!
    let tx_2 = continuation_manager
        .begin_transaction(key_2.clone())
        .unwrap();

    assert!(continuation_manager.is_in_flight(&key_1));
    assert!(continuation_manager.is_in_flight(&key_2));

    tx_1.commit().unwrap();
    tx_2.commit().unwrap();
}

#[tokio::test]
async fn test_receiver_drop_before_completed() {
    let mock_server = MockServer::start().await;

    let sse_body = "event: message_start\n\
                    data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_drop_before\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-3-5-sonnet\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n\
                    event: content_block_start\n\
                    data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
                    event: content_block_delta\n\
                    data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello drop\"}}\n\n\
                    event: content_block_stop\n\
                    data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
                    event: message_delta\n\
                    data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n\
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
        locator: "TEST_KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_KEY", "ant-key"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, Arc::clone(&continuation_manager)).unwrap();

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("anthropic").unwrap(),
        "Mock Anthropic Drop",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: sec_ref,
        },
    )
    .unwrap();

    router.register_instance(instance);

    let desc = test_anthropic_descriptor(instance_id, "claude-3-5-sonnet");
    router
        .register_model("claude-3-5-sonnet", instance_id, desc)
        .unwrap();

    let thread_id = "thread-drop-before-completed";
    let req = create_test_request("claude-3-5-sonnet");
    let ctx = ModelInferenceContext {
        thread_id: thread_id.to_string(),
        turn_id: None,
    };

    let mut stream = router.stream(req, ctx).await.unwrap();

    // Consume until OutputTextDelta, then immediately drop stream before consuming Completed
    while let Some(ev_res) = stream.next().await {
        if let Ok(ResponseEvent::OutputTextDelta(_)) = ev_res {
            break;
        }
    }
    drop(stream);

    let key = ContinuationKey::new(instance_id, thread_id);
    for _ in 0..100 {
        if !continuation_manager.is_in_flight(&key) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    assert!(!continuation_manager.is_in_flight(&key));
    let state = continuation_manager.get(&key);
    assert!(state.anthropic.is_empty());
}

#[tokio::test]
async fn test_receiver_drop_mid_stream() {
    let mock_server = MockServer::start().await;

    let sse_body = "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"chunk 1\"},\"finish_reason\":null}]}\n\n\
                    data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"chunk 2\"},\"finish_reason\":null}]}\n\n\
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
        locator: "TEST_KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_KEY", "sk-test"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, Arc::clone(&continuation_manager)).unwrap();

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "Mock Drop Mid",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::BearerToken { secret: sec_ref },
    )
    .unwrap();

    router.register_instance(instance);

    let desc = test_descriptor(instance_id, "gpt-4o");
    router.register_model("gpt-4o", instance_id, desc).unwrap();

    let thread_id = "thread-drop-mid";
    let req = create_test_request("gpt-4o");
    let ctx = ModelInferenceContext {
        thread_id: thread_id.to_string(),
        turn_id: None,
    };

    let mut stream = router.stream(req, ctx).await.unwrap();

    // Consume first event and drop
    let _ = stream.next().await;
    drop(stream);

    let key = ContinuationKey::new(instance_id, thread_id);
    for _ in 0..100 {
        if !continuation_manager.is_in_flight(&key) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    assert!(!continuation_manager.is_in_flight(&key));
}

#[tokio::test]
async fn test_request_headers_timeout() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: [DONE]\n\n")
                .set_delay(std::time::Duration::from_millis(500)),
        )
        .mount(&mock_server)
        .await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_KEY", "sk-test"));
    let continuation_manager = Arc::new(ContinuationManager::new());

    let options = RuntimeTransportOptions {
        request_headers_timeout: std::time::Duration::from_millis(50),
        ..Default::default()
    };

    let router = RuntimeRouter::new_with_options(
        secret_resolver,
        continuation_manager,
        options,
        Arc::new(agent_studios_runtime_transport::NoopRuntimeDiagnosticSink),
    )
    .unwrap();

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "Mock Timeout Instance",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::BearerToken { secret: sec_ref },
    )
    .unwrap();

    router.register_instance(instance);

    let desc = test_descriptor(instance_id, "gpt-4o");
    router.register_model("gpt-4o", instance_id, desc).unwrap();

    let req = create_test_request("gpt-4o");
    let ctx = ModelInferenceContext {
        thread_id: "thread-headers-timeout".to_string(),
        turn_id: None,
    };

    let err = match router.stream(req, ctx).await {
        Err(e) => e,
        Ok(_) => panic!("Expected timeout error"),
    };
    assert!(
        err.to_string().contains("Request headers timeout")
            || err.to_string().contains("timed out")
    );
}

#[tokio::test]
async fn test_stream_idle_timeout() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", local_addr);

    let server_handle = tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;

            let response_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: keep-alive\r\n\r\n";
            let _ = socket.write_all(response_headers.as_bytes()).await;

            let first_event = "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"start\"},\"finish_reason\":null}]}\n\n";
            let _ = socket.write_all(first_event.as_bytes()).await;
            let _ = socket.flush().await;

            // Keep connection open and stall without sending any more data
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    });

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_KEY", "sk-test"));
    let continuation_manager = Arc::new(ContinuationManager::new());

    let options = RuntimeTransportOptions {
        stream_idle_timeout: std::time::Duration::from_millis(50),
        ..Default::default()
    };

    let router = RuntimeRouter::new_with_options(
        secret_resolver,
        Arc::clone(&continuation_manager),
        options,
        Arc::new(agent_studios_runtime_transport::NoopRuntimeDiagnosticSink),
    )
    .unwrap();

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "Mock Idle Timeout",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(&base_url).unwrap(),
        AuthenticationScheme::BearerToken { secret: sec_ref },
    )
    .unwrap();

    router.register_instance(instance);

    let desc = test_descriptor(instance_id, "gpt-4o");
    router.register_model("gpt-4o", instance_id, desc).unwrap();

    let thread_id = "thread-idle-timeout";
    let req = create_test_request("gpt-4o");
    let ctx = ModelInferenceContext {
        thread_id: thread_id.to_string(),
        turn_id: None,
    };

    let mut stream = router.stream(req, ctx).await.unwrap();

    // Consume initial events emitted from the first chunk
    let first = stream.next().await;
    assert!(matches!(first, Some(Ok(ResponseEvent::Created { .. }))));

    let second = stream.next().await;
    assert!(matches!(
        second,
        Some(Ok(ResponseEvent::OutputItemAdded(_)))
    ));

    let third = stream.next().await;
    assert!(matches!(third, Some(Ok(ResponseEvent::OutputTextDelta(ref d))) if d == "start"));

    // Next event MUST be StreamIdleTimeout (unconditional assert, not if let)
    let fourth = stream.next().await;
    match fourth {
        Some(Err(e)) => {
            let err_str = e.to_string();
            assert!(
                err_str.contains("Stream idle timeout"),
                "Expected Stream idle timeout error, got: {err_str}"
            );
        }
        other => panic!("Expected Some(Err(StreamIdleTimeout)), got: {other:?}"),
    }

    // Stream must terminate after error
    let fifth = stream.next().await;
    assert!(fifth.is_none());

    let key = ContinuationKey::new(instance_id, thread_id);
    assert!(!continuation_manager.is_in_flight(&key));

    let _ = server_handle.await;
}

#[tokio::test]
async fn test_model_route_not_found_fail_closed() {
    let secret_resolver = Arc::new(InMemorySecretResolver::new());
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let req = create_test_request("unregistered-custom-model");
    let ctx = ModelInferenceContext {
        thread_id: "thread-route-404".to_string(),
        turn_id: None,
    };

    let err = match router.stream(req, ctx).await {
        Err(e) => e,
        Ok(_) => panic!("Expected error for unrouted model"),
    };
    assert!(err.to_string().contains("No route configured for model"));
}

#[tokio::test]
async fn test_register_route_validation() {
    let secret_resolver = Arc::new(InMemorySecretResolver::new());
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let instance_1 = ProviderInstanceId::new();
    let instance_2 = ProviderInstanceId::new();

    let instance = ProviderInstance::new(
        instance_1,
        ProviderId::new("openai").unwrap(),
        "Instance 1",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("http://localhost:8080").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    router.register_instance(instance);

    // Descriptor has instance_2, but route has instance_1
    let desc_mismatch = test_descriptor(instance_2, "gpt-4o");
    let route = RuntimeModelRoute::new(instance_1, desc_mismatch);
    let err = router.register_route("gpt-4o", route).unwrap_err();
    assert!(matches!(err, TransportError::InvalidModelRoute(_)));

    // Model string mismatch
    let desc_id_mismatch = test_descriptor(instance_1, "gpt-4o");
    let route2 = RuntimeModelRoute::new(instance_1, desc_id_mismatch);
    let err2 = router
        .register_route("different-model-name", route2)
        .unwrap_err();
    assert!(matches!(err2, TransportError::InvalidModelRoute(_)));

    // Provider not found
    let desc_p_missing = test_descriptor(instance_2, "gpt-4o");
    let route3 = RuntimeModelRoute::new(instance_2, desc_p_missing);
    let err3 = router.register_route("gpt-4o", route3).unwrap_err();
    assert!(matches!(err3, TransportError::ProviderNotFound(_)));
}

#[tokio::test]
async fn test_catalog_descriptor_duplicate_model_id_across_instances() {
    let instance_1 = ProviderInstanceId::new();
    let instance_2 = ProviderInstanceId::new();

    let desc1 = test_descriptor(instance_1, "gpt-4o");
    let desc2 = test_descriptor(instance_2, "gpt-4o");

    assert_eq!(desc1.id.as_str(), "gpt-4o");
    assert_eq!(desc2.id.as_str(), "gpt-4o");
    assert_ne!(desc1.provider_instance_id, desc2.provider_instance_id);

    // Both descriptors exist concurrently and are valid for their respective instances
    assert_eq!(desc1.provider_instance_id, instance_1);
    assert_eq!(desc2.provider_instance_id, instance_2);
}

#[tokio::test]
async fn test_single_router_duplicate_model_route_rejection() {
    let secret_resolver = Arc::new(InMemorySecretResolver::new());
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let instance_1 = ProviderInstanceId::new();
    let instance_2 = ProviderInstanceId::new();

    let p1 = ProviderInstance::new(
        instance_1,
        ProviderId::new("openai").unwrap(),
        "OpenAI East",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("http://localhost:8080").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    let p2 = ProviderInstance::new(
        instance_2,
        ProviderId::new("openai").unwrap(),
        "OpenAI West",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("http://localhost:8081").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    router.register_instance(p1);
    router.register_instance(p2);

    let desc1 = test_descriptor(instance_1, "gpt-4o");
    let desc2 = test_descriptor(instance_2, "gpt-4o");

    // Register instance 1 for gpt-4o
    router
        .register_model("gpt-4o", instance_1, desc1.clone())
        .unwrap();

    // Idempotent re-registration of exact same route succeeds
    assert!(router.register_model("gpt-4o", instance_1, desc1).is_ok());

    // Conflicting re-registration targeting a different instance fails closed with DuplicateModelRoute
    let err = router
        .register_model("gpt-4o", instance_2, desc2)
        .unwrap_err();
    match err {
        TransportError::DuplicateModelRoute {
            model,
            existing_instance_id,
        } => {
            assert_eq!(model, "gpt-4o");
            assert_eq!(existing_instance_id, instance_1);
        }
        other => panic!("Expected DuplicateModelRoute, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_separate_routers_bind_same_slug_to_different_instances() {
    let secret_resolver_1 = Arc::new(InMemorySecretResolver::new());
    let cont_manager_1 = Arc::new(ContinuationManager::new());
    let router_1 = RuntimeRouter::new(secret_resolver_1, cont_manager_1).unwrap();

    let secret_resolver_2 = Arc::new(InMemorySecretResolver::new());
    let cont_manager_2 = Arc::new(ContinuationManager::new());
    let router_2 = RuntimeRouter::new(secret_resolver_2, cont_manager_2).unwrap();

    let instance_1 = ProviderInstanceId::new();
    let instance_2 = ProviderInstanceId::new();

    let p1 = ProviderInstance::new(
        instance_1,
        ProviderId::new("openai").unwrap(),
        "OpenAI East",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("http://localhost:8080").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    let p2 = ProviderInstance::new(
        instance_2,
        ProviderId::new("openai").unwrap(),
        "OpenAI West",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("http://localhost:8081").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    router_1.register_instance(p1);
    router_2.register_instance(p2);

    let desc1 = test_descriptor(instance_1, "gpt-4o");
    let desc2 = test_descriptor(instance_2, "gpt-4o");

    // Router 1 binds gpt-4o to instance 1
    router_1
        .register_model("gpt-4o", instance_1, desc1)
        .unwrap();
    // Router 2 binds gpt-4o to instance 2
    router_2
        .register_model("gpt-4o", instance_2, desc2)
        .unwrap();

    let req = create_test_request("gpt-4o");
    let (inst_1, desc_res_1) = router_1.resolve_route(&req).unwrap();
    assert_eq!(inst_1.id, instance_1);
    assert_eq!(desc_res_1.provider_instance_id, instance_1);

    let (inst_2, desc_res_2) = router_2.resolve_route(&req).unwrap();
    assert_eq!(inst_2.id, instance_2);
    assert_eq!(desc_res_2.provider_instance_id, instance_2);
}

#[tokio::test]
async fn test_gemini_alt_case_a_runtime_inserts_alt() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path_regex(r"^/models/.*:streamGenerateContent$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: {\"responseId\":\"1\",\"candidates\":[{\"index\":0,\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"alt A\"}]},\"finishReason\":\"STOP\"}]}\n\n"),
        )
        .mount(&mock_server)
        .await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("KEY", "mock-val"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("google").unwrap(),
        "Gemini Alt Case A",
        ProtocolFamily::GeminiGenerateContent,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::QueryParameter {
            parameter_name: "key".to_string(),
            secret: sec_ref,
        },
    )
    .unwrap();

    router.register_instance(instance);
    let desc = test_descriptor(instance_id, "gemini-2.5-flash");
    router
        .register_model("gemini-2.5-flash", instance_id, desc)
        .unwrap();

    let req = create_test_request("gemini-2.5-flash");
    let ctx = ModelInferenceContext {
        thread_id: "thread-alt-case-a".to_string(),
        turn_id: None,
    };
    let mut stream = router.stream(req, ctx).await.unwrap();
    while let Some(res) = stream.next().await {
        let _ = res.unwrap();
    }

    let reqs = mock_server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 1);
    let alts: Vec<_> = reqs[0]
        .url
        .query_pairs()
        .filter(|(k, _)| k == "alt")
        .collect();
    assert_eq!(
        alts.len(),
        1,
        "alt query parameter must appear exactly once"
    );
    assert_eq!(alts[0].1, "sse");
}

#[tokio::test]
async fn test_gemini_alt_case_b_base_url_has_alt() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path_regex(r"^/v1/models/.*:streamGenerateContent$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: {\"responseId\":\"1\",\"candidates\":[{\"index\":0,\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"alt B\"}]},\"finishReason\":\"STOP\"}]}\n\n"),
        )
        .mount(&mock_server)
        .await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("KEY", "mock-val"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let instance_id = ProviderInstanceId::new();
    let base_url = format!("{}/v1?alt=sse", mock_server.uri());
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("google").unwrap(),
        "Gemini Alt Case B",
        ProtocolFamily::GeminiGenerateContent,
        EndpointProfile::new(&base_url).unwrap(),
        AuthenticationScheme::QueryParameter {
            parameter_name: "key".to_string(),
            secret: sec_ref,
        },
    )
    .unwrap();

    router.register_instance(instance);
    let desc = test_descriptor(instance_id, "gemini-2.5-flash");
    router
        .register_model("gemini-2.5-flash", instance_id, desc)
        .unwrap();

    let req = create_test_request("gemini-2.5-flash");
    let ctx = ModelInferenceContext {
        thread_id: "thread-alt-case-b".to_string(),
        turn_id: None,
    };
    let mut stream = router.stream(req, ctx).await.unwrap();
    while let Some(res) = stream.next().await {
        let _ = res.unwrap();
    }

    let reqs = mock_server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 1);
    let alts: Vec<_> = reqs[0]
        .url
        .query_pairs()
        .filter(|(k, _)| k == "alt")
        .collect();
    assert_eq!(
        alts.len(),
        1,
        "alt query parameter must appear exactly once"
    );
    assert_eq!(alts[0].1, "sse");
}

#[tokio::test]
async fn test_gemini_alt_case_c_endpoint_query_has_alt() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path_regex(r"^/models/.*:streamGenerateContent$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: {\"responseId\":\"1\",\"candidates\":[{\"index\":0,\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"alt C\"}]},\"finishReason\":\"STOP\"}]}\n\n"),
        )
        .mount(&mock_server)
        .await;

    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "KEY".to_string(),
    };
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("KEY", "mock-val"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let instance_id = ProviderInstanceId::new();
    let mut ep = EndpointProfile::new(mock_server.uri()).unwrap();
    ep.query_params.insert("alt".to_string(), "sse".to_string());

    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("google").unwrap(),
        "Gemini Alt Case C",
        ProtocolFamily::GeminiGenerateContent,
        ep,
        AuthenticationScheme::QueryParameter {
            parameter_name: "key".to_string(),
            secret: sec_ref,
        },
    )
    .unwrap();

    router.register_instance(instance);
    let desc = test_descriptor(instance_id, "gemini-2.5-flash");
    router
        .register_model("gemini-2.5-flash", instance_id, desc)
        .unwrap();

    let req = create_test_request("gemini-2.5-flash");
    let ctx = ModelInferenceContext {
        thread_id: "thread-alt-case-c".to_string(),
        turn_id: None,
    };
    let mut stream = router.stream(req, ctx).await.unwrap();
    while let Some(res) = stream.next().await {
        let _ = res.unwrap();
    }

    let reqs = mock_server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 1);
    let alts: Vec<_> = reqs[0]
        .url
        .query_pairs()
        .filter(|(k, _)| k == "alt")
        .collect();
    assert_eq!(
        alts.len(),
        1,
        "alt query parameter must appear exactly once"
    );
    assert_eq!(alts[0].1, "sse");
}

#[tokio::test]
async fn test_gemini_alt_case_d_both_sources_alt_rejected() {
    let mock_server = MockServer::start().await;

    let secret_resolver = Arc::new(InMemorySecretResolver::new());
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let instance_id = ProviderInstanceId::new();
    let base_url = format!("{}/v1?alt=sse", mock_server.uri());
    let mut ep = EndpointProfile::new(&base_url).unwrap();
    ep.query_params.insert("alt".to_string(), "sse".to_string());

    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("google").unwrap(),
        "Gemini Alt Case D",
        ProtocolFamily::GeminiGenerateContent,
        ep,
        AuthenticationScheme::None,
    )
    .unwrap();

    router.register_instance(instance);
    let desc = test_descriptor(instance_id, "gemini-2.5-flash");
    router
        .register_model("gemini-2.5-flash", instance_id, desc)
        .unwrap();

    let req = create_test_request("gemini-2.5-flash");
    let ctx = ModelInferenceContext {
        thread_id: "thread-alt-case-d".to_string(),
        turn_id: None,
    };

    let err = match router.stream(req, ctx).await {
        Err(e) => e,
        Ok(_) => panic!("Expected error, got Ok stream"),
    };
    assert!(err.to_string().contains("Reserved query parameter 'alt'"));

    // Verify no HTTP request was sent
    let reqs = mock_server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 0);
}

#[tokio::test]
async fn test_gemini_alt_case_e_invalid_alt_value_rejected() {
    let mock_server = MockServer::start().await;

    let secret_resolver = Arc::new(InMemorySecretResolver::new());
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let instance_id = ProviderInstanceId::new();
    let base_url = format!("{}/v1?alt=json", mock_server.uri());
    let ep = EndpointProfile::new(&base_url).unwrap();

    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("google").unwrap(),
        "Gemini Alt Case E",
        ProtocolFamily::GeminiGenerateContent,
        ep,
        AuthenticationScheme::None,
    )
    .unwrap();

    router.register_instance(instance);
    let desc = test_descriptor(instance_id, "gemini-2.5-flash");
    router
        .register_model("gemini-2.5-flash", instance_id, desc)
        .unwrap();

    let req = create_test_request("gemini-2.5-flash");
    let ctx = ModelInferenceContext {
        thread_id: "thread-alt-case-e".to_string(),
        turn_id: None,
    };

    let err = match router.stream(req, ctx).await {
        Err(e) => e,
        Ok(_) => panic!("Expected error, got Ok stream"),
    };
    assert!(err.to_string().contains("Reserved query parameter 'alt'"));

    // Verify no HTTP request was sent
    let reqs = mock_server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 0);
}

#[tokio::test]
async fn test_gemini_request_id_headers_priority() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path_regex(r"^/models/.*:streamGenerateContent$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .insert_header("x-goog-request-id", "goog-req-id-123")
                .insert_header("x-request-id", "generic-req-id-456")
                .set_body_string("data: {\"responseId\":\"1\",\"candidates\":[{\"index\":0,\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"header test\"}]},\"finishReason\":\"STOP\"}]}\n\n"),
        )
        .mount(&mock_server)
        .await;

    let secret_resolver = Arc::new(InMemorySecretResolver::new());
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("google").unwrap(),
        "Gemini Headers Priority",
        ProtocolFamily::GeminiGenerateContent,
        EndpointProfile::new(mock_server.uri()).unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    router.register_instance(instance);
    let desc = test_descriptor(instance_id, "gemini-2.5-flash");
    router
        .register_model("gemini-2.5-flash", instance_id, desc)
        .unwrap();

    let req = create_test_request("gemini-2.5-flash");
    let ctx = ModelInferenceContext {
        thread_id: "thread-headers-prio".to_string(),
        turn_id: None,
    };
    let stream = router.stream(req, ctx).await.unwrap();
    assert_eq!(
        stream.upstream_request_id.as_deref(),
        Some("goog-req-id-123")
    );
}

#[tokio::test]
async fn test_base_url_with_query_preserved() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: [DONE]\n\n"),
        )
        .mount(&mock_server)
        .await;

    let base_with_query = format!("{}/v1?region=us-east-1&custom=flag", mock_server.uri());

    let secret_resolver = Arc::new(InMemorySecretResolver::new());
    let continuation_manager = Arc::new(ContinuationManager::new());
    let router = RuntimeRouter::new(secret_resolver, continuation_manager).unwrap();

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "Query Preserved",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(&base_with_query).unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    router.register_instance(instance);
    let desc = test_descriptor(instance_id, "gpt-4o");
    router.register_model("gpt-4o", instance_id, desc).unwrap();

    let req = create_test_request("gpt-4o");
    let ctx = ModelInferenceContext {
        thread_id: "thread-query-pres".to_string(),
        turn_id: None,
    };
    let stream = router.stream(req, ctx).await;
    assert!(stream.is_ok());
}

#[test]
fn test_arbitrary_query_secret_sanitization() {
    let raw = "Error sending to https://example.com/v1?token=secret12345&access_token=secret67890&custom=ok";
    let sanitized = sanitize_error_message(raw);
    assert!(!sanitized.contains("secret12345"));
    assert!(!sanitized.contains("secret67890"));
    assert!(sanitized.contains("token=[REDACTED]"));
    assert!(sanitized.contains("access_token=[REDACTED]"));
    assert!(sanitized.contains("custom=ok"));
}

#[test]
fn test_read_bounded_error_body_redacts_auth_secrets() {
    let mut auth = ResolvedAuth::default();
    let mut hval = reqwest::header::HeaderValue::from_str("Bearer super-secret-jwt-token").unwrap();
    hval.set_sensitive(true);
    auth.headers.insert(reqwest::header::AUTHORIZATION, hval);
    auth.query_params.push((
        "key".to_string(),
        agent_studios_runtime_transport::SecretString::new("custom-gemini-secret-api-key"),
    ));

    let body = "Error 401: Unauthorized access with token super-secret-jwt-token or key custom-gemini-secret-api-key";
    let redacted = auth.redact_secrets(body);
    assert!(!redacted.contains("super-secret-jwt-token"));
    assert!(!redacted.contains("custom-gemini-secret-api-key"));
    assert!(redacted.contains("[REDACTED]"));
}

#[test]
fn test_sse_finish_size_limit() {
    let mut parser = SseParser::new(50);
    // Push chunk close to 50
    let _ = parser.push_chunk(b"data: 12345678901234567890\n");
    // Trailing data without newline
    let _ = parser.push_chunk(b"data: 1234567890123456789012345678901234567890");
    let err = parser.finish().unwrap_err();
    assert!(matches!(err, TransportError::SseFrameTooLarge { .. }));
}
