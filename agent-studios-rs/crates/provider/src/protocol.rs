use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::ProviderError;

/// Wire protocol family supported by an endpoint.
///
/// Decoupled from provider brand: an instance selects its protocol
/// based on the API format it expects, not by matching the provider name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ProtocolFamily {
    /// OpenAI Responses API (e.g. `/v1/responses`).
    #[serde(rename = "openai_responses")]
    OpenAiResponses,
    /// OpenAI Chat Completions API (e.g. `/v1/chat/completions`).
    #[serde(rename = "openai_chat_completions")]
    OpenAiChatCompletions,
    /// Anthropic Messages API (e.g. `/v1/messages`).
    #[serde(rename = "anthropic_messages")]
    AnthropicMessages,
    /// Google Gemini GenerateContent API (e.g. `/v1beta/models/...:generateContent`).
    #[serde(rename = "gemini_generate_content")]
    GeminiGenerateContent,
    /// Custom wire protocol family with a non-empty name.
    #[serde(rename = "custom")]
    Custom(String),
}

impl ProtocolFamily {
    /// Validates the protocol family, rejecting empty custom identifiers.
    pub fn validate(&self) -> Result<(), ProviderError> {
        match self {
            Self::Custom(name) => {
                let trimmed = name.trim();
                if trimmed.is_empty() {
                    return Err(ProviderError::InvalidAuthentication(
                        "Custom protocol family name cannot be empty".to_string(),
                    ));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl fmt::Display for ProtocolFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenAiResponses => write!(f, "openai_responses"),
            Self::OpenAiChatCompletions => write!(f, "openai_chat_completions"),
            Self::AnthropicMessages => write!(f, "anthropic_messages"),
            Self::GeminiGenerateContent => write!(f, "gemini_generate_content"),
            Self::Custom(name) => write!(f, "custom:{name}"),
        }
    }
}

impl FromStr for ProtocolFamily {
    type Err = ProviderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "openai_responses" => Ok(Self::OpenAiResponses),
            "openai_chat_completions" => Ok(Self::OpenAiChatCompletions),
            "anthropic_messages" => Ok(Self::AnthropicMessages),
            "gemini_generate_content" => Ok(Self::GeminiGenerateContent),
            other => {
                let name = if let Some(stripped) = other.strip_prefix("custom:") {
                    stripped.trim()
                } else {
                    other
                };
                if name.is_empty() {
                    return Err(ProviderError::InvalidAuthentication(
                        "Protocol family name cannot be empty".to_string(),
                    ));
                }
                Ok(Self::Custom(name.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_family_serde() {
        let p = ProtocolFamily::OpenAiResponses;
        let s = serde_json::to_string(&p).unwrap();
        assert_eq!(s, "\"openai_responses\"");
        let des: ProtocolFamily = serde_json::from_str(&s).unwrap();
        assert_eq!(des, p);

        let custom = ProtocolFamily::Custom("my-rpc".to_string());
        let s_cust = serde_json::to_string(&custom).unwrap();
        assert_eq!(s_cust, "{\"custom\":\"my-rpc\"}");
        let des_cust: ProtocolFamily = serde_json::from_str(&s_cust).unwrap();
        assert_eq!(des_cust, custom);
    }

    #[test]
    fn test_protocol_family_display_from_str() {
        assert_eq!(
            ProtocolFamily::from_str("openai_chat_completions").unwrap(),
            ProtocolFamily::OpenAiChatCompletions
        );
        assert_eq!(
            ProtocolFamily::from_str("anthropic_messages").unwrap(),
            ProtocolFamily::AnthropicMessages
        );
        assert_eq!(
            ProtocolFamily::from_str("gemini_generate_content").unwrap(),
            ProtocolFamily::GeminiGenerateContent
        );
        assert_eq!(
            ProtocolFamily::from_str("custom:my_proto").unwrap(),
            ProtocolFamily::Custom("my_proto".to_string())
        );
    }
}
