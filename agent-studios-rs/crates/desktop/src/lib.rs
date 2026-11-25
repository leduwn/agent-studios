//! Agent Studios Standalone Desktop Host & Daemon Lifecycle Supervisor (Phase B / RC2)
//!
//! Provides:
//! - Canonical Windows & POSIX configuration, state DB, artifacts, and isolated IDE profile setup
//! - Embedded `agsd` background daemon discovery, auto-launch, health polling, crash respawn, and admin shutdown
//! - Standalone Code-OSS / VS Code Electron host launcher with isolated `--user-data-dir` and `--extensions-dir`
//! - Non-destructive Agent Mode <-> IDE Mode continuity over the single authoritative daemon session

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::{Child, Command};
use tokio::time::Instant;

use agent_studios_ipc_transport::daemon::{DaemonAdminBootstrapInfo, DaemonBootstrapInfo};
use agent_studios_ipc_transport::{HealthResponse, IpcClient};
use agent_studios_protocol::id::StudioId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "snake_case")]
pub enum DesktopMode {
    #[default]
    Agent,
    Ide,
}

impl DesktopMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DesktopMode::Agent => "agent",
            DesktopMode::Ide => "ide",
        }
    }
}

#[derive(Debug, Error)]
pub enum DesktopError {
    #[error("Desktop I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("IPC error: {0}")]
    Ipc(String),
    #[error("Required binary not found: {0}")]
    BinaryNotFound(String),
    #[error("Timed out waiting for daemon readiness after {0}ms")]
    Timeout(u64),
    #[error("Configuration error: {0}")]
    Config(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalDesktopPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub config_toml: PathBuf,
    pub state_db: PathBuf,
    pub artifacts_dir: PathBuf,
    pub profile_user_data_dir: PathBuf,
    pub extensions_dir: PathBuf,
    pub runtime_bootstrap: PathBuf,
    pub admin_bootstrap: PathBuf,
}

impl CanonicalDesktopPaths {
    pub fn resolve_default() -> Self {
        let config_base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        let data_base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
        Self::from_roots(config_base, data_base)
    }

    pub fn from_roots(app_data_root: PathBuf, local_app_data_root: PathBuf) -> Self {
        let config_dir = app_data_root.join("AgentStudios");
        let data_dir = local_app_data_root.join("AgentStudios");
        let runtime_dir = local_app_data_root.join("agent-studios");
        Self {
            config_toml: config_dir.join("config.toml"),
            state_db: data_dir.join("state.db"),
            artifacts_dir: data_dir.join("artifacts"),
            profile_user_data_dir: config_dir.join("desktop-profile"),
            extensions_dir: data_dir.join("extensions"),
            runtime_bootstrap: runtime_dir.join("agsd.json"),
            admin_bootstrap: runtime_dir.join("agsd-admin.json"),
            config_dir,
            data_dir,
        }
    }

