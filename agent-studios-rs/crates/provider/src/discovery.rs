use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

use crate::auth::AuthenticationScheme;
use crate::capabilities::{CapabilitySupport, ModelCapabilities};
use crate::id::{ModelId, ProviderInstanceId};
use crate::instance::ProviderInstance;
use crate::model::{ModelDescriptor, ModelLimits, ModelMetadataSource};
use crate::protocol::ProtocolFamily;
use crate::store::SecretStore;

/// Configuration options for dynamic model discovery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveryConfig {
    pub timeout: Duration,
    pub cache_ttl: Duration,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(15),
            cache_ttl: Duration::from_secs(300),
        }
    }
}

/// Errors returned during dynamic model discovery operations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DiscoveryError {
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Rate limited (retry after {retry_after_secs:?}s): {message}")]
    RateLimited {
        retry_after_secs: Option<u64>,
        message: String,
    },

    #[error("Network timeout: {0}")]
    NetworkTimeout(String),

    #[error("Endpoint unreachable: {0}")]
    EndpointUnreachable(String),

    #[error("Invalid payload: {0}")]
    InvalidPayload(String),

    #[error("Secret resolution failed: {0}")]
    SecretResolutionFailed(String),

    #[error("Invalid endpoint: {0}")]
    InvalidEndpoint(String),

    #[error("HTTP error ({status}): {message}")]
    HttpError { status: u16, message: String },
}

/// Status state of an individual connection test check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Passed,
    Failed,
    Skipped,
    NotTested,
}

/// Granular status detail for a connection test category.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CheckDetail {
    pub state: CheckState,
    pub message: String,
}

impl CheckDetail {
    pub fn passed(msg: impl Into<String>) -> Self {
        Self {
            state: CheckState::Passed,
            message: msg.into(),
        }
    }

    pub fn failed(msg: impl Into<String>) -> Self {
        Self {
            state: CheckState::Failed,
            message: msg.into(),
        }
    }

    pub fn skipped(msg: impl Into<String>) -> Self {
        Self {
            state: CheckState::Skipped,
            message: msg.into(),
        }
    }

    pub fn not_tested(msg: impl Into<String>) -> Self {
        Self {
            state: CheckState::NotTested,
            message: msg.into(),
        }
    }
}

/// Comprehensive connectivity test report for a provider instance.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderConnectionReport {
    pub instance_id: ProviderInstanceId,
    pub configuration: CheckDetail,
    pub credential: CheckDetail,
    pub reachability: CheckDetail,
    pub auth: CheckDetail,
    pub discovery: CheckDetail,
    pub runtime: CheckDetail,
}

impl ProviderConnectionReport {
    pub fn is_overall_success(&self) -> bool {
        self.configuration.state != CheckState::Failed
            && self.credential.state != CheckState::Failed
            && self.reachability.state != CheckState::Failed
            && self.auth.state != CheckState::Failed
    }
}

/// Dynamic model discovery cache mapping an instance ID to its timestamped descriptors.
pub type ModelDiscoveryCache = Arc<
    tokio::sync::RwLock<HashMap<ProviderInstanceId, (std::time::Instant, Vec<ModelDescriptor>)>>,
>;

/// Dynamic model discovery client with authenticated endpoint dispatch and TTL caching.
#[derive(Clone)]
pub struct ModelDiscoveryClient {
    secret_store: Arc<dyn SecretStore>,
    config: DiscoveryConfig,
    cache: ModelDiscoveryCache,
    http_client: reqwest::Client,
}

impl ModelDiscoveryClient {
    /// Creates a new `ModelDiscoveryClient` with the given secret store, timeout, and cache TTL.
    pub fn new(secret_store: Arc<dyn SecretStore>, timeout: Duration, cache_ttl: Duration) -> Self {
        Self::with_config(secret_store, DiscoveryConfig { timeout, cache_ttl })
    }

