//! Virtual-scrolling Transcript Widget
//!
//! Renders user messages, thoughts, tool invocations, and agent replies
//! with clean typography and scrolling support.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

use crate::theme::AgsTheme;

#[derive(Debug, Clone)]
pub enum TranscriptItem {
    UserPrompt(String),
    AssistantThought(String),
    AssistantMessage(String),
    ToolInvocation { tool_name: String, status: String },
    SystemNotice { level: String, message: String },
}

pub struct TranscriptWidget<'a> {
    pub theme: &'a AgsTheme,
    pub items: &'a [TranscriptItem],
    pub scroll_offset: usize,
}

impl<'a> Widget for TranscriptWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_subtle_style())
            .title(Span::styled(" Transcript ", Style::default().fg(self.theme.text_secondary)))
            .style(self.theme.base_style());

        let inner_area = block.inner(area);
        block.render(area, buf);

        if inner_area.height == 0 || inner_area.width == 0 {
            return;
        }

        // Convert items into wrapped lines
        let mut lines: Vec<Line> = Vec::new();

        for item in self.items {
            match item {
                TranscriptItem::UserPrompt(prompt) => {
                    lines.push(Line::from(vec![
                        Span::styled("You: ", Style::default().fg(self.theme.accent_sky).add_modifier(Modifier::BOLD)),
                        Span::styled(prompt, Style::default().fg(self.theme.text_primary).add_modifier(Modifier::BOLD)),
                    ]));
                    lines.push(Line::raw(""));
                }
                TranscriptItem::AssistantThought(thought) => {
                    lines.push(Line::from(vec![
                        Span::styled("  * thinking: ", Style::default().fg(self.theme.text_muted).add_modifier(Modifier::ITALIC)),
                        Span::styled(thought, Style::default().fg(self.theme.text_muted)),
                    ]));
                }
                TranscriptItem::AssistantMessage(msg) => {
                    lines.push(Line::from(vec![
                        Span::styled("Agent: ", Style::default().fg(self.theme.accent_emerald).add_modifier(Modifier::BOLD)),
                    ]));
                    for l in msg.lines() {
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(l, Style::default().fg(self.theme.text_primary)),
                        ]));
                    }
                    lines.push(Line::raw(""));
                }
                TranscriptItem::ToolInvocation { tool_name, status } => {
                    lines.push(Line::from(vec![
                        Span::styled("  [tool] ", Style::default().fg(self.theme.accent_purple)),
                        Span::styled(tool_name, Style::default().fg(self.theme.text_secondary).add_modifier(Modifier::BOLD)),
                        Span::styled(format!(" -> {status}"), Style::default().fg(self.theme.text_muted)),
                    ]));
                }
                TranscriptItem::SystemNotice { level, message } => {
                    let color = match level.as_str() {
                        "Error" => self.theme.accent_crimson,
                        "Warning" => self.theme.accent_amber,
                        _ => self.theme.accent_sky,
                    };
                    lines.push(Line::from(vec![
                        Span::styled(format!("  ! [{level}] "), Style::default().fg(color).add_modifier(Modifier::BOLD)),
                        Span::styled(message, Style::default().fg(color)),
                    ]));
                }
            }
        }

        // Calculate visible slice
        let total_lines = lines.len();
        let height = inner_area.height as usize;

        let start_index = if total_lines > height {
            let max_offset = total_lines.saturating_sub(height);
            self.scroll_offset.min(max_offset)
        } else {
            0
        };

        for (i, line) in lines.iter().skip(start_index).take(height).enumerate() {
            let y = inner_area.y + i as u16;
            buf.set_line(inner_area.x, y, line, inner_area.width);
        }
    }
}
