use std::collections::{BTreeMap, HashSet};

use codex_api::ResponseEvent;
use codex_protocol::ResponseItemId;
use codex_protocol::models::{ContentItem, ReasoningItemContent, ResponseItem};
use codex_protocol::protocol::TokenUsage;

use super::continuation::{AnthropicContinuationState, NativeThinkingBlock};
use super::error::AnthropicAdapterError;
use super::types::{
    AnthropicStreamBlockDelta, AnthropicStreamBlockStart, AnthropicStreamEvent, AnthropicUsage,
};

/// Tracks active content blocks during streaming.
#[derive(Debug)]
enum ActiveContentBlock {
    Text {
        item_id: ResponseItemId,
        accumulated_text: String,
    },
    ToolUse {
        item_id: ResponseItemId,
        call_id: String,
        name: String,
        arguments: String,
    },
    Thinking {
        #[allow(dead_code)]
        item_id: ResponseItemId,
        reasoning_id: String,
        thinking: String,
        signature: String,
    },
    RedactedThinking {
        #[allow(dead_code)]
        item_id: ResponseItemId,
        reasoning_id: String,
        #[allow(dead_code)]
        data: String,
    },
}

/// Translates a stream of Anthropic Messages SSE events into Codex `ResponseEvent` items.
pub struct AnthropicStreamTranslator {
    response_id: Option<String>,
    stream_model: Option<String>,
    started: bool,
    completed: bool,
    active_blocks: BTreeMap<u64, ActiveContentBlock>,
    closed_blocks: HashSet<u64>,
    stop_reason: Option<String>,
    continuation_state: AnthropicContinuationState,
    accumulated_input_tokens: i64,
    accumulated_output_tokens: i64,
    accumulated_cached_input_tokens: i64,
    accumulated_cache_write_input_tokens: i64,
    sse_data_buffer: String,
    sse_event_type: Option<String>,
}

impl Default for AnthropicStreamTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl AnthropicStreamTranslator {
    pub fn new() -> Self {
        Self {
            response_id: None,
            stream_model: None,
            started: false,
            completed: false,
            active_blocks: BTreeMap::new(),
            closed_blocks: HashSet::new(),
            stop_reason: None,
            continuation_state: AnthropicContinuationState::new(),
            accumulated_input_tokens: 0,
            accumulated_output_tokens: 0,
            accumulated_cached_input_tokens: 0,
            accumulated_cache_write_input_tokens: 0,
            sse_data_buffer: String::new(),
            sse_event_type: None,
        }
    }

    /// Accesses the captured native continuation state for subsequent conversation turns.
    pub fn continuation_state(&self) -> &AnthropicContinuationState {
        &self.continuation_state
    }

    /// Returns true if the stream reached a terminal `message_stop` event.
    pub fn is_completed(&self) -> bool {
        self.completed
    }

    /// Returns the response ID if the stream has started.
    pub fn response_id(&self) -> Option<&str> {
        self.response_id.as_deref()
    }

    /// Returns the model name reported by the server.
    pub fn stream_model(&self) -> Option<&str> {
        self.stream_model.as_deref()
    }

    /// Feeds a chunk containing one or more SSE lines into the translator.
    pub fn feed_sse_chunk(
        &mut self,
        chunk: &str,
    ) -> Result<Vec<ResponseEvent>, AnthropicAdapterError> {
        let mut events = Vec::new();
        for line in chunk.lines() {
            let evs = self.feed_line(line)?;
            events.extend(evs);
        }
        if chunk.ends_with("\n\n") || chunk.ends_with("\r\n\r\n") {
            let evs = self.feed_line("")?;
            events.extend(evs);
        }
        Ok(events)
    }

