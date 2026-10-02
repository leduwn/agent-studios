use codex_api::ResponsesApiRequest;
use codex_protocol::models::{
    ContentItem, FunctionCallOutputBody, ImageReference, ResponseItem,
    plaintext_agent_message_content,
};
use codex_protocol::openai_models::ReasoningEffort;

use super::continuation::{AnthropicContinuationState, NativeThinkingBlock};
use super::error::{AnthropicAdapterError, AnthropicAdapterWarning};
use super::types::{
    AnthropicCacheControl, AnthropicCacheTtl, AnthropicContentBlock, AnthropicImageSource,
    AnthropicMessage, AnthropicMessagesRequest, AnthropicOutputConfig, AnthropicOutputFormat,
    AnthropicSystemBlock, AnthropicThinkingConfig, AnthropicTool, AnthropicToolChoice,
    AnthropicToolResultBlock, AnthropicToolResultContent,
};

/// Caching strategy for Anthropic prompt caching breakpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnthropicPromptCachePolicy {
    #[default]
    None,
    LastUserMessage {
        ttl: Option<AnthropicCacheTtl>,
    },
    ToolsAndSystem {
        ttl: Option<AnthropicCacheTtl>,
    },
    AutomaticBreakpoint {
        ttl: Option<AnthropicCacheTtl>,
    },
}

/// Thinking strategy for Anthropic native reasoning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnthropicThinkingPolicy {
    #[default]
    ProviderDefault,
    Adaptive,
    LegacyBudgetTokens(u64),
}

/// Translation configuration options for Anthropic Messages requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicRequestOptions {
    /// Maximum output tokens to generate (mandatory in Anthropic API).
    pub max_tokens: u64,
    /// Caching breakpoint injection policy.
    pub prompt_cache_policy: AnthropicPromptCachePolicy,
    /// Thinking configuration policy.
    pub thinking_policy: AnthropicThinkingPolicy,
    /// Whether to map Codex reasoning_effort to Anthropic output_config.effort.
    pub effort_mapping: bool,
    /// Whether to validate tool names against Anthropic naming rules.
    pub validate_tool_names: bool,
}

impl AnthropicRequestOptions {
    /// Creates a new explicit options instance with mandatory max_tokens.
    pub fn new(max_tokens: u64) -> Self {
        Self {
            max_tokens,
            prompt_cache_policy: AnthropicPromptCachePolicy::None,
            thinking_policy: AnthropicThinkingPolicy::ProviderDefault,
            effort_mapping: true,
            validate_tool_names: true,
        }
    }
}

/// Result of translating a Codex request into an Anthropic Messages request.
#[derive(Debug, Clone, PartialEq)]
pub struct AnthropicRequestTranslation {
    pub request: AnthropicMessagesRequest,
    pub warnings: Vec<AnthropicAdapterWarning>,
}

