use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::ProviderError;

/// Storage backend for a secret reference.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretBackend {
    /// Read from process environment variable.
    EnvironmentVariable,
    /// Read from OS credential store (e.g. Windows Credential Manager, SecretService, Keychain).
    OsCredentialStore,
    /// External provider or plugin secret manager.
    External,
}

impl fmt::Display for SecretBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnvironmentVariable => write!(f, "environment_variable"),
            Self::OsCredentialStore => write!(f, "os_credential_store"),
            Self::External => write!(f, "external"),
        }
    }
}

/// An opaque reference to a secret. Never contains raw secret values.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SecretReference {
    pub backend: SecretBackend,
    pub locator: String,
}

impl SecretReference {
    pub fn new(backend: SecretBackend, locator: impl Into<String>) -> Result<Self, ProviderError> {
        let locator = locator.into();
        let trimmed = locator.trim();
        if trimmed.is_empty() {
            return Err(ProviderError::InvalidAuthentication(
                "SecretReference locator cannot be empty".to_string(),
            ));
        }
        Ok(Self {
            backend,
            locator: trimmed.to_string(),
        })
    }

    /// Helper to create an environment variable secret reference.
    pub fn env(var_name: impl Into<String>) -> Result<Self, ProviderError> {
        Self::new(SecretBackend::EnvironmentVariable, var_name)
    }

    /// Helper to create an OS credential store secret reference.
    pub fn os_store(target: impl Into<String>) -> Result<Self, ProviderError> {
        Self::new(SecretBackend::OsCredentialStore, target)
    }

    /// Helper to create an external secret reference.
    pub fn external(locator: impl Into<String>) -> Result<Self, ProviderError> {
        Self::new(SecretBackend::External, locator)
    }

    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.locator.trim().is_empty() {
            return Err(ProviderError::InvalidAuthentication(
                "SecretReference locator cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

impl fmt::Display for SecretReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.backend, self.locator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_ref_valid() {
        let s = SecretReference::env("OPENAI_API_KEY").unwrap();
        assert_eq!(s.backend, SecretBackend::EnvironmentVariable);
        assert_eq!(s.locator, "OPENAI_API_KEY");
        assert_eq!(s.to_string(), "environment_variable:OPENAI_API_KEY");
    }

    #[test]
    fn test_secret_ref_empty_rejected() {
        assert!(SecretReference::env("").is_err());
        assert!(SecretReference::env("   ").is_err());
    }

    #[test]
    fn test_secret_ref_serde() {
        let s = SecretReference::os_store("agent-studios/openai/instance-1").unwrap();
        let json = serde_json::to_string(&s).unwrap();
        let de: SecretReference = serde_json::from_str(&json).unwrap();
        assert_eq!(s, de);
    }
}
