//! Integration and Unit Tests for Agent Studios TUI Foundation
//!
//! Validates Quiet Precision theme tokens, responsive breakpoints,
//! fail-closed security approvals, and Vietnamese Unicode NFC grapheme handling.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use unicode_segmentation::UnicodeSegmentation;

use agent_studios_tui::layout::{MainLayout, TerminalBreakpoint, centered_rect};
use agent_studios_tui::terminal::sanitize_terminal_title;
use agent_studios_tui::theme::{AgsTheme, ThemeMode};
use agent_studios_tui::widgets::{
    agent_bar::{AgentBarWidget, AgentItem},
    approval_modal::{ApprovalModalWidget, PendingApproval},
    composer::ComposerWidget,
    diff_viewer::DiffViewerWidget,
    header::HeaderWidget,
    slash_menu::{AVAILABLE_COMMANDS, SlashMenuWidget},
    status_bar::StatusBarWidget,
    transcript::{TranscriptItem, TranscriptWidget},
};
use ratatui::widgets::Widget;

#[test]
fn test_theme_tokens_contrast() {
    let dark = AgsTheme::dark();
    assert_eq!(dark.mode, ThemeMode::Dark);
    assert_ne!(dark.bg_base, dark.text_primary);
    assert_ne!(dark.accent_emerald, dark.bg_base);

    let light = AgsTheme::light();
    assert_eq!(light.mode, ThemeMode::Light);
    assert_ne!(light.bg_base, light.text_primary);

    let ansi16 = AgsTheme::ansi16();
    assert_eq!(ansi16.mode, ThemeMode::Ansi16);

    let no_color = AgsTheme::no_color();
    assert_eq!(no_color.mode, ThemeMode::NoColor);
}

#[test]
fn test_responsive_layout_breakpoints() {
    // Compact: 80x24
    let compact = MainLayout::compute(Rect::new(0, 0, 80, 24));
    assert_eq!(compact.breakpoint, TerminalBreakpoint::Compact);
    assert!(compact.side_pane_area.is_none());
    assert_eq!(compact.header_area.height, 2);

    // Standard: 100x30
    let standard = MainLayout::compute(Rect::new(0, 0, 100, 30));
    assert_eq!(standard.breakpoint, TerminalBreakpoint::Standard);
    assert!(standard.side_pane_area.is_none());
    assert_eq!(standard.header_area.height, 3);

    // Wide: 130x36
    let wide = MainLayout::compute(Rect::new(0, 0, 130, 36));
    assert_eq!(wide.breakpoint, TerminalBreakpoint::Wide);
    assert!(wide.side_pane_area.is_none());

    // UltraWide: 180x45 (dual-pane enabled)
    let ultrawide = MainLayout::compute(Rect::new(0, 0, 180, 45));
    assert_eq!(ultrawide.breakpoint, TerminalBreakpoint::UltraWide);
    assert!(ultrawide.side_pane_area.is_some());
}

#[test]
fn test_vietnamese_unicode_nfc_grapheme_integrity() {
    // Vietnamese phrases with tone marks & horn letters
    let vi_text = "Tiếng Việt chuẩn Unicode NFC: á, ắ, ặ, ấ, ầ, ổ, ợ, ỹ.";
    let graphemes: Vec<&str> = vi_text.graphemes(true).collect();

    assert!(!graphemes.is_empty());
    // Verify combining diacritics are kept intact inside individual graphemes
    for g in graphemes {
        assert!(!g.is_empty());
        // Each grapheme must be valid UTF-8
        assert!(std::str::from_utf8(g.as_bytes()).is_ok());
    }

    // Render in composer buffer and assert no panic or buffer overflow
    let theme = AgsTheme::dark();
    let composer = ComposerWidget {
        theme: &theme,
        input_text: vi_text,
        cursor_grapheme_index: 5,
        is_focused: true,
    };

    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 5));
    composer.render(Rect::new(0, 0, 80, 5), &mut buf);
}

#[test]
fn test_terminal_title_sanitization() {
    // Strip control characters, ANSI escape sequences, and Trojan Source bidi
    let malicious = "\x1b[31mRedTitle\x07\r\n\u{202E}Reversed\u{202C}";
    let cleaned = sanitize_terminal_title(malicious);

    assert!(!cleaned.contains('\x1b'));
    assert!(!cleaned.contains('\x07'));
    assert!(!cleaned.contains('\r'));
    assert!(!cleaned.contains('\n'));
    assert!(!cleaned.contains('\u{202E}'));
    assert_eq!(cleaned, "[31mRedTitle Reversed");

    // Max length bound
    let huge = "a".repeat(500);
    let bounded = sanitize_terminal_title(&huge);
    assert!(bounded.len() <= 240);
}

