use codex_api::ResponsesApiRequest;
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::models::{
    ContentItem, FunctionCallOutputBody, FunctionCallOutputContentItem, ImageReference,
    ResponseItem,
};
use codex_protocol::openai_models::ReasoningEffort;

use super::continuation::GeminiContinuationState;
use super::error::{GeminiAdapterError, GeminiAdapterWarning};
use super::types::{
    GeminiAdapterOptions, GeminiContent, GeminiFunctionCallingConfig, GeminiFunctionCallingMode,
    GeminiFunctionDeclaration, GeminiGenerateContentRequest, GeminiGenerationConfig, GeminiPart,
    GeminiRequestTranslation, GeminiResponseFormat, GeminiThinkingConfig, GeminiThinkingLevel,
    GeminiThinkingPolicy, GeminiTool, GeminiToolConfig,
};

/// Translates a Codex `ResponsesApiRequest` into a Gemini `generateContent` wire request.
pub fn translate_request(
    request: &ResponsesApiRequest,
    options: &GeminiAdapterOptions,
    continuation: Option<&GeminiContinuationState>,
) -> Result<GeminiRequestTranslation, GeminiAdapterError> {
    let mut warnings = Vec::new();

    // 1. Max output tokens validation
    if let Some(0) = options.max_output_tokens {
        return Err(GeminiAdapterError::InvalidMaxOutputTokens(0));
    }

    // 2. Security rejection of access_programs
    if request.access_programs.is_some() {
        return Err(GeminiAdapterError::UnsupportedSecurityFeature(
            "access_programs".to_string(),
        ));
    }

    // 3. Responses-only request fields fidelity warnings
    if let Some(ref key) = request.prompt_cache_key {
        warnings.push(GeminiAdapterWarning::PromptCacheKeyNotSupported(
            key.clone(),
        ));
    }
    if request.client_metadata.is_some() {
        warnings.push(GeminiAdapterWarning::ClientMetadataNotSupported);
    }
    if request.store {
        warnings.push(GeminiAdapterWarning::StoreFieldNotSupported);
    }
    if let Some(ref tier) = request.service_tier {
        warnings.push(GeminiAdapterWarning::ServiceTierNotSupported(tier.clone()));
    }
    if !request.include.is_empty() {
        warnings.push(GeminiAdapterWarning::IncludeFieldNotSupported(
            request.include.join(", "),
        ));
    }
    if let Some(ref text_controls) = request.text
        && let Some(ref verbosity) = text_controls.verbosity
    {
        warnings.push(GeminiAdapterWarning::VerbosityNotSupported(format!(
            "{verbosity:?}"
        )));
    }

    // 4. System instructions extraction
    let mut system_parts = Vec::new();
    if !request.instructions.is_empty() {
        system_parts.push(GeminiPart::text(&request.instructions));
    }

    // 5. Partition leading system/developer messages vs conversational history
    let mut conversational_started = false;
    let mut input_items = Vec::new();

    for item in &request.input {
        match item {
            ResponseItem::Message { role, content, .. }
                if role == "system" || role == "developer" =>
            {
                if conversational_started {
                    return Err(GeminiAdapterError::UnsupportedSystemHistoryPlacement(
                        format!(
                            "A '{role}' message appeared after conversational history had already begun"
                        ),
                    ));
                }
                for c in content {
                    match c {
                        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                            system_parts.push(GeminiPart::text(text));
                        }
                        ContentItem::InputImage { .. } => {
                            return Err(GeminiAdapterError::UnsupportedContent(
                                "Images are not supported in system instructions".to_string(),
                            ));
                        }
                        ContentItem::InputAudio { .. } => {
                            return Err(GeminiAdapterError::UnsupportedContent(
                                "Audio is not supported in system instructions".to_string(),
                            ));
                        }
                    }
                }
            }
            other => {
                conversational_started = true;
                input_items.push(other);
            }
        }
    }

    let system_instruction = if !system_parts.is_empty() {
        Some(GeminiContent::system(system_parts))
    } else {
        None
    };

    // 6. Conversational turns translation
    let mut contents: Vec<GeminiContent> = Vec::new();
    let mut i = 0;

    while i < input_items.len() {
        let item = input_items[i];
        match item {
            // Group adjacent function call outputs into a single user content turn
            ResponseItem::FunctionCallOutput { .. } | ResponseItem::CustomToolCallOutput { .. } => {
                let mut tool_parts = Vec::new();
                while i < input_items.len() {
                    match input_items[i] {
                        ResponseItem::FunctionCallOutput {
                            call_id,
                            name,
                            output,
                            ..
                        } => {
                            let part = convert_tool_output(
                                call_id.as_deref().unwrap_or(""),
                                name.as_deref(),
                                output,
                                &request.input,
                                continuation,
                            )?;
                            tool_parts.push(part);
                            i += 1;
                        }
                        ResponseItem::CustomToolCallOutput {
                            call_id,
                            name,
                            output,
                            ..
                        } => {
                            let part = convert_tool_output(
                                call_id,
                                name.as_deref(),
                                output,
                                &request.input,
                                continuation,
                            )?;
                            tool_parts.push(part);
                            i += 1;
                        }
                        _ => break,
                    }
                }
                append_or_merge_content(&mut contents, "user", tool_parts);
            }
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            } => {
                let parsed_args: serde_json::Value =
                    serde_json::from_str(arguments).map_err(|e| {
                        GeminiAdapterError::InvalidToolArguments(format!(
                            "Function call '{name}' arguments is not valid JSON: {e}"
                        ))
                    })?;
                if !parsed_args.is_object() {
                    return Err(GeminiAdapterError::InvalidToolArguments(format!(
                        "Function call '{name}' arguments must be a JSON object, found {parsed_args}"
                    )));
                }

                // Check provider call ID and thought signature from continuation state
                let provider_id = continuation
                    .and_then(|c| c.get_tool_call(call_id))
                    .and_then(|m| m.provider_call_id.clone())
                    .or_else(|| {
                        if !call_id.starts_with("gemini-call-") {
                            Some(call_id.clone())
                        } else {
                            None
                        }
                    });

                let mut part = GeminiPart::function_call(provider_id, name, parsed_args);
                if let Some(sig) = continuation.and_then(|c| c.get_signature(call_id)) {
                    part.thought_signature = Some(sig.to_string());
                }

                append_or_merge_content(&mut contents, "model", vec![part]);
                i += 1;
            }
            ResponseItem::CustomToolCall {
                name,
                input,
                call_id,
                ..
            } => {
                let parsed_input: serde_json::Value = serde_json::from_str(input).map_err(|e| {
                    GeminiAdapterError::UnsupportedCustomToolInput(format!(
                        "Custom tool call '{name}' input is not valid JSON: {e}"
                    ))
                })?;
                if !parsed_input.is_object() {
                    return Err(GeminiAdapterError::UnsupportedCustomToolInput(format!(
                        "Custom tool call '{name}' input must be a JSON object, found {parsed_input}"
                    )));
                }

                let provider_id = continuation
                    .and_then(|c| c.get_tool_call(call_id))
                    .and_then(|m| m.provider_call_id.clone())
                    .or_else(|| {
                        if !call_id.starts_with("gemini-call-") {
                            Some(call_id.clone())
                        } else {
                            None
                        }
                    });

                let mut part = GeminiPart::function_call(provider_id, name, parsed_input);
                if let Some(sig) = continuation.and_then(|c| c.get_signature(call_id)) {
                    part.thought_signature = Some(sig.to_string());
                }

                append_or_merge_content(&mut contents, "model", vec![part]);
                i += 1;
            }
            ResponseItem::Reasoning {
                id,
                summary,
                content,
                ..
            } => {
                let id_str = id.as_ref().map(|rid| rid.to_string()).unwrap_or_default();
                // Check if reasoning originated from Gemini
                if let Some(cont) = continuation
                    && let Some(sig) = cont.get_signature(&id_str)
                {
                    let text =
                        summary
                            .first()
                            .map(|s| {
                                match s {
                            codex_protocol::models::ReasoningItemReasoningSummary::SummaryText {
                                text,
                            } => text.clone(),
                        }
                            })
                            .or_else(|| {
                                content.as_ref().and_then(|c| c.first()).map(|item| {
                                    match item {
                                    codex_protocol::models::ReasoningItemContent::Text {
                                        text,
                                    } => text.clone(),
                                    codex_protocol::models::ReasoningItemContent::ReasoningText {
                                        text,
                                    } => text.clone(),
                                }
                                })
                            })
                            .unwrap_or_default();
                    let mut part = GeminiPart::text(text);
                    part.thought = Some(true);
                    part.thought_signature = Some(sig.to_string());
                    append_or_merge_content(&mut contents, "model", vec![part]);
                    i += 1;
                    continue;
                }

                // If non-Gemini reasoning or missing continuation
                if id_str.starts_with("gemini-") {
                    return Err(GeminiAdapterError::MissingContinuationState(format!(
                        "Reasoning item '{id_str}' missing required Gemini continuation state"
                    )));
                }

                warnings.push(GeminiAdapterWarning::CrossProviderReasoningOmitted(id_str));
                i += 1;
            }
            ResponseItem::Message {
                role, content, id, ..
            } => {
                let gemini_role = match role.as_str() {
                    "user" => "user",
                    "assistant" => "model",
                    other => return Err(GeminiAdapterError::UnsupportedRole(other.to_string())),
                };

                let mut parts = Vec::new();
                for c in content {
                    match c {
                        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                            let mut part = GeminiPart::text(text);
                            if gemini_role == "model"
                                && let Some(item_id) = id
                                && let Some(sig) = continuation
                                    .and_then(|cont| cont.get_signature(item_id.as_ref()))
                            {
                                part.thought_signature = Some(sig.to_string());
                            }
                            parts.push(part);
                        }
                        ContentItem::InputImage { image, .. } => {
                            let part = convert_input_image(image)?;
                            parts.push(part);
                        }
                        ContentItem::InputAudio { audio_url } => {
                            let part = convert_input_audio(audio_url)?;
                            parts.push(part);
                        }
                    }
                }

                append_or_merge_content(&mut contents, gemini_role, parts);
                i += 1;
            }
            ResponseItem::AgentMessage { .. } => {
                return Err(GeminiAdapterError::UnsupportedContent(
                    "AgentMessage items are not supported in Gemini generateContent".to_string(),
                ));
            }
            ResponseItem::LocalShellCall { .. } => {
                return Err(GeminiAdapterError::UnsupportedContent(
                    "LocalShellCall items must be executed by Codex and converted to function outputs".to_string(),
                ));
            }
            ResponseItem::ToolSearchCall { .. } | ResponseItem::ToolSearchOutput { .. } => {
                return Err(GeminiAdapterError::UnsupportedContent(
                    "ToolSearch items are not supported in Gemini generateContent".to_string(),
                ));
            }
            ResponseItem::AdditionalTools { .. } => {
                return Err(GeminiAdapterError::UnsupportedContent(
                    "AdditionalTools items are not supported in conversational history".to_string(),
                ));
            }
            other => {
                return Err(GeminiAdapterError::UnsupportedContent(format!(
                    "Response item '{other:?}' is not supported in Gemini generateContent"
                )));
            }
        }
    }

    // 7. Tool definitions and strict mode analysis
    let mut has_strict = false;
    let mut has_non_strict = false;
    let mut function_declarations = Vec::new();

    if let Some(ref tools_raw) = request.tools {
        let tools_json: serde_json::Value = serde_json::to_value(tools_raw).map_err(|e| {
            GeminiAdapterError::UnsupportedToolType(format!(
                "Malformed tools JSON schema array: {e}"
            ))
        })?;

        if let Some(tool_array) = tools_json.as_array() {
            for tool in tool_array {
                let tool_type = tool.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if tool_type != "function" {
                    return Err(GeminiAdapterError::UnsupportedToolType(format!(
                        "Unsupported tool type '{tool_type}'; only 'function' tools are supported"
                    )));
                }

                let name = tool
                    .get("name")
                    .or_else(|| tool.get("function").and_then(|f| f.get("name")))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        GeminiAdapterError::UnsupportedToolType(
                            "Tool definition missing function name".to_string(),
                        )
                    })?;

                if options.validate_tool_names && !is_valid_gemini_tool_name(name) {
                    return Err(GeminiAdapterError::UnsupportedToolType(format!(
                        "Tool name '{name}' violates Gemini naming contract (must be 1-128 chars, alphanumeric, underscore, dot, hyphen)"
                    )));
                }

                // Check strict property (flat or nested)
                let strict = tool
                    .get("strict")
                    .or_else(|| tool.get("function").and_then(|f| f.get("strict")))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if strict {
                    has_strict = true;
                } else {
                    has_non_strict = true;
                }

                let description = tool
                    .get("description")
                    .or_else(|| tool.get("function").and_then(|f| f.get("description")))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let parameters = tool
                    .get("parameters")
                    .or_else(|| tool.get("function").and_then(|f| f.get("parameters")))
                    .cloned();

                function_declarations.push(GeminiFunctionDeclaration {
                    name: name.to_string(),
                    description,
                    parameters,
                });
            }
        }
    }

    // 8. Tool choice and function calling config
    let has_tools = !function_declarations.is_empty();
    let (tools, tool_config) = if has_tools {
        let (mode, allowed_names) = match request.tool_choice.as_str() {
            "" => {
                if has_strict {
                    (Some(GeminiFunctionCallingMode::Validated), None)
                } else {
                    (None, None)
                }
            }
            "auto" => {
                if has_strict {
                    (Some(GeminiFunctionCallingMode::Validated), None)
                } else {
                    (Some(GeminiFunctionCallingMode::Auto), None)
                }
            }
            "none" => (Some(GeminiFunctionCallingMode::None), None),
            "required" => (Some(GeminiFunctionCallingMode::Any), None),
            other => {
                return Err(GeminiAdapterError::UnsupportedToolChoice(other.to_string()));
            }
        };

        if has_strict && has_non_strict {
            warnings.push(GeminiAdapterWarning::StrictToolScopePromoted);
        }

        // Parallel tool call validation
        if !request.parallel_tool_calls
            && request.tool_choice != "none"
            && mode != Some(GeminiFunctionCallingMode::None)
        {
            return Err(GeminiAdapterError::SequentialToolCallingNotEnforceable);
        }

        let tools = Some(vec![GeminiTool {
            function_declarations: Some(function_declarations),
        }]);

        let tool_config = mode.map(|m| GeminiToolConfig {
            function_calling_config: Some(GeminiFunctionCallingConfig {
                mode: Some(m),
                allowed_function_names: allowed_names,
            }),
        });

        (tools, tool_config)
    } else {
        if !request.tool_choice.is_empty()
            && request.tool_choice != "none"
            && request.tool_choice != "auto"
        {
            return Err(GeminiAdapterError::UnsupportedToolChoice(
                request.tool_choice.clone(),
            ));
        }
        (None, None)
    };

    // 9. Generation config: structured outputs, thinking, and max tokens
    let mut gen_config = GeminiGenerationConfig::default();

    if let Some(max_tokens) = options.max_output_tokens {
        gen_config.max_output_tokens = Some(max_tokens);
    }

    // Structured outputs
    if let Some(ref text_controls) = request.text
        && let Some(ref format) = text_controls.format
    {
        if !format.name.is_empty() {
            warnings.push(GeminiAdapterWarning::StructuredOutputNameIgnored(
                format.name.clone(),
            ));
        }
        if !format.strict {
            warnings.push(GeminiAdapterWarning::StructuredOutputSchemaConstrained);
        }
        gen_config.response_mime_type = Some("application/json".to_string());
        gen_config.response_schema = Some(format.schema.clone());
        gen_config.response_format = Some(GeminiResponseFormat {
            mime_type: Some("application/json".to_string()),
            schema: Some(format.schema.clone()),
        });
    }

    // Thinking configuration
    let mut thinking_config = GeminiThinkingConfig::default();
    let mut has_thinking_config = false;

    match options.thinking_policy {
        GeminiThinkingPolicy::ProviderDefault => {}
        GeminiThinkingPolicy::ExactReasoningEffort => {
            if let Some(ref reasoning) = request.reasoning
                && let Some(ref effort) = reasoning.effort
            {
                let level = match effort {
                    ReasoningEffort::Minimal => GeminiThinkingLevel::Minimal,
                    ReasoningEffort::Low => GeminiThinkingLevel::Low,
                    ReasoningEffort::Medium => GeminiThinkingLevel::Medium,
                    ReasoningEffort::High => GeminiThinkingLevel::High,
                    other => {
                        return Err(GeminiAdapterError::UnsupportedReasoningEffort(
                            other.to_string(),
                        ));
                    }
                };
                thinking_config.thinking_level = Some(level);
                has_thinking_config = true;
            }
        }
        GeminiThinkingPolicy::LegacyBudget(budget) => {
            thinking_config.thinking_budget = Some(budget);
            has_thinking_config = true;
        }
    }

    // Reasoning summary -> include_thoughts
    if let Some(ref reasoning) = request.reasoning {
        if let Some(ref summary) = reasoning.summary {
            match summary {
                ReasoningSummary::None => {
                    thinking_config.include_thoughts = Some(false);
                    has_thinking_config = true;
                }
                ReasoningSummary::Auto => {
                    thinking_config.include_thoughts = Some(true);
                    has_thinking_config = true;
                }
                ReasoningSummary::Concise => {
                    thinking_config.include_thoughts = Some(true);
                    has_thinking_config = true;
                    warnings.push(
                        GeminiAdapterWarning::ReasoningSummaryGranularityNotRepresentable(
                            "concise".to_string(),
                        ),
                    );
                }
                ReasoningSummary::Detailed => {
                    thinking_config.include_thoughts = Some(true);
                    has_thinking_config = true;
                    warnings.push(
                        GeminiAdapterWarning::ReasoningSummaryGranularityNotRepresentable(
                            "detailed".to_string(),
                        ),
                    );
                }
            }
        }

        if reasoning.context.is_some() {
            warnings.push(
                GeminiAdapterWarning::ReasoningContextNotDirectlyRepresentable(
                    "ReasoningContext is governed by thought signatures in Gemini".to_string(),
                ),
            );
        }
    }

    if has_thinking_config {
        gen_config.thinking_config = Some(thinking_config);
    }

    let generation_config = if gen_config != GeminiGenerationConfig::default() {
        Some(gen_config)
    } else {
        None
    };

    Ok(GeminiRequestTranslation {
        model: request.model.clone(),
        request: GeminiGenerateContentRequest {
            contents,
            tools,
            tool_config,
            safety_settings: None,
            system_instruction,
            generation_config,
        },
        warnings,
    })
}

