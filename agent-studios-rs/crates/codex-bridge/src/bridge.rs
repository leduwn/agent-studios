use std::collections::HashMap;
use url::Url;

use agent_studios_provider::{
    AuthenticationScheme, CapabilitySupport, ModelRef, ProtocolFamily, ProviderCatalog,
    ProviderError, ProviderInstanceId, SecretBackend,
};
use codex_model_provider_info::{ModelProviderInfo, WireApi};
use codex_utils_redacted_string::RedactedString;

use crate::binding::CodexResponsesBinding;
use crate::error::CodexBridgeError;
use crate::options::CodexBridgeOptions;
use crate::report::CodexCompatibilityReport;

/// Forbidden header names in static_headers that carry credentials.
const FORBIDDEN_STATIC_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "api-key",
    "x-auth-token",
    "bearer",
];

/// Computes the deterministic Codex provider key for a given provider instance.
///
/// Format: `agent-studios-<provider-instance-uuid>`
pub fn deterministic_codex_provider_key(instance_id: &ProviderInstanceId) -> String {
    format!("agent-studios-{}", instance_id)
}

/// Validates an HTTP header name according to RFC 7230 token rules.
pub fn validate_header_name(name: &str) -> Result<(), CodexBridgeError> {
    if name.is_empty() {
        return Err(CodexBridgeError::InvalidHeaderName {
            name: name.to_string(),
            reason: "Header name cannot be empty".to_string(),
        });
    }
    for b in name.bytes() {
        match b {
            b'a'..=b'z'
            | b'A'..=b'Z'
            | b'0'..=b'9'
            | b'!'
            | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~' => {}
            _ => {
                return Err(CodexBridgeError::InvalidHeaderName {
                    name: name.to_string(),
                    reason: format!("Header name contains invalid character: {b:02x}"),
                });
            }
        }
    }
    Ok(())
}

/// Deterministically resolves a model catalog URL against a base URL.
///
/// Preserves absolute `http(s)` URLs. For relative paths or absolute paths (`/models`),
/// joins against the base URL. Rejects userinfo / credentials in URLs.
pub fn resolve_catalog_url(
    base_url: &str,
    catalog_path_or_url: &str,
) -> Result<String, CodexBridgeError> {
    let trimmed = catalog_path_or_url.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let parsed = Url::parse(trimmed).map_err(|e| CodexBridgeError::InvalidCatalogUrl {
            url: trimmed.to_string(),
            reason: e.to_string(),
        })?;
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(CodexBridgeError::InvalidCatalogUrl {
                url: trimmed.to_string(),
                reason: "URL must not contain credentials".to_string(),
            });
        }
        return Ok(parsed.to_string());
    }

    let base = Url::parse(base_url).map_err(|e| CodexBridgeError::InvalidCatalogUrl {
        url: base_url.to_string(),
        reason: e.to_string(),
    })?;

    let joined = if trimmed.starts_with('/') {
        let mut origin = base;
        origin.set_path(trimmed);
        origin.set_query(None);
        origin.set_fragment(None);
        origin
    } else {
        let mut joined = base;
        let current_path = joined.path().trim_end_matches('/');
        joined.set_path(&format!("{current_path}/{trimmed}"));
        joined
    };

    if !joined.username().is_empty() || joined.password().is_some() {
        return Err(CodexBridgeError::InvalidCatalogUrl {
            url: joined.to_string(),
            reason: "URL must not contain credentials".to_string(),
        });
    }

    Ok(joined.to_string())
}

/// Core bridge adapting Agent Studios Provider Core to Codex Responses runtime.
pub struct CodexProviderBridge;

