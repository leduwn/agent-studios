use agent_studios_opencode_runtime::error::OpenCodeError;
use agent_studios_opencode_runtime::provider_mapping::{
    map_model_ref_to_opencode, resolve_opencode_provider_name,
};
use agent_studios_provider::id::{ModelId, ProviderId, ProviderInstanceId};
use agent_studios_provider::model::ModelRef;
use agent_studios_provider::{
    AuthenticationScheme, EndpointProfile, ProtocolFamily, ProviderCatalog, ProviderDefinition,
    ProviderInstance, SecretReference,
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
        resolve_opencode_provider_name(&ProviderId::new("anthropic").unwrap(), None),
        "anthropic"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("openai").unwrap(), None),
        "openai"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("google").unwrap(), None),
        "google"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("openrouter").unwrap(), None),
        "openrouter"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("ollama").unwrap(), None),
        "ollama"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("lmstudio").unwrap(), None),
        "lmstudio"
    );
}

#[test]
fn test_custom_provider_id_does_not_match_by_substring() {
    // Custom provider IDs must NOT match by substring heuristics
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("not-openai").unwrap(), None),
        "not-openai"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("openai-clone").unwrap(), None),
        "openai-clone"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("custom-codex").unwrap(), None),
        "custom-codex"
    );
    assert_eq!(
        resolve_opencode_provider_name(
            &ProviderId::new("my-not-openai-proxy").unwrap(),
            Some(&ProtocolFamily::Custom("custom-rpc".to_string()))
        ),
        "my-not-openai-proxy"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("claude-vpc").unwrap(), None),
        "claude-vpc"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("google-ai").unwrap(), None),
        "google-ai"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("openrouter-global").unwrap(), None),
        "openrouter-global"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("ollama-cluster").unwrap(), None),
        "ollama-cluster"
    );

    // Verify mapping via catalog for custom provider ID without standard protocol
    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let custom_def = ProviderDefinition::new(
        ProviderId::new("not-openai").unwrap(),
        "Custom Proxy",
        vec![ProtocolFamily::Custom("custom-wire".to_string())],
    )
    .unwrap();
    catalog.register_provider_definition(custom_def).unwrap();

    let instance_id = ProviderInstanceId::new();
    let custom_inst = ProviderInstance::new(
        instance_id,
        ProviderId::new("not-openai").unwrap(),
        "Custom Proxy",
        ProtocolFamily::Custom("custom-wire".to_string()),
        EndpointProfile::new("https://proxy.example.com").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(custom_inst).unwrap();

    let model_ref = ModelRef::new(instance_id, ModelId::new("custom-model").unwrap());
    let binding = map_model_ref_to_opencode(&catalog, &model_ref).unwrap();
    assert_eq!(
        binding.provider_name, "not-openai",
        "Custom provider without OpenAi protocol must not resolve to 'openai'"
    );
}

