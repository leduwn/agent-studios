use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_studios_control_plane::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::error::ControlPlaneError;
use agent_studios_control_plane::store::InMemoryStore;
use agent_studios_internal_agent::{
    AgentBudgetTracker, AgentExecutionBudget, AgentExecutionContext, AgentExecutor,
    AgentRuntimeExtensionContext, AgentStudiosCodexRuntimeFactory, AgentStudiosSupervisor,
    BudgetScopeId, CodexAgentExecutor, ControlPlaneActor, ControlPlaneHandle, FailurePolicy,
    InternalAgentError, InternalAgentSpec, InternalTeamSpec, WorkspaceAccessMode,
    WorkspacePolicyArbitrator, build_agent_studios_extension_builder,
};
use agent_studios_orchestration::projection::project_agent_summary;
use agent_studios_protocol::agent::{AgentKind, AgentState};
use agent_studios_protocol::cancellation::CancellationScope;
use agent_studios_protocol::error::TransitionError;
use agent_studios_protocol::event::ControlPlaneEvent;
use agent_studios_protocol::id::AgentId;
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::TaskState;
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
use agent_studios_runtime_transport::{ContinuationManager, InMemorySecretResolver};
use codex_core::config::Config;
use codex_core::{
    AgentInput, SpawnAgentOptions, SpawnRequest, StartThreadOptions, ThreadManager,
    TurnInputRequest,
};
use codex_extension_api::{
    ExtensionData, ToolCall, ToolCallOutcome, ToolContributor, ToolFinishInput,
    ToolLifecycleContributor, ToolLifecycleFuture, ToolName,
};
use codex_login::{AuthManager, CodexAuth};
use codex_protocol::protocol::{AgentStatus, EventMsg, SessionSource, SubAgentSource};
use codex_protocol::user_input::UserInput;
use codex_tools::{
    JsonToolOutput, ResponsesApiTool, ToolExecutor, ToolExecutorFuture, ToolExposure, ToolOutput,
    ToolSpec, parse_tool_input_schema,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Notify;
use wiremock::matchers::{header, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Clone)]
struct TestCounterTool {
    calls: Arc<AtomicUsize>,
}

impl ToolContributor for TestCounterTool {
    fn tools(
        &self,
        _session_store: &ExtensionData,
        _thread_store: &ExtensionData,
    ) -> Vec<Arc<dyn for<'call> ToolExecutor<ToolCall<'call>>>> {
        vec![Arc::new(self.clone())]
    }
}

impl<'call> ToolExecutor<ToolCall<'call>> for TestCounterTool {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("test_tool")
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::Function(ResponsesApiTool {
            name: "test_tool".to_string(),
            description: "Test tool for N+1 budget verification".to_string(),
            strict: false,
            defer_loading: None,
            parameters: parse_tool_input_schema(&serde_json::json!({
                "type": "object",
                "properties": {
                    "step": { "type": "integer" }
                }
            }))
            .unwrap(),
            output_schema: None,
        })
    }

    fn exposure(&self) -> ToolExposure {
        ToolExposure::Direct
    }

    fn handle<'a>(&'a self, _call: ToolCall<'call>) -> ToolExecutorFuture<'a>
    where
        'call: 'a,
    {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(15)).await;
            Ok(Box::new(JsonToolOutput::new(serde_json::json!({
                "result": "counter executed"
            }))) as Box<dyn ToolOutput>)
        })
    }
}

#[derive(Default, Clone)]
struct ToolOutcomeRecorder {
    outcomes: Arc<Mutex<Vec<(String, ToolCallOutcome)>>>,
}

impl ToolLifecycleContributor for ToolOutcomeRecorder {
    fn on_tool_finish<'a>(&'a self, input: ToolFinishInput<'a>) -> ToolLifecycleFuture<'a> {
        self.outcomes
            .lock()
            .unwrap()
            .push((input.call_id.to_string(), input.outcome));
        Box::pin(std::future::ready(()))
    }
}

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

fn create_test_actor() -> (ControlPlaneHandle, tokio::task::JoinHandle<()>) {
    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    ControlPlaneActor::spawn(cp)
}

async fn create_test_thread_manager(codex_home: &std::path::Path) -> (ThreadManager, Config) {
    let config = AgentStudiosCodexRuntimeFactory::create_test_config(codex_home)
        .await
        .expect("load default test config");
    let auth_manager = AuthManager::from_auth_for_testing_with_home(
        CodexAuth::from_api_key("dummy"),
        config.codex_home.to_path_buf(),
    );
    let manager = AgentStudiosCodexRuntimeFactory::build_thread_manager(
        &config,
        auth_manager,
        Some(SessionSource::Exec),
    )
    .await
    .expect("build agent studios thread manager");
    (manager, config)
}

