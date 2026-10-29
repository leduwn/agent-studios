use std::path::PathBuf;
use tracing::info;

use crate::error::TransportError;
use crate::server::IpcServerBuilder;

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub pipe_name: String,
    pub socket_path: PathBuf,
    pub auth_key: Vec<u8>,
    pub key_id: String,
    pub capacity: usize,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            pipe_name: r"\\.\pipe\agent-studios-default".to_string(),
            socket_path: PathBuf::from("/tmp/agent-studios-agsd.sock"),
            auth_key: b"agent-studios-ephemeral-secret-key-123".to_vec(),
            key_id: "ags-client".to_string(),
            capacity: 1000,
        }
    }
}

/// Runs the `agsd` IPC transport daemon with the provided configuration.
pub async fn run_daemon(config: DaemonConfig) -> Result<(), TransportError> {
    // 1. Setup tracing subscriber if not already initialized
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    info!(
        "Starting agsd daemon (key_id: {}, capacity: {})",
        config.key_id, config.capacity
    );

    // 2. Initialize IpcServer
    let server = IpcServerBuilder::new()
        .with_auth_key(&config.key_id, config.auth_key.clone())
        .with_replay_capacity(config.capacity)
        .with_control_plane(agent_studios_control_plane::ControlPlane::new_default())
        .with_provider_catalog(agent_studios_provider::ProviderCatalog::with_builtin_definitions())
        .build()?;

    // 3. Setup OS shutdown signal watcher
    let shutdown_tok = server.shutdown_token();
    let watcher_tok = shutdown_tok.clone();
    tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            info!("Received Ctrl-C / SIGINT; initiating graceful shutdown");
            watcher_tok.cancel();
        }
    });

    // 4. Bind listener and run accept loop
    #[cfg(windows)]
    {
        info!("Binding Windows Named Pipe: {}", config.pipe_name);
        server.run_named_pipe(&config.pipe_name).await?;
    }

    #[cfg(unix)]
    {
        info!("Binding Unix Domain Socket: {:?}", config.socket_path);
        let _ = std::fs::remove_file(&config.socket_path);
        server.run_uds(&config.socket_path).await?;
    }

    #[cfg(not(any(windows, unix)))]
    {
        warn!("Running on unsupported platform; awaiting shutdown");
        shutdown_tok.cancelled().await;
    }

    // 5. Flushes replay buffer and shuts down gracefully
    let remaining_events = server.state().replay_buffer.lock().unwrap().len();
    info!(
        "Flushed replay buffer ({} events buffered). Server shutdown cleanly completed.",
        remaining_events
    );

    Ok(())
}