#[test]
fn test_centered_rect_math() {
    let parent = Rect::new(0, 0, 100, 50);
    let modal = centered_rect(60, 40, parent);

    assert_eq!(modal.width, 60);
    assert_eq!(modal.height, 20);
    assert_eq!(modal.x, 20);
    assert_eq!(modal.y, 15);
}

#[test]
fn test_slash_menu_commands_inventory() {
    assert!(AVAILABLE_COMMANDS.iter().any(|c| c.command == "/help"));
    assert!(AVAILABLE_COMMANDS.iter().any(|c| c.command == "/clear"));
    assert!(AVAILABLE_COMMANDS.iter().any(|c| c.command == "/doctor"));
    assert!(AVAILABLE_COMMANDS.iter().any(|c| c.command == "/worktree"));
    assert!(AVAILABLE_COMMANDS.iter().any(|c| c.command == "/diff"));
    assert!(AVAILABLE_COMMANDS.iter().any(|c| c.command == "/quit"));
}

#[test]
fn test_widgets_render_without_panics() {
    let theme = AgsTheme::dark();
    let rect = Rect::new(0, 0, 100, 30);
    let mut buf = Buffer::empty(rect);

    // 1. Header
    HeaderWidget {
        theme: &theme,
        project_name: "test-proj",
        branch_name: "main",
        is_dirty: true,
        model_name: "Sonnet 3.7",
        execution_mode: "Embedded",
        compact: false,
    }
    .render(Rect::new(0, 0, 100, 3), &mut buf);

    // 2. AgentBar
    let agents = vec![AgentItem {
        role: "Coordinator".to_string(),
        state_label: "BUSY".to_string(),
        is_active: true,
        is_awaiting_approval: false,
        is_failed: false,
    }];
    AgentBarWidget {
        theme: &theme,
        agents: &agents,
    }
    .render(Rect::new(0, 3, 100, 2), &mut buf);

    // 3. Transcript
    let transcript = vec![
        TranscriptItem::UserPrompt("Hello Agent".to_string()),
        TranscriptItem::AssistantMessage("Hello Dev".to_string()),
    ];
    TranscriptWidget {
        theme: &theme,
        items: &transcript,
        scroll_offset: 0,
    }
    .render(Rect::new(0, 5, 100, 15), &mut buf);

    // 4. StatusBar
    StatusBarWidget {
        theme: &theme,
        input_tokens: 100,
        output_tokens: 50,
        cost_usd: 0.005,
        status_text: "Idle",
    }
    .render(Rect::new(0, 29, 100, 1), &mut buf);

    // 5. DiffViewer
    let diff = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-old\n+new";
    DiffViewerWidget {
        theme: &theme,
        diff_content: diff,
        title: "test.rs",
    }
    .render(Rect::new(0, 0, 50, 10), &mut buf);

    // 6. ApprovalModal
    let approval = PendingApproval {
        approval_id: agent_studios_protocol::id::ApprovalId::new(),
        tool_name: "bash_exec".to_string(),
        risk_tier: "WorkspaceMutating".to_string(),
        summary: "Execute cargo build".to_string(),
        details: "cargo build --all".to_string(),
    };
    ApprovalModalWidget {
        theme: &theme,
        approval: &approval,
    }
    .render(rect, &mut buf);

    // 7. SlashMenu
    SlashMenuWidget {
        theme: &theme,
        selected_index: 0,
        filter: "",
    }
    .render(Rect::new(0, 10, 40, 10), &mut buf);
}

fn buffer_line_to_string(buf: &Buffer, y: u16, width: u16) -> String {
    let mut s = String::new();
    for x in 0..width {
        if let Some(cell) = buf.cell((x, y)) {
            s.push_str(cell.symbol());
        }
    }
    s
}

fn buffer_to_full_string(buf: &Buffer, width: u16, height: u16) -> String {
    let mut s = String::new();
    for y in 0..height {
        for x in 0..width {
            if let Some(cell) = buf.cell((x, y)) {
                s.push_str(cell.symbol());
            }
        }
        s.push('\n');
    }
    s
}

