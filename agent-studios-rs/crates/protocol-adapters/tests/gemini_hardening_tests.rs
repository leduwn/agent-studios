use std::sync::Arc;

use agent_studios_protocol_adapters::gemini::{
    GeminiAdapter, GeminiAdapterError, GeminiAdapterOptions, GeminiAdapterWarning, GeminiCandidate,
    GeminiContent, GeminiFunctionCall, GeminiFunctionCallingMode, GeminiGenerateContentResponse,
    GeminiPart, GeminiResponseFormat, GeminiTextFormatConfig, GeminiThinkingLevel,
    GeminiThinkingPolicy, GeminiUsageMetadata, is_valid_gemini_tool_name,
};
use codex_api::{
    Reasoning, ResponseEvent, ResponsesApiRequest, ResponsesApiTools, create_text_param_for_request,
};
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::models::{
    ContentItem, FunctionCallOutputBody, FunctionCallOutputPayload, ResponseItem,
};
use codex_protocol::openai_models::ReasoningEffort;
use serde_json::value::RawValue;

fn make_base_request() -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: "gemini-2.5-flash".to_string(),
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

// ----------------------------------------------------------------------------
// 1. Tool name regex validation
// ----------------------------------------------------------------------------

#[test]
fn test_01_tool_name_regex_boundaries() {
    // 1 char valid
    assert!(is_valid_gemini_tool_name("a"));
    assert!(is_valid_gemini_tool_name("Z"));
    assert!(is_valid_gemini_tool_name("0"));
    assert!(is_valid_gemini_tool_name("_"));
    assert!(is_valid_gemini_tool_name("-"));

    // 128 chars valid
    let max_len_name = "a".repeat(128);
    assert!(is_valid_gemini_tool_name(&max_len_name));

    // 129 chars invalid
    let too_long_name = "a".repeat(129);
    assert!(!is_valid_gemini_tool_name(&too_long_name));

    // Empty invalid
    assert!(!is_valid_gemini_tool_name(""));

    // Disallowed characters
    assert!(!is_valid_gemini_tool_name(".")); // dot removed from callable set
    assert!(!is_valid_gemini_tool_name("my.tool")); // dot
    assert!(!is_valid_gemini_tool_name("my tool")); // space
    assert!(!is_valid_gemini_tool_name("my:tool")); // colon
    assert!(!is_valid_gemini_tool_name("my/tool")); // slash
    assert!(!is_valid_gemini_tool_name("my$tool")); // dollar
}

#[test]
fn test_02_invalid_tool_name_fails_closed_in_translation() {
    let tools_raw = serde_json::json!([
        {
            "type": "function",
            "name": "invalid name with spaces",
            "description": "desc",
            "parameters": { "type": "object" }
        }
    ]);
    let raw = RawValue::from_string(tools_raw.to_string()).unwrap();

    let mut req = make_base_request();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));

    let options = GeminiAdapterOptions::new();
    let err = GeminiAdapter::translate_request(&req, &options, None)
        .expect_err("Should fail on invalid name");
    assert!(matches!(err, GeminiAdapterError::UnsupportedToolType(_)));
}

// ----------------------------------------------------------------------------
// 2. Sequential tool calling rejection
// ----------------------------------------------------------------------------

#[test]
fn test_03_sequential_tool_calling_rejected_when_tools_active() {
    let tools_raw = serde_json::json!([
        {
            "type": "function",
            "name": "calc",
            "description": "Calculator",
            "parameters": { "type": "object" }
        }
    ]);
    let raw = RawValue::from_string(tools_raw.to_string()).unwrap();

    let mut req = make_base_request();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));
    req.parallel_tool_calls = false; // Sequential tool calling requested!

    let options = GeminiAdapterOptions::new();
    let err = GeminiAdapter::translate_request(&req, &options, None)
        .expect_err("Should reject sequential tool calling");
    assert_eq!(err, GeminiAdapterError::SequentialToolCallingNotEnforceable);
}

#[test]
fn test_04_sequential_tool_calling_allowed_when_no_tools() {
    let mut req = make_base_request();
    req.parallel_tool_calls = false;
    req.tools = None;

    let options = GeminiAdapterOptions::new();
    let res = GeminiAdapter::translate_request(&req, &options, None);
    assert!(res.is_ok());
}

// ----------------------------------------------------------------------------
// 3. System instruction interleaving rejection
// ----------------------------------------------------------------------------

