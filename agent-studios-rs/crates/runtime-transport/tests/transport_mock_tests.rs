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
    let router = Arc::new(RuntimeRouter::new(
        secret_resolver,
        Arc::clone(&continuation_manager),
    ));

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
    router.set_active_instance(instance_id);

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
    let router = RuntimeRouter::new(secret_resolver, Arc::clone(&continuation_manager));

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
    router.set_active_instance(instance_id);

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
    let router = RuntimeRouter::new(secret_resolver, continuation_manager);

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
    router.register_model("claude-test-no-limit", instance_id, descriptor);

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
    );

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
    router.register_model("model-no-stream", instance_id, desc_no_stream);

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
    router.register_model("model-unknown-stream", instance_id, desc_unknown_stream);

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
    let router = RuntimeRouter::new(secret_resolver, continuation_manager);

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
    router.register_route("custom-routed-model", route);

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

    let err = auth
        .check_collisions(static_headers.iter(), query_params.iter())
        .unwrap_err();
    match err {
        TransportError::AuthenticationCollision { name, location } => {
            assert_eq!(name, "authorization");
            assert_eq!(location, "static_headers");
        }
        other => panic!("Unexpected error: {other:?}"),
    }
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
    let router = RuntimeRouter::new(secret_resolver, continuation_manager);

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
    router.set_active_instance(instance_id);

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
    );

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
