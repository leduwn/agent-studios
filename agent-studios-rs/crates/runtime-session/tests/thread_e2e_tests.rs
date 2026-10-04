use std::sync::Arc;
use std::time::Duration;

use agent_studios_provider::ProviderCatalog;
use agent_studios_provider::ProviderDefinition;
use agent_studios_provider::auth::AuthenticationScheme;
use agent_studios_provider::capabilities::ModelCapabilities;
use agent_studios_provider::id::{ModelId, ProviderId, ProviderInstanceId};
use agent_studios_provider::instance::ProviderInstance;
use agent_studios_provider::model::{ModelDescriptor, ModelLimits, ModelRef};
use agent_studios_provider::protocol::ProtocolFamily;
use agent_studios_provider::secret::{SecretBackend, SecretReference};
use agent_studios_runtime_session::AgentStudiosRuntimeSessionFactory;
use agent_studios_runtime_transport::{
    ContinuationManager, InMemorySecretResolver, RuntimeTransportOptions,
};
use codex_config::LoaderOverrides;
use codex_core::config::{Config, ConfigBuilder};
use codex_core::{
    StartThreadOptions, ThreadManager, TurnInputRequest, init_state_db, passthrough_image_store,
    resolve_installation_id, thread_store_from_config,
};
use codex_extension_api::empty_extension_registry;
use codex_home::CodexHomeUserInstructionsProvider;
use codex_login::{AuthManager, CodexAuth};
use codex_protocol::protocol::{EventMsg, InternalSessionSource, SessionSource};
use codex_protocol::user_input::UserInput;
use codex_utils_absolute_path::AbsolutePathBuf;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn register_provider_and_instance(
    catalog: &mut ProviderCatalog,
    provider_slug: &str,
    provider_name: &str,
    protocol: ProtocolFamily,
    base_url: &str,
    auth_scheme: AuthenticationScheme,
) -> ProviderInstanceId {
    let def_id = ProviderId::new(provider_slug).unwrap();
    let def =
        ProviderDefinition::new(def_id.clone(), provider_name, vec![protocol.clone()]).unwrap();
    let _ = catalog.register_provider_definition(def);
    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        def_id,
        provider_name,
        protocol,
        agent_studios_provider::endpoint::EndpointProfile::new(base_url).unwrap(),
        auth_scheme,
    )
    .unwrap();
    catalog.register_provider_instance(instance).unwrap();
    instance_id
}

async fn create_test_thread_manager(codex_home: &std::path::Path) -> (ThreadManager, Config) {
    let mut config = ConfigBuilder::default()
        .loader_overrides(LoaderOverrides::without_managed_config_for_tests())
        .codex_home(codex_home.to_path_buf())
        .build()
        .await
        .expect("load default test config");

    config.cwd = AbsolutePathBuf::from_absolute_path(codex_home).expect("cwd path");

    let state_db = init_state_db(&config).await;
    let thread_store = thread_store_from_config(&config, state_db);
    let installation_id = resolve_installation_id(&config.codex_home)
        .await
        .unwrap_or_else(|_| "test-installation-id".to_string());
    let user_instructions_provider = Arc::new(CodexHomeUserInstructionsProvider::new(
        config.codex_home.clone(),
    ));
    let auth_manager = AuthManager::from_auth_for_testing_with_home(
        CodexAuth::from_api_key("dummy"),
        config.codex_home.to_path_buf(),
    );
    let models_manager = codex_core::build_models_manager(&config, auth_manager.clone());
    let environment_manager = Arc::new(codex_exec_server::EnvironmentManager::default_for_tests());
    let extensions = empty_extension_registry();
    let image_store = passthrough_image_store();

    let manager = ThreadManager::new(
        &config,
        auth_manager,
        models_manager,
        codex_core::CodexAppsToolsCache::default(),
        SessionSource::Exec,
        environment_manager,
        extensions,
        user_instructions_provider,
        None,
        image_store,
        thread_store,
        None,
        installation_id,
        None,
        None,
    );
    (manager, config)
}

async fn run_turn_to_completion(
    thread: &codex_core::CodexThread,
    prompt: &str,
) -> (bool, Option<String>, Vec<String>) {
    thread
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: prompt.to_string(),
            text_elements: Vec::new(),
        }]))
        .await
        .expect("turn input accepted");

    let mut got_turn_complete = false;
    let mut got_error = None;
    let mut messages = Vec::new();

    let timeout_deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < timeout_deadline {
        match tokio::time::timeout(Duration::from_millis(500), thread.next_event()).await {
            Ok(Ok(event)) => match event.msg {
                EventMsg::AgentMessage(msg) => {
                    messages.push(msg.message);
                }
                EventMsg::TurnComplete(_) => {
                    got_turn_complete = true;
                    break;
                }
                EventMsg::Error(err) => {
                    got_error = Some(err.message);
                    break;
                }
                _ => {}
            },
            Ok(Err(_)) => break,
            Err(_) => {}
        }
    }

    let _ = thread.shutdown_and_wait().await;
    (got_turn_complete, got_error, messages)
}