/// Validates tool name against Gemini naming rules: 1-128 chars, [a-zA-Z0-9_.-]
pub fn is_valid_gemini_tool_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
}

/// Converts a tool execution output into a Gemini `functionResponse` part.
fn convert_tool_output(
    call_id: &str,
    declared_name: Option<&str>,
    payload: &codex_protocol::models::FunctionCallOutputPayload,
    history: &[ResponseItem],
    continuation: Option<&GeminiContinuationState>,
) -> Result<GeminiPart, GeminiAdapterError> {
    // 1. Resolve function name
    let name = if let Some(n) = declared_name {
        n.to_string()
    } else if let Some(cont_name) = continuation
        .and_then(|c| c.get_tool_call(call_id))
        .map(|m| m.name.clone())
    {
        cont_name
    } else if let Some(hist_name) = find_tool_name_in_history(call_id, history) {
        hist_name
    } else {
        return Err(GeminiAdapterError::MissingToolCallAssociation(
            call_id.to_string(),
        ));
    };

    // 2. Resolve provider call ID: only send if original provider had an ID
    let provider_id = continuation
        .and_then(|c| c.get_tool_call(call_id))
        .and_then(|m| m.provider_call_id.clone())
        .or_else(|| {
            if !call_id.is_empty() && !call_id.starts_with("gemini-call-") {
                Some(call_id.to_string())
            } else {
                None
            }
        });

    // 3. Process output body
    let is_error = payload.success.map(|s| !s).unwrap_or(false);

    match &payload.body {
        FunctionCallOutputBody::Text(text) => {
            let response = if is_error {
                serde_json::json!({ "error": text })
            } else {
                serde_json::json!({ "output": text })
            };
            Ok(GeminiPart::function_response(
                provider_id,
                name,
                response,
                None,
            ))
        }
        FunctionCallOutputBody::ContentItems(items) => {
            let mut text_parts = Vec::new();
            let mut media_parts = Vec::new();

            for item in items {
                match item {
                    FunctionCallOutputContentItem::InputText { text } => {
                        text_parts.push(text.as_str());
                    }
                    FunctionCallOutputContentItem::InputImage { image, .. } => match image {
                        ImageReference::Inline { image_url } => {
                            let (mime, data) = parse_data_url(image_url)?;
                            validate_image_mime(&mime)?;
                            media_parts.push(GeminiPart::inline_data(mime, data));
                        }
                        ImageReference::File { .. } => {
                            return Err(GeminiAdapterError::UnsupportedContent(
                                "File image references are not supported in tool output"
                                    .to_string(),
                            ));
                        }
                    },
                    FunctionCallOutputContentItem::InputAudio { .. } => {
                        return Err(GeminiAdapterError::UnsupportedToolOutputContent(
                            "Audio is not supported in tool output".to_string(),
                        ));
                    }
                    FunctionCallOutputContentItem::EncryptedContent { .. } => {
                        return Err(GeminiAdapterError::UnsupportedContent(
                            "Encrypted content is not supported in tool output".to_string(),
                        ));
                    }
                }
            }

            let combined_text = text_parts.join("\n");
            let response = if is_error {
                serde_json::json!({ "error": combined_text })
            } else {
                serde_json::json!({ "output": combined_text })
            };

            let parts = if !media_parts.is_empty() {
                Some(media_parts)
            } else {
                None
            };

            Ok(GeminiPart::function_response(
                provider_id,
                name,
                response,
                parts,
            ))
        }
    }
}

