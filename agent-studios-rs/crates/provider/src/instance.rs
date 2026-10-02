use serde::{Deserialize, Serialize};

use crate::auth::AuthenticationScheme;
use crate::endpoint::EndpointProfile;
use crate::error::ProviderError;
use crate::id::{ProviderId, ProviderInstanceId};
use crate::protocol::ProtocolFamily;

/// A concrete configured instance of a provider with its own endpoint and credentials.
///
/// Multiple instances may share the same `ProviderDefinition` while pointing
/// to different base URLs, using different credential references, or selecting
/// different wire protocols.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderInstance {
    pub id: ProviderInstanceId,
    pub provider_id: ProviderId,
    pub display_name: String,
    pub protocol: ProtocolFamily,
    pub endpoint: EndpointProfile,
    pub authentication: AuthenticationScheme,
    pub enabled: bool,
}

impl ProviderInstance {
    pub fn new(
        id: ProviderInstanceId,
        provider_id: ProviderId,
        display_name: impl Into<String>,
        protocol: ProtocolFamily,
        endpoint: EndpointProfile,
        authentication: AuthenticationScheme,
    ) -> Result<Self, ProviderError> {
        let display_name = display_name.into();
        let trimmed_name = display_name.trim();
        if trimmed_name.is_empty() {
            return Err(ProviderError::InvalidAuthentication(
                "ProviderInstance display_name cannot be empty".to_string(),
            ));
        }

        protocol.validate()?;
        endpoint.validate()?;
        authentication.validate()?;

        Ok(Self {
            id,
            provider_id,
            display_name: trimmed_name.to_string(),
            protocol,
            endpoint,
            authentication,
            enabled: true,
        })
    }

    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.display_name.trim().is_empty() {
            return Err(ProviderError::InvalidAuthentication(
                "ProviderInstance display_name cannot be empty".to_string(),
            ));
        }
        self.protocol.validate()?;
        self.endpoint.validate()?;
        self.authentication.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::SecretReference;

    #[test]
    fn test_provider_instance_valid() {
        let inst_id = ProviderInstanceId::new();
        let prov_id = ProviderId::new("openai").unwrap();
        let ep = EndpointProfile::new("https://api.openai.com/v1").unwrap();
        let auth = AuthenticationScheme::BearerToken {
            secret: SecretReference::env("OPENAI_API_KEY").unwrap(),
        };

        let inst = ProviderInstance::new(
            inst_id,
            prov_id.clone(),
            "OpenAI Personal",
            ProtocolFamily::OpenAiResponses,
            ep,
            auth,
        )
        .unwrap();

        assert_eq!(inst.id, inst_id);
        assert_eq!(inst.provider_id, prov_id);
        assert_eq!(inst.display_name, "OpenAI Personal");
        assert!(inst.enabled);
    }
}
