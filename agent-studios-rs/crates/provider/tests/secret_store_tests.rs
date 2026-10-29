use std::sync::Arc;

use agent_studios_provider::{
    CompositeSecretStore, DefaultSecretResolver, EnvironmentSecretStore, InMemorySecretStore,
    ProviderError, ProviderInstanceId, SecretBackend, SecretReference, SecretStore,
    locator_for_provider_key, secret_ref_for_provider_key,
};

#[test]
fn test_in_memory_secret_store_crud() {
    let store = InMemorySecretStore::new();
    let reference =
        SecretReference::new(SecretBackend::EnvironmentVariable, "test_api_key").unwrap();

    // 1. Initial lookup -> Not Found
    let res = store.get_secret(&reference);
    assert_eq!(
        res.unwrap_err(),
        ProviderError::SecretNotFound("test_api_key".to_string())
    );
    assert!(!store.contains_key("test_api_key").unwrap());

    // 2. Create (Set)
    store.set_secret(&reference, "initial_token_123").unwrap();
    assert!(store.contains_key("test_api_key").unwrap());

    // 3. Read (Get)
    let fetched = store.get_secret(&reference).unwrap();
    assert_eq!(fetched, "initial_token_123");

    // 4. Update (Set again)
    store.set_secret(&reference, "updated_token_456").unwrap();
    let updated = store.get_secret(&reference).unwrap();
    assert_eq!(updated, "updated_token_456");

    // 5. Delete
    store.delete_secret(&reference).unwrap();
    assert!(!store.contains_key("test_api_key").unwrap());

    // 6. Read after delete -> Not Found
    let deleted_fetch = store.get_secret(&reference);
    assert_eq!(
        deleted_fetch.unwrap_err(),
        ProviderError::SecretNotFound("test_api_key".to_string())
    );

    // 7. Delete non-existent -> Not Found
    let repeat_delete = store.delete_secret(&reference);
    assert_eq!(
        repeat_delete.unwrap_err(),
        ProviderError::SecretNotFound("test_api_key".to_string())
    );
}

#[test]
fn test_environment_secret_store_crud_and_missing() {
    let store = EnvironmentSecretStore::new();
    let var_name = "AGENT_STUDIOS_TEST_SECRET_ENV_VAR_UNIQUE";
    let reference = SecretReference::env(var_name).unwrap();

    // Ensure clean state
    unsafe {
        std::env::remove_var(var_name);
    }

    // 1. Missing env var returns SecretNotFound
    let err = store.get_secret(&reference).unwrap_err();
    assert_eq!(err, ProviderError::SecretNotFound(var_name.to_string()));

    // 2. Set secret through store
    store
        .set_secret(&reference, "env_secret_token_abc")
        .unwrap();

    // 3. Read secret through store and verify std::env
    let fetched = store.get_secret(&reference).unwrap();
    assert_eq!(fetched, "env_secret_token_abc");
    assert_eq!(std::env::var(var_name).unwrap(), "env_secret_token_abc");

    // 4. Delete secret through store
    store.delete_secret(&reference).unwrap();
    assert!(std::env::var(var_name).is_err());

    // 5. Read again -> SecretNotFound
    let post_delete = store.get_secret(&reference).unwrap_err();
    assert_eq!(
        post_delete,
        ProviderError::SecretNotFound(var_name.to_string())
    );

    // 6. Delete again -> SecretNotFound
    let repeat_delete = store.delete_secret(&reference).unwrap_err();
    assert_eq!(
        repeat_delete,
        ProviderError::SecretNotFound(var_name.to_string())
    );

    // 7. Rejection of non-environment backend
    let os_ref = SecretReference::os_store("some_key").unwrap();
    assert_eq!(
        store.get_secret(&os_ref).unwrap_err(),
        ProviderError::UnsupportedSecretBackend(SecretBackend::OsCredentialStore)
    );
    assert_eq!(
        store.set_secret(&os_ref, "val").unwrap_err(),
        ProviderError::UnsupportedSecretBackend(SecretBackend::OsCredentialStore)
    );
    assert_eq!(
        store.delete_secret(&os_ref).unwrap_err(),
        ProviderError::UnsupportedSecretBackend(SecretBackend::OsCredentialStore)
    );
}

