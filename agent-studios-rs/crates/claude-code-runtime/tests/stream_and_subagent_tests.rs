use std::io::Cursor;
use agent_studios_claude_code_runtime::mapping::map_claude_message;
use agent_studios_claude_code_runtime::protocol::{
    AssistantMessage, AssistantMessageInner, CanUseToolRequest, ClaudeStreamMessage,
    ClaudeTaskUsage, ClaudeTokenUsage, ContentBlock, ControlRequest, ControlRequestPayload,
    ResultSummary, StreamDelta, StreamEventPayload, SystemMessage, TaskNotificationMessage,
    TaskStartedMessage, ToolResultBlock, UserMessage, UserMessageInner,
};
use agent_studios_claude_code_runtime::stream::{NdjsonStreamReader, parse_ndjson_line};
use agent_studios_claude_code_runtime::subagent::SubagentCorrelationTracker;
use agent_studios_external_runtime::{
    RuntimeEventKind, RuntimeImplementationId, RuntimeInstanceId, RuntimeSessionId,
    RuntimeSessionRef,
};

fn test_session_ref() -> RuntimeSessionRef {
    RuntimeSessionRef::new(
        RuntimeImplementationId::new("claude-code").unwrap(),
        RuntimeInstanceId::from_uuid(uuid::Uuid::new_v4()),
        RuntimeSessionId::new(),
    )
}

// ============================================================================
// 1. NDJSON Line Parsing & Bounded Streaming Tests
// ============================================================================

#[test]
fn test_parse_ndjson_line_valid_events() {
    let raw_init = r#"{"type":"system","subtype":"init","claude_code_version":"2.1.310","model":"claude-sonnet-5"}"#;
    let msg = parse_ndjson_line(raw_init).expect("should parse valid init message");
    match msg {
        ClaudeStreamMessage::System(SystemMessage::Init(init)) => {
            assert_eq!(init.claude_code_version.as_deref(), Some("2.1.310"));
            assert_eq!(init.model.as_deref(), Some("claude-sonnet-5"));
        }
        other => panic!("Unexpected parsed message: {other:?}"),
    }

    let raw_stream = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello World"}}}"#;
    let msg = parse_ndjson_line(raw_stream).expect("should parse valid stream event");
    match msg {
        ClaudeStreamMessage::StreamEvent(wrapper) => match wrapper.event {
            StreamEventPayload::ContentBlockDelta { delta, index } => {
                assert_eq!(index, 0);
                assert_eq!(
                    delta,
                    StreamDelta::TextDelta {
                        text: "Hello World".to_string()
                    }
                );
            }
            other => panic!("Unexpected payload: {other:?}"),
        },
        other => panic!("Unexpected parsed message: {other:?}"),
    }
}

#[test]
fn test_parse_ndjson_line_invalid_json_returns_error() {
    assert!(parse_ndjson_line("").is_err());
    assert!(parse_ndjson_line("{not json}").is_err());
}

#[tokio::test]
async fn test_ndjson_stream_reader_read_message_sequence() {
    let input = concat!(
        "{\"type\":\"system\",\"subtype\":\"init\",\"claude_code_version\":\"2.1.310\"}\n",
        "\n",
        "   \n",
        "{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Chunk 1\"}}}\n",
        "{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Chunk 2\"}}}\n",
        "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1}\n"
    );

    let cursor = Cursor::new(input.as_bytes());
    let mut reader = NdjsonStreamReader::new(cursor);

    let msg1 = reader.read_message().await.unwrap().expect("msg1 expected");
    assert!(matches!(msg1, ClaudeStreamMessage::System(SystemMessage::Init(_))));

    let msg2 = reader.read_message().await.unwrap().expect("msg2 expected");
    assert!(matches!(msg2, ClaudeStreamMessage::StreamEvent(_)));

    let msg3 = reader.read_message().await.unwrap().expect("msg3 expected");
    assert!(matches!(msg3, ClaudeStreamMessage::StreamEvent(_)));

    let msg4 = reader.read_message().await.unwrap().expect("msg4 expected");
    assert!(matches!(msg4, ClaudeStreamMessage::Result(_)));

    let eof = reader.read_message().await.unwrap();
    assert!(eof.is_none(), "Stream should return None at EOF");
}

#[tokio::test]
async fn test_ndjson_stream_reader_rejects_overlong_line() {
    // Construct line exceeding 16 MiB (16 * 1024 * 1024 + 32 bytes)
    let oversize = 16 * 1024 * 1024 + 32;
    let mut big_bytes = Vec::with_capacity(oversize);
    big_bytes.extend(b"{\"type\":\"system\",\"padding\":\"");
    big_bytes.resize(oversize - 4, b'A');
    big_bytes.extend(b"\"}\n");

    let cursor = Cursor::new(big_bytes);
    let mut reader = NdjsonStreamReader::new(cursor);

    let result = reader.read_message().await;
    assert!(result.is_err(), "Must reject line exceeding MAX_LINE_LENGTH (16 MiB)");
}

