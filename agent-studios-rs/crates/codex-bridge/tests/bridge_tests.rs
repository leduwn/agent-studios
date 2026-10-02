use std::collections::BTreeMap;

use agent_studios_codex_bridge::{
    CodexBridgeError, CodexBridgeOptions, CodexProviderBridge, deterministic_codex_provider_key,
    resolve_catalog_url,
};
use agent_studios_provider::{
    AuthenticationScheme, CapabilitySupport, EndpointProfile, ModelCapabilities, ModelDescriptor,
    ModelId, ModelLimits, ModelRef, ProtocolFamily, ProviderCatalog, ProviderId, ProviderInstance,
    ProviderInstanceId, SecretBackend, SecretReference,
};
use codex_model_provider_info::WireApi;

fn create_test_catalog() -> ProviderCatalog {
    ProviderCatalog::with_builtin_definitions()
}

fn create_responses_instance(
    instance_id: ProviderInstanceId,
    display_name: &str,
    base_url: &str,
    auth: AuthenticationScheme,
) -> ProviderInstance {
    let mut instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        display_name,
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new(base_url).unwrap(),
        auth,
    )
    .unwrap();
    instance.enabled = true;
    instance
}

fn create_test_model(
    instance_id: ProviderInstanceId,
    model_id_str: &str,
    tool_calling: CapabilitySupport,
    streaming: CapabilitySupport,
) -> ModelDescriptor {
    let caps = ModelCapabilities {
        tool_calling,
        streaming,
        ..Default::default()
    };

    ModelDescriptor::new(
        instance_id,
        ModelId::new(model_id_str).unwrap(),
        format!("Model {model_id_str}"),
        caps,
        ModelLimits::default(),
    )
    .unwrap()
}

