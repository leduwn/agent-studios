use std::sync::Arc;

use agent_studios_protocol_adapters::gemini::{
    GeminiAdapterError, GeminiAdapterOptions, GeminiAdapterWarning, GeminiFunctionCallingMode,
    GeminiThinkingLevel, GeminiThinkingPolicy, translate_request,
};
use codex_api::{
    AccessPrograms, Reasoning, ResponsesApiRequest, ResponsesApiTools,
    create_text_param_for_request,
};
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::models::{
    ContentItem, FunctionCallOutputBody, FunctionCallOutputPayload, ImageReference, ResponseItem,
};
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::turn_input::CyberAccessProgram;
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

#[test]
fn test_01_basic_request_translation() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Hello, Gemini!".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new().with_max_output_tokens(2048);
    let result = translate_request(&req, &options, None).expect("Translation failed");

    // Model preserved verbatim, no "models/" prefix
    assert_eq!(result.model, "gemini-2.5-flash");
    assert_eq!(
        result
            .request
            .generation_config
            .as_ref()
            .and_then(|g| g.max_output_tokens),
        Some(2048)
    );
    assert_eq!(result.request.contents.len(), 1);
    assert_eq!(result.request.contents[0].role.as_deref(), Some("user"));
    assert_eq!(result.request.contents[0].parts.len(), 1);
    assert_eq!(
        result.request.contents[0].parts[0].text.as_deref(),
        Some("Hello, Gemini!")
    );
}

#[test]
fn test_02_max_output_tokens_validation() {
    let req = make_base_request();
    let options_zero = GeminiAdapterOptions::new().with_max_output_tokens(0);
    let err = translate_request(&req, &options_zero, None).expect_err("Should reject 0");
    assert_eq!(err, GeminiAdapterError::InvalidMaxOutputTokens(0));
}

#[test]
fn test_03_instructions_and_leading_system_turns() {
    let mut req = make_base_request();
    req.instructions = "You are an expert software engineer.".to_string();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "system".to_string(),
        content: vec![ContentItem::InputText {
            text: "Follow security best practices.".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Review this diff.".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new();
    let result = translate_request(&req, &options, None).expect("Translation failed");

    let sys = result
        .request
        .system_instruction
        .expect("systemInstruction missing");
    assert_eq!(sys.role, None);
    assert_eq!(sys.parts.len(), 2);
    assert_eq!(
        sys.parts[0].text.as_deref(),
        Some("You are an expert software engineer.")
    );
    assert_eq!(
        sys.parts[1].text.as_deref(),
        Some("Follow security best practices.")
    );

    assert_eq!(result.request.contents.len(), 1);
    assert_eq!(result.request.contents[0].role.as_deref(), Some("user"));
}

#[test]
fn test_04_interleaved_system_turn_rejected() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "First turn".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });
    req.input.push(ResponseItem::Message {
        id: None,
        role: "system".to_string(),
        content: vec![ContentItem::InputText {
            text: "Late system turn".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new();
    let err = translate_request(&req, &options, None).expect_err("Should fail closed");
    assert!(matches!(
        err,
        GeminiAdapterError::UnsupportedSystemHistoryPlacement(_)
    ));
}

#[test]
fn test_05_role_mapping_user_and_model() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "Ping".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });
    req.input.push(ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: "Pong".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new();
    let result = translate_request(&req, &options, None).expect("Translation failed");

    assert_eq!(result.request.contents.len(), 2);
    assert_eq!(result.request.contents[0].role.as_deref(), Some("user"));
    assert_eq!(result.request.contents[1].role.as_deref(), Some("model"));
}

#[test]
fn test_06_unsupported_role_rejected() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "moderator".to_string(),
        content: vec![ContentItem::InputText {
            text: "Halt".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new();
    let err = translate_request(&req, &options, None).expect_err("Should reject role");
    assert_eq!(
        err,
        GeminiAdapterError::UnsupportedRole("moderator".to_string())
    );
}

#[test]
fn test_07_multimodal_inline_image_and_audio() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![
            ContentItem::InputText {
                text: "Inspect image and audio".to_string(),
            },
            ContentItem::InputImage {
                image: ImageReference::Inline {
                    image_url: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string(),
                },
                detail: None,
            },
            ContentItem::InputAudio {
                audio_url: "data:audio/mp3;base64,SUQzBAAAAAAAI1RTU0UAAAAPAAADTGF2ZjU4Ljc2LjEwMAAAAAAAAAAAAAAA".to_string(),
            },
        ],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new();
    let result = translate_request(&req, &options, None).expect("Translation failed");

    assert_eq!(result.request.contents.len(), 1);
    let parts = &result.request.contents[0].parts;
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0].text.as_deref(), Some("Inspect image and audio"));

    let img_blob = parts[1].inline_data.as_ref().expect("Expected image blob");
    assert_eq!(img_blob.mime_type, "image/png");

    let audio_blob = parts[2].inline_data.as_ref().expect("Expected audio blob");
    assert_eq!(audio_blob.mime_type, "audio/mp3");
}