// ============================================================================
// 2. Subagent Correlation DAG & Non-Synthetic Hierarchy Tests
// ============================================================================

#[test]
fn test_subagent_correlation_tracker_hierarchical_dag() {
    let mut tracker = SubagentCorrelationTracker::new();

    // Root Turn invokes Agent tool call: toolu_agent_01 -> Task ID: task_alpha
    let task_alpha = TaskStartedMessage {
        uuid: Some("uuid-1".to_string()),
        session_id: Some("session-1".to_string()),
        task_id: "task_alpha".to_string(),
        tool_use_id: "toolu_agent_01".to_string(),
        description: "Root exploration subagent".to_string(),
        task_type: Some("subagent".to_string()),
        is_backgrounded: Some(false),
        spawn_depth: Some(1),
    };

    let node1 = tracker.register_task_started(&task_alpha);
    assert_eq!(node1.task_id, "task_alpha");
    assert_eq!(node1.tool_use_id, "toolu_agent_01");
    assert_eq!(node1.spawn_depth, 1);
    assert!(tracker.is_active("task_alpha"));

    // Subagent alpha invokes child Agent tool call: toolu_agent_02 -> Task ID: task_beta
    let task_beta = TaskStartedMessage {
        uuid: Some("uuid-2".to_string()),
        session_id: Some("session-1".to_string()),
        task_id: "task_beta".to_string(),
        tool_use_id: "toolu_agent_02".to_string(),
        description: "Nested code reviewer subagent".to_string(),
        task_type: Some("subagent".to_string()),
        is_backgrounded: Some(false),
        spawn_depth: Some(2),
    };
    tracker.register_task_started(&task_beta);
    // Link beta to parent tool call toolu_agent_01
    tracker.set_parent_tool_use_id("toolu_agent_02", "toolu_agent_01");

    // Subagent beta invokes grandchild Agent tool call: toolu_agent_03 -> Task ID: task_gamma
    let task_gamma = TaskStartedMessage {
        uuid: Some("uuid-3".to_string()),
        session_id: Some("session-1".to_string()),
        task_id: "task_gamma".to_string(),
        tool_use_id: "toolu_agent_03".to_string(),
        description: "Leaf security scanner subagent".to_string(),
        task_type: Some("subagent".to_string()),
        is_backgrounded: Some(true),
        spawn_depth: Some(3),
    };
    tracker.register_task_started(&task_gamma);
    tracker.set_parent_tool_use_id("toolu_agent_03", "toolu_agent_02");

    // Verify Active counts
    assert_eq!(tracker.active_subagents().len(), 3);
    assert_eq!(tracker.all_subagents().len(), 3);

    // Verify Ancestry Chain from leaf task_gamma (toolu_agent_03)
    let ancestry = tracker.resolve_ancestry("toolu_agent_03");
    assert_eq!(ancestry.len(), 3);
    assert_eq!(ancestry[0].task_id, "task_gamma");
    assert_eq!(ancestry[1].task_id, "task_beta");
    assert_eq!(ancestry[2].task_id, "task_alpha");

    // Complete task_gamma
    let complete_gamma = TaskNotificationMessage {
        uuid: Some("uuid-4".to_string()),
        session_id: Some("session-1".to_string()),
        task_id: "task_gamma".to_string(),
        status: "completed".to_string(),
        output_file: Some("/tmp/gamma_output.txt".to_string()),
        summary: Some("Scan completed with 0 warnings".to_string()),
        usage: Some(ClaudeTaskUsage {
            total_tokens: Some(1500),
            tool_uses: Some(4),
            duration_ms: Some(2500),
        }),
    };
    let completed_node = tracker
        .register_task_notification(&complete_gamma)
        .expect("should complete node");
    assert!(completed_node.success);
    assert_eq!(completed_node.status.as_deref(), Some("completed"));
    assert!(!tracker.is_active("task_gamma"));
    assert_eq!(tracker.active_subagents().len(), 2);
}

// ============================================================================
// 3. Normalized Wire Event Mapping Tests
// ============================================================================