/// Translates a Codex `ResponsesApiRequest` into an `AnthropicMessagesRequest`.
pub fn translate_request(
    codex_req: &ResponsesApiRequest,
    options: &AnthropicRequestOptions,
    continuation: Option<&AnthropicContinuationState>,
) -> Result<AnthropicRequestTranslation, AnthropicAdapterError> {
    if options.max_tokens == 0 {
        return Err(AnthropicAdapterError::InvalidMaxTokens(0));
    }

    if codex_req.access_programs.is_some() {
        return Err(AnthropicAdapterError::UnsupportedSecurityFeature(
            "access_programs is security-sensitive and rejected".to_string(),
        ));
    }

    let mut warnings = Vec::new();

    // Check dropped fields
    if codex_req.store {
        warnings.push(AnthropicAdapterWarning::DroppedStore(true));
    }
    if let Some(service_tier) = &codex_req.service_tier {
        warnings.push(AnthropicAdapterWarning::DroppedServiceTier(
            service_tier.clone(),
        ));
    }
    if !codex_req.include.is_empty() {
        warnings.push(AnthropicAdapterWarning::DroppedInclude(
            codex_req.include.clone(),
        ));
    }
    if let Some(metadata) = codex_req.client_metadata.as_ref().filter(|m| !m.is_empty()) {
        warnings.push(AnthropicAdapterWarning::DroppedClientMetadata(
            metadata.keys().cloned().collect(),
        ));
    }
    if let Some(stream_options) = &codex_req.stream_options {
        warnings.push(AnthropicAdapterWarning::DroppedStreamOptions(format!(
            "{stream_options:?}"
        )));
    }
    if let Some(cache_key) = &codex_req.prompt_cache_key {
        warnings.push(
            AnthropicAdapterWarning::PromptCacheKeyNotDirectlyRepresentable(cache_key.clone()),
        );
    }

    // 1. Process System Blocks (instructions + leading system/developer messages)
    let mut system_blocks = Vec::new();
    if !codex_req.instructions.is_empty() {
        system_blocks.push(AnthropicSystemBlock::Text {
            text: codex_req.instructions.clone(),
            cache_control: None,
        });
    }

    let mut seen_conversational_item = false;
    let mut conversational_items = Vec::new();

    for item in &codex_req.input {
        match item {
            ResponseItem::Message { role, content, .. }
                if role == "system" || role == "developer" =>
            {
                if seen_conversational_item {
                    return Err(AnthropicAdapterError::UnsupportedSystemHistoryPlacement(
                        format!(
                            "System/developer message with role '{role}' appeared after conversational turns"
                        ),
                    ));
                }
                for part in content {
                    match part {
                        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                            system_blocks.push(AnthropicSystemBlock::Text {
                                text: text.clone(),
                                cache_control: None,
                            });
                        }
                        other => {
                            return Err(AnthropicAdapterError::UnsupportedContent(format!(
                                "Non-text content in system/developer message: {other:?}"
                            )));
                        }
                    }
                }
            }
            other => {
                seen_conversational_item = true;
                conversational_items.push(other);
            }
        }
    }

    // 2. Translate Message History (coalescing consecutive items of same role)
    let mut messages: Vec<AnthropicMessage> = Vec::new();

    for item in conversational_items {
        match item {
            ResponseItem::Message { role, content, .. } => {
                let target_role = match role.as_str() {
                    "user" => "user",
                    "assistant" => "assistant",
                    other => return Err(AnthropicAdapterError::InvalidRole(other.to_string())),
                };

                let mut blocks = Vec::new();
                for part in content {
                    match part {
                        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                            blocks.push(AnthropicContentBlock::Text {
                                text: text.clone(),
                                cache_control: None,
                            });
                        }
                        ContentItem::InputImage { image, .. } => match image {
                            ImageReference::Inline { image_url } => {
                                let (media_type, data) = parse_data_url(image_url)?;
                                blocks.push(AnthropicContentBlock::Image {
                                    source: AnthropicImageSource {
                                        r#type: "base64".to_string(),
                                        media_type,
                                        data,
                                    },
                                    cache_control: None,
                                });
                            }
                            ImageReference::File { file_id } => {
                                return Err(AnthropicAdapterError::UnsupportedContent(format!(
                                    "File-based image references ({file_id}) cannot be translated to Anthropic wire format"
                                )));
                            }
                        },
                        ContentItem::InputAudio { .. } => {
                            return Err(AnthropicAdapterError::UnsupportedContent(
                                "Audio input is not supported in Anthropic Messages wire format"
                                    .to_string(),
                            ));
                        }
                    }
                }

                append_or_push_message(&mut messages, target_role, blocks);
            }

            ResponseItem::FunctionCall {
                call_id,
                name,
                arguments,
                ..
            } => {
                let input_json: serde_json::Value =
                    serde_json::from_str(arguments).map_err(|e| {
                        AnthropicAdapterError::InvalidToolArguments(format!(
                            "Invalid function call arguments JSON: {e}"
                        ))
                    })?;

                let block = AnthropicContentBlock::ToolUse {
                    id: call_id.clone(),
                    name: name.clone(),
                    input: input_json,
                    cache_control: None,
                };

                append_or_push_message(&mut messages, "assistant", vec![block]);
            }

            ResponseItem::CustomToolCall {
                call_id,
                name,
                input,
                ..
            } => {
                let input_json: serde_json::Value = serde_json::from_str(input).map_err(|e| {
                    AnthropicAdapterError::UnsupportedCustomToolInput(format!(
                        "Custom tool call '{name}' input is not valid JSON: {e}"
                    ))
                })?;
                if !input_json.is_object() {
                    return Err(AnthropicAdapterError::UnsupportedCustomToolInput(format!(
                        "Custom tool call '{name}' input must be a JSON object, found {input_json}"
                    )));
                }
                let block = AnthropicContentBlock::ToolUse {
                    id: call_id.clone(),
                    name: name.clone(),
                    input: input_json,
                    cache_control: None,
                };

                append_or_push_message(&mut messages, "assistant", vec![block]);
            }

            ResponseItem::FunctionCallOutput {
                call_id, output, ..
            } => {
                let id = call_id.as_ref().ok_or_else(|| {
                    AnthropicAdapterError::MissingToolCallId(
                        "FunctionCallOutput must have call_id to map to tool_result".to_string(),
                    )
                })?;

                let block = convert_tool_output_payload(id, output)?;
                append_or_push_message(&mut messages, "user", vec![block]);
            }

            ResponseItem::CustomToolCallOutput {
                call_id, output, ..
            } => {
                let block = convert_tool_output_payload(call_id, output)?;
                append_or_push_message(&mut messages, "user", vec![block]);
            }

            ResponseItem::Reasoning {
                id,
                encrypted_content,
                ..
            } => {
                if encrypted_content.is_some() {
                    return Err(AnthropicAdapterError::UnsupportedSecurityFeature(
                        "Encrypted reasoning content is not supported".to_string(),
                    ));
                }

                let item_id_str = id.as_ref().map(|id| id.to_string()).unwrap_or_default();
                if let Some(cont) = continuation
                    && let Some(native_block) = cont.get_reasoning_block(&item_id_str)
                {
                    let block = match native_block {
                        NativeThinkingBlock::Thinking {
                            thinking,
                            signature,
                        } => AnthropicContentBlock::Thinking {
                            thinking: thinking.clone(),
                            signature: signature.clone(),
                        },
                        NativeThinkingBlock::RedactedThinking { data } => {
                            AnthropicContentBlock::RedactedThinking { data: data.clone() }
                        }
                    };
                    append_or_push_message(&mut messages, "assistant", vec![block]);
                } else if item_id_str.starts_with("anthropic-reasoning-") {
                    return Err(AnthropicAdapterError::MissingContinuationState(format!(
                        "Missing native continuation state for Anthropic-origin reasoning item: {item_id_str}"
                    )));
                } else {
                    warnings.push(AnthropicAdapterWarning::CrossProviderReasoningOmitted(
                        item_id_str,
                    ));
                }
            }

            ResponseItem::AgentMessage { content, .. } => {
                if let Some(text) = plaintext_agent_message_content(content) {
                    let block = AnthropicContentBlock::Text {
                        text,
                        cache_control: None,
                    };
                    append_or_push_message(&mut messages, "assistant", vec![block]);
                } else {
                    return Err(AnthropicAdapterError::UnsupportedContent(
                        "Encrypted agent message is not supported".to_string(),
                    ));
                }
            }

            ResponseItem::LocalShellCall { .. }
            | ResponseItem::ToolSearchCall { .. }
            | ResponseItem::ToolSearchOutput { .. }
            | ResponseItem::WebSearchCall { .. }
            | ResponseItem::ImageGenerationCall { .. }
            | ResponseItem::Compaction { .. }
            | ResponseItem::ConfigurationUpdate { .. }
            | ResponseItem::AdditionalTools { .. } => {
                return Err(AnthropicAdapterError::UnsupportedResponseItem(format!(
                    "{item:?} is not supported in Anthropic Messages wire format"
                )));
            }

            item => {
                return Err(AnthropicAdapterError::UnsupportedResponseItem(format!(
                    "Unsupported ResponseItem variant in conversation history: {item:?}"
                )));
            }
        }
    }

    // 3. Translate Tools
    let mut tools = if let Some(raw_tools) = &codex_req.tools {
        let parsed: serde_json::Value = serde_json::to_value(raw_tools).map_err(|e| {
            AnthropicAdapterError::UnsupportedToolType(format!("Failed to parse tools JSON: {e}"))
        })?;

        let array = parsed.as_array().ok_or_else(|| {
            AnthropicAdapterError::UnsupportedToolType(
                "tools payload must be a JSON array".to_string(),
            )
        })?;

        let mut anthropic_tools = Vec::with_capacity(array.len());
        for tool_val in array {
            let tool_type = tool_val.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if tool_type != "function" {
                return Err(AnthropicAdapterError::UnsupportedToolType(format!(
                    "Tool type '{tool_type}' cannot be translated to Anthropic Messages"
                )));
            }

            let (name, description, parameters, strict) =
                if let Some(function_obj) = tool_val.get("function") {
                    let name = function_obj
                        .get("name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            AnthropicAdapterError::UnsupportedToolType(
                                "Tool function missing name".to_string(),
                            )
                        })?;
                    let description = function_obj
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let parameters = function_obj.get("parameters").cloned().unwrap_or_else(
                        || serde_json::json!({ "type": "object", "properties": {} }),
                    );
                    let strict = function_obj
                        .get("strict")
                        .and_then(|v| v.as_bool())
                        .or_else(|| tool_val.get("strict").and_then(|v| v.as_bool()));
                    (name, description, parameters, strict)
                } else {
                    let name = tool_val
                        .get("name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            AnthropicAdapterError::UnsupportedToolType(
                                "Responses tool missing name".to_string(),
                            )
                        })?;
                    let description = tool_val
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let parameters = tool_val.get("parameters").cloned().unwrap_or_else(
                        || serde_json::json!({ "type": "object", "properties": {} }),
                    );
                    let strict = tool_val.get("strict").and_then(|v| v.as_bool());
                    (name, description, parameters, strict)
                };

            if name == "access_programs" {
                return Err(AnthropicAdapterError::UnsupportedSecurityFeature(
                    "access_programs tool is security-sensitive and rejected".to_string(),
                ));
            }

            if options.validate_tool_names && !is_valid_anthropic_tool_name(name) {
                return Err(AnthropicAdapterError::UnsupportedToolType(format!(
                    "Tool name '{name}' violates Anthropic naming rules (must match ^[a-zA-Z0-9_-]{{1,64}}$)"
                )));
            }

            anthropic_tools.push(AnthropicTool {
                name: name.to_string(),
                description,
                input_schema: parameters,
                strict,
                cache_control: None,
            });
        }
        Some(anthropic_tools)
    } else {
        None
    };

    // 4. Translate Tool Choice
    let tool_choice = match codex_req.tool_choice.as_str() {
        "" => None,
        "auto" => Some(AnthropicToolChoice::Auto {
            disable_parallel_tool_use: (!codex_req.parallel_tool_calls).then_some(true),
        }),
        "none" => Some(AnthropicToolChoice::None),
        "required" => Some(AnthropicToolChoice::Any {
            disable_parallel_tool_use: (!codex_req.parallel_tool_calls).then_some(true),
        }),
        other => {
            return Err(AnthropicAdapterError::UnsupportedToolChoice(
                other.to_string(),
            ));
        }
    };

    // 5. Thinking and Output Configuration
    let thinking = match options.thinking_policy {
        AnthropicThinkingPolicy::ProviderDefault => None,
        AnthropicThinkingPolicy::Adaptive => Some(AnthropicThinkingConfig::Adaptive),
        AnthropicThinkingPolicy::LegacyBudgetTokens(budget) => {
            if budget < 1024 {
                return Err(AnthropicAdapterError::InvalidThinkingBudget(budget));
            }
            warnings.push(AnthropicAdapterWarning::LegacyThinkingBudgetUsed(budget));
            Some(AnthropicThinkingConfig::Enabled {
                budget_tokens: budget,
            })
        }
    };

    let mut output_config = AnthropicOutputConfig::default();

    if options.effort_mapping
        && let Some(reasoning) = &codex_req.reasoning
        && let Some(effort) = &reasoning.effort
    {
        let mapped_effort = match effort {
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium => "medium",
            ReasoningEffort::High => "high",
            ReasoningEffort::XHigh => "xhigh",
            ReasoningEffort::Max => "max",
            unsupported => {
                return Err(AnthropicAdapterError::UnsupportedReasoningEffort(format!(
                    "{unsupported:?}"
                )));
            }
        };
        output_config.effort = Some(mapped_effort.to_string());
    }

    if let Some(text_controls) = &codex_req.text {
        if let Some(verbosity) = &text_controls.verbosity {
            warnings.push(AnthropicAdapterWarning::DroppedVerbosity(format!(
                "{verbosity:?}"
            )));
        }
        if let Some(format) = &text_controls.format {
            output_config.format = Some(AnthropicOutputFormat::JsonSchema {
                schema: format.schema.clone(),
            });
        }
    }

    // 6. Apply Prompt Caching Policy
    apply_cache_policy(
        options.prompt_cache_policy,
        &mut system_blocks,
        tools.as_deref_mut(),
        &mut messages,
    );

    let system = if system_blocks.is_empty() {
        None
    } else {
        Some(system_blocks)
    };

    let output_config_final = if output_config.effort.is_none() && output_config.format.is_none() {
        None
    } else {
        Some(output_config)
    };

    let request = AnthropicMessagesRequest {
        model: codex_req.model.clone(),
        messages,
        max_tokens: options.max_tokens,
        system,
        tools,
        tool_choice,
        thinking,
        output_config: output_config_final,
        stream: if codex_req.stream { Some(true) } else { None },
        metadata: None,
    };

    Ok(AnthropicRequestTranslation { request, warnings })
}

