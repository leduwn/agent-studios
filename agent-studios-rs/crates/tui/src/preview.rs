//! Offline Design Previewer for Agent Studios CLI (`ags --preview`)
//!
//! Enables developers and designers to visually audit the TUI components,
//! color themes, responsive breakpoints, and security gate modals without
//! needing a running agent backend.

use agent_studios_protocol::id::ApprovalId;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use std::io;

use crate::layout::MainLayout;
use crate::terminal::TerminalGuard;
use crate::theme::{AgsTheme, ThemeMode};
use crate::widgets::{
    agent_bar::AgentItem,
    approval_modal::{ApprovalModalWidget, PendingApproval},
    composer::ComposerWidget,
    diff_viewer::DiffViewerWidget,
    header::HeaderWidget,
    slash_menu::{AVAILABLE_COMMANDS, SlashMenuWidget},
    status_bar::StatusBarWidget,
    transcript::{TranscriptItem, TranscriptWidget},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewTab {
    Transcript,
    SecurityGate,
    UnifiedDiff,
    ThemePalette,
    ResponsiveLayout,
}

pub struct OfflinePreviewApp {
    pub theme: AgsTheme,
    pub active_tab: PreviewTab,
    pub input_text: String,
    pub cursor_grapheme_index: usize,
    pub show_slash_menu: bool,
    pub slash_menu_index: usize,
    pub sample_agents: Vec<AgentItem>,
    pub sample_transcript: Vec<TranscriptItem>,
    pub sample_approval: PendingApproval,
    pub sample_diff: String,
    pub should_quit: bool,
}

impl Default for OfflinePreviewApp {
    fn default() -> Self {
        Self::new()
    }
}

impl OfflinePreviewApp {
    pub fn new() -> Self {
        let sample_agents = vec![
            AgentItem {
                role: "Coordinator".to_string(),
                state_label: "COMPLETED".to_string(),
                is_active: false,
                is_awaiting_approval: false,
                is_failed: false,
            },
            AgentItem {
                role: "Coder".to_string(),
                state_label: "BUSY".to_string(),
                is_active: true,
                is_awaiting_approval: false,
                is_failed: false,
            },
            AgentItem {
                role: "Tester".to_string(),
                state_label: "IDLE".to_string(),
                is_active: false,
                is_awaiting_approval: false,
                is_failed: false,
            },
            AgentItem {
                role: "Reviewer".to_string(),
                state_label: "AWAITING".to_string(),
                is_active: false,
                is_awaiting_approval: true,
                is_failed: false,
            },
        ];

        let sample_transcript = vec![
            TranscriptItem::UserPrompt("Xây dựng Agent Studios CLI và kiểm tra hiển thị tiếng Việt có dấu chuẩn Unicode NFC: á, ắ, ặ, ấ, ầ, ổ, ợ, ỹ.".to_string()),
            TranscriptItem::AssistantThought("Analyzing typography, grapheme clusters, and ensuring terminal window title avoids ANSI injection...".to_string()),
            TranscriptItem::AssistantMessage("Đã cấu hình hệ thống giao diện TUI theo triết lý Quiet Precision.\nToàn bộ phông chữ tiếng Việt hiển thị chính xác, không tràn viền, không mất dấu.".to_string()),
            TranscriptItem::ToolInvocation {
                tool_name: "git_diff".to_string(),
                status: "success (3 files changed, +142 lines)".to_string(),
            },
            TranscriptItem::SystemNotice {
                level: "Info".to_string(),
                message: "Workspace sandboxed in isolated Git worktree: wt-coder-01".to_string(),
            },
        ];

        let sample_approval = PendingApproval {
            approval_id: ApprovalId::new(),
            tool_name: "write_file".to_string(),
            risk_tier: "WorkspaceMutating".to_string(),
            summary: "Overwrite configuration file `crates/tui/Cargo.toml` with updated dependency specs.".to_string(),
            details: "+ ratatui = \"0.30.2\"\n+ crossterm = \"0.29.0\"\n+ unicode-width = \"0.2\"\n+ unicode-segmentation = \"1.12\"".to_string(),
        };

        let sample_diff = r#"diff --git a/crates/tui/src/theme.rs b/crates/tui/src/theme.rs
index 8f3c2a1..9e4b7c2 100644
--- a/crates/tui/src/theme.rs
+++ b/crates/tui/src/theme.rs
@@ -10,6 +10,12 @@ pub struct AgsTheme {
+    pub accent_emerald: Color, // #3fb950 Quiet Precision focus
+    pub accent_amber: Color,   // #d29922 Mutating action warning
+    pub accent_crimson: Color, // #f85149 Destructive danger gate
+    pub bg_base: Color,        // #0d1117 85% slate carbon
-    pub placeholder: bool,
+    pub is_production_grade: bool,
 }"#
        .to_string();

        Self {
            theme: AgsTheme::dark(),
            active_tab: PreviewTab::Transcript,
            input_text: "Triển khai TUI tiếng Việt chuẩn NFC không lỗi dấu".to_string(),
            cursor_grapheme_index: 46,
            show_slash_menu: false,
            slash_menu_index: 0,
            sample_agents,
            sample_transcript,
            sample_approval,
            sample_diff,
            should_quit: false,
        }
    }

    /// Runs interactive offline preview loop until user presses 'q' or 'Esc'
    pub fn run(&mut self) -> io::Result<()> {
        let (mut terminal, _guard) = TerminalGuard::enter()?;
        let _ = crate::terminal::set_terminal_title("AGS CLI — Offline Design Preview");

        while !self.should_quit {
            terminal.draw(|f| self.render(f.area(), f.buffer_mut()))?;
            self.handle_input()?;
        }

        Ok(())
    }

    pub fn handle_input(&mut self) -> io::Result<()> {
        if event::poll(std::time::Duration::from_millis(50))? {
            let key = match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => k,
                _ => return Ok(()),
            };

            // Global quit
            if key.code == KeyCode::Char('q')
                || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c'))
            {
                self.should_quit = true;
                return Ok(());
            }

            // Tab switching
            match key.code {
                KeyCode::Char('1') => self.active_tab = PreviewTab::Transcript,
                KeyCode::Char('2') => self.active_tab = PreviewTab::SecurityGate,
                KeyCode::Char('3') => self.active_tab = PreviewTab::UnifiedDiff,
                KeyCode::Char('4') => self.active_tab = PreviewTab::ThemePalette,
                KeyCode::Char('5') => self.active_tab = PreviewTab::ResponsiveLayout,
                KeyCode::Tab => {
                    self.active_tab = match self.active_tab {
                        PreviewTab::Transcript => PreviewTab::SecurityGate,
                        PreviewTab::SecurityGate => PreviewTab::UnifiedDiff,
                        PreviewTab::UnifiedDiff => PreviewTab::ThemePalette,
                        PreviewTab::ThemePalette => PreviewTab::ResponsiveLayout,
                        PreviewTab::ResponsiveLayout => PreviewTab::Transcript,
                    };
                }
                KeyCode::Char('t') => {
                    // Toggle theme mode
                    self.theme = match self.theme.mode {
                        ThemeMode::Dark => AgsTheme::light(),
                        ThemeMode::Light => AgsTheme::ansi16(),
                        ThemeMode::Ansi16 => AgsTheme::no_color(),
                        ThemeMode::NoColor => AgsTheme::dark(),
                    };
                }
                KeyCode::Char('/') => {
                    self.show_slash_menu = !self.show_slash_menu;
                }
                KeyCode::Up => {
                    if self.show_slash_menu && self.slash_menu_index > 0 {
                        self.slash_menu_index -= 1;
                    }
                }
                KeyCode::Down => {
                    if self.show_slash_menu && self.slash_menu_index + 1 < AVAILABLE_COMMANDS.len()
                    {
                        self.slash_menu_index += 1;
                    }
                }
                KeyCode::Esc => {
                    if self.show_slash_menu {
                        self.show_slash_menu = false;
                    } else {
                        self.should_quit = true;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn render(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let layout = MainLayout::compute(area);

        // Top Header
        HeaderWidget {
            theme: &self.theme,
            project_name: "agent-studios",
            branch_name: "feat/ags-cli-tui-foundation",
            is_dirty: true,
            model_name: "Claude 3.7 Sonnet",
            execution_mode: "Preview Mode",
            compact: layout.breakpoint == crate::layout::TerminalBreakpoint::Compact,
        }
        .render(layout.header_area, buf);

        // Top Navigation Ribbon & Agent ribbon
        let nav_spans = vec![
            Span::styled(
                " [1] Transcript ",
                if self.active_tab == PreviewTab::Transcript {
                    self.theme
                        .badge_style(self.theme.accent_emerald, self.theme.text_inverse)
                } else {
                    Style::default().fg(self.theme.text_secondary)
                },
            ),
            Span::styled(
                " [2] Security Gate ",
                if self.active_tab == PreviewTab::SecurityGate {
                    self.theme
                        .badge_style(self.theme.accent_amber, self.theme.text_inverse)
                } else {
                    Style::default().fg(self.theme.text_secondary)
                },
            ),
            Span::styled(
                " [3] Unified Diff ",
                if self.active_tab == PreviewTab::UnifiedDiff {
                    self.theme
                        .badge_style(self.theme.accent_sky, self.theme.text_inverse)
                } else {
                    Style::default().fg(self.theme.text_secondary)
                },
            ),
            Span::styled(
                " [4] Theme Palette ",
                if self.active_tab == PreviewTab::ThemePalette {
                    self.theme
                        .badge_style(self.theme.accent_purple, self.theme.text_inverse)
                } else {
                    Style::default().fg(self.theme.text_secondary)
                },
            ),
            Span::styled(
                " [5] Responsive Inspector ",
                if self.active_tab == PreviewTab::ResponsiveLayout {
                    self.theme
                        .badge_style(self.theme.text_primary, self.theme.text_inverse)
                } else {
                    Style::default().fg(self.theme.text_secondary)
                },
            ),
            Span::styled(
                " | (t) Theme | (q) Exit",
                Style::default().fg(self.theme.text_muted),
            ),
        ];
        buf.set_line(
            layout.agent_bar_area.x,
            layout.agent_bar_area.y,
            &Line::from(nav_spans),
            layout.agent_bar_area.width,
        );

        // Active Content Body
        match self.active_tab {
            PreviewTab::Transcript => {
                TranscriptWidget {
                    theme: &self.theme,
                    items: &self.sample_transcript,
                    scroll_offset: 0,
                }
                .render(layout.content_area, buf);

                if let Some(side_area) = layout.side_pane_area {
                    DiffViewerWidget {
                        theme: &self.theme,
                        diff_content: &self.sample_diff,
                        title: "crates/tui/src/theme.rs",
                    }
                    .render(side_area, buf);
                }
            }
            PreviewTab::SecurityGate => {
                TranscriptWidget {
                    theme: &self.theme,
                    items: &self.sample_transcript,
                    scroll_offset: 0,
                }
                .render(layout.content_area, buf);

                ApprovalModalWidget {
                    theme: &self.theme,
                    approval: &self.sample_approval,
                }
                .render(area, buf);
            }
            PreviewTab::UnifiedDiff => {
                DiffViewerWidget {
                    theme: &self.theme,
                    diff_content: &self.sample_diff,
                    title: "crates/tui/src/theme.rs",
                }
                .render(layout.content_area, buf);
            }
            PreviewTab::ThemePalette => {
                self.render_palette_inspector(layout.content_area, buf);
            }
            PreviewTab::ResponsiveLayout => {
                self.render_layout_inspector(layout.content_area, buf, &layout);
            }
        }

        // Composer
        ComposerWidget {
            theme: &self.theme,
            input_text: &self.input_text,
            cursor_grapheme_index: self.cursor_grapheme_index,
            is_focused: true,
        }
        .render(layout.composer_area, buf);

        // Slash command popup overlay if active
        if self.show_slash_menu {
            let menu_area = Rect {
                x: layout.composer_area.x + 2,
                y: layout.composer_area.y.saturating_sub(12),
                width: 72.min(layout.composer_area.width.saturating_sub(4)),
                height: 12,
            };
            SlashMenuWidget {
                theme: &self.theme,
                selected_index: self.slash_menu_index,
                filter: "",
            }
            .render(menu_area, buf);
        }

        // Status bar
        StatusBarWidget {
            theme: &self.theme,
            input_tokens: 34_820,
            output_tokens: 1_490,
            cost_usd: 0.126,
            status_text: format!("{:?} Theme", self.theme.mode).as_str(),
        }
        .render(layout.status_bar_area, buf);
    }

    fn render_palette_inspector(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_subtle_style())
            .title(Span::styled(
                " Semantic Color Token Palette ",
                Style::default().fg(self.theme.accent_purple),
            ))
            .style(self.theme.card_style());

        let inner = block.inner(area);
        block.render(area, buf);

        let lines = vec![
            Line::from(vec![
                Span::styled(
                    "Theme Mode: ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    format!("{:?}", self.theme.mode),
                    Style::default()
                        .fg(self.theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " (Press 't' to cycle Dark -> Light -> Ansi16 -> NoColor)",
                    Style::default().fg(self.theme.text_muted),
                ),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::styled(
                    "  bg_base:        ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(" ■■■■ ", Style::default().fg(self.theme.bg_base)),
                Span::styled(
                    "  border_focused: ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(" ■■■■ ", Style::default().fg(self.theme.border_focused)),
            ]),
            Line::from(vec![
                Span::styled(
                    "  bg_card:        ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(" ■■■■ ", Style::default().fg(self.theme.bg_card)),
                Span::styled(
                    "  accent_emerald: ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    " ■■■■ (Focus/Allow)",
                    Style::default().fg(self.theme.accent_emerald),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "  bg_surface:     ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(" ■■■■ ", Style::default().fg(self.theme.bg_surface)),
                Span::styled(
                    "  accent_amber:   ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    " ■■■■ (Mutating)",
                    Style::default().fg(self.theme.accent_amber),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "  border_subtle:  ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(" ■■■■ ", Style::default().fg(self.theme.border_subtle)),
                Span::styled(
                    "  accent_crimson: ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    " ■■■■ (Danger/Deny)",
                    Style::default().fg(self.theme.accent_crimson),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "  text_primary:   ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    " Primary Text Display Sample",
                    Style::default()
                        .fg(self.theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "  text_secondary: ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    " Secondary Metadata and Labels",
                    Style::default().fg(self.theme.text_secondary),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "  text_muted:     ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    " Muted Timestamps and Line Numbers",
                    Style::default().fg(self.theme.text_muted),
                ),
            ]),
        ];

        Paragraph::new(lines).render(inner, buf);
    }

    fn render_layout_inspector(
        &self,
        area: Rect,
        buf: &mut ratatui::buffer::Buffer,
        layout: &MainLayout,
    ) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border_subtle_style())
            .title(Span::styled(
                " Responsive Layout Engine Inspector ",
                Style::default().fg(self.theme.accent_sky),
            ))
            .style(self.theme.card_style());

        let inner = block.inner(area);
        block.render(area, buf);

        let lines = vec![
            Line::from(vec![
                Span::styled(
                    "Terminal Breakpoint: ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    format!("{:?}", layout.breakpoint),
                    Style::default()
                        .fg(self.theme.accent_emerald)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::styled(
                    "  Dimensions:        ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    format!("width = {}, height = {}", area.width, area.height),
                    Style::default().fg(self.theme.text_primary),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "  Header Area:       ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    format!(
                        "{}x{} at ({},{})",
                        layout.header_area.width,
                        layout.header_area.height,
                        layout.header_area.x,
                        layout.header_area.y
                    ),
                    Style::default().fg(self.theme.text_muted),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "  Content Area:      ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    format!(
                        "{}x{} at ({},{})",
                        layout.content_area.width,
                        layout.content_area.height,
                        layout.content_area.x,
                        layout.content_area.y
                    ),
                    Style::default().fg(self.theme.text_muted),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "  Side Pane Area:    ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    format!("{:?}", layout.side_pane_area),
                    Style::default().fg(self.theme.text_muted),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "  Composer Area:     ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    format!(
                        "{}x{} at ({},{})",
                        layout.composer_area.width,
                        layout.composer_area.height,
                        layout.composer_area.x,
                        layout.composer_area.y
                    ),
                    Style::default().fg(self.theme.text_muted),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "  Dual-Pane Status:  ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    if layout.side_pane_area.is_some() {
                        "ENABLED (>=160 cols)"
                    } else {
                        "DISABLED (<160 cols)"
                    },
                    Style::default().fg(if layout.side_pane_area.is_some() {
                        self.theme.accent_emerald
                    } else {
                        self.theme.accent_amber
                    }),
                ),
            ]),
        ];

        Paragraph::new(lines).render(inner, buf);
    }
}
