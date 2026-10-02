use serde::{Deserialize, Serialize};

use crate::error::ProviderError;
use crate::secret::SecretReference;

/// Authentication scheme configuration for a provider instance.
/// Holds only references to secrets, never raw secrets.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticationScheme {
    /// No authentication required (e.g. local Ollama, unauthenticated proxy).
    #[default]
    None,

    /// HTTP `Authorization: Bearer <token>` using a secret reference.
    BearerToken { secret: SecretReference },

    /// Custom API key header (e.g. `x-api-key: <token>`, `api-key: <token>`).
    ApiKeyHeader {
        header_name: String,
        secret: SecretReference,
    },

    /// Query parameter authentication (e.g. `?key=<token>`).
    QueryParameter {
        parameter_name: String,
        secret: SecretReference,
    },

    /// OAuth token loaded from a secret reference.
    OAuthToken { secret: SecretReference },

    /// AWS Signature Version 4 metadata.
    AwsSigV4 {
        credential: SecretReference,
        region: String,
        service: String,
    },
}

impl AuthenticationScheme {
    /// Validates authentication configuration metadata.
    pub fn validate(&self) -> Result<(), ProviderError> {
        match self {
            Self::None => Ok(()),
            Self::BearerToken { secret } | Self::OAuthToken { secret } => secret.validate(),
            Self::ApiKeyHeader {
                header_name,
                secret,
            } => {
                let trimmed = header_name.trim();
                if trimmed.is_empty() {
                    return Err(ProviderError::InvalidAuthentication(
                        "ApiKeyHeader header_name cannot be empty".to_string(),
                    ));
                }
                secret.validate()
            }
            Self::QueryParameter {
                parameter_name,
                secret,
            } => {
                let trimmed = parameter_name.trim();
                if trimmed.is_empty() {
                    return Err(ProviderError::InvalidAuthentication(
                        "QueryParameter parameter_name cannot be empty".to_string(),
                    ));
                }
                secret.validate()
            }
            Self::AwsSigV4 {
                credential,
                region,
                service,
            } => {
                credential.validate()?;
                if region.trim().is_empty() {
                    return Err(ProviderError::InvalidAuthentication(
                        "AwsSigV4 region cannot be empty".to_string(),
                    ));
                }
                if service.trim().is_empty() {
                    return Err(ProviderError::InvalidAuthentication(
                        "AwsSigV4 service cannot be empty".to_string(),
                    ));
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::SecretBackend;

    #[test]
    fn test_auth_scheme_validation() {
        let sec = SecretReference::env("KEY").unwrap();
        assert!(AuthenticationScheme::None.validate().is_ok());

        let bearer = AuthenticationScheme::BearerToken {
            secret: sec.clone(),
        };
        assert!(bearer.validate().is_ok());

        let api_key = AuthenticationScheme::ApiKeyHeader {
            header_name: "x-api-key".to_string(),
            secret: sec.clone(),
        };
        assert!(api_key.validate().is_ok());

        let empty_header = AuthenticationScheme::ApiKeyHeader {
            header_name: "   ".to_string(),
            secret: sec.clone(),
        };
        assert!(empty_header.validate().is_err());

        let qparam = AuthenticationScheme::QueryParameter {
            parameter_name: "key".to_string(),
            secret: sec.clone(),
        };
        assert!(qparam.validate().is_ok());

        let aws = AuthenticationScheme::AwsSigV4 {
            credential: sec,
            region: "us-east-1".to_string(),
            service: "bedrock".to_string(),
        };
        assert!(aws.validate().is_ok());
    }

    #[test]
    fn test_auth_scheme_serde() {
        let sec = SecretReference::new(SecretBackend::OsCredentialStore, "my-token").unwrap();
        let auth = AuthenticationScheme::BearerToken { secret: sec };
        let json = serde_json::to_string(&auth).unwrap();
        let de: AuthenticationScheme = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, de);
    }
}
