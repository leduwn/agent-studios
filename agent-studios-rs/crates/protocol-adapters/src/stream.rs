use std::collections::BTreeMap;

use codex_api::ResponseEvent;
use codex_protocol::ResponseItemId;
use codex_protocol::models::{ContentItem, ResponseItem};
use codex_protocol::protocol::TokenUsage;

use crate::error::ChatAdapterError;
use crate::types::{ChatCompletionChunk, ChatUsage};

/// Accumulates streaming state for a single tool call index.
#[derive(Debug, Clone)]
struct ToolCallAccumulator {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
    added_emitted: bool,
    done_emitted: bool,
    item_id: ResponseItemId,
}

/// Translates a stream of OpenAI `ChatCompletionChunk` events into Codex `ResponseEvent` items.
///
/// Implements deterministic lifecycle state transitions:
/// `Created` -> `OutputItemAdded` -> `OutputTextDelta` / `ToolCallInputDelta` -> `OutputItemDone` -> `Completed`.
pub struct ChatCompletionStreamTranslator {
    response_id: Option<String>,
    created_emitted: bool,
    text_active: bool,
    accumulated_text: String,
    tool_calls: BTreeMap<usize, ToolCallAccumulator>,
    final_usage: Option<TokenUsage>,
    completed: bool,
}

impl Default for ChatCompletionStreamTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatCompletionStreamTranslator {
    pub fn new() -> Self {
        Self {
            response_id: None,
            created_emitted: false,
            text_active: false,
            accumulated_text: String::new(),
            tool_calls: BTreeMap::new(),
            final_usage: None,
            completed: false,
        }
    }

    /// Feeds a decoded `ChatCompletionChunk` into the translator, returning newly emitted `ResponseEvent`s.
    pub fn feed_chunk(
        &mut self,
        chunk: &ChatCompletionChunk,
    ) -> Result<Vec<ResponseEvent>, ChatAdapterError> {
        if self.completed {
            return Ok(Vec::new());
        }

        let mut events = Vec::new();

        // Initialize response ID and emit Created event on first chunk
        if !self.created_emitted {
            let resp_id = chunk.id.clone();
            self.response_id = Some(resp_id.clone());
            events.push(ResponseEvent::Created {
                response_id: Some(resp_id),
            });
            self.created_emitted = true;
        }

        let resp_id = self
            .response_id
            .clone()
            .unwrap_or_else(|| "chat-resp".to_string());

        // Capture usage statistics if present
        if let Some(usage) = &chunk.usage {
            self.final_usage = Some(map_usage(usage));
        }

        // Process choices
        for choice in &chunk.choices {
            // Invariant: single choice constraint (choice.index == 0)
            if choice.index != 0 {
                return Err(ChatAdapterError::MultipleChoicesUnsupported(choice.index));
            }

            // 1. Text deltas
            if let Some(content) = choice.delta.content.as_ref().filter(|c| !c.is_empty()) {
                if !self.text_active {
                    let text_item_id = ResponseItemId::from_server(format!("chat-msg-{resp_id}"));
                    events.push(ResponseEvent::OutputItemAdded(ResponseItem::Message {
                        id: Some(text_item_id),
                        role: "assistant".to_string(),
                        content: vec![ContentItem::OutputText {
                            text: String::new(),
                        }],
                        phase: None,
                        internal_chat_message_metadata_passthrough: None,
                    }));
                    self.text_active = true;
                }

                events.push(ResponseEvent::OutputTextDelta(content.clone()));
                self.accumulated_text.push_str(content);
            }

            // 2. Tool call deltas
            if let Some(tool_calls) = &choice.delta.tool_calls {
                // If text was active, finalize text item before streaming tool calls
                if self.text_active {
                    let text_item_id = ResponseItemId::from_server(format!("chat-msg-{resp_id}"));
                    events.push(ResponseEvent::OutputItemDone(ResponseItem::Message {
                        id: Some(text_item_id),
                        role: "assistant".to_string(),
                        content: vec![ContentItem::OutputText {
                            text: self.accumulated_text.clone(),
                        }],
                        phase: None,
                        internal_chat_message_metadata_passthrough: None,
                    }));
                    self.text_active = false;
                }

                for call in tool_calls {
                    let entry =
                        self.tool_calls
                            .entry(call.index)
                            .or_insert_with(|| ToolCallAccumulator {
                                id: None,
                                name: None,
                                arguments: String::new(),
                                added_emitted: false,
                                done_emitted: false,
                                item_id: ResponseItemId::from_server(format!(
                                    "chat-tool-{resp_id}-{}",
                                    call.index
                                )),
                            });

                    if let Some(id) = &call.id {
                        entry.id = Some(id.clone());
                    }
                    if let Some(name) = call.function.as_ref().and_then(|f| f.name.as_ref()) {
                        entry.name = Some(name.clone());
                    }

                    // Emit OutputItemAdded when call_id and name are first known
                    if !entry.added_emitted && entry.id.is_some() && entry.name.is_some() {
                        events.push(ResponseEvent::OutputItemAdded(ResponseItem::FunctionCall {
                            id: Some(entry.item_id.clone()),
                            name: entry.name.clone().unwrap(),
                            namespace: None,
                            arguments: String::new(),
                            encrypted_function_args: None,
                            call_id: entry.id.clone().unwrap(),
                            internal_chat_message_metadata_passthrough: None,
                        }));
                        entry.added_emitted = true;
                    }

                    // Emit ToolCallInputDelta for argument fragments
                    if let Some(args_delta) = call
                        .function
                        .as_ref()
                        .and_then(|f| f.arguments.as_ref())
                        .filter(|a| !a.is_empty())
                    {
                        events.push(ResponseEvent::ToolCallInputDelta {
                            item_id: entry.item_id.to_string(),
                            call_id: entry.id.clone(),
                            delta: args_delta.clone(),
                        });
                        entry.arguments.push_str(args_delta);
                    }
                }
            }

            // 3. Finish reason handling
            if let Some(finish_reason) = &choice.finish_reason {
                match finish_reason.as_str() {
                    "stop" => {
                        self.flush_open_items(&resp_id, &mut events);
                    }
                    "tool_calls" | "function_call" => {
                        self.flush_open_items(&resp_id, &mut events);
                    }
                    "length" => {
                        return Err(ChatAdapterError::StreamIncomplete(
                            "Maximum token limit or context length exceeded (finish_reason = length)"
                                .to_string(),
                        ));
                    }
                    "content_filter" => {
                        return Err(ChatAdapterError::ContentFilterTriggered(
                            "Content filter flagged response (finish_reason = content_filter)"
                                .to_string(),
                        ));
                    }
                    other => {
                        return Err(ChatAdapterError::UnexpectedFinishReason(format!(
                            "Unrecognized finish reason: '{other}'"
                        )));
                    }
                }
            }
        }

        Ok(events)
    }

