use std::sync::Arc;
use std::time::Duration;

use agent_studios_control_plane::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::store::InMemoryStore;
use agent_studios_internal_agent::{
    AgentExecutionBudget, AgentExecutionContext, AgentExecutor, AgentReasoningEffort,
    AgentReasoningSelection, CodexAgentExecutor, ControlPlaneActor, ControlPlaneHandle,
    InternalAgentSpec, InternalTeamSpec, WorkspaceAccessMode,
};
use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId};
use agent_studios_provider::ProviderCatalog;
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
use codex_protocol::protocol::{EventMsg, SessionSource};
use codex_protocol::user_input::UserInput;
use codex_utils_absolute_path::AbsolutePathBuf;
use wiremock::matchers::{header, method, path, path_regex};
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

use agent_studios_provider::ProviderDefinition;

fn create_test_actor() -> (ControlPlaneHandle, tokio::task::JoinHandle<()>) {
    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    ControlPlaneActor::spawn(cp)
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

fn run_with_large_stack<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(fut)
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn test_cross_provider_team_execution_and_isolation() {
    run_with_large_stack(async {
        // 1. Setup 3 independent mock servers for 3 distinct providers
        let gemini_server = MockServer::start().await;
        let anthropic_server = MockServer::start().await;
        let openai_server = MockServer::start().await;

        // Gemini mock: Coordinator
        let gemini_sse = "data: {\"responseId\":\"resp_gem_coord\",\"candidates\":[{\"index\":0,\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"Coordinator: plan completed\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":8,\"totalTokenCount\":20}}\n\n";
        Mock::given(method("POST"))
            .and(path_regex(r"^/models/.*:streamGenerateContent$"))
            .and(header("x-goog-api-key", "gemini-secret-token-111"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(gemini_sse),
            )
            .mount(&gemini_server)
            .await;

        // Anthropic mock: Coder
        let anthropic_sse = "event: message_start\n\
                         data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_coder\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-3-7-sonnet-20250219\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":25,\"output_tokens\":0}}}\n\n\
                         event: content_block_start\n\
                         data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
                         event: content_block_delta\n\
                         data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Coder: implemented module\"}}\n\n\
                         event: content_block_stop\n\
                         data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
                         event: message_delta\n\
                         data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":18}}\n\n\
                         event: message_stop\n\
                         data: {\"type\":\"message_stop\"}\n\n";
        Mock::given(method("POST"))
            .and(path("/messages"))
            .and(header("x-api-key", "anthropic-secret-token-222"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(anthropic_sse),
            )
            .mount(&anthropic_server)
            .await;

        // OpenAI mock: Reviewer
        let openai_sse = "data: {\"id\":\"chatcmpl-reviewer\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\n\
                      data: {\"id\":\"chatcmpl-reviewer\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Reviewer: code looks great\"},\"finish_reason\":null}]}\n\n\
                      data: {\"id\":\"chatcmpl-reviewer\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":30,\"completion_tokens\":10,\"total_tokens\":40}}\n\n\
                      data: [DONE]\n\n";
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("Authorization", "Bearer openai-secret-token-333"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(openai_sse),
            )
            .mount(&openai_server)
            .await;

        // 2. Setup ProviderCatalog and InMemorySecretResolver
        let mut catalog = ProviderCatalog::new();
        let secret_resolver = Arc::new(
            InMemorySecretResolver::new()
                .with_env_secret("GEMINI_KEY", "gemini-secret-token-111")
                .with_env_secret("ANTHROPIC_KEY", "anthropic-secret-token-222")
                .with_env_secret("OPENAI_KEY", "openai-secret-token-333"),
        );

        // Register Gemini instance
        let gemini_instance_id = register_provider_and_instance(
            &mut catalog,
            "google-gemini",
            "Google Gemini Instance",
            ProtocolFamily::GeminiGenerateContent,
            &gemini_server.uri(),
            AuthenticationScheme::ApiKeyHeader {
                header_name: "x-goog-api-key".to_string(),
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "GEMINI_KEY".to_string(),
                },
            },
        );
        let gemini_model_id = ModelId::new("gemini-2.5-pro").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    gemini_instance_id,
                    gemini_model_id.clone(),
                    "Gemini 2.5 Pro",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        // Register Anthropic instance
        let anthropic_instance_id = register_provider_and_instance(
            &mut catalog,
            "anthropic-messages",
            "Anthropic Messages Instance",
            ProtocolFamily::AnthropicMessages,
            &anthropic_server.uri(),
            AuthenticationScheme::ApiKeyHeader {
                header_name: "x-api-key".to_string(),
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "ANTHROPIC_KEY".to_string(),
                },
            },
        );
        let anthropic_model_id = ModelId::new("claude-3-7-sonnet-20250219").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    anthropic_instance_id,
                    anthropic_model_id.clone(),
                    "Claude 3.7 Sonnet",
                    ModelCapabilities::default(),
                    ModelLimits {
                        context_window_tokens: Some(200_000),
                        max_output_tokens: Some(8192),
                    },
                )
                .unwrap(),
            )
            .unwrap();

        // Register OpenAI instance
        let openai_instance_id = register_provider_and_instance(
            &mut catalog,
            "openai-chat",
            "OpenAI Chat Instance",
            ProtocolFamily::OpenAiChatCompletions,
            &openai_server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "OPENAI_KEY".to_string(),
                },
            },
        );
        let openai_model_id = ModelId::new("gpt-4o").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    openai_instance_id,
                    openai_model_id.clone(),
                    "GPT-4o",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let catalog = Arc::new(catalog);
        let continuation_manager = Arc::new(ContinuationManager::new());
        let factory = AgentStudiosRuntimeSessionFactory::new(
            catalog.clone(),
            secret_resolver.clone(),
            continuation_manager,
        )
        .with_transport_options(RuntimeTransportOptions::default());

        // 3. Define the multi-agent team with distinct roles, providers, models, reasoning, and budgets
        let coord_model_ref = ModelRef::new(gemini_instance_id, gemini_model_id);
        let coord_spec = InternalAgentSpec::new(
            AgentId::new(),
            "Team Coordinator",
            "Architecture & Planning",
            coord_model_ref.clone(),
        )
        .with_reasoning(AgentReasoningSelection {
            effort: Some(AgentReasoningEffort::High),
            summary: Some("Plan validation".to_string()),
        })
        .with_budget(
            AgentExecutionBudget::default()
                .with_turns(5)
                .with_tool_calls(20),
        );

        let coder_model_ref = ModelRef::new(anthropic_instance_id, anthropic_model_id);
        let coder_spec = InternalAgentSpec::new(
            AgentId::new(),
            "Backend Developer",
            "Implementation",
            coder_model_ref.clone(),
        )
        .with_workspace_access(WorkspaceAccessMode::Mutating)
        .with_reasoning(AgentReasoningSelection {
            effort: Some(AgentReasoningEffort::Medium),
            summary: None,
        })
        .with_budget(
            AgentExecutionBudget::default()
                .with_turns(10)
                .with_tool_calls(50),
        );

        let reviewer_model_ref = ModelRef::new(openai_instance_id, openai_model_id);
        let reviewer_spec = InternalAgentSpec::new(
            AgentId::new(),
            "Security Reviewer",
            "Code Review",
            reviewer_model_ref.clone(),
        )
        .with_workspace_access(WorkspaceAccessMode::ReadOnly)
        .with_reasoning(AgentReasoningSelection {
            effort: Some(AgentReasoningEffort::Low),
            summary: None,
        })
        .with_budget(
            AgentExecutionBudget::default()
                .with_turns(3)
                .with_tool_calls(10),
        );

        let studio_id = StudioId::new();
        let team = InternalTeamSpec::new(studio_id, "production-team", coord_spec.clone())
            .add_agent("coder", coder_spec.clone())
            .unwrap()
            .add_agent("reviewer", reviewer_spec.clone())
            .unwrap();

        assert_eq!(team.len(), 3);

        // 4. Verify prepared runtime session overrides for all 3 agents
        let temp_dir = tempfile::tempdir().unwrap();
        let (manager, base_config) = create_test_thread_manager(temp_dir.path()).await;

        // A. Coordinator execution
        let coord_prepared = factory
            .prepare_runtime_session(&coord_spec.model_ref)
            .unwrap();
        let mut coord_options = StartThreadOptions::new(base_config.clone());
        coord_prepared.apply_to_start_thread_options(&mut coord_options);

        let coord_thread = manager.start_thread(coord_options).await.unwrap();
        let (coord_done, coord_err, coord_msgs) =
            run_turn_to_completion(&coord_thread.thread, "Generate architecture plan").await;

        assert!(coord_done, "Coordinator turn failed: {:?}", coord_err);
        assert!(
            coord_msgs
                .iter()
                .any(|m| m.contains("Coordinator: plan completed")),
            "Coordinator message missing: {:?}",
            coord_msgs
        );

        // B. Coder execution
        let coder_prepared = factory
            .prepare_runtime_session(&coder_spec.model_ref)
            .unwrap();
        let mut coder_options = StartThreadOptions::new(base_config.clone());
        coder_prepared.apply_to_start_thread_options(&mut coder_options);

        let coder_thread = manager.start_thread(coder_options).await.unwrap();
        let (coder_done, coder_err, coder_msgs) =
            run_turn_to_completion(&coder_thread.thread, "Write implementation").await;

        assert!(coder_done, "Coder turn failed: {:?}", coder_err);
        assert!(
            coder_msgs
                .iter()
                .any(|m| m.contains("Coder: implemented module")),
            "Coder message missing: {:?}",
            coder_msgs
        );

        // C. Reviewer execution
        let reviewer_prepared = factory
            .prepare_runtime_session(&reviewer_spec.model_ref)
            .unwrap();
        let mut reviewer_options = StartThreadOptions::new(base_config.clone());
        reviewer_prepared.apply_to_start_thread_options(&mut reviewer_options);

        let reviewer_thread = manager.start_thread(reviewer_options).await.unwrap();
        let (reviewer_done, reviewer_err, reviewer_msgs) =
            run_turn_to_completion(&reviewer_thread.thread, "Audit code diff").await;

        assert!(reviewer_done, "Reviewer turn failed: {:?}", reviewer_err);
        assert!(
            reviewer_msgs
                .iter()
                .any(|m| m.contains("Reviewer: code looks great")),
            "Reviewer message missing: {:?}",
            reviewer_msgs
        );

        // 5. Verify independent request routing across the 3 mock servers
        let gemini_requests = gemini_server.received_requests().await.unwrap();
        assert_eq!(gemini_requests.len(), 1);
        assert!(gemini_requests[0].headers.contains_key("x-goog-api-key"));

        let anthropic_requests = anthropic_server.received_requests().await.unwrap();
        assert_eq!(anthropic_requests.len(), 1);
        assert_eq!(
            anthropic_requests[0].headers.get("x-api-key").unwrap(),
            "anthropic-secret-token-222"
        );

        let openai_requests = openai_server.received_requests().await.unwrap();
        assert_eq!(openai_requests.len(), 1);
        assert_eq!(
            openai_requests[0].headers.get("authorization").unwrap(),
            "Bearer openai-secret-token-333"
        );

        // 6. Verify CodexAgentExecutor integrates with the factory cleanly
        let temp_dir = tempfile::tempdir().unwrap();
        let (tm, config) = create_test_thread_manager(temp_dir.path()).await;
        let (cp_handle, _actor_task) = create_test_actor();
        let executor = CodexAgentExecutor::try_new(
            Arc::new(factory),
            Arc::new(tm),
            Arc::new(config),
            cp_handle,
        )
        .unwrap();
        assert_eq!(
            executor.factory().transport_options().stream_idle_timeout,
            Duration::from_secs(300)
        );
    });
}