impl CodexProviderBridge {
    /// Resolves an Agent Studios `ModelRef` from a `ProviderCatalog` into a `CodexResponsesBinding`.
    ///
    /// Invariants enforced:
    /// - Model and ProviderInstance must exist and instance must be enabled.
    /// - Protocol must be `ProtocolFamily::OpenAiResponses` (Codex runtime is Responses-only).
    /// - Model must not have `Unsupported` tool_calling or streaming.
    /// - Capabilities in `Unknown` status are tracked in `CodexCompatibilityReport`.
    /// - Authentication secrets must be environment variable references (zero plaintext resolution).
    /// - Static headers must not include sensitive credential headers.
    /// - URLs must not contain embedded userinfo/credentials.
    /// - Provider key is deterministically generated (`agent-studios-<uuid>`).
    pub fn resolve(
        catalog: &ProviderCatalog,
        model_ref: &ModelRef,
        options: CodexBridgeOptions,
    ) -> Result<CodexResponsesBinding, CodexBridgeError> {
        let model = catalog
            .resolve_model(model_ref)
            .ok_or_else(|| CodexBridgeError::UnknownModel(model_ref.clone()))?;

        let instance = catalog
            .get_provider_instance(&model_ref.provider_instance_id)
            .ok_or(CodexBridgeError::UnknownProviderInstance(
                model_ref.provider_instance_id,
            ))?;

        if !instance.enabled {
            return Err(CodexBridgeError::ProviderInstanceDisabled(
                model_ref.provider_instance_id,
            ));
        }

        // Protocol enforcement: upstream Codex runtime natively only supports Responses API.
        if instance.protocol != ProtocolFamily::OpenAiResponses {
            return Err(CodexBridgeError::UnsupportedProtocol {
                protocol: instance.protocol.clone(),
                expected: ProtocolFamily::OpenAiResponses,
            });
        }

        // Capability safety enforcement
        if instance.protocol == ProtocolFamily::OpenAiResponses {
            if model.capabilities.tool_calling == CapabilitySupport::Unsupported {
                return Err(CodexBridgeError::UnsupportedModelCapability {
                    capability: "tool_calling",
                    reason: "Codex runtime requires tool calling capability",
                });
            }
            if model.capabilities.streaming == CapabilitySupport::Unsupported {
                return Err(CodexBridgeError::UnsupportedModelCapability {
                    capability: "streaming",
                    reason: "Codex runtime requires streaming capability",
                });
            }
        }

        let mut unverified = Vec::new();
        if model.capabilities.tool_calling == CapabilitySupport::Unknown {
            unverified.push("tool_calling".to_string());
        }
        if model.capabilities.streaming == CapabilitySupport::Unknown {
            unverified.push("streaming".to_string());
        }
        let compatibility_report = CodexCompatibilityReport {
            unverified_capabilities: unverified,
        };

        // Validate base_url
        let base_url_parsed = Url::parse(&instance.endpoint.base_url).map_err(|e| {
            CodexBridgeError::Provider(ProviderError::InvalidEndpoint(format!(
                "Invalid base_url: {e}"
            )))
        })?;
        if !base_url_parsed.username().is_empty() || base_url_parsed.password().is_some() {
            return Err(CodexBridgeError::Provider(ProviderError::InvalidEndpoint(
                "base_url must not contain username or password credentials".to_string(),
            )));
        }

        // Resolve model catalog URL if set
        let model_catalog_url = if let Some(cat_url) = &instance.endpoint.model_catalog_url {
            Some(RedactedString::from(resolve_catalog_url(
                &instance.endpoint.base_url,
                cat_url,
            )?))
        } else {
            None
        };

        // Static headers validation and mapping
        let http_headers = if instance.endpoint.static_headers.is_empty() {
            None
        } else {
            let mut map = HashMap::new();
            for (k, v) in &instance.endpoint.static_headers {
                let lower_k = k.to_ascii_lowercase();
                if FORBIDDEN_STATIC_HEADERS.contains(&lower_k.as_str()) {
                    return Err(CodexBridgeError::Provider(ProviderError::InvalidEndpoint(
                        format!("Static header '{k}' is forbidden as it may leak credentials"),
                    )));
                }
                validate_header_name(k)?;
                map.insert(k.clone(), RedactedString::from(v.as_str()));
            }
            Some(map)
        };

        // Query parameters mapping
        let query_params = if instance.endpoint.query_params.is_empty() {
            None
        } else {
            let mut map = HashMap::new();
            for (k, v) in &instance.endpoint.query_params {
                map.insert(k.clone(), RedactedString::from(v.as_str()));
            }
            Some(map)
        };

        // Authentication mapping: zero secret resolution
        let (env_key, env_http_headers) = match &instance.authentication {
            AuthenticationScheme::BearerToken { secret } => {
                if secret.backend != SecretBackend::EnvironmentVariable {
                    return Err(CodexBridgeError::UnsupportedSecretBackend {
                        backend: secret.backend.clone(),
                        scheme: "BearerToken".to_string(),
                    });
                }
                (Some(secret.locator.clone()), None)
            }
            AuthenticationScheme::OAuthToken { secret } => {
                if secret.backend != SecretBackend::EnvironmentVariable {
                    return Err(CodexBridgeError::UnsupportedSecretBackend {
                        backend: secret.backend.clone(),
                        scheme: "OAuthToken".to_string(),
                    });
                }
                (Some(secret.locator.clone()), None)
            }
            AuthenticationScheme::ApiKeyHeader {
                header_name,
                secret,
            } => {
                if secret.backend != SecretBackend::EnvironmentVariable {
                    return Err(CodexBridgeError::UnsupportedSecretBackend {
                        backend: secret.backend.clone(),
                        scheme: "ApiKeyHeader".to_string(),
                    });
                }
                validate_header_name(header_name)?;
                let mut map = HashMap::new();
                map.insert(header_name.clone(), secret.locator.clone());
                (None, Some(map))
            }
            AuthenticationScheme::QueryParameter { .. } => {
                return Err(CodexBridgeError::SecretQueryParameterAuthUnsupported);
            }
            AuthenticationScheme::AwsSigV4 { .. } => {
                return Err(CodexBridgeError::UnsupportedAuthentication(
                    "AwsSigV4 authentication is not supported by Codex Responses bridge"
                        .to_string(),
                ));
            }
            AuthenticationScheme::None => (None, None),
        };

        let codex_provider_key = deterministic_codex_provider_key(&instance.id);

        let provider_info = ModelProviderInfo {
            name: instance.display_name.clone(),
            base_url: Some(instance.endpoint.base_url.clone()),
            model_catalog_url,
            env_key,
            env_key_instructions: None,
            experimental_bearer_token: None,
            auth: None,
            gateway_oauth: None,
            aws: None,
            wire_api: WireApi::Responses,
            query_params,
            http_headers,
            env_http_headers,
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
            websocket_connect_timeout_ms: None,
            requires_openai_auth: false,
            supports_websockets: options.supports_websockets,
            supports_standalone_web_search: options.supports_standalone_web_search,
            include_internal_metadata: options.include_internal_metadata,
        };

        // Validate resulting ModelProviderInfo with Codex's own validator
        provider_info
            .validate()
            .map_err(CodexBridgeError::CodexProviderValidation)?;

        Ok(CodexResponsesBinding {
            model_ref: model_ref.clone(),
            provider_instance_id: instance.id,
            codex_provider_key,
            model_id: model.id.as_str().to_string(),
            provider_info,
            compatibility_report,
        })
    }
}
