//! Horizontal Agent Activity Bar Widget
//!
//! Visualizes active agent roles (Coordinator, Coder, Tester, Reviewer) and their states.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::theme::AgsTheme;

#[derive(Debug, Clone)]
pub struct AgentItem {
    pub role: String,
    pub state_label: String,
    pub is_active: bool,
    pub is_awaiting_approval: bool,
    pub is_failed: bool,
}

pub struct AgentBarWidget<'a> {
    pub theme: &'a AgsTheme,
    pub agents: &'a [AgentItem],
}

impl<'a> Widget for AgentBarWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 1 {
            return;
        }

        let mut spans = Vec::new();
        spans.push(Span::styled("AGENTS: ", Style::default().fg(self.theme.text_muted).add_modifier(Modifier::BOLD)));

        for (i, agent) in self.agents.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(" | ", Style::default().fg(self.theme.border_subtle)));
            }

            let (pill_style, dot) = if agent.is_failed {
                (Style::default().fg(self.theme.accent_crimson).add_modifier(Modifier::BOLD), "●")
            } else if agent.is_awaiting_approval {
                (Style::default().fg(self.theme.accent_amber).add_modifier(Modifier::BOLD), "▲")
            } else if agent.is_active {
                (Style::default().fg(self.theme.accent_emerald).add_modifier(Modifier::BOLD), "●")
            } else {
                (Style::default().fg(self.theme.text_muted), "○")
            };

            spans.push(Span::styled(format!("{dot} "), pill_style));
            spans.push(Span::styled(&agent.role, Style::default().fg(self.theme.text_primary)));
            spans.push(Span::styled(format!(" [{}]", agent.state_label), pill_style));
        }

        let line = Line::from(spans);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}
