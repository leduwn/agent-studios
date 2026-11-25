//! Agent Studios CLI (`ags`) Entrypoint
//!
//! Provides the developer-first command-line interface and interactive TUI
//! for multi-agent autonomous engineering.

use clap::{Parser, Subcommand};
use std::sync::Arc;

mod commands;
mod config;
mod doctor;
use doctor::AgsDoctor;

use agent_studios_cli_client::embedded::EmbeddedClient;
use agent_studios_tui::{AgsApp, OfflinePreviewApp};

#[derive(Parser, Debug)]
#[command(
    name = "ags",
    version,
    about = "Agent Studios Developer CLI & Autonomous Multi-Agent TUI",
    long_about = "High-polish, developer-first terminal interface for orchestrating autonomous multi-agent teams."
)]
struct Cli {
    /// Launch offline interactive design preview (themes, modals, diffs)
    #[arg(long, help = "Run offline interactive visual design previewer")]
    preview: bool,

    /// Run in deterministic simulated demo mode (no remote model credentials required)
    #[arg(
        long,
        help = "Run in simulated demo mode without remote model credentials"
    )]
    demo: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run system and environment diagnostics
    Doctor {
        /// Format output as JSON for tooling and CI
        #[arg(long)]
        json: bool,
    },

    /// Manage model provider families, instances, endpoints, and credentials
    Provider {
        #[command(subcommand)]
        command: commands::provider::ProviderCommands,
    },

    /// Manage and inspect registered model catalog
    Model {
        #[command(subcommand)]
        command: commands::model::ModelCommands,
    },

    /// Execute an engineering goal via multi-agent orchestration (supports --headless and --stdin)
    Run(commands::run::RunArgs),

    /// Inspect or query the local agsd background daemon
    Daemon {
        #[command(subcommand)]
        command: commands::daemon::DaemonCommands,
    },

    /// Manage M16 SKILL.md packages across Global, Project, and Agent scopes
    Skill {
        #[command(subcommand)]
        command: commands::extensibility::SkillCommands,
    },

    /// Inspect configured MCP servers with redacted credentials
    Mcp {
        #[command(subcommand)]
        command: commands::extensibility::McpCommands,
    },

    /// Inspect configured lifecycle hooks (pre_turn, post_task, on_reconciliation, on_approval)
    Hooks {
        #[command(subcommand)]
        command: commands::extensibility::HooksCommands,
    },

    /// Resolve hierarchical AGENTS.md and CLAUDE.md instructions inside worktree bounds
    Instructions(commands::extensibility::InstructionsArgs),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Route 1: Offline Design Previewer
    if cli.preview {
        let mut preview_app = OfflinePreviewApp::new();
        preview_app.run()?;
        return Ok(());
    }

    // Route 2: Subcommands
    if let Some(cmd) = cli.command {
        match cmd {
            Commands::Doctor { json } => {
                let report = AgsDoctor::run_diagnostics();
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    AgsDoctor::print_human_report(&report);
                }
                return Ok(());
            }
            Commands::Provider { command } => {
                commands::provider::handle_provider_command(command).await?;
                return Ok(());
            }
            Commands::Model { command } => {
                commands::model::handle_model_command(command).await?;
                return Ok(());
            }
            Commands::Run(args) => {
                let code = commands::run::execute_run_command(args, cli.demo).await;
                if code != 0 {
                    std::process::exit(code);
                }
                return Ok(());
            }
            Commands::Daemon { command } => {
                commands::daemon::handle_daemon_command(command).await?;
                return Ok(());
            }
            Commands::Skill { command } => {
                commands::extensibility::handle_skill_command(command)?;
                return Ok(());
            }
            Commands::Mcp { command } => {
                commands::extensibility::handle_mcp_command(command)?;
                return Ok(());
            }
            Commands::Hooks { command } => {
                commands::extensibility::handle_hooks_command(command)?;
                return Ok(());
            }
            Commands::Instructions(args) => {
                commands::extensibility::handle_instructions_command(args)?;
                return Ok(());
            }
        }
    }

    // Route 3: Interactive TUI
    let runtime_mode = if cli.demo {
        agent_studios_cli_client::client::RuntimeMode::Demo
    } else {
        agent_studios_cli_client::client::RuntimeMode::Production
    };

    let embedded_client = match EmbeddedClient::bootstrap_with_mode(
        "agent-studios-workspace",
        runtime_mode,
    )
    .await
    {
        Ok(client) => Some(client),
        Err(agent_studios_cli_client::client::ClientError::Configuration(msg)) => {
            let detected = agent_studios_cli_client::runtime::discover_available_runtimes().await;
            let mut onboarding = agent_studios_tui::OnboardingApp::new(detected, Some(msg));
            match onboarding.run() {
                Ok(agent_studios_tui::OnboardingAction::LaunchDemo) => {
                    let client = EmbeddedClient::bootstrap_with_mode(
                        "agent-studios-workspace",
                        agent_studios_cli_client::client::RuntimeMode::Demo,
                    )
                    .await?;
                    Some(client)
                }
                Ok(agent_studios_tui::OnboardingAction::LaunchPreview) => {
                    let mut preview_app = OfflinePreviewApp::new();
                    preview_app.run()?;
                    return Ok(());
                }
                Ok(agent_studios_tui::OnboardingAction::RunDoctor) => {
                    let report = AgsDoctor::run_diagnostics();
                    AgsDoctor::print_human_report(&report);
                    return Ok(());
                }
                Ok(agent_studios_tui::OnboardingAction::Exit) => {
                    return Ok(());
                }
                Err(_) => {
                    // Non-interactive or headless environment: display friendly guidance
                    eprintln!("\nAgent Studios — Runtime Configuration Notice:");
                    eprintln!("  No AI runtime configured or discovered.");
                    eprintln!(
                        "  To run in simulated execution without live AI credentials: `ags --demo`"
                    );
                    eprintln!("  To preview UI themes and modal components: `ags --preview`");
                    eprintln!("  To inspect environment health: `ags doctor`\n");
                    return Ok(());
                }
            }
        }
        Err(e) => return Err(e.into()),
    };

    if let Some(client) = embedded_client {
        let client_arc = Arc::new(client);
        let mut app = AgsApp::new(client_arc);
        app.run().await?;
    }

    Ok(())
}
