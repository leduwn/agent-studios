use std::sync::Arc;

use agent_studios_protocol_adapters::anthropic::{
    AnthropicAdapterError, AnthropicAdapterWarning, AnthropicCacheControl, AnthropicCacheTtl,
    AnthropicContentBlock, AnthropicContinuationState, AnthropicPromptCachePolicy,
    AnthropicRequestOptions, AnthropicStreamTranslator, AnthropicThinkingConfig,
    AnthropicThinkingPolicy, AnthropicToolChoice, AnthropicToolResultBlock,
    AnthropicToolResultContent, NativeThinkingBlock, translate_request,
};
use codex_api::{ResponseEvent, ResponsesApiRequest, ResponsesApiTools};
use codex_protocol::models::{
    ContentItem, FunctionCallOutputBody, FunctionCallOutputContentItem, FunctionCallOutputPayload,
    ImageReference, ResponseItem,
};
use serde_json::value::RawValue;

fn make_base_request() -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: "claude-3-7-sonnet-20250219".to_string(),
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

// ---------------------------------------------------------------------------
// 1. cache-read TokenUsage maps into full Codex input
// ---------------------------------------------------------------------------
#[test]
fn test_01_cache_read_token_usage_maps_into_full_codex_input() {
    let mut translator = AnthropicStreamTranslator::new();
    let sse = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","model":"claude-3-7-sonnet-20250219","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":100,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":40}}}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":15}}

event: message_stop
data: {"type":"message_stop"}

"#;

    let events = translator.feed_sse_chunk(sse).unwrap();
    let usage = events
        .iter()
        .find_map(|e| match e {
            ResponseEvent::Completed { token_usage, .. } => token_usage.as_ref(),
            _ => None,
        })
        .expect("Completed token usage");

    // effective_input = 100 (uncached) + 40 (cached) + 0 (creation) = 140
    assert_eq!(usage.input_tokens, 140);
    assert_eq!(usage.cached_input_tokens, 40);
    assert_eq!(usage.cache_write_input_tokens, 0);
    assert_eq!(usage.output_tokens, 15);
    assert_eq!(usage.total_tokens, 155);
}

// ---------------------------------------------------------------------------
// 2. cache-write TokenUsage maps into full Codex input
// ---------------------------------------------------------------------------
#[test]
fn test_02_cache_write_token_usage_maps_into_full_codex_input() {
    let mut translator = AnthropicStreamTranslator::new();
    let sse = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_2","type":"message","role":"assistant","model":"claude-3-7-sonnet-20250219","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":200,"output_tokens":0,"cache_creation_input_tokens":50,"cache_read_input_tokens":0}}}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":20}}

event: message_stop
data: {"type":"message_stop"}

"#;

    let events = translator.feed_sse_chunk(sse).unwrap();
    let usage = events
        .iter()
        .find_map(|e| match e {
            ResponseEvent::Completed { token_usage, .. } => token_usage.as_ref(),
            _ => None,
        })
        .expect("Completed token usage");

    // effective_input = 200 + 0 + 50 = 250
    assert_eq!(usage.input_tokens, 250);
    assert_eq!(usage.cached_input_tokens, 0);
    assert_eq!(usage.cache_write_input_tokens, 50);
    assert_eq!(usage.output_tokens, 20);
    assert_eq!(usage.total_tokens, 270);
}

// ---------------------------------------------------------------------------
// 3. mixed cache read/write total correct
// ---------------------------------------------------------------------------
#[test]
fn test_03_mixed_cache_read_write_total_correct() {
    let mut translator = AnthropicStreamTranslator::new();
    let sse = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_3","type":"message","role":"assistant","model":"claude-3-7-sonnet-20250219","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":300,"output_tokens":0,"cache_creation_input_tokens":25,"cache_read_input_tokens":75}}}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":50}}

event: message_stop
data: {"type":"message_stop"}