#[test]
fn test_08_unsupported_image_mime_rejected() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image: ImageReference::Inline {
                image_url: "data:image/bmp;base64,Qk0AAAAAAAAAAAAA".to_string(),
            },
            detail: None,
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new();
    let err = translate_request(&req, &options, None).expect_err("Should reject bmp");
    assert_eq!(
        err,
        GeminiAdapterError::UnsupportedImageMime("image/bmp".to_string())
    );
}

#[test]
fn test_09_external_image_file_rejected() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
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
    });

    let options = GeminiAdapterOptions::new();
    let err = translate_request(&req, &options, None).expect_err("Should reject file ref");
    assert!(matches!(err, GeminiAdapterError::UnsupportedContent(_)));
}

#[test]
fn test_10_external_audio_url_rejected() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputAudio {
            audio_url: "https://example.com/sound.mp3".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new();
    let err = translate_request(&req, &options, None).expect_err("Should reject external audio");
    assert!(matches!(err, GeminiAdapterError::UnsupportedContent(_)));
}

#[test]
fn test_11_coalesced_parallel_tool_outputs() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::FunctionCall {
        id: None,
        call_id: "gemini-call-1".to_string(),
        name: "read_file".to_string(),
        arguments: r#"{"path":"Cargo.toml"}"#.to_string(),
        namespace: None,
        encrypted_function_args: None,
        internal_chat_message_metadata_passthrough: None,
    });
    req.input.push(ResponseItem::FunctionCall {
        id: None,
        call_id: "gemini-call-2".to_string(),
        name: "list_dir".to_string(),
        arguments: r#"{"path":"src"}"#.to_string(),
        namespace: None,
        encrypted_function_args: None,
        internal_chat_message_metadata_passthrough: None,
    });

    req.input.push(ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("gemini-call-1".to_string()),
        name: Some("read_file".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text("[package]\nname = \"agent-studios\"".to_string()),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    });
    req.input.push(ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("gemini-call-2".to_string()),
        name: Some("list_dir".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text("lib.rs\nmain.rs".to_string()),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new();
    let result = translate_request(&req, &options, None).expect("Translation failed");

    // Model calls are coalesced into model turn, tool outputs into a single user turn
    assert_eq!(result.request.contents.len(), 2);
    assert_eq!(result.request.contents[0].role.as_deref(), Some("model"));
    assert_eq!(result.request.contents[0].parts.len(), 2);

    assert_eq!(result.request.contents[1].role.as_deref(), Some("user"));
    let user_parts = &result.request.contents[1].parts;
    assert_eq!(user_parts.len(), 2);

    let fr1 = user_parts[0]
        .function_response
        .as_ref()
        .expect("Expected functionResponse");
    assert_eq!(fr1.name, "read_file");
    assert_eq!(
        fr1.response,
        serde_json::json!({ "output": "[package]\nname = \"agent-studios\"" })
    );

    let fr2 = user_parts[1]
        .function_response
        .as_ref()
        .expect("Expected functionResponse");
    assert_eq!(fr2.name, "list_dir");
    assert_eq!(
        fr2.response,
        serde_json::json!({ "output": "lib.rs\nmain.rs" })
    );
}

#[test]
fn test_12_tool_output_error_mapping() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("call-fail".to_string()),
        name: Some("fetch".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text("404 Not Found".to_string()),
            success: Some(false),
        },
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new();
    let result = translate_request(&req, &options, None).expect("Translation failed");

    let fr = result.request.contents[0].parts[0]
        .function_response
        .as_ref()
        .expect("Expected functionResponse");
    assert_eq!(fr.response, serde_json::json!({ "error": "404 Not Found" }));
}

#[test]
fn test_13_tool_arguments_must_be_json_object() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::FunctionCall {
        id: None,
        call_id: "bad-call".to_string(),
        name: "test".to_string(),
        arguments: "\"string-not-object\"".to_string(),
        namespace: None,
        encrypted_function_args: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new();
    let err = translate_request(&req, &options, None).expect_err("Should reject non-object");
    assert!(matches!(err, GeminiAdapterError::InvalidToolArguments(_)));
}

#[test]
fn test_14_custom_tool_input_validation() {
    let mut req = make_base_request();
    req.input.push(ResponseItem::CustomToolCall {
        id: None,
        call_id: "custom-call".to_string(),
        name: "my_tool".to_string(),
        input: "[1, 2, 3]".to_string(),
        namespace: None,
        status: None,
        internal_chat_message_metadata_passthrough: None,
    });

    let options = GeminiAdapterOptions::new();
    let err = translate_request(&req, &options, None).expect_err("Should reject array input");
    assert!(matches!(
        err,
        GeminiAdapterError::UnsupportedCustomToolInput(_)
    ));
}

