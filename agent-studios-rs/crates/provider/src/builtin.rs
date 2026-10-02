use crate::definition::ProviderDefinition;
use crate::id::ProviderId;
use crate::protocol::ProtocolFamily;

/// Returns standard built-in provider family definitions.
///
/// Contains ONLY metadata definitions (family ID, display name, supported protocols).
/// Contains NO credentials, NO user endpoints, and NO hardcoded model catalogs.
pub fn builtin_definitions() -> Vec<ProviderDefinition> {
    vec![
        ProviderDefinition::new(
            ProviderId::new("openai").expect("valid id"),
            "OpenAI",
            vec![
                ProtocolFamily::OpenAiResponses,
                ProtocolFamily::OpenAiChatCompletions,
            ],
        )
        .expect("valid definition")
        .with_documentation_url("https://platform.openai.com/docs"),
        ProviderDefinition::new(
            ProviderId::new("anthropic").expect("valid id"),
            "Anthropic",
            vec![ProtocolFamily::AnthropicMessages],
        )
        .expect("valid definition")
        .with_documentation_url("https://docs.anthropic.com"),
        ProviderDefinition::new(
            ProviderId::new("google-gemini").expect("valid id"),
            "Google Gemini",
            vec![ProtocolFamily::GeminiGenerateContent],
        )
        .expect("valid definition")
        .with_documentation_url("https://ai.google.dev/docs"),
        ProviderDefinition::new(
            ProviderId::new("openrouter").expect("valid id"),
            "OpenRouter",
            vec![
                ProtocolFamily::OpenAiChatCompletions,
                ProtocolFamily::AnthropicMessages,
            ],
        )
        .expect("valid definition")
        .with_documentation_url("https://openrouter.ai/docs"),
        ProviderDefinition::new(
            ProviderId::new("deepseek").expect("valid id"),
            "DeepSeek",
            vec![ProtocolFamily::OpenAiChatCompletions],
        )
        .expect("valid definition")
        .with_documentation_url("https://platform.deepseek.com"),
        ProviderDefinition::new(
            ProviderId::new("groq").expect("valid id"),
            "Groq",
            vec![ProtocolFamily::OpenAiChatCompletions],
        )
        .expect("valid definition")
        .with_documentation_url("https://console.groq.com/docs"),
        ProviderDefinition::new(
            ProviderId::new("9router").expect("valid id"),
            "9Router",
            vec![
                ProtocolFamily::OpenAiChatCompletions,
                ProtocolFamily::OpenAiResponses,
            ],
        )
        .expect("valid definition"),
        ProviderDefinition::new(
            ProviderId::new("ollama").expect("valid id"),
            "Ollama",
            vec![
                ProtocolFamily::OpenAiChatCompletions,
                ProtocolFamily::OpenAiResponses,
            ],
        )
        .expect("valid definition")
        .with_documentation_url("https://ollama.com"),
        ProviderDefinition::new(
            ProviderId::new("lm-studio").expect("valid id"),
            "LM Studio",
            vec![ProtocolFamily::OpenAiChatCompletions],
        )
        .expect("valid definition")
        .with_documentation_url("https://lmstudio.ai"),
        ProviderDefinition::new(
            ProviderId::new("custom").expect("valid id"),
            "Custom Provider",
            vec![
                ProtocolFamily::OpenAiResponses,
                ProtocolFamily::OpenAiChatCompletions,
                ProtocolFamily::AnthropicMessages,
                ProtocolFamily::GeminiGenerateContent,
            ],
        )
        .expect("valid definition"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_definitions() {
        let defs = builtin_definitions();
        assert!(!defs.is_empty());
        for def in &defs {
            assert!(def.validate().is_ok());
        }
        let ids: Vec<_> = defs.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"openai"));
        assert!(ids.contains(&"anthropic"));
        assert!(ids.contains(&"google-gemini"));
        assert!(ids.contains(&"openrouter"));
        assert!(ids.contains(&"9router"));
        assert!(ids.contains(&"custom"));
    }
}