    /// Creates a new `ModelDiscoveryClient` with the given secret store and configuration.
    pub fn with_config(secret_store: Arc<dyn SecretStore>, config: DiscoveryConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if let Some(prev) = attempt.previous().first() {
                    // Block redirects across different origins (scheme, host, or port) to prevent leaking credentials
                    if prev.origin() != attempt.url().origin() {
                        return attempt.stop();
                    }
                }
                if attempt.previous().len() >= 5 {
                    return attempt.stop();
                }
                attempt.follow()
            }))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            secret_store,
            config,
            cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            http_client,
        }
    }

    /// Returns a reference to the internal TTL cache.
    pub fn cache(&self) -> &ModelDiscoveryCache {
        &self.cache
    }

    /// Returns a reference to the discovery configuration.
    pub fn config(&self) -> &DiscoveryConfig {
        &self.config
    }

    /// Performs a granular, real 6-stage connectivity test for a provider instance.
    pub async fn test_connection(&self, instance: &ProviderInstance) -> ProviderConnectionReport {
        // 1. Configuration check
        let url_str = match resolve_endpoint_url(instance) {
            Ok(url) => {
                if url.starts_with("http://") || url.starts_with("https://") {
                    url
                } else {
                    return ProviderConnectionReport {
                        instance_id: instance.id,
                        configuration: CheckDetail::failed(
                            "Endpoint URL must use http or https scheme",
                        ),
                        credential: CheckDetail::skipped("Skipped due to configuration failure"),
                        reachability: CheckDetail::skipped("Skipped due to configuration failure"),
                        auth: CheckDetail::skipped("Skipped due to configuration failure"),
                        discovery: CheckDetail::skipped("Skipped due to configuration failure"),
                        runtime: CheckDetail::not_tested(
                            "Runtime inference requires explicit probe flag",
                        ),
                    };
                }
            }
            Err(e) => {
                return ProviderConnectionReport {
                    instance_id: instance.id,
                    configuration: CheckDetail::failed(format!("Endpoint resolution failed: {e}")),
                    credential: CheckDetail::skipped("Skipped due to configuration failure"),
                    reachability: CheckDetail::skipped("Skipped due to configuration failure"),
                    auth: CheckDetail::skipped("Skipped due to configuration failure"),
                    discovery: CheckDetail::skipped("Skipped due to configuration failure"),
                    runtime: CheckDetail::not_tested(
                        "Runtime inference requires explicit probe flag",
                    ),
                };
            }
        };

        let configuration = CheckDetail::passed(format!("Valid endpoint URL: {url_str}"));

        // 2. Credential check
        let credential = match &instance.authentication {
            AuthenticationScheme::None => {
                CheckDetail::passed("No authentication required (Scheme: None)")
            }
            AuthenticationScheme::BearerToken { secret }
            | AuthenticationScheme::ApiKeyHeader { secret, .. }
            | AuthenticationScheme::QueryParameter { secret, .. }
            | AuthenticationScheme::OAuthToken { secret } => {
                match self.secret_store.get_secret(secret) {
                    Ok(val) if !val.trim().is_empty() => CheckDetail::passed(format!(
                        "Secret resolved from {} ({})",
                        secret.backend, secret.locator
                    )),
                    Ok(_) => CheckDetail::failed(format!(
                        "Secret in {} ({}) is empty",
                        secret.backend, secret.locator
                    )),
                    Err(e) => CheckDetail::failed(format!(
                        "Secret resolution failed from {} ({}): {e}",
                        secret.backend, secret.locator
                    )),
                }
            }
            AuthenticationScheme::AwsSigV4 { credential, .. } => {
                match self.secret_store.get_secret(credential) {
                    Ok(val) if !val.trim().is_empty() => CheckDetail::passed(format!(
                        "AWS credential resolved from {} ({})",
                        credential.backend, credential.locator
                    )),
                    Ok(_) => CheckDetail::failed(format!(
                        "AWS credential in {} ({}) is empty",
                        credential.backend, credential.locator
                    )),
                    Err(e) => CheckDetail::failed(format!("AWS credential resolution failed: {e}")),
                }
            }
        };

        if credential.state == CheckState::Failed {
            return ProviderConnectionReport {
                instance_id: instance.id,
                configuration,
                credential,
                reachability: CheckDetail::skipped("Skipped due to credential failure"),
                auth: CheckDetail::skipped("Skipped due to credential failure"),
                discovery: CheckDetail::skipped("Skipped due to credential failure"),
                runtime: CheckDetail::not_tested("Runtime inference requires explicit probe flag"),
            };
        }

        // 3. Reachability, 4. Auth, 5. Discovery
        let (headers, query_auth) =
            match build_headers_and_auth(instance, self.secret_store.as_ref()) {
                Ok(h) => h,
                Err(e) => {
                    return ProviderConnectionReport {
                        instance_id: instance.id,
                        configuration,
                        credential,
                        reachability: CheckDetail::failed(format!(
                            "Failed to build headers/auth: {e}"
                        )),
                        auth: CheckDetail::skipped("Skipped due to header construction failure"),
                        discovery: CheckDetail::skipped(
                            "Skipped due to header construction failure",
                        ),
                        runtime: CheckDetail::not_tested(
                            "Runtime inference requires explicit probe flag",
                        ),
                    };
                }
            };

        let mut req = self.http_client.get(&url_str).headers(headers);
        if !instance.endpoint.query_params.is_empty() {
            req = req.query(&instance.endpoint.query_params);
        }
        if let Some((param, val)) = query_auth {
            req = req.query(&[(param, val)]);
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                let err_msg = if e.is_timeout() {
                    format!("Connection timed out after {:?}", self.config.timeout)
                } else if e.is_connect() {
                    format!("Connection refused or DNS resolution failed: {e}")
                } else {
                    format!("Network error: {e}")
                };
                return ProviderConnectionReport {
                    instance_id: instance.id,
                    configuration,
                    credential,
                    reachability: CheckDetail::failed(err_msg),
                    auth: CheckDetail::skipped("Skipped due to reachability failure"),
                    discovery: CheckDetail::skipped("Skipped due to reachability failure"),
                    runtime: CheckDetail::not_tested(
                        "Runtime inference requires explicit probe flag",
                    ),
                };
            }
        };

        let status = resp.status();
        let reachability =
            CheckDetail::passed(format!("HTTP probe contacted endpoint (HTTP {status})"));

        let (auth, discovery) = if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            (
                CheckDetail::failed(format!("Authentication rejected: HTTP {status}")),
                CheckDetail::skipped("Discovery skipped due to authentication failure"),
            )
        } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            (
                CheckDetail::passed("Authenticated (HTTP 429 Rate Limited)"),
                CheckDetail::skipped("Discovery skipped due to rate limit"),
            )
        } else if status.is_success() {
            let auth_detail =
                CheckDetail::passed(format!("Authentication accepted (HTTP {status})"));
            match resp.text().await {
                Ok(body) => match parse_and_normalize_payload(instance.id, &body) {
                    Ok(models) => (
                        auth_detail,
                        CheckDetail::passed(format!(
                            "Successfully discovered {} model(s)",
                            models.len()
                        )),
                    ),
                    Err(e) => (
                        auth_detail,
                        CheckDetail::failed(format!("Catalog parsing failed: {e}")),
                    ),
                },
                Err(e) => (
                    auth_detail,
                    CheckDetail::failed(format!("Failed to read response body: {e}")),
                ),
            }
        } else {
            (
                CheckDetail::failed(format!("Endpoint returned unexpected HTTP {status}")),
                CheckDetail::skipped(format!("Discovery skipped due to HTTP {status}")),
            )
        };

        ProviderConnectionReport {
            instance_id: instance.id,
            configuration,
            credential,
            reachability,
            auth,
            discovery,
            runtime: CheckDetail::not_tested("Runtime inference requires explicit probe flag"),
        }
    }

    /// Discovers models available on a provider instance, returning cached results if unexpired.
    pub async fn discover_models(
        &self,
        instance: &ProviderInstance,
    ) -> Result<Vec<ModelDescriptor>, DiscoveryError> {
        self.discover_models_internal(instance, false).await
    }

    /// Forces a refresh of the model list for a provider instance, bypassing cache.
    pub async fn refresh_models(
        &self,
        instance: &ProviderInstance,
    ) -> Result<Vec<ModelDescriptor>, DiscoveryError> {
        self.discover_models_internal(instance, true).await
    }

    /// Discovers models for a provider instance with an explicit force-refresh flag.
    pub async fn discover_models_with_refresh(
        &self,
        instance: &ProviderInstance,
        force_refresh: bool,
    ) -> Result<Vec<ModelDescriptor>, DiscoveryError> {
        self.discover_models_internal(instance, force_refresh).await
    }

    async fn discover_models_internal(
        &self,
        instance: &ProviderInstance,
        force_refresh: bool,
    ) -> Result<Vec<ModelDescriptor>, DiscoveryError> {
        if !force_refresh {
            let cache = self.cache.read().await;
            if let Some((_, models)) = cache
                .get(&instance.id)
                .filter(|(at, _)| at.elapsed() < self.config.cache_ttl)
            {
                return Ok(models.clone());
            }
        }

        let descriptors = self.fetch_models_remote(instance).await?;

        {
            let mut cache = self.cache.write().await;
            cache.insert(
                instance.id,
                (std::time::Instant::now(), descriptors.clone()),
            );
        }

        Ok(descriptors)
    }

    async fn fetch_models_remote(
        &self,
        instance: &ProviderInstance,
    ) -> Result<Vec<ModelDescriptor>, DiscoveryError> {
        let url_str = resolve_endpoint_url(instance)?;
        let (headers, query_auth) = build_headers_and_auth(instance, self.secret_store.as_ref())?;

        let mut req = self.http_client.get(&url_str).headers(headers);

        if !instance.endpoint.query_params.is_empty() {
            req = req.query(&instance.endpoint.query_params);
        }
        if let Some((param, val)) = query_auth {
            req = req.query(&[(param, val)]);
        }

        let response = req.send().await.map_err(|e| {
            if e.is_timeout() {
                DiscoveryError::NetworkTimeout(e.to_string())
            } else {
                DiscoveryError::EndpointUnreachable(e.to_string())
            }
        })?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            let msg = response.text().await.unwrap_or_else(|_| status.to_string());
            return Err(DiscoveryError::AuthenticationFailed(msg));
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after_secs = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse::<u64>().ok());
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "Rate limited".to_string());
            return Err(DiscoveryError::RateLimited {
                retry_after_secs,
                message,
            });
        }

        if !status.is_success() {
            let msg = response.text().await.unwrap_or_else(|_| status.to_string());
            return Err(DiscoveryError::HttpError {
                status: status.as_u16(),
                message: msg,
            });
        }

        let body = response.text().await.map_err(|e| {
            DiscoveryError::InvalidPayload(format!("Failed to read response body: {e}"))
        })?;

        parse_and_normalize_payload(instance.id, &body)
    }
}