#[test]
fn test_multi_instance_coexistence_and_deterministic_keying() {
    let mut catalog = create_test_catalog();

    let instance_id_1 = ProviderInstanceId::new();
    let instance_id_2 = ProviderInstanceId::new();

    let instance_1 = create_responses_instance(
        instance_id_1,
        "9Router Local",
        "http://localhost:8080/v1",
        AuthenticationScheme::BearerToken {
            secret: SecretReference::new(
                SecretBackend::EnvironmentVariable,
                "LOCAL_NINEROUTER_API_KEY",
            )
            .unwrap(),
        },
    );

    let instance_2 = create_responses_instance(
        instance_id_2,
        "9Router VPS",
        "https://vps.example.com/v1",
        AuthenticationScheme::BearerToken {
            secret: SecretReference::new(
                SecretBackend::EnvironmentVariable,
                "VPS_NINEROUTER_API_KEY",
            )
            .unwrap(),
        },
    );

    catalog.register_provider_instance(instance_1).unwrap();
    catalog.register_provider_instance(instance_2).unwrap();

    // Register identical model IDs under both instances
    let model_1 = create_test_model(
        instance_id_1,
        "gpt-4o",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let model_2 = create_test_model(
        instance_id_2,
        "gpt-4o",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );

    let ref_1 = model_1.model_ref();
    let ref_2 = model_2.model_ref();

    catalog.register_model(model_1).unwrap();
    catalog.register_model(model_2).unwrap();

    let binding_1 =
        CodexProviderBridge::resolve(&catalog, &ref_1, CodexBridgeOptions::default()).unwrap();
    let binding_2 =
        CodexProviderBridge::resolve(&catalog, &ref_2, CodexBridgeOptions::default()).unwrap();

    assert_eq!(
        binding_1.codex_provider_key,
        deterministic_codex_provider_key(&instance_id_1)
    );
    assert_eq!(
        binding_2.codex_provider_key,
        deterministic_codex_provider_key(&instance_id_2)
    );
    assert_ne!(binding_1.codex_provider_key, binding_2.codex_provider_key);

    assert_eq!(
        binding_1.provider_info.base_url.as_deref(),
        Some("http://localhost:8080/v1")
    );
    assert_eq!(
        binding_2.provider_info.base_url.as_deref(),
        Some("https://vps.example.com/v1")
    );
    assert_eq!(
        binding_1.provider_info.env_key.as_deref(),
        Some("LOCAL_NINEROUTER_API_KEY")
    );
    assert_eq!(
        binding_2.provider_info.env_key.as_deref(),
        Some("VPS_NINEROUTER_API_KEY")
    );
    assert_eq!(binding_1.provider_info.wire_api, WireApi::Responses);
    assert_eq!(binding_2.provider_info.wire_api, WireApi::Responses);
}

#[test]
fn test_bearer_auth_mapping() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let instance = create_responses_instance(
        instance_id,
        "OpenAI Custom",
        "https://api.openai.com/v1",
        AuthenticationScheme::BearerToken {
            secret: SecretReference::new(SecretBackend::EnvironmentVariable, "MY_OPENAI_KEY")
                .unwrap(),
        },
    );
    catalog.register_provider_instance(instance).unwrap();

    let model = create_test_model(
        instance_id,
        "gpt-4o-mini",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let mref = model.model_ref();
    catalog.register_model(model).unwrap();

    let binding =
        CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap();

    assert_eq!(
        binding.provider_info.env_key.as_deref(),
        Some("MY_OPENAI_KEY")
    );
    assert!(binding.provider_info.env_http_headers.is_none());
    assert!(binding.provider_info.experimental_bearer_token.is_none());
}

#[test]
fn test_oauth_auth_mapping() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let instance = create_responses_instance(
        instance_id,
        "Enterprise Gateway",
        "https://gateway.corp.com/v1",
        AuthenticationScheme::OAuthToken {
            secret: SecretReference::new(SecretBackend::EnvironmentVariable, "CORP_OAUTH_TOKEN")
                .unwrap(),
        },
    );
    catalog.register_provider_instance(instance).unwrap();

    let model = create_test_model(
        instance_id,
        "corp-llm",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let mref = model.model_ref();
    catalog.register_model(model).unwrap();

    let binding =
        CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap();

    assert_eq!(
        binding.provider_info.env_key.as_deref(),
        Some("CORP_OAUTH_TOKEN")
    );
    assert!(binding.provider_info.env_http_headers.is_none());
}

#[test]
fn test_api_key_header_mapping() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let instance = create_responses_instance(
        instance_id,
        "Header Auth Provider",
        "https://api.custom.com/v1",
        AuthenticationScheme::ApiKeyHeader {
            header_name: "X-Custom-Key".to_string(),
            secret: SecretReference::new(SecretBackend::EnvironmentVariable, "CUSTOM_API_KEY")
                .unwrap(),
        },
    );
    catalog.register_provider_instance(instance).unwrap();

    let model = create_test_model(
        instance_id,
        "custom-model",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let mref = model.model_ref();
    catalog.register_model(model).unwrap();

    let binding =
        CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap();

    assert!(binding.provider_info.env_key.is_none());
    let env_headers = binding.provider_info.env_http_headers.unwrap();
    assert_eq!(
        env_headers.get("X-Custom-Key").map(|s| s.as_str()),
        Some("CUSTOM_API_KEY")
    );
}

#[test]
fn test_no_auth_mapping() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let instance = create_responses_instance(
        instance_id,
        "Local Ollama",
        "http://localhost:11434/v1",
        AuthenticationScheme::None,
    );
    catalog.register_provider_instance(instance).unwrap();

    let model = create_test_model(
        instance_id,
        "llama3.3",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let mref = model.model_ref();
    catalog.register_model(model).unwrap();

    let binding =
        CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap();

    assert!(binding.provider_info.env_key.is_none());
    assert!(binding.provider_info.env_http_headers.is_none());
}

#[test]
fn test_query_parameter_auth_rejected() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let instance = create_responses_instance(
        instance_id,
        "Query Param Auth",
        "https://api.example.com/v1",
        AuthenticationScheme::QueryParameter {
            parameter_name: "api_key".to_string(),
            secret: SecretReference::new(SecretBackend::EnvironmentVariable, "KEY_VAR").unwrap(),
        },
    );
    catalog.register_provider_instance(instance).unwrap();

    let model = create_test_model(
        instance_id,
        "model-1",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let mref = model.model_ref();
    catalog.register_model(model).unwrap();

    let err =
        CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap_err();
    assert_eq!(err, CodexBridgeError::SecretQueryParameterAuthUnsupported);
}

#[test]
fn test_unsupported_secret_backend_rejected() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let instance = create_responses_instance(
        instance_id,
        "OS Keychain Provider",
        "https://api.example.com/v1",
        AuthenticationScheme::BearerToken {
            secret: SecretReference::new(SecretBackend::OsCredentialStore, "keychain_account")
                .unwrap(),
        },
    );
    catalog.register_provider_instance(instance).unwrap();

    let model = create_test_model(
        instance_id,
        "model-1",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let mref = model.model_ref();
    catalog.register_model(model).unwrap();

    let err =
        CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap_err();
    assert!(matches!(
        err,
        CodexBridgeError::UnsupportedSecretBackend {
            backend: SecretBackend::OsCredentialStore,
            ..
        }
    ));
}

#[test]
fn test_static_headers_and_query_params_mapping() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let mut headers = BTreeMap::new();
    headers.insert(
        "OpenAI-Organization".to_string(),
        "org-test-123".to_string(),
    );
    headers.insert("X-Project-Id".to_string(), "proj-456".to_string());

    let mut query_params = BTreeMap::new();
    query_params.insert("api-version".to_string(), "2024-02-15-preview".to_string());

    let endpoint = EndpointProfile {
        base_url: "https://api.openai.com/v1".to_string(),
        model_catalog_url: None,
        static_headers: headers,
        query_params,
    };

    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "Configured Instance",
        ProtocolFamily::OpenAiResponses,
        endpoint,
        AuthenticationScheme::BearerToken {
            secret: SecretReference::new(SecretBackend::EnvironmentVariable, "KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(instance).unwrap();

    let model = create_test_model(
        instance_id,
        "gpt-4o",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let mref = model.model_ref();
    catalog.register_model(model).unwrap();

    let binding =
        CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap();

    let http_headers = binding.provider_info.http_headers.unwrap();
    assert_eq!(
        http_headers.get("OpenAI-Organization").map(|s| s.as_ref()),
        Some("org-test-123")
    );
    assert_eq!(
        http_headers.get("X-Project-Id").map(|s| s.as_ref()),
        Some("proj-456")
    );

    let qp = binding.provider_info.query_params.unwrap();
    assert_eq!(
        qp.get("api-version").map(|s| s.as_ref()),
        Some("2024-02-15-preview")
    );
}

#[test]
fn test_forbidden_static_headers_rejected() {
    let forbidden_names = [
        "Authorization",
        "authorization",
        "Proxy-Authorization",
        "X-Api-Key",
        "api-key",
        "x-auth-token",
        "bearer",
    ];

    for name in forbidden_names {
        let instance_id = ProviderInstanceId::new();
        let mut headers = BTreeMap::new();
        headers.insert(name.to_string(), "leaked-token".to_string());

        let endpoint = EndpointProfile {
            base_url: "https://api.openai.com/v1".to_string(),
            model_catalog_url: None,
            static_headers: headers,
            query_params: BTreeMap::new(),
        };

        // endpoint.validate() or bridge validation catches this
        let res = ProviderInstance::new(
            instance_id,
            ProviderId::new("openai").unwrap(),
            "Instance",
            ProtocolFamily::OpenAiResponses,
            endpoint,
            AuthenticationScheme::None,
        );
        assert!(res.is_err(), "Header '{name}' should be rejected");
    }
}

#[test]
fn test_invalid_header_name_rejected() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let instance = create_responses_instance(
        instance_id,
        "Header Name Test",
        "https://api.example.com/v1",
        AuthenticationScheme::ApiKeyHeader {
            header_name: "Bad Header:WithColon".to_string(),
            secret: SecretReference::new(SecretBackend::EnvironmentVariable, "KEY").unwrap(),
        },
    );
    catalog.register_provider_instance(instance).unwrap();

    let model = create_test_model(
        instance_id,
        "model-1",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let mref = model.model_ref();
    catalog.register_model(model).unwrap();

    let err =
        CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap_err();
    assert!(matches!(err, CodexBridgeError::InvalidHeaderName { .. }));
}

#[test]
fn test_catalog_url_resolution() {
    // Absolute http(s) URL preserved
    let abs = resolve_catalog_url(
        "https://api.openai.com/v1",
        "https://catalog.openai.com/models",
    )
    .unwrap();
    assert_eq!(abs, "https://catalog.openai.com/models");

    // Relative path joined with base_url
    let rel = resolve_catalog_url("https://api.openai.com/v1", "models").unwrap();
    assert_eq!(rel, "https://api.openai.com/v1/models");

    // Absolute root path joined with base origin
    let root = resolve_catalog_url("https://api.openai.com/v1", "/models").unwrap();
    assert_eq!(root, "https://api.openai.com/models");

    // Rejection of credentials in catalog URL
    let cred_err = resolve_catalog_url(
        "https://api.openai.com/v1",
        "https://user:pass@example.com/models",
    )
    .unwrap_err();
    assert!(matches!(
        cred_err,
        CodexBridgeError::InvalidCatalogUrl { .. }
    ));
}

#[test]
fn test_unsupported_protocol_rejected() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();

    let mut instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("anthropic").unwrap(),
        "Anthropic Instance",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new("https://api.anthropic.com/v1").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::new(SecretBackend::EnvironmentVariable, "ANTHROPIC_KEY")
                .unwrap(),
        },
    )
    .unwrap();
    instance.enabled = true;
    catalog.register_provider_instance(instance).unwrap();

    let model = create_test_model(
        instance_id,
        "claude-3-5-sonnet",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let mref = model.model_ref();
    catalog.register_model(model).unwrap();

    let err =
        CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap_err();
    assert_eq!(
        err,
        CodexBridgeError::UnsupportedProtocol {
            protocol: ProtocolFamily::AnthropicMessages,
            expected: ProtocolFamily::OpenAiResponses,
        }
    );
}

#[test]
fn test_capability_safety_explicit_unsupported_rejected() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let instance = create_responses_instance(
        instance_id,
        "Test Instance",
        "https://api.example.com/v1",
        AuthenticationScheme::None,
    );
    catalog.register_provider_instance(instance).unwrap();

    // Model with tool_calling explicitly unsupported
    let no_tools = create_test_model(
        instance_id,
        "no-tools-model",
        CapabilitySupport::Unsupported,
        CapabilitySupport::Supported,
    );
    let ref_no_tools = no_tools.model_ref();
    catalog.register_model(no_tools).unwrap();

    let err1 = CodexProviderBridge::resolve(&catalog, &ref_no_tools, CodexBridgeOptions::default())
        .unwrap_err();
    assert!(matches!(
        err1,
        CodexBridgeError::UnsupportedModelCapability {
            capability: "tool_calling",
            ..
        }
    ));

    // Model with streaming explicitly unsupported
    let no_stream = create_test_model(
        instance_id,
        "no-stream-model",
        CapabilitySupport::Supported,
        CapabilitySupport::Unsupported,
    );
    let ref_no_stream = no_stream.model_ref();
    catalog.register_model(no_stream).unwrap();

    let err2 =
        CodexProviderBridge::resolve(&catalog, &ref_no_stream, CodexBridgeOptions::default())
            .unwrap_err();
    assert!(matches!(
        err2,
        CodexBridgeError::UnsupportedModelCapability {
            capability: "streaming",
            ..
        }
    ));
}

