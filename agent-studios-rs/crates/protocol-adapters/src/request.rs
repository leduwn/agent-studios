use codex_api::ResponsesApiRequest;
use codex_protocol::models::{
    ContentItem, FunctionCallOutputBody, FunctionCallOutputContentItem, ImageDetail,
    ImageReference, ResponseItem, plaintext_agent_message_content,
};

use crate::error::{ChatAdapterError, ChatAdapterWarning};
use crate::types::{
    ChatCompletionRequest, ChatContentPart, ChatFunctionCall, ChatFunctionDefinition, ChatImageUrl,
    ChatMessage, ChatMessageContent, ChatResponseFormat, ChatStreamOptions, ChatTool, ChatToolCall,
    ChatToolChoice,
};

/// Result of translating a Codex `ResponsesApiRequest` to an OpenAI `ChatCompletionRequest`.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequestTranslation {
    pub request: ChatCompletionRequest,
    pub warnings: Vec<ChatAdapterWarning>,
}

fn map_image_detail(
    detail: Option<ImageDetail>,
    warnings: &mut Vec<ChatAdapterWarning>,
) -> Option<String> {
    detail.map(|d| match d {
        ImageDetail::Auto => "auto".to_string(),
        ImageDetail::Low => "low".to_string(),
        ImageDetail::High => "high".to_string(),
        ImageDetail::Original => {
            warnings.push(ChatAdapterWarning::NormalizedImageDetail {
                from: "original".to_string(),
                to: "high".to_string(),
            });
            "high".to_string()
        }
    })
}

fn convert_tool_output_body(body: &FunctionCallOutputBody) -> Result<String, ChatAdapterError> {
    match body {
        FunctionCallOutputBody::Text(text) => Ok(text.clone()),
        FunctionCallOutputBody::ContentItems(items) => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    FunctionCallOutputContentItem::InputText { text } => {
                        parts.push(text.as_str());
                    }
                    FunctionCallOutputContentItem::InputImage { .. } => {
                        return Err(ChatAdapterError::UnsupportedToolOutputContent(
                            "Tool output contains image content which cannot be translated to Chat Completions without data loss".to_string(),
                        ));
                    }
                    FunctionCallOutputContentItem::InputAudio { .. } => {
                        return Err(ChatAdapterError::UnsupportedToolOutputContent(
                            "Tool output contains audio content which cannot be translated to Chat Completions without data loss".to_string(),
                        ));
                    }
                    FunctionCallOutputContentItem::EncryptedContent { .. } => {
                        return Err(ChatAdapterError::UnsupportedToolOutputContent(
                            "Tool output contains encrypted content which cannot be translated to Chat Completions without data loss".to_string(),
                        ));
                    }
                }
            }
            Ok(parts.join("\n"))
        }
    }
}

fn flatten_tool_definitions(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut flattened = Vec::with_capacity(tools.len());
    for tool in tools {
        let tool_type = tool.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if tool_type == "namespace" {
            if let Some(inner) = tool.get("tools").and_then(|v| v.as_array()) {
                flattened.extend(flatten_tool_definitions(inner));
            }
        } else if tool_type == "web_search" || tool_type == "tool_search" {
            // Server-hosted Responses API tools have no counterpart in wire formats that only accept functions;
            // skip them rather than failing the entire request.
            continue;
        } else {
            flattened.push(tool.clone());
        }
    }
    flattened
}