async fn wait_for_turn(thread: &codex_core::CodexThread) -> (bool, Option<String>, Vec<String>) {
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
                EventMsg::TurnAborted(_) => {
                    got_error = Some("TurnAborted".to_string());
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

    (got_turn_complete, got_error, messages)
}

async fn run_turn(
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
    wait_for_turn(thread).await
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
fn real_tool_budget_n_plus_one_is_blocked_before_handler() {
    run_with_large_stack(async {
        let (cp_handle, _actor_task) = create_test_actor();
        let temp_dir = tempfile::tempdir().unwrap();
        let config = AgentStudiosCodexRuntimeFactory::create_test_config(temp_dir.path())
            .await
            .expect("test config");
        let auth_manager = AuthManager::from_auth_for_testing_with_home(
            CodexAuth::from_api_key("dummy"),
            config.codex_home.to_path_buf(),
        );

        let tool_calls_counter = Arc::new(AtomicUsize::new(0));
        let counter_tool = Arc::new(TestCounterTool {
            calls: Arc::clone(&tool_calls_counter),
        });
        let outcome_recorder = Arc::new(ToolOutcomeRecorder::default());

        let mut builder = build_agent_studios_extension_builder::<Config>();
        builder.tool_contributor(counter_tool);
        builder.tool_lifecycle_contributor(outcome_recorder.clone());
        let extensions = Arc::new(builder.build());

        let manager = AgentStudiosCodexRuntimeFactory::build_thread_manager_with_extensions(
            &config,
            auth_manager,
            Some(SessionSource::Exec),
            extensions,
        )
        .await
        .expect("build thread manager");

        let server = MockServer::start().await;

        let sse_tool1 = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-t1",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "tool_calls": [{
                            "index": 0,
                            "id": "call_tool_1",
                            "type": "function",
                            "function": {
                                "name": "test_tool",
                                "arguments": "{\"step\":1}"
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 10,
                    "total_tokens": 20
                }
            })
        );

        let sse_tool2 = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-t2",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "tool_calls": [{
                            "index": 0,
                            "id": "call_tool_2",
                            "type": "function",
                            "function": {
                                "name": "test_tool",
                                "arguments": "{\"step\":2}"
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 10,
                    "total_tokens": 20
                }
            })
        );

        let sse_done = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chatcmpl-t3",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "content": "Turn concluded after tool budget block"
                    },
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 10,
                    "total_tokens": 20
                }
            })
        );

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_tool1),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_tool2),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_done),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let studio = cp_handle.create_studio("tool-budget-studio").await.unwrap();
        let studio_id = studio.id;
        let agent_id = AgentId::new();
        cp_handle
            .register_agent_with_id(
                studio_id,
                agent_id,
                "Budget Agent",
                AgentKind::Internal,
                Some("developer".to_string()),
            )
            .await
            .unwrap();
        let task = cp_handle
            .create_task(studio_id, "Budget task", "desc", None, None, vec![])
            .await
            .unwrap();
        let task_id = task.id;
        let run = cp_handle.create_run(task_id, agent_id).await.unwrap();
        let run_id = run.id;

        let budget = AgentExecutionBudget::default().with_tool_calls(1);
        let tracker = Arc::new(AgentBudgetTracker::new_with_scope(
            BudgetScopeId::Run(run_id),
            agent_id,
            budget,
        ));

        let mut catalog = ProviderCatalog::new();
        let secret_resolver = Arc::new(
            InMemorySecretResolver::new().with_env_secret("TOOL_OPENAI_KEY", "tool-openai-token"),
        );
        let instance_id = register_provider_and_instance(
            &mut catalog,
            "openai-chat",
            "OpenAI Tool Instance",
            ProtocolFamily::OpenAiChatCompletions,
            &server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "TOOL_OPENAI_KEY".to_string(),
                },
            },
        );
        let model_id = ModelId::new("gpt-4o").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    instance_id,
                    model_id.clone(),
                    "GPT-4o",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();
        let factory = AgentStudiosRuntimeSessionFactory::new(
            Arc::new(catalog),
            secret_resolver,
            Arc::new(ContinuationManager::new()),
        );
        let prepared = factory
            .prepare_runtime_session(&ModelRef::new(instance_id, model_id))
            .unwrap();

        let mut thread_options = StartThreadOptions::new(config.clone());
        prepared.apply_to_start_thread_options(&mut thread_options);

        let mut init = codex_extension_api::ExtensionDataInit::new();
        let ext_ctx = Arc::new(AgentRuntimeExtensionContext::new(
            studio_id,
            agent_id,
            Arc::clone(&tracker),
            cp_handle.clone(),
        ));
        ext_ctx.set_task_and_run(Some(task_id), Some(run_id), Arc::clone(&tracker));
        init.insert((*ext_ctx).clone());
        thread_options.thread_extension_init = init;

        let thread = manager.start_thread(thread_options).await.unwrap();
        let (done, err, _msgs) = run_turn(&thread.thread, "Execute tools").await;
        assert!(done, "turn should complete: {:?}", err);

        // Tool 1 handler ran, Tool 2 handler NEVER ran
        assert_eq!(tool_calls_counter.load(Ordering::SeqCst), 1);
        assert_eq!(tracker.tool_calls_used(), 1);

        let call2_outcome = {
            let outcomes = outcome_recorder.outcomes.lock().unwrap();
            outcomes.iter().find(|(id, _)| id == "call_tool_2").cloned()
        };
        assert!(
            matches!(call2_outcome, Some((_, ToolCallOutcome::Blocked))),
            "Expected call_tool_2 outcome to be Blocked, got: {:?}",
            call2_outcome
        );

        tokio::time::sleep(Duration::from_millis(100)).await;
        let (durable_envelopes, _) = cp_handle.subscribe_events(studio_id, 0).await.unwrap();

        let budget_exceeded_ev = durable_envelopes.iter().find_map(|env| {
            if let ControlPlaneEvent::BudgetExceeded {
                dimension,
                limit,
                actual,
                ..
            } = &env.event
            {
                Some((dimension.clone(), *limit, *actual))
            } else {
                None
            }
        });
        assert_eq!(
            budget_exceeded_ev,
            Some(("tool_calls".to_string(), 1, 2)),
            "Expected BudgetExceeded for tool_calls"
        );

        let tool_started_ev = durable_envelopes.iter().find_map(|env| {
            if let ControlPlaneEvent::ToolStarted {
                agent_id: a,
                task_id: t,
                run_id: r,
                tool_name,
                call_id,
                timestamp,
            } = &env.event
            {
                if call_id == "call_tool_1" {
                    Some((a, t, r, tool_name.clone(), *timestamp))
                } else {
                    None
                }
            } else {
                None
            }
        });
        assert!(
            tool_started_ev.is_some(),
            "ToolStarted for call_tool_1 must be recorded"
        );
        let (a_id, t_id, r_id, t_name, _) = tool_started_ev.unwrap();
        assert_eq!(*a_id, agent_id);
        assert_eq!(*t_id, Some(task_id));
        assert_eq!(*r_id, Some(run_id));
        assert_eq!(t_name, "test_tool");

        let tool_completed_ev = durable_envelopes.iter().find_map(|env| {
            if let ControlPlaneEvent::ToolCompleted {
                agent_id: a,
                task_id: t,
                run_id: r,
                tool_name,
                call_id,
                duration_ms,
                ..
            } = &env.event
            {
                if call_id == "call_tool_1" {
                    Some((a, t, r, tool_name.clone(), *duration_ms))
                } else {
                    None
                }
            } else {
                None
            }
        });
        assert!(
            tool_completed_ev.is_some(),
            "ToolCompleted for call_tool_1 must be recorded"
        );
        let (ca_id, ct_id, cr_id, ct_name, duration_ms) = tool_completed_ev.unwrap();
        assert_eq!(*ca_id, agent_id);
        assert_eq!(*ct_id, Some(task_id));
        assert_eq!(*cr_id, Some(run_id));
        assert_eq!(ct_name, "test_tool");
        assert!(duration_ms > 0, "Tool duration must be > 0ms");

        for env in &durable_envelopes {
            let serialized = serde_json::to_string(&env.event).unwrap();
            assert!(
                !serialized.contains("\"step\""),
                "Event leaked tool arguments/payload: {serialized}"
            );
        }

        for env in &durable_envelopes {
            if let ControlPlaneEvent::ToolFailed { error, .. } = &env.event {
                assert!(
                    error.chars().count() <= 512,
                    "Failure output exceeds 512 UTF-8 characters"
                );
            }
        }
    });
}

