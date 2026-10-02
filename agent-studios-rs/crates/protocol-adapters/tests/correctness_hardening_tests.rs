use std::collections::HashMap;

use agent_studios_protocol_adapters::{
    ChatAdapterError, ChatAdapterWarning, ChatChunkChoice, ChatChunkDelta, ChatChunkFunctionCall,
    ChatChunkToolCall, ChatCompletionChunk, ChatCompletionStreamTranslator, ChatCompletionsAdapter,
    ChatMessageContent, ChatUsage,
};
use codex_api::{ResponseEvent, ResponsesApiRequest};
use codex_protocol::models::{
    AgentMessageInputContent, ContentItem, FunctionCallOutputBody, FunctionCallOutputContentItem,
    FunctionCallOutputPayload, ImageDetail, ImageReference, ResponseItem,
};

fn base_request() -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: "gpt-4o".to_string(),
        stream: true,
        service_tier: None,
        instructions: "System instructions".to_string(),
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

fn make_chunk(
    id: &str,
    model: Option<&str>,
    delta: ChatChunkDelta,
    finish_reason: Option<&str>,
    usage: Option<ChatUsage>,
) -> ChatCompletionChunk {
    ChatCompletionChunk {
        id: id.to_string(),
        object: Some("chat.completion.chunk".to_string()),
        created: Some(1720000000),
        model: model.map(|m| m.to_string()),
        choices: vec![ChatChunkChoice {
            index: 0,
            delta,
            finish_reason: finish_reason.map(|s| s.to_string()),
        }],
        usage,
    }
}

// 1. finish before first chunk -> MissingResponseId
#[test]
fn test_01_finish_before_first_chunk_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let err = translator.finish().unwrap_err();
    assert_eq!(err, ChatAdapterError::MissingResponseId);
}

// 2. empty response ID -> error
#[test]
fn test_02_empty_response_id_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk = make_chunk(
        "",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: Some("Hi".to_string()),
            tool_calls: None,
        },
        None,
        None,
    );
    let err = translator.feed_chunk(&chunk).unwrap_err();
    assert_eq!(err, ChatAdapterError::MissingResponseId);
}

// 3. response ID changes mid-stream -> error
#[test]
fn test_03_response_id_changes_mid_stream_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk1 = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: Some("Hi".to_string()),
            tool_calls: None,
        },
        None,
        None,
    );
    translator.feed_chunk(&chunk1).expect("chunk 1 ok");

    let chunk2 = make_chunk(
        "resp-2",
        None,
        ChatChunkDelta {
            role: None,
            content: Some(" there".to_string()),
            tool_calls: None,
        },
        None,
        None,
    );
    let err = translator.feed_chunk(&chunk2).unwrap_err();
    assert_eq!(
        err,
        ChatAdapterError::ResponseIdMismatch {
            expected: "resp-1".to_string(),
            actual: "resp-2".to_string(),
        }
    );
}

// 4. model changes mid-stream -> error
#[test]
fn test_04_model_changes_mid_stream_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk1 = make_chunk(
        "resp-1",
        Some("gpt-4o"),
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: Some("Hi".to_string()),
            tool_calls: None,
        },
        None,
        None,
    );
    translator.feed_chunk(&chunk1).expect("chunk 1 ok");

    let chunk2 = make_chunk(
        "resp-1",
        Some("gpt-4o-mini"),
        ChatChunkDelta {
            role: None,
            content: Some(" there".to_string()),
            tool_calls: None,
        },
        None,
        None,
    );
    let err = translator.feed_chunk(&chunk2).unwrap_err();
    assert_eq!(
        err,
        ChatAdapterError::ResponseModelMismatch {
            expected: "gpt-4o".to_string(),
            actual: "gpt-4o-mini".to_string(),
        }
    );
}

// 5. duplicate feed after completion -> AlreadyCompleted
#[test]
fn test_05_duplicate_feed_after_completion_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: Some("Hi".to_string()),
            tool_calls: None,
        },
        Some("stop"),
        None,
    );
    translator.feed_chunk(&chunk).expect("feed ok");
    translator.finish().expect("finish ok");

    let duplicate = make_chunk("resp-1", None, ChatChunkDelta::default(), None, None);
    let err = translator.feed_chunk(&duplicate).unwrap_err();
    assert_eq!(err, ChatAdapterError::AlreadyCompleted);
}