/// Translates a Codex `ResponsesApiRequest` into an OpenAI `ChatCompletionRequest`.
pub fn translate_request(
    request: &ResponsesApiRequest,
) -> Result<ChatRequestTranslation, ChatAdapterError> {
    let mut warnings = Vec::new();

    // Invariant: access_programs is security/authorization-sensitive and must be rejected.
    if request.access_programs.is_some() {
        return Err(ChatAdapterError::UnsupportedSecurityFeature(
            "access_programs is authorization/security-sensitive and cannot be translated to Chat Completions"
                .to_string(),
        ));
    }

    // Dropped Responses-only fields emit typed warnings
    if let Some(reasoning) = &request.reasoning {
        warnings.push(ChatAdapterWarning::DroppedReasoning(format!(
            "{reasoning:?}"
        )));
    }
    if let Some(key) = &request.prompt_cache_key {
        warnings.push(ChatAdapterWarning::DroppedPromptCacheKey(key.clone()));
    }
    if !request.include.is_empty() {
        warnings.push(ChatAdapterWarning::DroppedInclude(request.include.clone()));
    }
    if let Some(client_metadata) = &request.client_metadata {
        warnings.push(ChatAdapterWarning::DroppedClientMetadata(
            client_metadata.keys().cloned().collect(),
        ));
    }
    if request.store {
        warnings.push(ChatAdapterWarning::DroppedStore(true));
    }
    if let Some(tier) = &request.service_tier {
        warnings.push(ChatAdapterWarning::DroppedServiceTier(tier.clone()));
    }

    let mut messages = Vec::new();

    // Map instructions to the first role = "system" message
    if !request.instructions.trim().is_empty() {
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: Some(ChatMessageContent::Text(request.instructions.clone())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });
    }

    // Translate message history
    for item in &request.input {
        match item {
            ResponseItem::Message { role, content, .. } => {
                // Strict role validation: only user, assistant, system, developer accepted
                let normalized_role = match role.as_str() {
                    "user" | "assistant" | "system" => role.clone(),
                    "developer" => "system".to_string(),
                    _ => return Err(ChatAdapterError::InvalidRole(role.clone())),
                };

                let message_content = if content.is_empty() {
                    None
                } else if content.len() == 1 {
                    match &content[0] {
                        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                            Some(ChatMessageContent::Text(text.clone()))
                        }
                        ContentItem::InputImage { image, detail } => match image {
                            ImageReference::Inline { image_url } => {
                                Some(ChatMessageContent::Parts(vec![ChatContentPart::ImageUrl {
                                    image_url: ChatImageUrl {
                                        url: image_url.clone(),
                                        detail: map_image_detail(*detail, &mut warnings),
                                    },
                                }]))
                            }
                            ImageReference::File { file_id } => {
                                return Err(ChatAdapterError::UnsupportedContent(format!(
                                    "File-based image references ({file_id}) cannot be translated to Chat wire format"
                                )));
                            }
                        },
                        ContentItem::InputAudio { .. } => {
                            return Err(ChatAdapterError::UnsupportedContent(
                                "Audio input is not supported in Chat Completions wire format"
                                    .to_string(),
                            ));
                        }
                    }
                } else {
                    let mut parts = Vec::new();
                    for part in content {
                        match part {
                            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                                parts.push(ChatContentPart::Text { text: text.clone() });
                            }
                            ContentItem::InputImage { image, detail } => match image {
                                ImageReference::Inline { image_url } => {
                                    parts.push(ChatContentPart::ImageUrl {
                                        image_url: ChatImageUrl {
                                            url: image_url.clone(),
                                            detail: map_image_detail(*detail, &mut warnings),
                                        },
                                    });
                                }
                                ImageReference::File { file_id } => {
                                    return Err(ChatAdapterError::UnsupportedContent(format!(
                                        "File-based image references ({file_id}) cannot be translated to Chat wire format"
                                    )));
                                }
                            },
                            ContentItem::InputAudio { .. } => {
                                return Err(ChatAdapterError::UnsupportedContent(
                                    "Audio input is not supported in Chat Completions wire format"
                                        .to_string(),
                                ));
                            }
                        }
                    }
                    Some(ChatMessageContent::Parts(parts))
                };

                messages.push(ChatMessage {
                    role: normalized_role,
                    content: message_content,
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                });
            }

            ResponseItem::FunctionCall {
                call_id,
                name,
                arguments,
                ..
            } => {
                messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: None,
                    name: None,
                    tool_calls: Some(vec![ChatToolCall {
                        id: call_id.clone(),
                        r#type: "function".to_string(),
                        function: ChatFunctionCall {
                            name: name.clone(),
                            arguments: arguments.clone(),
                        },
                    }]),
                    tool_call_id: None,
                });
            }

            ResponseItem::FunctionCallOutput {
                call_id, output, ..
            } => {
                let id = call_id.as_ref().ok_or_else(|| {
                    ChatAdapterError::MissingToolCallId(
                        "FunctionCallOutput must have call_id to map to Chat tool message"
                            .to_string(),
                    )
                })?;

                let content_str = convert_tool_output_body(&output.body)?;

                messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: Some(ChatMessageContent::Text(content_str)),
                    name: None,
                    tool_calls: None,
                    tool_call_id: Some(id.clone()),
                });
            }

            ResponseItem::CustomToolCall {
                call_id,
                name,
                input,
                ..
            } => {
                messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: None,
                    name: None,
                    tool_calls: Some(vec![ChatToolCall {
                        id: call_id.clone(),
                        r#type: "function".to_string(),
                        function: ChatFunctionCall {
                            name: name.clone(),
                            arguments: input.clone(),
                        },
                    }]),
                    tool_call_id: None,
                });
            }

            ResponseItem::CustomToolCallOutput {
                call_id, output, ..
            } => {
                let content_str = convert_tool_output_body(&output.body)?;

                messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: Some(ChatMessageContent::Text(content_str)),
                    name: None,
                    tool_calls: None,
                    tool_call_id: Some(call_id.clone()),
                });
            }

            ResponseItem::AgentMessage {
                author, content, ..
            } => {
                if let Some(text) = plaintext_agent_message_content(content) {
                    messages.push(ChatMessage {
                        role: "assistant".to_string(),
                        content: Some(ChatMessageContent::Text(text)),
                        name: Some(author.clone()),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                } else {
                    return Err(ChatAdapterError::UnsupportedEncryptedAgentMessage);
                }
            }

            ResponseItem::Reasoning { .. } => {
                warnings.push(ChatAdapterWarning::DroppedReasoning(
                    "Reasoning item in conversation history dropped".to_string(),
                ));
            }

            ResponseItem::LocalShellCall { .. } => {
                return Err(ChatAdapterError::UnsupportedResponseItem(
                    "LocalShellCall is not supported in Chat Completions wire format".to_string(),
                ));
            }
            ResponseItem::ToolSearchCall { .. } => {
                return Err(ChatAdapterError::UnsupportedResponseItem(
                    "ToolSearchCall is not supported in Chat Completions wire format".to_string(),
                ));
            }
            ResponseItem::ToolSearchOutput { .. } => {
                return Err(ChatAdapterError::UnsupportedResponseItem(
                    "ToolSearchOutput is not supported in Chat Completions wire format".to_string(),
                ));
            }
            ResponseItem::WebSearchCall { .. } => {
                return Err(ChatAdapterError::UnsupportedResponseItem(
                    "WebSearchCall is not supported in Chat Completions wire format".to_string(),
                ));
            }
            ResponseItem::ImageGenerationCall { .. } => {
                return Err(ChatAdapterError::UnsupportedResponseItem(
                    "ImageGenerationCall is not supported in Chat Completions wire format"
                        .to_string(),
                ));
            }
            ResponseItem::Compaction { .. } => {
                return Err(ChatAdapterError::UnsupportedResponseItem(
                    "Compaction is not supported in Chat Completions wire format".to_string(),
                ));
            }
            ResponseItem::ConfigurationUpdate { .. } => {
                return Err(ChatAdapterError::UnsupportedResponseItem(
                    "ConfigurationUpdate is not supported in Chat Completions wire format"
                        .to_string(),
                ));
            }
            ResponseItem::AdditionalTools { .. } => {
                return Err(ChatAdapterError::UnsupportedResponseItem(
                    "AdditionalTools is not supported in Chat Completions wire format".to_string(),
                ));
            }
            item => {
                return Err(ChatAdapterError::UnsupportedResponseItem(format!(
                    "Unsupported ResponseItem variant in conversation history: {item:?}"
                )));
            }
        }
    }

    // Translate tools (Responses flat function vs Chat nested function)
    let tools = if let Some(raw_tools) = &request.tools {
        let parsed: serde_json::Value = serde_json::to_value(raw_tools).map_err(|e| {
            ChatAdapterError::InvalidJson(format!("Failed to parse tools JSON: {e}"))
        })?;

        let array = parsed.as_array().ok_or_else(|| {
            ChatAdapterError::InvalidToolDefinition(
                "tools payload must be a JSON array".to_string(),
            )
        })?;

        let flattened = flatten_tool_definitions(array);
        let mut chat_tools = Vec::with_capacity(flattened.len());
        for tool_val in &flattened {
            let tool_type = tool_val.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if tool_type != "function" {
                return Err(ChatAdapterError::UnsupportedToolType(format!(
                    "Tool type '{tool_type}' cannot be translated to Chat Completions"
                )));
            }

            if let Some(function_obj) = tool_val.get("function") {
                // Already nested Chat format
                let name = function_obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ChatAdapterError::InvalidToolDefinition(
                            "Tool function missing name".to_string(),
                        )
                    })?;
                let description = function_obj
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let parameters = function_obj.get("parameters").cloned();
                let strict = function_obj.get("strict").and_then(|v| v.as_bool());

                chat_tools.push(ChatTool {
                    r#type: "function".to_string(),
                    function: ChatFunctionDefinition {
                        name: name.to_string(),
                        description,
                        parameters,
                        strict,
                    },
                });
            } else {
                // Responses API flat format: { "type": "function", "name": ..., "description": ..., "parameters": ..., "strict": ... }
                let name = tool_val
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ChatAdapterError::InvalidToolDefinition(
                            "Responses tool missing name".to_string(),
                        )
                    })?;
                let description = tool_val
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let parameters = tool_val.get("parameters").cloned();
                let strict = tool_val.get("strict").and_then(|v| v.as_bool());

                chat_tools.push(ChatTool {
                    r#type: "function".to_string(),
                    function: ChatFunctionDefinition {
                        name: name.to_string(),
                        description,
                        parameters,
                        strict,
                    },
                });
            }
        }
        Some(chat_tools)
    } else {
        None
    };

    // Tool choice mapping: strictly allow only "", "auto", "none", "required"
    let tool_choice = match request.tool_choice.as_str() {
        "" => None,
        "auto" => Some(ChatToolChoice::Auto),
        "none" => Some(ChatToolChoice::None),
        "required" => Some(ChatToolChoice::Required),
        other => return Err(ChatAdapterError::UnsupportedToolChoice(other.to_string())),
    };

    // Structured output / response_format mapping
    let response_format = if let Some(text_controls) = &request.text {
        if let Some(verbosity) = &text_controls.verbosity {
            warnings.push(ChatAdapterWarning::DroppedVerbosity(format!(
                "{verbosity:?}"
            )));
        }

        text_controls
            .format
            .as_ref()
            .map(|format| ChatResponseFormat {
                r#type: "json_schema".to_string(),
                json_schema: Some(crate::types::ChatJsonSchemaFormat {
                    name: format.name.clone(),
                    schema: Some(format.schema.clone()),
                    strict: Some(format.strict),
                }),
            })
    } else {
        None
    };

    let stream = if request.stream { Some(true) } else { None };
    let stream_options = if request.stream {
        Some(ChatStreamOptions {
            include_usage: Some(true),
        })
    } else {
        None
    };

    let chat_request = ChatCompletionRequest {
        model: request.model.clone(),
        messages,
        tools,
        tool_choice,
        parallel_tool_calls: Some(request.parallel_tool_calls),
        response_format,
        stream,
        stream_options,
    };

    Ok(ChatRequestTranslation {
        request: chat_request,
        warnings,
    })
}