fn resolve_endpoint_url(instance: &ProviderInstance) -> Result<String, DiscoveryError> {
    if let Some(catalog_url) = &instance.endpoint.model_catalog_url {
        let trimmed = catalog_url.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            return Ok(trimmed.to_string());
        } else if trimmed.starts_with('/') {
            let base = instance.endpoint.base_url.trim_end_matches('/');
            return Ok(format!("{base}{trimmed}"));
        } else {
            let base = instance.endpoint.base_url.trim_end_matches('/');
            return Ok(format!("{base}/{trimmed}"));
        }
    }

    let base = instance.endpoint.base_url.trim_end_matches('/');
    let provider_id = instance.provider_id.as_str().to_ascii_lowercase();

    match provider_id.as_str() {
        "ollama" => {
            if base.ends_with("/api") {
                Ok(format!("{base}/tags"))
            } else {
                Ok(format!("{base}/api/tags"))
            }
        }
        "anthropic" => {
            if base.ends_with("/v1") {
                Ok(format!("{base}/models"))
            } else {
                Ok(format!("{base}/v1/models"))
            }
        }
        "openrouter" => {
            if base.ends_with("/api/v1") || base.ends_with("/v1") {
                Ok(format!("{base}/models"))
            } else {
                Ok(format!("{base}/api/v1/models"))
            }
        }
        "lm-studio" | "lmstudio" => {
            if base.ends_with("/v1") {
                Ok(format!("{base}/models"))
            } else {
                Ok(format!("{base}/v1/models"))
            }
        }
        _ => {
            if instance.protocol == ProtocolFamily::AnthropicMessages {
                if base.ends_with("/v1") {
                    Ok(format!("{base}/models"))
                } else {
                    Ok(format!("{base}/v1/models"))
                }
            } else if base.ends_with("/v1") {
                Ok(format!("{base}/models"))
            } else {
                Ok(format!("{base}/v1/models"))
            }
        }
    }
}

