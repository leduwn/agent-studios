use std::sync::Arc;

use agent_studios_protocol_adapters::anthropic::{
    AnthropicAdapterError, AnthropicAdapterWarning, AnthropicCacheControl, AnthropicContentBlock,
    AnthropicContinuationState, AnthropicPromptCachePolicy, AnthropicRequestOptions,
    AnthropicThinkingConfig, AnthropicThinkingPolicy, AnthropicToolChoice,
    AnthropicToolResultContent, NativeThinkingBlock, translate_request,
};
use codex_api::{Reasoning, ResponsesApiRequest, ResponsesApiTools, create_text_param_for_request};
use codex_protocol::models::{
    ContentItem, FunctionCallOutputBody, FunctionCallOutputPayload, ImageReference, ResponseItem,
};
use codex_protocol::openai_models::ReasoningEffort;
use serde_json::value::RawValue;

fn make_base_request() -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        stream: true,
        service_tier: None,
        instructions: String::new(),
        input: Vec::new(),
        tools: None,
        tool_choice: String::new(),
        parallel_tool_calls: true,
        reasoning: None,
        store: false,
        stream_options: None,
        include: Vec::new(),
        prompt_cache_key: None,
        text: None,
        client_metadata: None,
        access_programs: None,
    }
}

#[test]
fn test_01_basic_request_translation() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Hello, Claude!".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions {
        max_tokens: 2048,
        ..Default::default()
    };

    let result = translate_request(&req, &options, None).expect("Translation failed");
    assert_eq!(result.request.model, "claude-3-5-sonnet-20241022");
    assert_eq!(result.request.max_tokens, 2048);
    assert_eq!(result.request.stream, Some(true));
    assert_eq!(result.request.messages.len(), 1);
    assert_eq!(result.request.messages[0].role, "user");
    assert_eq!(result.request.messages[0].content.len(), 1);
    match &result.request.messages[0].content[0] {
        AnthropicContentBlock::Text { text, .. } => assert_eq!(text, "Hello, Claude!"),
        other => panic!("Expected Text block, found {other:?}"),
    }
}

#[test]
fn test_02_zero_max_tokens_rejected() {
    let req = make_base_request();
    let options = AnthropicRequestOptions {
        max_tokens: 0,
        ..Default::default()
    };

    let err = translate_request(&req, &options, None).unwrap_err();
    assert_eq!(err, AnthropicAdapterError::InvalidMaxTokens(0));
}

