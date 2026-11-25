//! Agent Studios Standalone Desktop Executable (`AgentStudios.exe`)

use std::path::PathBuf;

use clap::Parser;
use serde::Serialize;

use agent_studios_desktop::{
    CanonicalDesktopPaths, DesktopDaemonSupervisor, DesktopMode, DesktopSupervisorConfig,
    build_workbench_launch_args, ensure_extension_installed_in_isolated_profile, find_bundled_vsix,
    resolve_workbench_binary,
};

#[derive(Parser, Debug)]
#[command(
    name = "AgentStudios",
    version,
    about = "Agent Studios Standalone Desktop Workbench & Daemon Supervisor"
)]
struct DesktopCli {
    /// Initial workbench mode (`agent` or `ide`)
    #[arg(long, default_value = "agent", value_enum)]
    mode: DesktopMode,

    /// Workspace directory to open on startup
    #[arg(long)]
    workspace: Option<PathBuf>,

    /// Provision canonical paths, probe binaries & daemon connectivity, and exit cleanly
    #[arg(long)]
    headless_check: bool,

    /// Start/verify embedded agsd daemon without launching GUI window
    #[arg(long)]
    ensure_daemon: bool,

    /// Query daemon and desktop host readiness status
    #[arg(long)]
    status: bool,

    /// Output status or headless check as JSON
    #[arg(long)]
    json: bool,

    /// Shutdown supervised/default agsd daemon using admin bootstrap credentials
    #[arg(long)]
    shutdown_daemon: bool,

    /// Override UI display language (en-US or vi-VN)
    #[arg(long, default_value = "en-US")]
    display_language: String,
}

#[derive(Serialize)]
struct HeadlessCheckReport {
    product: &'static str,
    version: &'static str,
    mode: &'static str,
    config_toml: String,
    state_db: String,
    artifacts_dir: String,
    profile_dir: String,
    extensions_dir: String,
    agsd_binary: Option<String>,
    workbench_binary: Option<String>,
    bundled_vsix: Option<String>,
    daemon_running: bool,
    daemon_pid: Option<u32>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = DesktopCli::parse();
    let paths = CanonicalDesktopPaths::resolve_default();
    paths.ensure_provisioned(&cli.display_language, "auto")?;

    let sup_cfg = DesktopSupervisorConfig::from_paths(paths.clone());
    let mut supervisor = DesktopDaemonSupervisor::new(sup_cfg);

    if cli.shutdown_daemon {
        let stopped = supervisor.shutdown_daemon().await?;
        if cli.json {
            println!("{}", serde_json::json!({ "shutdown": stopped }));
        } else {
            println!("Daemon shutdown executed: {stopped}");
        }
        return Ok(());
    }

    if cli.status {
        let status = supervisor.probe_status().await;
        if cli.json {
            println!("{}", serde_json::to_string_pretty(&status)?);
        } else {
            println!(
                "Agent Studios Desktop Host — Daemon {}",
                if status.running { "ONLINE" } else { "OFFLINE" }
            );
            println!("  Bootstrap: {}", status.bootstrap_path.display());
            if let Some(pid) = status.pid {
                println!("  PID:       {pid}");
            }
        }
        return Ok(());
    }

    if cli.headless_check {
        let agsd_binary = supervisor
            .resolve_agsd_binary()
            .ok()
            .map(|p| p.to_string_lossy().into_owned());
        let workbench_binary =
            resolve_workbench_binary(None).map(|p| p.to_string_lossy().into_owned());
        let bundled_vsix = find_bundled_vsix(None).map(|p| p.to_string_lossy().into_owned());
        let status = supervisor.probe_status().await;

        let report = HeadlessCheckReport {
            product: "Agent Studios",
            version: env!("CARGO_PKG_VERSION"),
            mode: cli.mode.as_str(),
            config_toml: paths.config_toml.to_string_lossy().into_owned(),
            state_db: paths.state_db.to_string_lossy().into_owned(),
            artifacts_dir: paths.artifacts_dir.to_string_lossy().into_owned(),
            profile_dir: paths.profile_user_data_dir.to_string_lossy().into_owned(),
            extensions_dir: paths.extensions_dir.to_string_lossy().into_owned(),
            agsd_binary,
            workbench_binary,
            bundled_vsix,
            daemon_running: status.running,
            daemon_pid: status.pid,
        };

        if cli.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!(
                "AgentStudios v{} headless check OK (config: {})",
                report.version, report.config_toml
            );
            println!(
                "  agsd:      {}",
                report.agsd_binary.as_deref().unwrap_or("not located")
            );
            println!(
                "  workbench: {}",
                report.workbench_binary.as_deref().unwrap_or("not located")
            );
        }
        return Ok(());
    }

    let daemon_status = supervisor.ensure_running().await?;
    if cli.ensure_daemon {
        if cli.json {
            println!("{}", serde_json::to_string_pretty(&daemon_status)?);
        } else {
            println!(
                "agsd daemon online (PID: {:?}, endpoint: {:?})",
                daemon_status.pid, daemon_status.endpoint
            );
        }
        return Ok(());
    }

    let workbench = match resolve_workbench_binary(None) {
        Some(bin) => bin,
        None => {
            eprintln!(
                "Code-OSS / VS Code desktop binary not found. Set AGENT_STUDIOS_CODE_BIN or use `ags` CLI/TUI."
            );
            std::process::exit(3);
        }
    };

    if let Some(vsix) = find_bundled_vsix(None) {
        let _ = ensure_extension_installed_in_isolated_profile(&workbench, &paths, &vsix);
    }

    let (args, env_vars) = build_workbench_launch_args(&paths, cli.workspace.as_deref(), cli.mode);
    let mut cmd = std::process::Command::new(&workbench);
    cmd.args(&args);
    for (k, v) in env_vars {
        cmd.env(k, v);
    }

    let status = cmd.status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}
