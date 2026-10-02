use agent_studios_protocol_adapters::anthropic::{
    AnthropicAdapterError, AnthropicStreamApiError, AnthropicStreamTranslator, NativeThinkingBlock,
};
use codex_api::ResponseEvent;
use codex_protocol::models::ResponseItem;

#[test]
fn test_01_complete_text_stream_happy_path() {
    let mut translator = AnthropicStreamTranslator::new();

    let chunks = [
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_123\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-3-5-sonnet-20241022\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":25,\"output_tokens\":0,\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":10}}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello \"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"world!\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":5}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    ];

    let mut events = Vec::new();
    for chunk in chunks {
        events.extend(translator.feed_sse_chunk(chunk).unwrap());
    }

    assert!(
        events
            .iter()
            .any(|e| matches!(e, ResponseEvent::Created { .. }))
    );
    assert!(events.iter().any(|e| match e {
        ResponseEvent::ServerModel(m) => m == "claude-3-5-sonnet-20241022",
        _ => false,
    }));
    assert!(events.iter().any(|e| match e {
        ResponseEvent::OutputTextDelta(d) => d == "Hello ",
        _ => false,
    }));
    assert!(events.iter().any(|e| match e {
        ResponseEvent::OutputTextDelta(d) => d == "world!",
        _ => false,
    }));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ResponseEvent::OutputItemDone(_)))
    );

    let completed = events
        .iter()
        .find_map(|e| match e {
            ResponseEvent::Completed {
                response_id,
                token_usage,
                end_turn,
                ..
            } => Some((response_id, token_usage, end_turn)),
            _ => None,
        })
        .expect("Expected ResponseEvent::Completed");

    assert_eq!(completed.0, "msg_123");
    assert_eq!(*completed.2, Some(true));
    let usage = completed.1.as_ref().expect("Expected token usage");
    assert_eq!(usage.input_tokens, 25);
    assert_eq!(usage.output_tokens, 5);
    assert_eq!(usage.total_tokens, 30);
    assert_eq!(usage.cached_input_tokens, 10);
}

#[test]
fn test_02_stop_reason_turn_continuation_mapping() {
    // 1. end_turn -> end_turn: Some(true)
    let mut tr1 = AnthropicStreamTranslator::new();
    let sse1 = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-3-5-sonnet-20241022\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":2}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    let events1 = tr1.feed_sse_chunk(sse1).unwrap();
    let end_turn1 = events1.iter().find_map(|e| match e {
        ResponseEvent::Completed { end_turn, .. } => *end_turn,
        _ => None,
    });
    assert_eq!(end_turn1, Some(true));

    // 2. tool_use -> end_turn: Some(false)
    let mut tr2 = AnthropicStreamTranslator::new();
    let sse2 = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-3-5-sonnet-20241022\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_abc\",\"name\":\"bash\",\"input\":{}}}\n\nevent: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":15}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    let events2 = tr2.feed_sse_chunk(sse2).unwrap();
    assert!(events2.iter().any(|e| match e {
        ResponseEvent::OutputItemAdded(ResponseItem::FunctionCall { call_id, name, .. }) => {
            call_id == "call_abc" && name == "bash"
        }
        _ => false,
    }));
    let end_turn2 = events2.iter().find_map(|e| match e {
        ResponseEvent::Completed { end_turn, .. } => *end_turn,
        _ => None,
    });
    assert_eq!(end_turn2, Some(false));
}

#[test]
fn test_03_fail_closed_on_truncation_and_refusal() {
    // max_tokens
    let mut tr1 = AnthropicStreamTranslator::new();
    let sse1 = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-3-5-sonnet\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"max_tokens\"},\"usage\":{\"output_tokens\":4096}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    let err1 = tr1.feed_sse_chunk(sse1).unwrap_err();
    assert_eq!(err1, AnthropicAdapterError::MaxTokensExceeded);

    // model_context_window_exceeded
    let mut tr2 = AnthropicStreamTranslator::new();
    let sse2 = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-3-5-sonnet\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":200000,\"output_tokens\":0}}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"model_context_window_exceeded\"},\"usage\":{\"output_tokens\":0}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    let err2 = tr2.feed_sse_chunk(sse2).unwrap_err();
    assert_eq!(err2, AnthropicAdapterError::ContextWindowExceeded);

    // refusal
    let mut tr3 = AnthropicStreamTranslator::new();
    let sse3 = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_3\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-3-5-sonnet\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"refusal\"},\"usage\":{\"output_tokens\":0}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    let err3 = tr3.feed_sse_chunk(sse3).unwrap_err();
    assert_eq!(
        err3,
        AnthropicAdapterError::ModelRefusal("refusal".to_string())
    );
}

#[test]
fn test_04_tool_call_streaming_with_json_delta() {
    let mut translator = AnthropicStreamTranslator::new();
    let sse = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_tool","type":"message","role":"assistant","model":"claude-3-5-sonnet-20241022","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_read_1","name":"read_file","input":{}}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"\"src/main.rs\"}"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":20}}

event: message_stop
data: {"type":"message_stop"}

"#;

    let events = translator.feed_sse_chunk(sse).unwrap();

    // Verify OutputItemAdded with tool call
    assert!(events.iter().any(|e| match e {
        ResponseEvent::OutputItemAdded(ResponseItem::FunctionCall {
            call_id,
            name,
            arguments,
            ..
        }) => call_id == "call_read_1" && name == "read_file" && arguments.is_empty(),
        _ => false,
    }));

    // Verify ToolCallInputDelta events
    let deltas: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            ResponseEvent::ToolCallInputDelta { delta, .. } => Some(delta.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, vec!["{\"path\":", "\"src/main.rs\"}"]);

    // Verify OutputItemDone contains complete deserialized arguments
    assert!(events.iter().any(|e| match e {
        ResponseEvent::OutputItemDone(ResponseItem::FunctionCall {
            call_id,
            name,
            arguments,
            ..
        }) =>
            call_id == "call_read_1"
                && name == "read_file"
                && arguments == "{\"path\":\"src/main.rs\"}",
        _ => false,
    }));
}

