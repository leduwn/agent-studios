use std::collections::HashMap;
use std::sync::Arc;

use agent_studios_protocol_adapters::{
    ChatAdapterError, ChatAdapterWarning, ChatCompletionsAdapter, ChatContentPart,
    ChatMessageContent, ChatToolChoice,
};
use codex_api::{
    AccessPrograms, ResponsesApiRequest, ResponsesApiTools, create_text_param_for_request,
};
use codex_protocol::config_types::Verbosity;
use codex_protocol::models::{
    ContentItem, FunctionCallOutputBody, FunctionCallOutputPayload, ImageDetail, ImageReference,
    ResponseItem,
};
use codex_protocol::turn_input::CyberAccessProgram;
use serde_json::json;

fn create_base_request() -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: "gpt-4o-mini".to_string(),
        stream: true,
        service_tier: None,
        instructions: "You are a helpful assistant.".to_string(),
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
fn test_instructions_and_message_history() {
    let mut req = create_base_request();
    req.instructions = "Core system instructions".to_string();
    req.input = vec![
        ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: "Dev instruction".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "Hello".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
    ];

    let translation = ChatCompletionsAdapter::translate_request(&req).expect("translation success");
    let chat_req = translation.request;

    assert_eq!(chat_req.model, "gpt-4o-mini");
    assert_eq!(chat_req.messages.len(), 3);

    // Message 0: Leading system message from instructions
    assert_eq!(chat_req.messages[0].role, "system");
    assert_eq!(
        chat_req.messages[0].content,
        Some(ChatMessageContent::Text(
            "Core system instructions".to_string()
        ))
    );

    // Message 1: Developer role normalized to system role
    assert_eq!(chat_req.messages[1].role, "system");
    assert_eq!(
        chat_req.messages[1].content,
        Some(ChatMessageContent::Text("Dev instruction".to_string()))
    );

    // Message 2: User message
    assert_eq!(chat_req.messages[2].role, "user");
    assert_eq!(
        chat_req.messages[2].content,
        Some(ChatMessageContent::Text("Hello".to_string()))
    );
}

#[test]
fn test_multimodal_message_translation() {
    let mut req = create_base_request();
    req.input = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![
            ContentItem::InputText {
                text: "Describe this image:".to_string(),
            },
            ContentItem::InputImage {
                image: ImageReference::Inline {
                    image_url: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string(),
                },
                detail: Some(ImageDetail::High),
            },
        ],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }];

    let translation = ChatCompletionsAdapter::translate_request(&req).expect("translation success");
    let user_msg = &translation.request.messages[1]; // after instructions
    assert_eq!(user_msg.role, "user");

    match &user_msg.content {
        Some(ChatMessageContent::Parts(parts)) => {
            assert_eq!(parts.len(), 2);
            assert!(
                matches!(&parts[0], ChatContentPart::Text { text } if text == "Describe this image:")
            );
            assert!(matches!(
                &parts[1],
                ChatContentPart::ImageUrl { image_url } if image_url.url.starts_with("data:image/png;base64,") && image_url.detail.as_deref() == Some("high")
            ));
        }
        _ => panic!("Expected multimodal Parts content"),
    }
}

#[test]
fn test_file_image_and_audio_rejected() {
    let mut req_file = create_base_request();
    req_file.input = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image: ImageReference::File {
                file_id: "file-xyz".to_string(),
            },
            detail: None,
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }];

    let err = ChatCompletionsAdapter::translate_request(&req_file).unwrap_err();
    assert!(matches!(err, ChatAdapterError::UnsupportedContent(_)));

    let mut req_audio = create_base_request();
    req_audio.input = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputAudio {
            audio_url: "https://example.com/audio.mp3".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }];

    let err = ChatCompletionsAdapter::translate_request(&req_audio).unwrap_err();
    assert!(matches!(err, ChatAdapterError::UnsupportedContent(_)));
}

#[test]
fn test_access_programs_security_rejection() {
    let mut req = create_base_request();
    req.access_programs = Some(AccessPrograms::from(CyberAccessProgram::Standard));

    let err = ChatCompletionsAdapter::translate_request(&req).unwrap_err();
    assert!(matches!(
        err,
        ChatAdapterError::UnsupportedSecurityFeature(_)
    ));
}

#[test]
fn test_tools_schema_flat_to_nested_translation() {
    let mut req = create_base_request();
    let flat_tools = json!([
        {
            "type": "function",
            "name": "lookup_user",
            "description": "Look up a user record",
            "strict": true,
            "parameters": {
                "type": "object",
                "properties": {
                    "user_id": { "type": "string" }
                },
                "required": ["user_id"]
            }
        }
    ]);
    let raw = serde_json::value::to_raw_value(&flat_tools).unwrap();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));

    let translation = ChatCompletionsAdapter::translate_request(&req).expect("translation success");
    let chat_tools = translation.request.tools.expect("tools present");
    assert_eq!(chat_tools.len(), 1);

    let tool = &chat_tools[0];
    assert_eq!(tool.r#type, "function");
    assert_eq!(tool.function.name, "lookup_user");
    assert_eq!(
        tool.function.description.as_deref(),
        Some("Look up a user record")
    );
    assert_eq!(tool.function.strict, Some(true));
    assert!(tool.function.parameters.is_some());
}