// 6. duplicate finish -> AlreadyCompleted
#[test]
fn test_06_duplicate_finish_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: Some("Hi".to_string()),
            tool_calls: None,
        },
        Some("stop"),
        None,
    );
    translator.feed_chunk(&chunk).expect("feed ok");
    translator.finish().expect("finish ok");

    let err = translator.finish().unwrap_err();
    assert_eq!(err, ChatAdapterError::AlreadyCompleted);
}

// 7. [DONE] without successful finish_reason -> error
#[test]
fn test_07_done_without_successful_finish_reason_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: Some("Hi".to_string()),
            tool_calls: None,
        },
        None,
        None,
    );
    translator.feed_chunk(&chunk).expect("feed ok");

    let err = translator.feed_done().unwrap_err();
    assert_eq!(err, ChatAdapterError::MissingFinishReason);
}

// 8. stop -> end_turn Some(true)
#[test]
fn test_08_stop_finish_reason_sets_end_turn_true() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: Some("Done".to_string()),
            tool_calls: None,
        },
        Some("stop"),
        None,
    );
    translator.feed_chunk(&chunk).expect("feed ok");
    let events = translator.finish().expect("finish ok");
    assert_eq!(events.len(), 1);
    match &events[0] {
        ResponseEvent::Completed { end_turn, .. } => {
            assert_eq!(end_turn, &Some(true));
        }
        _ => panic!("Expected Completed event"),
    }
}

// 9. tool_calls -> end_turn Some(false)
#[test]
fn test_09_tool_calls_finish_reason_sets_end_turn_false() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk1 = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: Some("call_1".to_string()),
                r#type: Some("function".to_string()),
                function: Some(ChatChunkFunctionCall {
                    name: Some("test_fn".to_string()),
                    arguments: Some("{}".to_string()),
                }),
            }]),
        },
        Some("tool_calls"),
        None,
    );
    translator.feed_chunk(&chunk1).expect("feed ok");
    let events = translator.finish().expect("finish ok");
    match &events[0] {
        ResponseEvent::Completed { end_turn, .. } => {
            assert_eq!(end_turn, &Some(false));
        }
        _ => panic!("Expected Completed event"),
    }
}

// 10. legacy function_call -> end_turn Some(false)
#[test]
fn test_10_legacy_function_call_finish_reason_sets_end_turn_false() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk1 = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: Some("call_1".to_string()),
                r#type: Some("function".to_string()),
                function: Some(ChatChunkFunctionCall {
                    name: Some("legacy_fn".to_string()),
                    arguments: Some("{}".to_string()),
                }),
            }]),
        },
        Some("function_call"),
        None,
    );
    translator.feed_chunk(&chunk1).expect("feed ok");
    let events = translator.finish().expect("finish ok");
    match &events[0] {
        ResponseEvent::Completed { end_turn, .. } => {
            assert_eq!(end_turn, &Some(false));
        }
        _ => panic!("Expected Completed event"),
    }
}

// 11. tool call missing id at finish -> error
#[test]
fn test_11_tool_call_missing_id_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: None,
                r#type: Some("function".to_string()),
                function: Some(ChatChunkFunctionCall {
                    name: Some("test_fn".to_string()),
                    arguments: Some("{}".to_string()),
                }),
            }]),
        },
        Some("tool_calls"),
        None,
    );
    let err = translator.feed_chunk(&chunk).unwrap_err();
    assert_eq!(
        err,
        ChatAdapterError::IncompleteToolCall {
            index: 0,
            missing_id: true,
            missing_name: false,
        }
    );
}

// 12. tool call missing name at finish -> error
#[test]
fn test_12_tool_call_missing_name_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: Some("call_1".to_string()),
                r#type: Some("function".to_string()),
                function: Some(ChatChunkFunctionCall {
                    name: None,
                    arguments: Some("{}".to_string()),
                }),
            }]),
        },
        Some("tool_calls"),
        None,
    );
    let err = translator.feed_chunk(&chunk).unwrap_err();
    assert_eq!(
        err,
        ChatAdapterError::IncompleteToolCall {
            index: 0,
            missing_id: false,
            missing_name: true,
        }
    );
}