/// Helper to search preceding FunctionCall in history by call_id.
fn find_tool_name_in_history(call_id: &str, history: &[ResponseItem]) -> Option<String> {
    for item in history {
        match item {
            ResponseItem::FunctionCall {
                call_id: cid, name, ..
            } if cid == call_id => return Some(name.clone()),
            ResponseItem::CustomToolCall {
                call_id: cid, name, ..
            } if cid == call_id => return Some(name.clone()),
            _ => {}
        }
    }
    None
}

/// Converts an input image to GeminiPart.
fn convert_input_image(image: &ImageReference) -> Result<GeminiPart, GeminiAdapterError> {
    match image {
        ImageReference::Inline { image_url } => {
            let (mime, data) = parse_data_url(image_url)?;
            validate_image_mime(&mime)?;
            Ok(GeminiPart::inline_data(mime, data))
        }
        ImageReference::File { .. } => Err(GeminiAdapterError::UnsupportedContent(
            "File image references are not supported without I/O".to_string(),
        )),
    }
}

/// Converts an input audio to GeminiPart.
fn convert_input_audio(audio_url: &str) -> Result<GeminiPart, GeminiAdapterError> {
    if !audio_url.starts_with("data:") {
        return Err(GeminiAdapterError::UnsupportedContent(
            "External audio URLs are not supported without network access".to_string(),
        ));
    }
    let (mime, data) = parse_data_url(audio_url)?;
    validate_audio_mime(&mime)?;
    Ok(GeminiPart::inline_data(mime, data))
}