#[test]
fn test_same_model_slug_cross_instance_isolation() {
    run_with_large_stack(async {
        let server_a = MockServer::start().await;
        let server_b = MockServer::start().await;

        let sse_a = "data: {\"id\":\"chat-a\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Response from Instance A\"},\"finish_reason\":null}]}\n\n\
                 data: {\"id\":\"chat-a\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":5,\"total_tokens\":10}}\n\n\
                 data: [DONE]\n\n";

        let sse_b = "data: {\"id\":\"chat-b\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Response from Instance B\"},\"finish_reason\":null}]}\n\n\
                 data: {\"id\":\"chat-b\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":5,\"total_tokens\":10}}\n\n\
                 data: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("Authorization", "Bearer token-tenant-A"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_a),
            )
            .mount(&server_a)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("Authorization", "Bearer token-tenant-B"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_b),
            )
            .mount(&server_b)
            .await;

        let mut catalog = ProviderCatalog::new();
        let secret_resolver = Arc::new(
            InMemorySecretResolver::new()
                .with_env_secret("KEY_A", "token-tenant-A")
                .with_env_secret("KEY_B", "token-tenant-B"),
        );

        // Register Instance A with model slug "gpt-4o"
        let instance_a = register_provider_and_instance(
            &mut catalog,
            "openai-tenant-a",
            "Tenant A OpenAI",
            ProtocolFamily::OpenAiChatCompletions,
            &server_a.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "KEY_A".to_string(),
                },
            },
        );
        let shared_model_id = ModelId::new("gpt-4o").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    instance_a,
                    shared_model_id.clone(),
                    "Tenant A GPT-4o",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        // Register Instance B with SAME model slug "gpt-4o"
        let instance_b = register_provider_and_instance(
            &mut catalog,
            "openai-tenant-b",
            "Tenant B OpenAI",
            ProtocolFamily::OpenAiChatCompletions,
            &server_b.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "KEY_B".to_string(),
                },
            },
        );
        catalog
            .register_model(
                ModelDescriptor::new(
                    instance_b,
                    shared_model_id.clone(),
                    "Tenant B GPT-4o",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let catalog = Arc::new(catalog);
        let continuation_manager = Arc::new(ContinuationManager::new());
        let factory =
            AgentStudiosRuntimeSessionFactory::new(catalog, secret_resolver, continuation_manager);

        let temp_dir = tempfile::tempdir().unwrap();
        let (manager, base_config) = create_test_thread_manager(temp_dir.path()).await;

        // Target Instance A
        let ref_a = ModelRef::new(instance_a, shared_model_id.clone());
        let prep_a = factory.prepare_runtime_session(&ref_a).unwrap();
        let mut opt_a = StartThreadOptions::new(base_config.clone());
        prep_a.apply_to_start_thread_options(&mut opt_a);

        let thread_a = manager.start_thread(opt_a).await.unwrap();
        let (done_a, _, msgs_a) = run_turn_to_completion(&thread_a.thread, "Call instance A").await;
        assert!(done_a);
        assert!(
            msgs_a
                .iter()
                .any(|m| m.contains("Response from Instance A"))
        );

        // Target Instance B
        let ref_b = ModelRef::new(instance_b, shared_model_id);
        let prep_b = factory.prepare_runtime_session(&ref_b).unwrap();
        let mut opt_b = StartThreadOptions::new(base_config);
        prep_b.apply_to_start_thread_options(&mut opt_b);

        let thread_b = manager.start_thread(opt_b).await.unwrap();
        let (done_b, _, msgs_b) = run_turn_to_completion(&thread_b.thread, "Call instance B").await;
        assert!(done_b);
        assert!(
            msgs_b
                .iter()
                .any(|m| m.contains("Response from Instance B"))
        );

        // Server A received only 1 request, Server B received only 1 request
        let reqs_a = server_a.received_requests().await.unwrap();
        let reqs_b = server_b.received_requests().await.unwrap();
        assert_eq!(reqs_a.len(), 1);
        assert_eq!(reqs_b.len(), 1);
        assert_eq!(
            reqs_a[0].headers.get("authorization").unwrap(),
            "Bearer token-tenant-A"
        );
        assert_eq!(
            reqs_b[0].headers.get("authorization").unwrap(),
            "Bearer token-tenant-B"
        );
    });
}

