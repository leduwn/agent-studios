use serde::{Deserialize, Serialize};

use super::error::GeminiAdapterWarning;

/// Primary wire request payload for Gemini `generateContent` and `streamGenerateContent`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiGenerateContentRequest {
    pub contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<GeminiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_config: Option<GeminiToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_settings: Option<Vec<GeminiSafetySetting>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GeminiGenerationConfig>,
}

/// A single turn of content in Gemini conversation history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub parts: Vec<GeminiPart>,
}

impl GeminiContent {
    pub fn user(parts: Vec<GeminiPart>) -> Self {
        Self {
            role: Some("user".to_string()),
            parts,
        }
    }

    pub fn model(parts: Vec<GeminiPart>) -> Self {
        Self {
            role: Some("model".to_string()),
            parts,
        }
    }

    pub fn system(parts: Vec<GeminiPart>) -> Self {
        Self { role: None, parts }
    }
}

/// A structured content part in a Gemini Content turn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<GeminiBlob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<GeminiFunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_response: Option<GeminiFunctionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<bool>,
    /// Opaque thought signature required for thinking model replay.
    /// Can be present on text, functionCall, or thought parts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
}

impl GeminiPart {
    pub fn text(t: impl Into<String>) -> Self {
        Self {
            text: Some(t.into()),
            inline_data: None,
            function_call: None,
            function_response: None,
            thought: None,
            thought_signature: None,
        }
    }

    pub fn inline_data(mime_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            text: None,
            inline_data: Some(GeminiBlob {
                mime_type: mime_type.into(),
                data: data.into(),
            }),
            function_call: None,
            function_response: None,
            thought: None,
            thought_signature: None,
        }
    }

    pub fn function_call(
        id: Option<String>,
        name: impl Into<String>,
        args: serde_json::Value,
    ) -> Self {
        Self {
            text: None,
            inline_data: None,
            function_call: Some(GeminiFunctionCall {
                id,
                name: name.into(),
                args,
            }),
            function_response: None,
            thought: None,
            thought_signature: None,
        }
    }

    pub fn function_response(
        id: Option<String>,
        name: impl Into<String>,
        response: serde_json::Value,
        parts: Option<Vec<GeminiPart>>,
    ) -> Self {
        Self {
            text: None,
            inline_data: None,
            function_call: None,
            function_response: Some(GeminiFunctionResponse {
                id,
                name: name.into(),
                response,
                parts,
            }),
            thought: None,
            thought_signature: None,
        }
    }
}

/// Binary blob for inline image, audio, or document data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiBlob {
    pub mime_type: String,
    pub data: String,
}

/// Tool declaration container.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_declarations: Option<Vec<GeminiFunctionDeclaration>>,
}

/// Client function declaration specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiFunctionDeclaration {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

/// Tool execution configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_calling_config: Option<GeminiFunctionCallingConfig>,
}

/// Function calling mode configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiFunctionCallingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<GeminiFunctionCallingMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_function_names: Option<Vec<String>>,
}

/// Allowed function calling modes in Gemini generateContent.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GeminiFunctionCallingMode {
    Auto,
    Any,
    None,
    Validated,
}

/// Generation configuration parameters.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<GeminiResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<GeminiThinkingConfig>,
}

/// Structured response format definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiResponseFormat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

/// Thinking / reasoning configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiThinkingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<GeminiThinkingLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_thoughts: Option<bool>,
}

/// Thinking levels supported by Gemini 3 / Gemini 2.5 thinking models.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GeminiThinkingLevel {
    Minimal,
    Low,
    Medium,
    High,
}

/// Function call emitted by model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiFunctionCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub args: serde_json::Value,
}

/// Function execution response provided by user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiFunctionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub response: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<GeminiPart>>,
}

/// Safety setting rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiSafetySetting {
    pub category: String,
    pub threshold: String,
}

/// Primary wire response payload for `generateContent` and streaming chunks.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiGenerateContentResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<GeminiCandidate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_feedback: Option<GeminiPromptFeedback>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_metadata: Option<GeminiUsageMetadata>,
}

/// Candidate completion turn in Gemini response.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiCandidate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_ratings: Option<Vec<GeminiSafetyRating>>,
}

/// Prompt feedback indicating safety or policy blocks before generation.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiPromptFeedback {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_ratings: Option<Vec<GeminiSafetyRating>>,
}

/// Detailed usage metadata reported by Gemini generateContent.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiUsageMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_content_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thoughts_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_prompt_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_token_count: Option<i64>,
}

/// Safety rating item in candidate or prompt feedback.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiSafetyRating {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probability: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<bool>,
}

// ============================================================================
// Adapter Options and Translation Return Types
// ============================================================================

/// Policy governing Gemini thinking configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GeminiThinkingPolicy {
    /// Omit explicit thinking config from wire payload (provider default).
    #[default]
    ProviderDefault,
    /// Maps exact Codex ReasoningEffort (Minimal -> MINIMAL, Low -> LOW, Medium -> MEDIUM, High -> HIGH).
    /// Rejects non-equivalent tiers (None, XHigh, Max, Ultra, Persistent, Custom) fail-closed.
    ExactReasoningEffort,
    /// Legacy explicit budget tokens compatibility mode (never combined with thinkingLevel).
    LegacyBudget(u64),
}

/// Configuration options for Gemini generateContent request translation.
#[derive(Debug, Clone, PartialEq)]
pub struct GeminiAdapterOptions {
    /// Explicit max output tokens. None preserves provider/model default. Rejects 0.
    pub max_output_tokens: Option<u64>,
    /// Thinking configuration policy.
    pub thinking_policy: GeminiThinkingPolicy,
    /// Whether to enforce strict tool name validation against Gemini regex contract. Default: true.
    pub validate_tool_names: bool,
}

impl GeminiAdapterOptions {
    pub fn new() -> Self {
        Self {
            max_output_tokens: None,
            thinking_policy: GeminiThinkingPolicy::ProviderDefault,
            validate_tool_names: true,
        }
    }

    pub fn with_max_output_tokens(mut self, max_tokens: u64) -> Self {
        self.max_output_tokens = Some(max_tokens);
        self
    }

    pub fn with_thinking_policy(mut self, policy: GeminiThinkingPolicy) -> Self {
        self.thinking_policy = policy;
        self
    }
}

impl Default for GeminiAdapterOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Complete translation output from Codex `ResponsesApiRequest` to Gemini generateContent.
#[derive(Debug, Clone, PartialEq)]
pub struct GeminiRequestTranslation {
    /// Model name from `ResponsesApiRequest.model` preserved verbatim for path routing.
    pub model: String,
    /// The generated wire request payload.
    pub request: GeminiGenerateContentRequest,
    /// Non-fatal fidelity and compatibility warnings.
    pub warnings: Vec<GeminiAdapterWarning>,
}