    /// Feeds a single SSE line into the translator.
    pub fn feed_line(&mut self, line: &str) -> Result<Vec<ResponseEvent>, AnthropicAdapterError> {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let trimmed_clean = trimmed.trim();

        if trimmed_clean.is_empty() {
            if !self.sse_data_buffer.is_empty() {
                let data = std::mem::take(&mut self.sse_data_buffer);
                self.sse_event_type = None;
                let event: AnthropicStreamEvent = serde_json::from_str(&data).map_err(|e| {
                    AnthropicAdapterError::MalformedSse(format!(
                        "Failed to parse SSE JSON: {e} - buffer: '{data}'"
                    ))
                })?;
                return self.feed_event(&event);
            }
            return Ok(Vec::new());
        }

        if trimmed_clean.starts_with(':') {
            return Ok(Vec::new());
        }

        if let Some(event_type) = trimmed_clean.strip_prefix("event:") {
            self.sse_event_type = Some(event_type.trim().to_string());
            return Ok(Vec::new());
        }

        if let Some(data_part) = trimmed_clean.strip_prefix("data:") {
            let payload = data_part.trim();
            if !self.sse_data_buffer.is_empty() {
                self.sse_data_buffer.push('\n');
            }
            self.sse_data_buffer.push_str(payload);

            if payload.starts_with('{') && payload.ends_with('}') {
                let parsed = serde_json::from_str::<AnthropicStreamEvent>(&self.sse_data_buffer);
                if let Ok(event) = parsed {
                    self.sse_data_buffer.clear();
                    self.sse_event_type = None;
                    return self.feed_event(&event);
                }
            }
            return Ok(Vec::new());
        }

        if trimmed_clean.starts_with('{') && trimmed_clean.ends_with('}') {
            let event: AnthropicStreamEvent = serde_json::from_str(trimmed_clean).map_err(|e| {
                AnthropicAdapterError::MalformedSse(format!(
                    "Failed to parse JSON stream event: {e} - input: '{trimmed_clean}'"
                ))
            })?;
            return self.feed_event(&event);
        }

        Err(AnthropicAdapterError::MalformedSse(format!(
            "Malformed SSE stream line: '{trimmed_clean}'"
        )))
    }