    /// Creates canonical directories, writes default `config.toml` only when absent,
    /// and seeds `desktop-profile/User/settings.json` non-destructively.
    pub fn ensure_provisioned(
        &self,
        display_language: &str,
        agent_response_language: &str,
    ) -> Result<(), DesktopError> {
        let profile_user_dir = self.profile_user_data_dir.join("User");
        for dir in [
            &self.config_dir,
            &self.data_dir,
            &self.artifacts_dir,
            &self.profile_user_data_dir,
            &profile_user_dir,
            &self.extensions_dir,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        if let Some(rt_parent) = self.runtime_bootstrap.parent() {
            std::fs::create_dir_all(rt_parent)?;
        }

        if !self.config_toml.exists() {
            let default_toml = format!(
                "# Agent Studios Desktop Configuration\n[ui]\ndisplay_language = \"{}\"\nagent_response_language = \"{}\"\ndefault_mode = \"agent\"\n\n[security]\nzero_billable_guard = true\nfail_closed_sandbox = true\n",
                display_language, agent_response_language
            );
            std::fs::write(&self.config_toml, default_toml)?;
        }

        let settings_path = profile_user_dir.join("settings.json");
        let mut settings_obj = if settings_path.is_file() {
            let raw = std::fs::read_to_string(&settings_path).unwrap_or_else(|_| "{}".to_string());
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&raw)
                .unwrap_or_default()
        } else {
            serde_json::Map::new()
        };

        settings_obj
            .entry("agentStudios.displayLanguage".to_string())
            .or_insert_with(|| serde_json::Value::String(display_language.to_string()));
        settings_obj
            .entry("agentStudios.agentResponseLanguage".to_string())
            .or_insert_with(|| serde_json::Value::String(agent_response_language.to_string()));
        settings_obj
            .entry("workbench.startupEditor".to_string())
            .or_insert_with(|| serde_json::Value::String("none".to_string()));
        settings_obj
            .entry("security.workspace.trust.enabled".to_string())
            .or_insert_with(|| serde_json::Value::Bool(true));

        let serialized = serde_json::to_string_pretty(&settings_obj)
            .map_err(|e| DesktopError::Config(format!("Serialize desktop settings.json: {e}")))?;
        std::fs::write(&settings_path, serialized)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub running: bool,
    pub owned_by_supervisor: bool,
    pub pid: Option<u32>,
    pub endpoint: Option<String>,
    pub health: Option<HealthResponse>,
    pub bootstrap_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DesktopSupervisorConfig {
    pub paths: CanonicalDesktopPaths,
    pub agsd_binary_override: Option<PathBuf>,
    pub startup_timeout: Duration,
    pub pipe_or_socket_name: String,
}

impl DesktopSupervisorConfig {
    pub fn from_paths(paths: CanonicalDesktopPaths) -> Self {
        let id = uuid::Uuid::new_v4().simple().to_string();
        #[cfg(windows)]
        let endpoint = format!(r"\\.\pipe\agent-studios-desktop-{id}");
        #[cfg(not(windows))]
        let endpoint = paths
            .runtime_bootstrap
            .parent()
            .unwrap_or_else(|| Path::new("/tmp"))
            .join(format!("agsd-desktop-{id}.sock"))
            .to_string_lossy()
            .into_owned();

        Self {
            paths,
            agsd_binary_override: None,
            startup_timeout: Duration::from_secs(15),
            pipe_or_socket_name: endpoint,
        }
    }

    pub fn with_agsd_binary(mut self, path: PathBuf) -> Self {
        self.agsd_binary_override = Some(path);
        self
    }
}

pub struct DesktopDaemonSupervisor {
    config: DesktopSupervisorConfig,
    child: Option<Child>,
    owned_pid: Option<u32>,
}

impl DesktopDaemonSupervisor {
    pub fn new(config: DesktopSupervisorConfig) -> Self {
        Self {
            config,
            child: None,
            owned_pid: None,
        }
    }

    pub fn resolve_agsd_binary(&self) -> Result<PathBuf, DesktopError> {
        if let Some(p) = &self.config.agsd_binary_override
            && p.is_file()
        {
            return Ok(p.clone());
        }
        if let Ok(env_path) = std::env::var("AGSD_BIN_PATH") {
            let p = PathBuf::from(env_path.trim());
            if p.is_file() {
                return Ok(p);
            }
        }

        let bin_name = if cfg!(windows) { "agsd.exe" } else { "agsd" };
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            let sibling = dir.join(bin_name);
            if sibling.is_file() {
                return Ok(sibling);
            }
            if let Some(parent) = dir.parent() {
                let target_bin = parent.join(bin_name);
                if target_bin.is_file() {
                    return Ok(target_bin);
                }
            }
        }

        if let Some(found) = find_on_path(bin_name) {
            return Ok(found);
        }

        Err(DesktopError::BinaryNotFound(format!(
            "Could not locate `{bin_name}` next to AgentStudios executable, in AGSD_BIN_PATH, or on PATH"
        )))
    }

    pub async fn probe_status(&self) -> DaemonStatus {
        let rt_path = &self.config.paths.runtime_bootstrap;
        if !rt_path.is_file() {
            return DaemonStatus {
                running: false,
                owned_by_supervisor: self.owned_pid.is_some(),
                pid: self.owned_pid,
                endpoint: None,
                health: None,
                bootstrap_path: rt_path.clone(),
            };
        }

        let info = match DaemonBootstrapInfo::load_from_file(rt_path) {
            Ok(i) => i,
            Err(_) => {
                return DaemonStatus {
                    running: false,
                    owned_by_supervisor: self.owned_pid.is_some(),
                    pid: self.owned_pid,
                    endpoint: None,
                    health: None,
                    bootstrap_path: rt_path.clone(),
                };
            }
        };

        match IpcClient::connect_from_bootstrap_file(rt_path).await {
            Ok(mut client) => match client.get_health().await {
                Ok(health) => DaemonStatus {
                    running: true,
                    owned_by_supervisor: self.owned_pid.is_some(),
                    pid: Some(info.pid),
                    endpoint: Some(info.endpoint),
                    health: Some(health),
                    bootstrap_path: rt_path.clone(),
                },
                Err(_) => DaemonStatus {
                    running: false,
                    owned_by_supervisor: self.owned_pid.is_some(),
                    pid: Some(info.pid),
                    endpoint: Some(info.endpoint),
                    health: None,
                    bootstrap_path: rt_path.clone(),
                },
            },
            Err(_) => DaemonStatus {
                running: false,
                owned_by_supervisor: self.owned_pid.is_some(),
                pid: Some(info.pid),
                endpoint: Some(info.endpoint),
                health: None,
                bootstrap_path: rt_path.clone(),
            },
        }
    }

    pub async fn ensure_running(&mut self) -> Result<DaemonStatus, DesktopError> {
        let current = self.probe_status().await;
        if current.running {
            return Ok(current);
        }
        self.spawn_daemon().await
    }

    /// Verifies daemon health; if dead or unreachable (crash recovery), respawns cleanly
    /// onto the same SQLite WAL database path so persisted sessions and tasks survive.
    pub async fn ensure_healthy_or_respawn(&mut self) -> Result<DaemonStatus, DesktopError> {
        if let Some(child) = self.child.as_mut()
            && let Ok(Some(_exit)) = child.try_wait()
        {
            self.child = None;
            self.owned_pid = None;
        }
        let status = self.probe_status().await;
        if status.running {
            return Ok(status);
        }
        self.spawn_daemon().await
    }

    async fn spawn_daemon(&mut self) -> Result<DaemonStatus, DesktopError> {
        let agsd_bin = self.resolve_agsd_binary()?;
        let _ = std::fs::remove_file(&self.config.paths.runtime_bootstrap);
        let _ = std::fs::remove_file(&self.config.paths.admin_bootstrap);

        if let Some(parent) = self.config.paths.state_db.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut cmd = Command::new(&agsd_bin);
        cmd.arg("--runtime-info-path")
            .arg(&self.config.paths.runtime_bootstrap)
            .arg("--admin-runtime-info-path")
            .arg(&self.config.paths.admin_bootstrap)
            .arg("--db-path")
            .arg(&self.config.paths.state_db)
            .arg("--state-dir")
            .arg(&self.config.paths.data_dir);

        #[cfg(windows)]
        {
            cmd.arg("--pipe-name").arg(&self.config.pipe_or_socket_name);
        }
        #[cfg(not(windows))]
        {
            cmd.arg("--socket-path")
                .arg(&self.config.pipe_or_socket_name);
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd.spawn()?;
        let spawned_pid = child.id();
        self.child = Some(child);
        self.owned_pid = spawned_pid;

        let deadline = Instant::now() + self.config.startup_timeout;
        while Instant::now() < deadline {
            if let Some(child) = self.child.as_mut()
                && let Ok(Some(exit)) = child.try_wait()
            {
                return Err(DesktopError::Ipc(format!(
                    "agsd daemon exited prematurely with status {exit}"
                )));
            }
            let status = self.probe_status().await;
            if status.running {
                return Ok(status);
            }
            tokio::time::sleep(Duration::from_millis(80)).await;
        }

        Err(DesktopError::Timeout(
            self.config.startup_timeout.as_millis() as u64,
        ))
    }

    /// Gracefully shuts down the daemon via authenticated admin IPC if owned (or when forced).
    pub async fn shutdown_daemon(&mut self) -> Result<bool, DesktopError> {
        let admin_path = &self.config.paths.admin_bootstrap;
        if !admin_path.is_file() {
            if let Some(mut child) = self.child.take() {
                let _ = child.kill().await;
            }
            self.owned_pid = None;
            return Ok(false);
        }

        match IpcClient::connect_from_admin_bootstrap_file(admin_path).await {
            Ok(mut admin_client) => {
                admin_client
                    .shutdown()
                    .await
                    .map_err(|e| DesktopError::Ipc(format!("Admin shutdown failed: {e}")))?;
                if let Some(mut child) = self.child.take() {
                    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
                }
                self.owned_pid = None;
                let _ = std::fs::remove_file(&self.config.paths.runtime_bootstrap);
                let _ = std::fs::remove_file(&self.config.paths.admin_bootstrap);
                Ok(true)
            }
            Err(e) => Err(DesktopError::Ipc(format!(
                "Could not connect with admin bootstrap {}: {e}",
                admin_path.display()
            ))),
        }
    }

    pub fn owned_pid(&self) -> Option<u32> {
        self.owned_pid
    }
}

/// Discovers a Code-OSS / VS Code desktop shell binary (`Code.exe` / `code-oss` / `code`).
pub fn resolve_workbench_binary(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit
        && p.is_file()
    {
        return Some(p.to_path_buf());
    }
    if let Ok(env_bin) = std::env::var("AGENT_STUDIOS_CODE_BIN") {
        let p = PathBuf::from(env_bin.trim());
        if p.is_file() {
            return Some(p);
        }
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidates = if cfg!(windows) {
            vec![
                dir.join("code-oss").join("Code.exe"),
                dir.join("Code.exe"),
                dir.parent()
                    .map(|p| p.join("code-oss").join("Code.exe"))
                    .unwrap_or_default(),
            ]
        } else {
            vec![dir.join("code-oss").join("code-oss"), dir.join("code-oss")]
        };
        for c in candidates {
            if !c.as_os_str().is_empty() && c.is_file() {
                return Some(c);
            }
        }
    }

    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let user_code = PathBuf::from(local)
                .join("Programs")
                .join("Microsoft VS Code")
                .join("Code.exe");
            if user_code.is_file() {
                return Some(user_code);
            }
        }
        if let Ok(pf) = std::env::var("ProgramFiles") {
            let sys_code = PathBuf::from(pf).join("Microsoft VS Code").join("Code.exe");
            if sys_code.is_file() {
                return Some(sys_code);
            }
        }
        if let Some(p) = find_on_path("code.cmd") {
            return Some(p);
        }
    }