#[test]
fn test_capability_unknown_tracked_in_compatibility_report() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let instance = create_responses_instance(
        instance_id,
        "Test Instance",
        "https://api.example.com/v1",
        AuthenticationScheme::None,
    );
    catalog.register_provider_instance(instance).unwrap();

    let unknown_model = create_test_model(
        instance_id,
        "unknown-caps-model",
        CapabilitySupport::Unknown,
        CapabilitySupport::Unknown,
    );
    let mref = unknown_model.model_ref();
    catalog.register_model(unknown_model).unwrap();

    let binding =
        CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap();

    assert!(!binding.compatibility_report.is_fully_verified());
    assert!(
        binding
            .compatibility_report
            .unverified_capabilities
            .contains(&"tool_calling".to_string())
    );
    assert!(
        binding
            .compatibility_report
            .unverified_capabilities
            .contains(&"streaming".to_string())
    );
}

#[test]
fn test_disabled_provider_instance_rejected() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let mut instance = create_responses_instance(
        instance_id,
        "Disabled Instance",
        "https://api.example.com/v1",
        AuthenticationScheme::None,
    );
    instance.enabled = false;
    catalog.register_provider_instance(instance).unwrap();

    let model = create_test_model(
        instance_id,
        "model-1",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let mref = model.model_ref();
    catalog.register_model(model).unwrap();

    let err =
        CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap_err();
    assert_eq!(err, CodexBridgeError::ProviderInstanceDisabled(instance_id));
}

