use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};

use crate::error::ProviderError;
use crate::secret::{SecretBackend, SecretReference};

/// Default service name for OS keyring storage.
pub const DEFAULT_KEYRING_SERVICE: &str = "agent-studios";

/// Abstraction for storing, retrieving, and deleting provider credentials.
pub trait SecretStore: Send + Sync {
    /// Retrieves a secret value for the given reference.
    fn get_secret(&self, reference: &SecretReference) -> Result<String, ProviderError>;

    /// Stores a secret value for the given reference.
    fn set_secret(&self, reference: &SecretReference, value: &str) -> Result<(), ProviderError>;

    /// Deletes the secret associated with the given reference.
    fn delete_secret(&self, reference: &SecretReference) -> Result<(), ProviderError>;
}

/// Secret store backed by the platform OS credential store (Keyring / Keychain / Credential Manager).
#[derive(Clone, Debug)]
pub struct OsKeyringSecretStore {
    service_name: String,
}

impl Default for OsKeyringSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl OsKeyringSecretStore {
    /// Creates a new OS keyring secret store with default service name `"agent-studios"`.
    pub fn new() -> Self {
        Self {
            service_name: DEFAULT_KEYRING_SERVICE.to_string(),
        }
    }

    /// Creates a new OS keyring secret store with a custom service name.
    pub fn with_service(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
        }
    }

    /// Returns the configured service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}

impl SecretStore for OsKeyringSecretStore {
    fn get_secret(&self, reference: &SecretReference) -> Result<String, ProviderError> {
        reference.validate()?;
        if reference.backend != SecretBackend::OsCredentialStore {
            return Err(ProviderError::UnsupportedSecretBackend(
                reference.backend.clone(),
            ));
        }

        let entry = keyring::Entry::new(&self.service_name, &reference.locator)
            .map_err(|e| map_keyring_error(e, reference))?;
        entry
            .get_password()
            .map_err(|e| map_keyring_error(e, reference))
    }

    fn set_secret(&self, reference: &SecretReference, value: &str) -> Result<(), ProviderError> {
        reference.validate()?;
        if reference.backend != SecretBackend::OsCredentialStore {
            return Err(ProviderError::UnsupportedSecretBackend(
                reference.backend.clone(),
            ));
        }

        let entry = keyring::Entry::new(&self.service_name, &reference.locator)
            .map_err(|e| map_keyring_error(e, reference))?;
        entry
            .set_password(value)
            .map_err(|e| map_keyring_error(e, reference))
    }

    fn delete_secret(&self, reference: &SecretReference) -> Result<(), ProviderError> {
        reference.validate()?;
        if reference.backend != SecretBackend::OsCredentialStore {
            return Err(ProviderError::UnsupportedSecretBackend(
                reference.backend.clone(),
            ));
        }

        let entry = keyring::Entry::new(&self.service_name, &reference.locator)
            .map_err(|e| map_keyring_error(e, reference))?;
        entry
            .delete_credential()
            .map_err(|e| map_keyring_error(e, reference))
    }
}

/// Secret store backed by process environment variables.
#[derive(Clone, Debug, Default)]
pub struct EnvironmentSecretStore;

impl EnvironmentSecretStore {
    /// Creates a new environment secret store.
    pub fn new() -> Self {
        Self
    }
}

impl SecretStore for EnvironmentSecretStore {
    fn get_secret(&self, reference: &SecretReference) -> Result<String, ProviderError> {
        reference.validate()?;
        if reference.backend != SecretBackend::EnvironmentVariable {
            return Err(ProviderError::UnsupportedSecretBackend(
                reference.backend.clone(),
            ));
        }

        match std::env::var(&reference.locator) {
            Ok(val) => Ok(val),
            Err(std::env::VarError::NotPresent) => {
                Err(ProviderError::SecretNotFound(reference.locator.clone()))
            }
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(ProviderError::SecretStoreError(format!(
                    "Environment variable '{}' contains invalid unicode",
                    reference.locator
                )))
            }
        }
    }

    fn set_secret(&self, reference: &SecretReference, value: &str) -> Result<(), ProviderError> {
        reference.validate()?;
        if reference.backend != SecretBackend::EnvironmentVariable {
            return Err(ProviderError::UnsupportedSecretBackend(
                reference.backend.clone(),
            ));
        }

        // SAFETY: EnvironmentSecretStore sets process environment variables for secret management.
        unsafe {
            std::env::set_var(&reference.locator, value);
        }
        Ok(())
    }

    fn delete_secret(&self, reference: &SecretReference) -> Result<(), ProviderError> {
        reference.validate()?;
        if reference.backend != SecretBackend::EnvironmentVariable {
            return Err(ProviderError::UnsupportedSecretBackend(
                reference.backend.clone(),
            ));
        }

        if std::env::var(&reference.locator).is_err() {
            return Err(ProviderError::SecretNotFound(reference.locator.clone()));
        }

        // SAFETY: EnvironmentSecretStore removes process environment variables for secret management.
        unsafe {
            std::env::remove_var(&reference.locator);
        }
        Ok(())
    }
}