"#;

    let events = translator.feed_sse_chunk(sse).unwrap();
    let usage = events
        .iter()
        .find_map(|e| match e {
            ResponseEvent::Completed { token_usage, .. } => token_usage.as_ref(),
            _ => None,
        })
        .expect("Completed token usage");

    // effective_input = 300 + 75 + 25 = 400
    assert_eq!(usage.input_tokens, 400);
    assert_eq!(usage.cached_input_tokens, 75);
    assert_eq!(usage.cache_write_input_tokens, 25);
    assert_eq!(usage.output_tokens, 50);
    assert_eq!(usage.total_tokens, 450);
}

// ---------------------------------------------------------------------------
// 4. cumulative output usage not double-added
// ---------------------------------------------------------------------------
#[test]
fn test_04_cumulative_output_usage_not_double_added() {
    let mut translator = AnthropicStreamTranslator::new();
    let sse = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_cumul","type":"message","role":"assistant","model":"claude-3-7-sonnet-20250219","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":50,"output_tokens":0}}}

event: message_delta
data: {"type":"message_delta","delta":{"stop_sequence":null},"usage":{"output_tokens":10}}

event: message_delta
data: {"type":"message_delta","delta":{"stop_sequence":null},"usage":{"output_tokens":25}}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":40}}

event: message_stop
data: {"type":"message_stop"}

"#;

    let events = translator.feed_sse_chunk(sse).unwrap();
    let usage = events
        .iter()
        .find_map(|e| match e {
            ResponseEvent::Completed { token_usage, .. } => token_usage.as_ref(),
            _ => None,
        })
        .expect("Completed token usage");

    // Must be latest cumulative 40, not 10 + 25 + 40 = 75
    assert_eq!(usage.output_tokens, 40);
    assert_eq!(usage.total_tokens, 90);
}

// ---------------------------------------------------------------------------
// 5. tool_choice none preserves tools
// ---------------------------------------------------------------------------
#[test]
fn test_05_tool_choice_none_preserves_tools() {
    let mut req = make_base_request();
    let raw = RawValue::from_string(
        r#"[{"type": "function", "name": "test_fn", "description": "desc", "parameters": {}}]"#
            .to_string(),
    )
    .unwrap();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));
    req.tool_choice = "none".to_string();

    let options = AnthropicRequestOptions::new(4096);
    let res = translate_request(&req, &options, None).unwrap();

    let tools = res.request.tools.expect("Tools should be preserved");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "test_fn");
}

// ---------------------------------------------------------------------------
// 6. tool_choice none serializes native None
// ---------------------------------------------------------------------------
#[test]
fn test_06_tool_choice_none_serializes_native_none() {
    let mut req = make_base_request();
    let raw = RawValue::from_string(
        r#"[{"type": "function", "name": "test_fn", "description": "desc", "parameters": {}}]"#
            .to_string(),
    )
    .unwrap();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));
    req.tool_choice = "none".to_string();

    let options = AnthropicRequestOptions::new(4096);
    let res = translate_request(&req, &options, None).unwrap();

    assert_eq!(res.request.tool_choice, Some(AnthropicToolChoice::None));
    let json = serde_json::to_value(&res.request).unwrap();
    assert_eq!(json["tool_choice"], serde_json::json!({"type": "none"}));
}

// ---------------------------------------------------------------------------
// 7. arbitrary tool_choice rejected
// ---------------------------------------------------------------------------
#[test]
fn test_07_arbitrary_tool_choice_rejected() {
    let options = AnthropicRequestOptions::new(4096);

    for tc in ["custom_fn", "any", "other", "function:my_tool"] {
        let mut req = make_base_request();
        req.tool_choice = tc.to_string();
        let err = translate_request(&req, &options, None).unwrap_err();
        assert_eq!(
            err,
            AnthropicAdapterError::UnsupportedToolChoice(tc.to_string())
        );
    }

    // Supported choices must not be rejected with UnsupportedToolChoice
    for tc in ["", "auto", "none", "required"] {
        let mut req = make_base_request();
        req.tool_choice = tc.to_string();
        assert!(translate_request(&req, &options, None).is_ok());
    }
}

