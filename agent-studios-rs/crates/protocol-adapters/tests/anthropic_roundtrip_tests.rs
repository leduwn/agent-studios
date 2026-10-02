use std::sync::Arc;

use agent_studios_protocol_adapters::AnthropicMessagesAdapter;
use agent_studios_protocol_adapters::anthropic::{
    AnthropicCacheControl, AnthropicContentBlock, AnthropicPromptCachePolicy,
    AnthropicRequestOptions, AnthropicThinkingPolicy, AnthropicToolResultContent,
};
use codex_api::{ResponsesApiRequest, ResponsesApiTools};
use codex_protocol::models::{
    ContentItem, FunctionCallOutputBody, FunctionCallOutputPayload, ResponseItem,
};
use serde_json::value::RawValue;

fn make_request() -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: "claude-3-7-sonnet-20250219".to_string(),
        stream: true,
        service_tier: None,
        instructions: "You are an expert coding assistant in Agent Studios IDE.".to_string(),
        input: Vec::new(),
        tools: None,
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
    }
}

#[test]
fn test_multiturn_coding_agent_loop_with_thinking_and_tool_use() {
    // -------------------------------------------------------------
    // TURN 1: User asks agent to inspect repository structure
    // -------------------------------------------------------------
    let mut req1 = make_request();
    let tools_raw = RawValue::from_string(
        r#"[
            {
                "type": "function",
                "name": "list_dir",
                "description": "List files in directory",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            },
            {
                "type": "function",
                "name": "read_file",
                "description": "Read file contents",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }
        ]"#
        .to_string(),
    )
    .unwrap();
    req1.tools = Some(ResponsesApiTools::from(Arc::from(tools_raw)));
    req1.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Please inspect the project layout.".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions {
        prompt_cache_policy: AnthropicPromptCachePolicy::ToolsAndSystem,
        thinking_policy: AnthropicThinkingPolicy::BudgetTokens(2048),
        ..Default::default()
    };

    let trans1 = AnthropicMessagesAdapter::translate_request(&req1, &options, None)
        .expect("Turn 1 request translation failed");

    // Verify system instructions and prompt cache marker
    let sys = trans1.request.system.expect("System blocks");
    assert_eq!(sys.len(), 1);
    match &sys[0] {
        agent_studios_protocol_adapters::anthropic::AnthropicSystemBlock::Text {
            cache_control,
            ..
        } => {
            assert_eq!(
                cache_control.as_ref(),
                Some(&AnthropicCacheControl::Ephemeral { ttl: None })
            );
        }
    }

    // Verify tools translated with cache control on last tool
    let tools = trans1.request.tools.expect("Tools translated");
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name, "list_dir");
    assert_eq!(tools[1].name, "read_file");
    assert_eq!(
        tools[1].cache_control,
        Some(AnthropicCacheControl::Ephemeral { ttl: None })
    );

    // Simulate Anthropic streaming response for Turn 1
    let mut stream_translator = AnthropicMessagesAdapter::new_stream_translator();
    let turn1_sse = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_turn1","type":"message","role":"assistant","model":"claude-3-7-sonnet-20250219","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":120,"output_tokens":0,"cache_creation_input_tokens":120,"cache_read_input_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"I should list the workspace root directory first to inspect the project layout."}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"cryptographic_hmac_sig_turn1"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call_list_1","name":"list_dir","input":{}}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\": \".\"}"}}

event: content_block_stop
data: {"type":"content_block_stop","index":1}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":45}}

event: message_stop
data: {"type":"message_stop"}

