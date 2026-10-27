//! Fail-Closed Interactive Security Gate Modal
//!
//! Renders 3-tier risk taxonomy (ReadOnly, WorkspaceMutating, DangerousHost)
//! prompting user for explicit authorization before executing sensitive operations.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

use agent_studios_protocol::id::ApprovalId;

use crate::layout::centered_rect;
use crate::theme::AgsTheme;

#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub approval_id: ApprovalId,
    pub tool_name: String,
    pub risk_tier: String,
    pub summary: String,
    pub details: String,
}

pub struct ApprovalModalWidget<'a> {
    pub theme: &'a AgsTheme,
    pub approval: &'a PendingApproval,
}

impl<'a> Widget for ApprovalModalWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let modal_area = centered_rect(70, 60, area);

        // Clear backdrop behind popup
        Clear.render(modal_area, buf);

        let (risk_color, risk_badge) = match self.approval.risk_tier.as_str() {
            "DangerousHost" | "Dangerous" => (self.theme.risk_dangerous, " [!] DANGEROUS HOST OPERATION "),
            "WorkspaceMutating" | "Mutating" => (self.theme.risk_mutating, " [*] WORKSPACE MUTATING "),
            _ => (self.theme.risk_readonly, " [i] READ ONLY "),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(risk_color).add_modifier(Modifier::BOLD))
            .title(Span::styled(" SECURITY GATE APPROVAL ", Style::default().bg(risk_color).fg(self.theme.text_inverse).add_modifier(Modifier::BOLD)))
            .style(self.theme.card_style());

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        if inner.height < 6 || inner.width < 20 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Risk banner & tool name
                Constraint::Min(4),    // Summary & details
                Constraint::Length(2), // Action keys
            ])
            .split(inner);

        // Header info
        let header_lines = vec![
            Line::from(vec![
                Span::styled(risk_badge, Style::default().bg(risk_color).fg(self.theme.text_inverse).add_modifier(Modifier::BOLD)),
                Span::raw(" Tool: "),
                Span::styled(&self.approval.tool_name, Style::default().fg(self.theme.text_primary).add_modifier(Modifier::BOLD)),
            ]),
        ];
        Paragraph::new(header_lines).render(chunks[0], buf);

        // Body content
        let mut body_lines = vec![
            Line::from(vec![
                Span::styled("Action: ", Style::default().fg(self.theme.text_secondary)),
                Span::styled(&self.approval.summary, Style::default().fg(self.theme.text_primary)),
            ]),
            Line::raw(""),
        ];

        for line in self.approval.details.lines() {
            body_lines.push(Line::from(vec![
                Span::styled(line, Style::default().fg(self.theme.text_muted)),
            ]));
        }

        Paragraph::new(body_lines)
            .wrap(Wrap { trim: false })
            .render(chunks[1], buf);

        // Actions footer
        let footer_spans = vec![
            Span::styled(" [a] ", Style::default().fg(self.theme.accent_emerald).add_modifier(Modifier::BOLD)),
            Span::styled("Allow Once  ", Style::default().fg(self.theme.text_primary)),
            Span::styled(" [s] ", Style::default().fg(self.theme.accent_sky).add_modifier(Modifier::BOLD)),
            Span::styled("Allow for Session  ", Style::default().fg(self.theme.text_primary)),
            Span::styled(" [d] ", Style::default().fg(self.theme.accent_crimson).add_modifier(Modifier::BOLD)),
            Span::styled("Deny  ", Style::default().fg(self.theme.text_primary)),
            Span::styled(" [Esc] ", Style::default().fg(self.theme.text_muted)),
            Span::styled("Cancel", Style::default().fg(self.theme.text_muted)),
        ];

        Paragraph::new(Line::from(footer_spans)).render(chunks[2], buf);
    }
}
