use agent_studios_protocol_adapters::gemini::{
    GeminiAdapterError, GeminiCandidate, GeminiContent, GeminiFunctionCall,
    GeminiGenerateContentResponse, GeminiPart, GeminiPromptFeedback, GeminiSafetyRating,
    GeminiStreamTranslator, GeminiUsageMetadata,
};
use codex_api::ResponseEvent;
use codex_protocol::models::{ContentItem, ReasoningItemContent, ResponseItem};

#[test]
fn test_01_text_streaming_happy_path() {
    let mut translator = GeminiStreamTranslator::new();

    // Chunk 1: message starts, model version, first text delta
    let chunk1 = serde_json::json!({
        "responseId": "gemini-resp-001",
        "modelVersion": "gemini-2.5-pro",
        "candidates": [{
            "index": 0,
            "content": {
                "role": "model",
                "parts": [{ "text": "Hello! " }]
            }
        }]
    });
    let evs1 = translator
        .feed_response(&serde_json::from_value(chunk1).unwrap())
        .expect("Chunk 1 failed");

    assert_eq!(evs1.len(), 4);
    assert!(matches!(&evs1[0], ResponseEvent::Created { .. }));
    assert!(matches!(&evs1[1], ResponseEvent::ServerModel(m) if m == "gemini-2.5-pro"));
    assert!(matches!(
        &evs1[2],
        ResponseEvent::OutputItemAdded(ResponseItem::Message { .. })
    ));
    assert!(matches!(&evs1[3], ResponseEvent::OutputTextDelta(t) if t == "Hello! "));

    // Chunk 2: second text delta
    let chunk2 = serde_json::json!({
        "responseId": "gemini-resp-001",
        "candidates": [{
            "index": 0,
            "content": {
                "role": "model",
                "parts": [{ "text": "How can I help you today?" }]
            }
        }]
    });
    let evs2 = translator
        .feed_response(&serde_json::from_value(chunk2).unwrap())
        .expect("Chunk 2 failed");

    assert_eq!(evs2.len(), 1);
    assert!(
        matches!(&evs2[0], ResponseEvent::OutputTextDelta(t) if t == "How can I help you today?")
    );

    // Chunk 3: finishReason STOP, usage metadata
    let chunk3 = serde_json::json!({
        "responseId": "gemini-resp-001",
        "candidates": [{
            "index": 0,
            "finishReason": "STOP"
        }],
        "usageMetadata": {
            "promptTokenCount": 25,
            "candidatesTokenCount": 10,
            "totalTokenCount": 35
        }
    });
    let evs3 = translator
        .feed_response(&serde_json::from_value(chunk3).unwrap())
        .expect("Chunk 3 failed");
    assert!(evs3.is_empty());

    // Finalize stream
    let evs_done = translator.finish_stream().expect("Finalize failed");
    assert_eq!(evs_done.len(), 2);
    match &evs_done[0] {
        ResponseEvent::OutputItemDone(ResponseItem::Message { content, .. }) => match &content[0] {
            ContentItem::OutputText { text } => {
                assert_eq!(text, "Hello! How can I help you today?");
            }
            _ => panic!("Expected OutputText"),
        },
        _ => panic!("Expected OutputItemDone Message"),
    }
    match &evs_done[1] {
        ResponseEvent::Completed {
            response_id,
            token_usage,
            end_turn,
            ..
        } => {
            assert_eq!(response_id, "gemini-resp-001");
            assert_eq!(*end_turn, Some(true));
            let usage = token_usage.as_ref().expect("usage present");
            assert_eq!(usage.input_tokens, 25);
            assert_eq!(usage.cached_input_tokens, 0);
            assert_eq!(usage.output_tokens, 10);
            assert_eq!(usage.total_tokens, 35);
        }
        _ => panic!("Expected Completed event"),
    }
    assert!(translator.is_completed());
}

