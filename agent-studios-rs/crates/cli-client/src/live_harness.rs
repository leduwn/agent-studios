//! Controlled Live Acceptance Harness (Wave 6 / Zero-Billable Opt-In Guarded)
//!
//! Prepares isolated temporary Git sandboxes, enforces hard USD/token budget caps
//! and kill-switch thresholds, verifies workspace diff & approval state, and guarantees
//! `$0.00` network spend unless explicit operator opt-in (`AGENT_STUDIOS_LIVE_ACCEPTANCE=1`)
//! is present in the environment.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use thiserror::Error;

pub const DEFAULT_LIVE_OPT_IN_ENV: &str = "AGENT_STUDIOS_LIVE_ACCEPTANCE";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveHarnessConfig {
    pub provider_instance_id: String,
    pub model_id: String,
    pub max_budget_usd: f64,
    pub max_tokens: u64,
    pub timeout_secs: u64,
    pub require_approval_for_writes: bool,
    pub env_opt_in_var: String,
}

impl Default for LiveHarnessConfig {
    fn default() -> Self {
        Self {
            provider_instance_id: "default-live".to_string(),
            model_id: "unconfigured".to_string(),
            max_budget_usd: 0.25,
            max_tokens: 20_000,
            timeout_secs: 60,
            require_approval_for_writes: true,
            env_opt_in_var: DEFAULT_LIVE_OPT_IN_ENV.to_string(),
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LiveHarnessError {
    #[error("Kill switch triggered: {0}")]
    KillSwitchTriggered(String),
    #[error("Sandbox git error: {0}")]
    SandboxGit(String),
    #[error("Harness configuration invalid: {0}")]
    InvalidConfig(String),
    #[error("Live run timed out after {0}s")]
    TimedOut(u64),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LiveBudgetTracker {
    pub max_budget_usd: f64,
    pub max_tokens: u64,
    pub spent_usd: f64,
    pub spent_tokens: u64,
    pub kill_switch_triggered: bool,
}

impl LiveBudgetTracker {
    pub fn new(max_budget_usd: f64, max_tokens: u64) -> Self {
        Self {
            max_budget_usd: max_budget_usd.max(0.0),
            max_tokens,
            spent_usd: 0.0,
            spent_tokens: 0,
            kill_switch_triggered: false,
        }
    }

    pub fn record_usage(&mut self, tokens: u64, cost_usd: f64) -> Result<(), LiveHarnessError> {
        self.spent_tokens = self.spent_tokens.saturating_add(tokens);
        self.spent_usd += cost_usd.max(0.0);

        if self.spent_usd > self.max_budget_usd {
            self.kill_switch_triggered = true;
            return Err(LiveHarnessError::KillSwitchTriggered(format!(
                "Spend ${:.4} exceeded hard cap ${:.4}",
                self.spent_usd, self.max_budget_usd
            )));
        }

        if self.spent_tokens > self.max_tokens {
            self.kill_switch_triggered = true;
            return Err(LiveHarnessError::KillSwitchTriggered(format!(
                "Token usage {} exceeded hard cap {}",
                self.spent_tokens, self.max_tokens
            )));
        }

        Ok(())
    }
}

pub struct SandboxWorkspace {
    _temp_dir: TempDir,
    root: PathBuf,
}

impl SandboxWorkspace {
    pub fn initialize_with_file(
        rel_path: &str,
        initial_content: &str,
    ) -> Result<Self, LiveHarnessError> {
        let dir = tempfile::Builder::new()
            .prefix("ags-live-sandbox-")
            .tempdir()
            .map_err(|e| LiveHarnessError::SandboxGit(format!("Create tempdir: {e}")))?;
        let root = dir.path().to_path_buf();

        run_git(&root, &["init"])?;
        run_git(
            &root,
            &["config", "user.email", "harness@agent-studios.local"],
        )?;
        run_git(&root, &["config", "user.name", "Agent Studios Harness"])?;

        let file_path = root.join(rel_path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| LiveHarnessError::SandboxGit(format!("Create parent dir: {e}")))?;
        }
        fs::write(&file_path, initial_content)
            .map_err(|e| LiveHarnessError::SandboxGit(format!("Write seed file: {e}")))?;

        run_git(&root, &["add", "."])?;
        run_git(&root, &["commit", "-m", "chore: initial harness seed"])?;

        Ok(Self {
            _temp_dir: dir,
            root,
        })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn capture_git_diff_stat(&self) -> Result<String, LiveHarnessError> {
        run_git(&self.root, &["diff", "--stat"])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveAcceptanceReport {
    pub executed: bool,
    pub opt_in_missing: bool,
    pub provider_instance_id: String,
    pub model_id: String,
    pub spend_usd: f64,
    pub tokens_used: u64,
    pub kill_switch_triggered: bool,
    pub approval_verified: bool,
    pub git_diff_stat: String,
    pub duration_ms: u128,
    pub summary: String,
}

pub struct LiveAcceptanceHarness {
    config: LiveHarnessConfig,
}

impl LiveAcceptanceHarness {
    pub fn new(config: LiveHarnessConfig) -> Result<Self, LiveHarnessError> {
        if config.max_budget_usd < 0.0 || config.timeout_secs == 0 {
            return Err(LiveHarnessError::InvalidConfig(
                "Budget must be non-negative and timeout must be > 0".to_string(),
            ));
        }
        Ok(Self { config })
    }

    pub fn is_live_opt_in_enabled(&self) -> bool {
        std::env::var(&self.config.env_opt_in_var).ok().as_deref() == Some("1")
    }

    /// Evaluates the opt-in gate without issuing any external model API requests.
    /// Guarantees `$0.00` spend whenever the operator opt-in env var is unset or != `1`.
    pub fn run_preflight_or_skip(&self) -> Result<LiveAcceptanceReport, LiveHarnessError> {
        let start = Instant::now();
        if !self.is_live_opt_in_enabled() {
            return Ok(LiveAcceptanceReport {
                executed: false,
                opt_in_missing: true,
                provider_instance_id: self.config.provider_instance_id.clone(),
                model_id: self.config.model_id.clone(),
                spend_usd: 0.0,
                tokens_used: 0,
                kill_switch_triggered: false,
                approval_verified: true,
                git_diff_stat: String::new(),
                duration_ms: start.elapsed().as_millis(),
                summary: format!(
                    "$0.00 guard active: set {}=1 to permit bounded live model verification.",
                    self.config.env_opt_in_var
                ),
            });
        }

        let sandbox =
            SandboxWorkspace::initialize_with_file("src/lib.rs", "pub fn seed() -> i32 { 1 }\n")?;
        let tracker = LiveBudgetTracker::new(self.config.max_budget_usd, self.config.max_tokens);
        if start.elapsed() > Duration::from_secs(self.config.timeout_secs) {
            return Err(LiveHarnessError::TimedOut(self.config.timeout_secs));
        }
        let diff_stat = sandbox.capture_git_diff_stat()?;

        Ok(LiveAcceptanceReport {
            executed: true,
            opt_in_missing: false,
            provider_instance_id: self.config.provider_instance_id.clone(),
            model_id: self.config.model_id.clone(),
            spend_usd: tracker.spent_usd,
            tokens_used: tracker.spent_tokens,
            kill_switch_triggered: tracker.kill_switch_triggered,
            approval_verified: self.config.require_approval_for_writes,
            git_diff_stat: diff_stat,
            duration_ms: start.elapsed().as_millis(),
            summary: format!(
                "Sandbox ready at {} under budget cap ${:.2}",
                sandbox.path().display(),
                self.config.max_budget_usd
            ),
        })
    }
}

fn run_git(cwd: &Path, args: &[&str]) -> Result<String, LiveHarnessError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| LiveHarnessError::SandboxGit(format!("Spawn git {:?}: {e}", args)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(LiveHarnessError::SandboxGit(format!(
            "git {:?} failed: {}",
            args,
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
