//! Deterministic Lifecycle Hooks Engine (`pre_turn`, `post_task`, `on_reconciliation`, `on_approval`)

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use agent_studios_ipc_protocol::SecretRedactor;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleHookPhase {
    PreTurn,
    PostTask,
    OnReconciliation,
    OnApproval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookScope {
    Global,
    Project,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HookPolicy {
    #[default]
    FailClosed,
    WarnOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookSpec {
    pub id: String,
    pub phase: LifecycleHookPhase,
    pub scope: HookScope,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub policy: HookPolicy,
    #[serde(default = "default_hook_enabled")]
    pub enabled: bool,
}

fn default_timeout_ms() -> u64 {
    10_000
}

fn default_hook_enabled() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookExecutionResult {
    pub hook_id: String,
    pub phase: LifecycleHookPhase,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub succeeded: bool,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HookError {
    #[error("Hook '{hook_id}' failed closed (exit code {exit_code:?}): {message}")]
    FailedClosed {
        hook_id: String,
        exit_code: Option<i32>,
        message: String,
    },
    #[error("Hook '{hook_id}' timed out after {timeout_ms}ms")]
    TimedOut { hook_id: String, timeout_ms: u64 },
    #[error("Hook execution error for '{hook_id}': {message}")]
    Io { hook_id: String, message: String },
}

/// Executes a single hook deterministically inside `working_dir` with timeout & secret scrubbing.
pub async fn execute_hook(
    spec: &HookSpec,
    working_dir: &Path,
) -> Result<HookExecutionResult, HookError> {
    if !spec.enabled {
        return Ok(HookExecutionResult {
            hook_id: spec.id.clone(),
            phase: spec.phase,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            succeeded: true,
        });
    }

    let timeout_ms = spec.timeout_ms.max(10);
    let mut cmd = Command::new(&spec.command);
    cmd.args(&spec.args)
        .current_dir(working_dir)
        .env("AGENT_STUDIOS_HOOK_PHASE", format!("{:?}", spec.phase))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        // Prevent opening transient console windows and ensure child inherits kill semantics
        cmd.kill_on_drop(true);
    }
    #[cfg(not(windows))]
    {
        cmd.kill_on_drop(true);
    }

    let child = cmd.spawn().map_err(|e| HookError::Io {
        hook_id: spec.id.clone(),
        message: format!("Failed to spawn hook command '{}': {e}", spec.command),
    })?;

    let waited =
        tokio::time::timeout(Duration::from_millis(timeout_ms), child.wait_with_output()).await;

    match waited {
        Ok(Ok(out)) => {
            let redactor = SecretRedactor::new();
            let stdout = redactor.redact_text(&String::from_utf8_lossy(&out.stdout));
            let stderr = redactor.redact_text(&String::from_utf8_lossy(&out.stderr));
            let code = out.status.code();
            let succeeded = out.status.success();

            if !succeeded && spec.policy == HookPolicy::FailClosed {
                return Err(HookError::FailedClosed {
                    hook_id: spec.id.clone(),
                    exit_code: code,
                    message: stderr.clone(),
                });
            }

            Ok(HookExecutionResult {
                hook_id: spec.id.clone(),
                phase: spec.phase,
                exit_code: code,
                stdout,
                stderr,
                timed_out: false,
                succeeded,
            })
        }
        Ok(Err(e)) => Err(HookError::Io {
            hook_id: spec.id.clone(),
            message: e.to_string(),
        }),
        Err(_) => {
            if spec.policy == HookPolicy::FailClosed {
                Err(HookError::TimedOut {
                    hook_id: spec.id.clone(),
                    timeout_ms,
                })
            } else {
                Ok(HookExecutionResult {
                    hook_id: spec.id.clone(),
                    phase: spec.phase,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("Hook timed out after {timeout_ms}ms (warn-only policy)"),
                    timed_out: true,
                    succeeded: false,
                })
            }
        }
    }
}

/// Deterministically executes all enabled hooks matching `phase` in declaration order.
pub async fn execute_phase_hooks(
    hooks: &[HookSpec],
    phase: LifecycleHookPhase,
    working_dir: &Path,
) -> Result<Vec<HookExecutionResult>, HookError> {
    let mut results = Vec::new();
    for spec in hooks.iter().filter(|h| h.phase == phase && h.enabled) {
        results.push(execute_hook(spec, working_dir).await?);
    }
    Ok(results)
}
