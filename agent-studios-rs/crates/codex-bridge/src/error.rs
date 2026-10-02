use agent_studios_provider::{
    ModelRef, ProtocolFamily, ProviderError, ProviderInstanceId, SecretBackend,
};
use thiserror::Error;

/// Typed error enum for Codex Responses bridge operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CodexBridgeError {
    #[error("Unknown model reference: {0:?}")]
    UnknownModel(ModelRef),

    #[error("Unknown provider instance: {0}")]
    UnknownProviderInstance(ProviderInstanceId),

    #[error("Provider instance '{0}' is disabled")]
    ProviderInstanceDisabled(ProviderInstanceId),

    #[error(
        "Unsupported protocol for Codex Responses bridge: got '{protocol}', expected '{expected}'"
    )]
    UnsupportedProtocol {
        protocol: ProtocolFamily,
        expected: ProtocolFamily,
    },

    #[error("Unsupported authentication scheme: {0}")]
    UnsupportedAuthentication(String),

    #[error(
        "Unsupported secret backend '{backend}' for {scheme} in Codex bridge (only EnvironmentVariable supported)"
    )]
    UnsupportedSecretBackend {
        backend: SecretBackend,
        scheme: String,
    },

    #[error("Secret query parameter authentication is unsupported by Codex Responses bridge")]
    SecretQueryParameterAuthUnsupported,

    #[error("Unsupported model capability '{capability}': {reason}")]
    UnsupportedModelCapability {
        capability: &'static str,
        reason: &'static str,
    },

    #[error("Invalid model catalog URL '{url}': {reason}")]
    InvalidCatalogUrl { url: String, reason: String },

    #[error("Invalid header name '{name}': {reason}")]
    InvalidHeaderName { name: String, reason: String },

    #[error("Codex provider validation failed: {0}")]
    CodexProviderValidation(String),

    #[error("Provider error: {0}")]
    Provider(#[from] ProviderError),
}