#[test]
fn test_03_system_instructions_and_leading_system_messages() {
    let mut req = make_base_request();
    req.instructions = "Base developer instruction".to_string();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "system".to_string(),
        content: vec![ContentItem::InputText {
            text: "Leading system message 1".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });
    req.input.push(ResponseItem::Message {
        id: None,
        role: "developer".to_string(),
        content: vec![ContentItem::InputText {
            text: "Leading developer message 2".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "User turn".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions::default();
    let result = translate_request(&req, &options, None).expect("Translation failed");

    let system = result.request.system.expect("Expected system blocks");
    assert_eq!(system.len(), 3);
    assert_eq!(
        system[0],
        agent_studios_protocol_adapters::anthropic::AnthropicSystemBlock::Text {
            text: "Base developer instruction".to_string(),
            cache_control: None,
        }
    );
    assert_eq!(
        system[1],
        agent_studios_protocol_adapters::anthropic::AnthropicSystemBlock::Text {
            text: "Leading system message 1".to_string(),
            cache_control: None,
        }
    );
    assert_eq!(
        system[2],
        agent_studios_protocol_adapters::anthropic::AnthropicSystemBlock::Text {
            text: "Leading developer message 2".to_string(),
            cache_control: None,
        }
    );
}

#[test]
fn test_04_interleaved_system_message_fails_closed() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "User turn 1".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });
    req.input.push(ResponseItem::Message {
        id: None,
        role: "system".to_string(),
        content: vec![ContentItem::InputText {
            text: "Late system message".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions::default();
    let err = translate_request(&req, &options, None).unwrap_err();
    match err {
        AnthropicAdapterError::UnsupportedSystemHistoryPlacement(msg) => {
            assert!(msg.contains("appeared after conversational turns"));
        }
        other => panic!("Expected UnsupportedSystemHistoryPlacement, found {other:?}"),
    }
}

#[test]
fn test_05_multimodal_image_translation() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![
            ContentItem::InputText {
                text: "Inspect this image:".to_string(),
            },
            ContentItem::InputImage {
                image: ImageReference::Inline {
                    image_url: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==".to_string(),
                },
                detail: None,
            },
        ],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions::default();
    let result = translate_request(&req, &options, None).expect("Translation failed");

    assert_eq!(result.request.messages.len(), 1);
    assert_eq!(result.request.messages[0].content.len(), 2);
    match &result.request.messages[0].content[1] {
        AnthropicContentBlock::Image { source, .. } => {
            assert_eq!(source.r#type, "base64");
            assert_eq!(source.media_type, "image/png");
            assert_eq!(source.data, "iVBORw0KGgoAAAANSUhEUg==");
        }
        other => panic!("Expected Image content block, found {other:?}"),
    }
}

#[test]
fn test_06_unsupported_image_mime_rejected() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image: ImageReference::Inline {
                image_url: "data:image/bmp;base64,Qk02".to_string(),
            },
            detail: None,
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions::default();
    let err = translate_request(&req, &options, None).unwrap_err();
    assert_eq!(
        err,
        AnthropicAdapterError::UnsupportedImageMime("image/bmp".to_string())
    );
}

#[test]
fn test_07_grouped_parallel_tool_results() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("call_tool_1".to_string()),
        name: Some("read_file".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text("contents of file 1".to_string()),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    });
    req.input.push(ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("call_tool_2".to_string()),
        name: Some("list_dir".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text("file1.txt\nfile2.txt".to_string()),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions::default();
    let result = translate_request(&req, &options, None).expect("Translation failed");

    // Both consecutive tool results coalesced into 1 user message
    assert_eq!(result.request.messages.len(), 1);
    let user_msg = &result.request.messages[0];
    assert_eq!(user_msg.role, "user");
    assert_eq!(user_msg.content.len(), 2);

    match &user_msg.content[0] {
        AnthropicContentBlock::ToolResult {
            tool_use_id,
            content,
            ..
        } => {
            assert_eq!(tool_use_id, "call_tool_1");
            assert_eq!(
                *content,
                AnthropicToolResultContent::Text("contents of file 1".to_string())
            );
        }
        other => panic!("Expected ToolResult, found {other:?}"),
    }

    match &user_msg.content[1] {
        AnthropicContentBlock::ToolResult {
            tool_use_id,
            content,
            ..
        } => {
            assert_eq!(tool_use_id, "call_tool_2");
            assert_eq!(
                *content,
                AnthropicToolResultContent::Text("file1.txt\nfile2.txt".to_string())
            );
        }
        other => panic!("Expected ToolResult, found {other:?}"),
    }
}

#[test]
fn test_08_function_call_invalid_json_arguments_rejected() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::FunctionCall {
        id: None,
        name: "test_tool".to_string(),
        namespace: None,
        arguments: "{ invalid json ...".to_string(),
        encrypted_function_args: None,
        call_id: "call_1".to_string(),
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions::default();
    let err = translate_request(&req, &options, None).unwrap_err();
    match err {
        AnthropicAdapterError::InvalidToolArguments(_) => {}
        other => panic!("Expected InvalidToolArguments, found {other:?}"),
    }
}

#[test]
fn test_09_tool_naming_validation() {
    let mut req = make_base_request();
    let raw = RawValue::from_string(
        r#"[{"type": "function", "name": "invalid name with spaces", "description": "desc", "parameters": {}}]"#
            .to_string(),
    )
    .unwrap();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));

    let options = AnthropicRequestOptions {
        validate_tool_names: true,
        ..Default::default()
    };

    let err = translate_request(&req, &options, None).unwrap_err();
    match err {
        AnthropicAdapterError::UnsupportedToolType(msg) => {
            assert!(msg.contains("violates Anthropic naming rules"));
        }
        other => panic!("Expected UnsupportedToolType, found {other:?}"),
    }
}

#[test]
fn test_10_security_rejection_of_access_programs() {
    let mut req = make_base_request();
    let raw = RawValue::from_string(
        r#"[{"type": "function", "name": "access_programs", "description": "sensitive", "parameters": {}}]"#
            .to_string(),
    )
    .unwrap();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));

    let options = AnthropicRequestOptions::default();
    let err = translate_request(&req, &options, None).unwrap_err();
    assert_eq!(
        err,
        AnthropicAdapterError::UnsupportedSecurityFeature(
            "access_programs tool is security-sensitive and rejected".to_string()
        )
    );
}

