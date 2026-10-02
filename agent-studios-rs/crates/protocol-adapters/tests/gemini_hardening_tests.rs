use std::sync::Arc;

use agent_studios_protocol_adapters::gemini::{
    GeminiAdapter, GeminiAdapterError, GeminiAdapterOptions, GeminiAdapterWarning, GeminiCandidate,
    GeminiContent, GeminiFunctionCall, GeminiFunctionCallingMode, GeminiGenerateContentResponse,
    GeminiPart, GeminiThinkingLevel, GeminiThinkingPolicy, is_valid_gemini_tool_name,
};
use codex_api::{Reasoning, ResponsesApiRequest, ResponsesApiTools};
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::models::{ContentItem, ResponseItem};
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
    assert!(is_valid_gemini_tool_name("."));
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
                }),
                function_response: None,
                thought: None,
                thought_signature: Some(raw_sig.to_string()),
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
