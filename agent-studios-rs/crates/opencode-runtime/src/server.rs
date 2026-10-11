use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use url::Url;

use crate::error::OpenCodeError;
use crate::sandbox::{ProcessContainment, SandboxPolicy};

/// Default startup timeout for the OpenCode server to become healthy.
pub const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
pub const HEALTH_CHECK_INTERVAL: Duration = Duration::from_millis(150);

/// Configuration for launching or connecting to an OpenCode server.
#[derive(Clone, Debug)]
pub struct OpenCodeServerConfig {
    pub binary_path: PathBuf,
    pub working_dir: PathBuf,
    pub port: Option<u16>,
    pub auth_token: Option<String>,
    pub startup_timeout: Duration,
    pub sandbox_policy: SandboxPolicy,
    pub env_vars: Vec<(String, String)>,
}

impl OpenCodeServerConfig {
    pub fn new(binary_path: impl Into<PathBuf>, working_dir: impl Into<PathBuf>) -> Self {
        Self {
            binary_path: binary_path.into(),
            working_dir: working_dir.into(),
            port: None,
            auth_token: None,
            startup_timeout: DEFAULT_STARTUP_TIMEOUT,
            sandbox_policy: SandboxPolicy::default(),
            env_vars: Vec::new(),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    pub fn with_sandbox_policy(mut self, policy: SandboxPolicy) -> Self {
        self.sandbox_policy = policy;
        self
    }

    pub fn with_env_var(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.env_vars.push((key.into(), val.into()));
        self
    }
}

/// Supervised OpenCode server instance.
pub struct OpenCodeServerHandle {
    base_url: Url,
    auth_token: String,
    containment: ProcessContainment,
    child: Arc<Mutex<Option<Child>>>,
}

impl OpenCodeServerHandle {
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub fn auth_token(&self) -> &str {
        &self.auth_token
    }

    pub fn containment(&self) -> &ProcessContainment {
        &self.containment
    }

    /// Creates an attached handle to an externally managed or mock server.
    /// Strictly verifies loopback binding to prevent remote or unencrypted communication.
    pub fn from_existing(
        base_url: Url,
        auth_token: impl Into<String>,
        containment: ProcessContainment,
    ) -> Result<Self, OpenCodeError> {
        verify_loopback_url(&base_url)?;
        Ok(Self {
            base_url,
            auth_token: auth_token.into(),
            containment,
            child: Arc::new(Mutex::new(None)),
        })
    }

    /// Stops and terminates the supervised child process.
    pub async fn stop(&self) -> Result<(), OpenCodeError> {
        info!(url = %self.base_url, "Stopping OpenCode server process");

        // 1. Terminate containment tree (Job Object on Windows)
        let _ = self.containment.terminate();

        // 2. Kill tokio child process handle if present
        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }

        Ok(())
    }
}

impl Drop for OpenCodeServerHandle {
    fn drop(&mut self) {
        let _ = self.containment.terminate();
    }
}

/// Spawns a supervised OpenCode server process with loopback binding, ephemeral authentication,
/// and OS-level sandbox containment.
pub async fn spawn_opencode_server(
    config: OpenCodeServerConfig,
) -> Result<OpenCodeServerHandle, OpenCodeError> {
    // 1. Allocate loopback port if not specified
    let port = match config.port {
        Some(p) => p,
        None => pick_ephemeral_loopback_port().await?,
    };

    let base_url = Url::parse(&format!("http://127.0.0.1:{port}"))
        .map_err(|e| OpenCodeError::InvalidConfiguration(format!("Invalid server URL: {e}")))?;
    verify_loopback_url(&base_url)?;

    // 2. Generate or use ephemeral auth token
    let auth_token = config
        .auth_token
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // 3. Initialize OS process containment (fails closed if unavailable and required)
    let containment = ProcessContainment::new(config.sandbox_policy)?;

    // 4. Construct Command
    let mut cmd = Command::new(&config.binary_path);
    cmd.current_dir(&config.working_dir)
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--hostname")
        .arg("127.0.0.1")
        .arg("--token")
        .arg(&auth_token)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("OPENCODE_SERVER_TOKEN", &auth_token)
        .env("OPENCODE_HOST", "127.0.0.1")
        .env("OPENCODE_PORT", port.to_string());

    for (k, v) in config.env_vars {
        cmd.env(k, v);
    }

    info!(
        binary = ?config.binary_path,
        working_dir = ?config.working_dir,
        port = port,
        "Spawning OpenCode server process"
    );

    let child = cmd.spawn().map_err(|e| {
        OpenCodeError::ProcessSpawn(format!("{}: {e}", config.binary_path.display()))
    })?;

    // 5. Assign child to sandbox containment immediately
    if let Some(pid) = child.id() {
        debug!(pid = pid, "Assigning child process to containment");
        containment.assign_child(pid)?;
    }

    let handle = OpenCodeServerHandle {
        base_url: base_url.clone(),
        auth_token: auth_token.clone(),
        containment,
        child: Arc::new(Mutex::new(Some(child))),
    };

    // 6. Poll health check until ready or timeout
    poll_server_health(&base_url, &auth_token, config.startup_timeout, port).await?;

    info!(
        port = port,
        "OpenCode server is healthy and accepting requests"
    );
    Ok(handle)
}

/// Verifies that the URL strictly binds to loopback interface.
pub fn verify_loopback_url(url: &Url) -> Result<(), OpenCodeError> {
    let host = url
        .host_str()
        .ok_or_else(|| OpenCodeError::InvalidConfiguration("URL missing host".into()))?;

    if host == "127.0.0.1" || host == "localhost" || host == "::1" || host == "[::1]" {
        Ok(())
    } else {
        Err(OpenCodeError::SandboxPolicyViolation(format!(
            "OpenCode server must strictly bind to loopback interface (127.0.0.1 or localhost); rejected host '{host}'"
        )))
    }
}

async fn pick_ephemeral_loopback_port() -> Result<u16, OpenCodeError> {
    let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|e| {
        OpenCodeError::ProcessSpawn(format!("Failed to bind ephemeral loopback port: {e}"))
    })?;
    let addr = listener
        .local_addr()
        .map_err(|e| OpenCodeError::ProcessSpawn(format!("Failed to get local address: {e}")))?;
    Ok(addr.port())
}

async fn poll_server_health(
    base_url: &Url,
    auth_token: &str,
    timeout: Duration,
    port: u16,
) -> Result<(), OpenCodeError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| OpenCodeError::Http(e.to_string()))?;

    let health_url = base_url
        .join("/global/health")
        .map_err(|e| OpenCodeError::InvalidConfiguration(e.to_string()))?;

    let start = tokio::time::Instant::now();
    let mut last_err = String::from("No response");

    while start.elapsed() < timeout {
        match client
            .get(health_url.clone())
            .bearer_auth(auth_token)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                return Ok(());
            }
            Ok(resp) => {
                last_err = format!("HTTP status {}", resp.status());
            }
            Err(e) => {
                last_err = e.to_string();
            }
        }
        tokio::time::sleep(HEALTH_CHECK_INTERVAL).await;
    }

    warn!(port = port, last_error = %last_err, "OpenCode server health check polling timed out");

    Err(OpenCodeError::ServerStartupTimeout {
        elapsed_ms: start.elapsed().as_millis() as u64,
        port,
    })
}
