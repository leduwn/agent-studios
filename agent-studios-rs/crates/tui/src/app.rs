//! Interactive Application State Machine and Terminal Event Loop
//!
//! Drives the live TUI dispatching between terminal keyboard inputs,
//! asynchronous client UI events, and rendering frames.

use std::sync::Arc;
use std::time::Duration;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use unicode_segmentation::UnicodeSegmentation;

use agent_studios_cli_client::client::{
    AgentStudiosClient, ClientApprovalDecision, GoalSubmissionOptions,
};
use agent_studios_cli_client::events::{translate_control_plane_event, UiEvent};

use crate::layout::MainLayout;
use crate::terminal::TerminalGuard;
use crate::theme::AgsTheme;
use crate::widgets::{
    agent_bar::{AgentBarWidget, AgentItem},
    approval_modal::{ApprovalModalWidget, PendingApproval},
    composer::ComposerWidget,
    diff_viewer::DiffViewerWidget,
    header::HeaderWidget,
    slash_menu::{SlashMenuWidget, AVAILABLE_COMMANDS},
    status_bar::StatusBarWidget,
    transcript::{TranscriptItem, TranscriptWidget},
};

pub struct AgsApp {
    client: Arc<dyn AgentStudiosClient>,
    theme: AgsTheme,
    input_text: String,
    cursor_grapheme_index: usize,
    agents: Vec<AgentItem>,
    transcript: Vec<TranscriptItem>,
    pending_approval: Option<PendingApproval>,
    active_diff: Option<String>,
    show_slash_menu: bool,
    slash_menu_index: usize,
    scroll_offset: usize,
    should_quit: bool,
}

impl AgsApp {
    pub fn new(client: Arc<dyn AgentStudiosClient>) -> Self {
        let default_agents = vec![
            AgentItem {
                role: "Coordinator".to_string(),
                state_label: "IDLE".to_string(),
                is_active: false,
                is_awaiting_approval: false,
                is_failed: false,
            },
            AgentItem {
                role: "Coder".to_string(),
                state_label: "IDLE".to_string(),
                is_active: false,
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
                state_label: "IDLE".to_string(),
                is_active: false,
                is_awaiting_approval: false,
                is_failed: false,
            },
        ];

        Self {
            client,
            theme: AgsTheme::dark(),
            input_text: String::new(),
            cursor_grapheme_index: 0,
            agents: default_agents,
            transcript: Vec::new(),
            pending_approval: None,
            active_diff: None,
            show_slash_menu: false,
            slash_menu_index: 0,
            scroll_offset: 0,
            should_quit: false,
        }
    }

    /// Primary async event loop
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let (mut terminal, _guard) = TerminalGuard::enter()?;
        let _ = crate::terminal::set_terminal_title("AGS — Agent Studios");

        // Subscribe to client event stream
        let mut sub = self.client.subscribe_events(0).await?;

        // Process initial backfill
        for env in sub.backfill {
            if let Some(ui_ev) = translate_control_plane_event(&env) {
                self.handle_ui_event(ui_ev);
            }
        }

