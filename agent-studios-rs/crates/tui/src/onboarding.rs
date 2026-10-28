//! Interactive Onboarding Application (`ags` graceful first-run experience)
//!
//! Provides an interactive welcome screen when no AI runtime is configured.
//! Allows the developer to switch to Demo mode, launch Design Preview, run Doctor diagnostics,
//! toggle themes, or exit cleanly.

use agent_studios_external_runtime::DiscoveredRuntimeInstance;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use std::time::Duration;

use crate::app::{detect_workspace_info, detect_workspace_path};
use crate::terminal::TerminalGuard;
use crate::theme::{AgsTheme, ThemeMode};
use crate::widgets::onboarding::OnboardingWidget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingAction {
    LaunchDemo,
    LaunchPreview,
    RunDoctor,
    Exit,
}

pub struct OnboardingApp {
    pub theme: AgsTheme,
    pub project_name: String,
    pub branch_name: String,
    pub is_dirty: bool,
    pub workspace_path: String,
    pub detected_runtimes: Vec<DiscoveredRuntimeInstance>,
    pub error_message: Option<String>,
}

impl OnboardingApp {
    pub fn new(
        detected_runtimes: Vec<DiscoveredRuntimeInstance>,
        error_message: Option<String>,
    ) -> Self {
        let (project_name, branch_name, is_dirty) = detect_workspace_info();
        let workspace_path = detect_workspace_path();

        Self {
            theme: AgsTheme::dark(),
            project_name,
            branch_name,
            is_dirty,
            workspace_path,
            detected_runtimes,
            error_message,
        }
    }

    pub fn run(&mut self) -> Result<OnboardingAction, Box<dyn std::error::Error>> {
        let (mut terminal, _guard) = TerminalGuard::enter()?;
        let _ = crate::terminal::set_terminal_title("AGS — Agent Studios Onboarding");

        loop {
            terminal.draw(|f| self.render(f.area(), f.buffer_mut()))?;

            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char('c')
                        {
                            return Ok(OnboardingAction::Exit);
                        }

                        match key.code {
                            KeyCode::Char('d') | KeyCode::Char('D') => {
                                return Ok(OnboardingAction::LaunchDemo);
                            }
                            KeyCode::Char('p') | KeyCode::Char('P') => {
                                return Ok(OnboardingAction::LaunchPreview);
                            }
                            KeyCode::Char('r') | KeyCode::Char('R') => {
                                return Ok(OnboardingAction::RunDoctor);
                            }
                            KeyCode::Char('t') | KeyCode::Char('T') => {
                                self.theme = if self.theme.mode == ThemeMode::Dark {
                                    AgsTheme::light()
                                } else {
                                    AgsTheme::dark()
                                };
                            }
                            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                                return Ok(OnboardingAction::Exit);
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn render(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        OnboardingWidget {
            theme: &self.theme,
            project_name: &self.project_name,
            branch_name: &self.branch_name,
            is_dirty: self.is_dirty,
            workspace_path: Some(&self.workspace_path),
            detected_runtimes: &self.detected_runtimes,
            error_message: self.error_message.as_deref(),
        }
        .render(area, buf);
    }
}