/// Thread-safe in-memory store for testing and non-interactive environments.
#[derive(Clone, Debug, Default)]
pub struct InMemorySecretStore {
    storage: Arc<RwLock<HashMap<String, String>>>,
}

impl InMemorySecretStore {
    /// Creates a new empty in-memory secret store.
    pub fn new() -> Self {
        Self {
            storage: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Creates an in-memory store using shared existing storage.
    pub fn from_map(storage: Arc<RwLock<HashMap<String, String>>>) -> Self {
        Self { storage }
    }

    /// Returns a reference to the underlying shared storage.
    pub fn storage(&self) -> &Arc<RwLock<HashMap<String, String>>> {
        &self.storage
    }

    /// Checks if a locator exists in the in-memory store.
    pub fn contains_key(&self, locator: &str) -> Result<bool, ProviderError> {
        let map = self
            .storage
            .read()
            .map_err(|e| ProviderError::SecretStoreError(format!("Lock poisoned: {e}")))?;
        Ok(map.contains_key(locator))
    }

    /// Inserts a secret value directly into the store.
    pub fn insert(&self, reference: &SecretReference, value: impl Into<String>) {
        let _ = self.set_secret(reference, &value.into());
    }

    /// Builder helper to insert a secret and return self.
    pub fn with_secret(self, reference: SecretReference, value: impl Into<String>) -> Self {
        self.insert(&reference, value);
        self
    }
}

impl SecretStore for InMemorySecretStore {
    fn get_secret(&self, reference: &SecretReference) -> Result<String, ProviderError> {
        reference.validate()?;
        let map = self
            .storage
            .read()
            .map_err(|e| ProviderError::SecretStoreError(format!("Lock poisoned: {e}")))?;
        map.get(&reference.locator)
            .cloned()
            .ok_or_else(|| ProviderError::SecretNotFound(reference.locator.clone()))
    }

    fn set_secret(&self, reference: &SecretReference, value: &str) -> Result<(), ProviderError> {
        reference.validate()?;
        let mut map = self
            .storage
            .write()
            .map_err(|e| ProviderError::SecretStoreError(format!("Lock poisoned: {e}")))?;
        map.insert(reference.locator.clone(), value.to_string());
        Ok(())
    }

    fn delete_secret(&self, reference: &SecretReference) -> Result<(), ProviderError> {
        reference.validate()?;
        let mut map = self
            .storage
            .write()
            .map_err(|e| ProviderError::SecretStoreError(format!("Lock poisoned: {e}")))?;
        if map.remove(&reference.locator).is_none() {
            return Err(ProviderError::SecretNotFound(reference.locator.clone()));
        }
        Ok(())
    }
}

/// Composite secret store that routes operations to backends based on `reference.backend`.
#[derive(Clone)]
pub struct CompositeSecretStore {
    os_store: Arc<dyn SecretStore>,
    env_store: Arc<dyn SecretStore>,
    external_store: Option<Arc<dyn SecretStore>>,
}

/// Alias for `CompositeSecretStore`.
pub type DefaultSecretResolver = CompositeSecretStore;

impl Default for CompositeSecretStore {
    fn default() -> Self {
        Self::new(
            Arc::new(OsKeyringSecretStore::new()),
            Arc::new(EnvironmentSecretStore::new()),
            None,
        )
    }
}

impl fmt::Debug for CompositeSecretStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompositeSecretStore")
            .field("has_os_store", &true)
            .field("has_env_store", &true)
            .field("has_external_store", &self.external_store.is_some())
            .finish()
    }
}

impl CompositeSecretStore {
    /// Creates a composite secret store with explicit backends.
    pub fn new(
        os_store: Arc<dyn SecretStore>,
        env_store: Arc<dyn SecretStore>,
        external_store: Option<Arc<dyn SecretStore>>,
    ) -> Self {
        Self {
            os_store,
            env_store,
            external_store,
        }
    }

    /// Convenience constructor using in-memory backends for automated tests and headless environments.
    pub fn in_memory() -> Self {
        let mem = Arc::new(InMemorySecretStore::new());
        Self {
            os_store: mem.clone(),
            env_store: mem,
            external_store: None,
        }
    }

    /// Configures the OS store backend.
    pub fn with_os_store(mut self, os_store: Arc<dyn SecretStore>) -> Self {
        self.os_store = os_store;
        self
    }

    /// Configures the environment store backend.
    pub fn with_env_store(mut self, env_store: Arc<dyn SecretStore>) -> Self {
        self.env_store = env_store;
        self
    }

    /// Configures the external store backend.
    pub fn with_external_store(mut self, external_store: Arc<dyn SecretStore>) -> Self {
        self.external_store = Some(external_store);
        self
    }