fn build_headers_and_auth(
    instance: &ProviderInstance,
    secret_store: &dyn SecretStore,
) -> Result<(reqwest::header::HeaderMap, Option<(String, String)>), DiscoveryError> {
    let mut headers = reqwest::header::HeaderMap::new();
    let mut query_auth: Option<(String, String)> = None;

    for (k, v) in &instance.endpoint.static_headers {
        let header_name = reqwest::header::HeaderName::from_bytes(k.as_bytes()).map_err(|e| {
            DiscoveryError::InvalidEndpoint(format!("Invalid static header name '{k}': {e}"))
        })?;
        let header_val = reqwest::header::HeaderValue::from_str(v).map_err(|e| {
            DiscoveryError::InvalidEndpoint(format!("Invalid static header value for '{k}': {e}"))
        })?;
        headers.insert(header_name, header_val);
    }

    let provider_id = instance.provider_id.as_str().to_ascii_lowercase();

    if (provider_id == "anthropic" || instance.protocol == ProtocolFamily::AnthropicMessages)
        && !headers.contains_key("anthropic-version")
    {
        headers.insert(
            reqwest::header::HeaderName::from_static("anthropic-version"),
            reqwest::header::HeaderValue::from_static("2023-06-01"),
        );
    }

    match &instance.authentication {
        AuthenticationScheme::None => {}
        AuthenticationScheme::BearerToken { secret } => {
            let token = secret_store
                .get_secret(secret)
                .map_err(|e| DiscoveryError::SecretResolutionFailed(e.to_string()))?;
            if token.trim().is_empty() {
                return Err(DiscoveryError::SecretResolutionFailed(
                    "Secret value is empty".to_string(),
                ));
            }
            if provider_id == "anthropic" || instance.protocol == ProtocolFamily::AnthropicMessages
            {
                let val = reqwest::header::HeaderValue::from_str(&token).map_err(|e| {
                    DiscoveryError::SecretResolutionFailed(format!("Invalid secret format: {e}"))
                })?;
                headers.insert(reqwest::header::HeaderName::from_static("x-api-key"), val);
            } else {
                let bearer_val = format!("Bearer {token}");
                let val = reqwest::header::HeaderValue::from_str(&bearer_val).map_err(|e| {
                    DiscoveryError::SecretResolutionFailed(format!("Invalid secret format: {e}"))
                })?;
                headers.insert(reqwest::header::AUTHORIZATION, val);
            }
        }
        AuthenticationScheme::ApiKeyHeader {
            header_name,
            secret,
        } => {
            let token = secret_store
                .get_secret(secret)
                .map_err(|e| DiscoveryError::SecretResolutionFailed(e.to_string()))?;
            if token.trim().is_empty() {
                return Err(DiscoveryError::SecretResolutionFailed(
                    "Secret value is empty".to_string(),
                ));
            }
            let name =
                reqwest::header::HeaderName::from_bytes(header_name.as_bytes()).map_err(|e| {
                    DiscoveryError::SecretResolutionFailed(format!(
                        "Invalid header name '{header_name}': {e}"
                    ))
                })?;
            let val = reqwest::header::HeaderValue::from_str(&token).map_err(|e| {
                DiscoveryError::SecretResolutionFailed(format!("Invalid secret format: {e}"))
            })?;
            headers.insert(name, val);

            if (provider_id == "anthropic"
                || instance.protocol == ProtocolFamily::AnthropicMessages)
                && !header_name.eq_ignore_ascii_case("x-api-key")
            {
                let x_api_val = reqwest::header::HeaderValue::from_str(&token).map_err(|e| {
                    DiscoveryError::SecretResolutionFailed(format!("Invalid secret format: {e}"))
                })?;
                headers.insert(
                    reqwest::header::HeaderName::from_static("x-api-key"),
                    x_api_val,
                );
            }
        }
        AuthenticationScheme::OAuthToken { secret } => {
            let token = secret_store
                .get_secret(secret)
                .map_err(|e| DiscoveryError::SecretResolutionFailed(e.to_string()))?;
            if token.trim().is_empty() {
                return Err(DiscoveryError::SecretResolutionFailed(
                    "Secret value is empty".to_string(),
                ));
            }
            let bearer_val = format!("Bearer {token}");
            let val = reqwest::header::HeaderValue::from_str(&bearer_val).map_err(|e| {
                DiscoveryError::SecretResolutionFailed(format!("Invalid secret format: {e}"))
            })?;
            headers.insert(reqwest::header::AUTHORIZATION, val);
        }
        AuthenticationScheme::QueryParameter {
            parameter_name,
            secret,
        } => {
            let token = secret_store
                .get_secret(secret)
                .map_err(|e| DiscoveryError::SecretResolutionFailed(e.to_string()))?;
            if token.trim().is_empty() {
                return Err(DiscoveryError::SecretResolutionFailed(
                    "Secret value is empty".to_string(),
                ));
            }
            query_auth = Some((parameter_name.clone(), token));
        }
        AuthenticationScheme::AwsSigV4 { credential, .. } => {
            let _token = secret_store
                .get_secret(credential)
                .map_err(|e| DiscoveryError::SecretResolutionFailed(e.to_string()))?;
        }
    }

    Ok((headers, query_auth))
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum RawDiscoveryPayload {
    Data { data: Vec<RawModelEntry> },
    Ollama { models: Vec<RawOllamaEntry> },
    Direct(Vec<RawModelEntry>),
}

#[derive(Deserialize, Debug, Clone)]
struct RawModelEntry {
    id: Option<String>,
    name: Option<String>,
    display_name: Option<String>,
    context_length: Option<u64>,
    max_output: Option<u64>,
    top_provider: Option<RawTopProvider>,
}

#[derive(Deserialize, Debug, Clone)]
struct RawTopProvider {
    max_completion_tokens: Option<u64>,
    context_length: Option<u64>,
}

#[derive(Deserialize, Debug, Clone)]
struct RawOllamaEntry {
    name: Option<String>,
    model: Option<String>,
}

fn parse_and_normalize_payload(
    instance_id: ProviderInstanceId,
    body: &str,
) -> Result<Vec<ModelDescriptor>, DiscoveryError> {
    let raw: RawDiscoveryPayload = serde_json::from_str(body).map_err(|e| {
        DiscoveryError::InvalidPayload(format!("Failed to parse JSON response: {e}"))
    })?;

    let mut descriptors = Vec::new();

    match raw {
        RawDiscoveryPayload::Data { data } | RawDiscoveryPayload::Direct(data) => {
            for entry in data {
                let id_candidate = entry
                    .id
                    .as_deref()
                    .or(entry.name.as_deref())
                    .map(|s| s.trim().to_string());

                if let Some(trimmed_id) = id_candidate {
                    if trimmed_id.is_empty() {
                        continue;
                    }
                    let model_id = match ModelId::new(&trimmed_id) {
                        Ok(id) => id,
                        Err(_) => continue,
                    };
                    let display_name = entry
                        .display_name
                        .filter(|n| !n.trim().is_empty())
                        .or_else(|| entry.name.filter(|n| !n.trim().is_empty()))
                        .unwrap_or_else(|| trimmed_id.clone());

                    let explicit_context = entry
                        .context_length
                        .or_else(|| entry.top_provider.as_ref().and_then(|tp| tp.context_length));
                    let explicit_max_output = entry.max_output.or_else(|| {
                        entry
                            .top_provider
                            .as_ref()
                            .and_then(|tp| tp.max_completion_tokens)
                    });

                    let (capabilities, limits) =
                        infer_model_metadata(&trimmed_id, explicit_context, explicit_max_output);

                    descriptors.push(ModelDescriptor {
                        provider_instance_id: instance_id,
                        id: model_id,
                        display_name,
                        capabilities,
                        limits,
                        metadata_source: ModelMetadataSource::ProviderDiscovery,
                    });
                }
            }
        }
        RawDiscoveryPayload::Ollama { models } => {
            for entry in models {
                let id_candidate = entry
                    .name
                    .as_deref()
                    .or(entry.model.as_deref())
                    .map(|s| s.trim().to_string());

                if let Some(trimmed_id) = id_candidate {
                    if trimmed_id.is_empty() {
                        continue;
                    }
                    let model_id = match ModelId::new(&trimmed_id) {
                        Ok(id) => id,
                        Err(_) => continue,
                    };
                    let display_name = trimmed_id.clone();
                    let (capabilities, limits) = infer_model_metadata(&trimmed_id, None, None);

                    descriptors.push(ModelDescriptor {
                        provider_instance_id: instance_id,
                        id: model_id,
                        display_name,
                        capabilities,
                        limits,
                        metadata_source: ModelMetadataSource::ProviderDiscovery,
                    });
                }
            }
        }
    }

    Ok(descriptors)
}

fn infer_model_metadata(
    model_id: &str,
    explicit_context: Option<u64>,
    explicit_max_output: Option<u64>,
) -> (ModelCapabilities, ModelLimits) {
    let lower = model_id.to_ascii_lowercase();

    let mut caps = ModelCapabilities::unknown().with_streaming(CapabilitySupport::Supported);

    let mut inferred_context = None;
    let mut inferred_max_output = None;

    if lower.contains("gpt-4o") || lower.contains("chatgpt-4o") {
        inferred_context = Some(128_000);
        inferred_max_output = Some(16_384);
        caps = caps
            .with_tool_calling(CapabilitySupport::Supported)
            .with_parallel_tool_calls(CapabilitySupport::Supported)
            .with_vision_input(CapabilitySupport::Supported)
            .with_structured_output(CapabilitySupport::Supported);
    } else if lower.contains("o1") || lower.contains("o3") {
        inferred_context = if lower.contains("mini") {
            Some(128_000)
        } else {
            Some(200_000)
        };
        inferred_max_output = Some(100_000);
        let vision = if lower.contains("mini") {
            CapabilitySupport::Unsupported
        } else {
            CapabilitySupport::Supported
        };
        caps = caps
            .with_reasoning(CapabilitySupport::Supported)
            .with_tool_calling(CapabilitySupport::Supported)
            .with_parallel_tool_calls(CapabilitySupport::Supported)
            .with_vision_input(vision)
            .with_structured_output(CapabilitySupport::Supported);
    } else if lower.contains("gpt-4-turbo")
        || lower.contains("gpt-4-1106")
        || lower.contains("gpt-4-0125")
    {
        inferred_context = Some(128_000);
        inferred_max_output = Some(4_096);
        caps = caps
            .with_tool_calling(CapabilitySupport::Supported)
            .with_parallel_tool_calls(CapabilitySupport::Supported)
            .with_vision_input(CapabilitySupport::Supported)
            .with_structured_output(CapabilitySupport::Supported);
    } else if lower.contains("gpt-4") {
        inferred_context = Some(8_192);
        inferred_max_output = Some(4_096);
        caps = caps.with_tool_calling(CapabilitySupport::Supported);
    } else if lower.contains("gpt-3.5") || lower.contains("gpt-35") {
        inferred_context = Some(16_385);
        inferred_max_output = Some(4_096);
        caps = caps.with_tool_calling(CapabilitySupport::Supported);
    } else if lower.contains("claude-3-5-sonnet")
        || lower.contains("claude-3.5-sonnet")
        || lower.contains("claude-3-7-sonnet")
        || lower.contains("claude-3.7-sonnet")
    {
        inferred_context = Some(200_000);
        inferred_max_output = Some(8_192);
        caps = caps
            .with_tool_calling(CapabilitySupport::Supported)
            .with_parallel_tool_calls(CapabilitySupport::Supported)
            .with_vision_input(CapabilitySupport::Supported)
            .with_prompt_caching(CapabilitySupport::Supported)
            .with_structured_output(CapabilitySupport::Supported);
    } else if lower.contains("claude-3-5-haiku") || lower.contains("claude-3.5-haiku") {
        inferred_context = Some(200_000);
        inferred_max_output = Some(8_192);
        caps = caps
            .with_tool_calling(CapabilitySupport::Supported)
            .with_parallel_tool_calls(CapabilitySupport::Supported)
            .with_vision_input(CapabilitySupport::Supported)
            .with_prompt_caching(CapabilitySupport::Supported);
    } else if lower.contains("claude-3") {
        inferred_context = Some(200_000);
        inferred_max_output = Some(4_096);
        caps = caps
            .with_tool_calling(CapabilitySupport::Supported)
            .with_parallel_tool_calls(CapabilitySupport::Supported)
            .with_vision_input(CapabilitySupport::Supported)
            .with_prompt_caching(CapabilitySupport::Supported);
    } else if lower.contains("llama-3.3")
        || lower.contains("llama3.3")
        || lower.contains("llama-3.2")
        || lower.contains("llama3.2")
        || lower.contains("llama-3.1")
        || lower.contains("llama3.1")
    {
        inferred_context = Some(128_000);
        inferred_max_output = Some(4_096);
        let vision = if lower.contains("vision") {
            CapabilitySupport::Supported
        } else {
            CapabilitySupport::Unsupported
        };
        caps = caps
            .with_tool_calling(CapabilitySupport::Supported)
            .with_vision_input(vision);
    } else if lower.contains("llama-3") || lower.contains("llama3") {
        inferred_context = Some(8_192);
        inferred_max_output = Some(4_096);
        caps = caps.with_tool_calling(CapabilitySupport::Supported);
    } else if lower.contains("gemini-1.5")
        || lower.contains("gemini-2.0")
        || lower.contains("gemini-flash")
        || lower.contains("gemini-pro")
    {
        inferred_context = Some(1_000_000);
        inferred_max_output = Some(8_192);
        caps = caps
            .with_tool_calling(CapabilitySupport::Supported)
            .with_parallel_tool_calls(CapabilitySupport::Supported)
            .with_vision_input(CapabilitySupport::Supported)
            .with_audio_input(CapabilitySupport::Supported);
    } else if lower.contains("qwen2.5") || lower.contains("qwen-2.5") {
        inferred_context = Some(128_000);
        inferred_max_output = Some(8_192);
        caps = caps.with_tool_calling(CapabilitySupport::Supported);
    } else if lower.contains("deepseek") {
        inferred_context = Some(64_000);
        inferred_max_output = Some(8_192);
        let reasoning = if lower.contains("r1") || lower.contains("reasoner") {
            CapabilitySupport::Supported
        } else {
            CapabilitySupport::Unknown
        };
        caps = caps
            .with_tool_calling(CapabilitySupport::Supported)
            .with_reasoning(reasoning);
    } else if lower.contains("mistral") || lower.contains("codestral") {
        inferred_context = Some(32_000);
        inferred_max_output = Some(4_096);
        caps = caps.with_tool_calling(CapabilitySupport::Supported);
    }

    let limits = ModelLimits {
        context_window_tokens: explicit_context.or(inferred_context),
        max_output_tokens: explicit_max_output.or(inferred_max_output),
    };

    (caps, limits)
}
