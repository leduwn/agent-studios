use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::error::ClaudeCodeError;
use crate::sandbox::{ProcessContainment, SandboxPolicy};

/// Maximum size for the circular ring buffer holding child stderr (64 KiB).
pub const STDERR_RING_BUFFER_LIMIT: usize = 64 * 1024;

/// Process launch configuration for Claude Code CLI.
#[derive(Clone, Debug)]
pub struct ClaudeCodeProcessConfig {
    pub binary_path: PathBuf,
    pub working_dir: PathBuf,
    pub session_id: String,
    pub sandbox_policy: SandboxPolicy,
    pub permission_mode: Option<String>,
    pub max_budget_usd: Option<f64>,
    pub model: Option<String>,
    pub bare: bool,
    pub env_vars: Vec<(String, String)>,
}

/// Supervised child process executing Claude Code under Win32 Job Object containment.
pub struct ClaudeCodeProcess {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    child: Arc<Mutex<Option<Child>>>,
    containment: ProcessContainment,
    stderr_ring_buffer: Arc<Mutex<VecDeque<u8>>>,
    exit_status: Arc<Mutex<Option<std::process::ExitStatus>>>,
    is_terminated: Arc<AtomicBool>,
}

impl ClaudeCodeProcess {
    pub fn containment(&self) -> &ProcessContainment {
        &self.containment
    }

    pub fn is_terminated(&self) -> bool {
        self.is_terminated.load(Ordering::SeqCst)
    }

    /// Writes a single NDJSON line to child stdin followed by `\n` and flushes the pipe.
    pub async fn write_stdin_line(&self, line: &str) -> Result<(), ClaudeCodeError> {
        let mut lock = self.stdin.lock().await;
        if let Some(ref mut stdin) = *lock {
            stdin
                .write_all(line.as_bytes())
                .await
                .map_err(ClaudeCodeError::ProcessIo)?;
            if !line.ends_with('\n') {
                stdin
                    .write_all(b"\n")
                    .await
                    .map_err(ClaudeCodeError::ProcessIo)?;
            }
            stdin.flush().await.map_err(ClaudeCodeError::ProcessIo)?;
            Ok(())
        } else {
            Err(ClaudeCodeError::ProcessIo(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Child stdin pipe is closed",
            )))
        }
    }

    /// Closes the child stdin pipe (signals EOF to child process).
    pub async fn close_stdin(&self) {
        let mut lock = self.stdin.lock().await;
        let _ = lock.take();
    }

    /// Returns a copy of the captured stderr text.
    pub async fn stderr_snapshot(&self) -> String {
        let lock = self.stderr_ring_buffer.lock().await;
        let bytes: Vec<u8> = lock.iter().copied().collect();
        String::from_utf8_lossy(&bytes).to_string()
    }

    /// Returns cached exit status if the child has exited.
    pub async fn exit_status(&self) -> Option<std::process::ExitStatus> {
        let lock = self.exit_status.lock().await;
        *lock
    }

    /// Waits for the child process to exit, caches the exit status in exit_status, and returns it.
    pub async fn wait_for_exit(&self) -> Option<std::process::ExitStatus> {
        {
            let lock = self.exit_status.lock().await;
            if let Some(status) = *lock {
                return Some(status);
            }
        }

        let mut child_lock = self.child.lock().await;
        {
            let lock = self.exit_status.lock().await;
            if let Some(status) = *lock {
                return Some(status);
            }
        }

        if let Some(ref mut child) = *child_lock {
            match child.wait().await {
                Ok(status) => {
                    let mut exit_lock = self.exit_status.lock().await;
                    *exit_lock = Some(status);
                    Some(status)
                }
                Err(_) => {
                    let exit_lock = self.exit_status.lock().await;
                    *exit_lock
                }
            }
        } else {
            let exit_lock = self.exit_status.lock().await;
            *exit_lock
        }
    }

    /// Checks if the child process is still actively running.
    pub async fn is_alive(&self) -> bool {
        if self.is_terminated.load(Ordering::SeqCst) {
            return false;
        }

        let mut lock = self.child.lock().await;
        if let Some(ref mut child) = *lock {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let mut exit_lock = self.exit_status.lock().await;
                    *exit_lock = Some(status);
                    false
                }
                Ok(None) => true,
                Err(_) => false,
            }
        } else {
            false
        }
    }

    /// Terminates the child process tree cleanly and closes OS Job Object.
    pub async fn terminate(&self) -> Result<(), ClaudeCodeError> {
        if self.is_terminated.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        debug!("Terminating Claude Code process supervisor");

        // 1. Close stdin to signal EOF
        self.close_stdin().await;

        // 2. Terminate Job Object tree if available
        let _ = self.containment.terminate();

        // 3. Grace period wait for child exit
        let mut child_lock = self.child.lock().await;
        if let Some(ref mut child) = *child_lock {
            let wait_res = tokio::time::timeout(Duration::from_millis(3000), child.wait()).await;
            match wait_res {
                Ok(Ok(status)) => {
                    info!(status = %status, "Claude Code process exited normally during termination");
                    let mut exit_lock = self.exit_status.lock().await;
                    *exit_lock = Some(status);
                }
                _ => {
                    warn!("Claude Code did not exit within grace period; sending kill signal");
                    let _ = child.kill().await;
                    if let Ok(status) = child.wait().await {
                        let mut exit_lock = self.exit_status.lock().await;
                        *exit_lock = Some(status);
                    }
                }
            }
        }

        Ok(())
    }
}

