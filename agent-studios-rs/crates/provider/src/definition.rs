use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::error::ProviderError;
use crate::id::ProviderId;
use crate::protocol::ProtocolFamily;

/// Provider family definition describing an upstream provider or gateway type.
///
/// Contains NO credentials, NO API keys, and NO mutable account-specific endpoints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDefinition {
    pub id: ProviderId,
    pub display_name: String,
    pub supported_protocols: BTreeSet<ProtocolFamily>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
}

impl ProviderDefinition {
    pub fn new(
        id: ProviderId,
        display_name: impl Into<String>,
        supported_protocols: impl IntoIterator<Item = ProtocolFamily>,
    ) -> Result<Self, ProviderError> {
        let display_name = display_name.into();
        let trimmed_name = display_name.trim();
        if trimmed_name.is_empty() {
            return Err(ProviderError::InvalidProviderId(
                "ProviderDefinition display_name cannot be empty".to_string(),
            ));
        }

        let protocol_set: BTreeSet<ProtocolFamily> = supported_protocols.into_iter().collect();
        if protocol_set.is_empty() {
            return Err(ProviderError::InvalidProviderId(
                "ProviderDefinition must support at least one protocol".to_string(),
            ));
        }

        for proto in &protocol_set {
            proto.validate()?;
        }

        Ok(Self {
            id,
            display_name: trimmed_name.to_string(),
            supported_protocols: protocol_set,
            documentation_url: None,
        })
    }

    pub fn with_documentation_url(mut self, url: impl Into<String>) -> Self {
        self.documentation_url = Some(url.into());
        self
    }

    pub fn supports_protocol(&self, protocol: &ProtocolFamily) -> bool {
        self.supported_protocols.contains(protocol)
    }

    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.display_name.trim().is_empty() {
            return Err(ProviderError::InvalidProviderId(
                "ProviderDefinition display_name cannot be empty".to_string(),
            ));
        }
        if self.supported_protocols.is_empty() {
            return Err(ProviderError::InvalidProviderId(
                "ProviderDefinition must support at least one protocol".to_string(),
            ));
        }
        for proto in &self.supported_protocols {
            proto.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_definition_valid() {
        let def = ProviderDefinition::new(
            ProviderId::new("openai").unwrap(),
            "OpenAI",
            vec![
                ProtocolFamily::OpenAiResponses,
                ProtocolFamily::OpenAiChatCompletions,
            ],
        )
        .unwrap();

        assert_eq!(def.id.as_str(), "openai");
        assert!(def.supports_protocol(&ProtocolFamily::OpenAiResponses));
        assert!(!def.supports_protocol(&ProtocolFamily::AnthropicMessages));
    }

    #[test]
    fn test_provider_definition_empty_protocols_rejected() {
        let res = ProviderDefinition::new(
            ProviderId::new("openai").unwrap(),
            "OpenAI",
            Vec::<ProtocolFamily>::new(),
        );
        assert!(res.is_err());
    }
}
