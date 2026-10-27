//! Multiline Composer Widget with Vietnamese NFC Grapheme Awareness
//!
//! Handles text editing, cursor movement, and grapheme width calculations
//! preventing diacritic clipping on Vietnamese text input.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::theme::AgsTheme;

pub struct ComposerWidget<'a> {
    pub theme: &'a AgsTheme,
    pub input_text: &'a str,
    pub cursor_grapheme_index: usize,
    pub is_focused: bool,
}

impl<'a> Widget for ComposerWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.is_focused {
            self.theme.border_focused_style()
        } else {
            self.theme.border_subtle_style()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(" Input Prompt ", Style::default().fg(self.theme.text_secondary)))
            .style(self.theme.card_style());

        let inner_area = block.inner(area);
        block.render(area, buf);

        if inner_area.height == 0 || inner_area.width == 0 {
            return;
        }

        if self.input_text.is_empty() {
            let placeholder = Line::from(vec![
                Span::styled(
                    "Type prompt or / for commands... (Enter để gửi, / xem lệnh)",
                    Style::default().fg(self.theme.text_muted).add_modifier(Modifier::ITALIC),
                ),
            ]);
            buf.set_line(inner_area.x, inner_area.y, &placeholder, inner_area.width);
            return;
        }

        // Deconstruct text into graphemes to prevent clipping Vietnamese combining marks
        let graphemes: Vec<&str> = self.input_text.graphemes(true).collect();
        let total_graphemes = graphemes.len();

        let mut spans = Vec::new();
        let mut visual_x = 0;
        let mut cursor_visual_x = 0;

        for (idx, &g) in graphemes.iter().enumerate() {
            if idx == self.cursor_grapheme_index && self.is_focused {
                cursor_visual_x = visual_x;
                // Highlight cursor character
                spans.push(Span::styled(
                    g,
                    Style::default()
                        .bg(self.theme.accent_emerald)
                        .fg(self.theme.text_inverse)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(g, Style::default().fg(self.theme.text_primary)));
            }
            visual_x += g.width();
        }

        // Cursor at the very end
        if self.cursor_grapheme_index >= total_graphemes && self.is_focused {
            cursor_visual_x = visual_x;
            spans.push(Span::styled(
                " ",
                Style::default().bg(self.theme.accent_emerald).fg(self.theme.text_inverse),
            ));
        }

        let line = Line::from(spans);
        buf.set_line(inner_area.x, inner_area.y, &line, inner_area.width);

        // Render grapheme counter on bottom-right of composer if space permits
        let counter_str = format!(" {}g ", total_graphemes);
        let counter_len = counter_str.len() as u16;
        if inner_area.width > counter_len + 4 && inner_area.height > 1 {
            let cx = inner_area.x + inner_area.width - counter_len;
            let cy = inner_area.y + inner_area.height - 1;
            let counter_line = Line::from(vec![Span::styled(
                counter_str,
                Style::default().fg(self.theme.text_muted),
            )]);
            buf.set_line(cx, cy, &counter_line, counter_len);
        }

        let _ = cursor_visual_x;
    }
}
