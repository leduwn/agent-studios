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

/// Generates a cryptographically secure 32-byte (256-bit) password formatted as 64 hex characters.
pub fn generate_secure_password() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("CSPRNG failure while generating server password");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Configuration for launching or connecting to an OpenCode server.
#[derive(Clone, Debug)]
pub struct OpenCodeServerConfig {
    pub binary_path: PathBuf,
    pub working_dir: PathBuf,
    pub port: Option<u16>,
    pub username: String,
    pub password: Option<String>,
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
            username: "opencode".to_string(),
            password: None,
            startup_timeout: DEFAULT_STARTUP_TIMEOUT,
            sandbox_policy: SandboxPolicy::default(),
            env_vars: Vec::new(),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn with_auth_credentials(
        mut self,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        self.username = username.into();
        self.password = Some(password.into());
        self
    }

    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        self.password = Some(token.into());
        self
    }

    pub fn with_sandbox_policy(mut self, policy: SandboxPolicy) -> Self {
        self.sandbox_policy = policy;
        self
    }

    pub fn with_startup_timeout(mut self, timeout: Duration) -> Self {
        self.startup_timeout = timeout;
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
    username: String,
    password: String,
    containment: ProcessContainment,
    child: Arc<Mutex<Option<Child>>>,
}

impl OpenCodeServerHandle {
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn password(&self) -> &str {
        &self.password
    }

    /// Backwards-compatible accessor for authorization token/password.
    pub fn auth_token(&self) -> &str {
        &self.password
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
        let token = auth_token.into();
        Ok(Self {
            base_url,
            username: "opencode".to_string(),
            password: token,
            containment,
            child: Arc::new(Mutex::new(None)),
        })
    }

    pub fn from_existing_credentials(
        base_url: Url,
        username: impl Into<String>,
        password: impl Into<String>,
        containment: ProcessContainment,
    ) -> Result<Self, OpenCodeError> {
        verify_loopback_url(&base_url)?;
        Ok(Self {
            base_url,
            username: username.into(),
            password: password.into(),
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

/// Spawns a supervised OpenCode server process with loopback binding, ephemeral HTTP Basic authentication,
/// and OS-level sandbox containment.
pub async fn spawn_opencode_server(
    config: OpenCodeServerConfig,
) -> Result<OpenCodeServerHandle, OpenCodeError> {
    // 0. Validate working directory exists before spawning (M11-R09)
    if !config.working_dir.exists() {
        return Err(OpenCodeError::InvalidConfiguration(format!(
            "Working directory does not exist: {}",
            config.working_dir.display()
        )));
    }

    // 1. Allocate loopback port if not specified
    let port = match config.port {
        Some(p) => p,
        None => pick_ephemeral_loopback_port().await?,
    };

    let base_url = Url::parse(&format!("http://127.0.0.1:{port}"))
        .map_err(|e| OpenCodeError::InvalidConfiguration(format!("Invalid server URL: {e}")))?;
    verify_loopback_url(&base_url)?;

    // 2. Generate or use CSPRNG 32-byte server password and username
    let password = config.password.unwrap_or_else(generate_secure_password);
    let username = if config.username.is_empty() {
        "opencode".to_string()
    } else {
        config.username
    };

    // 3. Initialize OS process containment BEFORE child process startup (fails closed if unavailable)
    let requires_os_containment = config.sandbox_policy.level.requires_os_containment();
    let containment = ProcessContainment::new(config.sandbox_policy)?;

    // 4. Construct Command: zero tokens in CLI args, pass credentials strictly via process environment
    let mut cmd = Command::new(&config.binary_path);
    cmd.current_dir(&config.working_dir)
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--hostname")
        .arg("127.0.0.1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("OPENCODE_SERVER_PASSWORD", &password)
        .env("OPENCODE_SERVER_USERNAME", &username)
        .env("OPENCODE_HOST", "127.0.0.1")
        .env("OPENCODE_PORT", port.to_string());

    for (k, v) in config.env_vars {
        cmd.env(k, v);
    }

    info!(
        binary = ?config.binary_path,
        working_dir = ?config.working_dir,
        port = port,
        "Spawning OpenCode server process with HTTP Basic authentication"
    );

    #[cfg(windows)]
    if requires_os_containment {
        use windows_sys::Win32::System::Threading::CREATE_SUSPENDED;
        cmd.creation_flags(CREATE_SUSPENDED);
    }

    let mut child = cmd.spawn().map_err(|e| {
        OpenCodeError::ProcessSpawn(format!("{}: {e}", config.binary_path.display()))
    })?;

    // 5. Assign child to sandbox containment immediately; fail closed and terminate child if assignment fails.
    // On Windows under OS containment, process was launched suspended so zero user-mode instructions ran prior to containment (M11-R04).
    if let Some(pid) = child.id() {
        debug!(pid = pid, "Assigning child process to containment");
        #[cfg(windows)]
        let assign_result = if requires_os_containment {
            containment.assign_and_resume_child(pid)
        } else {
            containment.assign_child(pid)
        };
        #[cfg(not(windows))]
        let assign_result = containment.assign_child(pid);

        if let Err(e) = assign_result {
            warn!(pid = pid, error = %e, "Containment assignment failed; terminating child immediately");
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(e);
        }
    }

    // 6. Asynchronous stdout and bounded stderr draining to prevent pipe buffer deadlock (M11-R09)
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::debug!(target: "opencode_stdout", "{line}");
            }
        });
    }

    let stderr_lines = Arc::new(Mutex::new(
        std::collections::VecDeque::<String>::with_capacity(32),
    ));
    if let Some(stderr) = child.stderr.take() {
        let stderr_buf = Arc::clone(&stderr_lines);
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::debug!(target: "opencode_stderr", "{line}");
                let mut buf = stderr_buf.lock().await;
                if buf.len() >= 32 {
                    buf.pop_front();
                }
                let truncated = if line.len() > 1024 {
                    format!("{}...", &line[..1024])
                } else {
                    line
                };
                buf.push_back(truncated);
            }
        });
    }

    let handle = OpenCodeServerHandle {
        base_url: base_url.clone(),
        username: username.clone(),
        password: password.clone(),
        containment,
        child: Arc::new(Mutex::new(Some(child))),
    };

    // 7. Poll health check with HTTP Basic Auth until ready or timeout, detecting early child exit immediately (M11-R09)
    if let Err(err) = poll_server_health(
        &base_url,
        &username,
        &password,
        config.startup_timeout,
        port,
        &handle.child,
        &stderr_lines,
    )
    .await
    {
        let _ = handle.stop().await;
        return Err(err);
    }

    info!(
        port = port,
        "OpenCode server is healthy and accepting requests"
    );
    Ok(handle)
}