#[test]
fn test_02_effective_prompt_token_accounting_with_cache() {
    let mut translator = GeminiStreamTranslator::new();

    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-cache-1".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart::text("OK")])),
            finish_reason: Some("STOP".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: Some(GeminiUsageMetadata {
            // In Gemini: prompt_token_count ALREADY includes cached_content_token_count
            prompt_token_count: Some(500),
            cached_content_token_count: Some(350),
            candidates_token_count: Some(40),
            thoughts_token_count: None,
            tool_use_prompt_token_count: None,
            total_token_count: Some(540),
        }),
    };

    translator.feed_response(&resp).unwrap();
    let evs = translator.finish_stream().unwrap();

    let completed = evs
        .iter()
        .find_map(|e| match e {
            ResponseEvent::Completed { token_usage, .. } => token_usage.as_ref(),
            _ => None,
        })
        .expect("Completed event with usage");

    // Effective input is prompt_token_count (500), cached breakdown is 350.
    // Cache is NOT added again! Total is 540.
    assert_eq!(completed.input_tokens, 500);
    assert_eq!(completed.cached_input_tokens, 350);
    assert_eq!(completed.output_tokens, 40);
    assert_eq!(completed.total_tokens, 540);
}

#[test]
fn test_03_tool_call_streaming_and_continuation_recording() {
    let mut translator = GeminiStreamTranslator::new();

    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-tool-1".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart {
                text: None,
                inline_data: None,
                function_call: Some(GeminiFunctionCall {
                    id: Some("provider-call-99".to_string()),
                    name: "get_weather".to_string(),
                    args: serde_json::json!({ "location": "Tokyo" }),
                }),
                function_response: None,
                thought: None,
                thought_signature: Some("sig-weather-call".to_string()),
            }])),
            finish_reason: Some("STOP".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: Some(GeminiUsageMetadata {
            prompt_token_count: Some(10),
            cached_content_token_count: None,
            candidates_token_count: Some(5),
            thoughts_token_count: None,
            tool_use_prompt_token_count: None,
            total_token_count: Some(15),
        }),
    };

    let events = translator.feed_response(&resp).unwrap();

    // Verify FunctionCall events
    assert_eq!(events.len(), 5); // Created, ServerModel, OutputItemAdded, ToolCallInputDelta, OutputItemDone
    assert!(
        matches!(&events[2], ResponseEvent::OutputItemAdded(ResponseItem::FunctionCall { name, call_id, .. }) if name == "get_weather" && call_id == "provider-call-99")
    );
    assert!(
        matches!(&events[3], ResponseEvent::ToolCallInputDelta { call_id: Some(cid), delta, .. } if cid == "provider-call-99" && delta.contains("Tokyo"))
    );
    assert!(
        matches!(&events[4], ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { name, call_id, arguments, .. }) if name == "get_weather" && call_id == "provider-call-99" && arguments.contains("Tokyo"))
    );

    // When tool calls emitted, finish_stream must produce end_turn: false
    let finish_evs = translator.finish_stream().unwrap();
    let completed = finish_evs.iter().find_map(|e| match e {
        ResponseEvent::Completed { end_turn, .. } => *end_turn,
        _ => None,
    });
    assert_eq!(completed, Some(false));

    // Verify continuation state has tool call and thought signature
    let cont = translator.continuation_state();
    let meta = cont
        .get_tool_call("provider-call-99")
        .expect("tool call recorded");
    assert_eq!(meta.name, "get_weather");
    assert_eq!(meta.provider_call_id.as_deref(), Some("provider-call-99"));
    assert_eq!(
        cont.get_signature("provider-call-99"),
        Some("sig-weather-call")
    );
}

#[test]
fn test_04_deterministic_internal_call_id_when_provider_omits_id() {
    let mut translator = GeminiStreamTranslator::new();

    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-noid".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart {
                text: None,
                inline_data: None,
                function_call: Some(GeminiFunctionCall {
                    id: None, // Provider omitted ID
                    name: "execute_cmd".to_string(),
                    args: serde_json::json!({ "cmd": "ls" }),
                }),
                function_response: None,
                thought: None,
                thought_signature: None,
            }])),
            finish_reason: Some("STOP".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let events = translator.feed_response(&resp).unwrap();
    let generated_id = "gemini-call-resp-noid-0-0";

    assert!(
        matches!(&events[2], ResponseEvent::OutputItemAdded(ResponseItem::FunctionCall { call_id, .. }) if call_id == generated_id)
    );

    let cont = translator.continuation_state();
    let meta = cont
        .get_tool_call(generated_id)
        .expect("deterministic tool call recorded");
    assert_eq!(meta.name, "execute_cmd");
    assert_eq!(meta.provider_call_id, None);
}