#[test]
fn test_05_interleaved_system_instruction_fails_closed() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Hello".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });
    // System message appears AFTER user turn
    req.input.push(ResponseItem::Message {
        id: None,
        role: "system".to_string(),
        content: vec![ContentItem::InputText {
            text: "You are now in debug mode".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new();
    let err = GeminiAdapter::translate_request(&req, &options, None)
        .expect_err("Should reject interleaved system turn");
    assert!(matches!(
        err,
        GeminiAdapterError::UnsupportedSystemHistoryPlacement(_)
    ));
}

// ----------------------------------------------------------------------------
// 4. Max output tokens validation
// ----------------------------------------------------------------------------

#[test]
fn test_06_max_output_tokens_zero_rejected() {
    let mut options = GeminiAdapterOptions::new();
    options.max_output_tokens = Some(0);

    let req = make_base_request();
    let err = GeminiAdapter::translate_request(&req, &options, None)
        .expect_err("Should reject 0 max output tokens");
    assert_eq!(err, GeminiAdapterError::InvalidMaxOutputTokens(0));
}

#[test]
fn test_07_max_output_tokens_positive_translated() {
    let mut options = GeminiAdapterOptions::new();
    options.max_output_tokens = Some(4096);

    let req = make_base_request();
    let trans = GeminiAdapter::translate_request(&req, &options, None).unwrap();
    let generation_cfg = trans.request.generation_config.unwrap();
    assert_eq!(generation_cfg.max_output_tokens, Some(4096));
}

// ----------------------------------------------------------------------------
// 5. Strict tool promotion and warning
// ----------------------------------------------------------------------------

#[test]
fn test_08_mixed_strict_tools_promoted_with_warning() {
    let tools_raw = serde_json::json!([
        {
            "type": "function",
            "name": "strict_tool",
            "description": "Strict",
            "strict": true,
            "parameters": { "type": "object" }
        },
        {
            "type": "function",
            "name": "flexible_tool",
            "description": "Flexible",
            "strict": false,
            "parameters": { "type": "object" }
        }
    ]);
    let raw = RawValue::from_string(tools_raw.to_string()).unwrap();

    let mut req = make_base_request();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));
    req.tool_choice = "auto".to_string();

    let options = GeminiAdapterOptions::new();
    let trans = GeminiAdapter::translate_request(&req, &options, None).unwrap();

    // Mode is VALIDATED
    let mode = trans
        .request
        .tool_config
        .unwrap()
        .function_calling_config
        .unwrap()
        .mode;
    assert_eq!(mode, Some(GeminiFunctionCallingMode::Validated));

    // Warning emitted
    assert_eq!(
        trans.warnings,
        vec![GeminiAdapterWarning::StrictToolScopePromoted]
    );
}

// ----------------------------------------------------------------------------
// 6. Reasoning effort mappings and fail-closed rejections
// ----------------------------------------------------------------------------

#[test]
fn test_09_exact_reasoning_effort_mapping() {
    let mut options = GeminiAdapterOptions::new();
    options.thinking_policy = GeminiThinkingPolicy::ExactReasoningEffort;

    // High -> HIGH
    let mut req = make_base_request();
    req.reasoning = Some(Reasoning {
        effort: Some(ReasoningEffort::High),
        summary: Some(ReasoningSummary::Detailed),
        context: None,
    });
    let trans = GeminiAdapter::translate_request(&req, &options, None).unwrap();
    let think = trans
        .request
        .generation_config
        .unwrap()
        .thinking_config
        .unwrap();
    assert_eq!(think.thinking_level, Some(GeminiThinkingLevel::High));

    // Medium -> MEDIUM
    req.reasoning = Some(Reasoning {
        effort: Some(ReasoningEffort::Medium),
        summary: None,
        context: None,
    });
    let trans = GeminiAdapter::translate_request(&req, &options, None).unwrap();
    let think = trans
        .request
        .generation_config
        .unwrap()
        .thinking_config
        .unwrap();
    assert_eq!(think.thinking_level, Some(GeminiThinkingLevel::Medium));

    // Low -> LOW
    req.reasoning = Some(Reasoning {
        effort: Some(ReasoningEffort::Low),
        summary: None,
        context: None,
    });
    let trans = GeminiAdapter::translate_request(&req, &options, None).unwrap();
    let think = trans
        .request
        .generation_config
        .unwrap()
        .thinking_config
        .unwrap();
    assert_eq!(think.thinking_level, Some(GeminiThinkingLevel::Low));

    // Minimal -> MINIMAL
    req.reasoning = Some(Reasoning {
        effort: Some(ReasoningEffort::Minimal),
        summary: None,
        context: None,
    });
    let trans = GeminiAdapter::translate_request(&req, &options, None).unwrap();
    let think = trans
        .request
        .generation_config
        .unwrap()
        .thinking_config
        .unwrap();
    assert_eq!(think.thinking_level, Some(GeminiThinkingLevel::Minimal));
}

#[test]
fn test_10_unsupported_reasoning_effort_fails_closed() {
    let mut options = GeminiAdapterOptions::new();
    options.thinking_policy = GeminiThinkingPolicy::ExactReasoningEffort;

    let mut req = make_base_request();
    req.reasoning = Some(Reasoning {
        effort: Some(ReasoningEffort::None),
        summary: None,
        context: None,
    });

    let err = GeminiAdapter::translate_request(&req, &options, None)
        .expect_err("Should fail closed on ReasoningEffort::None");
    assert_eq!(
        err,
        GeminiAdapterError::UnsupportedReasoningEffort("none".to_string())
    );
}

// ----------------------------------------------------------------------------
// 7. Stream finish reason fail-closed checks
// ----------------------------------------------------------------------------

#[test]
fn test_11_finish_reason_recitation_fails_closed() {
    let mut translator = GeminiAdapter::new_stream_translator();
    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-recite".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: None,
            finish_reason: Some("RECITATION".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let err = translator
        .feed_response(&resp)
        .expect_err("Should fail on recitation");
    assert!(matches!(err, GeminiAdapterError::RecitationBlocked(_)));
}

#[test]
fn test_12_finish_reason_language_fails_closed() {
    let mut translator = GeminiAdapter::new_stream_translator();
    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-lang".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: None,
            finish_reason: Some("LANGUAGE".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let err = translator
        .feed_response(&resp)
        .expect_err("Should fail on unsupported language");
    assert!(matches!(err, GeminiAdapterError::UnsupportedLanguage(_)));
}

#[test]
fn test_13_finish_reason_blocklist_fails_closed() {
    let mut translator = GeminiAdapter::new_stream_translator();
    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-block".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: None,
            finish_reason: Some("BLOCKLIST".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let err = translator
        .feed_response(&resp)
        .expect_err("Should fail on blocklist");
    assert_eq!(
        err,
        GeminiAdapterError::ContentBlocked("BLOCKLIST".to_string())
    );
}

#[test]
fn test_14_finish_reason_spii_fails_closed() {
    let mut translator = GeminiAdapter::new_stream_translator();
    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-spii".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: None,
            finish_reason: Some("SPII".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let err = translator
        .feed_response(&resp)
        .expect_err("Should fail on SPII");
    assert_eq!(err, GeminiAdapterError::ContentBlocked("SPII".to_string()));
}

// ----------------------------------------------------------------------------
// 8. Model version mismatch mid-stream
// ----------------------------------------------------------------------------

#[test]
fn test_15_model_version_mismatch_fails_closed() {
    let mut translator = GeminiAdapter::new_stream_translator();

    let resp1 = GeminiGenerateContentResponse {
        response_id: Some("resp-mv-1".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart::text("A")])),
            finish_reason: None,
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };
    translator.feed_response(&resp1).unwrap();

    let resp2 = GeminiGenerateContentResponse {
        response_id: Some("resp-mv-1".to_string()),
        model_version: Some("gemini-2.5-pro".to_string()), // Switched version mid-stream!
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart::text("B")])),
            finish_reason: None,
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let err = translator
        .feed_response(&resp2)
        .expect_err("Should detect model version change");
    assert_eq!(
        err,
        GeminiAdapterError::ModelVersionMismatch {
            expected: "gemini-2.5-flash".to_string(),
            actual: "gemini-2.5-pro".to_string(),
        }
    );
}

// ----------------------------------------------------------------------------
// 9. Thought signature byte-level preservation
// ----------------------------------------------------------------------------

#[test]
fn test_16_thought_signature_verbatim_preservation() {
    let mut translator = GeminiAdapter::new_stream_translator();

    // Raw signature containing unusual bytes, base64 padding, symbols
    let raw_sig = "==Cryptographic_Signature/2026+Gemini@Google!#$==\n\t";

    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-sig-exact".to_string()),
        model_version: Some("gemini-2.5-pro".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart {
                text: None,
                inline_data: None,
                function_call: Some(GeminiFunctionCall {
                    id: Some("call-sig-1".to_string()),
                    name: "test_call".to_string(),
                    args: serde_json::json!({}),
                    ..Default::default()
                }),
                function_response: None,
                thought: None,
                thought_signature: Some(raw_sig.to_string()),
                ..Default::default()
            }])),
            finish_reason: Some("STOP".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    translator.feed_response(&resp).unwrap();
    translator.finish_stream().unwrap();

    let cont = translator.continuation_state();
    // Must be exact byte-for-byte match, no trimming or stripping
    assert_eq!(cont.get_signature("call-sig-1"), Some(raw_sig));
}

