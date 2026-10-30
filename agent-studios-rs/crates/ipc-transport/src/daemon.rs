use std::path::{Path, PathBuf};
#[allow(unused_imports)]
use tracing::{info, warn};

use crate::error::TransportError;
use crate::server::IpcServerBuilder;

/// Standard client bootstrap discovery information containing client credentials.
/// By security invariant, ordinary discovery files NEVER contain admin secrets.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DaemonBootstrapInfo {
    pub endpoint: String,
    pub key_id: String,
    pub client_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin_key: Option<String>,
    pub pid: u32,
    pub created_at: i64,
}

impl std::fmt::Debug for DaemonBootstrapInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("DaemonBootstrapInfo");
        d.field("endpoint", &self.endpoint)
            .field("key_id", &self.key_id)
            .field("client_key", &"[REDACTED]");
        if self.admin_key.is_some() {
            d.field("admin_key", &"[REDACTED]");
        }
        d.field("pid", &self.pid)
            .field("created_at", &self.created_at)
            .finish()
    }
}

impl DaemonBootstrapInfo {
    pub fn default_runtime_path() -> PathBuf {
        #[cfg(unix)]
        {
            if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
            {
                return xdg.join("agent-studios").join("agsd.json");
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
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| TransportError::Serialization(e.to_string()))?;
        save_secure_bootstrap_file(path, &json)
    }

    pub fn load_from_file(path: &Path) -> Result<Self, TransportError> {
        let content = load_secure_bootstrap_file(path)?;
        serde_json::from_str(&content)
            .map_err(|e| TransportError::Serialization(format!("Invalid bootstrap file: {}", e)))
    }
}

/// Administrative bootstrap discovery information containing privileged credentials.
/// Kept strictly separated from standard client discovery files in `agsd-admin.json`.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DaemonAdminBootstrapInfo {
    pub endpoint: String,
    pub key_id: String,
    pub admin_key: String,
    pub pid: u32,
    pub created_at: i64,
}

impl std::fmt::Debug for DaemonAdminBootstrapInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonAdminBootstrapInfo")
            .field("endpoint", &self.endpoint)
            .field("key_id", &self.key_id)
            .field("admin_key", &"[REDACTED]")
            .field("pid", &self.pid)
            .field("created_at", &self.created_at)
            .finish()
    }
}

impl DaemonAdminBootstrapInfo {
    pub fn default_admin_runtime_path() -> PathBuf {
        #[cfg(unix)]
        {
            if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
            {
                return xdg.join("agent-studios").join("agsd-admin.json");
            }
            let uid = crate::unix::permissions::get_current_uid();
            PathBuf::from(format!("/tmp/agent-studios-{}/agsd-admin.json", uid))
        }
        #[cfg(windows)]
        {
            if let Ok(app_data) = std::env::var("LOCALAPPDATA") {
                PathBuf::from(app_data)
                    .join("agent-studios")
                    .join("agsd-admin.json")
            } else {
                std::env::temp_dir()
                    .join("agent-studios")
                    .join("agsd-admin.json")
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            std::env::temp_dir().join("agsd-admin.json")
        }
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), TransportError> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| TransportError::Serialization(e.to_string()))?;
        save_secure_bootstrap_file(path, &json)
    }

    pub fn load_from_file(path: &Path) -> Result<Self, TransportError> {
        let content = load_secure_bootstrap_file(path)?;
        serde_json::from_str(&content).map_err(|e| {
            TransportError::Serialization(format!("Invalid admin bootstrap file: {}", e))
        })
    }
}

fn save_secure_bootstrap_file(path: &Path, json: &str) -> Result<(), TransportError> {
    if let Ok(meta) = std::fs::symlink_metadata(path)
        && meta.file_type().is_symlink()
    {
        return Err(TransportError::PermissionDenied(
            "Bootstrap file path is a symlink: potential hijacking attack".into(),
        ));
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if let Ok(meta) = std::fs::symlink_metadata(parent)
        && meta.file_type().is_symlink()
    {
        return Err(TransportError::PermissionDenied(
            "Bootstrap parent directory is a symlink: potential hijacking attack".into(),
        ));
    }

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

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(TransportError::Io)?;
        file.write_all(json.as_bytes())
            .map_err(TransportError::Io)?;
        file.flush().map_err(TransportError::Io)?;
    }

    #[cfg(windows)]
    {
        use std::io::Write;
        use std::os::windows::ffi::OsStrExt;
        use std::os::windows::io::FromRawHandle;
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::Storage::FileSystem::*;

        let sa = crate::windows::security::UserOnlySecurityAttributes::new()?;
        let wide_path: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                FILE_GENERIC_WRITE,
                0,
                sa.as_security_attributes_mut_ptr() as _,
                CREATE_ALWAYS,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(TransportError::Io(std::io::Error::last_os_error()));
        }
        let mut file = unsafe { std::fs::File::from_raw_handle(handle as _) };
        file.write_all(json.as_bytes())
            .map_err(TransportError::Io)?;
        file.flush().map_err(TransportError::Io)?;
    }

    #[cfg(not(any(unix, windows)))]
    {
        std::fs::write(path, json.as_bytes()).map_err(TransportError::Io)?;
    }

    Ok(())
}

