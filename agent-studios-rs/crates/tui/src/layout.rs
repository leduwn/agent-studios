//! Responsive Terminal Layout Engine
//!
//! Adapts UI across terminal breakpoints: Compact (80x24), Standard (100x30),
//! Wide (120x36), and Ultra-Wide (160x45+ dual-pane).

use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalBreakpoint {
    Compact,   // < 100 width or < 28 height
    Standard,  // 100..120 width
    Wide,      // 120..160 width
    UltraWide, // >= 160 width
}

impl TerminalBreakpoint {
    pub fn from_dimensions(width: u16, height: u16) -> Self {
        if width < 100 || height < 28 {
            Self::Compact
        } else if width < 120 {
            Self::Standard
        } else if width < 160 {
            Self::Wide
        } else {
            Self::UltraWide
        }
    }
}

pub struct MainLayout {
    pub breakpoint: TerminalBreakpoint,
    pub header_area: Rect,
    pub agent_bar_area: Rect,
    pub content_area: Rect,
    pub side_pane_area: Option<Rect>,
    pub composer_area: Rect,
    pub status_bar_area: Rect,
}

impl MainLayout {
    pub fn compute(area: Rect) -> Self {
        let breakpoint = TerminalBreakpoint::from_dimensions(area.width, area.height);

        match breakpoint {
            TerminalBreakpoint::Compact => {
                // Vertical stack: Header (2), AgentBar (1), Content (flex), Composer (3), Status (1)
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(2),
                        Constraint::Length(1),
                        Constraint::Min(8),
                        Constraint::Length(3),
                        Constraint::Length(1),
                    ])
                    .split(area);

                Self {
                    breakpoint,
                    header_area: chunks[0],
                    agent_bar_area: chunks[1],
                    content_area: chunks[2],
                    side_pane_area: None,
                    composer_area: chunks[3],
                    status_bar_area: chunks[4],
                }
            }
            TerminalBreakpoint::Standard | TerminalBreakpoint::Wide => {
                // Vertical stack: Header (3), AgentBar (2), Content (flex), Composer (4), Status (1)
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Length(2),
                        Constraint::Min(12),
                        Constraint::Length(4),
                        Constraint::Length(1),
                    ])
                    .split(area);

                Self {
                    breakpoint,
                    header_area: chunks[0],
                    agent_bar_area: chunks[1],
                    content_area: chunks[2],
                    side_pane_area: None,
                    composer_area: chunks[3],
                    status_bar_area: chunks[4],
                }
            }
            TerminalBreakpoint::UltraWide => {
                // Top (Header 3 + AgentBar 2), Middle (Left 65% Transcript+Composer, Right 35% Inspector), Bottom Status 1
                let main_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Length(2),
                        Constraint::Min(14),
                        Constraint::Length(1),
                    ])
                    .split(area);

                let middle_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(65),
                        Constraint::Percentage(35),
                    ])
                    .split(main_chunks[2]);

                let left_column = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(10),
                        Constraint::Length(4),
                    ])
                    .split(middle_chunks[0]);

                Self {
                    breakpoint,
                    header_area: main_chunks[0],
                    agent_bar_area: main_chunks[1],
                    content_area: left_column[0],
                    side_pane_area: Some(middle_chunks[1]),
                    composer_area: left_column[1],
                    status_bar_area: main_chunks[3],
                }
            }
        }
    }
}

/// Helper to center a modal window inside the current viewport
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
