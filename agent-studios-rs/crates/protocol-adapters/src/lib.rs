//! OpenAI Chat Completions Protocol Adapter
//!
//! Provides pure protocol translation between Codex Responses API semantics
//! (`codex_api::ResponsesApiRequest`, `codex_api::ResponseEvent`) and standard
//! OpenAI Chat Completions wire format (`POST /v1/chat/completions`).

pub mod error;
pub mod request;
pub mod stream;
pub mod types;

pub use error::{ChatAdapterError, ChatAdapterWarning};
pub use request::{ChatRequestTranslation, translate_request};
pub use stream::{ChatCompletionStreamTranslator, decode_sse_line, is_done_line};
pub use types::{
    ChatChunkChoice, ChatChunkDelta, ChatChunkFunctionCall, ChatChunkToolCall, ChatCompletionChunk,
    ChatCompletionRequest, ChatCompletionTokensDetails, ChatContentPart, ChatFunctionCall,
    ChatFunctionDefinition, ChatImageUrl, ChatJsonSchemaFormat, ChatMessage, ChatMessageContent,
    ChatPromptTokensDetails, ChatResponseFormat, ChatStreamOptions, ChatTool, ChatToolCall,
    ChatToolChoice, ChatUsage,
};

use codex_api::ResponsesApiRequest;

/// Unified adapter interface for OpenAI Chat Completions protocol translation.
pub struct ChatCompletionsAdapter;

impl ChatCompletionsAdapter {
    /// Translates a Codex `ResponsesApiRequest` into an OpenAI `ChatCompletionRequest`.
    pub fn translate_request(
        request: &ResponsesApiRequest,
    ) -> Result<ChatRequestTranslation, ChatAdapterError> {
        translate_request(request)
    }

    /// Creates a fresh stateful stream translator for converting SSE chunks to `ResponseEvent`s.
    pub fn new_stream_translator() -> ChatCompletionStreamTranslator {
        ChatCompletionStreamTranslator::new()
    }

    /// Decodes an SSE stream line into an optional `ChatCompletionChunk`.
    pub fn decode_sse_line(line: &str) -> Result<Option<ChatCompletionChunk>, ChatAdapterError> {
        decode_sse_line(line)
    }
}
