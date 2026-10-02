use std::collections::BTreeMap;

use codex_api::ResponseEvent;
use codex_protocol::ResponseItemId;
use codex_protocol::models::{ContentItem, ResponseItem};
use codex_protocol::protocol::TokenUsage;

use crate::error::ChatAdapterError;
use crate::types::{ChatCompletionChunk, ChatUsage};

/// Finish kind indicating whether the turn ended normally or requires tool execution follow-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatFinishKind {
    Stop,
    ToolCalls,
}

/// Accumulates streaming state for a single tool call index.
#[derive(Debug, Clone)]
struct ToolCallAccumulator {
    id: Option<String>,
    name: Option<String>,
    pending_argument_fragments: Vec<String>,
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
    stream_model: Option<String>,
    created_emitted: bool,
    text_active: bool,
    text_done: bool,
    accumulated_text: String,
    tool_calls: BTreeMap<usize, ToolCallAccumulator>,
    finish_kind: Option<ChatFinishKind>,
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
            stream_model: None,
            created_emitted: false,
            text_active: false,
            text_done: false,
            accumulated_text: String::new(),
            tool_calls: BTreeMap::new(),
            finish_kind: None,
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
            return Err(ChatAdapterError::AlreadyCompleted);
        }

        // Validate response ID: non-empty and consistent across all chunks
        let chunk_id = chunk.id.trim();
        if chunk_id.is_empty() {
            return Err(ChatAdapterError::MissingResponseId);
        }

        if let Some(existing_id) = &self.response_id {
            if existing_id != chunk_id {
                return Err(ChatAdapterError::ResponseIdMismatch {
                    expected: existing_id.clone(),
                    actual: chunk_id.to_string(),
                });
            }
        } else {
            self.response_id = Some(chunk_id.to_string());
        }

        // Validate response model: first non-empty value sets model; subsequent values must match
        if let Some(model) = &chunk.model {
            let model_trimmed = model.trim();
            if !model_trimmed.is_empty() {
                if let Some(existing_model) = &self.stream_model {
                    if existing_model != model_trimmed {
                        return Err(ChatAdapterError::ResponseModelMismatch {
                            expected: existing_model.clone(),
                            actual: model_trimmed.to_string(),
                        });
                    }
                } else {
                    self.stream_model = Some(model_trimmed.to_string());
                }
            }
        }

        let resp_id = self.response_id.as_ref().unwrap().clone();
        let mut events = Vec::new();

        // Emit Created event on first valid chunk
        if !self.created_emitted {
            events.push(ResponseEvent::Created {
                response_id: Some(resp_id.clone()),
            });
            self.created_emitted = true;
        }

        // Capture and validate usage statistics if present
        if let Some(usage) = &chunk.usage {
            self.final_usage = Some(validate_and_map_usage(usage)?);
        }

        // Validate choice count: 0 choices allowed for usage-only chunks; >1 choices rejected
        if chunk.choices.len() > 1 {
            return Err(ChatAdapterError::MultipleChoicesUnsupported(
                chunk.choices.len() as u32,
            ));
        }

        if let Some(choice) = chunk.choices.first() {
            // Invariant: single choice constraint (choice.index == 0)
            if choice.index != 0 {
                return Err(ChatAdapterError::MultipleChoicesUnsupported(choice.index));
            }

            // 1. Text deltas
            if let Some(content) = choice.delta.content.as_ref().filter(|c| !c.is_empty()) {
                if self.text_done {
                    return Err(ChatAdapterError::InvalidStreamState(
                        "Cannot stream text delta after text output has already concluded"
                            .to_string(),
                    ));
                }

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
                    self.text_done = true;
                }

                for call in tool_calls {
                    // Validate stream tool type: if present, must be "function"
                    if let Some(tool_type) = call.r#type.as_deref().filter(|t| *t != "function") {
                        return Err(ChatAdapterError::UnsupportedToolCallType(
                            tool_type.to_string(),
                        ));
                    }

                    let entry =
                        self.tool_calls
                            .entry(call.index)
                            .or_insert_with(|| ToolCallAccumulator {
                                id: None,
                                name: None,
                                pending_argument_fragments: Vec::new(),
                                arguments: String::new(),
                                added_emitted: false,
                                done_emitted: false,
                                item_id: ResponseItemId::from_server(format!(
                                    "chat-tool-{resp_id}-{}",
                                    call.index
                                )),
                            });

                    // Tool identity immutability: once ID is known, conflicting ID must error
                    if let Some(id) = &call.id {
                        if let Some(existing_id) = &entry.id {
                            if existing_id != id {
                                return Err(ChatAdapterError::ToolCallIdentityMismatch(format!(
                                    "Tool call at index {} changed ID from '{existing_id}' to '{id}'",
                                    call.index
                                )));
                            }
                        } else {
                            entry.id = Some(id.clone());
                        }
                    }

                    // Tool identity immutability: once name is known, conflicting name must error
                    if let Some(name) = call.function.as_ref().and_then(|f| f.name.as_ref()) {
                        if let Some(existing_name) = &entry.name {
                            if existing_name != name {
                                return Err(ChatAdapterError::ToolCallIdentityMismatch(format!(
                                    "Tool call at index {} changed function name from '{existing_name}' to '{name}'",
                                    call.index
                                )));
                            }
                        } else {
                            entry.name = Some(name.clone());
                        }
                    }

                    let new_args_delta = call
                        .function
                        .as_ref()
                        .and_then(|f| f.arguments.as_ref())
                        .filter(|a| !a.is_empty());

                    // When both id and name are known, emit OutputItemAdded and flush pending deltas
                    if let (Some(call_id), Some(name)) = (&entry.id, &entry.name) {
                        if !entry.added_emitted {
                            events.push(ResponseEvent::OutputItemAdded(
                                ResponseItem::FunctionCall {
                                    id: Some(entry.item_id.clone()),
                                    name: name.clone(),
                                    namespace: None,
                                    arguments: String::new(),
                                    encrypted_function_args: None,
                                    call_id: call_id.clone(),
                                    internal_chat_message_metadata_passthrough: None,
                                },
                            ));
                            entry.added_emitted = true;

                            // Flush buffered argument fragments in original order
                            for buffered in entry.pending_argument_fragments.drain(..) {
                                events.push(ResponseEvent::ToolCallInputDelta {
                                    item_id: entry.item_id.to_string(),
                                    call_id: entry.id.clone(),
                                    delta: buffered,
                                });
                            }
                        }

                        // Emit current delta if present
                        if let Some(args_delta) = new_args_delta {
                            events.push(ResponseEvent::ToolCallInputDelta {
                                item_id: entry.item_id.to_string(),
                                call_id: entry.id.clone(),
                                delta: args_delta.clone(),
                            });
                            entry.arguments.push_str(args_delta);
                        }
                    } else if let Some(args_delta) = new_args_delta {
                        // Tool identity incomplete: buffer argument fragments until id + name arrive
                        entry.pending_argument_fragments.push(args_delta.clone());
                        entry.arguments.push_str(args_delta);
                    }
                }
            }

            // 3. Finish reason handling
            if let Some(finish_reason) = &choice.finish_reason {
                match finish_reason.as_str() {
                    "stop" => {
                        self.finish_kind = Some(ChatFinishKind::Stop);
                        self.flush_open_items(&resp_id, &mut events)?;
                    }
                    "tool_calls" | "function_call" => {
                        self.finish_kind = Some(ChatFinishKind::ToolCalls);
                        self.flush_open_items(&resp_id, &mut events)?;
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
    fn flush_open_items(
        &mut self,
        resp_id: &str,
        events: &mut Vec<ResponseEvent>,
    ) -> Result<(), ChatAdapterError> {
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
            self.text_done = true;
        }

        for (idx, acc) in &mut self.tool_calls {
            if !acc.done_emitted {
                let (call_id, name) = match (&acc.id, &acc.name) {
                    (Some(id), Some(name)) => (id.clone(), name.clone()),
                    (id, name) => {
                        return Err(ChatAdapterError::IncompleteToolCall {
                            index: *idx,
                            missing_id: id.is_none(),
                            missing_name: name.is_none(),
                        });
                    }
                };

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

                    for buffered in acc.pending_argument_fragments.drain(..) {
                        events.push(ResponseEvent::ToolCallInputDelta {
                            item_id: acc.item_id.to_string(),
                            call_id: Some(call_id.clone()),
                            delta: buffered,
                        });
                    }
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

        Ok(())
    }

    /// Concludes the stream, flushing any pending items and emitting `ResponseEvent::Completed`.
    pub fn finish(&mut self) -> Result<Vec<ResponseEvent>, ChatAdapterError> {
        if self.completed {
            return Err(ChatAdapterError::AlreadyCompleted);
        }

        let resp_id = self
            .response_id
            .clone()
            .ok_or(ChatAdapterError::MissingResponseId)?;

        let finish_kind = self
            .finish_kind
            .ok_or(ChatAdapterError::MissingFinishReason)?;

        let mut events = Vec::new();
        self.flush_open_items(&resp_id, &mut events)?;

        let end_turn = match finish_kind {
            ChatFinishKind::Stop => Some(true),
            ChatFinishKind::ToolCalls => Some(false),
        };

        events.push(ResponseEvent::Completed {
            response_id: resp_id,
            token_usage: self.final_usage.take(),
            usage_metadata: None,
            end_turn,
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
/// - `Err(ChatAdapterError)` on invalid JSON payload or malformed SSE line
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

    Err(ChatAdapterError::InvalidStreamChunk(format!(
        "Malformed SSE line: '{trimmed}'"
    )))
}

/// Checks if an SSE line is the terminal `data: [DONE]` marker.
pub fn is_done_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "data: [DONE]" || trimmed == "data:[DONE]"
}

fn validate_and_map_usage(usage: &ChatUsage) -> Result<TokenUsage, ChatAdapterError> {
    if usage.prompt_tokens < 0 || usage.completion_tokens < 0 || usage.total_tokens < 0 {
        return Err(ChatAdapterError::InvalidUsage(format!(
            "Negative token counts: prompt={}, completion={}, total={}",
            usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
        )));
    }

    let cached = if let Some(p) = &usage.prompt_tokens_details {
        if let Some(cached) = p.cached_tokens {
            if cached < 0 {
                return Err(ChatAdapterError::InvalidUsage(format!(
                    "Negative cached_tokens: {cached}"
                )));
            }
            cached
        } else {
            0
        }
    } else {
        0
    };

    let reasoning = if let Some(c) = &usage.completion_tokens_details {
        if let Some(reasoning) = c.reasoning_tokens {
            if reasoning < 0 {
                return Err(ChatAdapterError::InvalidUsage(format!(
                    "Negative reasoning_tokens: {reasoning}"
                )));
            }
            reasoning
        } else {
            0
        }
    } else {
        0
    };

    Ok(TokenUsage {
        input_tokens: usage.prompt_tokens,
        cached_input_tokens: cached,
        cache_write_input_tokens: 0,
        output_tokens: usage.completion_tokens,
        reasoning_output_tokens: reasoning,
        total_tokens: usage.total_tokens,
        codex_rollout_budget_units: None,
    })
}
