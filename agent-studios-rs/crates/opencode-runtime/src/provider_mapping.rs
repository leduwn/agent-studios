use agent_studios_provider::{
    AuthenticationScheme, ModelRef, ProtocolFamily, ProviderCatalog, ProviderId, SecretBackend,
};
use serde::{Deserialize, Serialize};

use crate::error::OpenCodeError;

/// Resolved OpenCode model and provider parameters.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenCodeModelBinding {
    pub provider_name: String,
    pub model_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_env_var: Option<String>,
}

/// Maps an Agent Studios `ModelRef` and `ProviderCatalog` into OpenCode provider and model bindings
/// without mutating global or host-level configuration files.
/// Strictly requires the provider instance in the catalog; fails closed without default fallbacks.
pub fn map_model_ref_to_opencode(
    catalog: &ProviderCatalog,
    model_ref: &ModelRef,
) -> Result<OpenCodeModelBinding, OpenCodeError> {
    let instance = catalog
        .get_provider_instance(&model_ref.provider_instance_id)
        .ok_or_else(|| {
            OpenCodeError::InvalidConfiguration(format!(
                "Provider instance '{}' not found in catalog for model '{}'",
                model_ref.provider_instance_id,
                model_ref.model_id.as_str()
            ))
        })?;

    let provider_name =
        resolve_opencode_provider_name(&instance.provider_id, Some(&instance.protocol));
    let model_name = model_ref.model_id.as_str().to_string();

    let auth_env_var = match &instance.authentication {
        AuthenticationScheme::BearerToken { secret }
        | AuthenticationScheme::ApiKeyHeader { secret, .. }
        | AuthenticationScheme::QueryParameter { secret, .. }
        | AuthenticationScheme::OAuthToken { secret } => {
            if secret.backend == SecretBackend::EnvironmentVariable {
                Some(secret.locator.clone())
            } else {
                None
            }
        }
        AuthenticationScheme::AwsSigV4 { credential, .. } => {
            if credential.backend == SecretBackend::EnvironmentVariable {
                Some(credential.locator.clone())
            } else {
                None
            }
        }
        AuthenticationScheme::None => None,
    };

    let base_url = Some(instance.endpoint.base_url.clone());

    Ok(OpenCodeModelBinding {
        provider_name,
        model_name,
        base_url,
        auth_env_var,
    })
}

/// Maps ProviderId to OpenCode's recognized provider identifiers.
///
/// Standard known identifiers are matched exactly. Non-standard identifiers
/// are mapped via their explicit `ProtocolFamily`, or fall back to the exact
/// sanitized provider id string.
pub fn resolve_opencode_provider_name(
    provider_id: &ProviderId,
    protocol: Option<&ProtocolFamily>,
) -> String {
    let id_str = provider_id.as_str().trim().to_lowercase();
    match id_str.as_str() {
        "anthropic" => "anthropic".to_string(),
        "openai" => "openai".to_string(),
        "google" => "google".to_string(),
        "openrouter" => "openrouter".to_string(),
        "ollama" => "ollama".to_string(),
        "lmstudio" => "lmstudio".to_string(),
        _ => {
            if let Some(proto) = protocol {
                match proto {
                    ProtocolFamily::AnthropicMessages => "anthropic".to_string(),
                    ProtocolFamily::OpenAiResponses | ProtocolFamily::OpenAiChatCompletions => {
                        "openai".to_string()
                    }
                    ProtocolFamily::GeminiGenerateContent => "google".to_string(),
                    _ => id_str,
                }
            } else {
                id_str
            }
        }
    }
}
