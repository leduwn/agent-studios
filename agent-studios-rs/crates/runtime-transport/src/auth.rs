use std::fmt;
use std::sync::Arc;

use agent_studios_provider::auth::AuthenticationScheme;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::error::TransportError;
use crate::secret::{SecretResolver, SecretString};

/// Resolved authentication assets ready to be applied to outbound HTTP requests.
#[derive(Clone, Default)]
pub struct ResolvedAuth {
    pub headers: HeaderMap,
    pub query_params: Vec<(String, SecretString)>,
}

impl fmt::Debug for ResolvedAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let header_names: Vec<&str> = self.headers.keys().map(|k| k.as_str()).collect();
        let query_names: Vec<&str> = self.query_params.iter().map(|(k, _)| k.as_str()).collect();
        f.debug_struct("ResolvedAuth")
            .field("header_names", &header_names)
            .field("query_names", &query_names)
            .finish()
    }
}

impl ResolvedAuth {
    /// Resolves an `AuthenticationScheme` using the provided secret resolver.
    pub async fn resolve(
        scheme: &AuthenticationScheme,
        resolver: &Arc<dyn SecretResolver>,
    ) -> Result<Self, TransportError> {
        let mut auth = Self::default();

        match scheme {
            AuthenticationScheme::None => Ok(auth),
            AuthenticationScheme::BearerToken { secret }
            | AuthenticationScheme::OAuthToken { secret } => {
                let token = resolver.resolve(secret).await?;
                let bearer_value = format!("Bearer {}", token.expose_secret());
                let mut header_value = HeaderValue::from_str(&bearer_value).map_err(|e| {
                    TransportError::Auth(format!("Invalid bearer token header value: {e}"))
                })?;
                header_value.set_sensitive(true);
                auth.headers
                    .insert(reqwest::header::AUTHORIZATION, header_value);
                Ok(auth)
            }
            AuthenticationScheme::ApiKeyHeader {
                header_name,
                secret,
            } => {
                let token = resolver.resolve(secret).await?;
                let header_name = HeaderName::from_bytes(header_name.as_bytes()).map_err(|e| {
                    TransportError::Auth(format!(
                        "Invalid API key header name '{header_name}': {e}"
                    ))
                })?;
                let mut header_value =
                    HeaderValue::from_str(token.expose_secret()).map_err(|e| {
                        TransportError::Auth(format!("Invalid API key header value: {e}"))
                    })?;
                header_value.set_sensitive(true);
                auth.headers.insert(header_name, header_value);
                Ok(auth)
            }
            AuthenticationScheme::QueryParameter {
                parameter_name,
                secret,
            } => {
                let token = resolver.resolve(secret).await?;
                auth.query_params.push((parameter_name.clone(), token));
                Ok(auth)
            }
            AuthenticationScheme::AwsSigV4 { .. } => Err(TransportError::UnsupportedProtocol(
                "AWS SigV4 authentication scheme not yet implemented in runtime transport"
                    .to_string(),
            )),
        }
    }

    /// Verifies that authentication credentials do not collide case-insensitively with static headers or query parameters.
    pub fn check_collisions<K1: AsRef<str>, K2: AsRef<str>>(
        &self,
        static_headers: impl IntoIterator<Item = K1>,
        query_params: impl IntoIterator<Item = K2>,
    ) -> Result<(), TransportError> {
        let headers_list: Vec<K1> = static_headers.into_iter().collect();
        for header in self.headers.keys() {
            if let Some(colliding) = headers_list
                .iter()
                .find(|k| k.as_ref().eq_ignore_ascii_case(header.as_str()))
            {
                return Err(TransportError::AuthenticationCollision {
                    name: colliding.as_ref().to_string(),
                    location: "static_headers",
                });
            }
        }

        let queries_list: Vec<K2> = query_params.into_iter().collect();
        for (param_name, _) in &self.query_params {
            if let Some(colliding) = queries_list
                .iter()
                .find(|k| k.as_ref().eq_ignore_ascii_case(param_name))
            {
                return Err(TransportError::AuthenticationCollision {
                    name: colliding.as_ref().to_string(),
                    location: "query_params",
                });
            }
        }

        Ok(())
    }

    /// Applies the resolved authentication headers and query parameters to a `reqwest::RequestBuilder`.
    pub fn apply(&self, mut builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        for (name, value) in &self.headers {
            builder = builder.header(name, value);
        }
        for (key, val) in &self.query_params {
            builder = builder.query(&[(key, val.expose_secret())]);
        }
        builder
    }

