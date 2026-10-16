use agent_studios_external_runtime::{RuntimeEventKind, RuntimeInstanceId, RuntimeSessionId};
use agent_studios_opencode_runtime::{SseParser, translate_opencode_event};

#[test]
fn test_sse_parser_single_event() {
    let mut parser = SseParser::new();
    let data = b"event: output.delta\ndata: {\"text\": \"Hello, world!\"}\nid: 1\n\n";
    let events = parser.feed(data);

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "output.delta");
    assert_eq!(events[0].data, "{\"text\": \"Hello, world!\"}");
    assert_eq!(events[0].id, Some("1".to_string()));
}

#[test]
fn test_sse_parser_chunked_streaming() {
    let mut parser = SseParser::new();
    // Feed in small arbitrary chunk fragments
    assert!(parser.feed(b"event: message").is_empty());
    assert!(parser.feed(b"\ndata: chunk 1").is_empty());
    assert!(parser.feed(b"\ndata: chunk 2\n").is_empty());

    let events = parser.feed(b"\n");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "message");
    assert_eq!(events[0].data, "chunk 1\nchunk 2");
}

#[test]
fn test_sse_parser_crlf_and_comments() {
    let mut parser = SseParser::new();
    let payload = b": keep-alive comment\r\nevent: ping\r\ndata: pong\r\n\r\n";
    let events = parser.feed(payload);

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "ping");
    assert_eq!(events[0].data, "pong");
}

#[test]
fn test_translate_opencode_session_created() {
    let session_id = RuntimeSessionId::new();
    let instance_id = RuntimeInstanceId::generate();

    let json = serde_json::json!({
        "type": "session.created",
        "id": "opencode-sess-1"
    });

    let res = translate_opencode_event(
        "session.created",
        &json.to_string(),
        &session_id,
        &instance_id,
    )
    .unwrap();

    assert_eq!(
        res,
        Some(RuntimeEventKind::SessionStarted {
            session_id,
            instance_id,
        })
    );
}

#[test]
fn test_translate_opencode_output_delta() {
    let session_id = RuntimeSessionId::new();
    let instance_id = RuntimeInstanceId::generate();

    let json = serde_json::json!({
        "type": "output.delta",
        "text": "Streaming tokens..."
    });

    let res =
        translate_opencode_event("output.delta", &json.to_string(), &session_id, &instance_id)
            .unwrap();

    assert_eq!(
        res,
        Some(RuntimeEventKind::OutputDelta {
            text: "Streaming tokens...".to_string(),
        })
    );
}

#[test]
fn test_translate_opencode_tool_execution() {
    let session_id = RuntimeSessionId::new();
    let instance_id = RuntimeInstanceId::generate();

    let start_json = serde_json::json!({
        "type": "tool.started",
        "tool": "bash",
        "call_id": "call_123"
    });

    let start_event = translate_opencode_event(
        "tool.started",
        &start_json.to_string(),
        &session_id,
        &instance_id,
    )
    .unwrap();

    assert_eq!(
        start_event,
        Some(RuntimeEventKind::ToolStarted {
            tool_name: "bash".to_string(),
            tool_call_id: "call_123".to_string(),
        })
    );

    let comp_json = serde_json::json!({
        "type": "tool.completed",
        "tool": "bash",
        "call_id": "call_123",
        "duration_ms": 42,
        "success": true
    });

    let comp_event = translate_opencode_event(
        "tool.completed",
        &comp_json.to_string(),
        &session_id,
        &instance_id,
    )
    .unwrap();

    assert_eq!(
        comp_event,
        Some(RuntimeEventKind::ToolCompleted {
            tool_name: "bash".to_string(),
            tool_call_id: "call_123".to_string(),
            duration_ms: 42,
            success: true,
        })
    );
}

#[test]
fn test_translate_opencode_approval_request() {
    let session_id = RuntimeSessionId::new();
    let instance_id = RuntimeInstanceId::generate();

    let json = serde_json::json!({
        "type": "approval.requested",
        "description": "Execute rm -rf /tmp/test",
        "command": "rm -rf /tmp/test"
    });

    let res = translate_opencode_event(
        "approval.requested",
        &json.to_string(),
        &session_id,
        &instance_id,
    )
    .unwrap();

    assert!(matches!(
        res,
        Some(RuntimeEventKind::ApprovalRequested { description, command, .. })
        if description.as_str() == "Execute rm -rf /tmp/test"
        && command.as_ref().map(|c| c.as_str()) == Some("rm -rf /tmp/test")
    ));
}

#[test]
fn test_translate_opencode_terminal_events() {
    let session_id = RuntimeSessionId::new();
    let instance_id = RuntimeInstanceId::generate();

    // Turn completed is diagnostic, not terminal (M11-R02)
    let turn_json = serde_json::json!({ "type": "turn.completed", "summary": "Done" });
    let turn_event = translate_opencode_event(
        "turn.completed",
        &turn_json.to_string(),
        &session_id,
        &instance_id,
    )
    .unwrap();
    assert!(matches!(
        turn_event,
        Some(RuntimeEventKind::Diagnostic { level, .. }) if level == "debug"
    ));

    // Completed
    let comp_json = serde_json::json!({ "type": "session.status", "status": "completed" });
    let comp_event = translate_opencode_event(
        "session.status",
        &comp_json.to_string(),
        &session_id,
        &instance_id,
    )
    .unwrap();
    assert_eq!(
        comp_event,
        Some(RuntimeEventKind::Completed { summary: None })
    );

    // Failed
    let fail_json = serde_json::json!({ "type": "failed", "error": "Context window exceeded" });
    let fail_event =
        translate_opencode_event("failed", &fail_json.to_string(), &session_id, &instance_id)
            .unwrap();
    assert!(
        matches!(fail_event, Some(RuntimeEventKind::Failed { safe_error_summary }) if safe_error_summary.as_str() == "Context window exceeded")
    );

    // Interrupted
    let abort_json = serde_json::json!({ "type": "aborted", "reason": "User abort" });
    let abort_event = translate_opencode_event(
        "aborted",
        &abort_json.to_string(),
        &session_id,
        &instance_id,
    )
    .unwrap();
    assert!(
        matches!(abort_event, Some(RuntimeEventKind::Interrupted { reason }) if reason.as_str() == "User abort")
    );

    // Stopped
    let stop_json = serde_json::json!({ "type": "stopped" });
    let stop_event =
        translate_opencode_event("stopped", &stop_json.to_string(), &session_id, &instance_id)
            .unwrap();
    assert_eq!(stop_event, Some(RuntimeEventKind::Stopped));
}