#[test]
fn real_cross_provider_hierarchy_gemini_anthropic_chat() {
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
            .expect(1)
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
            .expect(1)
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
            .expect(1)
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

        let anthropic_instance_id = register_provider_and_instance(
            &mut catalog,
            "anthropic",
            "Anthropic Claude Instance",
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
        let anthropic_model_id = ModelId::new("claude-3-7-sonnet").unwrap();
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

        let openai_instance_id = register_provider_and_instance(
            &mut catalog,
            "openai",
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
        let factory =
            AgentStudiosRuntimeSessionFactory::new(catalog, secret_resolver, continuation_manager);

        let temp_dir = tempfile::tempdir().unwrap();
        let (manager, base_config) = create_test_thread_manager(temp_dir.path()).await;
        let (cp_handle, _actor_task) = create_test_actor();

        let studio = cp_handle
            .create_studio("cross-provider-studio")
            .await
            .unwrap();
        let studio_id = studio.id;
        let coord_agent_id = AgentId::new();
        cp_handle
            .register_agent_with_id(
                studio_id,
                coord_agent_id,
                "Coordinator",
                AgentKind::Internal,
                Some("coordinator".to_string()),
            )
            .await
            .unwrap();

        // 3. Coordinator root execution on Gemini
        let coord_model_ref = ModelRef::new(gemini_instance_id, gemini_model_id);
        let coord_prep = factory.prepare_runtime_session(&coord_model_ref).unwrap();
        let mut coord_options = StartThreadOptions::new(base_config.clone());
        coord_prep.apply_to_start_thread_options(&mut coord_options);

        let coord_tracker = Arc::new(AgentBudgetTracker::new(
            coord_agent_id,
            AgentExecutionBudget::default(),
        ));
        let mut coord_init = codex_extension_api::ExtensionDataInit::new();
        let coord_ext_ctx = Arc::new(AgentRuntimeExtensionContext::new(
            studio_id,
            coord_agent_id,
            Arc::clone(&coord_tracker),
            cp_handle.clone(),
        ));
        coord_init.insert((*coord_ext_ctx).clone());
        coord_options.thread_extension_init = coord_init;

        let coord_thread = manager.start_thread(coord_options).await.unwrap();
        let (coord_done, coord_err, coord_msgs) =
            run_turn(&coord_thread.thread, "Plan project architecture").await;
        assert!(coord_done, "Coordinator turn failed: {:?}", coord_err);
        assert!(
            coord_msgs
                .iter()
                .any(|m| m.contains("Coordinator: plan completed")),
            "Expected coordinator message, got: {:?}",
            coord_msgs
        );

        // 4. Coordinator spawns Child 1 (Coder on Anthropic Messages) via AgentControl::spawn
        let coder_model_ref = ModelRef::new(anthropic_instance_id, anthropic_model_id);
        let coder_prep = factory.prepare_runtime_session(&coder_model_ref).unwrap();
        let coder_agent_id = AgentId::new();
        cp_handle
            .register_agent_with_id(
                studio_id,
                coder_agent_id,
                "Coder",
                AgentKind::Internal,
                Some("coder".to_string()),
            )
            .await
            .unwrap();

        let coder_tracker = Arc::new(AgentBudgetTracker::new(
            coder_agent_id,
            AgentExecutionBudget::default(),
        ));
        let mut coder_init = codex_extension_api::ExtensionDataInit::new();
        let coder_ext_ctx = Arc::new(AgentRuntimeExtensionContext::new(
            studio_id,
            coder_agent_id,
            Arc::clone(&coder_tracker),
            cp_handle.clone(),
        ));
        coder_init.insert((*coder_ext_ctx).clone());

        let mut coder_config = base_config.clone();
        coder_config.model_provider_id = coder_prep.model_provider_id().to_string();
        coder_config.model = Some(coder_prep.selected_model().to_string());

        let (coder_live_agent, _) = coord_thread
            .thread
            .agent_control()
            .spawn(SpawnRequest {
                caller: coord_thread.thread_id,
                config: coder_config,
                input: AgentInput::UserInput(vec![UserInput::Text {
                    text: "Write implementation".to_string(),
                    text_elements: vec![],
                }]),
                source: SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                    parent_thread_id: coord_thread.thread_id,
                    depth: 1,
                    agent_path: None,
                    agent_nickname: Some("coder".to_string()),
                    agent_role: Some("coder".to_string()),
                }),
                options: SpawnAgentOptions {
                    parent_thread_id: Some(coord_thread.thread_id),
                    ..Default::default()
                },
                model_runtime_override: Some(coder_prep.runtime_override().clone()),
                thread_extension_init: coder_init,
            })
            .await
            .unwrap();

        let coder_thread = manager
            .get_thread(coder_live_agent.thread_id)
            .await
            .unwrap();
        let (coder_done, coder_err, coder_msgs) = wait_for_turn(&coder_thread).await;
        assert!(coder_done, "Coder turn failed: {:?}", coder_err);
        assert!(
            coder_msgs
                .iter()
                .any(|m| m.contains("Coder: implemented module")),
            "Expected coder message, got: {:?}",
            coder_msgs
        );

        // 5. Coordinator spawns Child 2 (Reviewer on OpenAI Chat Completions) via AgentControl::spawn
        let reviewer_model_ref = ModelRef::new(openai_instance_id, openai_model_id);
        let reviewer_prep = factory
            .prepare_runtime_session(&reviewer_model_ref)
            .unwrap();
        let reviewer_agent_id = AgentId::new();
        cp_handle
            .register_agent_with_id(
                studio_id,
                reviewer_agent_id,
                "Reviewer",
                AgentKind::Internal,
                Some("reviewer".to_string()),
            )
            .await
            .unwrap();

        let reviewer_tracker = Arc::new(AgentBudgetTracker::new(
            reviewer_agent_id,
            AgentExecutionBudget::default(),
        ));
        let mut reviewer_init = codex_extension_api::ExtensionDataInit::new();
        let reviewer_ext_ctx = Arc::new(AgentRuntimeExtensionContext::new(
            studio_id,
            reviewer_agent_id,
            Arc::clone(&reviewer_tracker),
            cp_handle.clone(),
        ));
        reviewer_init.insert((*reviewer_ext_ctx).clone());

        let mut reviewer_config = base_config.clone();
        reviewer_config.model_provider_id = reviewer_prep.model_provider_id().to_string();
        reviewer_config.model = Some(reviewer_prep.selected_model().to_string());

        let (reviewer_live_agent, _) = coord_thread
            .thread
            .agent_control()
            .spawn(SpawnRequest {
                caller: coord_thread.thread_id,
                config: reviewer_config,
                input: AgentInput::UserInput(vec![UserInput::Text {
                    text: "Audit code diff".to_string(),
                    text_elements: vec![],
                }]),
                source: SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                    parent_thread_id: coord_thread.thread_id,
                    depth: 1,
                    agent_path: None,
                    agent_nickname: Some("reviewer".to_string()),
                    agent_role: Some("reviewer".to_string()),
                }),
                options: SpawnAgentOptions {
                    parent_thread_id: Some(coord_thread.thread_id),
                    ..Default::default()
                },
                model_runtime_override: Some(reviewer_prep.runtime_override().clone()),
                thread_extension_init: reviewer_init,
            })
            .await
            .unwrap();

        let reviewer_thread = manager
            .get_thread(reviewer_live_agent.thread_id)
            .await
            .unwrap();
        let (reviewer_done, reviewer_err, reviewer_msgs) = wait_for_turn(&reviewer_thread).await;
        assert!(reviewer_done, "Reviewer turn failed: {:?}", reviewer_err);
        assert!(
            reviewer_msgs
                .iter()
                .any(|m| m.contains("Reviewer: code looks great")),
            "Expected reviewer message, got: {:?}",
            reviewer_msgs
        );

        // 6. Assert independent request routing & payload formats across all 3 mock servers
        let gemini_requests = gemini_server.received_requests().await.unwrap();
        assert_eq!(gemini_requests.len(), 1);
        assert!(gemini_requests[0].headers.contains_key("x-goog-api-key"));
        let gemini_body = String::from_utf8_lossy(&gemini_requests[0].body);
        assert!(
            gemini_body.contains("contents") || gemini_body.contains("parts"),
            "Gemini request missing content structure: {gemini_body}"
        );

        let anthropic_requests = anthropic_server.received_requests().await.unwrap();
        assert_eq!(anthropic_requests.len(), 1);
        assert_eq!(
            anthropic_requests[0].headers.get("x-api-key").unwrap(),
            "anthropic-secret-token-222"
        );
        let anthropic_body = String::from_utf8_lossy(&anthropic_requests[0].body);
        assert!(
            anthropic_body.contains("messages"),
            "Anthropic request missing messages payload: {anthropic_body}"
        );

        let openai_requests = openai_server.received_requests().await.unwrap();
        assert_eq!(openai_requests.len(), 1);
        assert_eq!(
            openai_requests[0].headers.get("authorization").unwrap(),
            "Bearer openai-secret-token-333"
        );
        let openai_body = String::from_utf8_lossy(&openai_requests[0].body);
        assert!(
            openai_body.contains("messages"),
            "OpenAI request missing chat messages payload: {openai_body}"
        );
    });
}

