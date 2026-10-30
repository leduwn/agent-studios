//! Provider & Models Management TUI Panel Widget
//!
//! Provides an interactive split-pane interface for viewing configured
//! provider instances, their registered models, and inspecting model limits and capabilities.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Widget},
};

use agent_studios_provider::catalog::ProviderCatalog;
use agent_studios_provider::instance::ProviderInstance;
use agent_studios_provider::model::ModelDescriptor;

use crate::theme::AgsTheme;

/// Active pane focus within the provider panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderPane {
    Providers,
    Models,
    Inspector,
}

/// State machine for the provider & models management panel.
pub struct ProviderPanelState {
    pub catalog: ProviderCatalog,
    pub active_pane: ProviderPane,
    pub selected_provider_idx: usize,
    pub selected_model_idx: usize,
    pub status_message: Option<String>,
    pub show_help: bool,
}

/// Resolves the provider catalog file path from environment or default user directories.
pub fn catalog_path_from_environment() -> std::path::PathBuf {
    if let Ok(h) = std::env::var("AGENT_STUDIOS_HOME") {
        std::path::PathBuf::from(h.trim()).join("providers.json")
    } else if let Ok(u) = std::env::var("USERPROFILE") {
        std::path::PathBuf::from(u.trim())
            .join(".agent-studios")
            .join("providers.json")
    } else if let Ok(h) = std::env::var("HOME") {
        std::path::PathBuf::from(h.trim())
            .join(".agent-studios")
            .join("providers.json")
    } else {
        std::path::PathBuf::from(".agent-studios").join("providers.json")
    }
}

/// Loads the provider catalog from disk using `$AGENT_STUDIOS_HOME` or default user directory.
pub fn load_catalog_from_environment() -> ProviderCatalog {
    let path = catalog_path_from_environment();
    if let Some(cat) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .and_then(|snap| ProviderCatalog::from_snapshot(snap).ok())
    {
        return cat;
    }
    ProviderCatalog::with_builtin_definitions()
}

/// Atomically persists the provider catalog to disk using a sibling temporary file.
pub fn save_catalog_to_environment(
    catalog: &ProviderCatalog,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = catalog_path_from_environment();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let snapshot = catalog.export_snapshot();
    let json = serde_json::to_string_pretty(&snapshot)?;
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, json.as_bytes())?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

impl Default for ProviderPanelState {
    fn default() -> Self {
        Self::with_catalog(load_catalog_from_environment())
    }
}

impl ProviderPanelState {
    /// Creates a new provider panel state loaded from persistent configuration.
    pub fn load() -> Self {
        Self::default()
    }

    /// Creates a new provider panel state with the given catalog.
    pub fn with_catalog(catalog: ProviderCatalog) -> Self {
        Self {
            catalog,
            active_pane: ProviderPane::Providers,
            selected_provider_idx: 0,
            selected_model_idx: 0,
            status_message: None,
            show_help: false,
        }
    }

    /// Returns list of all provider instances in the catalog.
    pub fn instances(&self) -> Vec<&ProviderInstance> {
        self.catalog.list_provider_instances()
    }

    /// Returns the currently highlighted provider instance.
    pub fn selected_instance(&self) -> Option<&ProviderInstance> {
        let list = self.instances();
        if list.is_empty() {
            None
        } else {
            let idx = self.selected_provider_idx.min(list.len() - 1);
            Some(list[idx])
        }
    }

    /// Returns models registered under the currently highlighted provider instance.
    pub fn current_models(&self) -> Vec<&ModelDescriptor> {
        if let Some(inst) = self.selected_instance() {
            self.catalog.models_for_provider_instance(&inst.id)
        } else {
            Vec::new()
        }
    }

    /// Returns the currently highlighted model descriptor.
    pub fn selected_model(&self) -> Option<&ModelDescriptor> {
        let models = self.current_models();
        if models.is_empty() {
            None
        } else {
            let idx = self.selected_model_idx.min(models.len() - 1);
            Some(models[idx])
        }
    }