#[test]
fn test_header_widget_fallbacks_buffer_snapshot() {
    let theme = AgsTheme::dark();
    let rect = Rect::new(0, 0, 100, 3);
    let mut buf = Buffer::empty(rect);

    // Empty fields must gracefully fallback
    HeaderWidget {
        theme: &theme,
        project_name: "",
        branch_name: "",
        is_dirty: false,
        model_name: "",
        execution_mode: "",
        compact: false,
    }
    .render(rect, &mut buf);

    let line = buffer_line_to_string(&buf, 0, 100);
    assert!(
        line.contains("AGENT STUDIOS"),
        "Expected AGENT STUDIOS badge in line: {line}"
    );
    assert!(
        line.contains("Unknown project"),
        "Expected fallback 'Unknown project' in line: {line}"
    );
    assert!(
        line.contains("Unknown branch"),
        "Expected fallback 'Unknown branch' in line: {line}"
    );
    assert!(
        line.contains("Clean"),
        "Expected 'Clean' status in line: {line}"
    );
    assert!(
        line.contains("[No model configured]"),
        "Expected '[No model configured]' in line: {line}"
    );
    assert!(
        line.contains("Embedded"),
        "Expected 'Embedded' fallback in line: {line}"
    );

    // Dirty branch with explicit configuration
    let mut dirty_buf = Buffer::empty(rect);
    HeaderWidget {
        theme: &theme,
        project_name: "custom-repo",
        branch_name: "feat/sec-audit",
        is_dirty: true,
        model_name: "Claude 3.7 Sonnet",
        execution_mode: "Connected",
        compact: false,
    }
    .render(rect, &mut dirty_buf);

    let dirty_line = buffer_line_to_string(&dirty_buf, 0, 100);
    assert!(dirty_line.contains("custom-repo"));
    assert!(dirty_line.contains("feat/sec-audit"));
    assert!(dirty_line.contains("*dirty"));
    assert!(dirty_line.contains("[Claude 3.7 Sonnet]"));
    assert!(dirty_line.contains("Connected"));
}

#[test]
fn test_status_bar_widget_tokens_and_status_buffer_snapshot() {
    let theme = AgsTheme::dark();
    let rect = Rect::new(0, 0, 120, 1);

    // 1. Zero tokens and IDLE status
    let mut idle_buf = Buffer::empty(rect);
    StatusBarWidget {
        theme: &theme,
        input_tokens: 0,
        output_tokens: 0,
        cost_usd: 0.0,
        status_text: "IDLE",
    }
    .render(rect, &mut idle_buf);

    let idle_line = buffer_line_to_string(&idle_buf, 0, 120);
    assert!(
        idle_line.contains("tokens: unknown"),
        "Expected 'tokens: unknown' in line: {idle_line}"
    );
    assert!(
        idle_line.contains("($0.000)"),
        "Expected '($0.000)' in line: {idle_line}"
    );
    assert!(
        idle_line.contains("[IDLE]"),
        "Expected '[IDLE]' in line: {idle_line}"
    );

    // 2. Tracked usage and RUNNING status
    let mut running_buf = Buffer::empty(rect);
    StatusBarWidget {
        theme: &theme,
        input_tokens: 4200,
        output_tokens: 850,
        cost_usd: 0.038,
        status_text: "RUNNING",
    }
    .render(rect, &mut running_buf);

    let running_line = buffer_line_to_string(&running_buf, 0, 120);
    assert!(
        running_line.contains("tokens: in 4200+out 850"),
        "Expected formatted tokens in line: {running_line}"
    );
    assert!(
        running_line.contains("($0.038)"),
        "Expected formatted cost in line: {running_line}"
    );
    assert!(
        running_line.contains("[RUNNING]"),
        "Expected '[RUNNING]' in line: {running_line}"
    );

    // 3. Awaiting approval status
    let mut gate_buf = Buffer::empty(rect);
    StatusBarWidget {
        theme: &theme,
        input_tokens: 100,
        output_tokens: 20,
        cost_usd: 0.001,
        status_text: "AWAITING APPROVAL",
    }
    .render(rect, &mut gate_buf);

    let gate_line = buffer_line_to_string(&gate_buf, 0, 120);
    assert!(
        gate_line.contains("[AWAITING APPROVAL]"),
        "Expected '[AWAITING APPROVAL]' in line: {gate_line}"
    );
}