#[test]
fn real_cross_provider_hierarchy_visible_via_agent_control_list() {
    run_with_large_stack(async {
        let coord_server = MockServer::start().await;
        let coder_server = MockServer::start().await;
        let reviewer_server = MockServer::start().await;

        let coord_sse = "data: {\"id\":\"chat-coord\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Coord ready\"},\"finish_reason\":null}]}\n\n\
                         data: {\"id\":\"chat-coord\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n\
                         data: [DONE]\n\n";
        let coder_sse = "data: {\"id\":\"chat-coder\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Coder ready\"},\"finish_reason\":null}]}\n\n\
                         data: {\"id\":\"chat-coder\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n\
                         data: [DONE]\n\n";
        let reviewer_sse = "data: {\"id\":\"chat-rev\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Reviewer ready\"},\"finish_reason\":null}]}\n\n\
                           data: {\"id\":\"chat-rev\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n\
                           data: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(coord_sse),
            )
            .mount(&coord_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(coder_sse),
            )
            .mount(&coder_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(reviewer_sse),
            )
            .mount(&reviewer_server)
            .await;

        let mut catalog = ProviderCatalog::new();
        let secret_resolver = Arc::new(
            InMemorySecretResolver::new()
                .with_env_secret("COORD_KEY", "c-key")
                .with_env_secret("CODER_KEY", "cd-key")
                .with_env_secret("REV_KEY", "r-key"),
        );

        let coord_instance = register_provider_and_instance(
            &mut catalog,
            "p-coord",
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
        let m_coord = ModelId::new("m-coord").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    coord_instance,
                    m_coord.clone(),
                    "Coord",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let coder_instance = register_provider_and_instance(
            &mut catalog,
            "p-coder",
            "Coder Provider",
            ProtocolFamily::OpenAiChatCompletions,
            &coder_server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "CODER_KEY".to_string(),
                },
            },
        );
        let m_coder = ModelId::new("m-coder").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    coder_instance,
                    m_coder.clone(),
                    "Coder",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let rev_instance = register_provider_and_instance(
            &mut catalog,
            "p-rev",
            "Reviewer Provider",
            ProtocolFamily::OpenAiChatCompletions,
            &reviewer_server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "REV_KEY".to_string(),
                },
            },
        );
        let m_rev = ModelId::new("m-rev").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    rev_instance,
                    m_rev.clone(),
                    "Reviewer",
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

        let studio = cp_handle.create_studio("hierarchy-studio").await.unwrap();
        let studio_id = studio.id;
        let coord_agent_id = AgentId::new();
        cp_handle
            .register_agent_with_id(
                studio_id,
                coord_agent_id,
                "Coordinator",
                AgentKind::Internal,
                Some("coord".to_string()),
            )
            .await
            .unwrap();

        let coord_prep = factory
            .prepare_runtime_session(&ModelRef::new(coord_instance, m_coord))
            .unwrap();
        let mut coord_options = StartThreadOptions::new(base_config.clone());
        coord_prep.apply_to_start_thread_options(&mut coord_options);

        let coord_thread = manager.start_thread(coord_options).await.unwrap();
        let _ = cp_handle
            .record_runtime_bound(
                studio_id,
                coord_agent_id,
                "codex",
                coord_prep.model_provider_id(),
                coord_prep.selected_model(),
                coord_prep.protocol().to_string(),
            )
            .await;

        // Spawn Child 1 (coder)
        let coder_prep = factory
            .prepare_runtime_session(&ModelRef::new(coder_instance, m_coder))
            .unwrap();
        let coder_agent_id = AgentId::new();
        cp_handle
            .register_agent_with_id(
                studio_id,
                coder_agent_id,
                "Coder",
                AgentKind::Internal,
                Some("coder".to_string()),
            )
            .await
            .unwrap();
        let mut coder_config = base_config.clone();
        coder_config.model_provider_id = coder_prep.model_provider_id().to_string();
        coder_config.model = Some(coder_prep.selected_model().to_string());

        let (coder_live_agent, _) = coord_thread
            .thread
            .agent_control()
            .spawn(SpawnRequest {
                caller: coord_thread.thread_id,
                config: coder_config,
                input: AgentInput::UserInput(vec![UserInput::Text {
                    text: "Code task".to_string(),
                    text_elements: vec![],
                }]),
                source: SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                    parent_thread_id: coord_thread.thread_id,
                    depth: 1,
                    agent_path: None,
                    agent_nickname: Some("coder".to_string()),
                    agent_role: Some("coder".to_string()),
                }),
                options: SpawnAgentOptions {
                    parent_thread_id: Some(coord_thread.thread_id),
                    ..Default::default()
                },
                model_runtime_override: Some(coder_prep.runtime_override().clone()),
                thread_extension_init: codex_extension_api::ExtensionDataInit::new(),
            })
            .await
            .unwrap();

        // Spawn Child 2 (reviewer)
        let rev_prep = factory
            .prepare_runtime_session(&ModelRef::new(rev_instance, m_rev))
            .unwrap();
        let reviewer_agent_id = AgentId::new();
        cp_handle
            .register_agent_with_id(
                studio_id,
                reviewer_agent_id,
                "Reviewer",
                AgentKind::Internal,
                Some("reviewer".to_string()),
            )
            .await
            .unwrap();
        let mut rev_config = base_config.clone();
        rev_config.model_provider_id = rev_prep.model_provider_id().to_string();
        rev_config.model = Some(rev_prep.selected_model().to_string());

        let (reviewer_live_agent, _) = coord_thread
            .thread
            .agent_control()
            .spawn(SpawnRequest {
                caller: coord_thread.thread_id,
                config: rev_config,
                input: AgentInput::UserInput(vec![UserInput::Text {
                    text: "Review task".to_string(),
                    text_elements: vec![],
                }]),
                source: SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                    parent_thread_id: coord_thread.thread_id,
                    depth: 1,
                    agent_path: None,
                    agent_nickname: Some("reviewer".to_string()),
                    agent_role: Some("reviewer".to_string()),
                }),
                options: SpawnAgentOptions {
                    parent_thread_id: Some(coord_thread.thread_id),
                    ..Default::default()
                },
                model_runtime_override: Some(rev_prep.runtime_override().clone()),
                thread_extension_init: codex_extension_api::ExtensionDataInit::new(),
            })
            .await
            .unwrap();

        // 1. Invoke coordinator_thread.agent_control().list()
        let listed = coord_thread
            .thread
            .agent_control()
            .list(coord_thread.thread_id, None, &SessionSource::Exec, None)
            .await
            .unwrap();

        assert!(
            listed
                .iter()
                .any(|a| a.thread_id == coder_live_agent.thread_id),
            "Coder thread not found in agent_control().list(): {:?}",
            listed
        );
        assert!(
            listed
                .iter()
                .any(|a| a.thread_id == reviewer_live_agent.thread_id),
            "Reviewer thread not found in agent_control().list(): {:?}",
            listed
        );

        // 2. Assert child agent parent relationships match in Codex configuration
        let coder_thread = manager
            .get_thread(coder_live_agent.thread_id)
            .await
            .unwrap();
        let coder_config = coder_thread.config_snapshot().await;
        assert_eq!(coder_config.parent_thread_id, Some(coord_thread.thread_id));

        let reviewer_thread = manager
            .get_thread(reviewer_live_agent.thread_id)
            .await
            .unwrap();
        let reviewer_config = reviewer_thread.config_snapshot().await;
        assert_eq!(
            reviewer_config.parent_thread_id,
            Some(coord_thread.thread_id)
        );

        // 3. Record durable spawn relation in ControlPlane and verify read models
        cp_handle
            .record_agent_spawned(
                studio_id,
                coder_agent_id,
                coder_live_agent.thread_id.to_string(),
                Some(coord_agent_id),
                Some(coord_thread.thread_id.to_string()),
            )
            .await
            .unwrap();

        cp_handle
            .record_agent_spawned(
                studio_id,
                reviewer_agent_id,
                reviewer_live_agent.thread_id.to_string(),
                Some(coord_agent_id),
                Some(coord_thread.thread_id.to_string()),
            )
            .await
            .unwrap();

        let (events, _) = cp_handle.subscribe_events(studio_id, 0).await.unwrap();

        let coder_spawn_ev = events.iter().find(|env| {
            matches!(
                &env.event,
                ControlPlaneEvent::AgentSpawned {
                    agent_id,
                    parent_agent_id: Some(p),
                    ..
                } if *agent_id == coder_agent_id && *p == coord_agent_id
            )
        });
        assert!(
            coder_spawn_ev.is_some(),
            "AgentSpawned event for coder missing or wrong parent"
        );

        let rev_spawn_ev = events.iter().find(|env| {
            matches!(
                &env.event,
                ControlPlaneEvent::AgentSpawned {
                    agent_id,
                    parent_agent_id: Some(p),
                    ..
                } if *agent_id == reviewer_agent_id && *p == coord_agent_id
            )
        });
        assert!(
            rev_spawn_ev.is_some(),
            "AgentSpawned event for reviewer missing or wrong parent"
        );

        let coder_summary = project_agent_summary(coder_agent_id, &events, None).unwrap();
        assert_eq!(coder_summary.spawn_depth, 1);
        assert_eq!(
            coder_summary.parent_thread_id,
            Some(coord_thread.thread_id.to_string())
        );

        let reviewer_summary = project_agent_summary(reviewer_agent_id, &events, None).unwrap();
        assert_eq!(reviewer_summary.spawn_depth, 1);
        assert_eq!(
            reviewer_summary.parent_thread_id,
            Some(coord_thread.thread_id.to_string())
        );

        let coord_summary = project_agent_summary(coord_agent_id, &events, None).unwrap();
        assert_eq!(coord_summary.spawn_depth, 0);
        assert_eq!(coord_summary.parent_thread_id, None);
    });
}