#[test]
fn test_15_tool_choice_none_preserves_tool_definitions() {
    let mut req = make_base_request();
    let tools_raw = serde_json::json!([
        {
            "type": "function",
            "name": "bash",
            "description": "Execute bash",
            "parameters": {
                "type": "object",
                "properties": { "cmd": { "type": "string" } },
                "required": ["cmd"]
            }
        }
    ]);
    let raw = RawValue::from_string(tools_raw.to_string()).unwrap();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));
    req.tool_choice = "none".to_string();

    let options = GeminiAdapterOptions::new();
    let result = translate_request(&req, &options, None).expect("Translation failed");

    // Tools MUST be preserved
    let tools = result.request.tools.expect("Tools must not be omitted");
    assert_eq!(tools.len(), 1);
    let declarations = tools[0]
        .function_declarations
        .as_ref()
        .expect("declarations present");
    assert_eq!(declarations[0].name, "bash");

    // Mode is NONE
    let cfg = result
        .request
        .tool_config
        .expect("tool_config must be present");
    assert_eq!(
        cfg.function_calling_config.as_ref().and_then(|c| c.mode),
        Some(GeminiFunctionCallingMode::None)
    );
}

#[test]
fn test_16_strict_tool_promotion_and_warning() {
    let mut req = make_base_request();
    let tools_raw = serde_json::json!([
        {
            "type": "function",
            "name": "strict_tool",
            "strict": true,
            "parameters": { "type": "object" }
        },
        {
            "type": "function",
            "name": "non_strict_tool",
            "strict": false,
            "parameters": { "type": "object" }
        }
    ]);
    let raw = RawValue::from_string(tools_raw.to_string()).unwrap();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));
    req.tool_choice = "auto".to_string();

    let options = GeminiAdapterOptions::new();
    let result = translate_request(&req, &options, None).expect("Translation failed");

    // Promoted to VALIDATED
    let cfg = result.request.tool_config.unwrap();
    assert_eq!(
        cfg.function_calling_config.as_ref().and_then(|c| c.mode),
        Some(GeminiFunctionCallingMode::Validated)
    );

    // Warning emitted for mixed strict scope
    assert!(
        result
            .warnings
            .contains(&GeminiAdapterWarning::StrictToolScopePromoted)
    );
}

#[test]
fn test_17_sequential_tool_calling_rejected() {
    let mut req = make_base_request();
    let tools_raw = serde_json::json!([
        {
            "type": "function",
            "name": "bash",
            "parameters": { "type": "object" }
        }
    ]);
    let raw = RawValue::from_string(tools_raw.to_string()).unwrap();
    req.tools = Some(ResponsesApiTools::from(Arc::from(raw)));
    req.tool_choice = "auto".to_string();
    req.parallel_tool_calls = false;

    let options = GeminiAdapterOptions::new();
    let err = translate_request(&req, &options, None).expect_err("Should fail closed");
    assert_eq!(err, GeminiAdapterError::SequentialToolCallingNotEnforceable);
}

#[test]
fn test_18_security_rejection_access_programs() {
    let mut req = make_base_request();
    req.access_programs = Some(AccessPrograms::from(CyberAccessProgram::Standard));

    let options = GeminiAdapterOptions::new();
    let err = translate_request(&req, &options, None).expect_err("Should reject security feature");
    assert_eq!(
        err,
        GeminiAdapterError::UnsupportedSecurityFeature("access_programs".to_string())
    );
}

#[test]
fn test_19_structured_outputs_mapping() {
    let mut req = make_base_request();
    let schema = serde_json::json!({
        "type": "object",
        "properties": { "summary": { "type": "string" } },
        "required": ["summary"]
    });
    req.text = create_text_param_for_request(None, &Some(schema.clone()), false);

    let options = GeminiAdapterOptions::new();
    let result = translate_request(&req, &options, None).expect("Translation failed");

    let generation_cfg = result
        .request
        .generation_config
        .expect("generationConfig missing");
    assert_eq!(
        generation_cfg.response_mime_type.as_deref(),
        Some("application/json")
    );
    assert_eq!(generation_cfg.response_schema, Some(schema));

    assert!(
        result
            .warnings
            .contains(&GeminiAdapterWarning::StructuredOutputNameIgnored(
                "codex_output_schema".to_string()
            ))
    );
    assert!(
        result
            .warnings
            .contains(&GeminiAdapterWarning::StructuredOutputSchemaConstrained)
    );
}

#[test]
fn test_20_thinking_policy_and_reasoning_summary() {
    let mut req = make_base_request();
    req.reasoning = Some(Reasoning {
        effort: Some(ReasoningEffort::High),
        summary: Some(ReasoningSummary::Detailed),
        context: None,
    });

    let options = GeminiAdapterOptions::new()
        .with_thinking_policy(GeminiThinkingPolicy::ExactReasoningEffort);
    let result = translate_request(&req, &options, None).expect("Translation failed");

    let generation_cfg = result.request.generation_config.unwrap();
    let thinking = generation_cfg.thinking_config.unwrap();
    assert_eq!(thinking.thinking_level, Some(GeminiThinkingLevel::High));
    assert_eq!(thinking.include_thoughts, Some(true));

    assert!(result.warnings.contains(
        &GeminiAdapterWarning::ReasoningSummaryGranularityNotRepresentable("detailed".to_string())
    ));
}
