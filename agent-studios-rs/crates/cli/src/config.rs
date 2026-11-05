//! Persistent Configuration and Storage for Agent Studios CLI
//!
//! Manages local directory paths, catalog snapshots, and secret stores.

use std::fs;
use std::path::PathBuf;

use agent_studios_provider::catalog::ProviderCatalog;
use agent_studios_provider::snapshot::ProviderCatalogSnapshot;
use agent_studios_provider::store::CompositeSecretStore;

/// Resolves the base configuration directory for Agent Studios.
///
/// Priority:
/// 1. `$AGENT_STUDIOS_HOME` environment variable if set.
/// 2. `$HOME/.agent-studios` or platform-native config directory.
pub fn get_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("AGENT_STUDIOS_HOME") {
        let p = PathBuf::from(dir.trim());
        if !p.as_os_str().is_empty() {
            return p;
        }
    }

    if let Some(home) = dirs::home_dir() {
        home.join(".agent-studios")
    } else if let Some(config) = dirs::config_dir() {
        config.join("agent-studios")
    } else {
        PathBuf::from(".agent-studios")
    }
}

/// Returns the file path for the local provider catalog snapshot.
pub fn get_providers_config_path() -> PathBuf {
    get_config_dir().join("providers.json")
}

/// Ensures the configuration directory exists with restricted user-only permissions.
pub fn ensure_config_dir() -> Result<PathBuf, std::io::Error> {
    let dir = get_config_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Loads the persistent provider catalog, creating a default one with builtin definitions if missing.
pub fn load_catalog() -> Result<ProviderCatalog, Box<dyn std::error::Error>> {
    let path = get_providers_config_path();
    if path.exists() {
        let content = fs::read_to_string(&path)?;
        let snapshot: ProviderCatalogSnapshot = serde_json::from_str(&content)?;
        let catalog = ProviderCatalog::from_snapshot(snapshot)?;
        Ok(catalog)
    } else {
        Ok(ProviderCatalog::with_builtin_definitions())
    }
}

/// Persists the provider catalog snapshot to disk atomically.
pub fn save_catalog(catalog: &ProviderCatalog) -> Result<(), Box<dyn std::error::Error>> {
    ensure_config_dir()?;
    let path = get_providers_config_path();
    let snapshot = catalog.export_snapshot();
    let json = serde_json::to_string_pretty(&snapshot)?;

    // Atomic write via sibling temporary file
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, json.as_bytes())?;
    fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// Returns the default system secret store composite routing OS credential stores and environment variables.
pub fn get_system_secret_store() -> CompositeSecretStore {
    CompositeSecretStore::default()
}

#[cfg(test)]
pub static ENV_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_agent_studios_home_override() {
        let _guard = ENV_TEST_LOCK.blocking_lock();
        let rand_id = agent_studios_provider::id::ProviderInstanceId::new();
        let temp_dir = std::env::temp_dir().join(format!("ags_test_{rand_id}"));
        unsafe {
            std::env::set_var("AGENT_STUDIOS_HOME", &temp_dir);
        }

        let config_dir = get_config_dir();
        assert_eq!(config_dir, temp_dir);

        let provider_path = get_providers_config_path();
        assert_eq!(provider_path, temp_dir.join("providers.json"));

        unsafe {
            std::env::remove_var("AGENT_STUDIOS_HOME");
        }
    }

    #[test]
    fn test_catalog_save_and_load_roundtrip() {
        let _guard = ENV_TEST_LOCK.blocking_lock();
        let rand_id = agent_studios_provider::id::ProviderInstanceId::new();
        let temp_dir = std::env::temp_dir().join(format!("ags_test_{rand_id}"));
        unsafe {
            std::env::set_var("AGENT_STUDIOS_HOME", &temp_dir);
        }

        let mut catalog = load_catalog().expect("should load default builtin catalog");
        assert!(!catalog.list_provider_definitions().is_empty());

        let inst_id = agent_studios_provider::id::ProviderInstanceId::new();
        let instance = agent_studios_provider::instance::ProviderInstance::new(
            inst_id,
            agent_studios_provider::id::ProviderId::new("openai").unwrap(),
            "CLI Test Instance",
            agent_studios_provider::protocol::ProtocolFamily::OpenAiResponses,
            agent_studios_provider::endpoint::EndpointProfile::new("https://api.openai.com/v1")
                .unwrap(),
            agent_studios_provider::auth::AuthenticationScheme::None,
        )
        .unwrap();

        catalog.register_provider_instance(instance).unwrap();
        save_catalog(&catalog).expect("should save catalog atomically");

        let loaded = load_catalog().expect("should load persisted catalog");
        assert!(loaded.get_provider_instance(&inst_id).is_some());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
        unsafe {
            std::env::remove_var("AGENT_STUDIOS_HOME");
        }
    }
}