#[test]
fn test_unknown_model_rejected() {
    let catalog = create_test_catalog();
    let non_existent_ref = ModelRef {
        provider_instance_id: ProviderInstanceId::new(),
        model_id: ModelId::new("does-not-exist").unwrap(),
    };

    let err =
        CodexProviderBridge::resolve(&catalog, &non_existent_ref, CodexBridgeOptions::default())
            .unwrap_err();
    assert_eq!(err, CodexBridgeError::UnknownModel(non_existent_ref));
}

#[test]
fn test_bridge_options_propagation() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let instance = create_responses_instance(
        instance_id,
        "Options Instance",
        "https://api.example.com/v1",
        AuthenticationScheme::None,
    );
    catalog.register_provider_instance(instance).unwrap();

    let model = create_test_model(
        instance_id,
        "model-1",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let mref = model.model_ref();
    catalog.register_model(model).unwrap();

    let opts = CodexBridgeOptions {
        supports_websockets: true,
        supports_standalone_web_search: true,
        include_internal_metadata: true,
    };

    let binding = CodexProviderBridge::resolve(&catalog, &mref, opts).unwrap();

    assert!(binding.provider_info.supports_websockets);
    assert!(binding.provider_info.supports_standalone_web_search);
    assert!(binding.provider_info.include_internal_metadata);
}

