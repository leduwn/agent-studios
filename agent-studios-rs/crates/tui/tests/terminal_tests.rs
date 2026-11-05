//! Integration Tests for Terminal Lifecycle, Restoration, Sanitization & Layout
//!
//! Validates CLI-REV-007 terminal restoration idempotency, partial initialization recovery,
//! Trojan Source bidi sanitization, and responsive layout calculation.

use ratatui::layout::Rect;

use agent_studios_tui::layout::{MainLayout, TerminalBreakpoint, centered_rect};
use agent_studios_tui::terminal::{
    MAX_TERMINAL_TITLE_CHARS, TerminalGuard, sanitize_terminal_title, set_terminal_title,
};

#[test]
fn test_restore_raw_terminal_idempotency() {
    // Calling restore_raw_terminal multiple times consecutively must be completely
    // safe, idempotent, and return Ok(()) without panicking or corrupting state.
    for _ in 0..5 {
        let res = TerminalGuard::restore_raw_terminal();
        assert!(
            res.is_ok(),
            "restore_raw_terminal failed on consecutive call"
        );
        assert!(!TerminalGuard::is_raw_mode_active());
    }
}

#[test]
fn test_terminal_title_control_codes_stripped() {
    // Control characters (NUL, BEL, BS, TAB, LF, CR, ESC, DEL) must be stripped
    let raw = "Agent\x00\x07Studios\x08 \tConsole\r\n\x1b[32mActive\x7f";
    let cleaned = sanitize_terminal_title(raw);

    assert!(!cleaned.contains('\x00'));
    assert!(!cleaned.contains('\x07'));
    assert!(!cleaned.contains('\x08'));
    assert!(!cleaned.contains('\r'));
    assert!(!cleaned.contains('\n'));
    assert!(!cleaned.contains('\x1b'));
    assert!(!cleaned.contains('\x7f'));
    // Non-whitespace control chars removed without inserting spaces; whitespace runs collapsed to single space
    assert_eq!(cleaned, "AgentStudios Console [32mActive");
}

#[test]
fn test_terminal_title_ansi_neutralization() {
    // ANSI color and cursor control escape sequences must be neutralized by stripping ESC
    let ansi_title = "\x1b[31;1mCRITICAL\x1b[0m: \x1b]0;HackedTitle\x07Prompt";
    let cleaned = sanitize_terminal_title(ansi_title);

    assert!(!cleaned.contains('\x1b'));
    assert!(!cleaned.contains('\x07'));
    assert_eq!(cleaned, "[31;1mCRITICAL[0m: ]0;HackedTitlePrompt");
}

#[test]
fn test_terminal_title_trojan_source_bidi_stripped() {
    // Trojan Source CVE-2021-42574 and CVE-2021-42694 bidirectional control characters
    // LRE (\u{202A}), RLE (\u{202B}), PDF (\u{202C}), LRO (\u{202D}), RLO (\u{202E})
    // LRI (\u{2066}), RLI (\u{2067}), FSI (\u{2068}), PDI (\u{2069})
    // LRM (\u{200E}), RLM (\u{200F}), ALM (\u{061C}), ZWSP (\u{200B})
    let malicious = "Prefix \u{202A}LRE\u{202C} \u{202E}RLO\u{202C} \u{2066}LRI\u{2069} \u{061C}ALM \u{200E}LRM \u{200B}ZWSP Suffix";
    let cleaned = sanitize_terminal_title(malicious);

    assert!(!cleaned.contains('\u{202A}'));
    assert!(!cleaned.contains('\u{202B}'));
    assert!(!cleaned.contains('\u{202C}'));
    assert!(!cleaned.contains('\u{202D}'));
    assert!(!cleaned.contains('\u{202E}'));
    assert!(!cleaned.contains('\u{2066}'));
    assert!(!cleaned.contains('\u{2067}'));
    assert!(!cleaned.contains('\u{2068}'));
    assert!(!cleaned.contains('\u{2069}'));
    assert!(!cleaned.contains('\u{061C}'));
    assert!(!cleaned.contains('\u{200E}'));
    assert!(!cleaned.contains('\u{200F}'));
    assert!(!cleaned.contains('\u{200B}'));
    assert_eq!(cleaned, "Prefix LRE RLO LRI ALM LRM ZWSP Suffix");
}

#[test]
fn test_terminal_title_length_bounding() {
    // Excessively long titles must be bounded to MAX_TERMINAL_TITLE_CHARS (240)
    let long_title = "A".repeat(500);
    let cleaned = sanitize_terminal_title(&long_title);
    assert_eq!(cleaned.chars().count(), MAX_TERMINAL_TITLE_CHARS);
    assert_eq!(cleaned.len(), MAX_TERMINAL_TITLE_CHARS);

    // Multi-byte Unicode (Vietnamese tone marks) length bounding
    let vi_long = "Tiếng Việt chuẩn xác chất lượng cao. ".repeat(20);
    let cleaned_vi = sanitize_terminal_title(&vi_long);
    assert!(cleaned_vi.chars().count() <= MAX_TERMINAL_TITLE_CHARS);
    assert!(std::str::from_utf8(cleaned_vi.as_bytes()).is_ok());

    // Empty and whitespace-only titles
    assert_eq!(sanitize_terminal_title(""), "");
    assert_eq!(sanitize_terminal_title("   \t\n  "), "");

    // set_terminal_title with empty string is a no-op Ok
    assert!(set_terminal_title("").is_ok());
    assert!(set_terminal_title("   ").is_ok());
}