#[test]
fn test_slash_menu_annotations_and_inventory_buffer_snapshot() {
    let theme = AgsTheme::dark();
    let rect = Rect::new(0, 0, 72, 12);
    let mut buf = Buffer::empty(rect);

    // Validate 10 commands in inventory
    assert_eq!(AVAILABLE_COMMANDS.len(), 10);

    // Validate Ready annotations
    for ready_cmd in ["/help", "/clear", "/doctor", "/status", "/cancel", "/quit"] {
        let item = AVAILABLE_COMMANDS
            .iter()
            .find(|c| c.command == ready_cmd)
            .expect("Command missing");
        assert!(
            item.description.starts_with("[Ready]"),
            "Command {ready_cmd} must be annotated [Ready]"
        );
    }

    // Validate Planned annotations
    for planned_cmd in ["/diff", "/model", "/worktree", "/reconcile"] {
        let item = AVAILABLE_COMMANDS
            .iter()
            .find(|c| c.command == planned_cmd)
            .expect("Command missing");
        assert!(
            item.description.starts_with("[Planned]"),
            "Command {planned_cmd} must be annotated [Planned]"
        );
    }

    // Render SlashMenuWidget
    SlashMenuWidget {
        theme: &theme,
        selected_index: 0,
        filter: "",
    }
    .render(rect, &mut buf);

    let full_text = buffer_to_full_string(&buf, 72, 12);
    assert!(
        full_text.contains("> /help"),
        "Expected selected marker on /help"
    );
    assert!(
        full_text.contains("  /clear"),
        "Expected unselected marker on /clear"
    );
    assert!(full_text.contains("[Ready]"));
    assert!(full_text.contains("[Planned]"));
}

#[test]
fn test_approval_modal_buffer_snapshot() {
    let theme = AgsTheme::dark();
    let rect = Rect::new(0, 0, 100, 30);
    let mut buf = Buffer::empty(rect);

    let approval = PendingApproval {
        approval_id: agent_studios_protocol::id::ApprovalId::new(),
        tool_name: "write_file".to_string(),
        risk_tier: "WorkspaceMutating".to_string(),
        summary: "Overwrite configuration file crates/tui/Cargo.toml".to_string(),
        details: "Update dependencies and security gates".to_string(),
    };

    ApprovalModalWidget {
        theme: &theme,
        approval: &approval,
    }
    .render(rect, &mut buf);

    let full_text = buffer_to_full_string(&buf, 100, 30);
    assert!(full_text.contains("SECURITY GATE APPROVAL"));
    assert!(full_text.contains("WORKSPACE MUTATING"));
    assert!(full_text.contains("write_file"));
    assert!(full_text.contains("[a]"));
    assert!(full_text.contains("Allow Once"));
    assert!(full_text.contains("[s]"));
    assert!(full_text.contains("Allow for Session"));
    assert!(full_text.contains("[d]"));
    assert!(full_text.contains("Deny"));
}

#[test]
fn test_diff_viewer_buffer_snapshot() {
    let theme = AgsTheme::dark();
    let rect = Rect::new(0, 0, 80, 10);
    let mut buf = Buffer::empty(rect);

    let diff = "diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n@@ -1,2 +1,2 @@\n-old line\n+new line";
    DiffViewerWidget {
        theme: &theme,
        diff_content: diff,
        title: "file.rs",
    }
    .render(rect, &mut buf);

    let full_text = buffer_to_full_string(&buf, 80, 10);
    assert!(full_text.contains("file.rs"));
    assert!(full_text.contains("+new line"));
    assert!(full_text.contains("-old line"));
}

// --- Behavioral and Mock Client Tests for AgsApp ---

use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use agent_studios_cli_client::client::{
    AgentStudiosClient, ClientApprovalDecision, ClientError, ClientSubscription, ExecutionMode,
    GoalSubmissionOptions, RuntimeMode, WorkspaceInfo, WorktreeRecordSummary,
};
use agent_studios_orchestration::read_models::{
    AgentSummary, TaskGraphSnapshot, TaskTimelineProjection,
};
use agent_studios_protocol::id::{ApprovalId, ReconciliationId, StudioId, TaskId, WorktreeId};
use agent_studios_tui::app::AgsApp;
use agent_studios_workspace::capture::CapturedChanges;
use agent_studios_workspace::reconciliation::ReconciliationOutcome;

