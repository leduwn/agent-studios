//! Bottom Status Bar Widget
//!
//! Renders token counters, cost estimates, status indicators, and keyboard shortcuts footer.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::theme::AgsTheme;

pub struct StatusBarWidget<'a> {
    pub theme: &'a AgsTheme,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub status_text: &'a str,
}

impl<'a> Widget for StatusBarWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 1 {
            return;
        }

        // Left key hints
        let hints = vec![
            Span::styled(
                " [Enter] ",
                Style::default()
                    .fg(self.theme.accent_emerald)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Send", Style::default().fg(self.theme.text_secondary)),
            Span::styled(
                "  [/] ",
                Style::default()
                    .fg(self.theme.accent_sky)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Commands", Style::default().fg(self.theme.text_secondary)),
            Span::styled(
                "  [^C] ",
                Style::default()
                    .fg(self.theme.accent_crimson)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Cancel", Style::default().fg(self.theme.text_secondary)),
            Span::styled(
                "  [Tab] ",
                Style::default()
                    .fg(self.theme.accent_purple)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Switch View",
                Style::default().fg(self.theme.text_secondary),
            ),
        ];

        let left_line = Line::from(hints);
        buf.set_line(area.x, area.y, &left_line, area.width);

        // Right side token / cost metrics
        let token_text = if self.input_tokens == 0 && self.output_tokens == 0 {
            "0 tokens".to_string()
        } else {
            format!(
                "tokens: in {}+out {}",
                self.input_tokens, self.output_tokens
            )
        };

        let status_style = match self.status_text {
            "AWAITING APPROVAL" => Style::default()
                .bg(self.theme.accent_amber)
                .fg(self.theme.text_inverse)
                .add_modifier(Modifier::BOLD),
            "RUNNING" => Style::default()
                .bg(self.theme.accent_emerald)
                .fg(self.theme.text_inverse)
                .add_modifier(Modifier::BOLD),
            _ => Style::default()
                .bg(self.theme.bg_card)
                .fg(self.theme.text_secondary),
        };

        let metrics = vec![
            Span::styled(token_text, Style::default().fg(self.theme.text_muted)),
            Span::styled(
                format!(" (${:.3}) ", self.cost_usd),
                Style::default().fg(self.theme.accent_emerald),
            ),
            Span::styled(format!(" [{}] ", self.status_text), status_style),
        ];

        let right_line = Line::from(metrics);
        let right_width = right_line.width() as u16;

        if area.width >= right_width + left_line.width() as u16 {
            let right_x = area.x + area.width - right_width;
            buf.set_line(right_x, area.y, &right_line, right_width);
        }
    }
}