#[test]
fn test_11_tool_choice_mappings() {
    // 1. auto
    let mut req1 = make_base_request();
    req1.tool_choice = "auto".to_string();
    req1.parallel_tool_calls = false;
    let res1 = translate_request(&req1, &AnthropicRequestOptions::default(), None).unwrap();
    assert_eq!(
        res1.request.tool_choice,
        Some(AnthropicToolChoice::Auto {
            disable_parallel_tool_use: Some(true)
        })
    );

    // 2. none
    let mut req2 = make_base_request();
    let raw = RawValue::from_string(
        r#"[{"type": "function", "name": "valid_tool", "description": "desc", "parameters": {}}]"#
            .to_string(),
    )
    .unwrap();
    req2.tools = Some(ResponsesApiTools::from(Arc::from(raw)));
    req2.tool_choice = "none".to_string();
    let res2 = translate_request(&req2, &AnthropicRequestOptions::default(), None).unwrap();
    assert_eq!(res2.request.tools, None);
    assert_eq!(res2.request.tool_choice, None);

    // 3. required
    let mut req3 = make_base_request();
    req3.tool_choice = "required".to_string();
    let res3 = translate_request(&req3, &AnthropicRequestOptions::default(), None).unwrap();
    assert_eq!(
        res3.request.tool_choice,
        Some(AnthropicToolChoice::Any {
            disable_parallel_tool_use: None
        })
    );

    // 4. specific tool
    let mut req4 = make_base_request();
    req4.tool_choice = "custom_tool".to_string();
    let res4 = translate_request(&req4, &AnthropicRequestOptions::default(), None).unwrap();
    assert_eq!(
        res4.request.tool_choice,
        Some(AnthropicToolChoice::Tool {
            name: "custom_tool".to_string(),
            disable_parallel_tool_use: None,
        })
    );
}

#[test]
fn test_12_reasoning_effort_mapping() {
    let pairs = [
        (ReasoningEffort::Low, "low"),
        (ReasoningEffort::Medium, "medium"),
        (ReasoningEffort::High, "high"),
        (ReasoningEffort::XHigh, "xhigh"),
        (ReasoningEffort::Max, "max"),
    ];

    for (effort, expected) in pairs {
        let mut req = make_base_request();
        req.reasoning = Some(Reasoning {
            effort: Some(effort),
            summary: None,
            context: None,
        });

        let res = translate_request(&req, &AnthropicRequestOptions::default(), None).unwrap();
        assert_eq!(
            res.request
                .output_config
                .as_ref()
                .and_then(|o| o.effort.as_deref()),
            Some(expected)
        );
    }

    // Unsupported effort
    let mut req_unsupported = make_base_request();
    req_unsupported.reasoning = Some(Reasoning {
        effort: Some(ReasoningEffort::Ultra),
        summary: None,
        context: None,
    });

    let err =
        translate_request(&req_unsupported, &AnthropicRequestOptions::default(), None).unwrap_err();
    match err {
        AnthropicAdapterError::UnsupportedReasoningEffort(_) => {}
        other => panic!("Expected UnsupportedReasoningEffort, found {other:?}"),
    }
}

