use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock;

use agent_studios_provider::secret::{SecretBackend, SecretReference};
use async_trait::async_trait;
use serde::{Serialize, Serializer};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::TransportError;

/// A secure container for secret strings that protects against unintentional exposure.
///
/// - Redacts debug and display representation (`"[REDACTED]"`).
/// - Redacts serialization output (`"[REDACTED]"`).
/// - Zeroizes heap memory upon drop.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct SecretString {
    inner: String,
}

impl SecretString {
    /// Wraps a string in a secure container.
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            inner: secret.into(),
        }
    }

    /// Exposes the inner secret string for authorized usage (e.g. creating auth headers).
    pub fn expose_secret(&self) -> &str {
        &self.inner
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl Serialize for SecretString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str("[REDACTED]")
    }
}

/// Asynchronous secret resolver for provider credentials.
#[async_trait]
pub trait SecretResolver: Send + Sync + fmt::Debug {
    /// Resolves a secret reference to a protected `SecretString`.
    async fn resolve(&self, reference: &SecretReference) -> Result<SecretString, TransportError>;
}

/// Resolver that retrieves credentials from process environment variables.
#[derive(Debug, Default)]
pub struct EnvSecretResolver;

#[async_trait]
impl SecretResolver for EnvSecretResolver {
    async fn resolve(&self, reference: &SecretReference) -> Result<SecretString, TransportError> {
        match reference.backend {
            SecretBackend::EnvironmentVariable => match std::env::var(&reference.locator) {
                Ok(val) => Ok(SecretString::new(val)),
                Err(_) => Err(TransportError::SecretNotFound {
                    backend: reference.backend.to_string(),
                    locator: reference.locator.clone(),
                }),
            },
            _ => Err(TransportError::SecretNotFound {
                backend: reference.backend.to_string(),
                locator: reference.locator.clone(),
            }),
        }
    }
}

/// In-memory secret resolver for testing and scoped runtime credentials.
#[derive(Debug, Default)]
pub struct InMemorySecretResolver {
    secrets: RwLock<HashMap<SecretReference, SecretString>>,
}

impl InMemorySecretResolver {
    pub fn new() -> Self {
        Self {
            secrets: RwLock::new(HashMap::new()),
        }
    }

    pub fn insert(&self, reference: SecretReference, secret: impl Into<String>) {
        let mut map = self.secrets.write().unwrap();
        map.insert(reference, SecretString::new(secret));
    }

    pub fn with_env_secret(self, var_name: impl Into<String>, secret: impl Into<String>) -> Self {
        let reference = SecretReference {
            backend: SecretBackend::EnvironmentVariable,
            locator: var_name.into(),
        };
        self.insert(reference, secret);
        self
    }
}

#[async_trait]
impl SecretResolver for InMemorySecretResolver {
    async fn resolve(&self, reference: &SecretReference) -> Result<SecretString, TransportError> {
        let map = self.secrets.read().unwrap();
        if let Some(secret) = map.get(reference) {
            Ok(secret.clone())
        } else {
            Err(TransportError::SecretNotFound {
                backend: reference.backend.to_string(),
                locator: reference.locator.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_string_redaction() {
        let secret = SecretString::new("super-secret-key-12345");
        assert_eq!(format!("{secret:?}"), "[REDACTED]");
        assert_eq!(format!("{secret}"), "[REDACTED]");

        let json = serde_json::to_string(&secret).unwrap();
        assert_eq!(json, "\"[REDACTED]\"");

        assert_eq!(secret.expose_secret(), "super-secret-key-12345");
    }

    #[tokio::test]
    async fn test_in_memory_secret_resolver() {
        let resolver = InMemorySecretResolver::new().with_env_secret("API_KEY", "test-val-abc");

        let ref_ok = SecretReference {
            backend: SecretBackend::EnvironmentVariable,
            locator: "API_KEY".to_string(),
        };
        let res = resolver.resolve(&ref_ok).await.unwrap();
        assert_eq!(res.expose_secret(), "test-val-abc");

        let ref_missing = SecretReference {
            backend: SecretBackend::EnvironmentVariable,
            locator: "NON_EXISTENT".to_string(),
        };
        assert!(resolver.resolve(&ref_missing).await.is_err());
    }

    #[tokio::test]
    async fn test_env_secret_resolver() {
        unsafe {
            std::env::set_var("TRANSPORT_TEST_SECRET", "env-resolved-secret");
        }
        let resolver = EnvSecretResolver;

        let ref_ok = SecretReference {
            backend: SecretBackend::EnvironmentVariable,
            locator: "TRANSPORT_TEST_SECRET".to_string(),
        };
        let res = resolver.resolve(&ref_ok).await.unwrap();
        assert_eq!(res.expose_secret(), "env-resolved-secret");
        unsafe {
            std::env::remove_var("TRANSPORT_TEST_SECRET");
        }
    }
}