#[test]
fn test_unsupported_tool_type_rejected() {
    let mut req = create_base_request();
    let web_search_tools = json!([
        {
            "type": "web_search",
            "description": "Search the web"
        }
    ]);
    let raw = serde_json::value::to_raw_value(&web_search_tools).unwrap();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));

    let err = ChatCompletionsAdapter::translate_request(&req).unwrap_err();
    assert!(matches!(err, ChatAdapterError::UnsupportedToolType(_)));
}

#[test]
fn test_function_call_and_output_correlation() {
    let mut req = create_base_request();
    req.input = vec![
        ResponseItem::FunctionCall {
            id: None,
            name: "execute_command".to_string(),
            namespace: None,
            arguments: "{\"cmd\":\"ls\"}".to_string(),
            encrypted_function_args: None,
            call_id: "call_abc123".to_string(),
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::FunctionCallOutput {
            id: None,
            call_id: Some("call_abc123".to_string()),
            name: Some("execute_command".to_string()),
            namespace: None,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text("file1.txt\nfile2.txt".to_string()),
                success: Some(true),
            },
            internal_chat_message_metadata_passthrough: None,
        },
    ];

    let translation = ChatCompletionsAdapter::translate_request(&req).expect("translation success");
    let messages = &translation.request.messages;

    // Message 1: Assistant with tool_calls
    let asst = &messages[1];
    assert_eq!(asst.role, "assistant");
    let tool_calls = asst.tool_calls.as_ref().expect("tool_calls");
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].id, "call_abc123");
    assert_eq!(tool_calls[0].function.name, "execute_command");
    assert_eq!(tool_calls[0].function.arguments, "{\"cmd\":\"ls\"}");

    // Message 2: Tool message with tool_call_id
    let tool_msg = &messages[2];
    assert_eq!(tool_msg.role, "tool");
    assert_eq!(tool_msg.tool_call_id.as_deref(), Some("call_abc123"));
    assert_eq!(
        tool_msg.content,
        Some(ChatMessageContent::Text("file1.txt\nfile2.txt".to_string()))
    );
}

#[test]
fn test_missing_tool_call_id_rejected() {
    let mut req = create_base_request();
    req.input = vec![ResponseItem::FunctionCallOutput {
        id: None,
        call_id: None,
        name: Some("foo".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text("result".to_string()),
            success: None,
        },
        internal_chat_message_metadata_passthrough: None,
    }];

    let err = ChatCompletionsAdapter::translate_request(&req).unwrap_err();
    assert!(matches!(err, ChatAdapterError::MissingToolCallId(_)));
}

#[test]
fn test_structured_output_response_format() {
    let mut req = create_base_request();
    req.text = create_text_param_for_request(
        Some(Verbosity::High),
        &Some(json!({
            "type": "object",
            "properties": {
                "status": { "type": "string" }
            }
        })),
        true,
    );

    let translation = ChatCompletionsAdapter::translate_request(&req).expect("translation success");
    let resp_format = translation
        .request
        .response_format
        .expect("response_format present");

    assert_eq!(resp_format.r#type, "json_schema");
    let schema_format = resp_format.json_schema.expect("json_schema format");
    assert_eq!(schema_format.name, "codex_output_schema");
    assert_eq!(schema_format.strict, Some(true));
    assert_eq!(schema_format.strict, Some(true));

    // Verbosity dropped warning emitted
    assert!(
        translation
            .warnings
            .iter()
            .any(|w| matches!(w, ChatAdapterWarning::DroppedVerbosity(_)))
    );
}

#[test]
fn test_dropped_responses_only_warnings() {
    let mut req = create_base_request();
    req.prompt_cache_key = Some("cache-key-123".to_string());
    req.include = vec!["message.input_tokens".to_string()];
    let mut meta = HashMap::new();
    meta.insert("client".to_string(), "agent-studios".to_string());
    req.client_metadata = Some(meta);
    req.store = true;
    req.service_tier = Some("scale".to_string());

    let translation = ChatCompletionsAdapter::translate_request(&req).expect("translation success");
    let warnings = translation.warnings;

    assert!(
        warnings
            .iter()
            .any(|w| matches!(w, ChatAdapterWarning::DroppedPromptCacheKey(_)))
    );
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w, ChatAdapterWarning::DroppedInclude(_)))
    );
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w, ChatAdapterWarning::DroppedClientMetadata(_)))
    );
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w, ChatAdapterWarning::DroppedStore(true)))
    );
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w, ChatAdapterWarning::DroppedServiceTier(_)))
    );
}

#[test]
fn test_tool_choice_mapping() {
    let mut req = create_base_request();
    req.tool_choice = "none".to_string();
    let t = ChatCompletionsAdapter::translate_request(&req).unwrap();
    assert_eq!(t.request.tool_choice, Some(ChatToolChoice::None));

    req.tool_choice = "required".to_string();
    let t = ChatCompletionsAdapter::translate_request(&req).unwrap();
    assert_eq!(t.request.tool_choice, Some(ChatToolChoice::Required));

    req.tool_choice = "custom_fn".to_string();
    let t = ChatCompletionsAdapter::translate_request(&req).unwrap();
    assert_eq!(
        t.request.tool_choice,
        Some(ChatToolChoice::Function("custom_fn".to_string()))
    );
}