#[test]
fn test_05_thought_reasoning_stream_with_signature() {
    let mut translator = GeminiStreamTranslator::new();

    // Chunk with reasoning thought
    let resp1 = GeminiGenerateContentResponse {
        response_id: Some("resp-thought-1".to_string()),
        model_version: Some("gemini-2.5-pro".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart {
                text: Some("Let's analyze the codebase structure.".to_string()),
                inline_data: None,
                function_call: None,
                function_response: None,
                thought: Some(true),
                thought_signature: Some("opaque-crypto-sig-456".to_string()),
            }])),
            finish_reason: None,
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let evs1 = translator.feed_response(&resp1).unwrap();
    assert!(matches!(
        &evs1[2],
        ResponseEvent::OutputItemAdded(ResponseItem::Reasoning { .. })
    ));
    assert!(
        matches!(&evs1[3], ResponseEvent::ReasoningContentDelta { delta, .. } if delta.contains("analyze the codebase"))
    );

    // Chunk switching to model text answer closes reasoning
    let resp2 = GeminiGenerateContentResponse {
        response_id: Some("resp-thought-1".to_string()),
        model_version: Some("gemini-2.5-pro".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart::text(
                "Analysis complete.",
            )])),
            finish_reason: Some("STOP".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let evs2 = translator.feed_response(&resp2).unwrap();
    // Closes reasoning with OutputItemDone
    assert!(
        matches!(&evs2[0], ResponseEvent::OutputItemDone(ResponseItem::Reasoning { content: Some(c), .. }) if matches!(&c[0], ReasoningItemContent::ReasoningText { text } if text.contains("analyze")))
    );
    // Opens text item
    assert!(matches!(
        &evs2[1],
        ResponseEvent::OutputItemAdded(ResponseItem::Message { .. })
    ));
    assert!(matches!(&evs2[2], ResponseEvent::OutputTextDelta(t) if t == "Analysis complete."));

    translator.finish_stream().unwrap();

    // Verify signature was recorded for reasoning ID
    let cont = translator.continuation_state();
    assert_eq!(
        cont.get_signature("gemini-reasoning-resp-thought-1-0"),
        Some("opaque-crypto-sig-456")
    );
}

#[test]
fn test_06_prompt_blocked_fails_closed() {
    let mut translator = GeminiStreamTranslator::new();

    let resp = GeminiGenerateContentResponse {
        response_id: None,
        model_version: None,
        candidates: None,
        prompt_feedback: Some(GeminiPromptFeedback {
            block_reason: Some("SAFETY".to_string()),
            safety_ratings: None,
        }),
        usage_metadata: None,
    };

    let err = translator
        .feed_response(&resp)
        .expect_err("Should fail closed");
    assert_eq!(err, GeminiAdapterError::PromptBlocked("SAFETY".to_string()));
}

