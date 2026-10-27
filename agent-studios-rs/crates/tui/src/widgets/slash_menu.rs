//! Slash Command Palette Popup Widget
//!
//! Provides quick keyboard access to common CLI commands when user enters `/`.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Widget},
};

use crate::theme::AgsTheme;

#[derive(Debug, Clone)]
pub struct SlashCommandItem {
    pub command: &'static str,
    pub description: &'static str,
}

pub const AVAILABLE_COMMANDS: &[SlashCommandItem] = &[
    SlashCommandItem { command: "/help", description: "Show keybindings and help documentation" },
    SlashCommandItem { command: "/clear", description: "Clear current conversation transcript" },
    SlashCommandItem { command: "/doctor", description: "Run system diagnostics & health checks" },
    SlashCommandItem { command: "/model", description: "Inspect or switch active LLM model" },
    SlashCommandItem { command: "/worktree", description: "List and manage isolated Git worktrees" },
    SlashCommandItem { command: "/diff", description: "Review uncommitted changes in worktree" },
    SlashCommandItem { command: "/reconcile", description: "Reconcile or merge agent patch" },
    SlashCommandItem { command: "/cancel", description: "Cancel current active agent execution" },
    SlashCommandItem { command: "/quit", description: "Exit Agent Studios CLI" },
];

pub struct SlashMenuWidget<'a> {
    pub theme: &'a AgsTheme,
    pub selected_index: usize,
    pub filter: &'a str,
}

impl<'a> Widget for SlashMenuWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 30 || area.height < 4 {
            return;
        }

        Clear.render(area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_focused_style())
            .title(Span::styled(" Commands (↑/↓ chọn, Enter thực hiện) ", Style::default().fg(self.theme.accent_emerald)))
            .style(self.theme.card_style());

        let inner = block.inner(area);
        block.render(area, buf);

        let filtered: Vec<&SlashCommandItem> = AVAILABLE_COMMANDS
            .iter()
            .filter(|item| item.command.starts_with(self.filter) || self.filter.is_empty())
            .collect();

        if filtered.is_empty() {
            let empty_line = Line::from(vec![
                Span::styled("  No matching command", Style::default().fg(self.theme.text_muted)),
            ]);
            buf.set_line(inner.x, inner.y, &empty_line, inner.width);
            return;
        }

        for (i, item) in filtered.iter().enumerate().take(inner.height as usize) {
            let is_selected = i == self.selected_index;

            let (cmd_style, desc_style, prefix) = if is_selected {
                (
                    Style::default().fg(self.theme.accent_emerald).add_modifier(Modifier::BOLD),
                    Style::default().fg(self.theme.text_primary),
                    " > ",
                )
            } else {
                (
                    Style::default().fg(self.theme.accent_sky),
                    Style::default().fg(self.theme.text_muted),
                    "   ",
                )
            };

            let line = Line::from(vec![
                Span::styled(prefix, cmd_style),
                Span::styled(format!("{:<12}", item.command), cmd_style),
                Span::styled(item.description, desc_style),
            ]);

            buf.set_line(inner.x, inner.y + i as u16, &line, inner.width);
        }
    }
}
