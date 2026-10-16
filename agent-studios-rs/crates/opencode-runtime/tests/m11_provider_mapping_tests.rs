use agent_studios_opencode_runtime::error::OpenCodeError;
use agent_studios_opencode_runtime::provider_mapping::{
    map_model_ref_to_opencode, resolve_opencode_provider_name,
};
use agent_studios_provider::id::{ModelId, ProviderId, ProviderInstanceId};
use agent_studios_provider::model::ModelRef;
use agent_studios_provider::{
    AuthenticationScheme, EndpointProfile, ProtocolFamily, ProviderCatalog, ProviderInstance,
    SecretReference,
};

#[test]
fn test_m11_r07_provider_mapping_standard_providers() {
    let mut catalog = ProviderCatalog::with_builtin_definitions();

    // Anthropic
    let anthropic_id = ProviderInstanceId::new();
    let anthropic_inst = ProviderInstance::new(
        anthropic_id,
        ProviderId::new("anthropic").unwrap(),
        "Anthropic Main",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new("https://api.anthropic.com").unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: SecretReference::env("ANTHROPIC_API_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(anthropic_inst).unwrap();

    let model_ref = ModelRef::new(anthropic_id, ModelId::new("claude-3-7-sonnet").unwrap());
    let binding = map_model_ref_to_opencode(&catalog, &model_ref).unwrap();
    assert_eq!(binding.provider_name, "anthropic");
    assert_eq!(binding.model_name, "claude-3-7-sonnet");
    assert_eq!(binding.auth_env_var, Some("ANTHROPIC_API_KEY".to_string()));

    // OpenAI
    let openai_id = ProviderInstanceId::new();
    let openai_inst = ProviderInstance::new(
        openai_id,
        ProviderId::new("openai").unwrap(),
        "OpenAI Main",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("OPENAI_API_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(openai_inst).unwrap();

    let model_ref = ModelRef::new(openai_id, ModelId::new("gpt-4o").unwrap());
    let binding = map_model_ref_to_opencode(&catalog, &model_ref).unwrap();
    assert_eq!(binding.provider_name, "openai");
    assert_eq!(binding.model_name, "gpt-4o");
    assert_eq!(binding.auth_env_var, Some("OPENAI_API_KEY".to_string()));

    // Ollama (local, no auth)
    let ollama_id = ProviderInstanceId::new();
    let ollama_inst = ProviderInstance::new(
        ollama_id,
        ProviderId::new("ollama").unwrap(),
        "Ollama Local",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("http://127.0.0.1:11434").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(ollama_inst).unwrap();

    let model_ref = ModelRef::new(ollama_id, ModelId::new("llama3:8b").unwrap());
    let binding = map_model_ref_to_opencode(&catalog, &model_ref).unwrap();
    assert_eq!(binding.provider_name, "ollama");
    assert_eq!(binding.model_name, "llama3:8b");
    assert_eq!(binding.auth_env_var, None);
}

#[test]
fn test_m11_r07_custom_secret_env_var_override() {
    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let instance_id = ProviderInstanceId::new();
    let custom_inst = ProviderInstance::new(
        instance_id,
        ProviderId::new("anthropic").unwrap(),
        "Anthropic Enterprise",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new("https://proxy.corp.internal/v1").unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: SecretReference::env("CUSTOM_ANTHROPIC_KEY_FOR_TEAM").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(custom_inst).unwrap();

    let model_ref = ModelRef::new(instance_id, ModelId::new("claude-3-5-sonnet").unwrap());
    let binding = map_model_ref_to_opencode(&catalog, &model_ref).unwrap();
    assert_eq!(binding.provider_name, "anthropic");
    assert_eq!(
        binding.auth_env_var,
        Some("CUSTOM_ANTHROPIC_KEY_FOR_TEAM".to_string())
    );
    assert_eq!(
        binding.base_url,
        Some("https://proxy.corp.internal/v1".to_string())
    );
}

#[test]
fn test_m11_r07_unregistered_instance_strictly_fails_closed() {
    let catalog = ProviderCatalog::with_builtin_definitions();
    let random_id = ProviderInstanceId::new();
    let model_ref = ModelRef::new(random_id, ModelId::new("claude-3-7-sonnet").unwrap());

    let res = map_model_ref_to_opencode(&catalog, &model_ref);
    assert!(
        res.is_err(),
        "Must strictly fail closed when provider instance is not registered in catalog"
    );
    match res.unwrap_err() {
        OpenCodeError::InvalidConfiguration(msg) => {
            assert!(msg.contains("not found in catalog"));
        }
        other => panic!("Expected InvalidConfiguration error, got {other:?}"),
    }
}

#[test]
fn test_m11_r07_provider_name_resolution_variants() {
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("anthropic").unwrap()),
        "anthropic"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("claude-vpc").unwrap()),
        "anthropic"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("openai-codex").unwrap()),
        "openai"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("google-ai").unwrap()),
        "google"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("openrouter-global").unwrap()),
        "openrouter"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("ollama-cluster").unwrap()),
        "ollama"
    );
}
