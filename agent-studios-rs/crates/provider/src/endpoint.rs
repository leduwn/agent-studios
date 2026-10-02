use std::collections::BTreeMap;
use url::Url;

use crate::error::ProviderError;

/// List of header names that carry credentials and must NOT appear in static_headers.
const FORBIDDEN_STATIC_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "api-key",
    "x-auth-token",
    "bearer",
];

/// Network and request endpoint profile for a provider instance.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EndpointProfile {
    /// Base URL for the endpoint (e.g. `https://api.openai.com/v1`, `http://localhost:11434`).
    pub base_url: String,

    /// Optional explicit URL or path for a model catalog endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_catalog_url: Option<String>,

    /// Non-sensitive static HTTP headers attached to requests.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub static_headers: BTreeMap<String, String>,

    /// Non-sensitive static query parameters attached to requests.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub query_params: BTreeMap<String, String>,
}

impl EndpointProfile {
    /// Creates a simple EndpointProfile with just a base URL and default empty headers/query params.
    pub fn new(base_url: impl Into<String>) -> Result<Self, ProviderError> {
        let profile = Self {
            base_url: base_url.into(),
            model_catalog_url: None,
            static_headers: BTreeMap::new(),
            query_params: BTreeMap::new(),
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Validates the endpoint profile against security invariants and syntax rules.
    pub fn validate(&self) -> Result<(), ProviderError> {
        self.validate_url(&self.base_url)?;

        if let Some(catalog) = &self.model_catalog_url {
            let trimmed = catalog.trim();
            if trimmed.is_empty() {
                return Err(ProviderError::InvalidEndpoint(
                    "model_catalog_url cannot be empty string when set".to_string(),
                ));
            }
            // If it is an absolute URL, validate it with the same rules
            if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                self.validate_url(trimmed)?;
            } else if !trimmed.starts_with('/') {
                return Err(ProviderError::InvalidEndpoint(format!(
                    "model_catalog_url must be an absolute http(s) URL or an absolute path starting with '/', got '{trimmed}'"
                )));
            }
        }

        // Validate static headers
        for (header_name, header_val) in &self.static_headers {
            let trimmed_name = header_name.trim();
            if trimmed_name.is_empty() {
                return Err(ProviderError::InvalidEndpoint(
                    "Static header name cannot be empty".to_string(),
                ));
            }

            let lower_name = trimmed_name.to_ascii_lowercase();
            for &forbidden in FORBIDDEN_STATIC_HEADERS {
                if lower_name == forbidden {
                    return Err(ProviderError::SensitiveStaticHeaderForbidden(
                        header_name.clone(),
                    ));
                }
            }

            if header_val.trim().is_empty() {
                return Err(ProviderError::InvalidEndpoint(format!(
                    "Static header '{header_name}' value cannot be empty"
                )));
            }
        }

        // Validate query params
        for (param_name, param_val) in &self.query_params {
            if param_name.trim().is_empty() {
                return Err(ProviderError::InvalidEndpoint(
                    "Query parameter name cannot be empty".to_string(),
                ));
            }
            if param_val.trim().is_empty() {
                return Err(ProviderError::InvalidEndpoint(format!(
                    "Query parameter '{param_name}' value cannot be empty"
                )));
            }
        }

        Ok(())
    }

    fn validate_url(&self, raw_url: &str) -> Result<(), ProviderError> {
        let trimmed = raw_url.trim();
        if trimmed.is_empty() {
            return Err(ProviderError::InvalidEndpoint(
                "Endpoint URL cannot be empty".to_string(),
            ));
        }

        let parsed = Url::parse(trimmed).map_err(|e| {
            ProviderError::InvalidEndpoint(format!("Malformed URL '{trimmed}': {e}"))
        })?;

        // Only http and https schemes permitted
        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(ProviderError::InvalidEndpoint(format!(
                "Unsupported URL scheme '{scheme}' (only 'http' and 'https' allowed)"
            )));
        }

        // Strict rejection of embedded credentials/userinfo
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(ProviderError::EmbeddedCredentialsForbidden);
        }

        // Strict rejection of URL fragments
        if parsed.fragment().is_some() {
            return Err(ProviderError::InvalidEndpoint(format!(
                "URL fragments are forbidden in endpoint URLs: '{trimmed}'"
            )));
        }

        // Must have host
        if parsed.host_str().is_none() {
            return Err(ProviderError::InvalidEndpoint(format!(
                "Missing host in endpoint URL: '{trimmed}'"
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_endpoints() {
        let ep1 = EndpointProfile::new("https://api.openai.com/v1").unwrap();
        assert_eq!(ep1.base_url, "https://api.openai.com/v1");

        let ep2 = EndpointProfile::new("http://localhost:11434").unwrap();
        assert_eq!(ep2.base_url, "http://localhost:11434");

        let ep3 = EndpointProfile::new("http://127.0.0.1:8080/v1/").unwrap();
        assert_eq!(ep3.base_url, "http://127.0.0.1:8080/v1/");
    }

    #[test]
    fn test_reject_credentials_in_url() {
        let err = EndpointProfile::new("https://user:password@example.com/v1").unwrap_err();
        assert_eq!(err, ProviderError::EmbeddedCredentialsForbidden);

        let err2 = EndpointProfile::new("https://user@example.com/v1").unwrap_err();
        assert_eq!(err2, ProviderError::EmbeddedCredentialsForbidden);
    }

    #[test]
    fn test_reject_forbidden_schemes() {
        assert!(EndpointProfile::new("ftp://example.com").is_err());
        assert!(EndpointProfile::new("ws://example.com").is_err());
        assert!(EndpointProfile::new("file:///etc/hosts").is_err());
    }

    #[test]
    fn test_reject_url_fragment() {
        assert!(EndpointProfile::new("https://example.com/v1#part").is_err());
    }

    #[test]
    fn test_reject_sensitive_static_headers() {
        let mut ep = EndpointProfile::new("https://api.example.com").unwrap();
        ep.static_headers
            .insert("Authorization".to_string(), "Bearer secret".to_string());
        let err = ep.validate().unwrap_err();
        assert_eq!(
            err,
            ProviderError::SensitiveStaticHeaderForbidden("Authorization".to_string())
        );

        let mut ep2 = EndpointProfile::new("https://api.example.com").unwrap();
        ep2.static_headers
            .insert("x-api-key".to_string(), "secret".to_string());
        let err2 = ep2.validate().unwrap_err();
        assert_eq!(
            err2,
            ProviderError::SensitiveStaticHeaderForbidden("x-api-key".to_string())
        );

        let mut ep3 = EndpointProfile::new("https://api.example.com").unwrap();
        ep3.static_headers
            .insert("api-KEY".to_string(), "secret".to_string());
        let err3 = ep3.validate().unwrap_err();
        assert_eq!(
            err3,
            ProviderError::SensitiveStaticHeaderForbidden("api-KEY".to_string())
        );
    }

    #[test]
    fn test_allow_safe_static_headers() {
        let mut ep = EndpointProfile::new("https://api.example.com").unwrap();
        ep.static_headers
            .insert("User-Agent".to_string(), "AgentStudios/0.1".to_string());
        ep.static_headers
            .insert("X-Custom-Routing".to_string(), "region-us".to_string());
        assert!(ep.validate().is_ok());
    }
}
