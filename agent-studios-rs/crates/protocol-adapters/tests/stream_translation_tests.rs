use agent_studios_protocol_adapters::{
    ChatAdapterError, ChatChunkChoice, ChatChunkDelta, ChatChunkFunctionCall, ChatChunkToolCall,
    ChatCompletionChunk, ChatCompletionStreamTranslator, ChatPromptTokensDetails, ChatUsage,
    decode_sse_line, is_done_line,
};
use codex_api::ResponseEvent;
use codex_protocol::models::{ContentItem, ResponseItem};

fn make_chunk(
    id: &str,
    delta: ChatChunkDelta,
    finish_reason: Option<&str>,
    usage: Option<ChatUsage>,
) -> ChatCompletionChunk {
    ChatCompletionChunk {
        id: id.to_string(),
        object: Some("chat.completion.chunk".to_string()),
        created: Some(1720000000),
        model: Some("gpt-4o".to_string()),
        choices: vec![ChatChunkChoice {
            index: 0,
            delta,
            finish_reason: finish_reason.map(|s| s.to_string()),
        }],
        usage,
    }
}

#[test]
fn test_basic_text_streaming() {
    let mut translator = ChatCompletionStreamTranslator::new();

    // Chunk 1: first delta text
    let chunk1 = make_chunk(
        "chatcmpl-test-1",
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: Some("Hello".to_string()),
            tool_calls: None,
        },
        None,
        None,
    );
    let events1 = translator.feed_chunk(&chunk1).expect("feed chunk 1");
    // Expected: Created event, OutputItemAdded(Message), OutputTextDelta("Hello")
    assert_eq!(events1.len(), 3);
    assert!(
        matches!(&events1[0], ResponseEvent::Created { response_id } if response_id.as_deref() == Some("chatcmpl-test-1"))
    );
    assert!(matches!(
        &events1[1],
        ResponseEvent::OutputItemAdded(ResponseItem::Message { .. })
    ));
    assert!(matches!(&events1[2], ResponseEvent::OutputTextDelta(delta) if delta == "Hello"));

    // Chunk 2: second delta text
    let chunk2 = make_chunk(
        "chatcmpl-test-1",
        ChatChunkDelta {
            role: None,
            content: Some(" world!".to_string()),
            tool_calls: None,
        },
        None,
        None,
    );
    let events2 = translator.feed_chunk(&chunk2).expect("feed chunk 2");
    assert_eq!(events2.len(), 1);
    assert!(matches!(&events2[0], ResponseEvent::OutputTextDelta(delta) if delta == " world!"));

    // Chunk 3: finish_reason stop + usage
    let chunk3 = make_chunk(
        "chatcmpl-test-1",
        ChatChunkDelta::default(),
        Some("stop"),
        Some(ChatUsage {
            prompt_tokens: 15,
            completion_tokens: 8,
            total_tokens: 23,
            prompt_tokens_details: Some(ChatPromptTokensDetails {
                cached_tokens: Some(5),
            }),
            completion_tokens_details: None,
        }),
    );
    let events3 = translator.feed_chunk(&chunk3).expect("feed chunk 3");
    // Finish reason stop closes active message item
    assert_eq!(events3.len(), 1);
    match &events3[0] {
        ResponseEvent::OutputItemDone(ResponseItem::Message { content, .. }) => {
            assert_eq!(content.len(), 1);
            match &content[0] {
                ContentItem::OutputText { text } => assert_eq!(text, "Hello world!"),
                _ => panic!("Expected OutputText"),
            }
        }
        _ => panic!("Expected OutputItemDone(Message)"),
    }

    // Stream concluded
    let final_events = translator.finish().expect("finish");
    assert_eq!(final_events.len(), 1);
    match &final_events[0] {
        ResponseEvent::Completed {
            response_id,
            token_usage,
            end_turn,
            ..
        } => {
            assert_eq!(response_id, "chatcmpl-test-1");
            assert_eq!(end_turn, &Some(true));
            let usage = token_usage.as_ref().expect("token usage");
            assert_eq!(usage.input_tokens, 15);
            assert_eq!(usage.cached_input_tokens, 5);
            assert_eq!(usage.output_tokens, 8);
            assert_eq!(usage.total_tokens, 23);
        }
        _ => panic!("Expected Completed event"),
    }
}

