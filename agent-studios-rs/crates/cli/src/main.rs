//! Agent Studios CLI (`ags`) Entrypoint
//!
//! Provides the developer-first command-line interface and interactive TUI
//! for multi-agent autonomous engineering.

use clap::{Parser, Subcommand};
use std::sync::Arc;

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
    #[arg(long, help = "Run in simulated demo mode without remote model credentials")]
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
        Ok(client) => client,
        Err(agent_studios_cli_client::client::ClientError::Configuration(msg)) => {
            eprintln!("\nError: {msg}\n");
            eprintln!("To run simulated execution without live AI credentials, use: `ags --demo`");
            eprintln!("To preview UI themes and modal components, use: `ags --preview`\n");
            std::process::exit(1);
        }
        Err(e) => return Err(e.into()),
    };

    let client_arc = Arc::new(embedded_client);

    let mut app = AgsApp::new(client_arc);
    app.run().await?;

    Ok(())
}