#[test]
fn test_composite_secret_store_routing() {
    let os_mock = Arc::new(InMemorySecretStore::new());
    let env_mock = Arc::new(InMemorySecretStore::new());
    let ext_mock = Arc::new(InMemorySecretStore::new());

    let composite =
        CompositeSecretStore::new(os_mock.clone(), env_mock.clone(), Some(ext_mock.clone()));

    let os_ref = SecretReference::os_store("os_locator_target").unwrap();
    let env_ref = SecretReference::env("ENV_LOCATOR_TARGET").unwrap();
    let ext_ref = SecretReference::external("ext_locator_target").unwrap();

    // 1. Set secrets across backends
    composite.set_secret(&os_ref, "os_secret_data").unwrap();
    composite.set_secret(&env_ref, "env_secret_data").unwrap();
    composite.set_secret(&ext_ref, "ext_secret_data").unwrap();

    // 2. Verify routing isolation
    assert_eq!(os_mock.get_secret(&os_ref).unwrap(), "os_secret_data");
    assert!(os_mock.get_secret(&env_ref).is_err());
    assert!(os_mock.get_secret(&ext_ref).is_err());

    assert_eq!(env_mock.get_secret(&env_ref).unwrap(), "env_secret_data");
    assert!(env_mock.get_secret(&os_ref).is_err());
    assert!(env_mock.get_secret(&ext_ref).is_err());

    assert_eq!(ext_mock.get_secret(&ext_ref).unwrap(), "ext_secret_data");
    assert!(ext_mock.get_secret(&os_ref).is_err());
    assert!(ext_mock.get_secret(&env_ref).is_err());

    // 3. Read secrets through composite
    assert_eq!(composite.get_secret(&os_ref).unwrap(), "os_secret_data");
    assert_eq!(composite.get_secret(&env_ref).unwrap(), "env_secret_data");
    assert_eq!(composite.get_secret(&ext_ref).unwrap(), "ext_secret_data");

    // 4. Delete secrets through composite
    composite.delete_secret(&os_ref).unwrap();
    composite.delete_secret(&env_ref).unwrap();
    composite.delete_secret(&ext_ref).unwrap();

    assert!(composite.get_secret(&os_ref).is_err());
    assert!(composite.get_secret(&env_ref).is_err());
    assert!(composite.get_secret(&ext_ref).is_err());

    // 5. Unconfigured external store returns UnsupportedSecretBackend
    let composite_no_ext = CompositeSecretStore::new(os_mock, env_mock, None);
    assert_eq!(
        composite_no_ext.get_secret(&ext_ref).unwrap_err(),
        ProviderError::UnsupportedSecretBackend(SecretBackend::External)
    );
    assert_eq!(
        composite_no_ext.set_secret(&ext_ref, "val").unwrap_err(),
        ProviderError::UnsupportedSecretBackend(SecretBackend::External)
    );
    assert_eq!(
        composite_no_ext.delete_secret(&ext_ref).unwrap_err(),
        ProviderError::UnsupportedSecretBackend(SecretBackend::External)
    );

    // 6. Verify DefaultSecretResolver alias works as CompositeSecretStore
    let resolver: DefaultSecretResolver = CompositeSecretStore::in_memory();
    resolver.set_secret(&os_ref, "val_resolver").unwrap();
    assert_eq!(resolver.get_secret(&os_ref).unwrap(), "val_resolver");
}

#[test]
fn test_secret_reference_redaction_and_security_invariants() {
    let sensitive_raw_token = "sk-live-never-leak-this-secret-1234567890";
    let locator = "agent-studios:provider:test-instance:openai-key";
    let sec_ref = SecretReference::os_store(locator).unwrap();

    // 1. Display (to_string) contains only backend and locator, NEVER raw secret
    let display_str = sec_ref.to_string();
    assert_eq!(display_str, format!("os_credential_store:{locator}"));
    assert!(!display_str.contains(sensitive_raw_token));

    // 2. Debug representation contains only backend and locator, NEVER raw secret
    let debug_str = format!("{sec_ref:?}");
    assert!(debug_str.contains("OsCredentialStore"));
    assert!(debug_str.contains(locator));
    assert!(!debug_str.contains(sensitive_raw_token));

    // 3. Serialization contains only backend and locator, NEVER raw secret
    let json_str = serde_json::to_string(&sec_ref).unwrap();
    assert!(json_str.contains("\"backend\":\"os_credential_store\""));
    assert!(json_str.contains(&format!("\"locator\":\"{locator}\"")));
    assert!(!json_str.contains(sensitive_raw_token));

    // 4. ProviderError messages contain locator / safe details, NEVER raw secret
    let err_not_found = ProviderError::SecretNotFound(sec_ref.locator.clone());
    let err_display = err_not_found.to_string();
    assert!(err_display.contains(locator));
    assert!(!err_display.contains(sensitive_raw_token));

    let err_storage = ProviderError::SecretStoreError("connection failed".to_string());
    assert!(!err_storage.to_string().contains(sensitive_raw_token));

    let err_unsupported = ProviderError::UnsupportedSecretBackend(SecretBackend::External);
    assert!(!err_unsupported.to_string().contains(sensitive_raw_token));
}

#[test]
fn test_standardized_locator_generation() {
    let instance_id = ProviderInstanceId::new();
    let key_name = "api_key";

    let locator = locator_for_provider_key(&instance_id, key_name);
    assert_eq!(
        locator,
        format!("agent-studios:provider:{instance_id}:api_key")
    );

    let sec_ref =
        secret_ref_for_provider_key(&instance_id, key_name, SecretBackend::OsCredentialStore)
            .unwrap();
    assert_eq!(sec_ref.backend, SecretBackend::OsCredentialStore);
    assert_eq!(sec_ref.locator, locator);
}