/// Validates supported Gemini image MIME types.
fn validate_image_mime(mime: &str) -> Result<(), GeminiAdapterError> {
    match mime {
        "image/jpeg" | "image/jpg" | "image/png" | "image/webp" | "image/heic" | "image/heif" => {
            Ok(())
        }
        other => Err(GeminiAdapterError::UnsupportedImageMime(other.to_string())),
    }
}

/// Validates supported Gemini audio MIME types.
fn validate_audio_mime(mime: &str) -> Result<(), GeminiAdapterError> {
    match mime {
        "audio/wav" | "audio/mp3" | "audio/mpeg" | "audio/aiff" | "audio/aac" | "audio/ogg"
        | "audio/flac" => Ok(()),
        other => Err(GeminiAdapterError::UnsupportedAudioMime(other.to_string())),
    }
}

/// Parses a `data:<mime>;base64,<data>` URL into `(mime, base64_data)`.
fn parse_data_url(data_url: &str) -> Result<(String, String), GeminiAdapterError> {
    if !data_url.starts_with("data:") {
        return Err(GeminiAdapterError::UnsupportedContent(
            "Expected data: URL for inline multimedia".to_string(),
        ));
    }

    let remainder = &data_url["data:".len()..];
    let (header, data) = remainder.split_once(',').ok_or_else(|| {
        GeminiAdapterError::UnsupportedContent("Malformed data: URL (missing comma)".to_string())
    })?;

    let parts: Vec<&str> = header.split(';').collect();
    let mime = parts.first().copied().unwrap_or("").to_lowercase();
    let is_base64 = parts.iter().any(|&p| p.trim() == "base64");

    if !is_base64 {
        return Err(GeminiAdapterError::UnsupportedContent(
            "Only base64-encoded inline multimedia is supported".to_string(),
        ));
    }

    Ok((mime, data.to_string()))
}

/// Appends parts to the last content turn if roles match, or creates a new Content turn.
fn append_or_merge_content(contents: &mut Vec<GeminiContent>, role: &str, parts: Vec<GeminiPart>) {
    if parts.is_empty() {
        return;
    }

    if let Some(last) = contents.last_mut()
        && last.role.as_deref() == Some(role)
    {
        last.parts.extend(parts);
        return;
    }

    contents.push(GeminiContent {
        role: Some(role.to_string()),
        parts,
    });
}