// 13. arguments arrive before id/name -> buffered, event order valid
#[test]
fn test_13_arguments_buffered_until_tool_identity_complete() {
    let mut translator = ChatCompletionStreamTranslator::new();

    // Chunk 1: argument fragments arrive before id or name are known
    let chunk1 = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: None,
                r#type: Some("function".to_string()),
                function: Some(ChatChunkFunctionCall {
                    name: None,
                    arguments: Some("{\"key\":".to_string()),
                }),
            }]),
        },
        None,
        None,
    );
    let events1 = translator.feed_chunk(&chunk1).expect("feed 1 ok");
    // Only Created event emitted; tool deltas must NOT be emitted before OutputItemAdded
    assert_eq!(events1.len(), 1);
    assert!(matches!(&events1[0], ResponseEvent::Created { .. }));

    // Chunk 2: id and name arrive
    let chunk2 = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: None,
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: Some("call_abc".to_string()),
                r#type: None,
                function: Some(ChatChunkFunctionCall {
                    name: Some("my_tool".to_string()),
                    arguments: Some("\"val\"}".to_string()),
                }),
            }]),
        },
        Some("tool_calls"),
        None,
    );
    let events2 = translator.feed_chunk(&chunk2).expect("feed 2 ok");
    // Expected: OutputItemAdded, ToolCallInputDelta for buffered fragment, ToolCallInputDelta for new fragment, OutputItemDone
    assert_eq!(events2.len(), 4);
    assert!(matches!(
        &events2[0],
        ResponseEvent::OutputItemAdded(ResponseItem::FunctionCall { call_id, name, .. }) if call_id == "call_abc" && name == "my_tool"
    ));
    assert!(matches!(
        &events2[1],
        ResponseEvent::ToolCallInputDelta { delta, .. } if delta == "{\"key\":"
    ));
    assert!(matches!(
        &events2[2],
        ResponseEvent::ToolCallInputDelta { delta, .. } if delta == "\"val\"}"
    ));
    assert!(matches!(
        &events2[3],
        ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { call_id, arguments, .. }) if call_id == "call_abc" && arguments == "{\"key\":\"val\"}"
    ));
}

// 14. tool call ID changes -> error
#[test]
fn test_14_tool_call_id_changes_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk1 = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: Some("call_A".to_string()),
                r#type: Some("function".to_string()),
                function: Some(ChatChunkFunctionCall {
                    name: Some("test_fn".to_string()),
                    arguments: None,
                }),
            }]),
        },
        None,
        None,
    );
    translator.feed_chunk(&chunk1).expect("feed 1 ok");

    let chunk2 = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: None,
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: Some("call_B".to_string()),
                r#type: None,
                function: None,
            }]),
        },
        None,
        None,
    );
    let err = translator.feed_chunk(&chunk2).unwrap_err();
    assert!(matches!(err, ChatAdapterError::ToolCallIdentityMismatch(_)));
}

// 15. function name changes -> error
#[test]
fn test_15_tool_call_name_changes_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk1 = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: Some("call_A".to_string()),
                r#type: Some("function".to_string()),
                function: Some(ChatChunkFunctionCall {
                    name: Some("fn_A".to_string()),
                    arguments: None,
                }),
            }]),
        },
        None,
        None,
    );
    translator.feed_chunk(&chunk1).expect("feed 1 ok");

    let chunk2 = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: None,
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: None,
                r#type: None,
                function: Some(ChatChunkFunctionCall {
                    name: Some("fn_B".to_string()),
                    arguments: None,
                }),
            }]),
        },
        None,
        None,
    );
    let err = translator.feed_chunk(&chunk2).unwrap_err();
    assert!(matches!(err, ChatAdapterError::ToolCallIdentityMismatch(_)));
}

