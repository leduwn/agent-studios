use crate::id::{ModelId, ProviderId, ProviderInstanceId};
use crate::protocol::ProtocolFamily;
use thiserror::Error;

/// Root error type for provider metadata, registry, and configuration operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProviderError {
    #[error("Invalid provider ID: {0}")]
    InvalidProviderId(String),

    #[error("Invalid model ID: {0}")]
    InvalidModelId(String),

    #[error("Invalid provider instance ID: {0}")]
    InvalidProviderInstanceId(String),

    #[error("Duplicate provider definition: {0}")]
    DuplicateProviderDefinition(ProviderId),

    #[error("Unknown provider definition: {0}")]
    UnknownProviderDefinition(ProviderId),

    #[error("Provider definition {provider_id} is in use by {instance_count} instance(s)")]
    ProviderDefinitionInUse {
        provider_id: ProviderId,
        instance_count: usize,
    },

    #[error("Duplicate provider instance: {0}")]
    DuplicateProviderInstance(ProviderInstanceId),

    #[error("Unknown provider instance: {0}")]
    UnknownProviderInstance(ProviderInstanceId),

    #[error("Provider instance {instance_id} is in use by {model_count} model(s)")]
    ProviderInstanceInUse {
        instance_id: ProviderInstanceId,
        model_count: usize,
    },

    #[error("Protocol {protocol} is not supported by provider definition {provider_id}")]
    UnsupportedProtocol {
        provider_id: ProviderId,
        protocol: ProtocolFamily,
    },

    #[error("Invalid endpoint: {0}")]
    InvalidEndpoint(String),

    #[error("Embedded userinfo/credentials in URL are strictly forbidden")]
    EmbeddedCredentialsForbidden,

    #[error("Sensitive header '{0}' is forbidden in static_headers; use AuthenticationScheme")]
    SensitiveStaticHeaderForbidden(String),

    #[error("Invalid authentication configuration: {0}")]
    InvalidAuthentication(String),

    #[error("Duplicate model '{model_id}' under instance '{instance_id}'")]
    DuplicateModel {
        instance_id: ProviderInstanceId,
        model_id: ModelId,
    },

    #[error("Unknown model '{model_id}' under instance '{instance_id}'")]
    UnknownModel {
        instance_id: ProviderInstanceId,
        model_id: ModelId,
    },

    #[error("Invalid model limits: {0}")]
    InvalidModelLimits(String),

    #[error("Invalid catalog snapshot: {0}")]
    InvalidSnapshot(String),

    #[error("Unsupported catalog schema version: {version} (supported: {supported})")]
    UnsupportedSchemaVersion { version: u32, supported: u32 },
}
