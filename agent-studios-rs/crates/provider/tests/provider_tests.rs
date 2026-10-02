use std::str::FromStr;

use agent_studios_provider::{
    AuthenticationScheme, CapabilitySupport, EndpointProfile, ModelCapabilities, ModelDescriptor,
    ModelId, ModelLimits, ModelMetadataSource, ModelRef, PROVIDER_CATALOG_SCHEMA_VERSION,
    ProtocolFamily, ProviderCatalog, ProviderCatalogSnapshot, ProviderDefinition, ProviderError,
    ProviderId, ProviderInstance, ProviderInstanceId, SecretBackend, SecretReference,
    builtin_definitions,
};

#[test]
fn test_provider_id_validation_rules() {
    assert!(ProviderId::new("openai").is_ok());
    assert!(ProviderId::new("custom-v1.0").is_ok());
    assert!(ProviderId::new("9router_local").is_ok());

    assert_eq!(
        ProviderId::new("").unwrap_err(),
        ProviderError::InvalidProviderId("ProviderId cannot be empty".to_string())
    );
    assert_eq!(
        ProviderId::new("   ").unwrap_err(),
        ProviderError::InvalidProviderId("ProviderId cannot be empty".to_string())
    );
    assert!(matches!(
        ProviderId::new("has space").unwrap_err(),
        ProviderError::InvalidProviderId(_)
    ));
    assert!(matches!(
        ProviderId::new("has/slash").unwrap_err(),
        ProviderError::InvalidProviderId(_)
    ));
}

#[test]
fn test_model_id_validation_rules() {
    assert!(ModelId::new("gpt-5.6").is_ok());
    assert!(ModelId::new("claude-sonnet-4-5:latest").is_ok());
    assert!(ModelId::new("meta/llama-3.3-70b-instruct").is_ok());

    assert_eq!(
        ModelId::new("").unwrap_err(),
        ProviderError::InvalidModelId("ModelId cannot be empty".to_string())
    );
    assert_eq!(
        ModelId::new("   ").unwrap_err(),
        ProviderError::InvalidModelId("ModelId cannot be empty".to_string())
    );
}

#[test]
fn test_provider_instance_id_uniqueness_and_parse() {
    let id1 = ProviderInstanceId::new();
    let id2 = ProviderInstanceId::new();
    assert_ne!(id1, id2);

    let parsed = ProviderInstanceId::from_str(&id1.to_string()).unwrap();
    assert_eq!(id1, parsed);

    assert!(ProviderInstanceId::from_str("not-a-uuid").is_err());
}

#[test]
fn test_protocol_family_variants_and_custom_validation() {
    let protos = vec![
        ProtocolFamily::OpenAiResponses,
        ProtocolFamily::OpenAiChatCompletions,
        ProtocolFamily::AnthropicMessages,
        ProtocolFamily::GeminiGenerateContent,
        ProtocolFamily::Custom("grpc-custom".to_string()),
    ];

    for p in protos {
        assert!(p.validate().is_ok());
    }

    let empty_custom = ProtocolFamily::Custom("   ".to_string());
    assert!(empty_custom.validate().is_err());
}

#[test]
fn test_secret_reference_backends() {
    let env_ref = SecretReference::env("API_KEY_VAR").unwrap();
    assert_eq!(env_ref.backend, SecretBackend::EnvironmentVariable);
    assert_eq!(env_ref.locator, "API_KEY_VAR");

    let os_ref = SecretReference::os_store("agent-studios/creds/123").unwrap();
    assert_eq!(os_ref.backend, SecretBackend::OsCredentialStore);

    let ext_ref = SecretReference::external("vault:secret/data/llm").unwrap();
    assert_eq!(ext_ref.backend, SecretBackend::External);

    assert!(SecretReference::env("  ").is_err());
}

