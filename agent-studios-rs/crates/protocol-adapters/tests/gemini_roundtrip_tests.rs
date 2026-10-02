use std::sync::Arc;

use agent_studios_protocol_adapters::gemini::{
    GeminiAdapter, GeminiAdapterOptions, GeminiCandidate, GeminiContent, GeminiFunctionCall,
    GeminiFunctionCallingMode, GeminiGenerateContentResponse, GeminiPart, GeminiThinkingPolicy,
};
use codex_api::{ResponsesApiRequest, ResponsesApiTools};
use codex_protocol::models::{
    ContentItem, FunctionCallOutputBody, FunctionCallOutputPayload, ResponseItem,
};
use serde_json::value::RawValue;

fn make_base_request(model: &str) -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: model.to_string(),
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
fn test_01_simple_multi_turn_roundtrip() {
    let options = GeminiAdapterOptions::new();

    // Turn 1 Request
    let mut req1 = make_base_request("gemini-2.5-pro");
    req1.instructions = "You are a helpful assistant.".to_string();
    req1.input = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Hi, what is Rust?".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }];

    let trans1 = GeminiAdapter::translate_request(&req1, &options, None).unwrap();
    assert_eq!(trans1.model, "gemini-2.5-pro");
    assert_eq!(trans1.request.contents.len(), 1);
    assert_eq!(trans1.request.contents[0].role.as_deref(), Some("user"));

    // Turn 1 Response Simulation
    let mut translator = GeminiAdapter::new_stream_translator();
    let resp1 = GeminiGenerateContentResponse {
        response_id: Some("turn-1-resp".to_string()),
        model_version: Some("gemini-2.5-pro".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart::text(
                "Rust is a systems programming language.",
            )])),
            finish_reason: Some("STOP".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };
    translator.feed_response(&resp1).unwrap();
    translator.finish_stream().unwrap();

    let cont_state = translator.continuation_state();

    // Turn 2 Request: Includes user follow-up and assistant's previous message
    let mut req2 = make_base_request("gemini-2.5-pro");
    req2.instructions = "You are a helpful assistant.".to_string();
    req2.input = vec![
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "Hi, what is Rust?".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Message {
            id: None,
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: "Rust is a systems programming language.".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "Is it memory safe?".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
    ];

    let trans2 = GeminiAdapter::translate_request(&req2, &options, Some(cont_state)).unwrap();
    assert_eq!(trans2.request.contents.len(), 3);
    assert_eq!(trans2.request.contents[0].role.as_deref(), Some("user"));
    assert_eq!(trans2.request.contents[1].role.as_deref(), Some("model"));
    assert_eq!(trans2.request.contents[2].role.as_deref(), Some("user"));
    assert_eq!(
        trans2.request.contents[1].parts[0].text.as_deref(),
        Some("Rust is a systems programming language.")
    );
}

#[test]
fn test_02_tool_call_and_result_multi_turn_roundtrip() {
    let options = GeminiAdapterOptions::new();

    // Turn 1: Model emits function call
    let mut translator = GeminiAdapter::new_stream_translator();
    let resp1 = GeminiGenerateContentResponse {
        response_id: Some("resp-tc-001".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart {
                text: None,
                inline_data: None,
                function_call: Some(GeminiFunctionCall {
                    id: Some("wire-fc-id-42".to_string()),
                    name: "lookup_user".to_string(),
                    args: serde_json::json!({ "user_id": 100 }),
                }),
                function_response: None,
                thought: None,
                thought_signature: Some("opaque-crypto-sig-call-42".to_string()),
            }])),
            finish_reason: Some("STOP".to_string()),
            safety_ratings: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    translator.feed_response(&resp1).unwrap();
    translator.finish_stream().unwrap();

    let cont_state = translator.continuation_state();

    // Verify continuation state has mapped the tool call and signature
    let meta = cont_state.get_tool_call("wire-fc-id-42").unwrap();
    assert_eq!(meta.name, "lookup_user");
    assert_eq!(meta.provider_call_id.as_deref(), Some("wire-fc-id-42"));
    assert_eq!(
        cont_state.get_signature("wire-fc-id-42"),
        Some("opaque-crypto-sig-call-42")
    );

    // Turn 2: User responds with function call output
    let mut req2 = make_base_request("gemini-2.5-flash");
    req2.input = vec![
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "Find user 100".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::FunctionCall {
            id: None,
            call_id: "wire-fc-id-42".to_string(),
            name: "lookup_user".to_string(),
            arguments: "{\"user_id\":100}".to_string(),
            namespace: None,
            encrypted_function_args: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::FunctionCallOutput {
            id: None,
            call_id: Some("wire-fc-id-42".to_string()),
            name: Some("lookup_user".to_string()),
            namespace: None,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text("{\"name\":\"Alice\"}".to_string()),
                success: Some(true),
            },
            internal_chat_message_metadata_passthrough: None,
        },
    ];

    let trans2 = GeminiAdapter::translate_request(&req2, &options, Some(cont_state)).unwrap();
    assert_eq!(trans2.request.contents.len(), 3);

    // Turn 2 Model content: has tool call AND thought signature preserved!
    let model_turn = &trans2.request.contents[1];
    assert_eq!(model_turn.role.as_deref(), Some("model"));
    let fc_part = &model_turn.parts[0];
    assert_eq!(
        fc_part.thought_signature.as_deref(),
        Some("opaque-crypto-sig-call-42")
    );
    assert_eq!(fc_part.function_call.as_ref().unwrap().name, "lookup_user");

    // Turn 3 User content: has functionResponse
    let tool_resp_turn = &trans2.request.contents[2];
    assert_eq!(tool_resp_turn.role.as_deref(), Some("user"));
    let resp_part = &tool_resp_turn.parts[0];
    let fr = resp_part.function_response.as_ref().unwrap();
    assert_eq!(fr.name, "lookup_user");
    assert_eq!(fr.id.as_deref(), Some("wire-fc-id-42"));
    assert_eq!(
        fr.response["output"],
        serde_json::json!("{\"name\":\"Alice\"}")
    );
}