#[tokio::test]
async fn test_thread_e2e_openai_chat_completions() {
    let mock_server = MockServer::start().await;

    let sse_body = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\n\
                    data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello from OpenAI ChatCompletions!\"},\"finish_reason\":null}]}\n\n\
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

    let temp_dir = tempfile::tempdir().unwrap();
    let (manager, config) = create_test_thread_manager(temp_dir.path()).await;

    let mut catalog = ProviderCatalog::new();
    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_KEY".to_string(),
    };
    let instance_id = register_provider_and_instance(
        &mut catalog,
        "openai-chat",
        "Mock ChatCompletions",
        ProtocolFamily::OpenAiChatCompletions,
        &mock_server.uri(),
        AuthenticationScheme::BearerToken { secret: sec_ref },
    );

    let model_id = ModelId::new("gpt-4o-mini").unwrap();
    let desc = ModelDescriptor::new(
        instance_id,
        model_id.clone(),
        "GPT-4o Mini",
        ModelCapabilities::default(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(desc).unwrap();

    let catalog = Arc::new(catalog);
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_KEY", "sk-mock-key"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let factory =
        AgentStudiosRuntimeSessionFactory::new(catalog, secret_resolver, continuation_manager)
            .with_transport_options(RuntimeTransportOptions::default());

    let model_ref = ModelRef::new(instance_id, model_id);
    let prepared = factory.prepare_runtime_session(&model_ref).unwrap();

    let mut options = StartThreadOptions::new(config);
    prepared.apply_to_start_thread_options(&mut options);

    let new_thread = manager.start_thread(options).await.unwrap();
    let (completed, error, messages) =
        run_turn_to_completion(&new_thread.thread, "Say hello").await;

    assert!(
        completed,
        "Turn should complete successfully, got error: {:?}",
        error
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Hello from OpenAI ChatCompletions!")),
        "Expected response message, got: {:?}",
        messages
    );

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/chat/completions");

    // Assert /models discovery was never called
    assert!(requests.iter().all(|r| !r.url.path().contains("/models")));
}

#[tokio::test]
async fn test_thread_e2e_anthropic_messages() {
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

    let temp_dir = tempfile::tempdir().unwrap();
    let (manager, config) = create_test_thread_manager(temp_dir.path()).await;

    let mut catalog = ProviderCatalog::new();
    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_ANTHROPIC_KEY".to_string(),
    };
    let instance_id = register_provider_and_instance(
        &mut catalog,
        "anthropic",
        "Mock Anthropic",
        ProtocolFamily::AnthropicMessages,
        &mock_server.uri(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: sec_ref,
        },
    );

    let model_id = ModelId::new("claude-3-5-sonnet").unwrap();
    let desc = ModelDescriptor::new(
        instance_id,
        model_id.clone(),
        "Claude 3.5 Sonnet",
        ModelCapabilities::default(),
        ModelLimits {
            context_window_tokens: Some(200_000),
            max_output_tokens: Some(4096),
        },
    )
    .unwrap();
    catalog.register_model(desc).unwrap();

    let catalog = Arc::new(catalog);
    let secret_resolver = Arc::new(
        InMemorySecretResolver::new().with_env_secret("TEST_ANTHROPIC_KEY", "ant-mock-key"),
    );
    let continuation_manager = Arc::new(ContinuationManager::new());
    let factory =
        AgentStudiosRuntimeSessionFactory::new(catalog, secret_resolver, continuation_manager);

    let model_ref = ModelRef::new(instance_id, model_id);
    let prepared = factory.prepare_runtime_session(&model_ref).unwrap();

    let mut options = StartThreadOptions::new(config);
    prepared.apply_to_start_thread_options(&mut options);

    let new_thread = manager.start_thread(options).await.unwrap();
    let (completed, error, messages) =
        run_turn_to_completion(&new_thread.thread, "Say hello").await;

    assert!(completed, "Turn should complete, got error: {:?}", error);
    assert!(
        messages.iter().any(|m| m.contains("Hello from Anthropic!")),
        "Expected response message, got: {:?}",
        messages
    );

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/messages");
    assert!(requests.iter().all(|r| !r.url.path().contains("/models")));
}