#[test]
fn test_hierarchical_codex_agent_tree_and_worker_reuse() {
    run_with_large_stack(async {
        let coord_server = MockServer::start().await;
        let worker_server = MockServer::start().await;

        let coord_sse = "data: {\"id\":\"chat-coord\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Coordinator initialized plan\"},\"finish_reason\":null}]}\n\n\
                         data: {\"id\":\"chat-coord\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n\
                         data: [DONE]\n\n";

        let worker_turn1_sse = "data: {\"id\":\"chat-worker-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Worker turn 1 output\"},\"finish_reason\":null}]}\n\n\
                                data: {\"id\":\"chat-worker-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n\
                                data: [DONE]\n\n";

        let worker_turn2_sse = "data: {\"id\":\"chat-worker-2\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Worker turn 2 reused output\"},\"finish_reason\":null}]}\n\n\
                                data: {\"id\":\"chat-worker-2\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n\
                                data: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("Authorization", "Bearer coord-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(coord_sse),
            )
            .mount(&coord_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("Authorization", "Bearer worker-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(worker_turn1_sse),
            )
            .up_to_n_times(1)
            .mount(&worker_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("Authorization", "Bearer worker-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(worker_turn2_sse),
            )
            .mount(&worker_server)
            .await;

        let mut catalog = ProviderCatalog::new();
        let secret_resolver = Arc::new(
            InMemorySecretResolver::new()
                .with_env_secret("COORD_KEY", "coord-token")
                .with_env_secret("WORKER_KEY", "worker-token"),
        );

        let coord_instance = register_provider_and_instance(
            &mut catalog,
            "coord-provider",
            "Coord Provider",
            ProtocolFamily::OpenAiChatCompletions,
            &coord_server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "COORD_KEY".to_string(),
                },
            },
        );
        let coord_model_id = ModelId::new("gpt-4o-coord").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    coord_instance,
                    coord_model_id.clone(),
                    "Coord Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let worker_instance = register_provider_and_instance(
            &mut catalog,
            "worker-provider",
            "Worker Provider",
            ProtocolFamily::OpenAiChatCompletions,
            &worker_server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "WORKER_KEY".to_string(),
                },
            },
        );
        let worker_model_id = ModelId::new("gpt-4o-worker").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    worker_instance,
                    worker_model_id.clone(),
                    "Worker Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let catalog = Arc::new(catalog);
        let continuation_manager = Arc::new(ContinuationManager::new());
        let factory =
            AgentStudiosRuntimeSessionFactory::new(catalog, secret_resolver, continuation_manager);

        let temp_dir = tempfile::tempdir().unwrap();
        let (manager, base_config) = create_test_thread_manager(temp_dir.path()).await;
        let (cp_handle, _actor_task) = create_test_actor();

        let executor = CodexAgentExecutor::try_new(
            Arc::new(factory),
            Arc::new(manager),
            Arc::new(base_config),
            cp_handle,
        )
        .unwrap();

        let studio_id = StudioId::new();
        let run_id = RunId::new();

        let coord_id = AgentId::new();
        let coord_spec = InternalAgentSpec::new(
            coord_id,
            "Coordinator",
            "Coordinator",
            ModelRef::new(coord_instance, coord_model_id),
        );

        let worker_id = AgentId::new();
        let worker_spec = InternalAgentSpec::new(
            worker_id,
            "Worker 1",
            "Developer",
            ModelRef::new(worker_instance, worker_model_id),
        )
        .with_workspace_access(WorkspaceAccessMode::ReadOnly);

        // 1. Root coordinator start
        let coord_ctx = AgentExecutionContext {
            studio_id,
            task_id: Some(TaskId::new()),
            run_id: Some(run_id),
            agent_spec: coord_spec,
            prompt: "Plan the project".to_string(),
            output_schema: None,
            budget: AgentExecutionBudget::default(),
            parent_agent_id: None,
        };

        let coord_res = executor.execute_agent(coord_ctx).await.unwrap();
        assert!(coord_res.success);
        assert!(coord_res.output.contains("Coordinator initialized plan"));

        // 2. Child worker spawn (turn 1)
        let worker_turn1_ctx = AgentExecutionContext {
            studio_id,
            task_id: Some(TaskId::new()),
            run_id: Some(run_id),
            agent_spec: worker_spec.clone(),
            prompt: "Do task turn 1".to_string(),
            output_schema: None,
            budget: AgentExecutionBudget::default(),
            parent_agent_id: Some(coord_id),
        };

        let worker1_res = executor.execute_agent(worker_turn1_ctx).await.unwrap();
        assert!(worker1_res.success);
        assert!(worker1_res.output.contains("Worker turn 1 output"));

        // 3. Child worker reuse (turn 2)
        let worker_turn2_ctx = AgentExecutionContext {
            studio_id,
            task_id: Some(TaskId::new()),
            run_id: Some(run_id),
            agent_spec: worker_spec,
            prompt: "Do follow-up task turn 2".to_string(),
            output_schema: None,
            budget: AgentExecutionBudget::default(),
            parent_agent_id: Some(coord_id),
        };

        let worker2_res = executor.execute_agent(worker_turn2_ctx).await.unwrap();
        assert!(worker2_res.success);
        assert!(worker2_res.output.contains("Worker turn 2 reused output"));

        // Verify running agent tree contains both agents
        let running_map = executor.running_agents();
        let running = running_map.read().await;
        assert_eq!(running.len(), 2);
        assert!(running.contains_key(&coord_id));
        assert!(running.contains_key(&worker_id));
        assert_eq!(
            running.get(&worker_id).unwrap().parent_agent_id,
            Some(coord_id)
        );
    });
}