// ----------------------------------------------------------------------------
// 10. M07.1 Native Turn Continuation, Signed Parts, Usage Math, Finish Reasons
// ----------------------------------------------------------------------------

#[test]
fn test_17_exact_provider_native_roundtrip_turn_replay() {
    let mut translator = GeminiAdapter::new_stream_translator();

    let mut fc2 = GeminiFunctionCall {
        id: Some("call-2".to_string()),
        name: "check_status".to_string(),
        args: serde_json::json!({}),
        ..Default::default()
    };
    fc2.extra
        .insert("customField".to_string(), serde_json::json!("customVal"));

    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-multi-part-1".to_string()),
        model_version: Some("gemini-2.5-pro".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![
                GeminiPart {
                    text: Some("Analyzing input...".to_string()),
                    thought: Some(true),
                    thought_signature: Some("sig-thought-42".to_string()),
                    ..Default::default()
                },
                GeminiPart {
                    text: Some("I will check files and status.".to_string()),
                    thought_signature: Some("sig-text-42".to_string()),
                    ..Default::default()
                },
                GeminiPart {
                    function_call: Some(GeminiFunctionCall {
                        id: Some("call-1".to_string()),
                        name: "check_files".to_string(),
                        args: serde_json::json!({ "dir": "src" }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                GeminiPart {
                    function_call: Some(fc2),
                    thought_signature: Some("sig-call-2-42".to_string()),
                    ..Default::default()
                },
            ])),
            finish_reason: Some("STOP".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let events = translator.feed_response(&resp).unwrap();
    let finish_events = translator.finish_stream().unwrap();

    let mut model_items = Vec::new();
    for ev in events.into_iter().chain(finish_events) {
        if let ResponseEvent::OutputItemDone(item) = ev {
            model_items.push(item);
        }
    }
    // Should have 4 items: Reasoning, Message, FunctionCall 1, FunctionCall 2
    assert_eq!(model_items.len(), 4);

    let cont = translator.continuation_state();

    // Now turn 2: User responds to the two tool calls
    let mut req2 = make_base_request();
    // Include user turn 0
    req2.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Hello".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });
    // Include all 4 model items from turn 1
    req2.input.extend(model_items);
    // Include tool outputs for call-1 and call-2
    req2.input.push(ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("call-1".to_string()),
        name: Some("check_files".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text("main.rs".to_string()),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    });
    req2.input.push(ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("call-2".to_string()),
        name: Some("check_status".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text("ok".to_string()),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new();
    let trans = GeminiAdapter::translate_request(&req2, &options, Some(cont))
        .expect("Replay with native turn continuation must succeed");

    // Must have 3 contents: user turn 0, single replayed model turn, user tool response turn
    assert_eq!(trans.request.contents.len(), 3);
    assert_eq!(trans.request.contents[0].role.as_deref(), Some("user"));
    assert_eq!(trans.request.contents[1].role.as_deref(), Some("model"));
    assert_eq!(trans.request.contents[2].role.as_deref(), Some("user"));

    // Verify native model turn is replayed verbatim
    let replayed_model_parts = &trans.request.contents[1].parts;
    assert_eq!(replayed_model_parts.len(), 4);
    assert_eq!(
        replayed_model_parts[0].text.as_deref(),
        Some("Analyzing input...")
    );
    assert_eq!(replayed_model_parts[0].thought, Some(true));
    assert_eq!(
        replayed_model_parts[0].thought_signature.as_deref(),
        Some("sig-thought-42")
    );

    assert_eq!(
        replayed_model_parts[1].text.as_deref(),
        Some("I will check files and status.")
    );
    assert_eq!(
        replayed_model_parts[1].thought_signature.as_deref(),
        Some("sig-text-42")
    );

    let fc1 = replayed_model_parts[2].function_call.as_ref().unwrap();
    assert_eq!(fc1.name, "check_files");
    assert_eq!(fc1.id.as_deref(), Some("call-1"));

    let fc2_replayed = replayed_model_parts[3].function_call.as_ref().unwrap();
    assert_eq!(fc2_replayed.name, "check_status");
    assert_eq!(
        fc2_replayed.extra.get("customField"),
        Some(&serde_json::json!("customVal"))
    );
    assert_eq!(
        replayed_model_parts[3].thought_signature.as_deref(),
        Some("sig-call-2-42")
    );
}

#[test]
fn test_18_signed_final_text_roundtrip() {
    let mut translator = GeminiAdapter::new_stream_translator();

    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-signed-text-1".to_string()),
        model_version: Some("gemini-2.5-pro".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart {
                text: Some("Final verified answer.".to_string()),
                thought_signature: Some("sig-answer-999".to_string()),
                ..Default::default()
            }])),
            finish_reason: Some("STOP".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    translator.feed_response(&resp).unwrap();
    let events = translator.finish_stream().unwrap();

    let mut msg_item = None;
    for ev in events {
        if let ResponseEvent::OutputItemDone(ResponseItem::Message {
            id, role, content, ..
        }) = ev
        {
            msg_item = Some(ResponseItem::Message {
                id,
                role,
                content,
                phase: None,
                internal_chat_message_metadata_passthrough: None,
            });
        }
    }
    let msg_item = msg_item.expect("Emitted assistant message");

    let cont = translator.continuation_state();

    let mut req2 = make_base_request();
    req2.input.push(msg_item);
    req2.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Follow up question".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new();
    let trans = GeminiAdapter::translate_request(&req2, &options, Some(cont))
        .expect("Translation must succeed");

    assert_eq!(trans.request.contents.len(), 2);
    assert_eq!(trans.request.contents[0].role.as_deref(), Some("model"));
    assert_eq!(
        trans.request.contents[0].parts[0].text.as_deref(),
        Some("Final verified answer.")
    );
    assert_eq!(
        trans.request.contents[0].parts[0]
            .thought_signature
            .as_deref(),
        Some("sig-answer-999")
    );
}

#[test]
fn test_19_native_turn_without_visible_output() {
    let mut translator = GeminiAdapter::new_stream_translator();

    // Candidate has only empty signed part or whitespace thought
    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-empty-signed-1".to_string()),
        model_version: Some("gemini-2.5-pro".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart {
                text: None,
                thought_signature: Some("sig-empty-part-007".to_string()),
                ..Default::default()
            }])),
            finish_reason: Some("STOP".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    translator.feed_response(&resp).unwrap();
    translator.finish_stream().unwrap();

    let cont = translator.continuation_state();
    let native_turn = cont
        .get_model_turn("resp-empty-signed-1")
        .expect("Model turn must be stored in continuation");
    assert_eq!(native_turn.parts.len(), 1);
    assert_eq!(
        native_turn.parts[0].thought_signature.as_deref(),
        Some("sig-empty-part-007")
    );
}

#[test]
fn test_20_usage_accounting_with_thoughts() {
    // 1. Explicit total token count
    let mut translator1 = GeminiAdapter::new_stream_translator();
    let resp1 = GeminiGenerateContentResponse {
        response_id: Some("resp-usage-1".to_string()),
        model_version: Some("gemini-2.5-pro".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart::text("Hi")])),
            finish_reason: Some("STOP".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: Some(GeminiUsageMetadata {
            prompt_token_count: Some(100),
            cached_content_token_count: Some(40),
            candidates_token_count: Some(20),
            thoughts_token_count: Some(30),
            tool_use_prompt_token_count: None,
            total_token_count: Some(150),
        }),
    };

    translator1.feed_response(&resp1).unwrap();
    let events1 = translator1.finish_stream().unwrap();
    let completed1 = events1
        .into_iter()
        .find_map(|ev| {
            if let ResponseEvent::Completed {
                token_usage,
                usage_metadata,
                ..
            } = ev
            {
                Some((token_usage.unwrap(), usage_metadata.unwrap()))
            } else {
                None
            }
        })
        .expect("Completed event emitted");

    let (usage1, raw_meta1) = completed1;
    assert_eq!(usage1.input_tokens, 100);
    assert_eq!(usage1.cached_input_tokens, 40);
    assert_eq!(usage1.output_tokens, 20);
    assert_eq!(usage1.reasoning_output_tokens, 30);
    assert_eq!(usage1.total_tokens, 150);
    assert!(raw_meta1.metadata.is_some());

    // 2. Fallback total calculation when total_token_count is omitted
    let mut translator2 = GeminiAdapter::new_stream_translator();
    let resp2 = GeminiGenerateContentResponse {
        response_id: Some("resp-usage-2".to_string()),
        model_version: Some("gemini-2.5-pro".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart::text("Hi")])),
            finish_reason: Some("STOP".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: Some(GeminiUsageMetadata {
            prompt_token_count: Some(100),
            cached_content_token_count: Some(40),
            candidates_token_count: Some(20),
            thoughts_token_count: Some(30),
            tool_use_prompt_token_count: None,
            total_token_count: None, // Missing
        }),
    };

    translator2.feed_response(&resp2).unwrap();
    let events2 = translator2.finish_stream().unwrap();
    let usage2 = events2
        .into_iter()
        .find_map(|ev| {
            if let ResponseEvent::Completed { token_usage, .. } = ev {
                token_usage
            } else {
                None
            }
        })
        .expect("Completed event emitted");

    // 100 + 20 + 30 = 150
    assert_eq!(usage2.total_tokens, 150);
}

#[test]
fn test_21_structured_outputs_exact_json_format() {
    let mut req = make_base_request();
    let schema = serde_json::json!({
        "type": "object",
        "properties": { "name": { "type": "string" } },
        "required": ["name"]
    });
    req.text = create_text_param_for_request(None, &Some(schema.clone()), false);

    let options = GeminiAdapterOptions::new();
    let trans = GeminiAdapter::translate_request(&req, &options, None).unwrap();
    let gen_config = trans.request.generation_config.unwrap();

    assert_eq!(
        gen_config.response_format,
        Some(GeminiResponseFormat {
            text: Some(GeminiTextFormatConfig {
                mime_type: "application/json".to_string(),
                schema: Some(schema.clone()),
            }),
        })
    );

    let serialized = serde_json::to_value(&gen_config).unwrap();
    // Verify deprecated responseSchema does not exist
    assert!(serialized.get("responseSchema").is_none());
    // Verify modern responseFormat.text format
    assert_eq!(
        serialized["responseFormat"]["text"]["mimeType"],
        "application/json"
    );
    assert_eq!(
        serialized["responseFormat"]["text"]["schema"]["properties"]["name"]["type"],
        "string"
    );
}

#[test]
fn test_22_new_finish_reasons_coverage() {
    let cases = vec![
        (
            "MISSING_THOUGHT_SIGNATURE",
            GeminiAdapterError::MissingThoughtSignature("MISSING_THOUGHT_SIGNATURE".to_string()),
        ),
        (
            "UNEXPECTED_TOOL_CALL",
            GeminiAdapterError::UnexpectedToolCall("UNEXPECTED_TOOL_CALL".to_string()),
        ),
        (
            "TOO_MANY_TOOL_CALLS",
            GeminiAdapterError::TooManyToolCalls("TOO_MANY_TOOL_CALLS".to_string()),
        ),
        (
            "MALFORMED_RESPONSE",
            GeminiAdapterError::MalformedResponse("MALFORMED_RESPONSE".to_string()),
        ),
        (
            "IMAGE_SAFETY",
            GeminiAdapterError::ImageGenerationBlocked("IMAGE_SAFETY".to_string()),
        ),
        (
            "IMAGE_GENERATION_BLOCKED",
            GeminiAdapterError::ImageGenerationBlocked("IMAGE_GENERATION_BLOCKED".to_string()),
        ),
        (
            "FINISH_REASON_UNSPECIFIED",
            GeminiAdapterError::FinishReasonUnspecified,
        ),
        (
            "MALFORMED_FUNCTION_CALL",
            GeminiAdapterError::MalformedFunctionCall("MALFORMED_FUNCTION_CALL".to_string()),
        ),
    ];

    for (reason_str, expected_err) in cases {
        let mut translator = GeminiAdapter::new_stream_translator();
        let resp = GeminiGenerateContentResponse {
            response_id: Some("resp-fr".to_string()),
            model_version: Some("gemini-2.5-flash".to_string()),
            candidates: Some(vec![GeminiCandidate {
                index: Some(0),
                content: Some(GeminiContent::model(vec![GeminiPart::text("text")])),
                finish_reason: Some(reason_str.to_string()),
                safety_ratings: None,
            }]),
            prompt_feedback: None,
            usage_metadata: None,
        };

        let err = translator
            .feed_response(&resp)
            .expect_err(&format!("Should fail for finish reason {reason_str}"));
        assert_eq!(err, expected_err);
    }
}