#[tokio::test]
async fn test_thread_e2e_gemini_generate_content() {
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

    let temp_dir = tempfile::tempdir().unwrap();
    let (manager, config) = create_test_thread_manager(temp_dir.path()).await;

    let mut catalog = ProviderCatalog::new();
    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_GEMINI_KEY".to_string(),
    };
    let instance_id = register_provider_and_instance(
        &mut catalog,
        "google",
        "Mock Gemini",
        ProtocolFamily::GeminiGenerateContent,
        &mock_server.uri(),
        AuthenticationScheme::QueryParameter {
            parameter_name: "key".to_string(),
            secret: sec_ref,
        },
    );

    let model_id = ModelId::new("gemini-2.5-flash").unwrap();
    let desc = ModelDescriptor::new(
        instance_id,
        model_id.clone(),
        "Gemini 2.5 Flash",
        ModelCapabilities::default(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(desc).unwrap();

    let catalog = Arc::new(catalog);
    let secret_resolver = Arc::new(
        InMemorySecretResolver::new().with_env_secret("TEST_GEMINI_KEY", "gemini-mock-key"),
    );
    let continuation_manager = Arc::new(ContinuationManager::new());
    let factory =
        AgentStudiosRuntimeSessionFactory::new(catalog, secret_resolver, continuation_manager);

    let model_ref = ModelRef::new(instance_id, model_id);
    let prepared = factory.prepare_runtime_session(&model_ref).unwrap();

    let mut options = StartThreadOptions::new(config);
    prepared.apply_to_start_thread_options(&mut options);

    let new_thread = manager.start_thread(options).await.unwrap();
    let (completed, error, messages) =
        run_turn_to_completion(&new_thread.thread, "Say hello").await;

    assert!(completed, "Turn should complete, got error: {:?}", error);
    assert!(
        messages.iter().any(|m| m.contains("Hello from Gemini!")),
        "Expected response message, got: {:?}",
        messages
    );

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].url.path().contains(":streamGenerateContent"));
    assert!(requests.iter().all(|r| r.method != "GET"));
}

#[tokio::test]
async fn test_thread_e2e_openai_responses_native_bridge() {
    let mock_server = MockServer::start().await;

    let sse_body = "event: response.created\n\
                    data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp-1\"}}\n\n\
                    event: response.output_item.added\n\
                    data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg-1\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"\"}]}}\n\n\
                    event: response.output_text.delta\n\
                    data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"delta\":\"Hello from Native Responses!\"}\n\n\
                    event: response.output_item.done\n\
                    data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg-1\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Hello from Native Responses!\"}]}}\n\n\
                    event: response.completed\n\
                    data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-1\",\"usage\":{\"input_tokens\":10,\"input_tokens_details\":null,\"output_tokens\":5,\"output_tokens_details\":null,\"total_tokens\":15}}}\n\n";

    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body),
        )
        .mount(&mock_server)
        .await;

    let temp_dir = tempfile::tempdir().unwrap();
    let (manager, config) = create_test_thread_manager(temp_dir.path()).await;

    let mut catalog = ProviderCatalog::new();
    let instance_id = register_provider_and_instance(
        &mut catalog,
        "openai-responses",
        "Mock Responses",
        ProtocolFamily::OpenAiResponses,
        &mock_server.uri(),
        AuthenticationScheme::None,
    );

    let model_id = ModelId::new("gpt-5.5").unwrap();
    let desc = ModelDescriptor::new(
        instance_id,
        model_id.clone(),
        "GPT-5.5",
        ModelCapabilities::default(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(desc).unwrap();

    let catalog = Arc::new(catalog);
    let secret_resolver = Arc::new(InMemorySecretResolver::new());
    let continuation_manager = Arc::new(ContinuationManager::new());
    let factory =
        AgentStudiosRuntimeSessionFactory::new(catalog, secret_resolver, continuation_manager);

    let model_ref = ModelRef::new(instance_id, model_id);
    let prepared = factory.prepare_runtime_session(&model_ref).unwrap();

    let mut options = StartThreadOptions::new(config);
    prepared.apply_to_start_thread_options(&mut options);

    let new_thread = manager.start_thread(options).await.unwrap();
    let (completed, error, messages) =
        run_turn_to_completion(&new_thread.thread, "Say hello").await;

    assert!(completed, "Turn should complete, got error: {:?}", error);
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Hello from Native Responses!")),
        "Expected response message, got: {:?}",
        messages
    );

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/responses");
    assert!(requests.iter().all(|r| !r.url.path().contains("/models")));
}