#[test]
fn same_slug_different_instance_child_override() {
    run_with_large_stack(async {
        let server_a = MockServer::start().await;
        let server_b = MockServer::start().await;

        let sse_a = "data: {\"id\":\"chat-a\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Coord from Instance A\"},\"finish_reason\":null}]}\n\n\
                     data: {\"id\":\"chat-a\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":5,\"total_tokens\":10}}\n\n\
                     data: [DONE]\n\n";

        let sse_b = "data: {\"id\":\"chat-b\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Child from Instance B\"},\"finish_reason\":null}]}\n\n\
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
            .expect(1)
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
            .expect(1)
            .mount(&server_b)
            .await;

        let mut catalog = ProviderCatalog::new();
        let secret_resolver = Arc::new(
            InMemorySecretResolver::new()
                .with_env_secret("KEY_A", "token-tenant-A")
                .with_env_secret("KEY_B", "token-tenant-B"),
        );

        let instance_a = register_provider_and_instance(
            &mut catalog,
            "provider-a",
            "Provider A",
            ProtocolFamily::OpenAiChatCompletions,
            &server_a.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "KEY_A".to_string(),
                },
            },
        );

        let instance_b = register_provider_and_instance(
            &mut catalog,
            "provider-b",
            "Provider B",
            ProtocolFamily::OpenAiChatCompletions,
            &server_b.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "KEY_B".to_string(),
                },
            },
        );

        let shared_model_id = ModelId::new("shared-model-slug").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    instance_a,
                    shared_model_id.clone(),
                    "Shared A",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    instance_b,
                    shared_model_id.clone(),
                    "Shared B",
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

        // Coordinator on Instance A
        let ref_a = ModelRef::new(instance_a, shared_model_id.clone());
        let prep_a = factory.prepare_runtime_session(&ref_a).unwrap();
        let mut opt_a = StartThreadOptions::new(base_config.clone());
        prep_a.apply_to_start_thread_options(&mut opt_a);

        let coord_thread = manager.start_thread(opt_a).await.unwrap();
        let (done_a, _, msgs_a) = run_turn(&coord_thread.thread, "Call instance A").await;
        assert!(done_a);
        assert!(msgs_a.iter().any(|m| m.contains("Coord from Instance A")));

        // Coordinator spawns Child with override targeting Instance B with same slug
        let ref_b = ModelRef::new(instance_b, shared_model_id);
        let prep_b = factory.prepare_runtime_session(&ref_b).unwrap();
        let mut child_cfg = base_config.clone();
        child_cfg.model_provider_id = prep_b.model_provider_id().to_string();
        child_cfg.model = Some(prep_b.selected_model().to_string());

        let (child_agent, _) = coord_thread
            .thread
            .agent_control()
            .spawn(SpawnRequest {
                caller: coord_thread.thread_id,
                config: child_cfg,
                input: AgentInput::UserInput(vec![UserInput::Text {
                    text: "Call instance B".to_string(),
                    text_elements: vec![],
                }]),
                source: SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                    parent_thread_id: coord_thread.thread_id,
                    depth: 1,
                    agent_path: None,
                    agent_nickname: Some("child".to_string()),
                    agent_role: Some("child".to_string()),
                }),
                options: SpawnAgentOptions {
                    parent_thread_id: Some(coord_thread.thread_id),
                    ..Default::default()
                },
                model_runtime_override: Some(prep_b.runtime_override().clone()),
                thread_extension_init: codex_extension_api::ExtensionDataInit::new(),
            })
            .await
            .unwrap();

        let child_thread = manager.get_thread(child_agent.thread_id).await.unwrap();
        let (done_b, _, msgs_b) = wait_for_turn(&child_thread).await;
        assert!(done_b);
        assert!(msgs_b.iter().any(|m| m.contains("Child from Instance B")));

        // Server A received 1 request with Token A; Server B received 1 request with Token B
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
fn real_failfast_interrupts_slow_codex_worker() {
    run_with_large_stack(async {
        let slow_server = MockServer::start().await;

        let slow_sse = "data: {\"id\":\"chat-slow\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Slow worker streaming\"},\"finish_reason\":null}]}\n\n\
                        data: {\"id\":\"chat-slow\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":5,\"total_tokens\":10}}\n\n\
                        data: [DONE]\n\n";

        // Set delay on slow server long enough for failfast interrupt
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(slow_sse)
                    .set_delay(Duration::from_secs(10)),
            )
            .mount(&slow_server)
            .await;

        let mut catalog = ProviderCatalog::new();
        let secret_resolver =
            Arc::new(InMemorySecretResolver::new().with_env_secret("SLOW_KEY", "slow-token"));

        let instance_id = register_provider_and_instance(
            &mut catalog,
            "slow-provider",
            "Slow Provider",
            ProtocolFamily::OpenAiChatCompletions,
            &slow_server.uri(),
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "SLOW_KEY".to_string(),
                },
            },
        );

        let model_id = ModelId::new("slow-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    instance_id,
                    model_id.clone(),
                    "Slow Model",
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

        let studio = cp_handle.create_studio("failfast-studio").await.unwrap();
        let studio_id = studio.id;
        let worker_id = AgentId::new();
        cp_handle
            .register_agent_with_id(
                studio_id,
                worker_id,
                "Slow Worker",
                AgentKind::Internal,
                Some("worker".to_string()),
            )
            .await
            .unwrap();

        let task = cp_handle
            .create_task(studio_id, "Slow Task", "desc", None, None, vec![])
            .await
            .unwrap();
        let run = cp_handle.create_run(task.id, worker_id).await.unwrap();

        let executor = Arc::new(
            CodexAgentExecutor::try_new(
                Arc::new(factory),
                Arc::new(manager),
                Arc::new(base_config),
                cp_handle.clone(),
            )
            .unwrap(),
        );

        let worker_spec = InternalAgentSpec::new(
            worker_id,
            "Slow Worker",
            "worker",
            ModelRef::new(instance_id, model_id),
        );

        let worker_ctx = AgentExecutionContext {
            studio_id,
            task_id: Some(task.id),
            run_id: Some(run.id),
            agent_spec: worker_spec,
            prompt: "Start long calculation".to_string(),
            output_schema: None,
            budget: AgentExecutionBudget::default(),
            parent_agent_id: None,
        };

        // Spawn agent execution in background task
        let exec_clone = Arc::clone(&executor);
        let execution_task =
            tokio::spawn(async move { exec_clone.execute_agent(worker_ctx).await });

        // Wait until agent starts running
        let wait_deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        let mut started = false;
        while tokio::time::Instant::now() < wait_deadline {
            if execution_task.is_finished() {
                break;
            }
            let running = executor.running_agents();
            let guard = running.read().await;
            if guard.contains_key(&worker_id) {
                started = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if !started && execution_task.is_finished() {
            let task_res = execution_task.await;
            panic!(
                "Worker execution task finished early without registering: {:?}",
                task_res
            );
        }
        assert!(started, "Worker agent failed to register as running");

        // FailFast trigger: interrupt slow worker B
        executor.cancel_agent(worker_id).await.unwrap();

        // Worker execution completes with failure due to interrupt
        let result = execution_task.await.unwrap().unwrap();
        assert!(!result.success, "Interrupted agent should report failure");

        // Assert agent returned to Idle state in ControlPlane and is reusable
        let cp_state = cp_handle.get_state().await.unwrap();
        assert_eq!(
            cp_state.agents.get(&worker_id).unwrap().state,
            AgentState::Idle,
            "Interrupted agent must return to Idle state"
        );
    });
}

