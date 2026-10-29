use agent_studios_provider::{
    AuthenticationScheme, ConflictResolutionStrategy, EndpointProfile, ModelCapabilities,
    ModelDescriptor, ModelId, ModelLimits, ModelRef, ProtocolFamily, ProviderCatalog,
    ProviderError, ProviderId, ProviderInstance, ProviderInstanceId, SecretReference, SourceFormat,
    apply_import, preview_import,
};

#[test]
fn test_multi_instance_registration_and_querying() {
    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let openai_id = ProviderId::new("openai").unwrap();

    // 1. Register multiple distinct instances under the "openai" family
    let inst1_id = ProviderInstanceId::new();
    let inst1 = ProviderInstance::new(
        inst1_id,
        openai_id.clone(),
        "OpenAI Personal",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("OPENAI_PERSONAL_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(inst1).unwrap();

    let inst2_id = ProviderInstanceId::new();
    let inst2 = ProviderInstance::new(
        inst2_id,
        openai_id.clone(),
        "OpenAI Work",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("OPENAI_WORK_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(inst2).unwrap();

    let inst3_id = ProviderInstanceId::new();
    let inst3 = ProviderInstance::new(
        inst3_id,
        openai_id.clone(),
        "OpenAI Staging Proxy",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new("https://staging.openai-proxy.internal/v1").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("OPENAI_STAGING_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(inst3).unwrap();

    // 2. Query instances for provider family
    let instances = catalog.get_instances_for_provider(&openai_id);
    assert_eq!(instances.len(), 3);
    let names: Vec<&str> = instances.iter().map(|i| i.display_name.as_str()).collect();
    assert!(names.contains(&"OpenAI Personal"));
    assert!(names.contains(&"OpenAI Work"));
    assert!(names.contains(&"OpenAI Staging Proxy"));

    // 3. Configure and query default instance
    assert!(
        catalog
            .get_default_instance_for_provider(&openai_id)
            .is_none()
    );
    assert!(
        catalog
            .get_default_instance_id_for_provider(&openai_id)
            .is_none()
    );

    catalog
        .set_default_instance_for_provider(&openai_id, &inst1_id)
        .unwrap();

    let default_inst = catalog
        .get_default_instance_for_provider(&openai_id)
        .unwrap();
    assert_eq!(default_inst.id, inst1_id);
    assert_eq!(default_inst.display_name, "OpenAI Personal");
    assert_eq!(
        catalog.get_default_instance_id_for_provider(&openai_id),
        Some(&inst1_id)
    );

    // 4. Reject setting an instance belonging to a different provider family as default
    let anthropic_id = ProviderId::new("anthropic").unwrap();
    let anthropic_inst_id = ProviderInstanceId::new();
    let anthropic_inst = ProviderInstance::new(
        anthropic_inst_id,
        anthropic_id.clone(),
        "Anthropic Team",
        ProtocolFamily::AnthropicMessages,
        EndpointProfile::new("https://api.anthropic.com").unwrap(),
        AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: SecretReference::env("ANTHROPIC_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(anthropic_inst).unwrap();

    let err = catalog
        .set_default_instance_for_provider(&openai_id, &anthropic_inst_id)
        .unwrap_err();
    assert!(matches!(err, ProviderError::InvalidConfiguration(_)));

    // 5. Clear default instance
    let cleared = catalog.clear_default_instance_for_provider(&openai_id);
    assert_eq!(cleared, Some(inst1_id));
    assert!(
        catalog
            .get_default_instance_for_provider(&openai_id)
            .is_none()
    );

    // 6. Removing default instance clears default mapping automatically
    catalog
        .set_default_instance_for_provider(&openai_id, &inst2_id)
        .unwrap();
    assert!(
        catalog
            .get_default_instance_for_provider(&openai_id)
            .is_some()
    );

    catalog.remove_provider_instance(&inst2_id).unwrap();
    assert!(
        catalog
            .get_default_instance_for_provider(&openai_id)
            .is_none()
    );
    assert_eq!(catalog.get_instances_for_provider(&openai_id).len(), 2);
}

#[test]
fn test_codex_toml_import_preview_dry_run_and_commit() {
    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let openai_id = ProviderId::new("openai").unwrap();

    let toml_content = r#"
model = "gpt-5.6"

[model_providers.openai-personal]
name = "OpenAI Personal"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_PERSONAL_KEY"
wire_api = "responses"

[model_providers.openai-work]
name = "OpenAI Work"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_WORK_KEY"
wire_api = "responses"
"#;

    // 1. Dry run preview must NOT mutate the catalog
    let preview = preview_import(
        &catalog,
        SourceFormat::Codex,
        toml_content,
        ConflictResolutionStrategy::Overwrite,
    )
    .unwrap();

    assert_eq!(preview.added.len(), 2);
    assert_eq!(preview.updated.len(), 0);
    assert_eq!(preview.skipped.len(), 0);
    assert_eq!(preview.conflicts.len(), 0);

    // Verify catalog remains pristine
    assert_eq!(catalog.get_instances_for_provider(&openai_id).len(), 0);

    // 2. Commit preview into catalog
    let result =
        apply_import(&mut catalog, preview, ConflictResolutionStrategy::Overwrite).unwrap();

    assert_eq!(result.added_count, 2);
    assert_eq!(result.updated_count, 0);
    assert_eq!(result.skipped_count, 0);

    // Verify both instances are active in catalog
    let instances = catalog.get_instances_for_provider(&openai_id);
    assert_eq!(instances.len(), 2);

    let personal = instances
        .iter()
        .find(|i| i.display_name == "OpenAI Personal")
        .expect("OpenAI Personal must exist");
    assert_eq!(personal.protocol, ProtocolFamily::OpenAiResponses);
    assert_eq!(personal.endpoint.base_url, "https://api.openai.com/v1");
    match &personal.authentication {
        AuthenticationScheme::BearerToken { secret } => {
            assert_eq!(secret.locator, "OPENAI_PERSONAL_KEY");
        }
        other => panic!("Expected BearerToken, got {other:?}"),
    }

    let work = instances
        .iter()
        .find(|i| i.display_name == "OpenAI Work")
        .expect("OpenAI Work must exist");
    match &work.authentication {
        AuthenticationScheme::BearerToken { secret } => {
            assert_eq!(secret.locator, "OPENAI_WORK_KEY");
        }
        other => panic!("Expected BearerToken, got {other:?}"),
    }
}

#[test]
fn test_opencode_json_import_preview_dry_run_and_commit() {
    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let openai_id = ProviderId::new("openai").unwrap();
    let anthropic_id = ProviderId::new("anthropic").unwrap();

    let json_content = r#"{
      "$schema": "https://opencode.ai/config.json",
      "providers": {
        "openai": {
          "name": "OpenAI Main",
          "baseURL": "https://api.openai.com/v1",
          "apiKey": "OPENAI_API_KEY",
          "protocol": "openai"
        },
        "anthropic": {
          "name": "Anthropic Main",
          "baseURL": "https://api.anthropic.com",
          "apiKey": "env:ANTHROPIC_API_KEY",
          "protocol": "anthropic"
        }
      }
    }"#;

    // 1. Dry run preview
    let preview = preview_import(
        &catalog,
        SourceFormat::OpenCode,
        json_content,
        ConflictResolutionStrategy::Overwrite,
    )
    .unwrap();

    assert_eq!(preview.added.len(), 2);
    assert_eq!(preview.updated.len(), 0);
    assert_eq!(preview.conflicts.len(), 0);
    assert_eq!(catalog.list_provider_instances().len(), 0);

    // 2. Commit preview
    let result =
        apply_import(&mut catalog, preview, ConflictResolutionStrategy::Overwrite).unwrap();

    assert_eq!(result.added_count, 2);
    assert_eq!(catalog.get_instances_for_provider(&openai_id).len(), 1);
    assert_eq!(catalog.get_instances_for_provider(&anthropic_id).len(), 1);

    let anthropic_inst = catalog.get_instances_for_provider(&anthropic_id)[0];
    assert_eq!(anthropic_inst.display_name, "Anthropic Main");
    assert_eq!(anthropic_inst.protocol, ProtocolFamily::AnthropicMessages);
    match &anthropic_inst.authentication {
        AuthenticationScheme::ApiKeyHeader {
            header_name,
            secret,
        } => {
            assert_eq!(header_name, "x-api-key");
            assert_eq!(secret.locator, "ANTHROPIC_API_KEY");
        }
        other => panic!("Expected ApiKeyHeader for Anthropic, got {other:?}"),
    }
}

#[test]
fn test_conflict_resolution_overwrite_mode() {
    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let openai_id = ProviderId::new("openai").unwrap();

    // Pre-populate catalog with an existing instance
    let existing_id = ProviderInstanceId::new();
    let existing_inst = ProviderInstance::new(
        existing_id,
        openai_id.clone(),
        "OpenAI Work",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://old-endpoint.openai.com/v1").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("OLD_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(existing_inst).unwrap();

    // Incoming config with same display name but updated endpoint and key
    let toml_content = r#"
[model_providers.openai-work]
name = "OpenAI Work"
base_url = "https://new-endpoint.openai.com/v1"
env_key = "NEW_KEY"
wire_api = "responses"
"#;

    let preview = preview_import(
        &catalog,
        SourceFormat::Codex,
        toml_content,
        ConflictResolutionStrategy::Overwrite,
    )
    .unwrap();

    assert_eq!(preview.conflicts.len(), 1);
    assert_eq!(preview.updated.len(), 1);
    assert_eq!(preview.added.len(), 0);
    assert_eq!(preview.skipped.len(), 0);
    assert_eq!(preview.conflicts[0].existing_name, "OpenAI Work");
    assert_eq!(preview.conflicts[0].incoming_name, "OpenAI Work");

    // Apply Overwrite
    let result =
        apply_import(&mut catalog, preview, ConflictResolutionStrategy::Overwrite).unwrap();

    assert_eq!(result.updated_count, 1);
    assert_eq!(result.added_count, 0);

    // Verify existing instance was updated in place
    let updated = catalog.get_provider_instance(&existing_id).unwrap();
    assert_eq!(
        updated.endpoint.base_url,
        "https://new-endpoint.openai.com/v1"
    );
    match &updated.authentication {
        AuthenticationScheme::BearerToken { secret } => {
            assert_eq!(secret.locator, "NEW_KEY");
        }
        other => panic!("Expected BearerToken, got {other:?}"),
    }
}

#[test]
fn test_conflict_resolution_skip_mode() {
    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let openai_id = ProviderId::new("openai").unwrap();

    // Pre-populate catalog
    let existing_id = ProviderInstanceId::new();
    let existing_inst = ProviderInstance::new(
        existing_id,
        openai_id.clone(),
        "OpenAI Work",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://original.openai.com/v1").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("ORIGINAL_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(existing_inst).unwrap();

    let toml_content = r#"
[model_providers.openai-work]
name = "OpenAI Work"
base_url = "https://ignored.openai.com/v1"
env_key = "IGNORED_KEY"
wire_api = "responses"
"#;

    let preview = preview_import(
        &catalog,
        SourceFormat::Codex,
        toml_content,
        ConflictResolutionStrategy::Skip,
    )
    .unwrap();

    assert_eq!(preview.conflicts.len(), 1);
    assert_eq!(preview.skipped.len(), 1);
    assert_eq!(preview.updated.len(), 0);
    assert_eq!(preview.added.len(), 0);

    // Apply Skip
    let result = apply_import(&mut catalog, preview, ConflictResolutionStrategy::Skip).unwrap();

    assert_eq!(result.skipped_count, 1);
    assert_eq!(result.updated_count, 0);
    assert_eq!(result.added_count, 0);

    // Verify original configuration was retained
    let current = catalog.get_provider_instance(&existing_id).unwrap();
    assert_eq!(current.endpoint.base_url, "https://original.openai.com/v1");
    match &current.authentication {
        AuthenticationScheme::BearerToken { secret } => {
            assert_eq!(secret.locator, "ORIGINAL_KEY");
        }
        other => panic!("Expected BearerToken, got {other:?}"),
    }
}

#[test]
fn test_conflict_resolution_error_mode() {
    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let openai_id = ProviderId::new("openai").unwrap();

    let existing_id = ProviderInstanceId::new();
    let existing_inst = ProviderInstance::new(
        existing_id,
        openai_id,
        "OpenAI Work",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://original.openai.com/v1").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("ORIGINAL_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(existing_inst).unwrap();

    let toml_content = r#"
[model_providers.openai-work]
name = "OpenAI Work"
base_url = "https://conflicting.openai.com/v1"
env_key = "CONFLICTING_KEY"
"#;

    let preview = preview_import(
        &catalog,
        SourceFormat::Codex,
        toml_content,
        ConflictResolutionStrategy::Error,
    )
    .unwrap();

    assert_eq!(preview.conflicts.len(), 1);

    // Applying with Error strategy must fail completely
    let err = apply_import(&mut catalog, preview, ConflictResolutionStrategy::Error).unwrap_err();

    assert!(matches!(err, ProviderError::ImportConflict(_)));

    // Catalog remains unmodified
    let current = catalog.get_provider_instance(&existing_id).unwrap();
    assert_eq!(current.endpoint.base_url, "https://original.openai.com/v1");
}

#[test]
fn test_atomic_rollback_on_corrupt_or_invalid_input() {
    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let openai_id = ProviderId::new("openai").unwrap();

    // Baseline catalog instance
    let baseline_id = ProviderInstanceId::new();
    let baseline_inst = ProviderInstance::new(
        baseline_id,
        openai_id,
        "OpenAI Baseline",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();
    catalog.register_provider_instance(baseline_inst).unwrap();

    // 1. Corrupt TOML syntax
    let bad_toml = "model_providers = { unclosed_syntax";
    let err_toml = preview_import(
        &catalog,
        SourceFormat::Codex,
        bad_toml,
        ConflictResolutionStrategy::Overwrite,
    )
    .unwrap_err();
    assert!(matches!(err_toml, ProviderError::ImportParseError(_)));
    assert_eq!(catalog.list_provider_instances().len(), 1);

    // 2. Corrupt JSON syntax
    let bad_json = "{\"providers\": { invalid_json";
    let err_json = preview_import(
        &catalog,
        SourceFormat::OpenCode,
        bad_json,
        ConflictResolutionStrategy::Overwrite,
    )
    .unwrap_err();
    assert!(matches!(err_json, ProviderError::ImportParseError(_)));
    assert_eq!(catalog.list_provider_instances().len(), 1);

    // 3. Security invariant violation: URL containing embedded user credentials
    let bad_security_toml = r#"
[model_providers.bad-sec]
name = "Insecure"
base_url = "https://user:password@insecure-proxy.internal/v1"
"#;
    let err_sec = preview_import(
        &catalog,
        SourceFormat::Codex,
        bad_security_toml,
        ConflictResolutionStrategy::Overwrite,
    )
    .unwrap_err();
    assert!(matches!(
        err_sec,
        ProviderError::EmbeddedCredentialsForbidden
    ));
    assert_eq!(catalog.list_provider_instances().len(), 1);

    // 4. Staged rollback during commit: preview with unknown provider definition
    let unknown_prov_id = ProviderId::new("nonexistent-provider-family").unwrap();
    let orphan_inst = ProviderInstance::new(
        ProviderInstanceId::new(),
        unknown_prov_id,
        "Orphan Instance",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.example.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    let invalid_preview = agent_studios_provider::ImportPreview {
        added: vec![orphan_inst],
        ..Default::default()
    };

    let commit_err = apply_import(
        &mut catalog,
        invalid_preview,
        ConflictResolutionStrategy::Overwrite,
    )
    .unwrap_err();
    assert!(matches!(
        commit_err,
        ProviderError::UnknownProviderDefinition(_)
    ));

    // Catalog state remains completely untouched
    assert_eq!(catalog.list_provider_instances().len(), 1);
    assert_eq!(
        catalog.list_provider_instances()[0].display_name,
        "OpenAI Baseline"
    );
}

#[test]
fn test_agent_studios_snapshot_importer_with_defaults() {
    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let openai_id = ProviderId::new("openai").unwrap();

    let inst_id = ProviderInstanceId::new();
    let inst = ProviderInstance::new(
        inst_id,
        openai_id.clone(),
        "OpenAI Snapshot Instance",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::BearerToken {
            secret: SecretReference::env("SNAPSHOT_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog.register_provider_instance(inst).unwrap();
    catalog
        .set_default_instance_for_provider(&openai_id, &inst_id)
        .unwrap();

    let model_id = ModelId::new("gpt-5.6").unwrap();
    let model = ModelDescriptor::new(
        inst_id,
        model_id.clone(),
        "GPT-5.6",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();
    catalog.register_model(model).unwrap();

    let snapshot = catalog.export_snapshot();
    let snapshot_json = serde_json::to_string(&snapshot).unwrap();

    // Import snapshot into a new catalog
    let mut fresh_catalog = ProviderCatalog::with_builtin_definitions();
    let preview = preview_import(
        &fresh_catalog,
        SourceFormat::AgentStudios,
        &snapshot_json,
        ConflictResolutionStrategy::Overwrite,
    )
    .unwrap();

    assert_eq!(preview.added.len(), 1);
    assert_eq!(preview.conflicts.len(), 0);

    let result = apply_import(
        &mut fresh_catalog,
        preview,
        ConflictResolutionStrategy::Overwrite,
    )
    .unwrap();

    assert_eq!(result.added_count, 1);
    assert_eq!(
        fresh_catalog.get_default_instance_id_for_provider(&openai_id),
        Some(&inst_id)
    );
    let resolved_model = fresh_catalog.resolve_model(&ModelRef::new(inst_id, model_id));
    assert!(resolved_model.is_some());
}