#[tokio::test]
async fn test_thread_e2e_custom_protocol_failure_no_silent_fallback_to_responses() {
    let mock_server = MockServer::start().await;

    // Fail chat completions
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal server error"))
        .mount(&mock_server)
        .await;

    // Mount /responses returning 200 to ensure it is never reached
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Should not be called"))
        .mount(&mock_server)
        .await;

    let temp_dir = tempfile::tempdir().unwrap();
    let (manager, config) = create_test_thread_manager(temp_dir.path()).await;

    let mut catalog = ProviderCatalog::new();
    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_FAIL_KEY".to_string(),
    };
    let instance_id = register_provider_and_instance(
        &mut catalog,
        "openai-fail",
        "Mock Fail ChatCompletions",
        ProtocolFamily::OpenAiChatCompletions,
        &mock_server.uri(),
        AuthenticationScheme::BearerToken { secret: sec_ref },
    );

    let model_id = ModelId::new("gpt-4o-mini").unwrap();
    let desc = ModelDescriptor::new(
        instance_id,
        model_id.clone(),
        "GPT-4o Mini",
        ModelCapabilities::default(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(desc).unwrap();

    let catalog = Arc::new(catalog);
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_FAIL_KEY", "sk-mock-key"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let factory =
        AgentStudiosRuntimeSessionFactory::new(catalog, secret_resolver, continuation_manager);

    let model_ref = ModelRef::new(instance_id, model_id);
    let prepared = factory.prepare_runtime_session(&model_ref).unwrap();

    let mut options = StartThreadOptions::new(config);
    prepared.apply_to_start_thread_options(&mut options);

    let new_thread = manager.start_thread(options).await.unwrap();
    let (completed, error, _) = run_turn_to_completion(&new_thread.thread, "Say hello").await;

    // Assert that the turn did not succeed or errored out
    assert!(!completed || error.is_some());

    let requests = mock_server.received_requests().await.unwrap();
    assert!(!requests.is_empty());
    assert!(requests.iter().all(|r| r.url.path() == "/chat/completions"));

    // Critical assertion: /responses was NEVER called (no silent fallback)
    assert!(
        requests.iter().all(|r| r.url.path() != "/responses"),
        "Custom protocol failure must never fall back silently to /responses"
    );
}

#[tokio::test]
async fn test_thread_e2e_host_tools_preservation() {
    let mock_server = MockServer::start().await;

    let sse_body = "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\n\
                    data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Tools verified.\"},\"finish_reason\":null}]}\n\n\
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

    let temp_dir = tempfile::tempdir().unwrap();
    let (manager, config) = create_test_thread_manager(temp_dir.path()).await;

    let mut catalog = ProviderCatalog::new();
    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_KEY".to_string(),
    };
    let instance_id = register_provider_and_instance(
        &mut catalog,
        "openai-tools",
        "Mock Tools Instance",
        ProtocolFamily::OpenAiChatCompletions,
        &mock_server.uri(),
        AuthenticationScheme::BearerToken { secret: sec_ref },
    );

    let model_id = ModelId::new("gpt-4o-mini").unwrap();
    let desc = ModelDescriptor::new(
        instance_id,
        model_id.clone(),
        "GPT-4o Mini",
        ModelCapabilities::default(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(desc).unwrap();

    let catalog = Arc::new(catalog);
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_KEY", "sk-mock-key"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let factory =
        AgentStudiosRuntimeSessionFactory::new(catalog, secret_resolver, continuation_manager);

    let model_ref = ModelRef::new(instance_id, model_id);
    let prepared = factory.prepare_runtime_session(&model_ref).unwrap();

    let mut options = StartThreadOptions::new(config);
    prepared.apply_to_start_thread_options(&mut options);

    let new_thread = manager.start_thread(options).await.unwrap();
    let (completed, _, _) = run_turn_to_completion(&new_thread.thread, "Execute something").await;
    assert!(completed);

    let requests = mock_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();

    // Verify tools were passed to the wire protocol
    let tools = body.get("tools").and_then(|t| t.as_array());
    assert!(tools.is_some(), "Tools must be passed to the wire protocol");
    let tool_list = tools.unwrap();
    assert!(!tool_list.is_empty(), "Tool list should not be empty");

    let tool_names: Vec<&str> = tool_list
        .iter()
        .filter_map(|t| {
            t.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
        })
        .collect();

    // Verify natural Codex host tools (e.g. exec, apply_patch) exist
    assert!(
        tool_names.iter().any(|&n| n == "exec"
            || n == "exec_command"
            || n == "apply_patch"
            || n == "shell_command"),
        "Codex host tools must be preserved. Actual tools: {:?}",
        tool_names
    );
}

#[tokio::test]
async fn test_thread_e2e_child_and_fork_inheritance() {
    let mock_server = MockServer::start().await;

    let temp_dir = tempfile::tempdir().unwrap();
    let (manager, config) = create_test_thread_manager(temp_dir.path()).await;

    let mut catalog = ProviderCatalog::new();
    let sec_ref = SecretReference {
        backend: SecretBackend::EnvironmentVariable,
        locator: "TEST_INHERIT_KEY".to_string(),
    };
    let instance_id = register_provider_and_instance(
        &mut catalog,
        "openai-inherit",
        "Mock Inherit Instance",
        ProtocolFamily::OpenAiChatCompletions,
        &mock_server.uri(),
        AuthenticationScheme::BearerToken { secret: sec_ref },
    );

    let model_id = ModelId::new("gpt-4o-mini").unwrap();
    let desc = ModelDescriptor::new(
        instance_id,
        model_id.clone(),
        "GPT-4o Mini",
        ModelCapabilities::default(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(desc).unwrap();

    let catalog = Arc::new(catalog);
    let secret_resolver =
        Arc::new(InMemorySecretResolver::new().with_env_secret("TEST_INHERIT_KEY", "sk-mock-key"));
    let continuation_manager = Arc::new(ContinuationManager::new());
    let factory =
        AgentStudiosRuntimeSessionFactory::new(catalog, secret_resolver, continuation_manager);

    let model_ref = ModelRef::new(instance_id, model_id);
    let prepared = factory.prepare_runtime_session(&model_ref).unwrap();
    let parent_provider_id = prepared.model_provider_id().to_string();

    // 1. Start root parent thread
    let mut options = StartThreadOptions::new(config.clone());
    prepared.apply_to_start_thread_options(&mut options);
    let parent = manager.start_thread(options).await.unwrap();
    assert!(parent.thread.model_runtime_override().is_some());
    let parent_id = parent.session_configured.thread_id;

    // 2. Child with matching provider id -> inherits parent override
    let mut child_matching_options = StartThreadOptions::new(config.clone());
    child_matching_options.session_source =
        Some(SessionSource::Internal(InternalSessionSource::Guardian));
    child_matching_options.config.model_provider_id = parent_provider_id.clone();
    let child_matching = manager
        .spawn_internal_session(parent_id, child_matching_options)
        .await
        .unwrap();
    assert!(
        child_matching.thread.model_runtime_override().is_some(),
        "Child with matching model_provider_id must inherit ModelRuntimeOverride"
    );

    // 3. Child with different provider id -> does NOT inherit parent override
    let mut child_different_options = StartThreadOptions::new(config.clone());
    child_different_options.session_source =
        Some(SessionSource::Internal(InternalSessionSource::Guardian));
    child_different_options.config.model_provider_id = "different-provider-id".to_string();
    let child_different = manager
        .spawn_internal_session(parent_id, child_different_options)
        .await
        .unwrap();
    assert!(
        child_different.thread.model_runtime_override().is_none(),
        "Child with different model_provider_id must not inherit ModelRuntimeOverride"
    );

    // 4. Fork with matching provider id -> inherits parent override
    let mut fork_matching_options = StartThreadOptions::new(config.clone());
    fork_matching_options.session_source =
        Some(SessionSource::Internal(InternalSessionSource::Guardian));
    fork_matching_options.config.model_provider_id = parent_provider_id.clone();
    let fork_matching = manager
        .fork_internal_session(parent_id, fork_matching_options, Vec::new())
        .await
        .unwrap();
    assert!(
        fork_matching.thread.model_runtime_override().is_some(),
        "Fork with matching model_provider_id must inherit ModelRuntimeOverride"
    );

    // 5. Fork with different provider id -> does NOT inherit parent override
    let mut fork_different_options = StartThreadOptions::new(config.clone());
    fork_different_options.session_source =
        Some(SessionSource::Internal(InternalSessionSource::Guardian));
    fork_different_options.config.model_provider_id = "different-provider-id".to_string();
    let fork_different = manager
        .fork_internal_session(parent_id, fork_different_options, Vec::new())
        .await
        .unwrap();
    assert!(
        fork_different.thread.model_runtime_override().is_none(),
        "Fork with different model_provider_id must not inherit ModelRuntimeOverride"
    );

    // Cleanup
    let _ = parent.thread.shutdown_and_wait().await;
    let _ = child_matching.thread.shutdown_and_wait().await;
    let _ = child_different.thread.shutdown_and_wait().await;
    let _ = fork_matching.thread.shutdown_and_wait().await;
    let _ = fork_different.thread.shutdown_and_wait().await;
}