#[test]
fn late_completion_cannot_mark_cancelled_run_successful() {
    run_with_large_stack(async {
        let (cp_handle, _actor_task) = create_test_actor();
        let studio = cp_handle.create_studio("cancel-test-studio").await.unwrap();
        let studio_id = studio.id;
        let agent_id = AgentId::new();
        cp_handle
            .register_agent_with_id(
                studio_id,
                agent_id,
                "Worker Agent",
                AgentKind::Internal,
                Some("worker".to_string()),
            )
            .await
            .unwrap();

        let task = cp_handle
            .create_task(studio_id, "Task to cancel", "desc", None, None, vec![])
            .await
            .unwrap();
        let task_id = task.id;
        let run = cp_handle.create_run(task_id, agent_id).await.unwrap();
        let run_id = run.id;

        // Transition run and task to Cancelled
        cp_handle
            .transition_run_state(run_id, RunState::Cancelled)
            .await
            .unwrap();
        cp_handle
            .transition_task_state(task_id, TaskState::Cancelled)
            .await
            .unwrap();

        // Attempt late completion by marking run Succeeded
        let late_run_res = cp_handle
            .transition_run_state(run_id, RunState::Succeeded)
            .await;
        assert!(
            matches!(
                late_run_res,
                Err(InternalAgentError::ControlPlane(
                    ControlPlaneError::Transition(TransitionError::TerminalRunTransition {
                        state: RunState::Cancelled,
                    })
                ))
            ),
            "Expected TerminalRunTransition error, got: {:?}",
            late_run_res
        );

        // Attempt late completion by marking task Succeeded
        let late_task_res = cp_handle
            .transition_task_state(task_id, TaskState::Succeeded)
            .await;
        assert!(
            matches!(
                late_task_res,
                Err(InternalAgentError::ControlPlane(
                    ControlPlaneError::Transition(TransitionError::TerminalTaskTransition {
                        state: TaskState::Cancelled,
                    })
                ))
            ),
            "Expected TerminalTaskTransition error, got: {:?}",
            late_task_res
        );

        // Assert state remains Cancelled
        let cp_state = cp_handle.get_state().await.unwrap();
        assert_eq!(
            cp_state.runs.get(&run_id).unwrap().state,
            RunState::Cancelled,
            "Run state must remain Cancelled"
        );
        assert_eq!(
            cp_state.task_graph.get_task(task_id).unwrap().state,
            TaskState::Cancelled,
            "Task state must remain Cancelled"
        );
    });
}

