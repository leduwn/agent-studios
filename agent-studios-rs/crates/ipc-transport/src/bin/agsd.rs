use clap::Parser;
use std::path::PathBuf;

use agent_studios_ipc_transport::daemon::{DaemonConfig, run_daemon};

#[derive(Parser, Debug)]
#[command(name = "agsd", about = "Agent Studios Native IPC Transport Daemon")]
struct Cli {
    #[arg(long, default_value = r"\\.\pipe\agent-studios-default")]
    pipe_name: String,

    #[arg(long)]
    socket_path: Option<PathBuf>,

    #[arg(long)]
    auth_key: Option<String>,

    #[arg(long, default_value = "ags-client")]
    key_id: String,

    #[arg(long, default_value_t = 1000)]
    capacity: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let auth_key = cli.auth_key.unwrap_or_else(|| {
        let generated = uuid::Uuid::new_v4().to_string();
        eprintln!("Generated ephemeral auth key: {}", generated);
        generated
    });

    let socket_path = cli.socket_path.unwrap_or_else(|| {
        std::env::var("XDG_RUNTIME_DIR")
            .map(|xdg| PathBuf::from(xdg).join("agent-studios").join("agsd.sock"))
            .unwrap_or_else(|_| PathBuf::from("/tmp/agent-studios-agsd.sock"))
    });

    let config = DaemonConfig {
        pipe_name: cli.pipe_name,
        socket_path,
        auth_key: auth_key.into_bytes(),
        key_id: cli.key_id,
        capacity: cli.capacity,
    };

    run_daemon(config).await?;
    Ok(())
}
