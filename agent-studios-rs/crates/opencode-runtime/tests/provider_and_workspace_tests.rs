use std::path::PathBuf;

use agent_studios_external_runtime::WorkspaceAccessMode;
use agent_studios_opencode_runtime::control_plane::validate_session_workspace;
use agent_studios_opencode_runtime::provider_mapping::{
    map_model_ref_to_opencode, resolve_opencode_provider_name,
};
use agent_studios_opencode_runtime::server::verify_loopback_url;
use agent_studios_protocol::id::WorktreeId;
use agent_studios_protocol::worktree::ExecutionWorkspace;
use agent_studios_provider::ProviderCatalog;
use agent_studios_provider::id::{ModelId, ProviderId, ProviderInstanceId};
use agent_studios_provider::model::ModelRef;
use tempfile::tempdir;
use url::Url;

#[test]
fn test_loopback_url_verification() {
    assert!(verify_loopback_url(&Url::parse("http://127.0.0.1:8080").unwrap()).is_ok());
    assert!(verify_loopback_url(&Url::parse("http://localhost:3000").unwrap()).is_ok());
    assert!(verify_loopback_url(&Url::parse("http://[::1]:5432").unwrap()).is_ok());

    // Non-loopback / remote URLs must be rejected
    assert!(verify_loopback_url(&Url::parse("http://192.168.1.1:8080").unwrap()).is_err());
    assert!(verify_loopback_url(&Url::parse("http://example.com:8080").unwrap()).is_err());
    assert!(verify_loopback_url(&Url::parse("https://remote.server.io").unwrap()).is_err());
}

#[test]
fn test_provider_name_resolution() {
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("anthropic").unwrap()),
        "anthropic"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("claude-api").unwrap()),
        "anthropic"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("openai-global").unwrap()),
        "openai"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("google-gemini").unwrap()),
        "google"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("openrouter-proxy").unwrap()),
        "openrouter"
    );
    assert_eq!(
        resolve_opencode_provider_name(&ProviderId::new("ollama-local").unwrap()),
        "ollama"
    );
}

#[test]
fn test_model_ref_mapping() {
    let mut catalog = ProviderCatalog::with_builtin_definitions();
    let instance_id = ProviderInstanceId::new();
    let anthropic_instance = agent_studios_provider::ProviderInstance::new(
        instance_id,
        ProviderId::new("anthropic").unwrap(),
        "Anthropic Production",
        agent_studios_provider::ProtocolFamily::AnthropicMessages,
        agent_studios_provider::EndpointProfile::new("https://api.anthropic.com").unwrap(),
        agent_studios_provider::AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: agent_studios_provider::SecretReference::env("ANTHROPIC_API_KEY").unwrap(),
        },
    )
    .unwrap();
    catalog
        .register_provider_instance(anthropic_instance)
        .unwrap();

    let model_ref = ModelRef::new(instance_id, ModelId::new("claude-3-7-sonnet").unwrap());

    let binding = map_model_ref_to_opencode(&catalog, &model_ref).unwrap();
    assert_eq!(binding.model_name, "claude-3-7-sonnet");
    assert_eq!(binding.auth_env_var, Some("ANTHROPIC_API_KEY".to_string()));

    // Verify fail-closed behavior for unregistered provider instance (M11-R07)
    let unregistered_ref = ModelRef::new(
        ProviderInstanceId::new(),
        ModelId::new("claude-3-7-sonnet").unwrap(),
    );
    assert!(map_model_ref_to_opencode(&catalog, &unregistered_ref).is_err());
}

#[test]
fn test_workspace_validation_managed_vs_shared() {
    let temp = tempdir().unwrap();
    let temp_path = temp.path().to_path_buf();

    // 1. Valid managed worktree
    let managed = ExecutionWorkspace::managed(
        WorktreeId::new(),
        temp_path.clone(),
        temp_path.clone(),
        PathBuf::from("/source"),
        PathBuf::from("/source"),
        "0123456789abcdef0123456789abcdef01234567",
    );

    let res = validate_session_workspace(&managed, WorkspaceAccessMode::Mutating);
    assert!(
        res.is_ok(),
        "Expected managed workspace to validate: {:?}",
        res.err()
    );
    assert_eq!(res.unwrap(), temp_path.canonicalize().unwrap());

    // 2. Non-existent managed worktree path fails closed
    let non_existent = ExecutionWorkspace::managed(
        WorktreeId::new(),
        PathBuf::from("C:\\definitely\\does\\not\\exist\\worktree_123"),
        PathBuf::from("C:\\definitely\\does\\not\\exist\\worktree_123"),
        PathBuf::from("/source"),
        PathBuf::from("/source"),
        "0123456789abcdef0123456789abcdef01234567",
    );
    let fail_res = validate_session_workspace(&non_existent, WorkspaceAccessMode::Mutating);
    assert!(fail_res.is_err());

    // 3. SharedSource strictly fails closed
    let shared = ExecutionWorkspace::shared_source(temp_path);
    let shared_res = validate_session_workspace(&shared, WorkspaceAccessMode::Mutating);
    assert!(
        shared_res.is_err(),
        "SharedSource must be rejected to prevent direct repository mutation"
    );
}