/// Shared helper to losslessly convert a tool output payload into an Anthropic ToolResult content block.
fn convert_tool_output_payload(
    call_id: &str,
    output: &codex_protocol::models::FunctionCallOutputPayload,
) -> Result<AnthropicContentBlock, AnthropicAdapterError> {
    let result_content = match &output.body {
        FunctionCallOutputBody::Text(text) => AnthropicToolResultContent::Text(text.clone()),
        FunctionCallOutputBody::ContentItems(items) => {
            let mut tool_blocks = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    codex_protocol::models::FunctionCallOutputContentItem::InputText { text } => {
                        tool_blocks.push(AnthropicToolResultBlock::Text { text: text.clone() });
                    }
                    codex_protocol::models::FunctionCallOutputContentItem::InputImage {
                        image,
                        ..
                    } => match image {
                        ImageReference::Inline { image_url } => {
                            let (media_type, data) = parse_data_url(image_url)?;
                            tool_blocks.push(AnthropicToolResultBlock::Image {
                                source: AnthropicImageSource {
                                    r#type: "base64".to_string(),
                                    media_type,
                                    data,
                                },
                            });
                        }
                        ImageReference::File { file_id } => {
                            return Err(AnthropicAdapterError::UnsupportedContent(format!(
                                "File-based image references ({file_id}) not supported in tool output"
                            )));
                        }
                    },
                    codex_protocol::models::FunctionCallOutputContentItem::InputAudio {
                        ..
                    } => {
                        return Err(AnthropicAdapterError::UnsupportedToolOutputContent(
                            "Audio is not supported in tool output".to_string(),
                        ));
                    }
                    codex_protocol::models::FunctionCallOutputContentItem::EncryptedContent {
                        ..
                    } => {
                        return Err(AnthropicAdapterError::UnsupportedSecurityFeature(
                            "Encrypted tool output content is not supported".to_string(),
                        ));
                    }
                }
            }
            AnthropicToolResultContent::Blocks(tool_blocks)
        }
    };

    let is_error = output.success.map(|s| !s);

    Ok(AnthropicContentBlock::ToolResult {
        tool_use_id: call_id.to_string(),
        content: result_content,
        is_error,
        cache_control: None,
    })
}