#[test]
fn test_brand_independence() {
    let mut catalog = create_test_catalog();

    let brands = [
        "9Router High-Speed Proxy",
        "Local vLLM Cluster",
        "Company Private Gateway",
        "Self-Hosted TGI",
    ];

    for (i, brand_name) in brands.iter().enumerate() {
        let instance_id = ProviderInstanceId::new();
        let instance = create_responses_instance(
            instance_id,
            brand_name,
            &format!("http://10.0.0.{}:8000/v1", i + 1),
            AuthenticationScheme::None,
        );
        catalog.register_provider_instance(instance).unwrap();

        let model = create_test_model(
            instance_id,
            "qwen-2.5-coder",
            CapabilitySupport::Supported,
            CapabilitySupport::Supported,
        );
        let mref = model.model_ref();
        catalog.register_model(model).unwrap();

        let binding =
            CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap();
        assert_eq!(binding.provider_info.name, *brand_name);
        assert_eq!(
            binding.codex_provider_key,
            format!("agent-studios-{}", instance_id)
        );
        assert_eq!(binding.provider_info.wire_api, WireApi::Responses);
    }
}

#[test]
fn test_fully_verified_capabilities_report() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let instance = create_responses_instance(
        instance_id,
        "Fully Capable Instance",
        "https://api.openai.com/v1",
        AuthenticationScheme::None,
    );
    catalog.register_provider_instance(instance).unwrap();

    let model = create_test_model(
        instance_id,
        "gpt-4o",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let mref = model.model_ref();
    catalog.register_model(model).unwrap();

    let binding =
        CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap();

    assert!(binding.compatibility_report.is_fully_verified());
    assert!(
        binding
            .compatibility_report
            .unverified_capabilities
            .is_empty()
    );
}

#[test]
fn test_aws_sigv4_rejected_with_unsupported_auth() {
    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        ProviderId::new("openai").unwrap(),
        "AWS Bedrock like",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://bedrock-mantle.us-east-1.api.aws/openai/v1").unwrap(),
        AuthenticationScheme::AwsSigV4 {
            credential: SecretReference::new(
                SecretBackend::EnvironmentVariable,
                "AWS_SECRET_ACCESS_KEY",
            )
            .unwrap(),
            region: "us-east-1".to_string(),
            service: "bedrock".to_string(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(instance).unwrap();

    let model = create_test_model(
        instance_id,
        "bedrock-model",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let mref = model.model_ref();
    catalog.register_model(model).unwrap();

    let err =
        CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap_err();
    assert!(matches!(
        err,
        CodexBridgeError::UnsupportedAuthentication(..)
    ));
}

#[test]
fn test_zero_raw_credential_resolution_invariance() {
    // Set a dummy env var in the process
    let env_var_name = "AGENT_STUDIOS_TEST_SECRET_DO_NOT_RESOLVE";
    let secret_val = "SUPER_SECRET_PLAINTEXT_API_KEY_12345";
    unsafe {
        std::env::set_var(env_var_name, secret_val);
    }

    let mut catalog = create_test_catalog();
    let instance_id = ProviderInstanceId::new();
    let instance = create_responses_instance(
        instance_id,
        "Secret Safety Test",
        "https://api.openai.com/v1",
        AuthenticationScheme::BearerToken {
            secret: SecretReference::new(SecretBackend::EnvironmentVariable, env_var_name).unwrap(),
        },
    );
    catalog.register_provider_instance(instance).unwrap();

    let model = create_test_model(
        instance_id,
        "gpt-4o",
        CapabilitySupport::Supported,
        CapabilitySupport::Supported,
    );
    let mref = model.model_ref();
    catalog.register_model(model).unwrap();

    let binding =
        CodexProviderBridge::resolve(&catalog, &mref, CodexBridgeOptions::default()).unwrap();

    // The bridge must contain the reference name, NOT the secret value
    assert_eq!(binding.provider_info.env_key.as_deref(), Some(env_var_name));
    assert_ne!(binding.provider_info.env_key.as_deref(), Some(secret_val));

    // Debug output must not contain the secret value
    let debug_repr = format!("{:?}", binding);
    assert!(!debug_repr.contains(secret_val));
    assert!(debug_repr.contains(env_var_name));

    // Cleanup env
    unsafe {
        std::env::remove_var(env_var_name);
    }
}
