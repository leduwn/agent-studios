use clap::Parser;
use std::path::PathBuf;

use agent_studios_ipc_transport::daemon::{DaemonConfig, run_daemon};

#[derive(Parser, Debug)]
#[command(name = "agsd", about = "Agent Studios Native IPC Transport Daemon")]
struct Cli {
    #[arg(
        long,
        default_value = r"\\.\pipe\agent-studios-default",
        alias = "pipe_name"
    )]
    pipe_name: String,

    #[arg(long, alias = "socket_path")]
    socket_path: Option<PathBuf>,

    #[arg(long, alias = "runtime_info_path")]
    runtime_info_path: Option<PathBuf>,

    #[arg(long, alias = "admin_runtime_info_path")]
    admin_runtime_info_path: Option<PathBuf>,

    #[arg(long, default_value = "ags-client", alias = "key_id")]
    key_id: String,

    #[arg(long, default_value_t = 1000)]
    capacity: usize,

    #[arg(long, default_value_t = true, alias = "auto_execute")]
    auto_execute: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let socket_path = cli.socket_path.unwrap_or_else(|| {
        std::env::var("XDG_RUNTIME_DIR")
            .map(|xdg| PathBuf::from(xdg).join("agent-studios").join("agsd.sock"))
            .unwrap_or_else(|_| PathBuf::from("/tmp/agent-studios-agsd.sock"))
    });

    let config = DaemonConfig {
        pipe_name: cli.pipe_name,
        socket_path,
        runtime_info_path: cli.runtime_info_path,
        admin_runtime_info_path: cli.admin_runtime_info_path,
        key_id: cli.key_id,
        capacity: cli.capacity,
        auto_execute: cli.auto_execute,
    };

    run_daemon(config).await?;
    Ok(())
}
