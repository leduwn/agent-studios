use codex_api::ResponseEvent;
use codex_protocol::ResponseItemId;
use codex_protocol::models::{ContentItem, ReasoningItemContent, ResponseItem};
use codex_protocol::protocol::TokenUsage;

use super::continuation::GeminiContinuationState;
use super::error::GeminiAdapterError;
use super::types::{GeminiContent, GeminiGenerateContentResponse, GeminiPart, GeminiUsageMetadata};

/// Tracks active text block during streaming.
#[derive(Debug)]
struct ActiveText {
    item_id: ResponseItemId,
    accumulated_text: String,
    thought_signature: Option<String>,
}

/// Tracks active reasoning block during streaming.
#[derive(Debug)]
struct ActiveReasoning {
    item_id: ResponseItemId,
    reasoning_id: String,
    accumulated_text: String,
    thought_signature: Option<String>,
}

/// Translates a stream of Gemini generateContent SSE lines or response chunks into Codex `ResponseEvent` items.
pub struct GeminiStreamTranslator {
    response_id: Option<String>,
    model_version: Option<String>,
    started: bool,
    completed: bool,
    active_text: Option<ActiveText>,
    active_reasoning: Option<ActiveReasoning>,
    tool_calls_emitted: usize,
    output_index: usize,
    finish_reason: Option<String>,
    usage_metadata: Option<GeminiUsageMetadata>,
    model_parts: Vec<GeminiPart>,
    continuation_state: GeminiContinuationState,
    sse_data_buffer: String,
}

impl Default for GeminiStreamTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl GeminiStreamTranslator {
    pub fn new() -> Self {
        Self {
            response_id: None,
            model_version: None,
            started: false,
            completed: false,
            active_text: None,
            active_reasoning: None,
            tool_calls_emitted: 0,
            output_index: 0,
            finish_reason: None,
            usage_metadata: None,
            model_parts: Vec::new(),
            continuation_state: GeminiContinuationState::new(),
            sse_data_buffer: String::new(),
        }
    }

    /// Accesses the captured native continuation state for subsequent conversation turns.
    pub fn continuation_state(&self) -> &GeminiContinuationState {
        &self.continuation_state
    }

    /// Returns mutable access to continuation state if needed.
    pub fn continuation_state_mut(&mut self) -> &mut GeminiContinuationState {
        &mut self.continuation_state
    }

    /// Returns true if the stream reached a valid terminal completion.
    pub fn is_completed(&self) -> bool {
        self.completed
    }

    /// Returns the response ID if the stream has started.
    pub fn response_id(&self) -> Option<&str> {
        self.response_id.as_deref()
    }

    /// Returns the model version reported by the server.
    pub fn model_version(&self) -> Option<&str> {
        self.model_version.as_deref()
    }

