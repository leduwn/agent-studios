use agent_studios_provider::{ModelRef, ProviderCatalog, ProviderId};
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
pub fn map_model_ref_to_opencode(
    catalog: &ProviderCatalog,
    model_ref: &ModelRef,
) -> Result<OpenCodeModelBinding, OpenCodeError> {
    let instance = catalog.get_provider_instance(&model_ref.provider_instance_id);

    let provider_name = match instance {
        Some(inst) => resolve_opencode_provider_name(&inst.provider_id),
        None => "anthropic".to_string(),
    };

    let model_name = model_ref.model_id.as_str().to_string();

    let auth_env_var = match provider_name.as_str() {
        "anthropic" => Some("ANTHROPIC_API_KEY".to_string()),
        "openai" => Some("OPENAI_API_KEY".to_string()),
        "google" => Some("GEMINI_API_KEY".to_string()),
        "openrouter" => Some("OPENROUTER_API_KEY".to_string()),
        "ollama" => None,
        _ => None,
    };

    let base_url = instance.map(|inst| inst.endpoint.base_url.clone());

    Ok(OpenCodeModelBinding {
        provider_name,
        model_name,
        base_url,
        auth_env_var,
    })
}

/// Maps ProviderId to OpenCode's recognized provider identifiers.
pub fn resolve_opencode_provider_name(provider_id: &ProviderId) -> String {
    let id_str = provider_id.as_str().to_lowercase();
    if id_str.contains("anthropic") || id_str.contains("claude") {
        "anthropic".to_string()
    } else if id_str.contains("openai") || id_str.contains("codex") {
        "openai".to_string()
    } else if id_str.contains("google") || id_str.contains("gemini") {
        "google".to_string()
    } else if id_str.contains("openrouter") {
        "openrouter".to_string()
    } else if id_str.contains("ollama") {
        "ollama".to_string()
    } else {
        id_str
    }
}