#[test]
fn test_authentication_scheme_all_variants_and_validation() {
    let sec = SecretReference::env("TEST_KEY").unwrap();

    let none = AuthenticationScheme::None;
    assert!(none.validate().is_ok());

    let bearer = AuthenticationScheme::BearerToken {
        secret: sec.clone(),
    };
    assert!(bearer.validate().is_ok());

    let api_key = AuthenticationScheme::ApiKeyHeader {
        header_name: "x-api-key".to_string(),
        secret: sec.clone(),
    };
    assert!(api_key.validate().is_ok());

    let invalid_api_key = AuthenticationScheme::ApiKeyHeader {
        header_name: "".to_string(),
        secret: sec.clone(),
    };
    assert!(invalid_api_key.validate().is_err());

    let qparam = AuthenticationScheme::QueryParameter {
        parameter_name: "key".to_string(),
        secret: sec.clone(),
    };
    assert!(qparam.validate().is_ok());

    let invalid_qparam = AuthenticationScheme::QueryParameter {
        parameter_name: "   ".to_string(),
        secret: sec.clone(),
    };
    assert!(invalid_qparam.validate().is_err());

    let oauth = AuthenticationScheme::OAuthToken {
        secret: sec.clone(),
    };
    assert!(oauth.validate().is_ok());

    let aws = AuthenticationScheme::AwsSigV4 {
        credential: sec,
        region: "us-west-2".to_string(),
        service: "bedrock".to_string(),
    };
    assert!(aws.validate().is_ok());
}

#[test]
fn test_security_url_userinfo_rejected() {
    let err = EndpointProfile::new("https://admin:supersecret@api.openai.com/v1").unwrap_err();
    assert_eq!(err, ProviderError::EmbeddedCredentialsForbidden);

    let err2 = EndpointProfile::new("http://token@localhost:11434").unwrap_err();
    assert_eq!(err2, ProviderError::EmbeddedCredentialsForbidden);
}

#[test]
fn test_security_authorization_header_rejected() {
    let mut ep = EndpointProfile::new("https://api.openai.com/v1").unwrap();
    ep.static_headers
        .insert("Authorization".to_string(), "Bearer sk-12345".to_string());
    let err = ep.validate().unwrap_err();
    assert_eq!(
        err,
        ProviderError::SensitiveStaticHeaderForbidden("Authorization".to_string())
    );
}

#[test]
fn test_security_x_api_key_header_rejected() {
    let mut ep = EndpointProfile::new("https://api.anthropic.com/v1").unwrap();
    ep.static_headers
        .insert("X-Api-Key".to_string(), "secret-key".to_string());
    let err = ep.validate().unwrap_err();
    assert_eq!(
        err,
        ProviderError::SensitiveStaticHeaderForbidden("X-Api-Key".to_string())
    );
}

#[test]
fn test_security_api_key_header_rejected() {
    let mut ep = EndpointProfile::new("https://gateway.example.com").unwrap();
    ep.static_headers
        .insert("api-key".to_string(), "raw-secret".to_string());
    let err = ep.validate().unwrap_err();
    assert_eq!(
        err,
        ProviderError::SensitiveStaticHeaderForbidden("api-key".to_string())
    );
}

#[test]
fn test_security_bearer_stores_secret_reference_not_secret() {
    let auth = AuthenticationScheme::BearerToken {
        secret: SecretReference::env("MY_API_KEY_ENV").unwrap(),
    };

    let json = serde_json::to_string(&auth).unwrap();
    assert!(json.contains("MY_API_KEY_ENV"));
    assert!(!json.contains("sk-")); // confirms no actual secret is in JSON
}

#[test]
fn test_capability_support_unknown_preserved() {
    let caps = ModelCapabilities::unknown();
    assert_eq!(caps.tool_calling, CapabilitySupport::Unknown);
    assert_eq!(caps.vision_input, CapabilitySupport::Unknown);
    assert_eq!(caps.reasoning, CapabilitySupport::Unknown);
    assert_eq!(caps.streaming, CapabilitySupport::Unknown);

    // Verify Unknown does NOT equal Unsupported
    assert!(!caps.tool_calling.is_supported());
    assert!(!caps.tool_calling.is_unsupported());
    assert!(caps.tool_calling.is_unknown());
}

