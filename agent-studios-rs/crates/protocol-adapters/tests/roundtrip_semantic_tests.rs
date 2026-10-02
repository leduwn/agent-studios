use std::sync::Arc;

use agent_studios_protocol_adapters::{ChatCompletionsAdapter, decode_sse_line, is_done_line};
use codex_api::{ResponseEvent, ResponsesApiRequest, ResponsesApiTools};
use codex_protocol::models::{ContentItem, ResponseItem};
use serde_json::json;

#[test]
fn test_roundtrip_semantic_workflow() {
    // 1. Prepare Codex ResponsesApiRequest
    let tools_json = json!([
        {
            "type": "function",
            "name": "search_codebase",
            "description": "Search repository symbols and text",
            "strict": true,
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" }
                },
                "required": ["pattern"]
            }
        }
    ]);
    let raw = serde_json::value::to_raw_value(&tools_json).unwrap();

    let request = ResponsesApiRequest {
        model: "deepseek-chat".to_string(),
        stream: true,
        service_tier: None,
        instructions: "System persona for Agent Studios".to_string(),
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "Find callers of resolve_catalog_url".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        }],
        tools: Some(ResponsesApiTools::from(Arc::from(raw))),
        tool_choice: "auto".to_string(),
        parallel_tool_calls: true,
        reasoning: None,
        store: false,
        stream_options: None,
        include: Vec::new(),
        prompt_cache_key: None,
        text: None,
        client_metadata: None,
        access_programs: None,
    };

    // 2. Translate to ChatCompletionRequest
    let translation =
        ChatCompletionsAdapter::translate_request(&request).expect("translate request");
    let chat_req = translation.request;

    // Verify wire payload matches OpenAI specification
    let serialized_req = serde_json::to_value(&chat_req).expect("serialize request");
    assert_eq!(serialized_req["model"], "deepseek-chat");
    assert_eq!(serialized_req["messages"][0]["role"], "system");
    assert_eq!(
        serialized_req["messages"][0]["content"],
        "System persona for Agent Studios"
    );
    assert_eq!(serialized_req["messages"][1]["role"], "user");
    assert_eq!(
        serialized_req["messages"][1]["content"],
        "Find callers of resolve_catalog_url"
    );
    assert_eq!(serialized_req["tools"][0]["type"], "function");
    assert_eq!(
        serialized_req["tools"][0]["function"]["name"],
        "search_codebase"
    );

    // 3. Simulate stream SSE response from proxy / model endpoint
    let sse_stream = vec![
        ": keep-alive ping",
        "data: {\"id\":\"chatcmpl-roundtrip\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Searching now...\"}}]}",
        "data: {\"id\":\"chatcmpl-roundtrip\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_srch_1\",\"type\":\"function\",\"function\":{\"name\":\"search_codebase\",\"arguments\":\"{\\\"pattern\\\":\\\"resolve\"}}]}}]}",
        "data: {\"id\":\"chatcmpl-roundtrip\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"_catalog_url\\\"}\"}}]}}]}",
        "data: {\"id\":\"chatcmpl-roundtrip\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":50,\"completion_tokens\":25,\"total_tokens\":75}}",
        "data: [DONE]",
    ];

    // 4. Translate stream SSE into Codex ResponseEvents
    let mut translator = ChatCompletionsAdapter::new_stream_translator();
    let mut collected_events = Vec::new();

    for line in sse_stream {
        if is_done_line(line) {
            let done_events = translator.feed_done().expect("feed done");
            collected_events.extend(done_events);
            break;
        }

        if let Some(chunk) = decode_sse_line(line).expect("decode chunk") {
            let chunk_events = translator.feed_chunk(&chunk).expect("feed chunk");
            collected_events.extend(chunk_events);
        }
    }

    // 5. Verify event sequence
    assert!(!collected_events.is_empty());

    // Event 0: Created
    assert!(
        matches!(&collected_events[0], ResponseEvent::Created { response_id } if response_id.as_deref() == Some("chatcmpl-roundtrip"))
    );

    // Event 1: OutputItemAdded (Message)
    assert!(matches!(
        &collected_events[1],
        ResponseEvent::OutputItemAdded(ResponseItem::Message { .. })
    ));

    // Event 2: OutputTextDelta ("Searching now...")
    assert!(
        matches!(&collected_events[2], ResponseEvent::OutputTextDelta(d) if d == "Searching now...")
    );

    // Event 3: OutputItemDone (Message finalized when tool calls begin)
    match &collected_events[3] {
        ResponseEvent::OutputItemDone(ResponseItem::Message { content, .. }) => match &content[0] {
            ContentItem::OutputText { text } => assert_eq!(text, "Searching now..."),
            _ => panic!("Expected OutputText"),
        },
        _ => panic!("Expected OutputItemDone(Message)"),
    }

    // Event 4: OutputItemAdded (FunctionCall)
    assert!(
        matches!(&collected_events[4], ResponseEvent::OutputItemAdded(ResponseItem::FunctionCall { call_id, name, .. }) if call_id == "call_srch_1" && name == "search_codebase")
    );

    // Event 5 & 6: ToolCallInputDelta
    assert!(
        matches!(&collected_events[5], ResponseEvent::ToolCallInputDelta { delta, .. } if delta == "{\"pattern\":\"resolve")
    );
    assert!(
        matches!(&collected_events[6], ResponseEvent::ToolCallInputDelta { delta, .. } if delta == "_catalog_url\"}")
    );

    // Event 7: OutputItemDone (FunctionCall with fully assembled arguments)
    match &collected_events[7] {
        ResponseEvent::OutputItemDone(ResponseItem::FunctionCall {
            call_id, arguments, ..
        }) => {
            assert_eq!(call_id, "call_srch_1");
            assert_eq!(arguments, "{\"pattern\":\"resolve_catalog_url\"}");
        }
        _ => panic!("Expected OutputItemDone(FunctionCall)"),
    }

    // Event 8: Completed with TokenUsage
    match &collected_events[8] {
        ResponseEvent::Completed {
            response_id,
            token_usage,
            end_turn,
            ..
        } => {
            assert_eq!(response_id, "chatcmpl-roundtrip");
            assert_eq!(end_turn, &Some(false));
            let usage = token_usage.as_ref().expect("usage present");
            assert_eq!(usage.input_tokens, 50);
            assert_eq!(usage.output_tokens, 25);
            assert_eq!(usage.total_tokens, 75);
        }
        _ => panic!("Expected Completed event"),
    }
}
