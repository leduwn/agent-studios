use std::sync::Arc;

use agent_studios_codex_bridge::deterministic_codex_provider_key;
use agent_studios_provider::model::ModelRef;
use agent_studios_provider::{
    AuthenticationScheme, CapabilitySupport, EndpointProfile, ModelCapabilities, ModelDescriptor,
    ModelId, ModelLimits, ModelMetadataSource, ProtocolFamily, ProviderCatalog, ProviderDefinition,
    ProviderId, ProviderInstance, ProviderInstanceId, SecretReference,
};
use agent_studios_runtime_session::error::RuntimeSessionError;
use agent_studios_runtime_session::factory::AgentStudiosRuntimeSessionFactory;
use agent_studios_runtime_session::model_descriptor_to_model_info;
use agent_studios_runtime_transport::secret::InMemorySecretResolver;
use agent_studios_runtime_transport::state::ContinuationManager;
use codex_core::StartThreadOptions;
use codex_core::config::ConfigBuilder;
use codex_model_provider::RemoteCompactionSupport;
use codex_models_manager::ModelsManagerConfig;
use codex_models_manager::manager::RefreshStrategy;
use codex_protocol::openai_models::InputModality;

fn setup_test_catalog() -> (
    ProviderCatalog,
    ProviderInstanceId,
    ProviderInstanceId,
    ProviderInstanceId,
    ProviderInstanceId,
) {
    let mut catalog = ProviderCatalog::new();

    // 1. OpenAI Responses
    let openai_def_id = ProviderId::new("openai").unwrap();
    let openai_def = ProviderDefinition::new(
        openai_def_id.clone(),
        "OpenAI",
        vec![ProtocolFamily::OpenAiResponses],
    )
    .unwrap();
    catalog.register_provider_definition(openai_def).unwrap();

    let openai_inst_id = ProviderInstanceId::new();
    let openai_inst = ProviderInstance::new(
        openai_inst_id,
        openai_def_id,
        "OpenAI Production",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("OPENAI_API_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(openai_inst).unwrap();

    let openai_caps = ModelCapabilities {
        tool_calling: CapabilitySupport::Supported,
        streaming: CapabilitySupport::Supported,
        reasoning: CapabilitySupport::Supported,
        ..Default::default()
    };

    let m1 = ModelDescriptor {
        id: ModelId::new("gpt-5.6").unwrap(),
        provider_instance_id: openai_inst_id,
        display_name: "GPT 5.6".to_string(),
        capabilities: openai_caps.clone(),
        limits: ModelLimits::new(Some(128_000), Some(16_384)).unwrap(),
        metadata_source: ModelMetadataSource::Manual,
    };
    let m2 = ModelDescriptor {
        id: ModelId::new("gpt-5.6-mini").unwrap(),
        provider_instance_id: openai_inst_id,
        display_name: "GPT 5.6 Mini".to_string(),
        capabilities: openai_caps.clone(),
        limits: ModelLimits::new(Some(64_000), Some(8_192)).unwrap(),
        metadata_source: ModelMetadataSource::Manual,
    };
    catalog.register_model(m1).unwrap();
    catalog.register_model(m2).unwrap();

    // 2. Anthropic Messages
    let anthropic_def_id = ProviderId::new("anthropic").unwrap();
    let anthropic_def = ProviderDefinition::new(
        anthropic_def_id.clone(),
        "Anthropic",
        vec![ProtocolFamily::AnthropicMessages],
    )
    .unwrap();
    catalog.register_provider_definition(anthropic_def).unwrap();

    let anthropic_inst_id = ProviderInstanceId::new();
    let anthropic_inst = ProviderInstance::new(
        anthropic_inst_id,
        anthropic_def_id,
        "Anthropic Claude",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new("https://api.anthropic.com/v1").unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: SecretReference::env("ANTHROPIC_API_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(anthropic_inst).unwrap();

    let anthropic_caps = ModelCapabilities {
        tool_calling: CapabilitySupport::Supported,
        streaming: CapabilitySupport::Supported,
        reasoning: CapabilitySupport::Supported,
        ..Default::default()
    };

    let claude_model = ModelDescriptor {
        id: ModelId::new("claude-3-7-sonnet").unwrap(),
        provider_instance_id: anthropic_inst_id,
        display_name: "Claude 3.7 Sonnet".to_string(),
        capabilities: anthropic_caps,
        limits: ModelLimits::new(Some(200_000), Some(64_000)).unwrap(),
        metadata_source: ModelMetadataSource::Manual,
    };
    catalog.register_model(claude_model).unwrap();

    // 3. Gemini GenerateContent
    let gemini_def_id = ProviderId::new("gemini").unwrap();
    let gemini_def = ProviderDefinition::new(
        gemini_def_id.clone(),
        "Google Gemini",
        vec![ProtocolFamily::GeminiGenerateContent],
    )
    .unwrap();
    catalog.register_provider_definition(gemini_def).unwrap();

    let gemini_inst_id = ProviderInstanceId::new();
    let gemini_inst = ProviderInstance::new(
        gemini_inst_id,
        gemini_def_id,
        "Google Vertex Gemini",
        ProtocolFamily::GeminiGenerateContent,
        EndpointProfile::new("https://generativelanguage.googleapis.com/v1beta").unwrap(),
        AuthenticationScheme::QueryParameter {
            parameter_name: "key".to_string(),
            secret: SecretReference::env("GEMINI_API_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(gemini_inst).unwrap();

    let gemini_caps = ModelCapabilities {
        tool_calling: CapabilitySupport::Supported,
        streaming: CapabilitySupport::Supported,
        ..Default::default()
    };

    let gemini_model = ModelDescriptor {
        id: ModelId::new("gemini-2.5-pro").unwrap(),
        provider_instance_id: gemini_inst_id,
        display_name: "Gemini 2.5 Pro".to_string(),
        capabilities: gemini_caps,
        limits: ModelLimits::new(Some(1_000_000), Some(8_192)).unwrap(),
        metadata_source: ModelMetadataSource::Manual,
    };
    catalog.register_model(gemini_model).unwrap();

    // 4. OpenAI Chat Completions
    let chat_def_id = ProviderId::new("chat-completions").unwrap();
    let chat_def = ProviderDefinition::new(
        chat_def_id.clone(),
        "Chat Completions",
        vec![ProtocolFamily::OpenAiChatCompletions],
    )
    .unwrap();
    catalog.register_provider_definition(chat_def).unwrap();

    let chat_inst_id = ProviderInstanceId::new();
    let chat_inst = ProviderInstance::new(
        chat_inst_id,
        chat_def_id,
        "DeepSeek Chat",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("https://api.deepseek.com/v1").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("DEEPSEEK_API_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(chat_inst).unwrap();

    let chat_caps = ModelCapabilities {
        tool_calling: CapabilitySupport::Supported,
        streaming: CapabilitySupport::Supported,
        ..Default::default()
    };

    let chat_model = ModelDescriptor {
        id: ModelId::new("deepseek-v3").unwrap(),
        provider_instance_id: chat_inst_id,
        display_name: "DeepSeek V3".to_string(),
        capabilities: chat_caps,
        limits: ModelLimits::new(Some(64_000), Some(8_000)).unwrap(),
        metadata_source: ModelMetadataSource::Manual,
    };
    catalog.register_model(chat_model).unwrap();

    (
        catalog,
        openai_inst_id,
        anthropic_inst_id,
        gemini_inst_id,
        chat_inst_id,
    )
}

fn create_test_factory(catalog: ProviderCatalog) -> AgentStudiosRuntimeSessionFactory {
    let secrets = InMemorySecretResolver::new()
        .with_env_secret("OPENAI_API_KEY", "sk-test-openai")
        .with_env_secret("ANTHROPIC_API_KEY", "sk-ant-test")
        .with_env_secret("GEMINI_API_KEY", "test-gemini-key")
        .with_env_secret("DEEPSEEK_API_KEY", "sk-deepseek-test");

    AgentStudiosRuntimeSessionFactory::new(
        Arc::new(catalog),
        Arc::new(secrets),
        Arc::new(ContinuationManager::new()),
    )
}

#[tokio::test]
async fn test_factory_openai_responses_bridge() {
    let (catalog, openai_inst_id, _, _, _) = setup_test_catalog();
    let factory = create_test_factory(catalog);

    let target_model = ModelId::new("gpt-5.6").unwrap();
    let model_ref = ModelRef::new(openai_inst_id, target_model);
    let prepared = factory
        .prepare_runtime_session(&model_ref)
        .expect("Failed to prepare OpenAI Responses session");

    assert_eq!(prepared.model_ref(), &model_ref);
    assert_eq!(prepared.provider_instance_id(), &openai_inst_id);
    assert_eq!(prepared.protocol(), &ProtocolFamily::OpenAiResponses);
    assert_eq!(prepared.selected_model(), "gpt-5.6");
    assert_eq!(
        prepared.model_provider_id(),
        deterministic_codex_provider_key(&openai_inst_id)
    );
    assert_eq!(
        prepared.available_models(),
        &["gpt-5.6".to_string(), "gpt-5.6-mini".to_string()]
    );

    // Native Responses bridge does not attach a custom inference backend
    let (provider, models_manager) = prepared.runtime_override().clone().into_parts();
    assert!(provider.inference_backend().is_none());

    // Verify static models manager returns the 2 models without network calls
    let model_info = models_manager
        .get_model_info("gpt-5.6", &ModelsManagerConfig::default())
        .await;
    assert_eq!(model_info.slug, "gpt-5.6");
    assert_eq!(model_info.display_name, "GPT 5.6");
    assert_eq!(model_info.context_window, Some(128_000));
    assert_eq!(model_info.max_context_window, Some(128_000));
    assert!(!model_info.used_fallback_model_metadata);

    // Test injection into StartThreadOptions
    let temp_dir = tempfile::tempdir().unwrap();
    let config = ConfigBuilder::default()
        .codex_home(temp_dir.path().to_path_buf())
        .build()
        .await
        .expect("build test config");
    let mut options = StartThreadOptions::new(config);
    prepared.prepare_start_thread_options(&mut options);

    assert!(options.model_runtime_override.is_some());
    assert_eq!(
        options.config.model_provider_id,
        deterministic_codex_provider_key(&openai_inst_id)
    );
    assert_eq!(options.config.model, Some("gpt-5.6".to_string()));
    assert!(!options.allow_provider_model_fallback);
}

#[tokio::test]
async fn test_factory_custom_protocol_anthropic() {
    let (catalog, _, anthropic_inst_id, _, _) = setup_test_catalog();
    let factory = create_test_factory(catalog);

    let model_id = ModelId::new("claude-3-7-sonnet").unwrap();
    let model_ref = ModelRef::new(anthropic_inst_id, model_id);
    let prepared = factory
        .prepare_runtime_session(&model_ref)
        .expect("Failed to prepare Anthropic session");

    assert_eq!(prepared.model_ref(), &model_ref);
    assert_eq!(prepared.provider_instance_id(), &anthropic_inst_id);
    assert_eq!(prepared.protocol(), &ProtocolFamily::AnthropicMessages);
    assert_eq!(prepared.selected_model(), "claude-3-7-sonnet");
    assert_eq!(
        prepared.available_models(),
        &["claude-3-7-sonnet".to_string()]
    );

    let (provider, models_manager) = prepared.runtime_override().clone().into_parts();

    // Custom protocols attach session-scoped RuntimeRouter as inference backend
    assert!(provider.inference_backend().is_some());

    // Custom backend forces RemoteCompactionSupport::Unsupported
    assert_eq!(
        provider.capabilities().remote_compaction,
        RemoteCompactionSupport::Unsupported
    );

    // Verify static models manager metadata: no synthetic reasoning tiers
    let info = models_manager
        .get_model_info("claude-3-7-sonnet", &ModelsManagerConfig::default())
        .await;
    assert_eq!(info.slug, "claude-3-7-sonnet");
    assert_eq!(info.context_window, Some(200_000));
    assert_eq!(info.max_context_window, Some(200_000));
    assert!(info.default_reasoning_level.is_none());
    assert!(info.supported_reasoning_levels.is_empty());
    assert_eq!(info.input_modalities, vec![InputModality::Text]);
    assert!(!info.supports_search_tool);
    assert!(info.include_skills_usage_instructions);
    assert!(info.include_plugin_usage_instructions);
    assert!(info.include_apps_usage_instructions);
}

#[tokio::test]
async fn test_factory_custom_protocol_gemini() {
    let (catalog, _, _, gemini_inst_id, _) = setup_test_catalog();
    let factory = create_test_factory(catalog);

    let model_id = ModelId::new("gemini-2.5-pro").unwrap();
    let model_ref = ModelRef::new(gemini_inst_id, model_id);
    let prepared = factory
        .prepare_runtime_session(&model_ref)
        .expect("Failed to prepare Gemini session");

    assert_eq!(prepared.selected_model(), "gemini-2.5-pro");
    assert_eq!(prepared.available_models(), &["gemini-2.5-pro".to_string()]);
    assert_eq!(prepared.protocol(), &ProtocolFamily::GeminiGenerateContent);

    let (provider, models_manager) = prepared.runtime_override().clone().into_parts();
    assert!(provider.inference_backend().is_some());
    assert_eq!(
        provider.capabilities().remote_compaction,
        RemoteCompactionSupport::Unsupported
    );

    let info = models_manager
        .get_model_info("gemini-2.5-pro", &ModelsManagerConfig::default())
        .await;
    assert_eq!(info.slug, "gemini-2.5-pro");
    assert_eq!(info.context_window, Some(1_000_000));
    assert_eq!(info.max_context_window, Some(1_000_000));
}

#[tokio::test]
async fn test_factory_custom_protocol_chat_completions() {
    let (catalog, _, _, _, chat_inst_id) = setup_test_catalog();
    let factory = create_test_factory(catalog);

    let model_id = ModelId::new("deepseek-v3").unwrap();
    let model_ref = ModelRef::new(chat_inst_id, model_id);
    let prepared = factory
        .prepare_runtime_session(&model_ref)
        .expect("Failed to prepare ChatCompletions session");

    assert_eq!(prepared.selected_model(), "deepseek-v3");
    assert_eq!(prepared.available_models(), &["deepseek-v3".to_string()]);
    assert_eq!(prepared.protocol(), &ProtocolFamily::OpenAiChatCompletions);

    let (provider, models_manager) = prepared.runtime_override().clone().into_parts();
    assert!(provider.inference_backend().is_some());
    assert_eq!(
        provider.capabilities().remote_compaction,
        RemoteCompactionSupport::Unsupported
    );

    let info = models_manager
        .get_model_info("deepseek-v3", &ModelsManagerConfig::default())
        .await;
    assert_eq!(info.slug, "deepseek-v3");
    assert_eq!(info.context_window, Some(64_000));
    assert_eq!(info.max_context_window, Some(64_000));
}

#[tokio::test]
async fn test_factory_zero_discovery() {
    let (catalog, openai_inst_id, _, _, _) = setup_test_catalog();
    let factory = create_test_factory(catalog);

    let model_ref = ModelRef::new(openai_inst_id, ModelId::new("gpt-5.6").unwrap());
    let prepared = factory
        .prepare_runtime_session(&model_ref)
        .expect("prepare session");

    let models_manager = prepared.runtime_override().models_manager();
    let http_client_factory = codex_http_client::HttpClientFactory::new(
        codex_http_client::OutboundProxyPolicy::ReqwestDefault,
    );

    // List models with OnlineIfUncached must NOT fail even without internet/mock server
    let models = models_manager
        .list_models(
            RefreshStrategy::OnlineIfUncached,
            http_client_factory.clone(),
        )
        .await;
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].model, "gpt-5.6");
    assert_eq!(models[1].model, "gpt-5.6-mini");

    let default_model = models_manager
        .get_default_model(
            &None,
            false,
            RefreshStrategy::OnlineIfUncached,
            http_client_factory,
        )
        .await;
    assert_eq!(default_model, "gpt-5.6");
}

#[tokio::test]
async fn test_factory_error_handling() {
    let (mut catalog, openai_inst_id, _, _, _) = setup_test_catalog();

    // Unknown instance
    let unknown_id = ProviderInstanceId::new();
    let factory = create_test_factory(catalog.clone());
    let bad_ref = ModelRef::new(unknown_id, ModelId::new("gpt-5.6").unwrap());
    let err = factory.prepare_runtime_session(&bad_ref).unwrap_err();
    assert!(matches!(err, RuntimeSessionError::ProviderInstanceNotFound(id) if id == unknown_id));

    // Disabled instance
    let disabled_inst_id = ProviderInstanceId::new();
    let mut disabled_inst = ProviderInstance::new(
        disabled_inst_id,
        ProviderId::new("openai").unwrap(),
        "Disabled Instance",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    disabled_inst.enabled = false;
    catalog.register_provider_instance(disabled_inst).unwrap();

    let factory = create_test_factory(catalog.clone());
    let disabled_ref = ModelRef::new(disabled_inst_id, ModelId::new("any-model").unwrap());
    let err = factory.prepare_runtime_session(&disabled_ref).unwrap_err();
    assert!(
        matches!(err, RuntimeSessionError::ProviderInstanceDisabled(id) if id == disabled_inst_id)
    );

    // Instance with no models
    let empty_inst_id = ProviderInstanceId::new();
    let empty_inst = ProviderInstance::new(
        empty_inst_id,
        ProviderId::new("openai").unwrap(),
        "Empty Instance",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(empty_inst).unwrap();

    let factory = create_test_factory(catalog.clone());
    let empty_ref = ModelRef::new(empty_inst_id, ModelId::new("any-model").unwrap());
    let err = factory.prepare_runtime_session(&empty_ref).unwrap_err();
    assert!(matches!(err, RuntimeSessionError::NoModelsForProvider(id) if id == empty_inst_id));

    // Model not found for instance
    let nonexistent_model = ModelId::new("nonexistent-model").unwrap();
    let nonexistent_ref = ModelRef::new(openai_inst_id, nonexistent_model.clone());
    let err = factory
        .prepare_runtime_session(&nonexistent_ref)
        .unwrap_err();
    assert!(matches!(
        err,
        RuntimeSessionError::ModelNotFound {
            model_id,
            provider_instance_id
        } if model_id == nonexistent_model && provider_instance_id == openai_inst_id
    ));
}

#[test]
fn test_factory_unsupported_custom_protocol() {
    let mut catalog = ProviderCatalog::new();
    let def_id = ProviderId::new("custom-def").unwrap();
    let def = ProviderDefinition::new(
        def_id.clone(),
        "Custom Def",
        vec![ProtocolFamily::Custom("ollama-raw".to_string())],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    let inst_id = ProviderInstanceId::new();
    let inst = ProviderInstance::new(
        inst_id,
        def_id,
        "Ollama Raw",
        ProtocolFamily::Custom("ollama-raw".to_string()),
        EndpointProfile::new("http://localhost:11434").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(inst).unwrap();

    let model_id = ModelId::new("llama3:8b").unwrap();
    let model = ModelDescriptor {
        id: model_id.clone(),
        provider_instance_id: inst_id,
        display_name: "Llama 3 8B".to_string(),
        capabilities: ModelCapabilities::default(),
        limits: ModelLimits::new(Some(8192), Some(2048)).unwrap(),
        metadata_source: ModelMetadataSource::Manual,
    };
    catalog.register_model(model).unwrap();

    let factory = create_test_factory(catalog);
    let model_ref = ModelRef::new(inst_id, model_id);
    let err = factory.prepare_runtime_session(&model_ref).unwrap_err();

    assert!(
        matches!(err, RuntimeSessionError::UnsupportedProtocol(ref name) if name == "ollama-raw")
    );
}

#[test]
fn test_model_info_unknown_context() {
    let descriptor = ModelDescriptor {
        id: ModelId::new("unknown-ctx-model").unwrap(),
        provider_instance_id: ProviderInstanceId::new(),
        display_name: "Unknown Context Model".to_string(),
        capabilities: ModelCapabilities::default(),
        limits: ModelLimits::new(None, None).unwrap(),
        metadata_source: ModelMetadataSource::Manual,
    };

    let info = model_descriptor_to_model_info(&descriptor).unwrap();
    assert_eq!(info.context_window, None);
    assert_eq!(info.max_context_window, None);
    assert!(!info.used_fallback_model_metadata);
}

#[test]
fn test_model_info_context_overflow() {
    let descriptor = ModelDescriptor {
        id: ModelId::new("overflow-model").unwrap(),
        provider_instance_id: ProviderInstanceId::new(),
        display_name: "Overflow Model".to_string(),
        capabilities: ModelCapabilities::default(),
        limits: ModelLimits::new(Some(u64::MAX), None).unwrap(),
        metadata_source: ModelMetadataSource::Manual,
    };

    let result = model_descriptor_to_model_info(&descriptor);
    assert!(
        matches!(result, Err(RuntimeSessionError::ModelMetadataOutOfRange(ref msg)) if msg.contains("exceeds i64 range"))
    );
}

#[test]
fn test_model_info_input_modalities() {
    let inst_id = ProviderInstanceId::new();

    // 1) Vision Supported, Audio Unsupported -> [Text, Image]
    let d1 = ModelDescriptor {
        id: ModelId::new("vision-only").unwrap(),
        provider_instance_id: inst_id,
        display_name: "Vision Only".to_string(),
        capabilities: ModelCapabilities {
            vision_input: CapabilitySupport::Supported,
            audio_input: CapabilitySupport::Unsupported,
            ..Default::default()
        },
        limits: ModelLimits::default(),
        metadata_source: ModelMetadataSource::Manual,
    };
    let i1 = model_descriptor_to_model_info(&d1).unwrap();
    assert_eq!(
        i1.input_modalities,
        vec![InputModality::Text, InputModality::Image]
    );

    // 2) Vision Unsupported, Audio Supported -> [Text, Audio]
    let d2 = ModelDescriptor {
        id: ModelId::new("audio-only").unwrap(),
        provider_instance_id: inst_id,
        display_name: "Audio Only".to_string(),
        capabilities: ModelCapabilities {
            vision_input: CapabilitySupport::Unsupported,
            audio_input: CapabilitySupport::Supported,
            ..Default::default()
        },
        limits: ModelLimits::default(),
        metadata_source: ModelMetadataSource::Manual,
    };
    let i2 = model_descriptor_to_model_info(&d2).unwrap();
    assert_eq!(
        i2.input_modalities,
        vec![InputModality::Text, InputModality::Audio]
    );

    // 3) Vision Supported, Audio Supported -> [Text, Image, Audio]
    let d3 = ModelDescriptor {
        id: ModelId::new("both").unwrap(),
        provider_instance_id: inst_id,
        display_name: "Both".to_string(),
        capabilities: ModelCapabilities {
            vision_input: CapabilitySupport::Supported,
            audio_input: CapabilitySupport::Supported,
            ..Default::default()
        },
        limits: ModelLimits::default(),
        metadata_source: ModelMetadataSource::Manual,
    };
    let i3 = model_descriptor_to_model_info(&d3).unwrap();
    assert_eq!(
        i3.input_modalities,
        vec![
            InputModality::Text,
            InputModality::Image,
            InputModality::Audio
        ]
    );

    // 4) Vision Unsupported, Audio Unsupported -> [Text]
    let d4 = ModelDescriptor {
        id: ModelId::new("text-only").unwrap(),
        provider_instance_id: inst_id,
        display_name: "Text Only".to_string(),
        capabilities: ModelCapabilities {
            vision_input: CapabilitySupport::Unsupported,
            audio_input: CapabilitySupport::Unsupported,
            ..Default::default()
        },
        limits: ModelLimits::default(),
        metadata_source: ModelMetadataSource::Manual,
    };
    let i4 = model_descriptor_to_model_info(&d4).unwrap();
    assert_eq!(i4.input_modalities, vec![InputModality::Text]);
}

#[test]
fn test_model_info_reasoning_no_synthetic_tiers() {
    let descriptor = ModelDescriptor {
        id: ModelId::new("reasoning-model").unwrap(),
        provider_instance_id: ProviderInstanceId::new(),
        display_name: "Reasoning Model".to_string(),
        capabilities: ModelCapabilities {
            reasoning: CapabilitySupport::Supported,
            ..Default::default()
        },
        limits: ModelLimits::default(),
        metadata_source: ModelMetadataSource::Manual,
    };

    let info = model_descriptor_to_model_info(&descriptor).unwrap();
    assert!(info.supported_reasoning_levels.is_empty());
    assert_eq!(info.default_reasoning_level, None);
}

#[test]
fn test_model_info_host_instructions_and_scrubbed_flags() {
    let descriptor = ModelDescriptor {
        id: ModelId::new("host-instruction-model").unwrap(),
        provider_instance_id: ProviderInstanceId::new(),
        display_name: "Host Instruction Model".to_string(),
        capabilities: ModelCapabilities::default(),
        limits: ModelLimits::default(),
        metadata_source: ModelMetadataSource::Manual,
    };

    let info = model_descriptor_to_model_info(&descriptor).unwrap();
    assert!(!info.used_fallback_model_metadata);
    assert!(!info.supports_search_tool);
    assert!(info.include_skills_usage_instructions);
    assert!(info.include_plugin_usage_instructions);
    assert!(info.include_apps_usage_instructions);
}

#[tokio::test]
async fn test_custom_provider_auth_manager_isolation() {
    let (catalog, _, anthropic_inst_id, _, _) = setup_test_catalog();

    // Create factory WITH an auth manager configured
    let secrets = InMemorySecretResolver::new().with_env_secret("ANTHROPIC_API_KEY", "sk-ant-test");
    let factory = AgentStudiosRuntimeSessionFactory::new(
        Arc::new(catalog),
        Arc::new(secrets),
        Arc::new(ContinuationManager::new()),
    );

    let model_id = ModelId::new("claude-3-7-sonnet").unwrap();
    let model_ref = ModelRef::new(anthropic_inst_id, model_id);
    let prepared = factory.prepare_runtime_session(&model_ref).unwrap();

    let (_, models_manager) = prepared.runtime_override().clone().into_parts();
    // StaticModelsManager for custom provider must have auth_manager = None
    assert!(models_manager.auth_manager().is_none());
}

#[test]
fn test_prepared_session_debug_redaction() {
    let (catalog, openai_inst_id, _, _, _) = setup_test_catalog();
    let factory = create_test_factory(catalog);

    let model_ref = ModelRef::new(openai_inst_id, ModelId::new("gpt-5.6").unwrap());
    let prepared = factory.prepare_runtime_session(&model_ref).unwrap();

    let debug_str = format!("{prepared:?}");
    assert!(debug_str.contains("PreparedRuntimeSession"));
    assert!(debug_str.contains("model_ref"));
    assert!(debug_str.contains("gpt-5.6"));
    assert!(debug_str.contains("OpenAiResponses"));
    assert!(debug_str.contains("model_provider_id"));
    assert!(debug_str.contains("available_models"));
    // Ensure no secrets leaked
    assert!(!debug_str.contains("sk-test-openai"));
}

#[test]
fn test_factory_session_isolation_and_no_collision() {
    let mut catalog = ProviderCatalog::new();
    let prov_def = ProviderDefinition::new(
        ProviderId::new("custom").unwrap(),
        "Custom",
        vec![ProtocolFamily::OpenAiChatCompletions],
    )
    .unwrap();
    catalog.register_provider_definition(prov_def).unwrap();

    let inst1_id = ProviderInstanceId::new();
    let inst2_id = ProviderInstanceId::new();

    let inst1 = ProviderInstance::new(
        inst1_id,
        ProviderId::new("custom").unwrap(),
        "Instance One",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("https://host1.example.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    let inst2 = ProviderInstance::new(
        inst2_id,
        ProviderId::new("custom").unwrap(),
        "Instance Two",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("https://host2.example.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(inst1).unwrap();
    catalog.register_provider_instance(inst2).unwrap();

    // Both instances register the same model name
    let m1 = ModelDescriptor {
        id: ModelId::new("shared-model-name").unwrap(),
        provider_instance_id: inst1_id,
        display_name: "Model from Inst 1".to_string(),
        capabilities: ModelCapabilities::default(),
        limits: ModelLimits::new(Some(32_000), Some(4_096)).unwrap(),
        metadata_source: ModelMetadataSource::Manual,
    };
    let m2 = ModelDescriptor {
        id: ModelId::new("shared-model-name").unwrap(),
        provider_instance_id: inst2_id,
        display_name: "Model from Inst 2".to_string(),
        capabilities: ModelCapabilities::default(),
        limits: ModelLimits::new(Some(32_000), Some(4_096)).unwrap(),
        metadata_source: ModelMetadataSource::Manual,
    };
    catalog.register_model(m1).unwrap();
    catalog.register_model(m2).unwrap();

    let factory = create_test_factory(catalog);

    let ref1 = ModelRef::new(inst1_id, ModelId::new("shared-model-name").unwrap());
    let ref2 = ModelRef::new(inst2_id, ModelId::new("shared-model-name").unwrap());

    let prep1 = factory.prepare_runtime_session(&ref1).unwrap();
    let prep2 = factory.prepare_runtime_session(&ref2).unwrap();

    // Verify completely different provider keys avoiding cross-session collisions
    assert_ne!(prep1.model_provider_id(), prep2.model_provider_id());
    assert_eq!(
        prep1.model_provider_id(),
        deterministic_codex_provider_key(&inst1_id)
    );
    assert_eq!(
        prep2.model_provider_id(),
        deterministic_codex_provider_key(&inst2_id)
    );
}