    /// Feeds a chunk containing one or more SSE lines into the translator.
    pub fn feed_sse_chunk(
        &mut self,
        chunk: &str,
    ) -> Result<Vec<ResponseEvent>, GeminiAdapterError> {
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
    pub fn feed_line(&mut self, line: &str) -> Result<Vec<ResponseEvent>, GeminiAdapterError> {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let trimmed_clean = trimmed.trim();

        if trimmed_clean.is_empty() {
            if !self.sse_data_buffer.is_empty() {
                let data = std::mem::take(&mut self.sse_data_buffer);
                let response: GeminiGenerateContentResponse =
                    serde_json::from_str(&data).map_err(|e| {
                        GeminiAdapterError::MalformedSse(format!(
                            "Failed to parse SSE JSON: {e} - buffer: '{data}'"
                        ))
                    })?;
                return self.feed_response(&response);
            }
            return Ok(Vec::new());
        }

        // Ignore SSE comments
        if trimmed_clean.starts_with(':') {
            return Ok(Vec::new());
        }

        // Strip event tag if present
        if trimmed_clean.starts_with("event:") {
            return Ok(Vec::new());
        }

        if let Some(data_part) = trimmed_clean.strip_prefix("data:") {
            let payload = data_part.trim();
            if !self.sse_data_buffer.is_empty() {
                self.sse_data_buffer.push('\n');
            }
            self.sse_data_buffer.push_str(payload);

            if payload.starts_with('{') && payload.ends_with('}') {
                let parsed =
                    serde_json::from_str::<GeminiGenerateContentResponse>(&self.sse_data_buffer);
                if let Ok(response) = parsed {
                    self.sse_data_buffer.clear();
                    return self.feed_response(&response);
                }
            }
            return Ok(Vec::new());
        }

        if trimmed_clean.starts_with('{') && trimmed_clean.ends_with('}') {
            let response: GeminiGenerateContentResponse = serde_json::from_str(trimmed_clean)
                .map_err(|e| {
                    GeminiAdapterError::MalformedSse(format!(
                        "Failed to parse JSON response: {e} - input: '{trimmed_clean}'"
                    ))
                })?;
            return self.feed_response(&response);
        }

        Err(GeminiAdapterError::MalformedSse(format!(
            "Malformed SSE stream line: '{trimmed_clean}'"
        )))
    }

    /// Feeds a strongly typed `GeminiGenerateContentResponse` chunk into the state machine.
    pub fn feed_response(
        &mut self,
        response: &GeminiGenerateContentResponse,
    ) -> Result<Vec<ResponseEvent>, GeminiAdapterError> {
        if self.completed {
            return Err(GeminiAdapterError::AlreadyCompleted);
        }

        let mut events = Vec::new();

        // 1. Prompt feedback check
        if let Some(ref pf) = response.prompt_feedback
            && let Some(ref br) = pf.block_reason
        {
            return Err(GeminiAdapterError::PromptBlocked(br.clone()));
        }

        // 2. Response ID validation and lifecycle initialization
        if let Some(ref resp_id) = response.response_id {
            if let Some(ref current_id) = self.response_id {
                if current_id != resp_id {
                    return Err(GeminiAdapterError::ResponseIdMismatch {
                        expected: current_id.clone(),
                        actual: resp_id.clone(),
                    });
                }
            } else {
                self.response_id = Some(resp_id.clone());
            }
        } else if !self.started {
            return Err(GeminiAdapterError::MissingResponseId);
        }

        // 3. Model version check
        if let Some(ref mv) = response.model_version {
            if let Some(ref current_mv) = self.model_version {
                if current_mv != mv {
                    return Err(GeminiAdapterError::ModelVersionMismatch {
                        expected: current_mv.clone(),
                        actual: mv.clone(),
                    });
                }
            } else {
                self.model_version = Some(mv.clone());
            }
        }

        // 4. Emit Created and ServerModel on first chunk
        if !self.started {
            self.started = true;
            events.push(ResponseEvent::Created {
                response_id: self.response_id.clone(),
            });
            if let Some(ref mv) = self.model_version {
                events.push(ResponseEvent::ServerModel(mv.clone()));
            }
        }

        // 5. Update usage metadata if provided
        if response.usage_metadata.is_some() {
            self.usage_metadata = response.usage_metadata.clone();
        }

        // 6. Process candidate turn
        if let Some(ref candidates) = response.candidates {
            if candidates.len() > 1 {
                return Err(GeminiAdapterError::MultipleCandidatesUnsupported(
                    candidates.len(),
                ));
            }

            if let Some(cand) = candidates.first() {
                // Ensure candidate index is 0
                if cand.index.unwrap_or(0) != 0 {
                    return Err(GeminiAdapterError::MultipleCandidatesUnsupported(
                        (cand.index.unwrap_or(0) + 1) as usize,
                    ));
                }

                // Safety ratings check
                if let Some(ref ratings) = cand.safety_ratings {
                    for r in ratings {
                        if r.blocked == Some(true) {
                            let cat = r.category.clone().unwrap_or_else(|| "Unknown".to_string());
                            return Err(GeminiAdapterError::SafetyBlocked(cat));
                        }
                    }
                }

                // Process candidate parts
                if let Some(ref content) = cand.content {
                    for part in &content.parts {
                        self.model_parts.push(part.clone());

                        // Thought / Reasoning Part
                        if part.thought == Some(true) {
                            // Close active text if any
                            if let Some(active) = self.active_text.take() {
                                events.push(ResponseEvent::OutputItemDone(ResponseItem::Message {
                                    id: Some(active.item_id),
                                    role: "assistant".to_string(),
                                    content: vec![ContentItem::OutputText {
                                        text: active.accumulated_text,
                                    }],
                                    phase: None,
                                    internal_chat_message_metadata_passthrough: None,
                                }));
                            }

                            let thought_text = part.text.as_deref().unwrap_or("");
                            if self.active_reasoning.is_none() {
                                let resp_id =
                                    self.response_id.as_deref().unwrap_or("gemini-stream");
                                let reasoning_id =
                                    format!("gemini-reasoning-{resp_id}-{}", self.output_index);
                                let item_id = ResponseItemId::from_server(reasoning_id.clone());
                                events.push(ResponseEvent::OutputItemAdded(
                                    ResponseItem::Reasoning {
                                        id: Some(item_id.clone()),
                                        summary: Vec::new(),
                                        content: Some(vec![ReasoningItemContent::ReasoningText {
                                            text: String::new(),
                                        }]),
                                        encrypted_content: None,
                                        internal_chat_message_metadata_passthrough: None,
                                    },
                                ));
                                self.active_reasoning = Some(ActiveReasoning {
                                    item_id,
                                    reasoning_id,
                                    accumulated_text: String::new(),
                                    thought_signature: part.thought_signature.clone(),
                                });
                                self.output_index += 1;
                            }

                            if let Some(active) = self.active_reasoning.as_mut() {
                                active.accumulated_text.push_str(thought_text);
                                if part.thought_signature.is_some() {
                                    active.thought_signature = part.thought_signature.clone();
                                }
                            }

                            if !thought_text.is_empty() {
                                events.push(ResponseEvent::ReasoningContentDelta {
                                    delta: thought_text.to_string(),
                                    content_index: 0,
                                });
                            }
                        }
                        // Function Call Part
                        else if let Some(ref fc) = part.function_call {
                            // Close active reasoning if open
                            if let Some(active) = self.active_reasoning.take() {
                                if let Some(sig) = &active.thought_signature {
                                    self.continuation_state
                                        .record_signature(&active.reasoning_id, sig);
                                }
                                events.push(ResponseEvent::OutputItemDone(
                                    ResponseItem::Reasoning {
                                        id: Some(active.item_id),
                                        summary: Vec::new(),
                                        content: Some(vec![ReasoningItemContent::ReasoningText {
                                            text: active.accumulated_text,
                                        }]),
                                        encrypted_content: None,
                                        internal_chat_message_metadata_passthrough: None,
                                    },
                                ));
                            }

                            // Close active text if open
                            if let Some(active) = self.active_text.take() {
                                events.push(ResponseEvent::OutputItemDone(ResponseItem::Message {
                                    id: Some(active.item_id),
                                    role: "assistant".to_string(),
                                    content: vec![ContentItem::OutputText {
                                        text: active.accumulated_text,
                                    }],
                                    phase: None,
                                    internal_chat_message_metadata_passthrough: None,
                                }));
                            }

                            // Validate function call
                            if fc.name.trim().is_empty() {
                                return Err(GeminiAdapterError::MalformedFunctionCall(
                                    "Empty function name in candidate".to_string(),
                                ));
                            }
                            if !fc.args.is_object() {
                                return Err(GeminiAdapterError::MalformedFunctionCall(format!(
                                    "Function call args must be an object, got {}",
                                    fc.args
                                )));
                            }

                            let resp_id = self.response_id.as_deref().unwrap_or("gemini-stream");
                            let call_id = fc.id.clone().unwrap_or_else(|| {
                                GeminiContinuationState::generate_internal_call_id(
                                    resp_id,
                                    0,
                                    self.tool_calls_emitted,
                                )
                            });

                            self.continuation_state.record_tool_call(
                                &call_id,
                                &fc.name,
                                fc.id.clone(),
                            );
                            if let Some(sig) = &part.thought_signature {
                                self.continuation_state.record_signature(&call_id, sig);
                            }

                            let args_str = serde_json::to_string(&fc.args).map_err(|e| {
                                GeminiAdapterError::MalformedFunctionCall(format!(
                                    "Failed to serialize function arguments: {e}"
                                ))
                            })?;

                            let item_id = ResponseItemId::from_server(format!(
                                "gemini-call-item-{resp_id}-{}",
                                self.output_index
                            ));

                            events.push(ResponseEvent::OutputItemAdded(
                                ResponseItem::FunctionCall {
                                    id: Some(item_id.clone()),
                                    name: fc.name.clone(),
                                    namespace: None,
                                    arguments: String::new(),
                                    encrypted_function_args: None,
                                    call_id: call_id.clone(),
                                    internal_chat_message_metadata_passthrough: None,
                                },
                            ));
                            events.push(ResponseEvent::ToolCallInputDelta {
                                item_id: item_id.to_string(),
                                call_id: Some(call_id.clone()),
                                delta: args_str.clone(),
                            });
                            events.push(ResponseEvent::OutputItemDone(
                                ResponseItem::FunctionCall {
                                    id: Some(item_id),
                                    name: fc.name.clone(),
                                    namespace: None,
                                    arguments: args_str,
                                    encrypted_function_args: None,
                                    call_id,
                                    internal_chat_message_metadata_passthrough: None,
                                },
                            ));

                            self.tool_calls_emitted += 1;
                            self.output_index += 1;
                        }
                        // Regular Output Text Part
                        else if let Some(ref text) = part.text {
                            // Close active reasoning if open
                            if let Some(active) = self.active_reasoning.take() {
                                if let Some(sig) = &active.thought_signature {
                                    self.continuation_state
                                        .record_signature(&active.reasoning_id, sig);
                                }
                                events.push(ResponseEvent::OutputItemDone(
                                    ResponseItem::Reasoning {
                                        id: Some(active.item_id),
                                        summary: Vec::new(),
                                        content: Some(vec![ReasoningItemContent::ReasoningText {
                                            text: active.accumulated_text,
                                        }]),
                                        encrypted_content: None,
                                        internal_chat_message_metadata_passthrough: None,
                                    },
                                ));
                            }

                            if self.active_text.is_none() {
                                let resp_id =
                                    self.response_id.as_deref().unwrap_or("gemini-stream");
                                let item_id = ResponseItemId::from_server(format!(
                                    "gemini-msg-{resp_id}-{}",
                                    self.output_index
                                ));
                                events.push(ResponseEvent::OutputItemAdded(
                                    ResponseItem::Message {
                                        id: Some(item_id.clone()),
                                        role: "assistant".to_string(),
                                        content: vec![ContentItem::OutputText {
                                            text: String::new(),
                                        }],
                                        phase: None,
                                        internal_chat_message_metadata_passthrough: None,
                                    },
                                ));
                                self.active_text = Some(ActiveText {
                                    item_id,
                                    accumulated_text: String::new(),
                                    thought_signature: part.thought_signature.clone(),
                                });
                                self.output_index += 1;
                            }

                            if let Some(active) = self.active_text.as_mut() {
                                active.accumulated_text.push_str(text);
                                if part.thought_signature.is_some() {
                                    active.thought_signature = part.thought_signature.clone();
                                }
                            }

                            events.push(ResponseEvent::OutputTextDelta(text.clone()));
                        }
                    }
                }

                // Check finish reason
                if let Some(ref fr) = cand.finish_reason {
                    match fr.as_str() {
                        "STOP" => {
                            self.finish_reason = Some("STOP".to_string());
                        }
                        "MAX_TOKENS" => {
                            return Err(GeminiAdapterError::MaxTokensExceeded);
                        }
                        "SAFETY" => {
                            return Err(GeminiAdapterError::SafetyBlocked(
                                "Generation stopped by safety filters".to_string(),
                            ));
                        }
                        "RECITATION" => {
                            return Err(GeminiAdapterError::RecitationBlocked(
                                "Generation stopped by recitation check".to_string(),
                            ));
                        }
                        "LANGUAGE" => {
                            return Err(GeminiAdapterError::UnsupportedLanguage(
                                "Generation stopped by language policy".to_string(),
                            ));
                        }
                        "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" => {
                            return Err(GeminiAdapterError::ContentBlocked(fr.clone()));
                        }
                        other => {
                            return Err(GeminiAdapterError::UnknownFinishReason(other.to_string()));
                        }
                    }
                }
            }
        }

        Ok(events)
    }

    /// Explicitly completes the stream when EOF is reached.
    pub fn finish_stream(&mut self) -> Result<Vec<ResponseEvent>, GeminiAdapterError> {
        if self.completed {
            return Err(GeminiAdapterError::AlreadyCompleted);
        }
        if !self.started {
            return Err(GeminiAdapterError::StreamMessageNotStarted);
        }

        let finish_reason = self
            .finish_reason
            .as_deref()
            .ok_or(GeminiAdapterError::StreamMissingFinishReason)?;

        let mut events = Vec::new();

        // 1. Close active reasoning if open
        if let Some(active) = self.active_reasoning.take() {
            if let Some(sig) = &active.thought_signature {
                self.continuation_state
                    .record_signature(&active.reasoning_id, sig);
            }
            events.push(ResponseEvent::OutputItemDone(ResponseItem::Reasoning {
                id: Some(active.item_id),
                summary: Vec::new(),
                content: Some(vec![ReasoningItemContent::ReasoningText {
                    text: active.accumulated_text,
                }]),
                encrypted_content: None,
                internal_chat_message_metadata_passthrough: None,
            }));
        }

        // 2. Close active text if open
        if let Some(active) = self.active_text.take() {
            events.push(ResponseEvent::OutputItemDone(ResponseItem::Message {
                id: Some(active.item_id),
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: active.accumulated_text,
                }],
                phase: None,
                internal_chat_message_metadata_passthrough: None,
            }));
        }