// ---------------------------------------------------------------------------
// 8. strict=true preserved (flat and nested)
// ---------------------------------------------------------------------------
#[test]
fn test_08_strict_true_preserved() {
    let mut req = make_base_request();
    let raw = RawValue::from_string(
        r#"[
            {"type": "function", "name": "flat_strict", "description": "d", "strict": true, "parameters": {}},
            {"type": "function", "function": {"name": "nested_strict", "description": "d", "strict": true, "parameters": {}}}
        ]"#
        .to_string(),
    )
    .unwrap();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));

    let options = AnthropicRequestOptions::new(4096);
    let res = translate_request(&req, &options, None).unwrap();

    let tools = res.request.tools.as_ref().unwrap();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].strict, Some(true));
    assert_eq!(tools[1].strict, Some(true));

    let json = serde_json::to_value(&res.request).unwrap();
    assert_eq!(json["tools"][0]["strict"], true);
    assert_eq!(json["tools"][1]["strict"], true);
}

// ---------------------------------------------------------------------------
// 9. strict=false preserved
// ---------------------------------------------------------------------------
#[test]
fn test_09_strict_false_preserved() {
    let mut req = make_base_request();
    let raw = RawValue::from_string(
        r#"[{"type": "function", "name": "non_strict", "description": "d", "strict": false, "parameters": {}}]"#
            .to_string(),
    )
    .unwrap();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));

    let options = AnthropicRequestOptions::new(4096);
    let res = translate_request(&req, &options, None).unwrap();

    let tools = res.request.tools.as_ref().unwrap();
    assert_eq!(tools[0].strict, Some(false));

    let json = serde_json::to_value(&res.request).unwrap();
    assert_eq!(json["tools"][0]["strict"], false);
}

// ---------------------------------------------------------------------------
// 10. strict absent stays absent
// ---------------------------------------------------------------------------
#[test]
fn test_10_strict_absent_stays_absent() {
    let mut req = make_base_request();
    let raw = RawValue::from_string(
        r#"[{"type": "function", "name": "normal_tool", "description": "d", "parameters": {}}]"#
            .to_string(),
    )
    .unwrap();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));

    let options = AnthropicRequestOptions::new(4096);
    let res = translate_request(&req, &options, None).unwrap();

    let tools = res.request.tools.as_ref().unwrap();
    assert_eq!(tools[0].strict, None);

    let json = serde_json::to_value(&res.request).unwrap();
    assert!(json["tools"][0].get("strict").is_none());
}

// ---------------------------------------------------------------------------
// 11. tool-name boundary current contract (^[a-zA-Z0-9_-]{1,64}$)
// ---------------------------------------------------------------------------
#[test]
fn test_11_tool_name_boundary_current_contract() {
    let mut options = AnthropicRequestOptions::new(4096);
    options.validate_tool_names = true;

    // Length 1 (min) -> valid
    let mut req1 = make_base_request();
    let raw1 = RawValue::from_string(
        r#"[{"type": "function", "name": "a", "description": "d", "parameters": {}}]"#.to_string(),
    )
    .unwrap();
    req1.tools = Some(ResponsesApiTools::from(Arc::from(raw1)));
    assert!(translate_request(&req1, &options, None).is_ok());

    // Length 64 (max) -> valid
    let name_64 = "a".repeat(64);
    let mut req64 = make_base_request();
    let raw64 = RawValue::from_string(format!(
        r#"[{{ "type": "function", "name": "{name_64}", "description": "d", "parameters": {{}} }}]"#
    ))
    .unwrap();
    req64.tools = Some(ResponsesApiTools::from(Arc::from(raw64)));
    assert!(translate_request(&req64, &options, None).is_ok());

    // Length 65 (over max) -> invalid
    let name_65 = "a".repeat(65);
    let mut req65 = make_base_request();
    let raw65 = RawValue::from_string(format!(
        r#"[{{ "type": "function", "name": "{name_65}", "description": "d", "parameters": {{}} }}]"#
    ))
    .unwrap();
    req65.tools = Some(ResponsesApiTools::from(Arc::from(raw65)));
    let err65 = translate_request(&req65, &options, None).unwrap_err();
    assert!(matches!(
        err65,
        AnthropicAdapterError::UnsupportedToolType(_)
    ));

    // Valid characters: letters, digits, underscores, hyphens
    let mut req_valid_chars = make_base_request();
    let raw_vc = RawValue::from_string(
        r#"[{"type": "function", "name": "Valid_Tool-123", "description": "d", "parameters": {}}]"#
            .to_string(),
    )
    .unwrap();
    req_valid_chars.tools = Some(ResponsesApiTools::from(Arc::from(raw_vc)));
    assert!(translate_request(&req_valid_chars, &options, None).is_ok());

    // Invalid characters: spaces or symbols
    for invalid_name in ["tool with space", "tool@name", "tool.name", "tool$"] {
        let mut req_inv = make_base_request();
        let raw_inv = RawValue::from_string(format!(
            r#"[{{ "type": "function", "name": "{invalid_name}", "description": "d", "parameters": {{}} }}]"#
        ))
        .unwrap();
        req_inv.tools = Some(ResponsesApiTools::from(Arc::from(raw_inv)));
        let err = translate_request(&req_inv, &options, None).unwrap_err();
        assert!(matches!(err, AnthropicAdapterError::UnsupportedToolType(_)));
    }
}

