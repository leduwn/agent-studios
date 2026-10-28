//! Interactive Application State Machine and Terminal Event Loop
//!
//! Drives the live TUI dispatching between terminal keyboard inputs,
//! asynchronous client UI events, and rendering frames.

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use std::sync::Arc;
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;

use agent_studios_cli_client::client::{
    AgentStudiosClient, ClientApprovalDecision, GoalSubmissionOptions,
};
use agent_studios_cli_client::events::{UiEvent, translate_control_plane_event};

use crate::layout::MainLayout;
use crate::terminal::TerminalGuard;
use crate::theme::AgsTheme;
use crate::widgets::{
    agent_bar::{AgentBarWidget, AgentItem},
    approval_modal::{ApprovalModalWidget, PendingApproval},
    composer::ComposerWidget,
    diff_viewer::DiffViewerWidget,
    header::HeaderWidget,
    slash_menu::{AVAILABLE_COMMANDS, SlashMenuWidget},
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
    last_ctrl_c: Option<std::time::Instant>,
    pub project_name: String,
    pub branch_name: String,
    pub is_dirty: bool,
    pub model_name: String,
    pub execution_mode: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

fn detect_workspace_info() -> (String, String, bool) {
    let project_name = detect_project_name();
    let (branch_name, is_dirty) = detect_git_status();
    (project_name, branch_name, is_dirty)
}

fn detect_project_name() -> String {
    // Try git rev-parse --show-toplevel
    let git_top = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                None
            } else {
                std::path::Path::new(&s)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            }
        });

    if let Some(name) = git_top {
        return name;
    }

    // Fallback to current working directory
    if let Some(name) = std::env::current_dir()
        .ok()
        .and_then(|c| c.file_name().map(|n| n.to_string_lossy().to_string()))
    {
        return name;
    }

    "Unknown project".to_string()
}