#[test]
fn test_model_limits_zero_tokens_rejected() {
    assert!(ModelLimits::new(Some(0), Some(4096)).is_err());
    assert!(ModelLimits::new(Some(128_000), Some(0)).is_err());
    assert!(ModelLimits::new(None, None).is_ok());
    assert!(ModelLimits::new(Some(128_000), Some(4096)).is_ok());
}

#[test]
fn test_duplicate_provider_definition_rejected() {
    let mut catalog = ProviderCatalog::new();
    let def1 = ProviderDefinition::new(
        ProviderId::new("openai").unwrap(),
        "OpenAI",
        vec![ProtocolFamily::OpenAiResponses],
    )
    .unwrap();
    let def2 = ProviderDefinition::new(
        ProviderId::new("openai").unwrap(),
        "OpenAI Duplicate",
        vec![ProtocolFamily::OpenAiResponses],
    )
    .unwrap();

    catalog.register_provider_definition(def1).unwrap();
    let err = catalog.register_provider_definition(def2).unwrap_err();
    assert_eq!(
        err,
        ProviderError::DuplicateProviderDefinition(ProviderId::new("openai").unwrap())
    );
}

#[test]
fn test_unknown_provider_definition_rejected_for_instance() {
    let mut catalog = ProviderCatalog::new();
    let inst = ProviderInstance::new(
        ProviderInstanceId::new(),
        ProviderId::new("nonexistent").unwrap(),
        "Instance 1",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    let err = catalog.register_provider_instance(inst).unwrap_err();
    assert_eq!(
        err,
        ProviderError::UnknownProviderDefinition(ProviderId::new("nonexistent").unwrap())
    );
}

#[test]
fn test_unsupported_protocol_for_definition_rejected() {
    let mut catalog = ProviderCatalog::new();
    let def = ProviderDefinition::new(
        ProviderId::new("anthropic").unwrap(),
        "Anthropic",
        vec![ProtocolFamily::AnthropicMessages],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    let inst = ProviderInstance::new(
        ProviderInstanceId::new(),
        ProviderId::new("anthropic").unwrap(),
        "Anthropic Instance",
        ProtocolFamily::OpenAiResponses, // Anthropic definition does NOT support OpenAiResponses
        EndpointProfile::new("https://api.anthropic.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    let err = catalog.register_provider_instance(inst).unwrap_err();
    assert_eq!(
        err,
        ProviderError::UnsupportedProtocol {
            provider_id: ProviderId::new("anthropic").unwrap(),
            protocol: ProtocolFamily::OpenAiResponses,
        }
    );
}

#[test]
fn test_duplicate_provider_instance_rejected() {
    let mut catalog = ProviderCatalog::new();
    let def = ProviderDefinition::new(
        ProviderId::new("openai").unwrap(),
        "OpenAI",
        vec![ProtocolFamily::OpenAiResponses],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    let shared_id = ProviderInstanceId::new();
    let inst1 = ProviderInstance::new(
        shared_id,
        ProviderId::new("openai").unwrap(),
        "Instance 1",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    let inst2 = ProviderInstance::new(
        shared_id,
        ProviderId::new("openai").unwrap(),
        "Instance 2",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    catalog.register_provider_instance(inst1).unwrap();
    let err = catalog.register_provider_instance(inst2).unwrap_err();
    assert_eq!(err, ProviderError::DuplicateProviderInstance(shared_id));
}

#[test]
fn test_safe_provider_definition_removal_rejected_when_in_use() {
    let mut catalog = ProviderCatalog::new();
    let prov_id = ProviderId::new("openai").unwrap();
    let def = ProviderDefinition::new(
        prov_id.clone(),
        "OpenAI",
        vec![ProtocolFamily::OpenAiResponses],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    let inst = ProviderInstance::new(
        ProviderInstanceId::new(),
        prov_id.clone(),
        "Instance 1",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(inst).unwrap();

    // Trying to remove definition while instance exists should fail
    let err = catalog.remove_provider_definition(&prov_id).unwrap_err();
    assert_eq!(
        err,
        ProviderError::ProviderDefinitionInUse {
            provider_id: prov_id,
            instance_count: 1,
        }
    );
}

#[test]
fn test_safe_provider_definition_removal_succeeds_when_unused() {
    let mut catalog = ProviderCatalog::new();
    let prov_id = ProviderId::new("openai").unwrap();
    let def = ProviderDefinition::new(
        prov_id.clone(),
        "OpenAI",
        vec![ProtocolFamily::OpenAiResponses],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    let removed = catalog.remove_provider_definition(&prov_id).unwrap();
    assert_eq!(removed.id, prov_id);
    assert!(catalog.get_provider_definition(&prov_id).is_none());
}

#[test]
fn test_safe_provider_instance_removal_rejected_when_models_exist() {
    let mut catalog = ProviderCatalog::new();
    let prov_id = ProviderId::new("openai").unwrap();
    let def = ProviderDefinition::new(
        prov_id.clone(),
        "OpenAI",
        vec![ProtocolFamily::OpenAiResponses],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    let inst_id = ProviderInstanceId::new();
    let inst = ProviderInstance::new(
        inst_id,
        prov_id,
        "Instance 1",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(inst).unwrap();

    let model = ModelDescriptor::new(
        inst_id,
        ModelId::new("gpt-5.6").unwrap(),
        "GPT 5.6",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(model).unwrap();

    let err = catalog.remove_provider_instance(&inst_id).unwrap_err();
    assert_eq!(
        err,
        ProviderError::ProviderInstanceInUse {
            instance_id: inst_id,
            model_count: 1,
        }
    );
}

#[test]
fn test_safe_provider_instance_removal_succeeds_when_no_models() {
    let mut catalog = ProviderCatalog::new();
    let prov_id = ProviderId::new("openai").unwrap();
    let def = ProviderDefinition::new(
        prov_id.clone(),
        "OpenAI",
        vec![ProtocolFamily::OpenAiResponses],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    let inst_id = ProviderInstanceId::new();
    let inst = ProviderInstance::new(
        inst_id,
        prov_id,
        "Instance 1",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(inst).unwrap();

    let removed = catalog.remove_provider_instance(&inst_id).unwrap();
    assert_eq!(removed.id, inst_id);
    assert!(catalog.get_provider_instance(&inst_id).is_none());
}

#[test]
fn test_cascade_provider_instance_removal() {
    let mut catalog = ProviderCatalog::new();
    let prov_id = ProviderId::new("openai").unwrap();
    let def = ProviderDefinition::new(
        prov_id.clone(),
        "OpenAI",
        vec![ProtocolFamily::OpenAiResponses],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    let inst_id = ProviderInstanceId::new();
    let inst = ProviderInstance::new(
        inst_id,
        prov_id,
        "Instance 1",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(inst).unwrap();

    let model1 = ModelDescriptor::new(
        inst_id,
        ModelId::new("model-1").unwrap(),
        "Model 1",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();
    let model2 = ModelDescriptor::new(
        inst_id,
        ModelId::new("model-2").unwrap(),
        "Model 2",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(model1).unwrap();
    catalog.register_model(model2).unwrap();

    let (removed_inst, removed_models) =
        catalog.remove_provider_instance_cascade(&inst_id).unwrap();
    assert_eq!(removed_inst.id, inst_id);
    assert_eq!(removed_models.len(), 2);
    assert!(catalog.get_provider_instance(&inst_id).is_none());
    assert_eq!(catalog.models_for_provider_instance(&inst_id).len(), 0);
}

#[test]
fn test_model_registration_requires_existing_instance() {
    let mut catalog = ProviderCatalog::new();
    let missing_instance_id = ProviderInstanceId::new();
    let model = ModelDescriptor::new(
        missing_instance_id,
        ModelId::new("gpt-5.6").unwrap(),
        "GPT 5.6",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();

    let err = catalog.register_model(model).unwrap_err();
    assert_eq!(
        err,
        ProviderError::UnknownProviderInstance(missing_instance_id)
    );
}

#[test]
fn test_duplicate_model_under_same_instance_rejected() {
    let mut catalog = ProviderCatalog::new();
    let prov_id = ProviderId::new("openai").unwrap();
    let def = ProviderDefinition::new(
        prov_id.clone(),
        "OpenAI",
        vec![ProtocolFamily::OpenAiResponses],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    let inst_id = ProviderInstanceId::new();
    let inst = ProviderInstance::new(
        inst_id,
        prov_id,
        "Instance 1",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(inst).unwrap();

    let mid = ModelId::new("gpt-5.6").unwrap();
    let model1 = ModelDescriptor::new(
        inst_id,
        mid.clone(),
        "GPT 5.6",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();
    let model2 = ModelDescriptor::new(
        inst_id,
        mid.clone(),
        "GPT 5.6 Dupe",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();

    catalog.register_model(model1).unwrap();
    let err = catalog.register_model(model2).unwrap_err();
    assert_eq!(
        err,
        ProviderError::DuplicateModel {
            instance_id: inst_id,
            model_id: mid,
        }
    );
}

#[test]
fn test_model_ref_resolution() {
    let mut catalog = ProviderCatalog::new();
    let prov_id = ProviderId::new("openai").unwrap();
    let def = ProviderDefinition::new(
        prov_id.clone(),
        "OpenAI",
        vec![ProtocolFamily::OpenAiResponses],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    let inst_id = ProviderInstanceId::new();
    let inst = ProviderInstance::new(
        inst_id,
        prov_id,
        "Instance 1",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(inst).unwrap();

    let mid = ModelId::new("gpt-5.6").unwrap();
    let model = ModelDescriptor::new(
        inst_id,
        mid.clone(),
        "GPT 5.6",
        ModelCapabilities::unknown(),
        ModelLimits::new(Some(200_000), Some(8_192)).unwrap(),
    )
    .unwrap();
    catalog.register_model(model).unwrap();

    let mref = ModelRef::new(inst_id, mid);
    let resolved = catalog.resolve_model(&mref).unwrap();
    assert_eq!(resolved.display_name, "GPT 5.6");
    assert_eq!(resolved.limits.context_window_tokens, Some(200_000));
}

/// MANDATORY TEST (Section 24): Multi-instance 9Router/custom-compatible scenario.
///
/// Register one provider definition: custom-openai
/// Protocols: OpenAiResponses, OpenAiChatCompletions
/// Register instances: 9Router Local, 9Router VPS, Company Gateway with different base URLs.
/// Register model id: "model-a" under all three.
/// Verify: all three coexist, all three resolve independently, ModelRef uniquely selects each instance/model pair.
#[test]
fn test_mandatory_multi_instance_9router_custom_scenario() {
    let mut catalog = ProviderCatalog::new();

    // 1. Register one provider definition: custom-openai
    let def_id = ProviderId::new("custom-openai").unwrap();
    let def = ProviderDefinition::new(
        def_id.clone(),
        "Custom OpenAI Gateway",
        vec![
            ProtocolFamily::OpenAiResponses,
            ProtocolFamily::OpenAiChatCompletions,
        ],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    // 2. Register three instances with different base URLs and configurations
    let inst_local_id = ProviderInstanceId::new();
    let inst_vps_id = ProviderInstanceId::new();
    let inst_gateway_id = ProviderInstanceId::new();

    let inst_local = ProviderInstance::new(
        inst_local_id,
        def_id.clone(),
        "9Router Local",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("http://localhost:8080/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    let inst_vps = ProviderInstance::new(
        inst_vps_id,
        def_id.clone(),
        "9Router VPS",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("https://vps.my-router.net/v1").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("VPS_9ROUTER_KEY").unwrap(),
        },
    )
    .unwrap();

    let inst_gateway = ProviderInstance::new(
        inst_gateway_id,
        def_id,
        "Company Gateway",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://gateway.internal.corp/ai/v1").unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "X-Gateway-Key".to_string(),
            secret: SecretReference::os_store("corp/gateway-key").unwrap(),
        },
    )
    .unwrap();

    catalog.register_provider_instance(inst_local).unwrap();
    catalog.register_provider_instance(inst_vps).unwrap();
    catalog.register_provider_instance(inst_gateway).unwrap();

    // 3. Register identical model ID "model-a" under all three instances
    let model_id = ModelId::new("model-a").unwrap();

    let model_local = ModelDescriptor::new(
        inst_local_id,
        model_id.clone(),
        "Model A (Local 8B)",
        ModelCapabilities::unknown().with_streaming(CapabilitySupport::Supported),
        ModelLimits::new(Some(32_000), Some(2_048)).unwrap(),
    )
    .unwrap();

    let model_vps = ModelDescriptor::new(
        inst_vps_id,
        model_id.clone(),
        "Model A (VPS 70B)",
        ModelCapabilities::unknown()
            .with_streaming(CapabilitySupport::Supported)
            .with_tool_calling(CapabilitySupport::Supported),
        ModelLimits::new(Some(64_000), Some(4_096)).unwrap(),
    )
    .unwrap();

    let model_gateway = ModelDescriptor::new(
        inst_gateway_id,
        model_id.clone(),
        "Model A (Enterprise Managed)",
        ModelCapabilities::unknown()
            .with_streaming(CapabilitySupport::Supported)
            .with_tool_calling(CapabilitySupport::Supported)
            .with_structured_output(CapabilitySupport::Supported),
        ModelLimits::new(Some(128_000), Some(8_192)).unwrap(),
    )
    .unwrap();

    catalog.register_model(model_local).unwrap();
    catalog.register_model(model_vps).unwrap();
    catalog.register_model(model_gateway).unwrap();

    // 4. Verify all three coexist and resolve independently via ModelRef
    let ref_local = ModelRef::new(inst_local_id, model_id.clone());
    let ref_vps = ModelRef::new(inst_vps_id, model_id.clone());
    let ref_gateway = ModelRef::new(inst_gateway_id, model_id);

    let res_local = catalog.resolve_model(&ref_local).unwrap();
    let res_vps = catalog.resolve_model(&ref_vps).unwrap();
    let res_gateway = catalog.resolve_model(&ref_gateway).unwrap();

    assert_eq!(res_local.display_name, "Model A (Local 8B)");
    assert_eq!(res_local.limits.context_window_tokens, Some(32_000));
    assert_eq!(
        res_local.capabilities.tool_calling,
        CapabilitySupport::Unknown
    );

    assert_eq!(res_vps.display_name, "Model A (VPS 70B)");
    assert_eq!(res_vps.limits.context_window_tokens, Some(64_000));
    assert_eq!(
        res_vps.capabilities.tool_calling,
        CapabilitySupport::Supported
    );

    assert_eq!(res_gateway.display_name, "Model A (Enterprise Managed)");
    assert_eq!(res_gateway.limits.context_window_tokens, Some(128_000));
    assert_eq!(
        res_gateway.capabilities.structured_output,
        CapabilitySupport::Supported
    );

    // Verify instance endpoints and protocols are independent
    let inst_local_fetch = catalog.get_provider_instance(&inst_local_id).unwrap();
    assert_eq!(
        inst_local_fetch.protocol,
        ProtocolFamily::OpenAiChatCompletions
    );
    assert_eq!(
        inst_local_fetch.endpoint.base_url,
        "http://localhost:8080/v1"
    );

    let inst_gateway_fetch = catalog.get_provider_instance(&inst_gateway_id).unwrap();
    assert_eq!(inst_gateway_fetch.protocol, ProtocolFamily::OpenAiResponses);
    assert_eq!(
        inst_gateway_fetch.endpoint.base_url,
        "https://gateway.internal.corp/ai/v1"
    );
}

/// MANDATORY TEST (Section 25): Provider / Protocol Decoupling Test.
///
/// Proves provider brand does NOT dictate transport protocol.
/// ProviderDefinition: company-ai
/// supports: OpenAiResponses, AnthropicMessages
/// Create two instances: Company Responses, Company Anthropic
/// Both share provider definition; each selects a different ProtocolFamily.
/// Both must be valid.
#[test]
fn test_mandatory_provider_protocol_decoupling() {
    let mut catalog = ProviderCatalog::new();

    let def_id = ProviderId::new("company-ai").unwrap();
    let def = ProviderDefinition::new(
        def_id.clone(),
        "Company AI Unified Gateway",
        vec![
            ProtocolFamily::OpenAiResponses,
            ProtocolFamily::AnthropicMessages,
        ],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    let inst_resp_id = ProviderInstanceId::new();
    let inst_anth_id = ProviderInstanceId::new();

    let inst_resp = ProviderInstance::new(
        inst_resp_id,
        def_id.clone(),
        "Company Responses",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://gateway.corp/v1/responses").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("CORP_RESPONSES_KEY").unwrap(),
        },
    )
    .unwrap();

    let inst_anth = ProviderInstance::new(
        inst_anth_id,
        def_id,
        "Company Anthropic",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new("https://gateway.corp/v1/messages").unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "X-Api-Key".to_string(),
            secret: SecretReference::env("CORP_ANTHROPIC_KEY").unwrap(),
        },
    )
    .unwrap();

    catalog.register_provider_instance(inst_resp).unwrap();
    catalog.register_provider_instance(inst_anth).unwrap();

    let fetch_resp = catalog.get_provider_instance(&inst_resp_id).unwrap();
    let fetch_anth = catalog.get_provider_instance(&inst_anth_id).unwrap();

    assert_eq!(fetch_resp.provider_id.as_str(), "company-ai");
    assert_eq!(fetch_resp.protocol, ProtocolFamily::OpenAiResponses);

    assert_eq!(fetch_anth.provider_id.as_str(), "company-ai");
    assert_eq!(fetch_anth.protocol, ProtocolFamily::AnthropicMessages);
}

#[test]
fn test_snapshot_roundtrip_full_catalog() {
    let mut catalog = ProviderCatalog::new();
    let prov_id = ProviderId::new("test-provider").unwrap();
    let def = ProviderDefinition::new(
        prov_id.clone(),
        "Test Provider",
        vec![ProtocolFamily::OpenAiChatCompletions],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    let inst_id = ProviderInstanceId::new();
    let inst = ProviderInstance::new(
        inst_id,
        prov_id,
        "Test Instance",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("https://api.test.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(inst).unwrap();

    let model = ModelDescriptor::new(
        inst_id,
        ModelId::new("test-model").unwrap(),
        "Test Model",
        ModelCapabilities::unknown(),
        ModelLimits::new(Some(64000), Some(4096)).unwrap(),
    )
    .unwrap();
    catalog.register_model(model).unwrap();

    let snapshot = catalog.export_snapshot();
    let json = serde_json::to_string(&snapshot).unwrap();

    let restored_snapshot: ProviderCatalogSnapshot = serde_json::from_str(&json).unwrap();
    let restored_catalog = ProviderCatalog::from_snapshot(restored_snapshot).unwrap();
    assert_eq!(catalog, restored_catalog);
}

#[test]
fn test_snapshot_atomic_rollback_preserves_catalog_state() {
    let mut catalog = ProviderCatalog::new();
    let prov_id = ProviderId::new("openai").unwrap();
    let def = ProviderDefinition::new(
        prov_id.clone(),
        "OpenAI",
        vec![ProtocolFamily::OpenAiResponses],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    // Corrupt snapshot: model references non-existent instance
    let bad_snapshot = ProviderCatalogSnapshot {
        schema_version: PROVIDER_CATALOG_SCHEMA_VERSION,
        definitions: vec![],
        instances: vec![],
        models: vec![
            ModelDescriptor::new(
                ProviderInstanceId::new(),
                ModelId::new("ghost-model").unwrap(),
                "Ghost",
                ModelCapabilities::unknown(),
                ModelLimits::default(),
            )
            .unwrap(),
        ],
    };

    let res = catalog.import_snapshot(bad_snapshot);
    assert!(res.is_err());

    // Catalog state remains intact
    assert_eq!(catalog.list_provider_definitions().len(), 1);
    assert!(catalog.get_provider_definition(&prov_id).is_some());
}

#[test]
fn test_snapshot_unsupported_version_rejected() {
    let bad_snapshot = ProviderCatalogSnapshot {
        schema_version: 42,
        definitions: vec![],
        instances: vec![],
        models: vec![],
    };

    let err = ProviderCatalog::from_snapshot(bad_snapshot).unwrap_err();
    assert_eq!(
        err,
        ProviderError::UnsupportedSchemaVersion {
            version: 42,
            supported: PROVIDER_CATALOG_SCHEMA_VERSION,
        }
    );
}

#[test]
fn test_builtin_definitions_loaded_in_catalog() {
    let catalog = ProviderCatalog::with_builtin_definitions();
    let defs = catalog.list_provider_definitions();
    assert_eq!(defs.len(), builtin_definitions().len());

    let openai = catalog
        .get_provider_definition(&ProviderId::new("openai").unwrap())
        .unwrap();
    assert_eq!(openai.display_name, "OpenAI");
    assert!(openai.supports_protocol(&ProtocolFamily::OpenAiResponses));
    assert!(openai.supports_protocol(&ProtocolFamily::OpenAiChatCompletions));

    let groq = catalog
        .get_provider_definition(&ProviderId::new("groq").unwrap())
        .unwrap();
    assert_eq!(groq.display_name, "Groq");

    let router = catalog
        .get_provider_definition(&ProviderId::new("9router").unwrap())
        .unwrap();
    assert_eq!(router.display_name, "9Router");
}

#[test]
fn test_update_provider_instance() {
    let mut catalog = ProviderCatalog::new();
    let prov_id = ProviderId::new("openai").unwrap();
    let def = ProviderDefinition::new(
        prov_id.clone(),
        "OpenAI",
        vec![
            ProtocolFamily::OpenAiResponses,
            ProtocolFamily::OpenAiChatCompletions,
        ],
    )
    .unwrap();
    catalog.register_provider_definition(def).unwrap();

    let inst_id = ProviderInstanceId::new();
    let inst = ProviderInstance::new(
        inst_id,
        prov_id.clone(),
        "Initial",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(inst).unwrap();

    // Update instance with new endpoint, display name, and protocol
    let mut updated = catalog.get_provider_instance(&inst_id).unwrap().clone();
    updated.display_name = "Updated Display Name".to_string();
    updated.protocol = ProtocolFamily::OpenAiChatCompletions;
    updated.endpoint = EndpointProfile::new("https://api.openai.proxy.com/v1").unwrap();

    catalog.update_provider_instance(updated).unwrap();

    let fetched = catalog.get_provider_instance(&inst_id).unwrap();
    assert_eq!(fetched.display_name, "Updated Display Name");
    assert_eq!(fetched.protocol, ProtocolFamily::OpenAiChatCompletions);
    assert_eq!(fetched.endpoint.base_url, "https://api.openai.proxy.com/v1");
}

#[test]
fn test_model_metadata_source_display() {
    assert_eq!(ModelMetadataSource::Manual.to_string(), "manual");
    assert_eq!(
        ModelMetadataSource::StaticCatalog.to_string(),
        "static_catalog"
    );
    assert_eq!(
        ModelMetadataSource::ProviderDiscovery.to_string(),
        "provider_discovery"
    );
}