#[test]
fn test_mapping_assistant_tool_use_and_user_result() {
    let session_ref = test_session_ref();
    let mut tracker = SubagentCorrelationTracker::new();

    // 1. Assistant message with ToolUse
    let assistant_msg = ClaudeStreamMessage::Assistant(AssistantMessage {
        uuid: Some("uuid-ast".to_string()),
        session_id: Some("session-1".to_string()),
        parent_tool_use_id: None,
        agent_id: None,
        user_message_uuid: None,
        message: AssistantMessageInner {
            id: Some("msg_01".to_string()),
            role: Some("assistant".to_string()),
            model: Some("claude-sonnet-5".to_string()),
            content: vec![
                ContentBlock::Text {
                    text: "Calling bash...".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "toolu_bash_01".to_string(),
                    name: "Bash".to_string(),
                    input: serde_json::json!({"command": "cargo test"}),
                },
            ],
            stop_reason: Some("tool_use".to_string()),
            usage: Some(ClaudeTokenUsage {
                input_tokens: Some(100),
                output_tokens: Some(25),
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            }),
        },
    });

    let events = map_claude_message(&assistant_msg, &session_ref, &mut tracker);
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0],
        RuntimeEventKind::ToolStarted {
            tool_name: "Bash".to_string(),
            tool_call_id: "toolu_bash_01".to_string(),
        }
    );
    assert_eq!(
        events[1],
        RuntimeEventKind::UsageUpdated {
            prompt_tokens: Some(100),
            completion_tokens: Some(25),
            total_tokens: Some(125),
        }
    );

    // 2. User message with ToolResult
    let user_msg = ClaudeStreamMessage::User(UserMessage {
        uuid: Some("uuid-usr".to_string()),
        session_id: Some("session-1".to_string()),
        parent_tool_use_id: None,
        message: Some(UserMessageInner {
            role: Some("user".to_string()),
            content: vec![ToolResultBlock::ToolResult {
                tool_use_id: "toolu_bash_01".to_string(),
                content: serde_json::json!("test result: ok. 10 passed"),
                is_error: Some(false),
            }],
        }),
        tool_use_result: None,
    });

    let res_events = map_claude_message(&user_msg, &session_ref, &mut tracker);
    assert_eq!(res_events.len(), 1);
    assert_eq!(
        res_events[0],
        RuntimeEventKind::ToolCompleted {
            tool_name: "Bash".to_string(),
            tool_call_id: "toolu_bash_01".to_string(),
            duration_ms: 0,
            success: true,
        }
    );
}

#[test]
fn test_mapping_control_request_approval() {
    let session_ref = test_session_ref();
    let mut tracker = SubagentCorrelationTracker::new();

    let control_msg = ClaudeStreamMessage::ControlRequest(ControlRequest {
        request_id: "req_perm_01".to_string(),
        request: ControlRequestPayload::CanUseTool(CanUseToolRequest {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({"command": "rm -rf target/"}),
            tool_use_id: Some("toolu_bash_99".to_string()),
            agent_id: None,
            decision_reason: Some("Removing build directory".to_string()),
            blocked_path: None,
            default_to_no: Some(false),
            suggestions: None,
        }),
    });

    let events = map_claude_message(&control_msg, &session_ref, &mut tracker);
    assert_eq!(events.len(), 1);
    match &events[0] {
        RuntimeEventKind::ApprovalRequested {
            approval_id,
            description,
            command,
        } => {
            assert!(!approval_id.to_string().is_empty());
            assert_eq!(description.as_str(), "Removing build directory");
            assert_eq!(command.as_ref().map(|c| c.as_str()), Some("rm -rf target/"));
        }
        other => panic!("Expected ApprovalRequested, got {other:?}"),
    }
}

#[test]
fn test_mapping_result_summary() {
    let session_ref = test_session_ref();
    let mut tracker = SubagentCorrelationTracker::new();

    let result_msg = ClaudeStreamMessage::Result(ResultSummary {
        subtype: Some("success".to_string()),
        uuid: Some("uuid-res".to_string()),
        session_id: Some("session-1".to_string()),
        result: Some("Done with task".to_string()),
        duration_ms: Some(1500),
        duration_api_ms: Some(1200),
        is_error: false,
        errors: None,
        num_turns: Some(2),
        stop_reason: Some("end_turn".to_string()),
        total_cost_usd: Some(0.0125),
        usage: Some(ClaudeTokenUsage {
            input_tokens: Some(500),
            output_tokens: Some(120),
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
        }),
        model_usage: None,
        permission_denials: None,
        terminal_reason: None,
    });

    let events = map_claude_message(&result_msg, &session_ref, &mut tracker);
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0],
        RuntimeEventKind::Diagnostic {
            level: "info".to_string(),
            message: "Turn completed: Done with task".into(),
        }
    );
    assert_eq!(
        events[1],
        RuntimeEventKind::UsageUpdated {
            prompt_tokens: Some(500),
            completion_tokens: Some(120),
            total_tokens: Some(620),
        }
    );
}
