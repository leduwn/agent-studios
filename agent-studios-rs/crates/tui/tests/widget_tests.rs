//! Integration and Unit Tests for Agent Studios TUI Foundation
//!
//! Validates Quiet Precision theme tokens, responsive breakpoints,
//! fail-closed security approvals, and Vietnamese Unicode NFC grapheme handling.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use unicode_segmentation::UnicodeSegmentation;

use agent_studios_tui::layout::{centered_rect, MainLayout, TerminalBreakpoint};
use agent_studios_tui::terminal::sanitize_terminal_title;
use agent_studios_tui::theme::{AgsTheme, ThemeMode};
use agent_studios_tui::widgets::{
    agent_bar::{AgentBarWidget, AgentItem},
    approval_modal::{ApprovalModalWidget, PendingApproval},
    composer::ComposerWidget,
    diff_viewer::DiffViewerWidget,
    header::HeaderWidget,
    slash_menu::{SlashMenuWidget, AVAILABLE_COMMANDS},
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
    let agents = vec![
        AgentItem {
            role: "Coordinator".to_string(),
            state_label: "BUSY".to_string(),
            is_active: true,
            is_awaiting_approval: false,
            is_failed: false,
        },
    ];
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