#[test]
fn test_terminal_breakpoint_classification() {
    // Compact: width < 100 or height < 28
    assert_eq!(
        TerminalBreakpoint::from_dimensions(0, 0),
        TerminalBreakpoint::Compact
    );
    assert_eq!(
        TerminalBreakpoint::from_dimensions(79, 24),
        TerminalBreakpoint::Compact
    );
    assert_eq!(
        TerminalBreakpoint::from_dimensions(80, 24),
        TerminalBreakpoint::Compact
    );
    assert_eq!(
        TerminalBreakpoint::from_dimensions(99, 27),
        TerminalBreakpoint::Compact
    );
    assert_eq!(
        TerminalBreakpoint::from_dimensions(100, 27),
        TerminalBreakpoint::Compact
    );
    assert_eq!(
        TerminalBreakpoint::from_dimensions(120, 27),
        TerminalBreakpoint::Compact
    );
    assert_eq!(
        TerminalBreakpoint::from_dimensions(180, 27),
        TerminalBreakpoint::Compact
    );
    assert_eq!(
        TerminalBreakpoint::from_dimensions(99, 30),
        TerminalBreakpoint::Compact
    );

    // Standard: 100 <= width < 120 and height >= 28
    assert_eq!(
        TerminalBreakpoint::from_dimensions(100, 28),
        TerminalBreakpoint::Standard
    );
    assert_eq!(
        TerminalBreakpoint::from_dimensions(110, 30),
        TerminalBreakpoint::Standard
    );
    assert_eq!(
        TerminalBreakpoint::from_dimensions(119, 50),
        TerminalBreakpoint::Standard
    );

    // Wide: 120 <= width < 160 and height >= 28
    assert_eq!(
        TerminalBreakpoint::from_dimensions(120, 28),
        TerminalBreakpoint::Wide
    );
    assert_eq!(
        TerminalBreakpoint::from_dimensions(140, 35),
        TerminalBreakpoint::Wide
    );
    assert_eq!(
        TerminalBreakpoint::from_dimensions(159, 45),
        TerminalBreakpoint::Wide
    );

    // UltraWide: width >= 160 and height >= 28
    assert_eq!(
        TerminalBreakpoint::from_dimensions(160, 28),
        TerminalBreakpoint::UltraWide
    );
    assert_eq!(
        TerminalBreakpoint::from_dimensions(200, 50),
        TerminalBreakpoint::UltraWide
    );
    assert_eq!(
        TerminalBreakpoint::from_dimensions(320, 80),
        TerminalBreakpoint::UltraWide
    );
}

#[test]
fn test_main_layout_geometry_by_breakpoint() {
    // 1. Compact Layout (80x24)
    let compact_rect = Rect::new(0, 0, 80, 24);
    let layout = MainLayout::compute(compact_rect);
    assert_eq!(layout.breakpoint, TerminalBreakpoint::Compact);
    assert!(layout.side_pane_area.is_none());
    assert_eq!(layout.header_area.height, 2);
    assert_eq!(layout.agent_bar_area.height, 1);
    assert_eq!(layout.composer_area.height, 3);
    assert_eq!(layout.status_bar_area.height, 1);
    assert_eq!(layout.header_area.width, 80);

    // 2. Standard Layout (100x30)
    let std_rect = Rect::new(0, 0, 100, 30);
    let layout_std = MainLayout::compute(std_rect);
    assert_eq!(layout_std.breakpoint, TerminalBreakpoint::Standard);
    assert!(layout_std.side_pane_area.is_none());
    assert_eq!(layout_std.header_area.height, 3);
    assert_eq!(layout_std.agent_bar_area.height, 2);
    assert_eq!(layout_std.composer_area.height, 4);
    assert_eq!(layout_std.status_bar_area.height, 1);

    // 3. Wide Layout (130x35)
    let wide_rect = Rect::new(0, 0, 130, 35);
    let layout_wide = MainLayout::compute(wide_rect);
    assert_eq!(layout_wide.breakpoint, TerminalBreakpoint::Wide);
    assert!(layout_wide.side_pane_area.is_none());
    assert_eq!(layout_wide.header_area.height, 3);

    // 4. UltraWide Layout (200x50) - Dual-pane enabled
    let uw_rect = Rect::new(0, 0, 200, 50);
    let layout_uw = MainLayout::compute(uw_rect);
    assert_eq!(layout_uw.breakpoint, TerminalBreakpoint::UltraWide);
    assert!(layout_uw.side_pane_area.is_some());
    let side_pane = layout_uw.side_pane_area.unwrap();
    assert!(side_pane.width > 0);
    assert!(layout_uw.content_area.width > 0);
    // Middle split is 65% left column, 35% side pane
    assert!(layout_uw.content_area.width > side_pane.width);
}

#[test]
fn test_centered_rect_centering_and_proportions() {
    let parent = Rect::new(0, 0, 120, 60);

    // Centered 50% width, 50% height
    let modal = centered_rect(50, 50, parent);
    assert_eq!(modal.width, 60);
    assert_eq!(modal.height, 30);
    assert_eq!(modal.x, 30);
    assert_eq!(modal.y, 15);

    // Centered 80% width, 40% height
    let modal2 = centered_rect(80, 40, parent);
    assert_eq!(modal2.width, 96);
    assert_eq!(modal2.height, 24);
    assert_eq!(modal2.x, 12);
    assert_eq!(modal2.y, 18);

    // Ensure within bounds
    assert!(modal2.x + modal2.width <= parent.width);
    assert!(modal2.y + modal2.height <= parent.height);
}
