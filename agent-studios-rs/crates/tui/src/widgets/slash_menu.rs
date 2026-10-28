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
    SlashCommandItem {
        command: "/help",
        description: "[Ready] Show keybindings and command documentation",
    },
    SlashCommandItem {
        command: "/clear",
        description: "[Ready] Clear conversation transcript",
    },
    SlashCommandItem {
        command: "/doctor",
        description: "[Ready] Run system diagnostics & health checks",
    },
    SlashCommandItem {
        command: "/status",
        description: "[Ready] Show active studio session status",
    },
    SlashCommandItem {
        command: "/cancel",
        description: "[Ready] Cancel active agent execution",
    },
    SlashCommandItem {
        command: "/quit",
        description: "[Ready] Exit Agent Studios CLI",
    },
    SlashCommandItem {
        command: "/diff",
        description: "[Planned] Review uncommitted changes in worktree",
    },
    SlashCommandItem {
        command: "/model",
        description: "[Planned] Inspect or switch active model",
    },
    SlashCommandItem {
        command: "/worktree",
        description: "[Planned] List and manage isolated Git worktrees",
    },
    SlashCommandItem {
        command: "/reconcile",
        description: "[Planned] Reconcile or merge agent patch",
    },
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
            .title(Span::styled(
                " Commands (↑/↓ chọn, Enter thực hiện) ",
                Style::default().fg(self.theme.accent_emerald),
            ))
            .style(self.theme.card_style());

        let inner = block.inner(area);
        block.render(area, buf);

        let filtered: Vec<&SlashCommandItem> = AVAILABLE_COMMANDS
            .iter()
            .filter(|item| item.command.starts_with(self.filter) || self.filter.is_empty())
            .collect();

        if filtered.is_empty() {
            let empty_line = Line::from(vec![Span::styled(
                "  No matching command",
                Style::default().fg(self.theme.text_muted),
            )]);
            buf.set_line(inner.x, inner.y, &empty_line, inner.width);
            return;
        }

        for (i, item) in filtered.iter().enumerate().take(inner.height as usize) {
            let is_selected = i == self.selected_index;

            let (cmd_style, prefix) = if is_selected {
                (
                    Style::default()
                        .fg(self.theme.accent_emerald)
                        .add_modifier(Modifier::BOLD),
                    " > ",
                )
            } else {
                (Style::default().fg(self.theme.accent_sky), "   ")
            };

            let tag_style = if item.description.starts_with("[Ready]") {
                if is_selected {
                    Style::default()
                        .fg(self.theme.accent_emerald)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.accent_emerald)
                }
            } else {
                if is_selected {
                    Style::default()
                        .fg(self.theme.accent_amber)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.accent_amber)
                }
            };

            let text_style = if is_selected {
                Style::default().fg(self.theme.text_primary)
            } else {
                Style::default().fg(self.theme.text_muted)
            };

            let (tag, rest) = if let Some(stripped) = item.description.strip_prefix("[Ready] ") {
                ("[Ready] ", stripped)
            } else if let Some(stripped) = item.description.strip_prefix("[Planned] ") {
                ("[Planned] ", stripped)
            } else {
                ("", item.description)
            };

            let line = Line::from(vec![
                Span::styled(prefix, cmd_style),
                Span::styled(format!("{:<12}", item.command), cmd_style),
                Span::styled(tag, tag_style),
                Span::styled(rest, text_style),
            ]);

            buf.set_line(inner.x, inner.y + i as u16, &line, inner.width);
        }
    }
}