// 16. unsupported tool-call type -> error
#[test]
fn test_16_unsupported_tool_call_type_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: Some("call_1".to_string()),
                r#type: Some("custom_non_function".to_string()),
                function: Some(ChatChunkFunctionCall {
                    name: Some("test_fn".to_string()),
                    arguments: Some("{}".to_string()),
                }),
            }]),
        },
        None,
        None,
    );
    let err = translator.feed_chunk(&chunk).unwrap_err();
    assert_eq!(
        err,
        ChatAdapterError::UnsupportedToolCallType("custom_non_function".to_string())
    );
}

// 17. two choices in same chunk -> error
#[test]
fn test_17_two_choices_in_chunk_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk = ChatCompletionChunk {
        id: "resp-1".to_string(),
        object: None,
        created: None,
        model: None,
        choices: vec![
            ChatChunkChoice {
                index: 0,
                delta: ChatChunkDelta::default(),
                finish_reason: None,
            },
            ChatChunkChoice {
                index: 1,
                delta: ChatChunkDelta::default(),
                finish_reason: None,
            },
        ],
        usage: None,
    };
    let err = translator.feed_chunk(&chunk).unwrap_err();
    assert_eq!(err, ChatAdapterError::MultipleChoicesUnsupported(2));
}

// 18. negative token usage -> error
#[test]
fn test_18_negative_token_usage_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();
    let chunk = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: Some("Hi".to_string()),
            tool_calls: None,
        },
        Some("stop"),
        Some(ChatUsage {
            prompt_tokens: -10,
            completion_tokens: 5,
            total_tokens: -5,
            prompt_tokens_details: None,
            completion_tokens_details: None,
        }),
    );
    let err = translator.feed_chunk(&chunk).unwrap_err();
    assert!(matches!(err, ChatAdapterError::InvalidUsage(_)));
}

// 19. text-only tool output succeeds
#[test]
fn test_19_text_only_tool_output_succeeds() {
    let mut req = base_request();
    req.input = vec![ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("call_123".to_string()),
        name: Some("read_file".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::ContentItems(vec![
                FunctionCallOutputContentItem::InputText {
                    text: "Line 1".to_string(),
                },
                FunctionCallOutputContentItem::InputText {
                    text: "Line 2".to_string(),
                },
            ]),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    }];
    let translation = ChatCompletionsAdapter::translate_request(&req).expect("translate ok");
    let tool_msg = &translation.request.messages[1];
    assert_eq!(tool_msg.role, "tool");
    assert_eq!(tool_msg.tool_call_id.as_deref(), Some("call_123"));
    assert_eq!(
        tool_msg.content,
        Some(ChatMessageContent::Text("Line 1\nLine 2".to_string()))
    );
}

// 20. text + image tool output rejects without loss
#[test]
fn test_20_text_and_image_tool_output_rejects_without_loss() {
    let mut req = base_request();
    req.input = vec![ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("call_123".to_string()),
        name: Some("screenshot".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::ContentItems(vec![
                FunctionCallOutputContentItem::InputText {
                    text: "Screenshot captured:".to_string(),
                },
                FunctionCallOutputContentItem::InputImage {
                    image: ImageReference::Inline {
                        image_url: "data:image/png;base64,abc".to_string(),
                    },
                    detail: Some(ImageDetail::Auto),
                },
            ]),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    }];
    let err = ChatCompletionsAdapter::translate_request(&req).unwrap_err();
    assert!(matches!(
        err,
        ChatAdapterError::UnsupportedToolOutputContent(_)
    ));
}

// 21. image-only tool output rejects
#[test]
fn test_21_image_only_tool_output_rejects() {
    let mut req = base_request();
    req.input = vec![ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("call_123".to_string()),
        name: Some("screenshot".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::ContentItems(vec![
                FunctionCallOutputContentItem::InputImage {
                    image: ImageReference::Inline {
                        image_url: "data:image/png;base64,abc".to_string(),
                    },
                    detail: None,
                },
            ]),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    }];
    let err = ChatCompletionsAdapter::translate_request(&req).unwrap_err();
    assert!(matches!(
        err,
        ChatAdapterError::UnsupportedToolOutputContent(_)
    ));
}