#[test]
fn test_07_finish_reason_max_tokens_fails_closed() {
    let mut translator = GeminiStreamTranslator::new();

    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-max".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart::text("Partial")])),
            finish_reason: Some("MAX_TOKENS".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let err = translator
        .feed_response(&resp)
        .expect_err("Should fail on max tokens");
    assert_eq!(err, GeminiAdapterError::MaxTokensExceeded);
}

#[test]
fn test_08_finish_reason_safety_fails_closed() {
    let mut translator = GeminiStreamTranslator::new();

    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-safe".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: None,
            finish_reason: Some("SAFETY".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let err = translator
        .feed_response(&resp)
        .expect_err("Should fail on safety");
    assert!(matches!(err, GeminiAdapterError::SafetyBlocked(_)));
}

#[test]
fn test_09_multiple_candidates_rejected_fail_closed() {
    let mut translator = GeminiStreamTranslator::new();

    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-multi".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![
            GeminiCandidate {
                index: Some(0),
                content: Some(GeminiContent::model(vec![GeminiPart::text("Cand 0")])),
                finish_reason: None,
                safety_ratings: None,
            },
            GeminiCandidate {
                index: Some(1),
                content: Some(GeminiContent::model(vec![GeminiPart::text("Cand 1")])),
                finish_reason: None,
                safety_ratings: None,
            },
        ]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let err = translator
        .feed_response(&resp)
        .expect_err("Should reject multiple candidates");
    assert_eq!(err, GeminiAdapterError::MultipleCandidatesUnsupported(2));
}

#[test]
fn test_10_response_id_mismatch_fails_closed() {
    let mut translator = GeminiStreamTranslator::new();

    let resp1 = GeminiGenerateContentResponse {
        response_id: Some("resp-A".to_string()),
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
        response_id: Some("resp-B".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
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
        .expect_err("Should detect ID mismatch");
    assert_eq!(
        err,
        GeminiAdapterError::ResponseIdMismatch {
            expected: "resp-A".to_string(),
            actual: "resp-B".to_string(),
        }
    );
}

#[test]
fn test_11_candidate_safety_rating_blocked_fails_closed() {
    let mut translator = GeminiStreamTranslator::new();

    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-rating".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: None,
            finish_reason: None,
            safety_ratings: Some(vec![GeminiSafetyRating {
                category: Some("HARM_CATEGORY_DANGEROUS_CONTENT".to_string()),
                probability: Some("HIGH".to_string()),
                blocked: Some(true),
            }]),
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let err = translator
        .feed_response(&resp)
        .expect_err("Should fail on blocked rating");
    assert_eq!(
        err,
        GeminiAdapterError::SafetyBlocked("HARM_CATEGORY_DANGEROUS_CONTENT".to_string())
    );
}

#[test]
fn test_12_malformed_function_call_args_fails_closed() {
    let mut translator = GeminiStreamTranslator::new();

    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-bad-tool".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart {
                text: None,
                inline_data: None,
                function_call: Some(GeminiFunctionCall {
                    id: Some("call-1".to_string()),
                    name: "exec".to_string(),
                    args: serde_json::json!("not-an-object"),
                }),
                function_response: None,
                thought: None,
                thought_signature: None,
            }])),
            finish_reason: None,
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let err = translator
        .feed_response(&resp)
        .expect_err("Should reject non-object function args");
    assert!(matches!(err, GeminiAdapterError::MalformedFunctionCall(_)));
}

#[test]
fn test_13_feed_sse_chunk_multi_line() {
    let mut translator = GeminiStreamTranslator::new();

    let sse_stream = "data: {\"responseId\":\"resp-sse-1\",\"modelVersion\":\"gemini-2.5-flash\",\"candidates\":[{\"index\":0,\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"Hello\"}]}}]}\n\ndata: {\"responseId\":\"resp-sse-1\",\"candidates\":[{\"index\":0,\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":10,\"candidatesTokenCount\":2,\"totalTokenCount\":12}}\n\n";

    let evs = translator
        .feed_sse_chunk(sse_stream)
        .expect("SSE chunk parsing failed");
    assert!(evs.len() >= 3);

    let finish_evs = translator.finish_stream().expect("Finish failed");
    assert!(
        matches!(&finish_evs[1], ResponseEvent::Completed { response_id, .. } if response_id == "resp-sse-1")
    );
}

#[test]
fn test_14_cannot_finish_before_finish_reason() {
    let mut translator = GeminiStreamTranslator::new();

    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-incomplete".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart::text("Incomplete")])),
            finish_reason: None, // No finishReason
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    translator.feed_response(&resp).unwrap();
    let err = translator
        .finish_stream()
        .expect_err("Should require finishReason");
    assert_eq!(err, GeminiAdapterError::StreamMissingFinishReason);
}

#[test]
fn test_15_negative_token_count_fails_closed() {
    let mut translator = GeminiStreamTranslator::new();

    let resp = GeminiGenerateContentResponse {
        response_id: Some("resp-neg".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart::text(
                "Negative tokens",
            )])),
            finish_reason: Some("STOP".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: Some(GeminiUsageMetadata {
            prompt_token_count: Some(-5),
            cached_content_token_count: None,
            candidates_token_count: Some(10),
            thoughts_token_count: None,
            tool_use_prompt_token_count: None,
            total_token_count: Some(5),
        }),
    };

    translator.feed_response(&resp).unwrap();
    let err = translator
        .finish_stream()
        .expect_err("Should reject negative tokens");
    assert!(matches!(err, GeminiAdapterError::InvalidUsage(_)));
}
