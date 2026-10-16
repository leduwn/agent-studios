use agent_studios_external_runtime::{RuntimeEventKind, RuntimeInstanceId, RuntimeSessionId};
use agent_studios_opencode_runtime::{
    MAX_SSE_BUFFER_CAPACITY, MAX_SSE_LINE_LENGTH, SseParser, translate_opencode_event,
    translate_opencode_event_filtered,
};

#[test]
fn test_sse_parser_bounded_buffer_overflow_protection() {
    let mut parser = SseParser::new();

    // Fill buffer up to MAX_SSE_BUFFER_CAPACITY
    let large_chunk = vec![b'x'; MAX_SSE_BUFFER_CAPACITY / 2];
    let events = parser.feed(&large_chunk);
    assert!(events.is_empty());

    // Exceed capacity: parser must reset buffer instead of growing indefinitely
    let overflow_chunk = vec![b'y'; (MAX_SSE_BUFFER_CAPACITY / 2) + 1024];
    let events2 = parser.feed(&overflow_chunk);
    assert!(events2.is_empty());

    // Normal valid event should still parse cleanly afterwards
    let valid_event = b"event: message\ndata: healthy\n\n";
    let events3 = parser.feed(valid_event);
    assert_eq!(events3.len(), 1);
    assert_eq!(events3[0].event_type, "message");
    assert_eq!(events3[0].data, "healthy");
}

#[test]
fn test_sse_parser_oversized_line_discarded() {
    let mut parser = SseParser::new();

    // Construct line exceeding MAX_SSE_LINE_LENGTH
    let mut long_line = vec![b'a'; MAX_SSE_LINE_LENGTH + 10];
    long_line.push(b'\n');

    let events = parser.feed(&long_line);
    assert!(events.is_empty(), "Oversized line must be discarded");

    // Subsequent normal event parses cleanly
    let valid_event = b"event: delta\ndata: ok\n\n";
    let events2 = parser.feed(valid_event);
    assert_eq!(events2.len(), 1);
    assert_eq!(events2[0].event_type, "delta");
    assert_eq!(events2[0].data, "ok");
}

#[test]
fn test_sse_parser_last_event_id_tracking() {
    let mut parser = SseParser::new();
    assert_eq!(parser.last_event_id(), None);

    let stream = b"id: 101\nevent: message\ndata: hello\n\n";
    let events = parser.feed(stream);
    assert_eq!(events.len(), 1);
    assert_eq!(parser.last_event_id(), Some("101"));

    let stream2 = b"event: message\ndata: world\n\n";
    let events2 = parser.feed(stream2);
    assert_eq!(events2.len(), 1);
    // last_event_id persists until updated by next event with an id
    assert_eq!(parser.last_event_id(), Some("101"));

    let stream3 = b"id: 102\nevent: message\ndata: next\n\n";
    let events3 = parser.feed(stream3);
    assert_eq!(events3.len(), 1);
    assert_eq!(parser.last_event_id(), Some("102"));
}

#[test]
fn test_translate_upstream_properties_envelope() {
    let session_id = RuntimeSessionId::new();
    let instance_id = RuntimeInstanceId::generate();

    // Upstream OpenCode nests fields inside `properties`
    let upstream_json = serde_json::json!({
        "type": "message.part.delta",
        "properties": {
            "sessionID": "sess-xyz",
            "delta": "Model generated output token"
        }
    });

    let ev = translate_opencode_event_filtered(
        "message.part.delta",
        &upstream_json.to_string(),
        &session_id,
        &instance_id,
        Some("sess-xyz"),
    )
    .unwrap();

    assert_eq!(
        ev,
        Some(RuntimeEventKind::OutputDelta {
            text: "Model generated output token".to_string()
        })
    );
}

#[test]
fn test_session_id_correlation_drops_unmatched_events() {
    let session_id = RuntimeSessionId::new();
    let instance_id = RuntimeInstanceId::generate();

    let json = serde_json::json!({
        "type": "message.part.delta",
        "properties": {
            "sessionID": "sess-FOREIGN-999",
            "delta": "Leaked token"
        }
    });

    let ev = translate_opencode_event_filtered(
        "message.part.delta",
        &json.to_string(),
        &session_id,
        &instance_id,
        Some("sess-TARGET-111"),
    )
    .unwrap();

    assert_eq!(
        ev, None,
        "Event with foreign sessionID must be dropped to prevent cross-session leakage"
    );
}