// ---------------------------------------------------------------------------
// 12. invalid custom tool input rejects
// ---------------------------------------------------------------------------
#[test]
fn test_12_invalid_custom_tool_input_rejects() {
    let options = AnthropicRequestOptions::new(4096);

    // Non-JSON input
    let mut req1 = make_base_request();
    req1.input.push(ResponseItem::CustomToolCall {
        id: None,
        status: None,
        call_id: "call_1".to_string(),
        name: "my_tool".to_string(),
        namespace: None,
        input: "not valid json".to_string(),
        internal_chat_message_metadata_passthrough: None,
    });
    let err1 = translate_request(&req1, &options, None).unwrap_err();
    assert!(matches!(
        err1,
        AnthropicAdapterError::UnsupportedCustomToolInput(_)
    ));

    // Primitive JSON (number, string, array) rather than object
    for invalid_payload in ["123", "\"scalar string\"", "[1, 2, 3]", "true", "null"] {
        let mut req = make_base_request();
        req.input.push(ResponseItem::CustomToolCall {
            id: None,
            status: None,
            call_id: "call_test".to_string(),
            name: "my_tool".to_string(),
            namespace: None,
            input: invalid_payload.to_string(),
            internal_chat_message_metadata_passthrough: None,
        });
        let err = translate_request(&req, &options, None).unwrap_err();
        assert!(matches!(
            err,
            AnthropicAdapterError::UnsupportedCustomToolInput(_)
        ));
    }
}