    /// Flushes any open active text message or tool calls.
    fn flush_open_items(&mut self, resp_id: &str, events: &mut Vec<ResponseEvent>) {
        if self.text_active {
            let text_item_id = ResponseItemId::from_server(format!("chat-msg-{resp_id}"));
            events.push(ResponseEvent::OutputItemDone(ResponseItem::Message {
                id: Some(text_item_id),
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: self.accumulated_text.clone(),
                }],
                phase: None,
                internal_chat_message_metadata_passthrough: None,
            }));
            self.text_active = false;
        }

        for (idx, acc) in &mut self.tool_calls {
            if !acc.done_emitted {
                let call_id = acc.id.clone().unwrap_or_else(|| format!("call_{idx}"));
                let name = acc.name.clone().unwrap_or_default();

                if !acc.added_emitted {
                    events.push(ResponseEvent::OutputItemAdded(ResponseItem::FunctionCall {
                        id: Some(acc.item_id.clone()),
                        name: name.clone(),
                        namespace: None,
                        arguments: String::new(),
                        encrypted_function_args: None,
                        call_id: call_id.clone(),
                        internal_chat_message_metadata_passthrough: None,
                    }));
                    acc.added_emitted = true;
                }

                events.push(ResponseEvent::OutputItemDone(ResponseItem::FunctionCall {
                    id: Some(acc.item_id.clone()),
                    name,
                    namespace: None,
                    arguments: acc.arguments.clone(),
                    encrypted_function_args: None,
                    call_id,
                    internal_chat_message_metadata_passthrough: None,
                }));
                acc.done_emitted = true;
            }
        }
    }

    /// Concludes the stream, flushing any pending items and emitting `ResponseEvent::Completed`.
    pub fn finish(&mut self) -> Result<Vec<ResponseEvent>, ChatAdapterError> {
        if self.completed {
            return Ok(Vec::new());
        }

        let resp_id = self
            .response_id
            .clone()
            .unwrap_or_else(|| "chat-resp".to_string());
        let mut events = Vec::new();

        self.flush_open_items(&resp_id, &mut events);

        events.push(ResponseEvent::Completed {
            response_id: resp_id,
            token_usage: self.final_usage.take(),
            usage_metadata: None,
            end_turn: Some(true),
        });

        self.completed = true;
        Ok(events)
    }

    /// Convenience helper when encountering `data: [DONE]`.
    pub fn feed_done(&mut self) -> Result<Vec<ResponseEvent>, ChatAdapterError> {
        self.finish()
    }
}

/// Helper to parse a single Server-Sent Events (SSE) data line into a `ChatCompletionChunk`.
///
/// Returns:
/// - `Ok(Some(chunk))` for `data: { ... }`
/// - `Ok(None)` for empty lines, comment lines (`: ...`), or `data: [DONE]`
/// - `Err(ChatAdapterError)` on invalid JSON payload
pub fn decode_sse_line(line: &str) -> Result<Option<ChatCompletionChunk>, ChatAdapterError> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with(':') {
        return Ok(None);
    }

    if let Some(payload) = trimmed.strip_prefix("data:") {
        let payload = payload.trim();
        if payload == "[DONE]" {
            return Ok(None);
        }
        let chunk: ChatCompletionChunk = serde_json::from_str(payload).map_err(|e| {
            ChatAdapterError::InvalidStreamChunk(format!("Failed to parse SSE chunk JSON: {e}"))
        })?;
        return Ok(Some(chunk));
    }

    Ok(None)
}

/// Checks if an SSE line is the terminal `data: [DONE]` marker.
pub fn is_done_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "data: [DONE]" || trimmed == "data:[DONE]"
}

fn map_usage(usage: &ChatUsage) -> TokenUsage {
    TokenUsage {
        input_tokens: usage.prompt_tokens,
        cached_input_tokens: usage
            .prompt_tokens_details
            .as_ref()
            .and_then(|d| d.cached_tokens)
            .unwrap_or(0),
        cache_write_input_tokens: 0,
        output_tokens: usage.completion_tokens,
        reasoning_output_tokens: usage
            .completion_tokens_details
            .as_ref()
            .and_then(|d| d.reasoning_tokens)
            .unwrap_or(0),
        total_tokens: usage.total_tokens,
        codex_rollout_budget_units: None,
    }
}