"#;

    let events1 = stream_translator.feed_sse_chunk(turn1_sse).unwrap();

    // Verify turn continuation signals tool use (end_turn: Some(false))
    let completed1 = events1
        .iter()
        .find_map(|e| match e {
            codex_api::ResponseEvent::Completed { end_turn, .. } => *end_turn,
            _ => None,
        })
        .expect("Completed event Turn 1");
    assert!(!completed1);

    // Capture continuation state from translator
    let continuation = stream_translator.continuation_state();
    let reasoning_id = "anthropic-reasoning-msg_turn1-0";
    assert!(continuation.get_reasoning_block(reasoning_id).is_some());

    // -------------------------------------------------------------
    // TURN 2: Tool execution result added to history + assistant response
    // -------------------------------------------------------------
    let mut req2 = req1.clone();
    // Assistant thinking item from Turn 1
    req2.input.push(ResponseItem::Reasoning {
        id: Some(codex_protocol::ResponseItemId::from_server(
            reasoning_id.to_string(),
        )),
        summary: Vec::new(),
        content: None,
        encrypted_content: None,
        internal_chat_message_metadata_passthrough: None,
    });
    // Assistant tool use item from Turn 1
    req2.input.push(ResponseItem::FunctionCall {
        id: None,
        name: "list_dir".to_string(),
        namespace: None,
        arguments: "{\"path\": \".\"}".to_string(),
        encrypted_function_args: None,
        call_id: "call_list_1".to_string(),
        internal_chat_message_metadata_passthrough: None,
    });
    // Tool execution output from host
    req2.input.push(ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("call_list_1".to_string()),
        name: Some("list_dir".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text("Cargo.toml\ncrates/\nREADME.md".to_string()),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    });

    // Translate Turn 2 with continuation state
    let trans2 = AnthropicMessagesAdapter::translate_request(&req2, &options, Some(continuation))
        .expect("Turn 2 request translation failed");

    // Verify messages structure in Turn 2
    // Expected:
    // Message 0: User turn 1
    // Message 1: Assistant with [Thinking (with signature!), ToolUse]
    // Message 2: User with [ToolResult]
    assert_eq!(trans2.request.messages.len(), 3);

    // Message 0
    assert_eq!(trans2.request.messages[0].role, "user");

    // Message 1 (Assistant)
    let asst_msg = &trans2.request.messages[1];
    assert_eq!(asst_msg.role, "assistant");
    assert_eq!(asst_msg.content.len(), 2);
    match &asst_msg.content[0] {
        AnthropicContentBlock::Thinking {
            thinking,
            signature,
        } => {
            assert_eq!(
                thinking,
                "I should list the workspace root directory first to inspect the project layout."
            );
            assert_eq!(signature, "cryptographic_hmac_sig_turn1");
        }
        other => panic!("Expected Thinking content block, found {other:?}"),
    }
    match &asst_msg.content[1] {
        AnthropicContentBlock::ToolUse {
            id, name, input, ..
        } => {
            assert_eq!(id, "call_list_1");
            assert_eq!(name, "list_dir");
            assert_eq!(input["path"], ".");
        }
        other => panic!("Expected ToolUse content block, found {other:?}"),
    }

    // Message 2 (User tool result)
    let user_tool_msg = &trans2.request.messages[2];
    assert_eq!(user_tool_msg.role, "user");
    assert_eq!(user_tool_msg.content.len(), 1);
    match &user_tool_msg.content[0] {
        AnthropicContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
            ..
        } => {
            assert_eq!(tool_use_id, "call_list_1");
            assert_eq!(*is_error, Some(false));
            assert_eq!(
                content,
                &AnthropicToolResultContent::Text("Cargo.toml\ncrates/\nREADME.md".to_string())
            );
        }
        other => panic!("Expected ToolResult content block, found {other:?}"),
    }

    // Stream Turn 2 completion
    let mut stream_translator2 = AnthropicMessagesAdapter::new_stream_translator();
    let turn2_sse = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_turn2","type":"message","role":"assistant","model":"claude-3-7-sonnet-20250219","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":210,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":120}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"The repository is a Cargo workspace containing Cargo.toml, crates directory, and README.md."}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":22}}

event: message_stop
data: {"type":"message_stop"}

"#;

    let events2 = stream_translator2.feed_sse_chunk(turn2_sse).unwrap();

    // Verify turn completed with end_turn: Some(true)
    let completed2 = events2
        .iter()
        .find_map(|e| match e {
            codex_api::ResponseEvent::Completed { end_turn, .. } => *end_turn,
            _ => None,
        })
        .expect("Completed event Turn 2");
    assert!(completed2);

    // Verify text received
    let full_text: String = events2
        .iter()
        .filter_map(|e| match e {
            codex_api::ResponseEvent::OutputTextDelta(d) => Some(d.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        full_text,
        "The repository is a Cargo workspace containing Cargo.toml, crates directory, and README.md."
    );
}

#[test]
fn test_parallel_tool_calls_roundtrip() {
    let mut req = make_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Read file A and file B.".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });
    // Two parallel function calls by assistant
    req.input.push(ResponseItem::FunctionCall {
        id: None,
        name: "read_file".to_string(),
        namespace: None,
        arguments: "{\"path\": \"a.txt\"}".to_string(),
        encrypted_function_args: None,
        call_id: "call_a".to_string(),
        internal_chat_message_metadata_passthrough: None,
    });
    req.input.push(ResponseItem::FunctionCall {
        id: None,
        name: "read_file".to_string(),
        namespace: None,
        arguments: "{\"path\": \"b.txt\"}".to_string(),
        encrypted_function_args: None,
        call_id: "call_b".to_string(),
        internal_chat_message_metadata_passthrough: None,
    });
    // Two parallel function call outputs by host
    req.input.push(ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("call_a".to_string()),
        name: Some("read_file".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text("contents of A".to_string()),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    });
    req.input.push(ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("call_b".to_string()),
        name: Some("read_file".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text("contents of B".to_string()),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    });

    let options = AnthropicRequestOptions::default();
    let trans = AnthropicMessagesAdapter::translate_request(&req, &options, None).unwrap();

    // 1 user msg, 1 assistant msg with 2 tool_uses, 1 user msg with 2 tool_results
    assert_eq!(trans.request.messages.len(), 3);

    let asst_msg = &trans.request.messages[1];
    assert_eq!(asst_msg.role, "assistant");
    assert_eq!(asst_msg.content.len(), 2);
    match &asst_msg.content[0] {
        AnthropicContentBlock::ToolUse { id, .. } => assert_eq!(id, "call_a"),
        other => panic!("Expected ToolUse, found {other:?}"),
    }
    match &asst_msg.content[1] {
        AnthropicContentBlock::ToolUse { id, .. } => assert_eq!(id, "call_b"),
        other => panic!("Expected ToolUse, found {other:?}"),
    }

    let user_tool_msg = &trans.request.messages[2];
    assert_eq!(user_tool_msg.role, "user");
    assert_eq!(user_tool_msg.content.len(), 2);
    match &user_tool_msg.content[0] {
        AnthropicContentBlock::ToolResult { tool_use_id, .. } => assert_eq!(tool_use_id, "call_a"),
        other => panic!("Expected ToolResult, found {other:?}"),
    }
    match &user_tool_msg.content[1] {
        AnthropicContentBlock::ToolResult { tool_use_id, .. } => assert_eq!(tool_use_id, "call_b"),
        other => panic!("Expected ToolResult, found {other:?}"),
    }
}