// ---------------------------------------------------------------------------
// 13. custom tool valid structured input succeeds
// ---------------------------------------------------------------------------
#[test]
fn test_13_custom_tool_valid_structured_input_succeeds() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::CustomToolCall {
        id: None,
        status: None,
        call_id: "call_valid".to_string(),
        name: "structured_tool".to_string(),
        namespace: None,
        input: "{\"command\": \"build\", \"flags\": [\"--release\"]}".to_string(),
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions::new(4096);
    let res = translate_request(&req, &options, None).unwrap();

    assert_eq!(res.request.messages.len(), 1);
    let asst_msg = &res.request.messages[0];
    assert_eq!(asst_msg.role, "assistant");
    match &asst_msg.content[0] {
        AnthropicContentBlock::ToolUse {
            id, name, input, ..
        } => {
            assert_eq!(id, "call_valid");
            assert_eq!(name, "structured_tool");
            assert_eq!(input["command"], "build");
            assert_eq!(input["flags"], serde_json::json!(["--release"]));
        }
        other => panic!("Expected ToolUse block, found {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 14. custom tool output text succeeds
// ---------------------------------------------------------------------------
#[test]
fn test_14_custom_tool_output_text_succeeds() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::CustomToolCallOutput {
        id: None,
        call_id: "call_ctco_1".to_string(),
        name: Some("test_tool".to_string()),
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text("Command output: build succeeded".to_string()),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions::new(4096);
    let res = translate_request(&req, &options, None).unwrap();

    assert_eq!(res.request.messages.len(), 1);
    let user_msg = &res.request.messages[0];
    assert_eq!(user_msg.role, "user");
    match &user_msg.content[0] {
        AnthropicContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
            ..
        } => {
            assert_eq!(tool_use_id, "call_ctco_1");
            assert_eq!(*is_error, Some(false));
            assert_eq!(
                content,
                &AnthropicToolResultContent::Text("Command output: build succeeded".to_string())
            );
        }
        other => panic!("Expected ToolResult block, found {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 15. custom tool output image succeeds if representable
// ---------------------------------------------------------------------------
#[test]
fn test_15_custom_tool_output_image_succeeds_if_representable() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::CustomToolCallOutput {
        id: None,
        call_id: "call_image_tool".to_string(),
        name: Some("render".to_string()),
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::ContentItems(vec![
                FunctionCallOutputContentItem::InputText {
                    text: "Rendered screenshot:".to_string(),
                },
                FunctionCallOutputContentItem::InputImage {
                    image: ImageReference::Inline {
                        image_url: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==".to_string(),
                    },
                    detail: None,
                },
            ]),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions::new(4096);
    let res = translate_request(&req, &options, None).unwrap();

    let user_msg = &res.request.messages[0];
    match &user_msg.content[0] {
        AnthropicContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
            ..
        } => {
            assert_eq!(tool_use_id, "call_image_tool");
            assert_eq!(*is_error, Some(false));
            match content {
                AnthropicToolResultContent::Blocks(blocks) => {
                    assert_eq!(blocks.len(), 2);
                    assert_eq!(
                        blocks[0],
                        AnthropicToolResultBlock::Text {
                            text: "Rendered screenshot:".to_string()
                        }
                    );
                    match &blocks[1] {
                        AnthropicToolResultBlock::Image { source } => {
                            assert_eq!(source.media_type, "image/png");
                            assert_eq!(source.data, "iVBORw0KGgoAAAANSUhEUg==");
                        }
                        other => panic!("Expected Image block, found {other:?}"),
                    }
                }
                AnthropicToolResultContent::Text(_) => panic!("Expected Blocks content"),
            }
        }
        other => panic!("Expected ToolResult, found {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 16. custom tool output audio rejects
// ---------------------------------------------------------------------------
#[test]
fn test_16_custom_tool_output_audio_rejects() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::CustomToolCallOutput {
        id: None,
        call_id: "call_audio".to_string(),
        name: Some("audio_tool".to_string()),
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::ContentItems(vec![
                FunctionCallOutputContentItem::InputAudio {
                    audio_url: "data:audio/wav;base64,AAA=".to_string(),
                },
            ]),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions::new(4096);
    let err = translate_request(&req, &options, None).unwrap_err();
    assert!(matches!(
        err,
        AnthropicAdapterError::UnsupportedToolOutputContent(_)
    ));
}

// ---------------------------------------------------------------------------
// 17. custom tool output success=false -> is_error=true
// ---------------------------------------------------------------------------
#[test]
fn test_17_custom_tool_output_success_false_maps_to_is_error_true() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::CustomToolCallOutput {
        id: None,
        call_id: "call_fail".to_string(),
        name: Some("failing_tool".to_string()),
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text("Permission denied".to_string()),
            success: Some(false),
        },
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions::new(4096);
    let res = translate_request(&req, &options, None).unwrap();

    match &res.request.messages[0].content[0] {
        AnthropicContentBlock::ToolResult { is_error, .. } => {
            assert_eq!(*is_error, Some(true));
        }
        other => panic!("Expected ToolResult, found {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 18. no silent max_tokens default
// ---------------------------------------------------------------------------
#[test]
fn test_18_no_silent_max_tokens_default() {
    // Explicit constructor sets given max_tokens
    let opt = AnthropicRequestOptions::new(1234);
    assert_eq!(opt.max_tokens, 1234);

    // 0 is rejected fail-closed
    let opt_zero = AnthropicRequestOptions::new(0);
    let req = make_base_request();
    let err = translate_request(&req, &opt_zero, None).unwrap_err();
    assert_eq!(err, AnthropicAdapterError::InvalidMaxTokens(0));
}

// ---------------------------------------------------------------------------
// 19. ProviderDefault thinking omits explicit config
// ---------------------------------------------------------------------------
#[test]
fn test_19_provider_default_thinking_omits_explicit_config() {
    let mut opt = AnthropicRequestOptions::new(4096);
    opt.thinking_policy = AnthropicThinkingPolicy::ProviderDefault;

    let req = make_base_request();
    let res = translate_request(&req, &opt, None).unwrap();

    assert_eq!(res.request.thinking, None);
    let json = serde_json::to_value(&res.request).unwrap();
    assert!(json.get("thinking").is_none());
}

// ---------------------------------------------------------------------------
// 20. Adaptive thinking serialization
// ---------------------------------------------------------------------------
#[test]
fn test_20_adaptive_thinking_serialization() {
    let mut opt = AnthropicRequestOptions::new(4096);
    opt.thinking_policy = AnthropicThinkingPolicy::Adaptive;

    let req = make_base_request();
    let res = translate_request(&req, &opt, None).unwrap();

    assert_eq!(
        res.request.thinking,
        Some(AnthropicThinkingConfig::Adaptive)
    );
    let json = serde_json::to_value(&res.request).unwrap();
    assert_eq!(json["thinking"], serde_json::json!({"type": "adaptive"}));
}

// ---------------------------------------------------------------------------
// 21. legacy budget below minimum rejects if legacy mode retained
// ---------------------------------------------------------------------------
#[test]
fn test_21_legacy_budget_below_minimum_rejects() {
    let mut opt = AnthropicRequestOptions::new(4096);
    opt.thinking_policy = AnthropicThinkingPolicy::LegacyBudgetTokens(1023);

    let req = make_base_request();
    let err = translate_request(&req, &opt, None).unwrap_err();
    assert_eq!(err, AnthropicAdapterError::InvalidThinkingBudget(1023));

    // 1024 (minimum) succeeds
    opt.thinking_policy = AnthropicThinkingPolicy::LegacyBudgetTokens(1024);
    assert!(translate_request(&req, &opt, None).is_ok());
}

// ---------------------------------------------------------------------------
// 22. legacy budget compatibility warning if retained
// ---------------------------------------------------------------------------
#[test]
fn test_22_legacy_budget_compatibility_warning() {
    let mut opt = AnthropicRequestOptions::new(4096);
    opt.thinking_policy = AnthropicThinkingPolicy::LegacyBudgetTokens(2048);

    let req = make_base_request();
    let res = translate_request(&req, &opt, None).unwrap();

    assert!(
        res.warnings
            .contains(&AnthropicAdapterWarning::LegacyThinkingBudgetUsed(2048))
    );
    assert_eq!(
        res.request.thinking,
        Some(AnthropicThinkingConfig::Enabled {
            budget_tokens: 2048
        })
    );
}

// ---------------------------------------------------------------------------
// 23. missing thinking signature rejects
// ---------------------------------------------------------------------------
#[test]
fn test_23_missing_thinking_signature_rejects() {
    let mut translator = AnthropicStreamTranslator::new();
    let sse = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_no_sig","type":"message","role":"assistant","model":"claude-3-7-sonnet-20250219","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":50,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Some thoughts without sig"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

"#;

    let err = translator.feed_sse_chunk(sse).unwrap_err();
    assert_eq!(
        err,
        AnthropicAdapterError::MissingThinkingSignature(
            "anthropic-reasoning-msg_no_sig-0".to_string()
        )
    );
}

// ---------------------------------------------------------------------------
// 24. thinking signature exact roundtrip
// ---------------------------------------------------------------------------
#[test]
fn test_24_thinking_signature_exact_roundtrip() {
    let mut translator = AnthropicStreamTranslator::new();
    let exact_signature = "  opaque_hmac_sig_+/==_#@ \t\n";

    // Escape for JSON SSE event
    let escaped_sig = "  opaque_hmac_sig_+/==_#@ \\t\\n";
    let sse = format!(
        r#"event: message_start
data: {{"type":"message_start","message":{{"id":"msg_sig","type":"message","role":"assistant","model":"claude-3-7-sonnet-20250219","content":[],"stop_reason":null,"stop_sequence":null,"usage":{{"input_tokens":50,"output_tokens":0}}}}}}

event: content_block_start
data: {{"type":"content_block_start","index":0,"content_block":{{"type":"thinking","thinking":""}}}}

event: content_block_delta
data: {{"type":"content_block_delta","index":0,"delta":{{"type":"thinking_delta","thinking":"Internal thoughts"}}}}

event: content_block_delta
data: {{"type":"content_block_delta","index":0,"delta":{{"type":"signature_delta","signature":"{escaped_sig}"}}}}

event: content_block_stop
data: {{"type":"content_block_stop","index":0}}

event: message_delta
data: {{"type":"message_delta","delta":{{"stop_reason":"end_turn"}},"usage":{{"output_tokens":10}}}}

event: message_stop
data: {{"type":"message_stop"}}

"#
    );

    let _events = translator.feed_sse_chunk(&sse).unwrap();
    let cont = translator.continuation_state();

    let reasoning_id = "anthropic-reasoning-msg_sig-0";
    let stored = cont.get_reasoning_block(reasoning_id).unwrap();
    match stored {
        NativeThinkingBlock::Thinking { signature, .. } => {
            assert_eq!(signature, exact_signature);
        }
        _ => panic!("Expected Thinking block"),
    }

    // Replay in request translation
    let mut req = make_base_request();
    req.input.push(ResponseItem::Reasoning {
        id: Some(codex_protocol::ResponseItemId::from_server(
            reasoning_id.to_string(),
        )),
        summary: Vec::new(),
        content: None,
        encrypted_content: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions::new(4096);
    let res = translate_request(&req, &options, Some(cont)).unwrap();
    match &res.request.messages[0].content[0] {
        AnthropicContentBlock::Thinking { signature, .. } => {
            assert_eq!(signature, exact_signature);
        }
        other => panic!("Expected Thinking block, found {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 25. thinking order preserved
// ---------------------------------------------------------------------------
#[test]
fn test_25_thinking_order_preserved() {
    let mut cont = AnthropicContinuationState::new();
    let reasoning_id = "anthropic-reasoning-order_msg-0";
    cont.insert_reasoning_block(
        reasoning_id,
        NativeThinkingBlock::Thinking {
            thinking: "Step 1: think".to_string(),
            signature: "sig123".to_string(),
        },
    );

    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Calculate".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });
    // First thinking
    req.input.push(ResponseItem::Reasoning {
        id: Some(codex_protocol::ResponseItemId::from_server(
            reasoning_id.to_string(),
        )),
        summary: Vec::new(),
        content: None,
        encrypted_content: None,
        internal_chat_message_metadata_passthrough: None,
    });
    // Second tool call
    req.input.push(ResponseItem::FunctionCall {
        id: None,
        name: "calc".to_string(),
        namespace: None,
        arguments: "{\"x\": 1}".to_string(),
        encrypted_function_args: None,
        call_id: "call_calc_1".to_string(),
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions::new(4096);
    let res = translate_request(&req, &options, Some(&cont)).unwrap();

    assert_eq!(res.request.messages.len(), 2);
    let asst_msg = &res.request.messages[1];
    assert_eq!(asst_msg.content.len(), 2);
    // Block 0: Thinking
    assert!(matches!(
        asst_msg.content[0],
        AnthropicContentBlock::Thinking { .. }
    ));
    // Block 1: ToolUse
    assert!(matches!(
        asst_msg.content[1],
        AnthropicContentBlock::ToolUse { .. }
    ));
}

// ---------------------------------------------------------------------------
// 26. cache TTL 5m
// ---------------------------------------------------------------------------
#[test]
fn test_26_cache_ttl_5m() {
    let mut opt = AnthropicRequestOptions::new(4096);
    opt.prompt_cache_policy = AnthropicPromptCachePolicy::ToolsAndSystem {
        ttl: Some(AnthropicCacheTtl::FiveMinutes),
    };

    let mut req = make_base_request();
    req.instructions = "Cached developer instruction".to_string();

    let res = translate_request(&req, &opt, None).unwrap();
    let sys = res.request.system.as_ref().unwrap();
    match &sys[0] {
        agent_studios_protocol_adapters::anthropic::AnthropicSystemBlock::Text {
            cache_control,
            ..
        } => {
            assert_eq!(
                *cache_control,
                Some(AnthropicCacheControl::Ephemeral {
                    ttl: Some(AnthropicCacheTtl::FiveMinutes),
                })
            );
        }
    }

    let json = serde_json::to_value(&res.request).unwrap();
    assert_eq!(
        json["system"][0]["cache_control"],
        serde_json::json!({"type": "ephemeral", "ttl": "5m"})
    );
}

// ---------------------------------------------------------------------------
// 27. cache TTL 1h
// ---------------------------------------------------------------------------
#[test]
fn test_27_cache_ttl_1h() {
    let mut opt = AnthropicRequestOptions::new(4096);
    opt.prompt_cache_policy = AnthropicPromptCachePolicy::LastUserMessage {
        ttl: Some(AnthropicCacheTtl::OneHour),
    };

    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Large cached user context".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let res = translate_request(&req, &opt, None).unwrap();
    match &res.request.messages[0].content[0] {
        AnthropicContentBlock::Text { cache_control, .. } => {
            assert_eq!(
                *cache_control,
                Some(AnthropicCacheControl::Ephemeral {
                    ttl: Some(AnthropicCacheTtl::OneHour),
                })
            );
        }
        other => panic!("Expected Text block, found {other:?}"),
    }

    let json = serde_json::to_value(&res.request).unwrap();
    assert_eq!(
        json["messages"][0]["content"][0]["cache_control"],
        serde_json::json!({"type": "ephemeral", "ttl": "1h"})
    );
}

// ---------------------------------------------------------------------------
// 28. malformed final tool JSON stream rejects
// ---------------------------------------------------------------------------
#[test]
fn test_28_malformed_final_tool_json_stream_rejects() {
    // 1. Incomplete/invalid JSON syntax
    let mut tr1 = AnthropicStreamTranslator::new();
    let sse1 = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_bad_json","type":"message","role":"assistant","model":"claude-3-7-sonnet","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_bad","name":"tool","input":{}}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\": "}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

"#;
    let err1 = tr1.feed_sse_chunk(sse1).unwrap_err();
    assert!(matches!(
        err1,
        AnthropicAdapterError::InvalidToolArguments(_)
    ));

    // 2. Non-object valid JSON (e.g. integer or string)
    let mut tr2 = AnthropicStreamTranslator::new();
    let sse2 = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_scalar_json","type":"message","role":"assistant","model":"claude-3-7-sonnet","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_scalar","name":"tool","input":{}}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"42"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

"#;
    let err2 = tr2.feed_sse_chunk(sse2).unwrap_err();
    assert!(matches!(
        err2,
        AnthropicAdapterError::InvalidToolArguments(_)
    ));
}

// ---------------------------------------------------------------------------
// 29. empty tool-use ID rejects
// ---------------------------------------------------------------------------
#[test]
fn test_29_empty_tool_use_id_rejects() {
    let mut translator = AnthropicStreamTranslator::new();
    let sse = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_empty_id","type":"message","role":"assistant","model":"claude-3-7-sonnet","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"","name":"read_file","input":{}}}

"#;
    let err = translator.feed_sse_chunk(sse).unwrap_err();
    assert_eq!(
        err,
        AnthropicAdapterError::MissingStreamIdentity("ToolUse block has empty id".to_string())
    );
}

// ---------------------------------------------------------------------------
// 30. empty tool-use name rejects
// ---------------------------------------------------------------------------
#[test]
fn test_30_empty_tool_use_name_rejects() {
    let mut translator = AnthropicStreamTranslator::new();
    let sse = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_empty_name","type":"message","role":"assistant","model":"claude-3-7-sonnet","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_123","name":"","input":{}}}

"#;
    let err = translator.feed_sse_chunk(sse).unwrap_err();
    assert_eq!(
        err,
        AnthropicAdapterError::MissingStreamIdentity("ToolUse block has empty name".to_string())
    );
}