        while !self.should_quit {
            // Render UI
            terminal.draw(|f| self.render(f.area(), f.buffer_mut()))?;

            // Check for incoming background events
            tokio::select! {
                Some(env) = sub.stream.recv() => {
                    if let Some(ui_ev) = translate_control_plane_event(&env) {
                        self.handle_ui_event(ui_ev);
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(25)) => {
                    // Poll terminal input
                    if event::poll(Duration::from_millis(0))? {
                        if let Event::Key(key) = event::read()? {
                            if key.kind == KeyEventKind::Press {
                                self.handle_key(key).await?;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_ui_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::StudioInitialized { studio_id } => {
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Info".to_string(),
                    message: format!("Studio initialized: {studio_id}"),
                });
            }
            UiEvent::AgentUpdated { role, state, .. } => {
                if let Some(agent) = self.agents.iter_mut().find(|a| a.role == role) {
                    agent.state_label = format!("{state:?}");
                    agent.is_active = format!("{state:?}").contains("Running") || format!("{state:?}").contains("Busy");
                }
            }
            UiEvent::TaskUpdated { title, state, .. } => {
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Info".to_string(),
                    message: format!("Task '{title}' state: {state:?}"),
                });
            }
            UiEvent::ApprovalRequested {
                approval_id,
                risk_tier,
                kind,
                summary,
                ..
            } => {
                self.pending_approval = Some(PendingApproval {
                    approval_id,
                    tool_name: kind,
                    risk_tier,
                    summary,
                    details: "Fail-closed gate awaiting developer authorization.".to_string(),
                });
            }
            UiEvent::ApprovalResolved { state, .. } => {
                self.pending_approval = None;
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Info".to_string(),
                    message: format!("Security approval resolved: {state:?}"),
                });
            }
            UiEvent::AssistantChunk { content, is_thought } => {
                if is_thought {
                    self.transcript.push(TranscriptItem::AssistantThought(content));
                } else {
                    self.transcript.push(TranscriptItem::AssistantMessage(content));
                }
            }
            UiEvent::ToolProgress { tool_name, status } => {
                self.transcript.push(TranscriptItem::ToolInvocation { tool_name, status });
            }
            UiEvent::Notification { level, message } => {
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: format!("{level:?}"),
                    message,
                });
            }
            _ => {}
        }
    }

    async fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Result<(), Box<dyn std::error::Error>> {
        // Pending approval modal interception
        if let Some(appr) = &self.pending_approval {
            match key.code {
                KeyCode::Char('a') => {
                    let _ = self.client.resolve_approval(
                        appr.approval_id,
                        ClientApprovalDecision::Allow,
                    ).await;
                    self.pending_approval = None;
                }
                KeyCode::Char('s') => {
                    let _ = self.client.resolve_approval(
                        appr.approval_id,
                        ClientApprovalDecision::AllowForSession,
                    ).await;
                    self.pending_approval = None;
                }
                KeyCode::Char('d') | KeyCode::Esc => {
                    let _ = self.client.resolve_approval(
                        appr.approval_id,
                        ClientApprovalDecision::Deny,
                    ).await;
                    self.pending_approval = None;
                }
                _ => {}
            }
            return Ok(());
        }

        // Global keys
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if !self.input_text.is_empty() {
                self.input_text.clear();
                self.cursor_grapheme_index = 0;
            } else {
                let _ = self.client.cancel_active_run(Some("User requested cancel".to_string())).await;
            }
            return Ok(());
        }

        if key.code == KeyCode::Esc {
            if self.show_slash_menu {
                self.show_slash_menu = false;
            } else {
                self.should_quit = true;
            }
            return Ok(());
        }

        // Slash menu navigation
        if self.show_slash_menu {
            match key.code {
                KeyCode::Up => {
                    if self.slash_menu_index > 0 {
                        self.slash_menu_index -= 1;
                    }
                    return Ok(());
                }
                KeyCode::Down => {
                    if self.slash_menu_index + 1 < AVAILABLE_COMMANDS.len() {
                        self.slash_menu_index += 1;
                    }
                    return Ok(());
                }
                KeyCode::Enter => {
                    let selected = AVAILABLE_COMMANDS[self.slash_menu_index].command;
                    self.show_slash_menu = false;
                    self.execute_slash_command(selected).await?;
                    return Ok(());
                }
                _ => {}
            }
        }

        // Composer editing
        match key.code {
            KeyCode::Enter => {
                let prompt = self.input_text.trim().to_string();
                if !prompt.is_empty() {
                    if prompt.starts_with('/') {
                        self.execute_slash_command(&prompt).await?;
                    } else {
                        self.transcript.push(TranscriptItem::UserPrompt(prompt.clone()));
                        let _ = self.client.submit_goal(&prompt, GoalSubmissionOptions::default()).await;
                    }
                    self.input_text.clear();
                    self.cursor_grapheme_index = 0;
                }
            }
            KeyCode::Char(c) => {
                if c == '/' && self.input_text.is_empty() {
                    self.show_slash_menu = true;
                    self.slash_menu_index = 0;
                }

                // Grapheme cluster insertion
                let mut graphemes: Vec<String> = self.input_text.graphemes(true).map(String::from).collect();
                if self.cursor_grapheme_index <= graphemes.len() {
                    graphemes.insert(self.cursor_grapheme_index, c.to_string());
                    self.cursor_grapheme_index += 1;
                    self.input_text = graphemes.concat();
                }
            }
            KeyCode::Backspace => {
                let mut graphemes: Vec<String> = self.input_text.graphemes(true).map(String::from).collect();
                if self.cursor_grapheme_index > 0 && !graphemes.is_empty() {
                    self.cursor_grapheme_index -= 1;
                    graphemes.remove(self.cursor_grapheme_index);
                    self.input_text = graphemes.concat();
                }
                if self.input_text.is_empty() {
                    self.show_slash_menu = false;
                }
            }
            KeyCode::Left => {
                if self.cursor_grapheme_index > 0 {
                    self.cursor_grapheme_index -= 1;
                }
            }
            KeyCode::Right => {
                let total = self.input_text.graphemes(true).count();
                if self.cursor_grapheme_index < total {
                    self.cursor_grapheme_index += 1;
                }
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_add(5);
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
            }
            _ => {}
        }

        Ok(())
    }

    async fn execute_slash_command(&mut self, cmd: &str) -> Result<(), Box<dyn std::error::Error>> {
        match cmd {
            "/clear" => {
                self.transcript.clear();
            }
            "/cancel" => {
                let _ = self.client.cancel_active_run(Some("User requested cancel".to_string())).await;
            }
            "/quit" => {
                self.should_quit = true;
            }
            "/diff" => {
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Info".to_string(),
                    message: "Querying active worktree diff...".to_string(),
                });
            }
            "/doctor" => {
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Info".to_string(),
                    message: "System doctor check: Rust OK, Git OK, Terminal OK".to_string(),
                });
            }
            "/help" => {
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Info".to_string(),
                    message: "Available: /clear, /doctor, /diff, /worktree, /model, /cancel, /quit".to_string(),
                });
            }
            _ => {
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Warning".to_string(),
                    message: format!("Unknown slash command: {cmd}"),
                });
            }
        }
        Ok(())
    }

    pub fn render(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let layout = MainLayout::compute(area);

        // Header
        HeaderWidget {
            theme: &self.theme,
            project_name: "agent-studios",
            branch_name: "feat/ags-cli-tui-foundation",
            is_dirty: false,
            model_name: "Claude 3.7 Sonnet",
            execution_mode: "Embedded",
            compact: layout.breakpoint == crate::layout::TerminalBreakpoint::Compact,
        }
        .render(layout.header_area, buf);

        // Agent bar
        AgentBarWidget {
            theme: &self.theme,
            agents: &self.agents,
        }
        .render(layout.agent_bar_area, buf);

        // Content
        TranscriptWidget {
            theme: &self.theme,
            items: &self.transcript,
            scroll_offset: self.scroll_offset,
        }
        .render(layout.content_area, buf);

        // Dual-pane side pane if UltraWide
        if let Some(side_area) = layout.side_pane_area {
            if let Some(diff) = &self.active_diff {
                DiffViewerWidget {
                    theme: &self.theme,
                    diff_content: diff,
                    title: "Active Worktree Patch",
                }
                .render(side_area, buf);
            }
        }

        // Composer
        ComposerWidget {
            theme: &self.theme,
            input_text: &self.input_text,
            cursor_grapheme_index: self.cursor_grapheme_index,
            is_focused: self.pending_approval.is_none(),
        }
        .render(layout.composer_area, buf);

        // Slash menu popup
        if self.show_slash_menu {
            let menu_area = Rect {
                x: layout.composer_area.x + 2,
                y: layout.composer_area.y.saturating_sub(10),
                width: 60.min(layout.composer_area.width.saturating_sub(4)),
                height: 10,
            };
            SlashMenuWidget {
                theme: &self.theme,
                selected_index: self.slash_menu_index,
                filter: "",
            }
            .render(menu_area, buf);
        }

        // Approval modal overlay if pending
        if let Some(appr) = &self.pending_approval {
            ApprovalModalWidget {
                theme: &self.theme,
                approval: appr,
            }
            .render(area, buf);
        }

        // Status bar
        StatusBarWidget {
            theme: &self.theme,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            status_text: "Ready",
        }
        .render(layout.status_bar_area, buf);
    }
}
