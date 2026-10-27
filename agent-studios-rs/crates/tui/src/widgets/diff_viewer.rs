//! Unified Diff Syntax Highlighting Widget
//!
//! Renders git patch additions, deletions, and hunk headers with high-contrast precision.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

use crate::theme::AgsTheme;

pub struct DiffViewerWidget<'a> {
    pub theme: &'a AgsTheme,
    pub diff_content: &'a str,
    pub title: &'a str,
}

impl<'a> Widget for DiffViewerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_subtle_style())
            .title(Span::styled(format!(" Diff: {} ", self.title), Style::default().fg(self.theme.accent_sky)))
            .style(self.theme.card_style());

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let mut lines = Vec::new();

        for (idx, raw_line) in self.diff_content.lines().enumerate() {
            let line_num_span = Span::styled(
                format!("{:3} ", idx + 1),
                Style::default().fg(self.theme.text_muted),
            );

            let content_span = if raw_line.starts_with('+') && !raw_line.starts_with("+++") {
                Span::styled(
                    raw_line,
                    Style::default().fg(self.theme.accent_emerald),
                )
            } else if raw_line.starts_with('-') && !raw_line.starts_with("---") {
                Span::styled(
                    raw_line,
                    Style::default().fg(self.theme.accent_crimson),
                )
            } else if raw_line.starts_with("@@") {
                Span::styled(
                    raw_line,
                    Style::default().fg(self.theme.accent_sky).add_modifier(Modifier::BOLD),
                )
            } else if raw_line.starts_with("diff ") || raw_line.starts_with("index ") {
                Span::styled(
                    raw_line,
                    Style::default().fg(self.theme.text_muted).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(raw_line, Style::default().fg(self.theme.text_secondary))
            };

            lines.push(Line::from(vec![line_num_span, content_span]));
        }

        for (i, line) in lines.iter().take(inner.height as usize).enumerate() {
            buf.set_line(inner.x, inner.y + i as u16, line, inner.width);
        }
    }
}
