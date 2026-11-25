//! Daemon Lifecycle & Health Inspection Commands (`ags daemon ...`)

use clap::{Args, Subcommand};
use serde::Serialize;

use agent_studios_ipc_transport::IpcClient;
use agent_studios_ipc_transport::daemon::DaemonBootstrapInfo;

#[derive(Subcommand, Debug)]
pub enum DaemonCommands {
    /// Inspect agsd background daemon status and IPC bootstrap socket/pipe
    Status(DaemonOutputArgs),

    /// Query health and recovery telemetry from the running agsd daemon
    Health(DaemonOutputArgs),
}

#[derive(Args, Debug)]
pub struct DaemonOutputArgs {
    /// Output response in structured JSON format
    #[arg(long)]
    pub json: bool,
}

#[derive(Serialize)]
struct DaemonStatusSummary {
    online: bool,
    bootstrap_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    health: Option<agent_studios_ipc_transport::HealthResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub async fn handle_daemon_command(cmd: DaemonCommands) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        DaemonCommands::Status(args) => handle_status(args).await,
        DaemonCommands::Health(args) => handle_health(args).await,
    }
}

async fn probe_daemon() -> DaemonStatusSummary {
    let bootstrap_path = DaemonBootstrapInfo::default_runtime_path();
    let bootstrap_str = bootstrap_path.to_string_lossy().to_string();

    if !bootstrap_path.exists() {
        return DaemonStatusSummary {
            online: false,
            bootstrap_path: bootstrap_str,
            endpoint: None,
            pid: None,
            health: None,
            error: Some("Daemon bootstrap file not found (agsd offline)".to_string()),
        };
    }

    let info = match DaemonBootstrapInfo::load_from_file(&bootstrap_path) {
        Ok(i) => i,
        Err(e) => {
            return DaemonStatusSummary {
                online: false,
                bootstrap_path: bootstrap_str,
                endpoint: None,
                pid: None,
                health: None,
                error: Some(format!("Failed to read daemon bootstrap file: {e}")),
            };
        }
    };

    match IpcClient::connect_from_bootstrap_file(&bootstrap_path).await {
        Ok(mut client) => match client.get_health().await {
            Ok(health) => DaemonStatusSummary {
                online: true,
                bootstrap_path: bootstrap_str,
                endpoint: Some(info.endpoint),
                pid: Some(info.pid),
                health: Some(health),
                error: None,
            },
            Err(e) => DaemonStatusSummary {
                online: false,
                bootstrap_path: bootstrap_str,
                endpoint: Some(info.endpoint),
                pid: Some(info.pid),
                health: None,
                error: Some(format!("Connected but health probe failed: {e}")),
            },
        },
        Err(e) => DaemonStatusSummary {
            online: false,
            bootstrap_path: bootstrap_str,
            endpoint: Some(info.endpoint),
            pid: Some(info.pid),
            health: None,
            error: Some(format!(
                "IPC connection failed (stale bootstrap or offline): {e}"
            )),
        },
    }
}

async fn handle_status(args: DaemonOutputArgs) -> Result<(), Box<dyn std::error::Error>> {
    let summary = probe_daemon().await;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    println!("Agent Studios Daemon (`agsd`) Status");
    println!("{:-<56}", "");
    println!(
        "  Status:         {}",
        if summary.online { "ONLINE" } else { "OFFLINE" }
    );
    println!("  Bootstrap Path: {}", summary.bootstrap_path);
    if let Some(ep) = summary.endpoint {
        println!("  Endpoint:       {ep}");
    }
    if let Some(pid) = summary.pid {
        println!("  PID:            {pid}");
    }
    if let Some(h) = summary.health {
        println!(
            "  Daemon Health:  {} (uptime {}s, clients {})",
            h.status, h.uptime_secs, h.active_clients
        );
    }
    if let Some(err) = summary.error {
        println!("  Detail:         {err}");
    }
    Ok(())
}

async fn handle_health(args: DaemonOutputArgs) -> Result<(), Box<dyn std::error::Error>> {
    let bootstrap_path = DaemonBootstrapInfo::default_runtime_path();
    let mut client = IpcClient::connect_from_bootstrap_file(&bootstrap_path)
        .await
        .map_err(|e| format!("Failed to connect to agsd daemon: {e}"))?;

    let health = client.get_health().await?;
    let recovery = client.get_recovery_status().await?;

    if args.json {
        let combined = serde_json::json!({
            "health": health,
            "recovery": recovery,
        });
        println!("{}", serde_json::to_string_pretty(&combined)?);
        return Ok(());
    }

    println!("Agent Studios Daemon (`agsd`) Health & Recovery");
    println!("{:-<56}", "");
    println!("  Status:            {}", health.status);
    println!("  Active Clients:    {}", health.active_clients);
    println!("  Uptime (secs):     {}", health.uptime_secs);
    println!("  Buffered Events:   {}", health.memory_events_count);
    println!("  Database Healthy:  {}", recovery.database_healthy);
    println!("  Persisted Events:  {}", recovery.total_persisted_events);
    println!("  Interrupted Tasks: {}", recovery.interrupted_tasks_count);
    Ok(())
}