#[test]
fn test_03_deterministic_call_id_omitted_on_wire_roundtrip() {
    let options = GeminiAdapterOptions::new();

    // Turn 1: Model emits function call without ID
    let mut translator = GeminiAdapter::new_stream_translator();
    let resp1 = GeminiGenerateContentResponse {
        response_id: Some("resp-noid-test".to_string()),
        model_version: Some("gemini-2.5-flash".to_string()),
        candidates: Some(vec![GeminiCandidate {
            index: Some(0),
            content: Some(GeminiContent::model(vec![GeminiPart {
                text: None,
                inline_data: None,
                function_call: Some(GeminiFunctionCall {
                    id: None, // No ID from Gemini
                    name: "list_files".to_string(),
                    args: serde_json::json!({}),
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

    translator.feed_response(&resp1).unwrap();
    translator.finish_stream().unwrap();

    let cont_state = translator.continuation_state();
    let deterministic_id = "gemini-call-resp-noid-test-0-0";

    // Turn 2: Client returns output for the deterministic ID
    let mut req2 = make_base_request("gemini-2.5-flash");
    req2.input = vec![
        ResponseItem::FunctionCall {
            id: None,
            call_id: deterministic_id.to_string(),
            name: "list_files".to_string(),
            arguments: "{}".to_string(),
            namespace: None,
            encrypted_function_args: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::FunctionCallOutput {
            id: None,
            call_id: Some(deterministic_id.to_string()),
            name: Some("list_files".to_string()),
            namespace: None,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text("file1.txt\nfile2.txt".to_string()),
                success: Some(true),
            },
            internal_chat_message_metadata_passthrough: None,
        },
    ];

    let trans2 = GeminiAdapter::translate_request(&req2, &options, Some(cont_state)).unwrap();
    let tool_resp_turn = &trans2.request.contents[1];
    let fr = tool_resp_turn.parts[0].function_response.as_ref().unwrap();
    assert_eq!(fr.name, "list_files");
    // Since provider omitted ID, the wire functionResponse must NOT emit fake ID
    assert_eq!(fr.id, None);
}

#[test]
fn test_04_turn_coalescing_multiple_tool_outputs() {
    let options = GeminiAdapterOptions::new();

    // Client provides two consecutive function outputs
    let mut req = make_base_request("gemini-2.5-flash");
    req.input = vec![
        ResponseItem::FunctionCallOutput {
            id: None,
            call_id: Some("call-1".to_string()),
            name: Some("tool_a".to_string()),
            namespace: None,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text("Result 1".to_string()),
                success: Some(true),
            },
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::FunctionCallOutput {
            id: None,
            call_id: Some("call-2".to_string()),
            name: Some("tool_b".to_string()),
            namespace: None,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text("Result 2".to_string()),
                success: Some(true),
            },
            internal_chat_message_metadata_passthrough: None,
        },
    ];

    let trans = GeminiAdapter::translate_request(&req, &options, None).unwrap();
    // Must be coalesced into single user turn with 2 parts!
    assert_eq!(trans.request.contents.len(), 1);
    assert_eq!(trans.request.contents[0].role.as_deref(), Some("user"));
    assert_eq!(trans.request.contents[0].parts.len(), 2);
}

#[test]
fn test_05_strict_tools_promoted_to_validated() {
    let options = GeminiAdapterOptions::new();

    let tools_raw = serde_json::json!([
        {
            "type": "function",
            "name": "strict_checker",
            "description": "Strict function",
            "strict": true,
            "parameters": {
                "type": "object",
                "properties": { "id": { "type": "integer" } },
                "required": ["id"]
            }
        }
    ]);
    let raw = RawValue::from_string(tools_raw.to_string()).unwrap();

    let mut req = make_base_request("gemini-2.5-pro");
    req.input = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Call validate".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }];
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));
    req.tool_choice = "auto".to_string();

    let trans = GeminiAdapter::translate_request(&req, &options, None).unwrap();
    let tool_cfg = trans.request.tool_config.unwrap();
    let fc_cfg = tool_cfg.function_calling_config.unwrap();
    assert_eq!(fc_cfg.mode, Some(GeminiFunctionCallingMode::Validated));
}

#[test]
fn test_06_thinking_policy_high_roundtrip() {
    let mut options = GeminiAdapterOptions::new();
    options.thinking_policy = GeminiThinkingPolicy::LegacyBudget(8192);

    let mut req = make_base_request("gemini-2.5-pro");
    req.input = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Solve complex puzzle".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }];

    let trans = GeminiAdapter::translate_request(&req, &options, None).unwrap();
    let generation_cfg = trans.request.generation_config.unwrap();
    let think = generation_cfg.thinking_config.unwrap();
    assert_eq!(think.thinking_budget, Some(8192));
}
