//! Gemini generateContent Protocol Adapter
//!
//! Provides pure protocol translation between Codex Responses API semantics
//! (`codex_api::ResponsesApiRequest`, `codex_api::ResponseEvent`) and Google
//! Gemini `generateContent` / `streamGenerateContent` wire format.

pub mod continuation;
pub mod error;
pub mod request;
pub mod stream;
pub mod types;

pub use continuation::{GeminiContinuationState, GeminiOriginRef, GeminiToolCallMetadata};
pub use error::{GeminiAdapterError, GeminiAdapterWarning};
pub use request::{is_valid_gemini_tool_name, translate_request};
pub use stream::GeminiStreamTranslator;
pub use types::{
    GeminiAdapterOptions, GeminiBlob, GeminiCandidate, GeminiContent, GeminiFunctionCall,
    GeminiFunctionCallingConfig, GeminiFunctionCallingMode, GeminiFunctionDeclaration,
    GeminiFunctionResponse, GeminiGenerateContentRequest, GeminiGenerateContentResponse,
    GeminiGenerationConfig, GeminiPart, GeminiPromptFeedback, GeminiRequestTranslation,
    GeminiResponseFormat, GeminiSafetyRating, GeminiSafetySetting, GeminiTextFormatConfig,
    GeminiThinkingConfig, GeminiThinkingLevel, GeminiThinkingPolicy, GeminiTool, GeminiToolConfig,
    GeminiUsageMetadata,
};

use codex_api::ResponsesApiRequest;

/// Unified adapter interface for Gemini `generateContent` protocol translation.
pub struct GeminiAdapter;

impl GeminiAdapter {
    /// Translates a Codex `ResponsesApiRequest` into a Gemini `generateContent` wire request.
    pub fn translate_request(
        request: &ResponsesApiRequest,
        options: &GeminiAdapterOptions,
        continuation: Option<&GeminiContinuationState>,
    ) -> Result<GeminiRequestTranslation, GeminiAdapterError> {
        translate_request(request, options, continuation)
    }

    /// Creates a fresh stateful stream translator for converting SSE chunks to `ResponseEvent`s.
    pub fn new_stream_translator() -> GeminiStreamTranslator {
        GeminiStreamTranslator::new()
    }
}