/// Helper to coalesce consecutive items into messages of matching role.
fn append_or_push_message(
    messages: &mut Vec<AnthropicMessage>,
    role: &str,
    mut blocks: Vec<AnthropicContentBlock>,
) {
    if blocks.is_empty() {
        return;
    }
    if let Some(last) = messages.last_mut()
        && last.role == role
    {
        last.content.append(&mut blocks);
    } else {
        messages.push(AnthropicMessage {
            role: role.to_string(),
            content: blocks,
        });
    }
}

/// Validates Anthropic tool naming convention: `^[a-zA-Z0-9_-]{1,64}$`.
fn is_valid_anthropic_tool_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Parses base64 data URL: `data:<media_type>;base64,<data>`.
fn parse_data_url(url: &str) -> Result<(String, String), AnthropicAdapterError> {
    if !url.starts_with("data:") {
        return Err(AnthropicAdapterError::UnsupportedImageMime(
            "Image URL must be a data: URL".to_string(),
        ));
    }
    let rest = &url["data:".len()..];
    let (metadata, data) = rest.split_once(',').ok_or_else(|| {
        AnthropicAdapterError::UnsupportedImageMime("Malformed data URL".to_string())
    })?;

    let parts: Vec<&str> = metadata.split(';').collect();
    let media_type = parts
        .first()
        .copied()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    match media_type.as_str() {
        "image/jpeg" | "image/png" | "image/gif" | "image/webp" => {}
        other => {
            return Err(AnthropicAdapterError::UnsupportedImageMime(
                other.to_string(),
            ));
        }
    }

    if !parts.iter().any(|&p| p.trim() == "base64") {
        return Err(AnthropicAdapterError::UnsupportedImageMime(
            "Data URL must specify base64 encoding".to_string(),
        ));
    }

    Ok((media_type, data.trim().to_string()))
}