impl Drop for ClaudeCodeProcess {
    fn drop(&mut self) {
        if !self.is_terminated.swap(true, Ordering::SeqCst) {
            let _ = self.containment.terminate();
        }
    }
}

/// Spawns the Claude Code process in headless stream-json mode under sandbox containment.
pub async fn spawn_claude_process(
    config: ClaudeCodeProcessConfig,
) -> Result<(Arc<ClaudeCodeProcess>, ChildStdout), ClaudeCodeError> {
    let requires_os_containment = config.sandbox_policy.level.requires_os_containment();
    let containment = ProcessContainment::new(config.sandbox_policy.clone())?;

    let mut cmd = Command::new(&config.binary_path);
    cmd.current_dir(&config.working_dir)
        .arg("-p")
        .arg("--output-format=stream-json")
        .arg("--input-format=stream-json")
        .arg("--verbose")
        .arg("--include-partial-messages")
        .arg("--forward-subagent-text")
        .arg("--include-hook-events")
        .arg("--replay-user-messages")
        .arg("--session-id")
        .arg(&config.session_id)
        .arg("--permission-prompts=host")
        .arg("--no-session-persistence")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(ref mode) = config.permission_mode {
        cmd.arg("--permission-mode").arg(mode);
    }

    if let Some(budget) = config.max_budget_usd {
        cmd.arg("--max-budget-usd").arg(budget.to_string());
    }

    if let Some(ref model) = config.model {
        cmd.arg("--model").arg(model);
    }

    if config.bare {
        cmd.arg("--bare");
    }

    for (k, v) in &config.env_vars {
        cmd.env(k, v);
    }

    info!(
        binary = ?config.binary_path,
        working_dir = ?config.working_dir,
        session_id = %config.session_id,
        "Spawning Claude Code CLI process"
    );

    #[cfg(windows)]
    if requires_os_containment {
        use windows_sys::Win32::System::Threading::CREATE_SUSPENDED;
        cmd.creation_flags(CREATE_SUSPENDED);
    }

    let mut child = cmd.spawn().map_err(|e| {
        ClaudeCodeError::ProcessSpawn(format!("{}: {e}", config.binary_path.display()))
    })?;

    // Assign to sandbox containment
    if let Some(pid) = child.id() {
        debug!(pid = pid, "Assigning Claude Code process to containment");
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
            return Err(e);
        }
    }

    let stdin = child.stdin.take().ok_or_else(|| {
        ClaudeCodeError::ProcessSpawn("Failed to capture Claude Code child stdin pipe".to_string())
    })?;

    let stdout = child.stdout.take().ok_or_else(|| {
        ClaudeCodeError::ProcessSpawn("Failed to capture Claude Code child stdout pipe".to_string())
    })?;

    let mut stderr = child.stderr.take().ok_or_else(|| {
        ClaudeCodeError::ProcessSpawn("Failed to capture Claude Code child stderr pipe".to_string())
    })?;

    let stderr_ring_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(
        STDERR_RING_BUFFER_LIMIT,
    )));
    let is_terminated = Arc::new(AtomicBool::new(false));
    let exit_status = Arc::new(Mutex::new(None));

    // Spawn background stderr pump to feed circular ring buffer
    let stderr_buf_clone = Arc::clone(&stderr_ring_buffer);
    let term_flag_clone = Arc::clone(&is_terminated);
    tokio::spawn(async move {
        let mut chunk = [0u8; 4096];
        while !term_flag_clone.load(Ordering::SeqCst) {
            match stderr.read(&mut chunk).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let mut lock = stderr_buf_clone.lock().await;
                    for &byte in &chunk[..n] {
                        if lock.len() >= STDERR_RING_BUFFER_LIMIT {
                            lock.pop_front();
                        }
                        lock.push_back(byte);
                    }
                }
                Err(e) => {
                    debug!(error = %e, "Stderr stream closed or encountered error");
                    break;
                }
            }
        }
    });

    let process = Arc::new(ClaudeCodeProcess {
        stdin: Arc::new(Mutex::new(Some(stdin))),
        child: Arc::new(Mutex::new(Some(child))),
        containment,
        stderr_ring_buffer,
        exit_status,
        is_terminated,
    });

    Ok((process, stdout))
}