    /// Returns a reference to the configured OS credential backend.
    pub fn os_store(&self) -> &Arc<dyn SecretStore> {
        &self.os_store
    }

    /// Returns a reference to the configured environment backend.
    pub fn env_store(&self) -> &Arc<dyn SecretStore> {
        &self.env_store
    }

    /// Returns a reference to the optional external backend.
    pub fn external_store(&self) -> Option<&Arc<dyn SecretStore>> {
        self.external_store.as_ref()
    }
}

impl SecretStore for CompositeSecretStore {
    fn get_secret(&self, reference: &SecretReference) -> Result<String, ProviderError> {
        reference.validate()?;
        match reference.backend {
            SecretBackend::OsCredentialStore => self.os_store.get_secret(reference),
            SecretBackend::EnvironmentVariable => self.env_store.get_secret(reference),
            SecretBackend::External => match &self.external_store {
                Some(store) => store.get_secret(reference),
                None => Err(ProviderError::UnsupportedSecretBackend(
                    SecretBackend::External,
                )),
            },
        }
    }

    fn set_secret(&self, reference: &SecretReference, value: &str) -> Result<(), ProviderError> {
        reference.validate()?;
        match reference.backend {
            SecretBackend::OsCredentialStore => self.os_store.set_secret(reference, value),
            SecretBackend::EnvironmentVariable => self.env_store.set_secret(reference, value),
            SecretBackend::External => match &self.external_store {
                Some(store) => store.set_secret(reference, value),
                None => Err(ProviderError::UnsupportedSecretBackend(
                    SecretBackend::External,
                )),
            },
        }
    }

    fn delete_secret(&self, reference: &SecretReference) -> Result<(), ProviderError> {
        reference.validate()?;
        match reference.backend {
            SecretBackend::OsCredentialStore => self.os_store.delete_secret(reference),
            SecretBackend::EnvironmentVariable => self.env_store.delete_secret(reference),
            SecretBackend::External => match &self.external_store {
                Some(store) => store.delete_secret(reference),
                None => Err(ProviderError::UnsupportedSecretBackend(
                    SecretBackend::External,
                )),
            },
        }
    }
}

/// Maps a keyring error into `ProviderError`, ensuring raw secrets are never leaked.
fn map_keyring_error(err: keyring::Error, reference: &SecretReference) -> ProviderError {
    match err {
        keyring::Error::NoEntry => ProviderError::SecretNotFound(reference.locator.clone()),
        keyring::Error::BadEncoding(_) => ProviderError::SecretStoreError(
            "Stored credential contains invalid UTF-8 bytes".to_string(),
        ),
        keyring::Error::TooLong(attr, max) => ProviderError::SecretStoreError(format!(
            "Keyring attribute '{attr}' exceeded limit of {max}"
        )),
        keyring::Error::Invalid(attr, reason) => {
            ProviderError::SecretStoreError(format!("Invalid keyring attribute '{attr}': {reason}"))
        }
        keyring::Error::Ambiguous(_) => ProviderError::SecretStoreError(
            "Multiple matching keyring credentials found".to_string(),
        ),
        keyring::Error::NoStorageAccess(e) => {
            ProviderError::SecretStoreError(format!("Keyring storage access denied: {e}"))
        }
        keyring::Error::PlatformFailure(e) => {
            ProviderError::SecretStoreError(format!("Keyring platform failure: {e}"))
        }
        _ => ProviderError::SecretStoreError("Keyring operation failed".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_os_keyring_store_defaults() {
        let store = OsKeyringSecretStore::new();
        assert_eq!(store.service_name(), DEFAULT_KEYRING_SERVICE);

        let custom = OsKeyringSecretStore::with_service("custom-service");
        assert_eq!(custom.service_name(), "custom-service");
    }

    #[test]
    fn test_os_keyring_rejects_non_os_backend() {
        let store = OsKeyringSecretStore::new();
        let env_ref = SecretReference::env("SOME_VAR").unwrap();
        let res = store.get_secret(&env_ref);
        assert_eq!(
            res.unwrap_err(),
            ProviderError::UnsupportedSecretBackend(SecretBackend::EnvironmentVariable)
        );
    }

    #[test]
    fn test_environment_store_rejects_non_env_backend() {
        let store = EnvironmentSecretStore::new();
        let os_ref = SecretReference::os_store("some-locator").unwrap();
        let res = store.get_secret(&os_ref);
        assert_eq!(
            res.unwrap_err(),
            ProviderError::UnsupportedSecretBackend(SecretBackend::OsCredentialStore)
        );
    }

    #[test]
    fn test_composite_debug() {
        let comp = CompositeSecretStore::in_memory();
        let dbg = format!("{comp:?}");
        assert!(dbg.contains("CompositeSecretStore"));
        assert!(dbg.contains("has_os_store: true"));
    }
}
