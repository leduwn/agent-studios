use agent_studios_external_runtime::{RuntimeError, SanitizedRuntimeMessage};
use thiserror::Error;

/// Errors originating from the OpenCode runtime adapter.
#[derive(Debug, Error)]
pub enum OpenCodeError {
    #[error("Failed to spawn OpenCode process: {0}")]
    ProcessSpawn(String),

    #[error("OpenCode server startup timed out after {elapsed_ms}ms on port {port}")]
    ServerStartupTimeout { elapsed_ms: u64, port: u16 },

    #[error("OpenCode server exited unexpectedly with code {exit_code:?}: {stderr}")]
    ServerExitUnexpected {
        exit_code: Option<i32>,
        stderr: String,
    },

    #[error("OpenCode health check failed at {url} (status {status:?}): {reason}")]
    HealthCheckFailed {
        url: String,
        status: Option<u16>,
        reason: String,
    },

    #[error("OpenCode HTTP client error: {0}")]
    Http(String),

    #[error("OpenCode SSE protocol error: {0}")]
    SseProtocol(String),

    #[error("Windows Job Object error: {0}")]
    JobObject(String),

    #[error("Sandbox policy violation: {0}")]
    SandboxPolicyViolation(String),

    #[error("Unsupported platform for requested containment: {0}")]
    UnsupportedPlatform(String),

    #[error("Invalid OpenCode event payload: {0}")]
    InvalidEventPayload(String),

    #[error("OpenCode session not found: {0}")]
    SessionNotFound(String),

    #[error("Invalid OpenCode configuration: {0}")]
    InvalidConfiguration(String),

    #[error("Worktree isolation violation: {0}")]
    WorktreeIsolationViolation(String),

    #[error("Upstream external runtime contract error: {0}")]
    ExternalRuntime(#[from] RuntimeError),
}

impl From<OpenCodeError> for RuntimeError {
    fn from(err: OpenCodeError) -> Self {
        match err {
            OpenCodeError::ExternalRuntime(rt_err) => rt_err,
            OpenCodeError::ProcessSpawn(msg) => RuntimeError::startup_failed(msg),
            OpenCodeError::ServerStartupTimeout { elapsed_ms, port } => {
                RuntimeError::startup_failed(format!(
                    "OpenCode server failed to start within timeout ({elapsed_ms}ms) on port {port}"
                ))
            }
            OpenCodeError::ServerExitUnexpected { exit_code, stderr } => {
                RuntimeError::startup_failed(format!(
                    "OpenCode server exited unexpectedly ({exit_code:?}): {stderr}"
                ))
            }
            OpenCodeError::HealthCheckFailed {
                url,
                status,
                reason,
            } => RuntimeError::startup_failed(format!(
                "OpenCode health check failed for {url} ({status:?}): {reason}"
            )),
            OpenCodeError::Http(msg)
            | OpenCodeError::SseProtocol(msg)
            | OpenCodeError::InvalidEventPayload(msg) => RuntimeError::transport_failed(msg),
            OpenCodeError::JobObject(msg) | OpenCodeError::SandboxPolicyViolation(msg) => {
                RuntimeError::StartupFailed {
                    reason: SanitizedRuntimeMessage::new(msg),
                }
            }
            OpenCodeError::UnsupportedPlatform(msg) => RuntimeError::StartupFailed {
                reason: SanitizedRuntimeMessage::new(format!(
                    "Unsupported platform containment: {msg}"
                )),
            },
            OpenCodeError::SessionNotFound(msg) => RuntimeError::InvalidConfiguration {
                reason: SanitizedRuntimeMessage::new(msg),
            },
            OpenCodeError::InvalidConfiguration(msg) => RuntimeError::invalid_configuration(msg),
            OpenCodeError::WorktreeIsolationViolation(msg) => {
                RuntimeError::invalid_workspace_access(msg)
            }
        }
    }
}
