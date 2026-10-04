use agent_studios_codex_bridge::CodexBridgeError;
use agent_studios_provider::error::ProviderError;
use agent_studios_provider::id::{ModelId, ProviderInstanceId};
use agent_studios_runtime_transport::TransportError;
use thiserror::Error;

/// Errors arising during runtime session preparation and assembly.
#[derive(Debug, Error)]
pub enum RuntimeSessionError {
    #[error("Provider instance not found: {0}")]
    ProviderInstanceNotFound(ProviderInstanceId),

    #[error("Provider instance is disabled: {0}")]
    ProviderInstanceDisabled(ProviderInstanceId),

    #[error("No models registered for provider instance: {0}")]
    NoModelsForProvider(ProviderInstanceId),

    #[error("Model '{model_id}' not found for provider instance '{provider_instance_id}'")]
    ModelNotFound {
        model_id: ModelId,
        provider_instance_id: ProviderInstanceId,
    },

    #[error("Unsupported protocol: {0}")]
    UnsupportedProtocol(String),

    #[error("Model metadata limit out of range: {0}")]
    ModelMetadataOutOfRange(String),

    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("Codex bridge error: {0}")]
    CodexBridge(#[from] CodexBridgeError),

    #[error("Provider catalog error: {0}")]
    Provider(#[from] ProviderError),

    #[error("Session configuration error: {0}")]
    Config(String),
}