    /// Feeds a strongly typed `AnthropicStreamEvent` into the state machine.
    pub fn feed_event(
        &mut self,
        event: &AnthropicStreamEvent,
    ) -> Result<Vec<ResponseEvent>, AnthropicAdapterError> {
        if self.completed {
            return Err(AnthropicAdapterError::StreamAlreadyCompleted);
        }

        match event {
            AnthropicStreamEvent::Error { error } => {
                Err(AnthropicAdapterError::StreamApiError(error.clone()))
            }

            AnthropicStreamEvent::Ping => Ok(Vec::new()),

            AnthropicStreamEvent::MessageStart { message } => {
                if self.started {
                    return Err(AnthropicAdapterError::StreamMessageAlreadyStarted);
                }
                if message.id.trim().is_empty() {
                    return Err(AnthropicAdapterError::StreamMissingMessageId);
                }

                self.started = true;
                self.response_id = Some(message.id.clone());
                self.stream_model = Some(message.model.clone());

                if let Some(usage) = &message.usage {
                    self.update_usage(usage)?;
                }

                let events = vec![
                    ResponseEvent::Created {
                        response_id: Some(message.id.clone()),
                    },
                    ResponseEvent::ServerModel(message.model.clone()),
                ];
                Ok(events)
            }

            AnthropicStreamEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                if !self.started {
                    return Err(AnthropicAdapterError::StreamMessageNotStarted);
                }
                if self.active_blocks.contains_key(index) || self.closed_blocks.contains(index) {
                    return Err(AnthropicAdapterError::StreamBlockAlreadyStarted(*index));
                }

                let resp_id = self
                    .response_id
                    .as_deref()
                    .ok_or(AnthropicAdapterError::StreamMissingMessageId)?;

                let mut events = Vec::new();

                match content_block {
                    AnthropicStreamBlockStart::Text { text } => {
                        let item_id =
                            ResponseItemId::from_server(format!("anthropic-msg-{resp_id}-{index}"));
                        events.push(ResponseEvent::OutputItemAdded(ResponseItem::Message {
                            id: Some(item_id.clone()),
                            role: "assistant".to_string(),
                            content: vec![ContentItem::OutputText {
                                text: String::new(),
                            }],
                            phase: None,
                            internal_chat_message_metadata_passthrough: None,
                        }));
                        if !text.is_empty() {
                            events.push(ResponseEvent::OutputTextDelta(text.clone()));
                        }
                        self.active_blocks.insert(
                            *index,
                            ActiveContentBlock::Text {
                                item_id,
                                accumulated_text: text.clone(),
                            },
                        );
                    }

                    AnthropicStreamBlockStart::ToolUse { id, name } => {
                        let item_id = ResponseItemId::from_server(format!(
                            "anthropic-tool-{resp_id}-{index}"
                        ));
                        events.push(ResponseEvent::OutputItemAdded(ResponseItem::FunctionCall {
                            id: Some(item_id.clone()),
                            name: name.clone(),
                            namespace: None,
                            arguments: String::new(),
                            encrypted_function_args: None,
                            call_id: id.clone(),
                            internal_chat_message_metadata_passthrough: None,
                        }));
                        self.active_blocks.insert(
                            *index,
                            ActiveContentBlock::ToolUse {
                                item_id,
                                call_id: id.clone(),
                                name: name.clone(),
                                arguments: String::new(),
                            },
                        );
                    }

                    AnthropicStreamBlockStart::Thinking { thinking } => {
                        let reasoning_id =
                            AnthropicContinuationState::deterministic_reasoning_id(resp_id, *index);
                        let item_id = ResponseItemId::from_server(reasoning_id.clone());
                        events.push(ResponseEvent::OutputItemAdded(ResponseItem::Reasoning {
                            id: Some(item_id.clone()),
                            summary: Vec::new(),
                            content: Some(vec![ReasoningItemContent::ReasoningText {
                                text: thinking.clone(),
                            }]),
                            encrypted_content: None,
                            internal_chat_message_metadata_passthrough: None,
                        }));
                        if !thinking.is_empty() {
                            events.push(ResponseEvent::ReasoningContentDelta {
                                delta: thinking.clone(),
                                content_index: 0,
                            });
                        }
                        self.active_blocks.insert(
                            *index,
                            ActiveContentBlock::Thinking {
                                item_id,
                                reasoning_id,
                                thinking: thinking.clone(),
                                signature: String::new(),
                            },
                        );
                    }

                    AnthropicStreamBlockStart::RedactedThinking { data } => {
                        let reasoning_id =
                            AnthropicContinuationState::deterministic_reasoning_id(resp_id, *index);
                        let item_id = ResponseItemId::from_server(reasoning_id.clone());
                        self.continuation_state.insert_reasoning_block(
                            &reasoning_id,
                            NativeThinkingBlock::RedactedThinking { data: data.clone() },
                        );
                        events.push(ResponseEvent::OutputItemAdded(ResponseItem::Reasoning {
                            id: Some(item_id.clone()),
                            summary: Vec::new(),
                            content: None,
                            encrypted_content: None,
                            internal_chat_message_metadata_passthrough: None,
                        }));
                        self.active_blocks.insert(
                            *index,
                            ActiveContentBlock::RedactedThinking {
                                item_id,
                                reasoning_id,
                                data: data.clone(),
                            },
                        );
                    }
                }

                Ok(events)
            }

            AnthropicStreamEvent::ContentBlockDelta { index, delta } => {
                if !self.started {
                    return Err(AnthropicAdapterError::StreamMessageNotStarted);
                }
                let block = self
                    .active_blocks
                    .get_mut(index)
                    .ok_or(AnthropicAdapterError::StreamBlockNotStarted(*index))?;

                let mut events = Vec::new();

                match (block, delta) {
                    (
                        ActiveContentBlock::Text {
                            accumulated_text, ..
                        },
                        AnthropicStreamBlockDelta::TextDelta { text },
                    ) => {
                        accumulated_text.push_str(text);
                        events.push(ResponseEvent::OutputTextDelta(text.clone()));
                    }

                    (
                        ActiveContentBlock::ToolUse {
                            item_id,
                            call_id,
                            arguments,
                            ..
                        },
                        AnthropicStreamBlockDelta::InputJsonDelta { partial_json },
                    ) => {
                        arguments.push_str(partial_json);
                        events.push(ResponseEvent::ToolCallInputDelta {
                            item_id: item_id.to_string(),
                            call_id: Some(call_id.clone()),
                            delta: partial_json.clone(),
                        });
                    }

                    (
                        ActiveContentBlock::Thinking { thinking, .. },
                        AnthropicStreamBlockDelta::ThinkingDelta { thinking: dt },
                    ) => {
                        thinking.push_str(dt);
                        events.push(ResponseEvent::ReasoningContentDelta {
                            delta: dt.clone(),
                            content_index: 0,
                        });
                    }

                    (
                        ActiveContentBlock::Thinking { signature, .. },
                        AnthropicStreamBlockDelta::SignatureDelta { signature: ds },
                    ) => {
                        signature.push_str(ds);
                    }

                    (b, d) => {
                        return Err(AnthropicAdapterError::StreamInvalidState(format!(
                            "Mismatched block delta {d:?} for active block {b:?} at index {index}"
                        )));
                    }
                }

                Ok(events)
            }

            AnthropicStreamEvent::ContentBlockStop { index } => {
                if !self.started {
                    return Err(AnthropicAdapterError::StreamMessageNotStarted);
                }
                let block = self
                    .active_blocks
                    .remove(index)
                    .ok_or(AnthropicAdapterError::StreamBlockNotStarted(*index))?;
                self.closed_blocks.insert(*index);

                let mut events = Vec::new();

                match block {
                    ActiveContentBlock::Text {
                        item_id,
                        accumulated_text,
                    } => {
                        events.push(ResponseEvent::OutputItemDone(ResponseItem::Message {
                            id: Some(item_id),
                            role: "assistant".to_string(),
                            content: vec![ContentItem::OutputText {
                                text: accumulated_text,
                            }],
                            phase: None,
                            internal_chat_message_metadata_passthrough: None,
                        }));
                    }

                    ActiveContentBlock::ToolUse {
                        item_id,
                        call_id,
                        name,
                        arguments,
                    } => {
                        events.push(ResponseEvent::OutputItemDone(ResponseItem::FunctionCall {
                            id: Some(item_id),
                            name,
                            namespace: None,
                            arguments,
                            encrypted_function_args: None,
                            call_id,
                            internal_chat_message_metadata_passthrough: None,
                        }));
                    }

                    ActiveContentBlock::Thinking {
                        item_id: _,
                        reasoning_id,
                        thinking,
                        signature,
                    } => {
                        self.continuation_state.insert_reasoning_block(
                            &reasoning_id,
                            NativeThinkingBlock::Thinking {
                                thinking: thinking.clone(),
                                signature,
                            },
                        );
                        events.push(ResponseEvent::OutputItemDone(ResponseItem::Reasoning {
                            id: Some(ResponseItemId::from_server(reasoning_id)),
                            summary: Vec::new(),
                            content: Some(vec![ReasoningItemContent::ReasoningText {
                                text: thinking,
                            }]),
                            encrypted_content: None,
                            internal_chat_message_metadata_passthrough: None,
                        }));
                    }

                    ActiveContentBlock::RedactedThinking {
                        item_id: _,
                        reasoning_id,
                        ..
                    } => {
                        events.push(ResponseEvent::OutputItemDone(ResponseItem::Reasoning {
                            id: Some(ResponseItemId::from_server(reasoning_id)),
                            summary: Vec::new(),
                            content: None,
                            encrypted_content: None,
                            internal_chat_message_metadata_passthrough: None,
                        }));
                    }
                }

                Ok(events)
            }

            AnthropicStreamEvent::MessageDelta { delta, usage } => {
                if !self.started {
                    return Err(AnthropicAdapterError::StreamMessageNotStarted);
                }
                if let Some(sr) = &delta.stop_reason {
                    self.stop_reason = Some(sr.clone());
                }
                if let Some(out) = usage.as_ref().and_then(|u| u.output_tokens) {
                    if out < 0 {
                        return Err(AnthropicAdapterError::InvalidUsage(format!(
                            "Negative output_tokens in message_delta: {out}"
                        )));
                    }
                    self.accumulated_output_tokens = out;
                }
                Ok(Vec::new())
            }

            AnthropicStreamEvent::MessageStop => {
                if !self.started {
                    return Err(AnthropicAdapterError::StreamMessageNotStarted);
                }
                if !self.active_blocks.is_empty() {
                    return Err(AnthropicAdapterError::StreamOpenBlocksOnStop(
                        self.active_blocks.keys().cloned().collect(),
                    ));
                }

                let stop_reason = self
                    .stop_reason
                    .as_deref()
                    .ok_or(AnthropicAdapterError::StreamMissingStopReason)?;

                let end_turn = match stop_reason {
                    "end_turn" => Some(true),
                    "tool_use" => Some(false),
                    "stop_sequence" => Some(true),
                    "max_tokens" => return Err(AnthropicAdapterError::MaxTokensExceeded),
                    "model_context_window_exceeded" => {
                        return Err(AnthropicAdapterError::ContextWindowExceeded);
                    }
                    "refusal" => {
                        return Err(AnthropicAdapterError::ModelRefusal(stop_reason.to_string()));
                    }
                    "pause_turn" => {
                        return Err(AnthropicAdapterError::TurnPaused(stop_reason.to_string()));
                    }
                    other => {
                        return Err(AnthropicAdapterError::UnknownStopReason(other.to_string()));
                    }
                };

                self.completed = true;
                let resp_id = self.response_id.clone().unwrap();
                self.continuation_state.message_id = Some(resp_id.clone());

                let total = self.accumulated_input_tokens + self.accumulated_output_tokens;
                let token_usage = TokenUsage {
                    input_tokens: self.accumulated_input_tokens,
                    cached_input_tokens: self.accumulated_cached_input_tokens,
                    cache_write_input_tokens: self.accumulated_cache_write_input_tokens,
                    output_tokens: self.accumulated_output_tokens,
                    reasoning_output_tokens: 0,
                    total_tokens: total,
                    codex_rollout_budget_units: None,
                };

                let events = vec![ResponseEvent::Completed {
                    response_id: resp_id,
                    token_usage: Some(token_usage),
                    usage_metadata: None,
                    end_turn,
                }];

                Ok(events)
            }
        }
    }

    fn update_usage(&mut self, usage: &AnthropicUsage) -> Result<(), AnthropicAdapterError> {
        if usage.input_tokens < 0 || usage.output_tokens < 0 {
            return Err(AnthropicAdapterError::InvalidUsage(format!(
                "Negative token count in message_start usage: input={}, output={}",
                usage.input_tokens, usage.output_tokens
            )));
        }

        self.accumulated_input_tokens = usage.input_tokens;
        self.accumulated_output_tokens = usage.output_tokens;

        if let Some(read) = usage.cache_read_input_tokens {
            if read < 0 {
                return Err(AnthropicAdapterError::InvalidUsage(format!(
                    "Negative cache_read_input_tokens: {read}"
                )));
            }
            self.accumulated_cached_input_tokens = read;
        }

        if let Some(creation) = usage.cache_creation_input_tokens {
            if creation < 0 {
                return Err(AnthropicAdapterError::InvalidUsage(format!(
                    "Negative cache_creation_input_tokens: {creation}"
                )));
            }
            self.accumulated_cache_write_input_tokens = creation;
        }

        Ok(())
    }
}
