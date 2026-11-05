//! Onboarding & Runtime Discovery Widget
//!
//! Renders an informative, graceful first-run onboarding screen displaying
//! workspace metadata, runtime discovery status, setup instructions, and quick actions.

use agent_studios_external_runtime::DiscoveredRuntimeInstance;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

use crate::theme::AgsTheme;
use crate::widgets::header::HeaderWidget;

pub struct OnboardingWidget<'a> {
    pub theme: &'a AgsTheme,
    pub project_name: &'a str,
    pub branch_name: &'a str,
    pub is_dirty: bool,
    pub workspace_path: Option<&'a str>,
    pub detected_runtimes: &'a [DiscoveredRuntimeInstance],
    pub error_message: Option<&'a str>,
}

impl<'a> Widget for OnboardingWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 1 {
            return;
        }

        // Fill background
        let base_block = Block::default().style(self.theme.base_style());
        base_block.render(area, buf);

        // Partition: Header (3 rows), Content (flex), Footer (1 row)
        let is_compact = area.height < 22;
        let header_height = if is_compact { 2 } else { 3 };
        let footer_height = 1;

        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_height),
                Constraint::Min(8),
                Constraint::Length(footer_height),
            ])
            .split(area);

        let header_area = vertical[0];
        let content_area = vertical[1];
        let footer_area = vertical[2];

        // 1. Header with [SETUP] badge
        HeaderWidget {
            theme: self.theme,
            project_name: self.project_name,
            branch_name: self.branch_name,
            is_dirty: self.is_dirty,
            model_name: "No AI runtime configured",
            execution_mode: "Onboarding [SETUP]",
            compact: is_compact,
        }
        .render(header_area, buf);

        // 2. Content Sections
        // Split content area into Welcome Banner + 3 Cards (or 2 Cards if compact)
        let content_chunks = if is_compact {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(4), // Workspace info
                    Constraint::Length(4), // Runtime discovery
                    Constraint::Min(6),    // Actions & Instructions
                ])
                .split(content_area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Welcome banner
                    Constraint::Length(5), // Workspace info
                    Constraint::Length(6), // Runtime discovery
                    Constraint::Min(8),    // Actions & Instructions
                ])
                .split(content_area)
        };

        let mut chunk_idx = 0;

        if !is_compact {
            // Welcome Banner
            let banner_area = content_chunks[chunk_idx];
            chunk_idx += 1;

            let banner_lines = [
                Line::from(vec![
                    Span::styled(
                        " Welcome to Agent Studios (ags) ",
                        Style::default()
                            .fg(self.theme.accent_emerald)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "— Autonomous Multi-Agent Engineering",
                        Style::default().fg(self.theme.text_secondary),
                    ),
                ]),
                Line::from(vec![Span::styled(
                    " Môi trường tự hành đa tác nhân | Hỗ trợ chuẩn Unicode NFC tiếng Việt",
                    Style::default()
                        .fg(self.theme.text_muted)
                        .add_modifier(Modifier::ITALIC),
                )]),
            ];

            for (i, line) in banner_lines.iter().enumerate() {
                if (banner_area.y + i as u16) < banner_area.y + banner_area.height {
                    buf.set_line(
                        banner_area.x + 1,
                        banner_area.y + i as u16,
                        line,
                        banner_area.width.saturating_sub(2),
                    );
                }
            }
        }

        // Card 1: Workspace & Environment
        let ws_area = content_chunks[chunk_idx];
        chunk_idx += 1;

        let ws_block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_subtle_style())
            .title(Span::styled(
                " Environment & Workspace ",
                Style::default()
                    .fg(self.theme.accent_sky)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(self.theme.card_style());

        let ws_inner = ws_block.inner(ws_area);
        ws_block.render(ws_area, buf);

        if ws_inner.height > 0 {
            let dirty_str = if self.is_dirty {
                "*dirty (uncommitted changes present)"
            } else {
                "clean"
            };
            let dirty_style = if self.is_dirty {
                Style::default()
                    .fg(self.theme.accent_amber)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.accent_emerald)
            };

            let loc_path = self.workspace_path.unwrap_or("Current Directory");

            let ws_lines = [
                Line::from(vec![
                    Span::styled(
                        "  Workspace: ",
                        Style::default().fg(self.theme.text_secondary),
                    ),
                    Span::styled(
                        self.project_name,
                        Style::default()
                            .fg(self.theme.text_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "   Git Branch: ",
                        Style::default().fg(self.theme.text_secondary),
                    ),
                    Span::styled(self.branch_name, Style::default().fg(self.theme.accent_sky)),
                    Span::styled(" (", Style::default().fg(self.theme.text_muted)),
                    Span::styled(dirty_str, dirty_style),
                    Span::styled(")", Style::default().fg(self.theme.text_muted)),
                ]),
                Line::from(vec![
                    Span::styled(
                        "  Location:  ",
                        Style::default().fg(self.theme.text_secondary),
                    ),
                    Span::styled(loc_path, Style::default().fg(self.theme.text_muted)),
                ]),
            ];

            for (i, line) in ws_lines.iter().enumerate() {
                if (ws_inner.y + i as u16) < ws_inner.y + ws_inner.height {
                    buf.set_line(ws_inner.x, ws_inner.y + i as u16, line, ws_inner.width);
                }
            }
        }

        // Card 2: Runtime Discovery Status
        let rt_area = content_chunks[chunk_idx];
        chunk_idx += 1;

        let rt_block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_subtle_style())
            .title(Span::styled(
                " AI Runtime Discovery ",
                Style::default()
                    .fg(self.theme.accent_purple)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(self.theme.card_style());

        let rt_inner = rt_block.inner(rt_area);
        rt_block.render(rt_area, buf);

        if rt_inner.height > 0 {
            let live_runtimes: Vec<&DiscoveredRuntimeInstance> = self
                .detected_runtimes
                .iter()
                .filter(|r| r.implementation_id.as_str() != "deterministic-local")
                .collect();

            let mut rt_lines = Vec::new();

            if live_runtimes.is_empty() {
                rt_lines.push(Line::from(vec![
                    Span::styled(
                        "  Status:  ",
                        Style::default().fg(self.theme.text_secondary),
                    ),
                    Span::styled(
                        " [!] No AI runtime detected ",
                        self.theme
                            .badge_style(self.theme.accent_amber, self.theme.text_inverse),
                    ),
                    Span::styled(
                        "  (OpenCode daemon offline; no model credentials discovered)",
                        Style::default().fg(self.theme.accent_amber),
                    ),
                ]));

                rt_lines.push(Line::from(vec![
                    Span::styled("  Policy:  ", Style::default().fg(self.theme.text_secondary)),
                    Span::styled(
                        "Fail-closed runtime contract active. Production mode will NOT fabricate AI responses.",
                        Style::default().fg(self.theme.text_primary),
                    ),
                ]));

                if let Some(err_msg) = self.error_message {
                    rt_lines.push(Line::from(vec![
                        Span::styled(
                            "  Notice:  ",
                            Style::default().fg(self.theme.text_secondary),
                        ),
                        Span::styled(err_msg, Style::default().fg(self.theme.text_muted)),
                    ]));
                }
            } else {
                rt_lines.push(Line::from(vec![
                    Span::styled(
                        "  Status:  ",
                        Style::default().fg(self.theme.text_secondary),
                    ),
                    Span::styled(
                        format!(
                            " [OK] {} external runtime(s) detected ",
                            live_runtimes.len()
                        ),
                        self.theme
                            .badge_style(self.theme.accent_emerald, self.theme.text_inverse),
                    ),
                ]));

                for rt in live_runtimes.iter().take(2) {
                    let ver = rt.version.as_deref().unwrap_or("unknown");
                    let avail_str = if rt.availability.is_available() {
                        "Available"
                    } else {
                        "Unavailable"
                    };
                    rt_lines.push(Line::from(vec![
                        Span::styled("  * ", Style::default().fg(self.theme.accent_emerald)),
                        Span::styled(
                            &rt.display_name,
                            Style::default()
                                .fg(self.theme.text_primary)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(" (v{ver}) — {avail_str}"),
                            Style::default().fg(self.theme.text_muted),
                        ),
                    ]));
                }
            }

            for (i, line) in rt_lines.iter().enumerate() {
                if (rt_inner.y + i as u16) < rt_inner.y + rt_inner.height {
                    buf.set_line(rt_inner.x, rt_inner.y + i as u16, line, rt_inner.width);
                }
            }
        }

        // Card 3: Setup Instructions & Quick Actions
        let action_area = content_chunks[chunk_idx];
        let action_block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_focused_style())
            .title(Span::styled(
                " Setup Instructions & Quick Actions ",
                Style::default()
                    .fg(self.theme.accent_emerald)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(self.theme.card_style());

        let action_inner = action_block.inner(action_area);
        action_block.render(action_area, buf);

        if action_inner.height > 0 {
            let action_lines = [
                Line::from(vec![
                    Span::styled(
                        "  1. Connect Live AI Models:",
                        Style::default()
                            .fg(self.theme.text_primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " Start OpenCode daemon (",
                        Style::default().fg(self.theme.text_secondary),
                    ),
                    Span::styled(
                        "opencode daemon start",
                        Style::default()
                            .fg(self.theme.accent_sky)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        ") or configure remote credentials.",
                        Style::default().fg(self.theme.text_secondary),
                    ),
                ]),
                Line::from(vec![Span::styled(
                    "  2. Explore Offline Without Credentials:",
                    Style::default()
                        .fg(self.theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                )]),
                Line::from(vec![
                    Span::styled(
                        "     Press ",
                        Style::default().fg(self.theme.text_secondary),
                    ),
                    Span::styled(
                        " [d] ",
                        self.theme
                            .badge_style(self.theme.accent_amber, self.theme.text_inverse),
                    ),
                    Span::styled(
                        " Launch interactive ",
                        Style::default().fg(self.theme.text_primary),
                    ),
                    Span::styled(
                        "Demo Mode",
                        Style::default()
                            .fg(self.theme.accent_amber)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " (simulated multi-agent execution, no API keys needed)",
                        Style::default().fg(self.theme.text_muted),
                    ),
                ]),
                Line::from(vec![
                    Span::styled(
                        "     Press ",
                        Style::default().fg(self.theme.text_secondary),
                    ),
                    Span::styled(
                        " [p] ",
                        self.theme
                            .badge_style(self.theme.accent_sky, self.theme.text_inverse),
                    ),
                    Span::styled(" Open ", Style::default().fg(self.theme.text_primary)),
                    Span::styled(
                        "Design Preview",
                        Style::default()
                            .fg(self.theme.accent_sky)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " (visual inspection of Quiet Precision themes, modals, diffs)",
                        Style::default().fg(self.theme.text_muted),
                    ),
                ]),
                Line::from(vec![
                    Span::styled(
                        "     Press ",
                        Style::default().fg(self.theme.text_secondary),
                    ),
                    Span::styled(
                        " [r] ",
                        self.theme
                            .badge_style(self.theme.accent_purple, self.theme.text_inverse),
                    ),
                    Span::styled(" Run ", Style::default().fg(self.theme.text_primary)),
                    Span::styled(
                        "System Diagnostics (Doctor)",
                        Style::default()
                            .fg(self.theme.accent_purple)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " to verify OS, Git, and terminal compatibility",
                        Style::default().fg(self.theme.text_muted),
                    ),
                ]),
                Line::from(vec![
                    Span::styled(
                        "     Press ",
                        Style::default().fg(self.theme.text_secondary),
                    ),
                    Span::styled(
                        " [t] ",
                        Style::default()
                            .fg(self.theme.accent_emerald)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " Toggle Theme (Dark / Light)    ",
                        Style::default().fg(self.theme.text_secondary),
                    ),
                    Span::styled(" Press ", Style::default().fg(self.theme.text_secondary)),
                    Span::styled(
                        " [q/Esc] ",
                        Style::default()
                            .fg(self.theme.accent_crimson)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " Exit cleanly",
                        Style::default().fg(self.theme.text_secondary),
                    ),
                ]),
            ];

            for (i, line) in action_lines.iter().enumerate() {
                if (action_inner.y + i as u16) < action_inner.y + action_inner.height {
                    buf.set_line(
                        action_inner.x,
                        action_inner.y + i as u16,
                        line,
                        action_inner.width,
                    );
                }
            }
        }

        // 3. Footer Bar
        let footer_hints = vec![
            Span::styled(
                " [d] ",
                Style::default()
                    .fg(self.theme.accent_amber)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Demo Mode  ",
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                " [p] ",
                Style::default()
                    .fg(self.theme.accent_sky)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Preview  ", Style::default().fg(self.theme.text_secondary)),
            Span::styled(
                " [r] ",
                Style::default()
                    .fg(self.theme.accent_purple)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Doctor  ", Style::default().fg(self.theme.text_secondary)),
            Span::styled(
                " [t] ",
                Style::default()
                    .fg(self.theme.accent_emerald)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Theme  ", Style::default().fg(self.theme.text_secondary)),
            Span::styled(
                " [q/Esc] ",
                Style::default()
                    .fg(self.theme.accent_crimson)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Exit", Style::default().fg(self.theme.text_secondary)),
        ];

        let left_footer = Line::from(footer_hints);
        buf.set_line(
            footer_area.x,
            footer_area.y,
            &left_footer,
            footer_area.width,
        );

        let right_metrics = vec![
            Span::styled(
                "tokens: unknown",
                Style::default().fg(self.theme.text_muted),
            ),
            Span::styled(
                "  [SETUP] ",
                self.theme
                    .badge_style(self.theme.accent_purple, self.theme.text_inverse),
            ),
        ];
        let right_footer = Line::from(right_metrics);
        let right_width = right_footer.width() as u16;

        if footer_area.width >= right_width + left_footer.width() as u16 + 2 {
            let right_x = footer_area.x + footer_area.width - right_width;
            buf.set_line(right_x, footer_area.y, &right_footer, right_width);
        }
    }
}