// 22. audio tool output rejects
#[test]
fn test_22_audio_tool_output_rejects() {
    let mut req = base_request();
    req.input = vec![ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("call_123".to_string()),
        name: Some("record_audio".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::ContentItems(vec![
                FunctionCallOutputContentItem::InputAudio {
                    audio_url: "https://example.com/audio.mp3".to_string(),
                },
            ]),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    }];
    let err = ChatCompletionsAdapter::translate_request(&req).unwrap_err();
    assert!(matches!(
        err,
        ChatAdapterError::UnsupportedToolOutputContent(_)
    ));
}

// 23. encrypted tool output rejects
#[test]
fn test_23_encrypted_tool_output_rejects() {
    let mut req = base_request();
    req.input = vec![ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("call_123".to_string()),
        name: Some("secret_tool".to_string()),
        namespace: None,
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::ContentItems(vec![
                FunctionCallOutputContentItem::EncryptedContent {
                    encrypted_content: "enc_data".to_string(),
                },
            ]),
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    }];
    let err = ChatCompletionsAdapter::translate_request(&req).unwrap_err();
    assert!(matches!(
        err,
        ChatAdapterError::UnsupportedToolOutputContent(_)
    ));
}

// 24. encrypted AgentMessage rejects
#[test]
fn test_24_encrypted_agent_message_rejects() {
    let mut req = base_request();
    req.input = vec![ResponseItem::AgentMessage {
        id: None,
        author: "sub-agent".to_string(),
        recipient: "main".to_string(),
        content: vec![AgentMessageInputContent::EncryptedContent {
            encrypted_content: "secret_agent_data".to_string(),
        }],
        internal_chat_message_metadata_passthrough: None,
    }];
    let err = ChatCompletionsAdapter::translate_request(&req).unwrap_err();
    assert_eq!(err, ChatAdapterError::UnsupportedEncryptedAgentMessage);
}

// 25. unknown ResponseItem rejects
#[test]
fn test_25_unknown_response_item_rejects() {
    let mut req = base_request();
    req.input = vec![ResponseItem::WebSearchCall {
        id: None,
        status: Some("completed".to_string()),
        action: None,
        internal_chat_message_metadata_passthrough: None,
    }];
    let err = ChatCompletionsAdapter::translate_request(&req).unwrap_err();
    assert!(matches!(err, ChatAdapterError::UnsupportedResponseItem(_)));
}

// 26. invalid message role rejects
#[test]
fn test_26_invalid_message_role_rejects() {
    let mut req = base_request();
    req.input = vec![ResponseItem::Message {
        id: None,
        role: "moderator".to_string(),
        content: vec![ContentItem::InputText {
            text: "Hello".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }];
    let err = ChatCompletionsAdapter::translate_request(&req).unwrap_err();
    assert_eq!(err, ChatAdapterError::InvalidRole("moderator".to_string()));
}

// 27. Original image detail emits warning
#[test]
fn test_27_original_image_detail_emits_warning() {
    let mut req = base_request();
    req.input = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image: ImageReference::Inline {
                image_url: "data:image/png;base64,xyz".to_string(),
            },
            detail: Some(ImageDetail::Original),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }];
    let translation = ChatCompletionsAdapter::translate_request(&req).expect("translate ok");
    assert!(
        translation
            .warnings
            .iter()
            .any(|w| matches!(w, ChatAdapterWarning::NormalizedImageDetail { from, to } if from == "original" && to == "high"))
    );
}

// 28. unknown tool_choice rejects
#[test]
fn test_28_unknown_tool_choice_rejects() {
    let mut req = base_request();
    req.tool_choice = "unsupported_choice".to_string();
    let err = ChatCompletionsAdapter::translate_request(&req).unwrap_err();
    assert_eq!(
        err,
        ChatAdapterError::UnsupportedToolChoice("unsupported_choice".to_string())
    );
}