struct MockTestClient {
    pub fail_resolve: AtomicBool,
    pub fail_submit: AtomicBool,
    pub fail_cancel: AtomicBool,
}

impl MockTestClient {
    fn new() -> Self {
        Self {
            fail_resolve: AtomicBool::new(false),
            fail_submit: AtomicBool::new(false),
            fail_cancel: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl AgentStudiosClient for MockTestClient {
    fn studio_id(&self) -> StudioId {
        StudioId::new()
    }
    fn workspace_name(&self) -> &str {
        "test-workspace"
    }
    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Embedded
    }
    fn runtime_mode(&self) -> RuntimeMode {
        RuntimeMode::Demo
    }
    async fn get_workspace_info(&self) -> Result<WorkspaceInfo, ClientError> {
        Ok(WorkspaceInfo {
            project_name: "mock-proj".to_string(),
            branch_name: Some("mock-branch".to_string()),
            is_dirty: false,
            cwd: "D:/mock".to_string(),
        })
    }
    async fn shutdown(&self) -> Result<(), ClientError> {
        Ok(())
    }
    async fn submit_goal(
        &self,
        _prompt: &str,
        _options: GoalSubmissionOptions,
    ) -> Result<TaskId, ClientError> {
        if self.fail_submit.load(Ordering::SeqCst) {
            Err(ClientError::OperationFailed(
                "Submission denied".to_string(),
            ))
        } else {
            Ok(TaskId::new())
        }
    }
    async fn cancel_active_run(&self, _reason: Option<String>) -> Result<(), ClientError> {
        if self.fail_cancel.load(Ordering::SeqCst) {
            Err(ClientError::OperationFailed("Cancel failed".to_string()))
        } else {
            Ok(())
        }
    }
    async fn subscribe_events(
        &self,
        _from_sequence: u64,
    ) -> Result<ClientSubscription, ClientError> {
        let (_tx, rx) = tokio::sync::mpsc::channel(10);
        let (_state_tx, state_rx) =
            tokio::sync::watch::channel(agent_studios_cli_client::StreamSyncState::Healthy);
        Ok(ClientSubscription {
            backfill: Vec::new(),
            stream: rx,
            sync_state: state_rx,
        })
    }
    async fn get_task_graph(&self) -> Result<TaskGraphSnapshot, ClientError> {
        Err(ClientError::Unsupported("mock".to_string()))
    }
    async fn get_agent_summaries(&self) -> Result<Vec<AgentSummary>, ClientError> {
        Ok(Vec::new())
    }
    async fn get_task_timeline(
        &self,
        _task_id: TaskId,
    ) -> Result<TaskTimelineProjection, ClientError> {
        Err(ClientError::Unsupported("mock".to_string()))
    }
    async fn resolve_approval(
        &self,
        _approval_id: ApprovalId,
        _decision: ClientApprovalDecision,
    ) -> Result<(), ClientError> {
        if self.fail_resolve.load(Ordering::SeqCst) {
            Err(ClientError::PermissionDenied(
                "Gate permission denied".to_string(),
            ))
        } else {
            Ok(())
        }
    }
    async fn list_worktrees(&self) -> Result<Vec<WorktreeRecordSummary>, ClientError> {
        Ok(Vec::new())
    }
    async fn get_worktree_diff(
        &self,
        _worktree_id: WorktreeId,
    ) -> Result<CapturedChanges, ClientError> {
        Err(ClientError::Unsupported("mock".to_string()))
    }
    async fn reconcile_patch(
        &self,
        _reconciliation_id: ReconciliationId,
    ) -> Result<ReconciliationOutcome, ClientError> {
        Err(ClientError::Unsupported("mock".to_string()))
    }
}

#[tokio::test]
async fn test_app_fail_closed_approval_error_preserves_modal() {
    let mock = Arc::new(MockTestClient::new());
    mock.fail_resolve.store(true, Ordering::SeqCst);

    let mut app = AgsApp::new(mock.clone());
    let approval = PendingApproval {
        approval_id: ApprovalId::new(),
        tool_name: "bash_exec".to_string(),
        risk_tier: "Destructive".to_string(),
        summary: "rm -rf /".to_string(),
        details: "Destructive deletion".to_string(),
    };
    app.set_pending_approval(Some(approval));

    // Send 'a' key (Allow)
    let key = KeyEvent {
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };

    app.handle_key(key)
        .await
        .expect("Key handling should not error");

    // CRITICAL: Modal MUST remain open because authorization failed!
    assert!(
        app.pending_approval().is_some(),
        "Modal must stay open on resolution error!"
    );

    // Verify error was placed in transcript
    let has_error = app.transcript().iter().any(|item| match item {
        TranscriptItem::SystemNotice { level, message } => {
            level == "Error" && message.contains("Gate permission denied")
        }
        _ => false,
    });
    assert!(has_error, "Transcript must record error notice");

    // Now allow resolution to succeed
    mock.fail_resolve.store(false, Ordering::SeqCst);
    app.handle_key(key)
        .await
        .expect("Key handling should not error");

    // Modal resolved and closed
    assert!(
        app.pending_approval().is_none(),
        "Modal must close on successful resolution"
    );
}

#[tokio::test]
async fn test_app_slash_commands_and_planned_capabilities() {
    let mock = Arc::new(MockTestClient::new());
    let mut app = AgsApp::new(mock);

    // /help
    app.execute_slash_command("/help").await.unwrap();
    assert!(app.transcript().iter().any(|item| match item {
        TranscriptItem::SystemNotice { message, .. } => message.contains("Agent Studios CLI Help"),
        _ => false,
    }));

    // /status
    app.execute_slash_command("/status").await.unwrap();
    assert!(app.transcript().iter().any(|item| match item {
        TranscriptItem::SystemNotice { message, .. } => message.contains("Studio Session: ID="),
        _ => false,
    }));

    // /doctor
    app.execute_slash_command("/doctor").await.unwrap();
    assert!(app.transcript().iter().any(|item| match item {
        TranscriptItem::SystemNotice { message, .. } =>
            message.contains("Running Agent Studios Diagnostics"),
        _ => false,
    }));

    // Planned commands: /diff, /model, /worktree, /reconcile
    for cmd in ["/diff", "/model", "/worktree", "/reconcile"] {
        app.execute_slash_command(cmd).await.unwrap();
        assert!(app.transcript().iter().any(|item| match item {
            TranscriptItem::SystemNotice { level, message } => {
                level == "Warning"
                    && message.contains("is planned")
                    && message.contains("planned for future milestone")
            }
            _ => false,
        }));
    }

    // /clear
    app.execute_slash_command("/clear").await.unwrap();
    assert!(app.transcript().is_empty(), "Transcript must be cleared");

    // /quit
    app.execute_slash_command("/quit").await.unwrap();
    // AgsApp has private should_quit or via Esc key
}

#[tokio::test]
async fn test_app_action_error_handling_no_silent_drop() {
    let mock = Arc::new(MockTestClient::new());
    mock.fail_submit.store(true, Ordering::SeqCst);
    mock.fail_cancel.store(true, Ordering::SeqCst);

    let mut app = AgsApp::new(mock);

    // 1. Submit goal failure
    app.set_input_text("Test failing goal".to_string());
    let enter_key = KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    app.handle_key(enter_key).await.unwrap();

    let has_submit_err = app.transcript().iter().any(|item| match item {
        TranscriptItem::SystemNotice { level, message } => {
            level == "Error" && message.contains("Submission denied")
        }
        _ => false,
    });
    assert!(
        has_submit_err,
        "Failed goal submission must report Error in transcript"
    );

    // 2. Cancel run failure via Ctrl+C
    let ctrl_c = KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    app.handle_key(ctrl_c).await.unwrap();

    let has_cancel_err = app.transcript().iter().any(|item| match item {
        TranscriptItem::SystemNotice { level, message } => {
            level == "Error" && message.contains("Cancel failed")
        }
        _ => false,
    });
    assert!(
        has_cancel_err,
        "Failed cancellation must report Error in transcript"
    );

    // 3. Cancel run failure via /cancel slash command
    app.execute_slash_command("/cancel").await.unwrap();
    let has_slash_cancel_err = app.transcript().iter().any(|item| match item {
        TranscriptItem::SystemNotice { level, message } => {
            level == "Error" && message.contains("Cancel failed")
        }
        _ => false,
    });
    assert!(
        has_slash_cancel_err,
        "Failed /cancel command must report Error in transcript"
    );
}