/// Verifies that the URL strictly binds to loopback interface.
pub fn verify_loopback_url(url: &Url) -> Result<(), OpenCodeError> {
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(OpenCodeError::InvalidConfiguration(format!(
            "Server URL scheme must be http or https, got '{}'",
            url.scheme()
        )));
    }

    match url.host() {
        Some(url::Host::Ipv4(ip)) if ip.is_loopback() => Ok(()),
        Some(url::Host::Ipv6(ip)) if ip.is_loopback() => Ok(()),
        Some(url::Host::Domain(domain)) if domain.eq_ignore_ascii_case("localhost") => Ok(()),
        Some(other) => Err(OpenCodeError::SandboxPolicyViolation(format!(
            "OpenCode server must strictly bind to loopback interface (127.0.0.1 or localhost); rejected host '{other}'"
        ))),
        None => Err(OpenCodeError::InvalidConfiguration(
            "URL missing host".into(),
        )),
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
    username: &str,
    password: &str,
    timeout: Duration,
    port: u16,
    child_arc: &Arc<Mutex<Option<Child>>>,
    stderr_lines: &Arc<Mutex<std::collections::VecDeque<String>>>,
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
        // Prompt child exit check: detect if child process crashed early instead of waiting full timeout (M11-R09)
        {
            let mut child_guard = child_arc.lock().await;
            if let Some(child) = child_guard.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let stderr_content = {
                            let buf = stderr_lines.lock().await;
                            buf.iter().cloned().collect::<Vec<_>>().join("\n")
                        };
                        warn!(
                            port = port,
                            exit_code = ?status.code(),
                            stderr = %stderr_content,
                            "OpenCode server child process exited prematurely during startup"
                        );
                        return Err(OpenCodeError::ServerExitUnexpected {
                            exit_code: status.code(),
                            stderr: stderr_content,
                        });
                    }
                    Ok(None) => {} // Still running
                    Err(e) => {
                        warn!(error = %e, "Failed to poll child process status");
                    }
                }
            }
        }

        match client
            .get(health_url.clone())
            .basic_auth(username, Some(password))
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

    let stderr_content = {
        let buf = stderr_lines.lock().await;
        buf.iter().cloned().collect::<Vec<_>>().join("\n")
    };

    warn!(
        port = port,
        last_error = %last_err,
        stderr = %stderr_content,
        "OpenCode server health check polling timed out"
    );

    Err(OpenCodeError::ServerStartupTimeout {
        elapsed_ms: start.elapsed().as_millis() as u64,
        port,
    })
}
