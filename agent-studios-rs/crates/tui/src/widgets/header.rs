//! Header Widget for Agent Studios CLI
//!
//! Displays project repository path, active git branch, dirty indicator,
//! active LLM model badge, and execution mode.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

use crate::theme::AgsTheme;

pub struct HeaderWidget<'a> {
    pub theme: &'a AgsTheme,
    pub project_name: &'a str,
    pub branch_name: &'a str,
    pub is_dirty: bool,
    pub model_name: &'a str,
    pub execution_mode: &'a str,
    pub compact: bool,
}

impl<'a> Widget for HeaderWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 1 {
            return;
        }

        let project_display = if self.project_name.trim().is_empty() {
            "Unknown project"
        } else {
            self.project_name
        };

        let branch_display = if self.branch_name.trim().is_empty() {
            "Unknown branch"
        } else {
            self.branch_name
        };

        let model_display = if self.model_name.trim().is_empty() {
            "No model configured"
        } else {
            self.model_name
        };

        let mode_display = if self.execution_mode.trim().is_empty() {
            "Embedded"
        } else {
            self.execution_mode
        };

        let dirty_span = if self.is_dirty {
            Span::styled(
                " *dirty",
                Style::default()
                    .fg(self.theme.accent_amber)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(" Clean", Style::default().fg(self.theme.accent_emerald))
        };

        let title_spans = vec![
            Span::styled(
                " AGENT STUDIOS ",
                Style::default()
                    .bg(self.theme.accent_emerald)
                    .fg(self.theme.text_inverse)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                project_display,
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" (", Style::default().fg(self.theme.text_muted)),
            Span::styled(branch_display, Style::default().fg(self.theme.accent_sky)),
            dirty_span,
            Span::styled(")", Style::default().fg(self.theme.text_muted)),
        ];

        // Mode badge extraction and styling
        let upper_mode = mode_display.to_uppercase();
        let (mode_badge, badge_bg, badge_fg) = if upper_mode.contains("DEMO") {
            ("[DEMO]", self.theme.accent_amber, self.theme.text_inverse)
        } else if upper_mode.contains("PREVIEW") {
            ("[PREVIEW]", self.theme.accent_sky, self.theme.text_inverse)
        } else if upper_mode.contains("PROD") || upper_mode.contains("LIVE") {
            ("[LIVE]", self.theme.accent_emerald, self.theme.text_inverse)
        } else if upper_mode.contains("SETUP") || upper_mode.contains("ONBOARDING") {
            ("[SETUP]", self.theme.accent_purple, self.theme.text_inverse)
        } else {
            ("", self.theme.bg_surface, self.theme.text_secondary)
        };

        let mut badges_spans = vec![Span::styled(
            format!(" [{}] ", model_display),
            Style::default().fg(self.theme.accent_purple),
        )];

        if !mode_badge.is_empty() {
            let base_mode = mode_display
                .replace("[LIVE]", "")
                .replace("[DEMO]", "")
                .replace("[PREVIEW]", "")
                .replace("[SETUP]", "")
                .replace("[Production]", "")
                .replace("[Demo]", "")
                .replace("[Preview]", "")
                .trim()
                .to_string();

            if !base_mode.is_empty() {
                badges_spans.push(Span::styled(
                    format!(" {base_mode} "),
                    Style::default()
                        .bg(self.theme.bg_surface)
                        .fg(self.theme.text_secondary),
                ));
            }

            badges_spans.push(Span::styled(
                format!(" {} ", mode_badge),
                Style::default()
                    .bg(badge_bg)
                    .fg(badge_fg)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            badges_spans.push(Span::styled(
                format!(" {} ", mode_display),
                Style::default()
                    .bg(self.theme.bg_surface)
                    .fg(self.theme.text_secondary),
            ));
        }

        if self.compact {
            // Render 1-2 lines without border
            let line = Line::from(title_spans);
            buf.set_line(area.x, area.y, &line, area.width);
            return;
        }

        // Render card block with bottom border
        let block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(self.theme.border_subtle_style())
            .style(self.theme.card_style());

        let inner_area = block.inner(area);
        block.render(area, buf);

        if inner_area.height > 0 {
            let left_line = Line::from(title_spans);
            let right_line = Line::from(badges_spans);

            buf.set_line(inner_area.x, inner_area.y, &left_line, inner_area.width);

            // Right-aligned badges if space allows
            let right_width = right_line.width() as u16;
            if inner_area.width > right_width + left_line.width() as u16 + 4 {
                let right_x = inner_area.x + inner_area.width - right_width;
                buf.set_line(right_x, inner_area.y, &right_line, right_width);
            }
        }
    }
}