    /// Switches focus between the available panes.
    pub fn toggle_pane(&mut self) {
        self.active_pane = match self.active_pane {
            ProviderPane::Providers => ProviderPane::Models,
            ProviderPane::Models => ProviderPane::Inspector,
            ProviderPane::Inspector => ProviderPane::Providers,
        };
    }

    /// Navigates selection up in the currently active pane.
    pub fn select_prev(&mut self) {
        match self.active_pane {
            ProviderPane::Providers => {
                if self.selected_provider_idx > 0 {
                    self.selected_provider_idx -= 1;
                    self.selected_model_idx = 0;
                }
            }
            ProviderPane::Models => {
                if self.selected_model_idx > 0 {
                    self.selected_model_idx -= 1;
                }
            }
            ProviderPane::Inspector => {}
        }
    }

    /// Navigates selection down in the currently active pane.
    pub fn select_next(&mut self) {
        match self.active_pane {
            ProviderPane::Providers => {
                let count = self.instances().len();
                if count > 0 && self.selected_provider_idx + 1 < count {
                    self.selected_provider_idx += 1;
                    self.selected_model_idx = 0;
                }
            }
            ProviderPane::Models => {
                let count = self.current_models().len();
                if count > 0 && self.selected_model_idx + 1 < count {
                    self.selected_model_idx += 1;
                }
            }
            ProviderPane::Inspector => {}
        }
    }

    /// Sets the highlighted provider instance as default for its provider family,
    /// atomically persisting the updated catalog to disk.
    pub fn set_current_as_default(&mut self) -> Result<(), String> {
        let (p_id, i_id) = {
            let inst = self
                .selected_instance()
                .ok_or_else(|| "No provider instance selected".to_string())?;
            (inst.provider_id.clone(), inst.id)
        };

        self.catalog
            .set_default_instance_for_provider(&p_id, &i_id)
            .map_err(|e| e.to_string())?;

        if let Err(e) = save_catalog_to_environment(&self.catalog) {
            self.status_message = Some(format!("⚠ Marked default but failed to save: {e}"));
            return Err(e.to_string());
        }

        self.status_message = Some(format!("★ Marked {i_id} as default for '{p_id}' (saved)"));
        Ok(())
    }

    /// Handles keyboard events, returning `false` if user requested closing the panel.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.show_help {
            if key.code == KeyCode::Esc
                || key.code == KeyCode::Char('?')
                || key.code == KeyCode::Char('q')
            {
                self.show_help = false;
            }
            return true;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => false,
            KeyCode::Tab => {
                self.toggle_pane();
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_prev();
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                true
            }
            KeyCode::Char('d') => {
                let _ = self.set_current_as_default();
                true
            }
            KeyCode::Char('?') | KeyCode::Char('h') => {
                self.show_help = true;
                true
            }
            _ => true,
        }
    }
}

/// TUI widget for rendering the provider and models management panel.
pub struct ProviderPanelWidget<'a> {
    pub theme: &'a AgsTheme,
    pub state: &'a ProviderPanelState,
}

impl<'a> ProviderPanelWidget<'a> {
    pub fn new(theme: &'a AgsTheme, state: &'a ProviderPanelState) -> Self {
        Self { theme, state }
    }
}

impl<'a> Widget for ProviderPanelWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 20 || area.height < 6 {
            return;
        }

        // Clear background
        let bg_block = Block::default().style(Style::default().bg(self.theme.bg_base));
        bg_block.render(area, buf);

        // Outer container block
        let outer_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.border_focused))
            .title(Span::styled(
                " Provider & Models Management (AGS) ",
                Style::default()
                    .fg(self.theme.accent_emerald)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner_area = outer_block.inner(area);
        outer_block.render(area, buf);

        // Split vertically: Main Content (top) and Keybinding Footer (bottom)
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(4), Constraint::Length(1)])
            .split(inner_area);

        // Split main content horizontally: Left pane (Providers list) and Right pane (Models & Inspector)
        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(main_chunks[0]);

        // Render Left Pane: Providers List
        self.render_providers_pane(horizontal_chunks[0], buf);

        // Split Right Pane vertically: Top (Models List) and Bottom (Model Details Inspector)
        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
            .split(horizontal_chunks[1]);

        self.render_models_pane(right_chunks[0], buf);
        self.render_inspector_pane(right_chunks[1], buf);

        // Render Footer Bar
        self.render_footer(main_chunks[1], buf);

        // Render Help Modal if triggered
        if self.state.show_help {
            self.render_help_modal(area, buf);
        }
    }
}