    /// Redacts known credential values (tokens, secrets, API keys) from text.
    pub fn redact_secrets(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (_, val) in &self.query_params {
            let secret = val.expose_secret();
            if !secret.is_empty() {
                result = result.replace(secret, "[REDACTED]");
            }
        }
        for (_, val) in &self.headers {
            if let Ok(val_str) = val.to_str() {
                if let Some(token) = val_str.strip_prefix("Bearer ") {
                    if !token.is_empty() {
                        result = result.replace(token, "[REDACTED]");
                    }
                } else if !val_str.is_empty() {
                    result = result.replace(val_str, "[REDACTED]");
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::InMemorySecretResolver;
    use agent_studios_provider::secret::{SecretBackend, SecretReference};
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_auth_bearer_token() {
        let sec_ref = SecretReference {
            backend: SecretBackend::EnvironmentVariable,
            locator: "OPENAI_API_KEY".to_string(),
        };
        let resolver: Arc<dyn SecretResolver> =
            Arc::new(InMemorySecretResolver::new().with_env_secret("OPENAI_API_KEY", "sk-12345"));
        let scheme = AuthenticationScheme::BearerToken { secret: sec_ref };

        let auth = ResolvedAuth::resolve(&scheme, &resolver).await.unwrap();
        assert_eq!(
            auth.headers.get("authorization").unwrap(),
            "Bearer sk-12345"
        );
        assert!(auth.query_params.is_empty());
    }

    #[tokio::test]
    async fn test_auth_api_key_header() {
        let sec_ref = SecretReference {
            backend: SecretBackend::EnvironmentVariable,
            locator: "ANTHROPIC_API_KEY".to_string(),
        };
        let resolver: Arc<dyn SecretResolver> = Arc::new(
            InMemorySecretResolver::new().with_env_secret("ANTHROPIC_API_KEY", "ant-key-xyz"),
        );
        let scheme = AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: sec_ref,
        };

        let auth = ResolvedAuth::resolve(&scheme, &resolver).await.unwrap();
        assert_eq!(auth.headers.get("x-api-key").unwrap(), "ant-key-xyz");
        assert!(auth.query_params.is_empty());
    }

    #[tokio::test]
    async fn test_auth_query_parameter() {
        let sec_ref = SecretReference {
            backend: SecretBackend::EnvironmentVariable,
            locator: "GEMINI_API_KEY".to_string(),
        };
        let resolver: Arc<dyn SecretResolver> = Arc::new(
            InMemorySecretResolver::new().with_env_secret("GEMINI_API_KEY", "gemini-key-999"),
        );
        let scheme = AuthenticationScheme::QueryParameter {
            parameter_name: "key".to_string(),
            secret: sec_ref,
        };

        let auth = ResolvedAuth::resolve(&scheme, &resolver).await.unwrap();
        assert!(auth.headers.is_empty());
        assert_eq!(auth.query_params.len(), 1);
        assert_eq!(auth.query_params[0].0, "key");
        assert_eq!(auth.query_params[0].1.expose_secret(), "gemini-key-999");
    }

    #[tokio::test]
    async fn test_auth_debug_redaction() {
        let sec_ref = SecretReference {
            backend: SecretBackend::EnvironmentVariable,
            locator: "MY_KEY".to_string(),
        };
        let resolver: Arc<dyn SecretResolver> = Arc::new(
            InMemorySecretResolver::new().with_env_secret("MY_KEY", "sensitive-secret-token"),
        );
        let scheme = AuthenticationScheme::BearerToken { secret: sec_ref };

        let auth = ResolvedAuth::resolve(&scheme, &resolver).await.unwrap();
        let debug_str = format!("{auth:?}");
        assert!(!debug_str.contains("sensitive-secret-token"));
        assert!(!debug_str.contains("Bearer"));
        assert!(debug_str.contains("authorization"));
    }

    #[test]
    fn test_auth_collision_detection() {
        let mut auth = ResolvedAuth::default();
        let mut header_val = HeaderValue::from_static("secret");
        header_val.set_sensitive(true);
        auth.headers
            .insert(reqwest::header::AUTHORIZATION, header_val);
        auth.query_params
            .push(("api_key".to_string(), SecretString::new("secret")));

        // Colliding header with different case
        let mut static_headers = HashMap::new();
        static_headers.insert("Authorization".to_string(), "custom".to_string());
        let query_params: HashMap<String, String> = HashMap::new();

        let err = auth
            .check_collisions(static_headers.keys(), query_params.keys())
            .unwrap_err();
        match err {
            TransportError::AuthenticationCollision { name, location } => {
                assert_eq!(name, "Authorization");
                assert_eq!(location, "static_headers");
            }
            other => panic!("Unexpected error: {other:?}"),
        }

        // Colliding query parameter with different case
        let static_headers_clean: HashMap<String, String> = HashMap::new();
        let mut query_params_colliding = HashMap::new();
        query_params_colliding.insert("API_KEY".to_string(), "val".to_string());

        let err_q = auth
            .check_collisions(static_headers_clean.keys(), query_params_colliding.keys())
            .unwrap_err();
        match err_q {
            TransportError::AuthenticationCollision { name, location } => {
                assert_eq!(name, "API_KEY");
                assert_eq!(location, "query_params");
            }
            other => panic!("Unexpected error: {other:?}"),
        }
    }
}
