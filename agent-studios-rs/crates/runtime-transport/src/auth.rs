use std::sync::Arc;

use agent_studios_provider::auth::AuthenticationScheme;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::error::TransportError;
use crate::secret::{SecretResolver, SecretString};

/// Resolved authentication assets ready to be applied to outbound HTTP requests.
#[derive(Clone, Debug, Default)]
pub struct ResolvedAuth {
    pub headers: HeaderMap,
    pub query_params: Vec<(String, SecretString)>,
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
                let header_value = HeaderValue::from_str(&bearer_value).map_err(|e| {
                    TransportError::Auth(format!("Invalid bearer token header value: {e}"))
                })?;
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
                let header_value = HeaderValue::from_str(token.expose_secret()).map_err(|e| {
                    TransportError::Auth(format!("Invalid API key header value: {e}"))
                })?;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::InMemorySecretResolver;
    use agent_studios_provider::secret::{SecretBackend, SecretReference};

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
}