#[test]
fn test_05_thinking_and_signature_state_preservation() {
    let mut translator = AnthropicStreamTranslator::new();

    let sse = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_think","type":"message","role":"assistant","model":"claude-3-7-sonnet-20250219","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":50,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Plan: analyze AST"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"cryptographic_sig_xyz"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":30}}

event: message_stop
data: {"type":"message_stop"}

"#;

    let events = translator.feed_sse_chunk(sse).unwrap();

    // Verify Reasoning Item added and delta received
    assert!(events.iter().any(|e| matches!(
        e,
        ResponseEvent::OutputItemAdded(ResponseItem::Reasoning { .. })
    )));
    assert!(events.iter().any(|e| match e {
        ResponseEvent::ReasoningContentDelta { delta, .. } => delta == "Plan: analyze AST",
        _ => false,
    }));
    assert!(events.iter().any(|e| matches!(
        e,
        ResponseEvent::OutputItemDone(ResponseItem::Reasoning { .. })
    )));

    // Verify continuation state has preserved thinking block and signature
    let reasoning_id = "anthropic-reasoning-msg_think-0";
    let stored = translator
        .continuation_state()
        .get_reasoning_block(reasoning_id)
        .expect("Stored thinking block");
    match stored {
        NativeThinkingBlock::Thinking {
            thinking,
            signature,
        } => {
            assert_eq!(thinking, "Plan: analyze AST");
            assert_eq!(signature, "cryptographic_sig_xyz");
        }
        NativeThinkingBlock::RedactedThinking { .. } => {
            panic!("Expected Thinking, found RedactedThinking")
        }
    }
}

#[test]
fn test_06_redacted_thinking_preservation() {
    let mut translator = AnthropicStreamTranslator::new();

    let sse = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_redact","type":"message","role":"assistant","model":"claude-3-7-sonnet-20250219","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":50,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"redacted_thinking","data":"opaque_encrypted_data_blob"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":10}}

event: message_stop
data: {"type":"message_stop"}

"#;

    let _events = translator.feed_sse_chunk(sse).unwrap();

    let reasoning_id = "anthropic-reasoning-msg_redact-0";
    let stored = translator
        .continuation_state()
        .get_reasoning_block(reasoning_id)
        .expect("Stored redacted thinking block");
    match stored {
        NativeThinkingBlock::RedactedThinking { data } => {
            assert_eq!(data, "opaque_encrypted_data_blob");
        }
        NativeThinkingBlock::Thinking { .. } => panic!("Expected RedactedThinking, found Thinking"),
    }
}

#[test]
fn test_07_ping_and_server_api_error_events() {
    // 1. Ping event handled gracefully
    let mut translator = AnthropicStreamTranslator::new();
    let events = translator.feed_line("data: {\"type\": \"ping\"}").unwrap();
    assert!(events.is_empty());

    // 2. Stream API error event fails closed
    let mut translator2 = AnthropicStreamTranslator::new();
    let err = translator2
        .feed_line(
            "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Server overloaded\"}}",
        )
        .unwrap_err();
    assert_eq!(
        err,
        AnthropicAdapterError::StreamApiError(AnthropicStreamApiError {
            r#type: "overloaded_error".to_string(),
            message: "Server overloaded".to_string(),
        })
    );
}

#[test]
fn test_08_premature_eof_with_open_block_fails() {
    let mut translator = AnthropicStreamTranslator::new();
    let sse = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_open","type":"message","role":"assistant","model":"claude-3-5-sonnet","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":"unfinished text"}}

event: message_stop
data: {"type":"message_stop"}

"#;
    let err = translator.feed_sse_chunk(sse).unwrap_err();
    assert_eq!(err, AnthropicAdapterError::StreamOpenBlocksOnStop(vec![0]));
}

#[test]
fn test_09_state_machine_order_violations() {
    // Delta before start
    let mut tr1 = AnthropicStreamTranslator::new();
    let err1 = tr1
        .feed_line(
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"early\"}}",
        )
        .unwrap_err();
    assert_eq!(err1, AnthropicAdapterError::StreamMessageNotStarted);

    // Duplicate message_start
    let mut tr2 = AnthropicStreamTranslator::new();
    let sse_start = "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_dup\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-3-5-sonnet\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n";
    tr2.feed_sse_chunk(sse_start).unwrap();
    let err2 = tr2.feed_sse_chunk(sse_start).unwrap_err();
    assert_eq!(err2, AnthropicAdapterError::StreamMessageAlreadyStarted);
}

#[test]
fn test_10_negative_token_delta_rejected() {
    let mut translator = AnthropicStreamTranslator::new();
    let sse = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_neg","type":"message","role":"assistant","model":"claude-3-5-sonnet","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":-5}}

"#;
    let err = translator.feed_sse_chunk(sse).unwrap_err();
    assert_eq!(
        err,
        AnthropicAdapterError::InvalidUsage(
            "Negative output_tokens in message_delta: -5".to_string()
        )
    );
}