        // 3. Compute TokenUsage
        let (prompt_tokens, cached_tokens, output_tokens, total_tokens) = if let Some(ref u) =
            self.usage_metadata
        {
            let prompt = u.prompt_token_count.unwrap_or(0);
            let cached = u.cached_content_token_count.unwrap_or(0);
            let candidates = u.candidates_token_count.unwrap_or(0);
            let total = u.total_token_count.unwrap_or(prompt + candidates);

            if prompt < 0 || cached < 0 || candidates < 0 || total < 0 {
                return Err(GeminiAdapterError::InvalidUsage(format!(
                    "Negative token count in usage metadata: prompt={prompt}, cached={cached}, candidates={candidates}, total={total}"
                )));
            }
            if cached > prompt {
                return Err(GeminiAdapterError::InvalidUsage(format!(
                    "cached_content_token_count ({cached}) cannot exceed prompt_token_count ({prompt})"
                )));
            }
            (prompt, cached, candidates, total)
        } else {
            (0, 0, 0, 0)
        };

        let token_usage = TokenUsage {
            input_tokens: prompt_tokens,
            cached_input_tokens: cached_tokens,
            cache_write_input_tokens: 0,
            output_tokens,
            reasoning_output_tokens: 0,
            total_tokens,
            codex_rollout_budget_units: None,
        };

        // 4. Determine end_turn: STOP with tools -> end_turn: false; STOP without tools -> end_turn: true
        let end_turn = match finish_reason {
            "STOP" => {
                if self.tool_calls_emitted > 0 {
                    Some(false)
                } else {
                    Some(true)
                }
            }
            _ => None,
        };

        // 5. Store native model turn in continuation state
        let resp_id = self.response_id.clone().unwrap();
        self.continuation_state
            .insert_model_turn(&resp_id, GeminiContent::model(self.model_parts.clone()));

        // 6. Emit Completed event
        events.push(ResponseEvent::Completed {
            response_id: resp_id,
            token_usage: Some(token_usage),
            usage_metadata: None,
            end_turn,
        });

        self.completed = true;
        Ok(events)
    }
}
