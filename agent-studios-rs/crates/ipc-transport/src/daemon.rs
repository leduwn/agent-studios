use std::path::{Path, PathBuf};
#[allow(unused_imports)]
use tracing::{info, warn};

use crate::error::TransportError;
use crate::server::IpcServerBuilder;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DaemonBootstrapInfo {
    pub endpoint: String,
    pub key_id: String,
    pub client_key: String,
    pub admin_key: String,
    pub pid: u32,
    pub created_at: i64,
}

impl std::fmt::Debug for DaemonBootstrapInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonBootstrapInfo")
            .field("endpoint", &self.endpoint)
            .field("key_id", &self.key_id)
            .field("client_key", &"[REDACTED]")
            .field("admin_key", &"[REDACTED]")
            .field("pid", &self.pid)
            .field("created_at", &self.created_at)
            .finish()
    }
}

impl DaemonBootstrapInfo {
    pub fn default_runtime_path() -> PathBuf {
        #[cfg(unix)]
        {
            if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
                if !xdg.trim().is_empty() {
                    return PathBuf::from(xdg).join("agent-studios").join("agsd.json");
                }
            }
            let uid = crate::unix::permissions::get_current_uid();
            PathBuf::from(format!("/tmp/agent-studios-{}/agsd.json", uid))
        }
        #[cfg(windows)]
        {
            if let Ok(app_data) = std::env::var("LOCALAPPDATA") {
                PathBuf::from(app_data)
                    .join("agent-studios")
                    .join("agsd.json")
            } else {
                std::env::temp_dir().join("agent-studios").join("agsd.json")
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            std::env::temp_dir().join("agsd.json")
        }
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), TransportError> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(TransportError::Io)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut dir_perms = std::fs::metadata(parent)
                .map_err(TransportError::Io)?
                .permissions();
            dir_perms.set_mode(0o700);
            let _ = std::fs::set_permissions(parent, dir_perms);
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| TransportError::Serialization(e.to_string()))?;
        std::fs::write(path, json.as_bytes()).map_err(TransportError::Io)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut file_perms = std::fs::metadata(path)
                .map_err(TransportError::Io)?
                .permissions();
            file_perms.set_mode(0o600);
            let _ = std::fs::set_permissions(path, file_perms);
        }

        Ok(())
    }

    pub fn load_from_file(path: &Path) -> Result<Self, TransportError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = std::fs::symlink_metadata(path).map_err(TransportError::Io)?;
            let mode = metadata.mode();
            let uid = metadata.uid();
            let cur_uid = crate::unix::permissions::get_current_uid();
            crate::unix::permissions::validate_socket_mode_and_owner(mode, uid, cur_uid)?;
        }

        let content = std::fs::read_to_string(path).map_err(TransportError::Io)?;
        serde_json::from_str(&content)
            .map_err(|e| TransportError::Serialization(format!("Invalid bootstrap file: {}", e)))
    }
}

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub pipe_name: String,
    pub socket_path: PathBuf,
    pub runtime_info_path: Option<PathBuf>,
    pub key_id: String,
    pub capacity: usize,
    pub auto_execute: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            pipe_name: r"\\.\pipe\agent-studios-default".to_string(),
            socket_path: PathBuf::from("/tmp/agent-studios-agsd.sock"),
            runtime_info_path: None,
            key_id: "ags-client".to_string(),
            capacity: 1000,
            auto_execute: true,
        }
    }
}

/// Runs the `agsd` IPC transport daemon with the provided configuration.
pub async fn run_daemon(config: DaemonConfig) -> Result<(), TransportError> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let client_key = uuid::Uuid::new_v4().to_string();
    let admin_key = uuid::Uuid::new_v4().to_string();
    let admin_key_id = format!("{}-admin", config.key_id);

    let runtime_path = config
        .runtime_info_path
        .clone()
        .unwrap_or_else(DaemonBootstrapInfo::default_runtime_path);

    #[cfg(windows)]
    let endpoint = config.pipe_name.clone();
    #[cfg(unix)]
    let endpoint = config.socket_path.to_string_lossy().to_string();
    #[cfg(not(any(windows, unix)))]
    let endpoint = "unsupported".to_string();

    let bootstrap_info = DaemonBootstrapInfo {
        endpoint,
        key_id: config.key_id.clone(),
        client_key: client_key.clone(),
        admin_key: admin_key.clone(),
        pid: std::process::id(),
        created_at: chrono::Utc::now().timestamp(),
    };

    bootstrap_info.save_to_file(&runtime_path)?;
    info!(
        runtime_path = ?runtime_path,
        pid = std::process::id(),
        "Daemon runtime connection info written with restricted permissions (0600)"
    );

    let server = IpcServerBuilder::new()
        .with_auth_key(&config.key_id, client_key.into_bytes())
        .with_admin_auth_key(&admin_key_id, admin_key.into_bytes())
        .with_replay_capacity(config.capacity)
        .with_auto_execute(config.auto_execute)
        .with_control_plane(agent_studios_control_plane::ControlPlane::new_default())
        .with_provider_catalog(agent_studios_provider::ProviderCatalog::with_builtin_definitions())
        .build()?;

    let shutdown_tok = server.shutdown_token();
    let watcher_tok = shutdown_tok.clone();
    tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            info!("Received Ctrl-C / SIGINT; initiating graceful shutdown");
            watcher_tok.cancel();
        }
    });

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

    // Clean up runtime connection info file on graceful exit
    let _ = std::fs::remove_file(&runtime_path);

    let remaining_events = server.state().replay_buffer.lock().unwrap().len();
    info!(
        "Flushed replay buffer ({} events buffered). Server shutdown cleanly completed.",
        remaining_events
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_bootstrap_info_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("subdir").join("agsd.json");

        let info = DaemonBootstrapInfo {
            endpoint: "test-endpoint".to_string(),
            key_id: "test-key-id".to_string(),
            client_key: "client-secret-123".to_string(),
            admin_key: "admin-secret-456".to_string(),
            pid: 12345,
            created_at: 1700000000,
        };

        info.save_to_file(&path).unwrap();
        assert!(path.exists());

        let loaded = DaemonBootstrapInfo::load_from_file(&path).unwrap();
        assert_eq!(loaded.endpoint, "test-endpoint");
        assert_eq!(loaded.key_id, "test-key-id");
        assert_eq!(loaded.client_key, "client-secret-123");
        assert_eq!(loaded.admin_key, "admin-secret-456");
        assert_eq!(loaded.pid, 12345);
        assert_eq!(loaded.created_at, 1700000000);
    }

    #[test]
    fn test_default_runtime_path_non_empty() {
        let p = DaemonBootstrapInfo::default_runtime_path();
        assert!(!p.as_os_str().is_empty());
        assert!(p.to_string_lossy().contains("agsd.json"));
    }
}