impl<'a> ProviderPanelWidget<'a> {
    fn render_providers_pane(&self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.state.active_pane == ProviderPane::Providers;
        let border_fg = if is_focused {
            self.theme.accent_sky
        } else {
            self.theme.border_subtle
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_fg))
            .title(Span::styled(
                if is_focused {
                    " ▶ Provider Instances "
                } else {
                    " Provider Instances "
                },
                Style::default()
                    .fg(if is_focused {
                        self.theme.accent_sky
                    } else {
                        self.theme.text_secondary
                    })
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        block.render(area, buf);

        let instances = self.state.instances();
        if instances.is_empty() {
            let empty_text = Paragraph::new(
                "No provider instances configured.\nRun 'ags provider add' to register one.",
            )
            .style(Style::default().fg(self.theme.text_muted));
            empty_text.render(inner, buf);
            return;
        }

        let items: Vec<ListItem> = instances
            .iter()
            .enumerate()
            .map(|(idx, inst)| {
                let is_selected = idx == self.state.selected_provider_idx;
                let is_default = self
                    .state
                    .catalog
                    .get_default_instance_id_for_provider(&inst.provider_id)
                    .map(|def_id| def_id == &inst.id)
                    .unwrap_or(false);

                let default_tag = if is_default { " ★" } else { "  " };
                let model_count = self
                    .state
                    .catalog
                    .models_for_provider_instance(&inst.id)
                    .len();

                let style = if is_selected && is_focused {
                    Style::default()
                        .fg(self.theme.text_primary)
                        .bg(self.theme.bg_highlight)
                        .add_modifier(Modifier::BOLD)
                } else if is_selected {
                    Style::default()
                        .fg(self.theme.text_primary)
                        .bg(self.theme.bg_card)
                } else {
                    Style::default().fg(self.theme.text_secondary)
                };

                let line = Line::from(vec![
                    Span::styled(default_tag, Style::default().fg(self.theme.accent_amber)),
                    Span::raw(" "),
                    Span::styled(&inst.display_name, style),
                    Span::styled(
                        format!(" [{}]", inst.provider_id),
                        Style::default().fg(self.theme.text_muted),
                    ),
                    Span::styled(
                        format!(" ({} models)", model_count),
                        Style::default().fg(self.theme.accent_sky),
                    ),
                ]);

                ListItem::new(line).style(style)
            })
            .collect();

        let list = List::new(items);
        list.render(inner, buf);
    }

    fn render_models_pane(&self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.state.active_pane == ProviderPane::Models;
        let border_fg = if is_focused {
            self.theme.accent_sky
        } else {
            self.theme.border_subtle
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_fg))
            .title(Span::styled(
                if is_focused {
                    " ▶ Registered Models "
                } else {
                    " Registered Models "
                },
                Style::default()
                    .fg(if is_focused {
                        self.theme.accent_sky
                    } else {
                        self.theme.text_secondary
                    })
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        block.render(area, buf);

        let models = self.state.current_models();
        if models.is_empty() {
            let empty_text = Paragraph::new("No models registered for this instance.\nUse 'ags model add' or dynamic discovery.")
                .style(Style::default().fg(self.theme.text_muted));
            empty_text.render(inner, buf);
            return;
        }

        let items: Vec<ListItem> = models
            .iter()
            .enumerate()
            .map(|(idx, m)| {
                let is_selected = idx == self.state.selected_model_idx;
                let style = if is_selected && is_focused {
                    Style::default()
                        .fg(self.theme.text_primary)
                        .bg(self.theme.bg_highlight)
                        .add_modifier(Modifier::BOLD)
                } else if is_selected {
                    Style::default()
                        .fg(self.theme.text_primary)
                        .bg(self.theme.bg_card)
                } else {
                    Style::default().fg(self.theme.text_secondary)
                };

                let ctx_tag = m
                    .limits
                    .context_window_tokens
                    .map(|t| format!("{}k ctx", t / 1000))
                    .unwrap_or_else(|| "-".to_string());

                let mut caps = Vec::new();
                if m.capabilities.tool_calling.is_supported() {
                    caps.push("tools");
                }
                if m.capabilities.vision_input.is_supported() {
                    caps.push("vision");
                }
                if m.capabilities.streaming.is_supported() {
                    caps.push("stream");
                }
                let caps_str = if caps.is_empty() {
                    "".to_string()
                } else {
                    format!(" [{}]", caps.join(","))
                };

                let line = Line::from(vec![
                    Span::styled(&m.display_name, style),
                    Span::styled(
                        format!(" ({})", m.id),
                        Style::default().fg(self.theme.text_muted),
                    ),
                    Span::styled(
                        format!(" - {ctx_tag}"),
                        Style::default().fg(self.theme.accent_emerald),
                    ),
                    Span::styled(caps_str, Style::default().fg(self.theme.accent_purple)),
                ]);

                ListItem::new(line).style(style)
            })
            .collect();

        let list = List::new(items);
        list.render(inner, buf);
    }

    fn render_inspector_pane(&self, area: Rect, buf: &mut Buffer) {
        let is_focused = self.state.active_pane == ProviderPane::Inspector;
        let border_fg = if is_focused {
            self.theme.accent_sky
        } else {
            self.theme.border_subtle
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_fg))
            .title(Span::styled(
                if is_focused {
                    " ▶ Model Metadata Inspector "
                } else {
                    " Model Metadata Inspector "
                },
                Style::default()
                    .fg(if is_focused {
                        self.theme.accent_sky
                    } else {
                        self.theme.text_secondary
                    })
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        block.render(area, buf);

        if let Some(m) = self.state.selected_model() {
            let mut lines = Vec::new();

            lines.push(Line::from(vec![
                Span::styled(
                    "Model ID:       ",
                    Style::default().fg(self.theme.text_muted),
                ),
                Span::styled(
                    m.id.as_str(),
                    Style::default()
                        .fg(self.theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));

            lines.push(Line::from(vec![
                Span::styled(
                    "Display Name:   ",
                    Style::default().fg(self.theme.text_muted),
                ),
                Span::styled(
                    &m.display_name,
                    Style::default().fg(self.theme.text_primary),
                ),
            ]));

            lines.push(Line::from(vec![
                Span::styled(
                    "Source:         ",
                    Style::default().fg(self.theme.text_muted),
                ),
                Span::styled(
                    m.metadata_source.to_string(),
                    Style::default().fg(self.theme.accent_sky),
                ),
            ]));

            let ctx_str = m
                .limits
                .context_window_tokens
                .map(|t| format!("{t} tokens"))
                .unwrap_or_else(|| "Unknown".to_string());
            let out_str = m
                .limits
                .max_output_tokens
                .map(|t| format!("{t} tokens"))
                .unwrap_or_else(|| "Unknown".to_string());

            lines.push(Line::from(vec![
                Span::styled(
                    "Context Window: ",
                    Style::default().fg(self.theme.text_muted),
                ),
                Span::styled(ctx_str, Style::default().fg(self.theme.accent_emerald)),
                Span::styled(
                    " | Max Output: ",
                    Style::default().fg(self.theme.text_muted),
                ),
                Span::styled(out_str, Style::default().fg(self.theme.accent_emerald)),
            ]));

            lines.push(Line::from(vec![
                Span::styled(
                    "Capabilities:   ",
                    Style::default().fg(self.theme.text_muted),
                ),
                Span::styled(
                    format!(
                        "tools={:?}, vision={:?}, stream={:?}, reasoning={:?}",
                        m.capabilities.tool_calling,
                        m.capabilities.vision_input,
                        m.capabilities.streaming,
                        m.capabilities.reasoning,
                    ),
                    Style::default().fg(self.theme.accent_purple),
                ),
            ]));

            let paragraph = Paragraph::new(lines);
            paragraph.render(inner, buf);
        } else if let Some(inst) = self.state.selected_instance() {
            let lines = vec![
                Line::from(vec![
                    Span::styled(
                        "Provider ID:    ",
                        Style::default().fg(self.theme.text_muted),
                    ),
                    Span::styled(
                        inst.id.to_string(),
                        Style::default().fg(self.theme.text_primary),
                    ),
                ]),
                Line::from(vec![
                    Span::styled(
                        "Family:         ",
                        Style::default().fg(self.theme.text_muted),
                    ),
                    Span::styled(
                        inst.provider_id.as_str(),
                        Style::default().fg(self.theme.accent_sky),
                    ),
                    Span::styled(" | Protocol: ", Style::default().fg(self.theme.text_muted)),
                    Span::styled(
                        inst.protocol.to_string(),
                        Style::default().fg(self.theme.accent_emerald),
                    ),
                ]),
                Line::from(vec![
                    Span::styled(
                        "Endpoint:       ",
                        Style::default().fg(self.theme.text_muted),
                    ),
                    Span::styled(
                        &inst.endpoint.base_url,
                        Style::default().fg(self.theme.text_primary),
                    ),
                ]),
            ];
            let paragraph = Paragraph::new(lines);
            paragraph.render(inner, buf);
        } else {
            let p = Paragraph::new("Select a provider or model to inspect detailed parameters.")
                .style(Style::default().fg(self.theme.text_muted));
            p.render(inner, buf);
        }
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        let status = self.state.status_message.as_deref().unwrap_or(
            "[Tab] Switch Pane  [j/k] Navigate  [d] Set Default  [?] Help  [Esc/q] Close",
        );

        let footer_line = Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(status, Style::default().fg(self.theme.text_muted)),
        ]);
        let p = Paragraph::new(footer_line).style(Style::default().bg(self.theme.bg_card));
        p.render(area, buf);
    }

    fn render_help_modal(&self, area: Rect, buf: &mut Buffer) {
        let width = 54.min(area.width.saturating_sub(4));
        let height = 12.min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let modal_area = Rect::new(x, y, width, height);

        Clear.render(modal_area, buf);

        let help_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent_emerald))
            .title(Span::styled(
                " Provider Management Help ",
                Style::default()
                    .fg(self.theme.accent_emerald)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = help_block.inner(modal_area);
        help_block.render(modal_area, buf);

        let help_text = vec![
            Line::from(Span::styled(
                "Keyboard Navigation Controls:",
                Style::default().add_modifier(Modifier::UNDERLINED),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Tab        ", Style::default().fg(self.theme.accent_sky)),
                Span::raw("Cycle active pane (Providers -> Models -> Inspector)"),
            ]),
            Line::from(vec![
                Span::styled("  j / Down   ", Style::default().fg(self.theme.accent_sky)),
                Span::raw("Move cursor down in active pane"),
            ]),
            Line::from(vec![
                Span::styled("  k / Up     ", Style::default().fg(self.theme.accent_sky)),
                Span::raw("Move cursor up in active pane"),
            ]),
            Line::from(vec![
                Span::styled("  d          ", Style::default().fg(self.theme.accent_sky)),
                Span::raw("Set current instance as default for family"),
            ]),
            Line::from(vec![
                Span::styled("  Esc / q    ", Style::default().fg(self.theme.accent_sky)),
                Span::raw("Close provider management panel"),
            ]),
            Line::from(vec![
                Span::styled("  ? / h      ", Style::default().fg(self.theme.accent_sky)),
                Span::raw("Toggle this help popup"),
            ]),
        ];

        let p = Paragraph::new(help_text);
        p.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_studios_provider::auth::AuthenticationScheme;
    use agent_studios_provider::capabilities::ModelCapabilities;
    use agent_studios_provider::endpoint::EndpointProfile;
    use agent_studios_provider::id::{ModelId, ProviderId, ProviderInstanceId};
    use agent_studios_provider::instance::ProviderInstance;
    use agent_studios_provider::model::{ModelDescriptor, ModelLimits};
    use agent_studios_provider::protocol::ProtocolFamily;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use ratatui::buffer::Buffer;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn sample_catalog() -> ProviderCatalog {
        let mut cat = ProviderCatalog::with_builtin_definitions();

        let inst1_id = ProviderInstanceId::new();
        let inst1 = ProviderInstance::new(
            inst1_id,
            ProviderId::new("openai").unwrap(),
            "OpenAI Main",
            ProtocolFamily::OpenAiResponses,
            EndpointProfile::new("https://api.openai.com/v1").unwrap(),
            AuthenticationScheme::None,
        )
        .unwrap();

        let inst2_id = ProviderInstanceId::new();
        let inst2 = ProviderInstance::new(
            inst2_id,
            ProviderId::new("anthropic").unwrap(),
            "Anthropic Dev",
            ProtocolFamily::AnthropicMessages,
            EndpointProfile::new("https://api.anthropic.com").unwrap(),
            AuthenticationScheme::None,
        )
        .unwrap();

        cat.register_provider_instance(inst1).unwrap();
        cat.register_provider_instance(inst2).unwrap();

        let model = ModelDescriptor::new(
            inst1_id,
            ModelId::new("gpt-4o").unwrap(),
            "GPT-4o Omni",
            ModelCapabilities::default(),
            ModelLimits::new(Some(128_000), Some(4096)).unwrap(),
        )
        .unwrap();
        cat.register_model(model).unwrap();

        cat
    }

    #[test]
    fn test_provider_panel_navigation_and_pane_switching() {
        let cat = sample_catalog();
        let mut state = ProviderPanelState::with_catalog(cat);

        assert_eq!(state.active_pane, ProviderPane::Providers);
        assert_eq!(state.instances().len(), 2);
        assert_eq!(state.selected_provider_idx, 0);

        // Tab -> Models
        assert!(state.handle_key(key(KeyCode::Tab)));
        assert_eq!(state.active_pane, ProviderPane::Models);

        // Tab -> Inspector
        assert!(state.handle_key(key(KeyCode::Tab)));
        assert_eq!(state.active_pane, ProviderPane::Inspector);

        // Tab -> Providers
        assert!(state.handle_key(key(KeyCode::Tab)));
        assert_eq!(state.active_pane, ProviderPane::Providers);

        // Navigate down
        assert!(state.handle_key(key(KeyCode::Char('j'))));
        assert_eq!(state.selected_provider_idx, 1);

        // Navigate up
        assert!(state.handle_key(key(KeyCode::Char('k'))));
        assert_eq!(state.selected_provider_idx, 0);
    }

    #[test]
    fn test_provider_panel_set_default_instance() {
        let cat = sample_catalog();
        let mut state = ProviderPanelState::with_catalog(cat);

        assert!(state.handle_key(key(KeyCode::Char('d'))));
        assert!(state.status_message.is_some());
        assert!(state.status_message.as_ref().unwrap().contains("Marked"));
    }

    #[test]
    fn test_provider_panel_help_and_exit() {
        let cat = sample_catalog();
        let mut state = ProviderPanelState::with_catalog(cat);

        // Open help
        assert!(state.handle_key(key(KeyCode::Char('?'))));
        assert!(state.show_help);

        // Close help via Esc
        assert!(state.handle_key(key(KeyCode::Esc)));
        assert!(!state.show_help);

        // Close panel via Esc
        assert!(!state.handle_key(key(KeyCode::Esc)));
    }

    #[test]
    fn test_provider_panel_render_buffer() {
        let cat = sample_catalog();
        let state = ProviderPanelState::with_catalog(cat);
        let theme = AgsTheme::dark();
        let widget = ProviderPanelWidget::new(&theme, &state);

        let area = Rect::new(0, 0, 100, 30);
        let mut buf = Buffer::empty(area);

        widget.render(area, &mut buf);

        // Buffer contains title
        let mut found_title = false;
        for y in 0..area.height {
            let row_str: String = (0..area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect();
            if row_str.contains("Provider & Models Management") {
                found_title = true;
                break;
            }
        }
        assert!(found_title, "Rendered buffer must contain panel title");
    }
}