fn load_secure_bootstrap_file(path: &Path) -> Result<String, TransportError> {
    let metadata = std::fs::symlink_metadata(path).map_err(TransportError::Io)?;
    if metadata.file_type().is_symlink() {
        return Err(TransportError::PermissionDenied(
            "Bootstrap file path is a symlink: potential hijacking attack".into(),
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mode = metadata.mode();
        let uid = metadata.uid();
        let cur_uid = crate::unix::permissions::get_current_uid();
        crate::unix::permissions::validate_socket_mode_and_owner(mode, uid, cur_uid)?;
    }

    std::fs::read_to_string(path).map_err(TransportError::Io)
}

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub pipe_name: String,
    pub socket_path: PathBuf,
    pub runtime_info_path: Option<PathBuf>,
    pub admin_runtime_info_path: Option<PathBuf>,
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
            admin_runtime_info_path: None,
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
    let admin_runtime_path = config
        .admin_runtime_info_path
        .clone()
        .unwrap_or_else(DaemonAdminBootstrapInfo::default_admin_runtime_path);

    #[cfg(windows)]
    let endpoint = config.pipe_name.clone();
    #[cfg(unix)]
    let endpoint = config.socket_path.to_string_lossy().to_string();
    #[cfg(not(any(windows, unix)))]
    let endpoint = "unsupported".to_string();

    let bootstrap_info = DaemonBootstrapInfo {
        endpoint: endpoint.clone(),
        key_id: config.key_id.clone(),
        client_key: client_key.clone(),
        admin_key: None,
        pid: std::process::id(),
        created_at: chrono::Utc::now().timestamp(),
    };

    bootstrap_info.save_to_file(&runtime_path)?;

    let admin_bootstrap_info = DaemonAdminBootstrapInfo {
        endpoint,
        key_id: admin_key_id.clone(),
        admin_key: admin_key.clone(),
        pid: std::process::id(),
        created_at: chrono::Utc::now().timestamp(),
    };

    admin_bootstrap_info.save_to_file(&admin_runtime_path)?;

    info!(
        runtime_path = ?runtime_path,
        admin_runtime_path = ?admin_runtime_path,
        pid = std::process::id(),
        "Daemon standard and admin runtime connection info written with restricted permissions (0600)"
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

    // Clean up runtime connection info files on graceful exit
    let _ = std::fs::remove_file(&runtime_path);
    let _ = std::fs::remove_file(&admin_runtime_path);

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
            admin_key: None,
            pid: 12345,
            created_at: 1700000000,
        };

        info.save_to_file(&path).unwrap();
        assert!(path.exists());

        // Verify JSON content does not contain admin_key
        let raw_json = std::fs::read_to_string(&path).unwrap();
        assert!(!raw_json.contains("admin_key"));
        assert!(raw_json.contains("client_key"));

        let loaded = DaemonBootstrapInfo::load_from_file(&path).unwrap();
        assert_eq!(loaded.endpoint, "test-endpoint");
        assert_eq!(loaded.key_id, "test-key-id");
        assert_eq!(loaded.client_key, "client-secret-123");
        assert_eq!(loaded.admin_key, None);
        assert_eq!(loaded.pid, 12345);
        assert_eq!(loaded.created_at, 1700000000);
    }

    #[test]
    fn test_daemon_admin_bootstrap_info_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("subdir").join("agsd-admin.json");

        let info = DaemonAdminBootstrapInfo {
            endpoint: "test-endpoint".to_string(),
            key_id: "test-key-id-admin".to_string(),
            admin_key: "admin-secret-456".to_string(),
            pid: 12345,
            created_at: 1700000000,
        };

        info.save_to_file(&path).unwrap();
        assert!(path.exists());

        // Verify JSON content contains admin_key
        let raw_json = std::fs::read_to_string(&path).unwrap();
        assert!(raw_json.contains("admin_key"));
        assert!(!raw_json.contains("client_key"));

        let loaded = DaemonAdminBootstrapInfo::load_from_file(&path).unwrap();
        assert_eq!(loaded.endpoint, "test-endpoint");
        assert_eq!(loaded.key_id, "test-key-id-admin");
        assert_eq!(loaded.admin_key, "admin-secret-456");
        assert_eq!(loaded.pid, 12345);
        assert_eq!(loaded.created_at, 1700000000);

        // Verify debug representation is redacted
        let debug_str = format!("{:?}", loaded);
        assert!(!debug_str.contains("admin-secret-456"));
        assert!(debug_str.contains("[REDACTED]"));
    }

    #[test]
    fn test_default_runtime_path_non_empty() {
        let p = DaemonBootstrapInfo::default_runtime_path();
        assert!(!p.as_os_str().is_empty());
        assert!(p.to_string_lossy().contains("agsd.json"));

        let ap = DaemonAdminBootstrapInfo::default_admin_runtime_path();
        assert!(!ap.as_os_str().is_empty());
        assert!(ap.to_string_lossy().contains("agsd-admin.json"));
    }
}