fn detect_git_status() -> (String, bool) {
    let branch = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() { Some(s) } else { None }
            } else {
                None
            }
        })
        .unwrap_or_else(|| "Unknown branch".to_string());

    let is_dirty = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|out| out.status.success() && !out.stdout.is_empty())
        .unwrap_or(false);

    (branch, is_dirty)
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

        let (project_name, branch_name, is_dirty) = detect_workspace_info();
        let execution_mode = format!("{} [{}]", client.execution_mode(), client.runtime_mode());

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
            last_ctrl_c: None,
            project_name,
            branch_name,
            is_dirty,
            model_name: "Not configured".to_string(),
            execution_mode,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
        }
    }

    pub fn transcript(&self) -> &[TranscriptItem] {
        &self.transcript
    }

    pub fn pending_approval(&self) -> Option<&PendingApproval> {
        self.pending_approval.as_ref()
    }

    pub fn set_pending_approval(&mut self, approval: Option<PendingApproval>) {
        self.pending_approval = approval;
    }

    pub fn set_workspace_info(&mut self, project: String, branch: String, is_dirty: bool) {
        self.project_name = project;
        self.branch_name = branch;
        self.is_dirty = is_dirty;
    }

    pub fn set_model_name(&mut self, model: String) {
        self.model_name = model;
    }

    pub fn update_usage(&mut self, input_tokens: u64, output_tokens: u64, cost_usd: f64) {
        self.input_tokens = input_tokens;
        self.output_tokens = output_tokens;
        self.cost_usd = cost_usd;
    }

    /// Refresh workspace metadata from client
    pub async fn refresh_workspace_info(&mut self) {
        if let Ok(info) = self.client.get_workspace_info().await {
            self.project_name = info.project_name;
            self.branch_name = info
                .branch_name
                .unwrap_or_else(|| "Unknown branch".to_string());
            self.is_dirty = info.is_dirty;
        }
    }

    /// Primary async event loop
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let (mut terminal, _guard) = TerminalGuard::enter()?;
        let _ = crate::terminal::set_terminal_title("AGS — Agent Studios");

        // Query authoritative workspace information from client
        self.refresh_workspace_info().await;

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
                        match event::read()? {
                            Event::Key(key) if key.kind == KeyEventKind::Press => {
                                self.handle_key(key).await?;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn set_input_text(&mut self, text: String) {
        self.cursor_grapheme_index = text.chars().count();
        self.input_text = text;
    }

    pub fn input_text(&self) -> &str {
        &self.input_text
    }

    pub fn agents(&self) -> &[AgentItem] {
        &self.agents
    }

    pub fn agents_mut(&mut self) -> &mut [AgentItem] {
        &mut self.agents
    }

    pub fn handle_ui_event(&mut self, event: UiEvent) {
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
                    agent.is_active = format!("{state:?}").contains("Running")
                        || format!("{state:?}").contains("Busy");
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
            UiEvent::AssistantChunk {
                content,
                is_thought,
            } => {
                if is_thought {
                    if let Some(TranscriptItem::AssistantThought(existing)) =
                        self.transcript.last_mut()
                    {
                        existing.push_str(&content);
                    } else {
                        self.transcript
                            .push(TranscriptItem::AssistantThought(content));
                    }
                } else if let Some(TranscriptItem::AssistantMessage(existing)) =
                    self.transcript.last_mut()
                {
                    existing.push_str(&content);
                } else {
                    self.transcript
                        .push(TranscriptItem::AssistantMessage(content));
                }
            }
            UiEvent::ToolProgress { tool_name, status } => {
                self.transcript
                    .push(TranscriptItem::ToolInvocation { tool_name, status });
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

    pub async fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Pending approval modal interception
        if let Some(appr) = &self.pending_approval {
            let decision = match key.code {
                KeyCode::Char('a') => Some(ClientApprovalDecision::Allow),
                KeyCode::Char('s') => Some(ClientApprovalDecision::AllowForSession),
                KeyCode::Char('d') | KeyCode::Esc => Some(ClientApprovalDecision::Deny),
                _ => None,
            };

            if let Some(decision) = decision {
                match self
                    .client
                    .resolve_approval(appr.approval_id, decision)
                    .await
                {
                    Ok(()) => {
                        self.pending_approval = None;
                    }
                    Err(e) => {
                        self.transcript.push(TranscriptItem::SystemNotice {
                            level: "Error".to_string(),
                            message: format!("Action failed: {e}"),
                        });
                        // Modal stays open so developer knows authorization was not delivered
                    }
                }
            }
            return Ok(());
        }

        // Global keys: two-stage Ctrl+C semantics
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if !self.input_text.is_empty() {
                self.input_text.clear();
                self.cursor_grapheme_index = 0;
                return Ok(());
            }

            let has_active_agent = self.agents.iter().any(|a| a.is_active);
            let recently_pressed = self
                .last_ctrl_c
                .map(|t| t.elapsed() < Duration::from_millis(1500))
                .unwrap_or(false);

            if recently_pressed {
                self.should_quit = true;
                return Ok(());
            }

            self.last_ctrl_c = Some(std::time::Instant::now());
            match self
                .client
                .cancel_active_run(Some("User requested cancel".to_string()))
                .await
            {
                Err(e) => {
                    self.transcript.push(TranscriptItem::SystemNotice {
                        level: "Error".to_string(),
                        message: format!("Action failed: {e}"),
                    });
                }
                Ok(()) => {
                    if has_active_agent {
                        self.transcript.push(TranscriptItem::SystemNotice {
                            level: "Warning".to_string(),
                            message: "Run cancelled. Press Ctrl+C again within 1.5s to exit."
                                .to_string(),
                        });
                    } else {
                        self.should_quit = true;
                    }
                }
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
                        self.scroll_offset = 0;
                        self.transcript
                            .push(TranscriptItem::UserPrompt(prompt.clone()));
                        if let Err(e) = self
                            .client
                            .submit_goal(&prompt, GoalSubmissionOptions::default())
                            .await
                        {
                            self.transcript.push(TranscriptItem::SystemNotice {
                                level: "Error".to_string(),
                                message: format!("Action failed: {e}"),
                            });
                        }
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
                let mut graphemes: Vec<String> =
                    self.input_text.graphemes(true).map(String::from).collect();
                if self.cursor_grapheme_index <= graphemes.len() {
                    graphemes.insert(self.cursor_grapheme_index, c.to_string());
                    self.cursor_grapheme_index += 1;
                    self.input_text = graphemes.concat();
                }
            }
            KeyCode::Backspace => {
                let mut graphemes: Vec<String> =
                    self.input_text.graphemes(true).map(String::from).collect();
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

    pub async fn execute_slash_command(
        &mut self,
        cmd: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root_cmd = cmd.split_whitespace().next().unwrap_or(cmd);
        match root_cmd {
            "/clear" => {
                self.transcript.clear();
            }
            "/cancel" => {
                match self
                    .client
                    .cancel_active_run(Some("User requested cancel".to_string()))
                    .await
                {
                    Ok(()) => {
                        self.transcript.push(TranscriptItem::SystemNotice {
                            level: "Info".to_string(),
                            message: "Active agent execution cancelled.".to_string(),
                        });
                    }
                    Err(e) => {
                        self.transcript.push(TranscriptItem::SystemNotice {
                            level: "Error".to_string(),
                            message: format!("Action failed: {e}"),
                        });
                    }
                }
            }
            "/quit" => {
                self.should_quit = true;
            }
            "/help" => {
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Info".to_string(),
                    message: "--- Agent Studios CLI Help ---".to_string(),
                });
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Info".to_string(),
                    message: "Keybindings: [Enter] Submit prompt/command | [/] Slash menu | [Ctrl+C] Clear/Cancel | [Esc] Exit | [PageUp/Down] Scroll".to_string(),
                });
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Info".to_string(),
                    message: "Security Gate: [a] Allow once | [s] Allow for session | [d/Esc] Deny action".to_string(),
                });
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Info".to_string(),
                    message: "Commands: /help, /clear, /doctor, /status, /cancel, /quit [Ready] | /diff, /model, /worktree, /reconcile [Planned]".to_string(),
                });
            }
            "/status" => {
                self.refresh_workspace_info().await;
                let active_agents = self.agents.iter().filter(|a| a.is_active).count();
                let gate_status = if self.pending_approval.is_some() {
                    "1 pending authorization"
                } else {
                    "None"
                };
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Info".to_string(),
                    message: format!(
                        "Studio Session: ID={}, Mode={}, Project='{}', Branch='{}' ({}), Active Agents={}/{}, Gate={}",
                        self.client.studio_id(),
                        self.execution_mode,
                        self.project_name,
                        self.branch_name,
                        if self.is_dirty { "dirty" } else { "clean" },
                        active_agents,
                        self.agents.len(),
                        gate_status
                    ),
                });
            }
            "/doctor" => {
                self.refresh_workspace_info().await;
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Info".to_string(),
                    message: "--- Running Agent Studios Diagnostics ---".to_string(),
                });
                match std::env::current_dir() {
                    Ok(path) => {
                        self.transcript.push(TranscriptItem::SystemNotice {
                            level: "Info".to_string(),
                            message: format!(
                                "[OK] Workspace directory accessible: {}",
                                path.display()
                            ),
                        });
                    }
                    Err(e) => {
                        self.transcript.push(TranscriptItem::SystemNotice {
                            level: "Error".to_string(),
                            message: format!("[FAIL] Workspace directory error: {e}"),
                        });
                    }
                }
                match std::process::Command::new("git").arg("--version").output() {
                    Ok(out) if out.status.success() => {
                        let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        self.transcript.push(TranscriptItem::SystemNotice {
                            level: "Info".to_string(),
                            message: format!(
                                "[OK] Git installed: {ver} (branch: {}, {})",
                                self.branch_name,
                                if self.is_dirty { "dirty" } else { "clean" }
                            ),
                        });
                    }
                    _ => {
                        self.transcript.push(TranscriptItem::SystemNotice {
                            level: "Warning".to_string(),
                            message: "[WARN] Git binary not available or repository inaccessible"
                                .to_string(),
                        });
                    }
                }
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Info".to_string(),
                    message: format!(
                        "[OK] Client runtime: mode={}, studio_id={}",
                        self.execution_mode,
                        self.client.studio_id()
                    ),
                });
                if self.model_name == "Not configured" || self.model_name.is_empty() {
                    self.transcript.push(TranscriptItem::SystemNotice {
                        level: "Warning".to_string(),
                        message: "[WARN] LLM Model: Not configured (run with configured provider or pass model option)".to_string(),
                    });
                } else {
                    self.transcript.push(TranscriptItem::SystemNotice {
                        level: "Info".to_string(),
                        message: format!("[OK] LLM Model: {}", self.model_name),
                    });
                }
            }
            "/diff" => {
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Warning".to_string(),
                    message: "Command '/diff' is planned: Reviewing uncommitted changes in worktree requires external workspace reconciler integration (planned for future milestone).".to_string(),
                });
            }
            "/model" => {
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Warning".to_string(),
                    message: "Command '/model' is planned: Inspecting or switching active model requires runtime model manager integration (planned for future milestone).".to_string(),
                });
            }
            "/worktree" => {
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Warning".to_string(),
                    message: "Command '/worktree' is planned: Managing isolated Git worktrees requires external workspace reconciler integration (planned for future milestone).".to_string(),
                });
            }
            "/reconcile" => {
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Warning".to_string(),
                    message: "Command '/reconcile' is planned: Reconciling or merging agent patches requires external workspace reconciler integration (planned for future milestone).".to_string(),
                });
            }
            _ => {
                self.transcript.push(TranscriptItem::SystemNotice {
                    level: "Warning".to_string(),
                    message: format!(
                        "Unknown slash command: {cmd}. Type /help for available commands."
                    ),
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
            project_name: &self.project_name,
            branch_name: &self.branch_name,
            is_dirty: self.is_dirty,
            model_name: &self.model_name,
            execution_mode: &self.execution_mode,
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
        if let (Some(side_area), Some(diff)) = (layout.side_pane_area, &self.active_diff) {
            DiffViewerWidget {
                theme: &self.theme,
                diff_content: diff,
                title: "Active Worktree Patch",
            }
            .render(side_area, buf);
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

        // Approval modal overlay if pending
        if let Some(appr) = &self.pending_approval {
            ApprovalModalWidget {
                theme: &self.theme,
                approval: appr,
            }
            .render(area, buf);
        }

        // Status bar
        let status_text = if self.pending_approval.is_some() {
            "AWAITING APPROVAL"
        } else if self.agents.iter().any(|a| a.is_active) {
            "RUNNING"
        } else {
            "IDLE"
        };

        StatusBarWidget {
            theme: &self.theme,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cost_usd: self.cost_usd,
            status_text,
        }
        .render(layout.status_bar_area, buf);
    }
}