struct BodyContainsMatcher(&'static str);

impl wiremock::Match for BodyContainsMatcher {
    fn matches(&self, request: &wiremock::Request) -> bool {
        let body_str = String::from_utf8_lossy(&request.body);
        body_str.contains(self.0)
    }
}

#[test]
fn real_supervisor_failfast_interrupts_inflight_codex_sibling() {
    run_with_large_stack(async {
        // 1. Set up Coordinator WireMock server
        let coord_server = MockServer::start().await;

        let plan_decision = serde_json::json!({
            "action": "plan",
            "tasks": [
                {
                    "task_key": "task-a",
                    "title": "Task A for Worker A",
                    "description": "Failing task",
                    "assigned_alias": "worker-a",
                    "depends_on": [],
                    "workspace_access": "read_only",
                    "priority": 1
                },
                {
                    "task_key": "task-b",
                    "title": "Task B for Worker B",
                    "description": "Slow task",
                    "assigned_alias": "worker-b",
                    "depends_on": [],
                    "workspace_access": "read_only",
                    "priority": 1
                }
            ]
        });
        let plan_str = serde_json::to_string(&plan_decision).unwrap();
        let escaped_plan = plan_str.replace('"', "\\\"");

        let coord_plan_sse = format!(
            "data: {{\"id\":\"chat-coord\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{}\"}},\"finish_reason\":null}}]}}\n\n\
             data: {{\"id\":\"chat-coord\",\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":10,\"completion_tokens\":10,\"total_tokens\":20}}}}\n\n\
             data: [DONE]\n\n",
            escaped_plan
        );

        let coord_review_sse = "data: {\"id\":\"chat-coord-review\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Final review: FailFast triggered cancellation of Worker B after Worker A failed.\"},\"finish_reason\":null}]}\n\n\
                                data: {\"id\":\"chat-coord-review\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":10,\"total_tokens\":20}}\n\n\
                                data: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("Available team members:"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(coord_plan_sse),
            )
            .mount(&coord_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(BodyContainsMatcher("Review the executed tasks"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(coord_review_sse),
            )
            .mount(&coord_server)
            .await;

        // 2. Set up Worker B custom TCP SSE server (stays open, detects client disconnect)
        let worker_b_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let worker_b_addr = worker_b_listener.local_addr().unwrap();
        let worker_b_url = format!("http://{}", worker_b_addr);

        let b_request_started_flag = Arc::new(AtomicBool::new(false));
        let b_request_started_notify = Arc::new(Notify::new());
        let b_client_disconnected_flag = Arc::new(AtomicBool::new(false));
        let b_client_disconnected_notify = Arc::new(Notify::new());

        let b_started_flag_clone = Arc::clone(&b_request_started_flag);
        let b_started_notify_clone = Arc::clone(&b_request_started_notify);
        let b_disconnected_flag_clone = Arc::clone(&b_client_disconnected_flag);
        let b_disconnected_notify_clone = Arc::clone(&b_client_disconnected_notify);

        let _worker_b_server_task = tokio::spawn(async move {
            if let Ok((mut socket, _)) = worker_b_listener.accept().await {
                let mut req_buf = Vec::new();
                let mut temp_buf = [0u8; 1024];
                while !req_buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = match socket.read(&mut temp_buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    req_buf.extend_from_slice(&temp_buf[..n]);
                }

                // Drain request body if Content-Length present
                let header_str = String::from_utf8_lossy(&req_buf);
                let mut content_length = 0;
                for line in header_str.lines() {
                    if line.to_lowercase().starts_with("content-length:") {
                        content_length = line
                            .split(':')
                            .nth(1)
                            .and_then(|val| val.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                    }
                }
                if let Some(header_end) = req_buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let body_start = header_end + 4;
                    let body_already_read = req_buf.len() - body_start;
                    let mut remaining = content_length.saturating_sub(body_already_read);
                    while remaining > 0 {
                        let to_read = remaining.min(temp_buf.len());
                        match socket.read(&mut temp_buf[..to_read]).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => remaining = remaining.saturating_sub(n),
                        }
                    }
                }

                b_started_flag_clone.store(true, Ordering::SeqCst);
                b_started_notify_clone.notify_waiters();

                let partial_sse = "HTTP/1.1 200 OK\r\n\
                                   Content-Type: text/event-stream\r\n\
                                   Cache-Control: no-cache\r\n\
                                   Connection: close\r\n\
                                   \r\n\
                                   data: {\"id\":\"chat-b\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Worker B working chunk\"},\"finish_reason\":null}]}\n\n";

                if socket.write_all(partial_sse.as_bytes()).await.is_ok() {
                    let _ = socket.flush().await;
                }

                // Wait for client to disconnect upon cancellation (never send [DONE])
                loop {
                    match socket.read(&mut temp_buf).await {
                        Ok(0) | Err(_) => {
                            b_disconnected_flag_clone.store(true, Ordering::SeqCst);
                            b_disconnected_notify_clone.notify_waiters();
                            break;
                        }
                        Ok(_) => {}
                    }
                }
            }
        });

        // 3. Set up Worker A custom TCP server (waits for allow_failure, returns HTTP 400 Bad Request)
        let worker_a_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let worker_a_addr = worker_a_listener.local_addr().unwrap();
        let worker_a_url = format!("http://{}", worker_a_addr);

        let a_allow_failure_flag = Arc::new(AtomicBool::new(false));
        let a_allow_failure_notify = Arc::new(Notify::new());

        let a_allow_flag_clone = Arc::clone(&a_allow_failure_flag);
        let a_allow_notify_clone = Arc::clone(&a_allow_failure_notify);

        let _worker_a_server_task = tokio::spawn(async move {
            if let Ok((mut socket, _)) = worker_a_listener.accept().await {
                let mut req_buf = Vec::new();
                let mut temp_buf = [0u8; 1024];
                while !req_buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = match socket.read(&mut temp_buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    req_buf.extend_from_slice(&temp_buf[..n]);
                }

                // Drain request body if Content-Length present
                let header_str = String::from_utf8_lossy(&req_buf);
                let mut content_length = 0;
                for line in header_str.lines() {
                    if line.to_lowercase().starts_with("content-length:") {
                        content_length = line
                            .split(':')
                            .nth(1)
                            .and_then(|val| val.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                    }
                }
                if let Some(header_end) = req_buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let body_start = header_end + 4;
                    let body_already_read = req_buf.len() - body_start;
                    let mut remaining = content_length.saturating_sub(body_already_read);
                    while remaining > 0 {
                        let to_read = remaining.min(temp_buf.len());
                        match socket.read(&mut temp_buf[..to_read]).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => remaining = remaining.saturating_sub(n),
                        }
                    }
                }

                // Wait until Worker B is verified running
                while !a_allow_flag_clone.load(Ordering::SeqCst) {
                    a_allow_notify_clone.notified().await;
                }

                let err_body = "{\"error\":{\"message\":\"Simulated deterministic failure for Worker A\",\"type\":\"invalid_request_error\"}}";
                let err_resp = format!(
                    "HTTP/1.1 400 Bad Request\r\n\
                     Content-Type: application/json\r\n\
                     Content-Length: {}\r\n\
                     Connection: close\r\n\
                     \r\n\
                     {}",
                    err_body.len(),
                    err_body
                );

                if socket.write_all(err_resp.as_bytes()).await.is_ok() {
                    let _ = socket.flush().await;
                }
            }
        });

        // 4. Provider Catalog & Models
        let mut catalog = ProviderCatalog::new();
        let secret_resolver = Arc::new(
            InMemorySecretResolver::new()
                .with_env_secret("COORD_KEY", "coord-secret")
                .with_env_secret("WORKER_A_KEY", "worker-a-secret")
                .with_env_secret("WORKER_B_KEY", "worker-b-secret"),
        );

        let coord_inst_id = register_provider_and_instance(
            &mut catalog,
            "provider-coord",
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
        let coord_model_id = ModelId::new("coord-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    coord_inst_id,
                    coord_model_id.clone(),
                    "Coord Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let worker_a_inst_id = register_provider_and_instance(
            &mut catalog,
            "provider-worker-a",
            "Worker A Provider",
            ProtocolFamily::OpenAiChatCompletions,
            &worker_a_url,
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "WORKER_A_KEY".to_string(),
                },
            },
        );
        let worker_a_model_id = ModelId::new("worker-a-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    worker_a_inst_id,
                    worker_a_model_id.clone(),
                    "Worker A Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let worker_b_inst_id = register_provider_and_instance(
            &mut catalog,
            "provider-worker-b",
            "Worker B Provider",
            ProtocolFamily::OpenAiChatCompletions,
            &worker_b_url,
            AuthenticationScheme::BearerToken {
                secret: SecretReference {
                    backend: SecretBackend::EnvironmentVariable,
                    locator: "WORKER_B_KEY".to_string(),
                },
            },
        );
        let worker_b_model_id = ModelId::new("worker-b-model").unwrap();
        catalog
            .register_model(
                ModelDescriptor::new(
                    worker_b_inst_id,
                    worker_b_model_id.clone(),
                    "Worker B Model",
                    ModelCapabilities::default(),
                    ModelLimits::default(),
                )
                .unwrap(),
            )
            .unwrap();

        let catalog = Arc::new(catalog);
        let continuation_manager = Arc::new(ContinuationManager::new());
        let factory = Arc::new(AgentStudiosRuntimeSessionFactory::new(
            catalog,
            secret_resolver,
            continuation_manager,
        ));

        let temp_dir = tempfile::tempdir().unwrap();
        let (manager, base_config) = create_test_thread_manager(temp_dir.path()).await;
        let manager = Arc::new(manager);
        let (cp_handle, _actor_task) = create_test_actor();

        let studio = cp_handle
            .create_studio("failfast-e2e-studio")
            .await
            .unwrap();
        let studio_id = studio.id;

        let coord_agent_id = AgentId::new();
        let worker_a_id = AgentId::new();
        let worker_b_id = AgentId::new();

        let coord_spec = InternalAgentSpec::new(
            coord_agent_id,
            "Coordinator",
            "coordinator",
            ModelRef::new(coord_inst_id, coord_model_id),
        );

        let worker_a_spec = InternalAgentSpec::new(
            worker_a_id,
            "Worker A",
            "worker",
            ModelRef::new(worker_a_inst_id, worker_a_model_id),
        )
        .with_workspace_access(WorkspaceAccessMode::ReadOnly);

        let worker_b_spec = InternalAgentSpec::new(
            worker_b_id,
            "Worker B",
            "worker",
            ModelRef::new(worker_b_inst_id, worker_b_model_id),
        )
        .with_workspace_access(WorkspaceAccessMode::ReadOnly);

        let team_spec = InternalTeamSpec::new(studio_id, "failfast-team", coord_spec)
            .add_agent("worker-a", worker_a_spec)
            .unwrap()
            .add_agent("worker-b", worker_b_spec)
            .unwrap()
            .with_max_parallel_agents(2);

        let executor = CodexAgentExecutor::try_new(
            factory,
            Arc::clone(&manager),
            Arc::new(base_config),
            cp_handle.clone(),
        )
        .unwrap();

        let supervisor = AgentStudiosSupervisor::new(
            cp_handle.clone(),
            team_spec,
            WorkspacePolicyArbitrator::default(),
            executor.clone(),
        )
        .with_failure_policy(FailurePolicy::FailFast);

        // 5. Run supervisor in background task
        let sup = Arc::new(supervisor);
        let sup_clone = Arc::clone(&sup);
        let supervisor_run_task =
            tokio::spawn(async move { sup_clone.run("Parallel FailFast Workflow").await });

        // 6. Deterministic synchronization:
        // Wait for Worker B's HTTP request to start
        let wait_deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        while !b_request_started_flag.load(Ordering::SeqCst) {
            if tokio::time::Instant::now() >= wait_deadline {
                panic!("Worker B HTTP request failed to start before deadline");
            }
            let _ = tokio::time::timeout(
                Duration::from_millis(50),
                b_request_started_notify.notified(),
            )
            .await;
        }

        // Wait for Worker B run in ControlPlane to reach Running
        let mut run_b_id_opt = None;
        let mut task_b_id_opt = None;
        while tokio::time::Instant::now() < wait_deadline {
            let cp_state = cp_handle.get_state().await.unwrap();
            if let Some(run_b) = cp_state
                .runs
                .values()
                .find(|r| r.agent_id == worker_b_id && r.state == RunState::Running)
            {
                run_b_id_opt = Some(run_b.id);
                task_b_id_opt = Some(run_b.task_id);
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let run_b_id = run_b_id_opt.expect("Worker B run must be Running in ControlPlane");
        let task_b_id = task_b_id_opt.expect("Worker B task must exist");

        // Confirm Worker B Codex thread is AgentStatus::Running
        let (_worker_b_thread_id, worker_b_thread) = {
            let mut th = None;
            while tokio::time::Instant::now() < wait_deadline {
                let running = executor.running_agents();
                let guard = running.read().await;
                let maybe_thread_id = guard.get(&worker_b_id).map(|s| s.thread_id);
                drop(guard);
                let maybe_thread = match maybe_thread_id {
                    Some(tid) => manager
                        .get_thread(tid)
                        .await
                        .ok()
                        .map(|thread| (tid, thread)),
                    None => None,
                };
                if let Some(pair) = maybe_thread {
                    th = Some(pair);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            th.expect("Worker B live Codex thread must be registered")
        };

        let b_status_running = worker_b_thread.agent_status().await;
        assert_eq!(
            b_status_running,
            AgentStatus::Running,
            "Worker B Codex thread must be Running"
        );

        // Worker B is confirmed in-flight and running. Now allow Worker A failure!
        a_allow_failure_flag.store(true, Ordering::SeqCst);
        a_allow_failure_notify.notify_waiters();

        // Supervisor observes Worker A failure -> FailFast triggers cancellation of Worker B -> Worker B transport disconnects
        let supervisor_summary = tokio::time::timeout(Duration::from_secs(20), supervisor_run_task)
            .await
            .expect("Supervisor run timed out")
            .expect("Supervisor task panicked")
            .expect("Supervisor run returned Err");

        assert_eq!(supervisor_summary.failed_tasks, 1);
        assert_eq!(supervisor_summary.cancelled_tasks, 1);

        // Wait for Worker B client_disconnected signal
        while !b_client_disconnected_flag.load(Ordering::SeqCst) {
            if tokio::time::Instant::now() >= wait_deadline {
                panic!("Worker B client failed to disconnect before deadline");
            }
            let _ = tokio::time::timeout(
                Duration::from_millis(50),
                b_client_disconnected_notify.notified(),
            )
            .await;
        }
        assert!(
            b_client_disconnected_flag.load(Ordering::SeqCst),
            "Worker B transport must disconnect upon FailFast cancellation"
        );

        // 7. Validate Worker B Codex thread leaves AgentStatus::Running
        let b_status_after = worker_b_thread.agent_status().await;
        assert_ne!(
            b_status_after,
            AgentStatus::Running,
            "Worker B Codex thread must leave AgentStatus::Running after cancellation"
        );

        // 8. Find Worker A run_id and task_id
        let cp_state = cp_handle.get_state().await.unwrap();
        let run_a = cp_state
            .runs
            .values()
            .find(|r| r.agent_id == worker_a_id)
            .expect("Worker A run must exist");
        let run_a_id = run_a.id;
        let task_a_id = run_a.task_id;

        // 9. Validate event ordering in durable ControlPlane stream:
        // Worker B Run Running < Worker A Run Failed < CancellationRequested < Worker B Run Cancelled
        let (events, _) = cp_handle.subscribe_events(studio_id, 0).await.unwrap();

        let seq_b_running = events
            .iter()
            .find(|e| {
                matches!(
                    &e.event,
                    ControlPlaneEvent::RunStateChanged {
                        run_id,
                        new_state: RunState::Running,
                        ..
                    } if *run_id == run_b_id
                )
            })
            .map(|e| e.sequence)
            .expect("Worker B Run Running event must exist in event stream");

        let seq_a_failed = events
            .iter()
            .find(|e| {
                matches!(
                    &e.event,
                    ControlPlaneEvent::RunStateChanged {
                        run_id,
                        new_state: RunState::Failed,
                        ..
                    } if *run_id == run_a_id
                )
            })
            .map(|e| e.sequence)
            .expect("Worker A Run Failed event must exist in event stream");

        let seq_cancellation_requested = events
            .iter()
            .find(|e| {
                matches!(
                    &e.event,
                    ControlPlaneEvent::CancellationRequested { scope, .. }
                    if *scope == CancellationScope::Studio(studio_id)
                )
            })
            .map(|e| e.sequence)
            .expect("CancellationRequested event must exist in event stream");

        let seq_b_cancelled = events
            .iter()
            .find(|e| {
                matches!(
                    &e.event,
                    ControlPlaneEvent::RunStateChanged {
                        run_id,
                        new_state: RunState::Cancelled,
                        ..
                    } if *run_id == run_b_id
                )
            })
            .map(|e| e.sequence)
            .expect("Worker B Run Cancelled event must exist in event stream");

        assert!(
            seq_b_running < seq_a_failed,
            "Expected seq(Run B Running) < seq(Run A Failed), got {seq_b_running} >= {seq_a_failed}"
        );
        assert!(
            seq_a_failed < seq_cancellation_requested,
            "Expected seq(Run A Failed) < seq(CancellationRequested), got {seq_a_failed} >= {seq_cancellation_requested}"
        );
        assert!(
            seq_cancellation_requested < seq_b_cancelled,
            "Expected seq(CancellationRequested) < seq(Run B Cancelled), got {seq_cancellation_requested} >= {seq_b_cancelled}"
        );

        // 10. Validate final states: Worker A task/run Failed, Worker B task/run Cancelled, Worker B agent Idle, Coordinator agent Idle
        assert_eq!(
            cp_state.runs.get(&run_a_id).unwrap().state,
            RunState::Failed,
            "Worker A run must be Failed"
        );
        assert_eq!(
            cp_state.task_graph.get_task(task_a_id).unwrap().state,
            TaskState::Failed,
            "Worker A task must be Failed"
        );
        assert_eq!(
            cp_state.runs.get(&run_b_id).unwrap().state,
            RunState::Cancelled,
            "Worker B run must be Cancelled"
        );
        assert_eq!(
            cp_state.task_graph.get_task(task_b_id).unwrap().state,
            TaskState::Cancelled,
            "Worker B task must be Cancelled"
        );
        assert_eq!(
            cp_state.agents.get(&worker_b_id).unwrap().state,
            AgentState::Idle,
            "Worker B agent must be Idle"
        );
        assert_eq!(
            cp_state.agents.get(&coord_agent_id).unwrap().state,
            AgentState::Idle,
            "Coordinator agent must be Idle"
        );

        // 11. Validate late-completion safety: attempting transition_run_state / transition_task_state fails
        let late_run_res = cp_handle
            .transition_run_state(run_b_id, RunState::Succeeded)
            .await;
        assert!(
            matches!(
                late_run_res,
                Err(InternalAgentError::ControlPlane(
                    ControlPlaneError::Transition(TransitionError::TerminalRunTransition {
                        state: RunState::Cancelled,
                    })
                ))
            ),
            "Expected TerminalRunTransition error for late run completion: {:?}",
            late_run_res
        );

        let late_task_res = cp_handle
            .transition_task_state(task_b_id, TaskState::Succeeded)
            .await;
        assert!(
            matches!(
                late_task_res,
                Err(InternalAgentError::ControlPlane(
                    ControlPlaneError::Transition(TransitionError::TerminalTaskTransition {
                        state: TaskState::Cancelled,
                    })
                ))
            ),
            "Expected TerminalTaskTransition error for late task completion: {:?}",
            late_task_res
        );

        let verified_cp_state = cp_handle.get_state().await.unwrap();
        assert_eq!(
            verified_cp_state.runs.get(&run_b_id).unwrap().state,
            RunState::Cancelled
        );
        assert_eq!(
            verified_cp_state
                .task_graph
                .get_task(task_b_id)
                .unwrap()
                .state,
            TaskState::Cancelled
        );
    });
}