// 29. text-before-tools then illegal resumed text path is deterministic
#[test]
fn test_29_resumed_text_after_tools_fails() {
    let mut translator = ChatCompletionStreamTranslator::new();

    // Chunk 1: Text begins
    let chunk1 = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: Some("Calling tool:".to_string()),
            tool_calls: None,
        },
        None,
        None,
    );
    translator.feed_chunk(&chunk1).expect("chunk 1 ok");

    // Chunk 2: Tool call begins (closes text message)
    let chunk2 = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: None,
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: Some("call_1".to_string()),
                r#type: Some("function".to_string()),
                function: Some(ChatChunkFunctionCall {
                    name: Some("test_tool".to_string()),
                    arguments: Some("{}".to_string()),
                }),
            }]),
        },
        None,
        None,
    );
    translator.feed_chunk(&chunk2).expect("chunk 2 ok");

    // Chunk 3: Illegal resumed text delta
    let chunk3 = make_chunk(
        "resp-1",
        None,
        ChatChunkDelta {
            role: None,
            content: Some("More text after tool".to_string()),
            tool_calls: None,
        },
        None,
        None,
    );
    let err = translator.feed_chunk(&chunk3).unwrap_err();
    assert!(matches!(err, ChatAdapterError::InvalidStreamState(_)));
}

// 30. OutputItemAdded/Delta/Done ordering exactly once
#[test]
fn test_30_exactly_once_item_lifecycle() {
    let mut translator = ChatCompletionStreamTranslator::new();

    let chunk1 = make_chunk(
        "resp-lifecycle",
        None,
        ChatChunkDelta {
            role: Some("assistant".to_string()),
            content: Some("Thinking...".to_string()),
            tool_calls: None,
        },
        None,
        None,
    );
    let events1 = translator.feed_chunk(&chunk1).expect("chunk 1");

    let chunk2 = make_chunk(
        "resp-lifecycle",
        None,
        ChatChunkDelta {
            role: None,
            content: None,
            tool_calls: Some(vec![ChatChunkToolCall {
                index: 0,
                id: Some("call_tool_0".to_string()),
                r#type: Some("function".to_string()),
                function: Some(ChatChunkFunctionCall {
                    name: Some("my_fn".to_string()),
                    arguments: Some("{\"a\":1}".to_string()),
                }),
            }]),
        },
        Some("tool_calls"),
        None,
    );
    let events2 = translator.feed_chunk(&chunk2).expect("chunk 2");

    let events3 = translator.finish().expect("finish");

    let mut all_events = Vec::new();
    all_events.extend(events1);
    all_events.extend(events2);
    all_events.extend(events3);

    // Track lifecycle counts per item ID
    let mut added_counts = HashMap::new();
    let mut done_counts = HashMap::new();
    let mut completed_count = 0;

    for event in &all_events {
        match event {
            ResponseEvent::OutputItemAdded(item) => {
                let id = match item {
                    ResponseItem::Message { id, .. } => id.as_ref().unwrap().to_string(),
                    ResponseItem::FunctionCall { id, .. } => id.as_ref().unwrap().to_string(),
                    _ => panic!("Unexpected item"),
                };
                *added_counts.entry(id).or_insert(0) += 1;
            }
            ResponseEvent::OutputItemDone(item) => {
                let id = match item {
                    ResponseItem::Message { id, .. } => id.as_ref().unwrap().to_string(),
                    ResponseItem::FunctionCall { id, .. } => id.as_ref().unwrap().to_string(),
                    _ => panic!("Unexpected item"),
                };
                *done_counts.entry(id).or_insert(0) += 1;
            }
            ResponseEvent::Completed { .. } => {
                completed_count += 1;
            }
            _ => {}
        }
    }

    // Verify exactly-once lifecycle
    assert_eq!(completed_count, 1, "Completed must be emitted exactly once");

    let msg_id = "chat-msg-resp-lifecycle";
    let tool_id = "chat-tool-resp-lifecycle-0";

    assert_eq!(
        added_counts.get(msg_id),
        Some(&1),
        "Message OutputItemAdded exactly once"
    );
    assert_eq!(
        done_counts.get(msg_id),
        Some(&1),
        "Message OutputItemDone exactly once"
    );

    assert_eq!(
        added_counts.get(tool_id),
        Some(&1),
        "Tool OutputItemAdded exactly once"
    );
    assert_eq!(
        done_counts.get(tool_id),
        Some(&1),
        "Tool OutputItemDone exactly once"
    );
}