#[test]
fn test_turn_completed_is_non_terminal_diagnostic() {
    let session_id = RuntimeSessionId::new();
    let instance_id = RuntimeInstanceId::generate();

    let turn_completed_json = serde_json::json!({
        "type": "turn.completed",
        "properties": {
            "sessionID": "sess-abc",
            "summary": "Turn 1 finished"
        }
    });

    let ev = translate_opencode_event(
        "turn.completed",
        &turn_completed_json.to_string(),
        &session_id,
        &instance_id,
    )
    .unwrap();

    assert!(
        matches!(ev, Some(RuntimeEventKind::Diagnostic { .. })),
        "turn.completed must NOT be mapped to terminal Completed event"
    );

    if let Some(RuntimeEventKind::Diagnostic { level, message }) = ev {
        assert_eq!(level, "debug");
        assert_eq!(message.as_str(), "Turn completed");
    }

    // Compare with explicit session completion
    let session_completed_json = serde_json::json!({
        "type": "session.status",
        "properties": {
            "sessionID": "sess-abc",
            "status": "completed"
        }
    });

    let ev_session = translate_opencode_event(
        "session.status",
        &session_completed_json.to_string(),
        &session_id,
        &instance_id,
    )
    .unwrap();

    assert_eq!(
        ev_session,
        Some(RuntimeEventKind::Completed { summary: None })
    );
}

#[test]
fn test_unhandled_event_does_not_dump_raw_json() {
    let session_id = RuntimeSessionId::new();
    let instance_id = RuntimeInstanceId::generate();

    let secret_payload = serde_json::json!({
        "type": "custom.internal.vendor.event",
        "secret_token": "SUPER_CONFIDENTIAL_TOKEN_DO_NOT_LOG",
        "details": {
            "private_key": "MIIB..."
        }
    });

    let ev = translate_opencode_event(
        "custom.internal.vendor.event",
        &secret_payload.to_string(),
        &session_id,
        &instance_id,
    )
    .unwrap();

    assert!(matches!(ev, Some(RuntimeEventKind::Diagnostic { .. })));
    if let Some(RuntimeEventKind::Diagnostic { message, .. }) = ev {
        let msg = message.as_str();
        assert!(
            !msg.contains("SUPER_CONFIDENTIAL_TOKEN_DO_NOT_LOG"),
            "Raw JSON dumped in diagnostic message"
        );
        assert!(
            !msg.contains("private_key"),
            "Raw payload leaked in diagnostic"
        );
        assert!(msg.contains("custom.internal.vendor.event"));
    }
}

#[test]
fn test_tool_execution_translation_nested_and_root() {
    let session_id = RuntimeSessionId::new();
    let instance_id = RuntimeInstanceId::generate();

    // 1. Tool started in properties
    let tool_start = serde_json::json!({
        "type": "tool.started",
        "properties": {
            "tool": "file_reader",
            "call_id": "call-44"
        }
    });

    let ev = translate_opencode_event(
        "tool.started",
        &tool_start.to_string(),
        &session_id,
        &instance_id,
    )
    .unwrap();

    assert_eq!(
        ev,
        Some(RuntimeEventKind::ToolStarted {
            tool_name: "file_reader".to_string(),
            tool_call_id: "call-44".to_string(),
        })
    );

    // 2. Tool completed in message.part.updated
    let tool_updated = serde_json::json!({
        "type": "message.part.updated",
        "properties": {
            "part": {
                "type": "tool",
                "tool": "file_reader",
                "call_id": "call-44",
                "status": "completed",
                "duration_ms": 128
            }
        }
    });

    let ev_updated = translate_opencode_event(
        "message.part.updated",
        &tool_updated.to_string(),
        &session_id,
        &instance_id,
    )
    .unwrap();

    assert_eq!(
        ev_updated,
        Some(RuntimeEventKind::ToolCompleted {
            tool_name: "file_reader".to_string(),
            tool_call_id: "call-44".to_string(),
            duration_ms: 128,
            success: true,
        })
    );
}

#[test]
fn test_permission_approval_requested_translation() {
    let session_id = RuntimeSessionId::new();
    let instance_id = RuntimeInstanceId::generate();

    let perm_json = serde_json::json!({
        "type": "permission.updated",
        "properties": {
            "permission": {
                "id": "550e8400-e29b-41d4-a716-446655440000",
                "description": "Execute bash command cargo test",
                "command": "cargo test"
            }
        }
    });

    let ev = translate_opencode_event(
        "permission.updated",
        &perm_json.to_string(),
        &session_id,
        &instance_id,
    )
    .unwrap();

    assert!(matches!(
        ev,
        Some(RuntimeEventKind::ApprovalRequested { .. })
    ));
    if let Some(RuntimeEventKind::ApprovalRequested {
        description,
        command,
        ..
    }) = ev
    {
        assert_eq!(description.as_str(), "Execute bash command cargo test");
        assert_eq!(
            command.map(|c| c.as_str().to_string()),
            Some("cargo test".to_string())
        );
    }
}