#[test]
fn test_protocol_family_resolves_opencode_provider_for_custom_ids() {
    assert_eq!(
        resolve_opencode_provider_name(
            &ProviderId::new("custom-gateway").unwrap(),
            Some(&ProtocolFamily::AnthropicMessages)
        ),
        "anthropic"
    );
    assert_eq!(
        resolve_opencode_provider_name(
            &ProviderId::new("corp-proxy").unwrap(),
            Some(&ProtocolFamily::OpenAiResponses)
        ),
        "openai"
    );
    assert_eq!(
        resolve_opencode_provider_name(
            &ProviderId::new("corp-proxy").unwrap(),
            Some(&ProtocolFamily::OpenAiChatCompletions)
        ),
        "openai"
    );
    assert_eq!(
        resolve_opencode_provider_name(
            &ProviderId::new("internal-vertex").unwrap(),
            Some(&ProtocolFamily::GeminiGenerateContent)
        ),
        "google"
    );
    assert_eq!(
        resolve_opencode_provider_name(
            &ProviderId::new("local-custom").unwrap(),
            Some(&ProtocolFamily::Custom("custom-rpc".to_string()))
        ),
        "local-custom"
    );

    // Verify end-to-end model mapping uses instance.protocol for custom provider IDs
    let mut catalog = ProviderCatalog::with_builtin_definitions();
    catalog
        .register_provider_definition(
            ProviderDefinition::new(
                ProviderId::new("my-custom-proxy").unwrap(),
                "Custom OpenAI Proxy",
                vec![ProtocolFamily::OpenAiChatCompletions],
            )
            .unwrap(),
        )
        .unwrap();
    catalog
        .register_provider_definition(
            ProviderDefinition::new(
                ProviderId::new("enterprise-claude-gateway").unwrap(),
                "Enterprise Claude Gateway",
                vec![ProtocolFamily::AnthropicMessages],
            )
            .unwrap(),
        )
        .unwrap();
    catalog
        .register_provider_definition(
            ProviderDefinition::new(
                ProviderId::new("vertex-internal").unwrap(),
                "Internal Vertex Bridge",
                vec![ProtocolFamily::GeminiGenerateContent],
            )
            .unwrap(),
        )
        .unwrap();

    let openai_proxy_id = ProviderInstanceId::new();
    let openai_proxy = ProviderInstance::new(
        openai_proxy_id,
        ProviderId::new("my-custom-proxy").unwrap(),
        "Custom OpenAI Proxy",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("https://openai-proxy.internal/v1").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("PROXY_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(openai_proxy).unwrap();

    let binding = map_model_ref_to_opencode(
        &catalog,
        &ModelRef::new(openai_proxy_id, ModelId::new("gpt-4o").unwrap()),
    )
    .unwrap();
    assert_eq!(binding.provider_name, "openai");

    let anthropic_proxy_id = ProviderInstanceId::new();
    let anthropic_proxy = ProviderInstance::new(
        anthropic_proxy_id,
        ProviderId::new("enterprise-claude-gateway").unwrap(),
        "Enterprise Claude Gateway",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new("https://anthropic-gateway.internal").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("GATEWAY_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(anthropic_proxy).unwrap();

    let binding = map_model_ref_to_opencode(
        &catalog,
        &ModelRef::new(
            anthropic_proxy_id,
            ModelId::new("claude-3-7-sonnet").unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(binding.provider_name, "anthropic");

    let gemini_bridge_id = ProviderInstanceId::new();
    let gemini_bridge = ProviderInstance::new(
        gemini_bridge_id,
        ProviderId::new("vertex-internal").unwrap(),
        "Internal Vertex Bridge",
        ProtocolFamily::GeminiGenerateContent,
        EndpointProfile::new("https://vertex-bridge.internal").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("VERTEX_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(gemini_bridge).unwrap();

    let binding = map_model_ref_to_opencode(
        &catalog,
        &ModelRef::new(gemini_bridge_id, ModelId::new("gemini-2.5-pro").unwrap()),
    )
    .unwrap();
    assert_eq!(binding.provider_name, "google");
}

#[test]
fn test_os_credential_store_and_none_do_not_fallback_to_ambient_env_vars() {
    let mut catalog = ProviderCatalog::with_builtin_definitions();

    // 1. OpenAI with OsCredentialStore -> auth_env_var must be None, NOT "OPENAI_API_KEY"
    let openai_os_id = ProviderInstanceId::new();
    let openai_os_inst = ProviderInstance::new(
        openai_os_id,
        ProviderId::new("openai").unwrap(),
        "OpenAI OS Store",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::os_store("agent-studios/openai/key").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(openai_os_inst).unwrap();

    let binding = map_model_ref_to_opencode(
        &catalog,
        &ModelRef::new(openai_os_id, ModelId::new("gpt-4o").unwrap()),
    )
    .unwrap();
    assert_eq!(
        binding.auth_env_var, None,
        "OsCredentialStore must never fall back to ambient OPENAI_API_KEY"
    );

    // 2. Anthropic with External -> auth_env_var must be None, NOT "ANTHROPIC_API_KEY"
    let anthropic_ext_id = ProviderInstanceId::new();
    let anthropic_ext_inst = ProviderInstance::new(
        anthropic_ext_id,
        ProviderId::new("anthropic").unwrap(),
        "Anthropic External Vault",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new("https://api.anthropic.com").unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: SecretReference::external("vault:anthropic_key").unwrap(),
        },
    )
    .unwrap();
    catalog
        .register_provider_instance(anthropic_ext_inst)
        .unwrap();

    let binding = map_model_ref_to_opencode(
        &catalog,
        &ModelRef::new(anthropic_ext_id, ModelId::new("claude-3-7-sonnet").unwrap()),
    )
    .unwrap();
    assert_eq!(
        binding.auth_env_var, None,
        "External secret backend must never fall back to ambient ANTHROPIC_API_KEY"
    );

    // 3. OpenAI with AuthenticationScheme::None -> auth_env_var must be None
    let openai_none_id = ProviderInstanceId::new();
    let openai_none_inst = ProviderInstance::new(
        openai_none_id,
        ProviderId::new("openai").unwrap(),
        "OpenAI Unauthenticated Proxy",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("http://localhost:8080/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog
        .register_provider_instance(openai_none_inst)
        .unwrap();

    let binding = map_model_ref_to_opencode(
        &catalog,
        &ModelRef::new(openai_none_id, ModelId::new("gpt-4o").unwrap()),
    )
    .unwrap();
    assert_eq!(
        binding.auth_env_var, None,
        "AuthenticationScheme::None must never fall back to ambient OPENAI_API_KEY"
    );

    // 4. Anthropic with AuthenticationScheme::None -> auth_env_var must be None
    let anthropic_none_id = ProviderInstanceId::new();
    let anthropic_none_inst = ProviderInstance::new(
        anthropic_none_id,
        ProviderId::new("anthropic").unwrap(),
        "Anthropic Unauthenticated Proxy",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new("http://localhost:8080/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog
        .register_provider_instance(anthropic_none_inst)
        .unwrap();

    let binding = map_model_ref_to_opencode(
        &catalog,
        &ModelRef::new(
            anthropic_none_id,
            ModelId::new("claude-3-7-sonnet").unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(
        binding.auth_env_var, None,
        "AuthenticationScheme::None must never fall back to ambient ANTHROPIC_API_KEY"
    );

    // 5. Google with OsCredentialStore -> auth_env_var must be None
    catalog
        .register_provider_definition(
            ProviderDefinition::new(
                ProviderId::new("google").unwrap(),
                "Google",
                vec![ProtocolFamily::GeminiGenerateContent],
            )
            .unwrap(),
        )
        .unwrap();

    let google_os_id = ProviderInstanceId::new();
    let google_os_inst = ProviderInstance::new(
        google_os_id,
        ProviderId::new("google").unwrap(),
        "Google OS Store",
        ProtocolFamily::GeminiGenerateContent,
        EndpointProfile::new("https://generativelanguage.googleapis.com").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::os_store("agent-studios/google/key").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(google_os_inst).unwrap();

    let binding = map_model_ref_to_opencode(
        &catalog,
        &ModelRef::new(google_os_id, ModelId::new("gemini-2.5-flash").unwrap()),
    )
    .unwrap();
    assert_eq!(
        binding.auth_env_var, None,
        "OsCredentialStore must never fall back to ambient GEMINI_API_KEY"
    );

    // 6. OpenRouter with OsCredentialStore -> auth_env_var must be None
    let openrouter_os_id = ProviderInstanceId::new();
    let openrouter_os_inst = ProviderInstance::new(
        openrouter_os_id,
        ProviderId::new("openrouter").unwrap(),
        "OpenRouter OS Store",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("https://openrouter.ai/api/v1").unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "authorization".to_string(),
            secret: SecretReference::os_store("agent-studios/openrouter/key").unwrap(),
        },
    )
    .unwrap();
    catalog
        .register_provider_instance(openrouter_os_inst)
        .unwrap();

    let binding = map_model_ref_to_opencode(
        &catalog,
        &ModelRef::new(
            openrouter_os_id,
            ModelId::new("anthropic/claude-3.7-sonnet").unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(
        binding.auth_env_var, None,
        "OsCredentialStore must never fall back to ambient OPENROUTER_API_KEY"
    );
}