#[test]
fn test_interleaved_fragmented_tool_calls_streaming() {
    let mut translator = ChatCompletionStreamTranslator::new();

    // Chunk 1: Tool 0 begins
    let chunk1 = make_chunk(
        "chatcmpl-tools",
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: Some("call_tool_0".to_string()),
                r#type: Some("function".to_string()),
                function: Some(ChatChunkFunctionCall {
                    name: Some("read_file".to_string()),
                    arguments: Some("{\"path\":".to_string()),
                }),
            }]),
        },
        None,
        None,
    );
    let events1 = translator.feed_chunk(&chunk1).expect("feed chunk 1");
    // Created, OutputItemAdded(FunctionCall), ToolCallInputDelta
    assert_eq!(events1.len(), 3);
    assert!(matches!(&events1[0], ResponseEvent::Created { .. }));
    assert!(
        matches!(&events1[1], ResponseEvent::OutputItemAdded(ResponseItem::FunctionCall { call_id, name, .. }) if call_id == "call_tool_0" && name == "read_file")
    );
    assert!(
        matches!(&events1[2], ResponseEvent::ToolCallInputDelta { delta, .. } if delta == "{\"path\":")
    );

    // Chunk 2: Tool 1 begins (interleaved!)
    let chunk2 = make_chunk(
        "chatcmpl-tools",
        ChatChunkDelta {
            role: None,
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 1,
                id: Some("call_tool_1".to_string()),
                r#type: Some("function".to_string()),
                function: Some(ChatChunkFunctionCall {
                    name: Some("list_dir".to_string()),
                    arguments: Some("{\"dir\":".to_string()),
                }),
            }]),
        },
        None,
        None,
    );
    let events2 = translator.feed_chunk(&chunk2).expect("feed chunk 2");
    assert_eq!(events2.len(), 2);
    assert!(
        matches!(&events2[0], ResponseEvent::OutputItemAdded(ResponseItem::FunctionCall { call_id, name, .. }) if call_id == "call_tool_1" && name == "list_dir")
    );
    assert!(
        matches!(&events2[1], ResponseEvent::ToolCallInputDelta { delta, .. } if delta == "{\"dir\":")
    );

    // Chunk 3: Tool 0 gets more arguments
    let chunk3 = make_chunk(
        "chatcmpl-tools",
        ChatChunkDelta {
            role: None,
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: None,
                r#type: None,
                function: Some(ChatChunkFunctionCall {
                    name: None,
                    arguments: Some("\"src/lib.rs\"}".to_string()),
                }),
            }]),
        },
        None,
        None,
    );
    let events3 = translator.feed_chunk(&chunk3).expect("feed chunk 3");
    assert_eq!(events3.len(), 1);
    assert!(
        matches!(&events3[0], ResponseEvent::ToolCallInputDelta { delta, .. } if delta == "\"src/lib.rs\"}")
    );

    // Chunk 4: Tool 1 gets more arguments + finish_reason = tool_calls
    let chunk4 = make_chunk(
        "chatcmpl-tools",
        ChatChunkDelta {
            role: None,
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 1,
                id: None,
                r#type: None,
                function: Some(ChatChunkFunctionCall {
                    name: None,
                    arguments: Some("\".\"}".to_string()),
                }),
            }]),
        },
        Some("tool_calls"),
        None,
    );
    let events4 = translator.feed_chunk(&chunk4).expect("feed chunk 4");
    // ToolCallInputDelta for tool 1 + OutputItemDone for tool 0 + OutputItemDone for tool 1
    assert_eq!(events4.len(), 3);
    assert!(
        matches!(&events4[0], ResponseEvent::ToolCallInputDelta { delta, .. } if delta == "\".\"}")
    );
    assert!(
        matches!(&events4[1], ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { call_id, arguments, .. }) if call_id == "call_tool_0" && arguments == "{\"path\":\"src/lib.rs\"}")
    );
    assert!(
        matches!(&events4[2], ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { call_id, arguments, .. }) if call_id == "call_tool_1" && arguments == "{\"dir\":\".\"}")
    );

    // Complete stream
    let final_events = translator.finish().expect("finish");
    assert_eq!(final_events.len(), 1);
    assert!(
        matches!(&final_events[0], ResponseEvent::Completed { response_id, .. } if response_id == "chatcmpl-tools")
    );
}

#[test]
fn test_length_and_content_filter_rejections() {
    let mut translator1 = ChatCompletionStreamTranslator::new();
    let chunk_length = make_chunk(
        "chatcmpl-len",
        ChatChunkDelta {
            role: None,
            content: Some("truncated text".to_string()),
            tool_calls: None,
        },
        Some("length"),
        None,
    );
    let err1 = translator1.feed_chunk(&chunk_length).unwrap_err();
    assert!(matches!(err1, ChatAdapterError::StreamIncomplete(_)));

    let mut translator2 = ChatCompletionStreamTranslator::new();
    let chunk_filter = make_chunk(
        "chatcmpl-cf",
        ChatChunkDelta::default(),
        Some("content_filter"),
        None,
    );
    let err2 = translator2.feed_chunk(&chunk_filter).unwrap_err();
    assert!(matches!(err2, ChatAdapterError::ContentFilterTriggered(_)));
}

#[test]
fn test_multiple_choices_unsupported_rejection() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk = ChatCompletionChunk {
        id: "chatcmpl-multi".to_string(),
        object: None,
        created: None,
        model: None,
        choices: vec![ChatChunkChoice {
            index: 1, // Invalid: must be 0
            delta: ChatChunkDelta::default(),
            finish_reason: None,
        }],
        usage: None,
    };
    let err = translator.feed_chunk(&chunk).unwrap_err();
    assert_eq!(err, ChatAdapterError::MultipleChoicesUnsupported(1));
}

#[test]
fn test_sse_line_decoding() {
    // 1. Valid data chunk
    let line =
        "data: {\"id\":\"chunk-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"test\"}}]}";
    let chunk = decode_sse_line(line).expect("decode").expect("some chunk");
    assert_eq!(chunk.id, "chunk-1");
    assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("test"));

    // 2. Terminal marker
    let done_line = "data: [DONE]";
    assert!(is_done_line(done_line));
    assert!(decode_sse_line(done_line).expect("decode done").is_none());

    // 3. Keepalive / comments
    assert!(
        decode_sse_line(": keepalive")
            .expect("decode comment")
            .is_none()
    );
    assert!(decode_sse_line("   ").expect("decode whitespace").is_none());

    // 4. Invalid JSON payload
    let bad_line = "data: {not valid json}";
    assert!(matches!(
        decode_sse_line(bad_line).unwrap_err(),
        ChatAdapterError::InvalidStreamChunk(_)
    ));
}