    #[cfg(not(windows))]
    {
        for name in ["code-oss", "code"] {
            if let Some(p) = find_on_path(name) {
                return Some(p);
            }
        }
    }

    None
}

/// Discovers the bundled `agent-studios-vscode-*.vsix` package relative to `AgentStudios.exe` or install root.
pub fn find_bundled_vsix(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit
        && p.is_file()
    {
        return Some(p.to_path_buf());
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let mut search_dirs = vec![dir.join("extension")];
        if let Some(parent) = dir.parent() {
            search_dirs.push(parent.join("extension"));
        }
        for ext_dir in search_dirs {
            if ext_dir.is_dir()
                && let Ok(entries) = std::fs::read_dir(&ext_dir)
            {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("vsix") {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

/// Builds deterministic isolated arguments for launching the Code-OSS desktop host.
pub fn build_workbench_launch_args(
    paths: &CanonicalDesktopPaths,
    workspace: Option<&Path>,
    mode: DesktopMode,
) -> (Vec<String>, Vec<(String, String)>) {
    let mut args = vec![
        "--user-data-dir".to_string(),
        paths.profile_user_data_dir.to_string_lossy().into_owned(),
        "--extensions-dir".to_string(),
        paths.extensions_dir.to_string_lossy().into_owned(),
        "--new-window".to_string(),
    ];
    if let Some(ws) = workspace {
        args.push(ws.to_string_lossy().into_owned());
    }

    let env_vars = vec![
        (
            "AGSD_RUNTIME_INFO_PATH".to_string(),
            paths.runtime_bootstrap.to_string_lossy().into_owned(),
        ),
        (
            "AGENT_STUDIOS_DEFAULT_MODE".to_string(),
            mode.as_str().to_string(),
        ),
        (
            "AGENT_STUDIOS_STATE_DB".to_string(),
            paths.state_db.to_string_lossy().into_owned(),
        ),
        (
            "AGENT_STUDIOS_ARTIFACTS_DIR".to_string(),
            paths.artifacts_dir.to_string_lossy().into_owned(),
        ),
    ];

    (args, env_vars)
}

/// Installs the bundled `.vsix` into the isolated `--extensions-dir` cleanly without touching `~/.vscode`.
pub fn ensure_extension_installed_in_isolated_profile(
    workbench_bin: &Path,
    paths: &CanonicalDesktopPaths,
    vsix_path: &Path,
) -> Result<(), DesktopError> {
    let mut cmd = std::process::Command::new(workbench_bin);
    cmd.args([
        "--user-data-dir",
        &paths.profile_user_data_dir.to_string_lossy(),
        "--extensions-dir",
        &paths.extensions_dir.to_string_lossy(),
        "--install-extension",
        &vsix_path.to_string_lossy(),
        "--force",
    ]);
    let status = cmd.status()?;
    if !status.success() {
        return Err(DesktopError::Config(format!(
            "Failed to install extension {} into isolated profile (exit {status})",
            vsix_path.display()
        )));
    }
    Ok(())
}

/// Represents an active desktop session preserving `studio_id` and `daemon_pid` across Agent <-> IDE transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopSessionState {
    pub studio_id: StudioId,
    pub daemon_pid: u32,
    pub active_mode: DesktopMode,
    pub workspace_path: PathBuf,
}

impl DesktopSessionState {
    pub fn transition_mode(&mut self, next_mode: DesktopMode) {
        self.active_mode = next_mode;
    }
}

fn find_on_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(binary_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Ensures the default bootstrap file used by `DaemonBootstrapInfo::default_runtime_path()`
/// stays usable during shutdown or inspection.
pub fn resolve_admin_path_for(runtime_path: &Path) -> PathBuf {
    let default_admin = DaemonAdminBootstrapInfo::default_admin_runtime_path();
    runtime_path
        .parent()
        .map(|p| p.join("agsd-admin.json"))
        .unwrap_or(default_admin)
}