#[test]
fn test_13_structured_outputs_format_mapping() {
    let mut req = make_base_request();
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "answer": { "type": "string" }
        }
    });
    req.text = create_text_param_for_request(None, &Some(schema), true);

    let res = translate_request(&req, &AnthropicRequestOptions::default(), None).unwrap();
    let format = res
        .request
        .output_config
        .as_ref()
        .and_then(|o| o.format.as_ref())
        .expect("Expected output format");

    match format {
        agent_studios_protocol_adapters::anthropic::AnthropicOutputFormat::JsonSchema {
            schema,
        } => {
            assert_eq!(schema["type"], "object");
            assert_eq!(schema["properties"]["answer"]["type"], "string");
        }
    }
}

#[test]
fn test_14_caching_and_thinking_policies() {
    let mut req = make_base_request();
    req.instructions = "Instructions".to_string();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "User turn".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions {
        prompt_cache_policy: AnthropicPromptCachePolicy::AutomaticBreakpoint,
        thinking_policy: AnthropicThinkingPolicy::BudgetTokens(2048),
        ..Default::default()
    };

    let res = translate_request(&req, &options, None).unwrap();

    // Check system cache control
    let system = res.request.system.unwrap();
    match &system[0] {
        agent_studios_protocol_adapters::anthropic::AnthropicSystemBlock::Text {
            cache_control,
            ..
        } => {
            assert_eq!(
                *cache_control,
                Some(AnthropicCacheControl::Ephemeral { ttl: None })
            );
        }
    }

    // Check user message cache control
    match &res.request.messages[0].content[0] {
        AnthropicContentBlock::Text { cache_control, .. } => {
            assert_eq!(
                *cache_control,
                Some(AnthropicCacheControl::Ephemeral { ttl: None })
            );
        }
        other => panic!("Expected text block, found {other:?}"),
    }

    // Check thinking config
    assert_eq!(
        res.request.thinking,
        Some(AnthropicThinkingConfig::Enabled {
            budget_tokens: 2048
        })
    );
}

#[test]
fn test_15_continuation_state_and_cross_provider_reasoning() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::Reasoning {
        id: Some(codex_protocol::ResponseItemId::from_server(
            "anthropic-reasoning-msg_1-0".to_string(),
        )),
        summary: Vec::new(),
        content: None,
        encrypted_content: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let mut cont = AnthropicContinuationState::new();
    cont.insert_reasoning_block(
        "anthropic-reasoning-msg_1-0",
        NativeThinkingBlock::Thinking {
            thinking: "Preserved reasoning thoughts".to_string(),
            signature: "sig_abc123".to_string(),
        },
    );

    let options = AnthropicRequestOptions::default();
    let res = translate_request(&req, &options, Some(&cont)).unwrap();

    assert_eq!(res.request.messages.len(), 1);
    assert_eq!(res.request.messages[0].role, "assistant");
    match &res.request.messages[0].content[0] {
        AnthropicContentBlock::Thinking {
            thinking,
            signature,
        } => {
            assert_eq!(thinking, "Preserved reasoning thoughts");
            assert_eq!(signature, "sig_abc123");
        }
        other => panic!("Expected Thinking block, found {other:?}"),
    }

    // Cross-provider reasoning item omitted with warning
    let mut req_cross = make_base_request();
    req_cross.input.push(ResponseItem::Reasoning {
        id: Some(codex_protocol::ResponseItemId::from_server(
            "openai-reasoning-item-42".to_string(),
        )),
        summary: Vec::new(),
        content: None,
        encrypted_content: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let res_cross = translate_request(&req_cross, &options, None).unwrap();
    assert!(
        res_cross
            .warnings
            .contains(&AnthropicAdapterWarning::CrossProviderReasoningOmitted(
                "openai-reasoning-item-42".to_string()
            ))
    );
}