/// Applies prompt cache control markers based on selected policy.
fn apply_cache_policy(
    policy: AnthropicPromptCachePolicy,
    system_blocks: &mut [AnthropicSystemBlock],
    tools: Option<&mut [AnthropicTool]>,
    messages: &mut [AnthropicMessage],
) {
    match policy {
        AnthropicPromptCachePolicy::None => {}
        AnthropicPromptCachePolicy::LastUserMessage { ttl } => {
            mark_last_user_message(messages, ttl);
        }
        AnthropicPromptCachePolicy::ToolsAndSystem { ttl } => {
            mark_last_system_block(system_blocks, ttl);
            if let Some(tool_list) = tools {
                mark_last_tool(tool_list, ttl);
            }
        }
        AnthropicPromptCachePolicy::AutomaticBreakpoint { ttl } => {
            mark_last_system_block(system_blocks, ttl);
            if let Some(tool_list) = tools {
                mark_last_tool(tool_list, ttl);
            }
            mark_last_user_message(messages, ttl);
        }
    }
}

fn mark_last_system_block(
    system_blocks: &mut [AnthropicSystemBlock],
    ttl: Option<AnthropicCacheTtl>,
) {
    if let Some(last) = system_blocks.last_mut() {
        let AnthropicSystemBlock::Text { cache_control, .. } = last;
        *cache_control = Some(AnthropicCacheControl::Ephemeral { ttl });
    }
}

fn mark_last_tool(tools: &mut [AnthropicTool], ttl: Option<AnthropicCacheTtl>) {
    if let Some(last) = tools.last_mut() {
        last.cache_control = Some(AnthropicCacheControl::Ephemeral { ttl });
    }
}

fn mark_last_user_message(messages: &mut [AnthropicMessage], ttl: Option<AnthropicCacheTtl>) {
    if let Some(last_user) = messages.iter_mut().rev().find(|m| m.role == "user")
        && let Some(last_block) = last_user.content.last_mut()
    {
        match last_block {
            AnthropicContentBlock::Text { cache_control, .. }
            | AnthropicContentBlock::Image { cache_control, .. }
            | AnthropicContentBlock::ToolUse { cache_control, .. }
            | AnthropicContentBlock::ToolResult { cache_control, .. } => {
                *cache_control = Some(AnthropicCacheControl::Ephemeral { ttl });
            }
            AnthropicContentBlock::Thinking { .. }
            | AnthropicContentBlock::RedactedThinking { .. } => {}
        }
    }
}
