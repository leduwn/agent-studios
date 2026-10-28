use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, warn};

use agent_studios_external_runtime::{
    DiscoveredRuntimeInstance, RuntimeAvailability, RuntimeCapabilities, RuntimeCapabilitySupport,
    RuntimeConfigRef, RuntimeError, RuntimeImplementationId, RuntimeInstanceId,
    SanitizedRuntimeMessage,
};

/// Canonical implementation identifier for Claude Code in Agent Studios.
pub const CLAUDE_CODE_IMPLEMENTATION_ID: &str = "anthropic:claude-code";

/// Runtime ID conforming to `RuntimeImplementationId` character constraints `[a-zA-Z0-9_-]`.
pub const CLAUDE_CODE_RUNTIME_ID: &str = "claude-code";

/// Default instance identifier name.
pub const DEFAULT_INSTANCE_ID: &str = "default";

/// Default capability profile declared by the Claude Code CLI runtime.
pub fn default_claude_capabilities() -> RuntimeCapabilities {
    RuntimeCapabilities {
        streaming_events: RuntimeCapabilitySupport::Supported,
        interrupt: RuntimeCapabilitySupport::Supported,
        resume: RuntimeCapabilitySupport::Supported,
        stop: RuntimeCapabilitySupport::Supported,
        // The Claude Code CLI is executed with `--no-session-persistence` in headless
        // automation mode, and disk-persisted session reload across process restarts is
        // not supported. In-process turn continuation (`resume`) remains supported.
        persistent_session: RuntimeCapabilitySupport::Unsupported,
        workspace_binding: RuntimeCapabilitySupport::Supported,
        model_selection: RuntimeCapabilitySupport::Supported,
        provider_selection: RuntimeCapabilitySupport::Unsupported,
        tools: RuntimeCapabilitySupport::Supported,
        approvals: RuntimeCapabilitySupport::Supported,
        mcp: RuntimeCapabilitySupport::Supported,
        skills: RuntimeCapabilitySupport::Supported,
        image_input: RuntimeCapabilitySupport::Supported,
        structured_output: RuntimeCapabilitySupport::Supported,
        subagents: RuntimeCapabilitySupport::Supported,
        usage_reporting: RuntimeCapabilitySupport::Supported,
    }
}

/// Configuration options for discovering the Claude Code executable.
#[derive(Clone, Debug, Default)]
pub struct ClaudeCodeDiscoveryConfig {
    pub custom_binary_path: Option<PathBuf>,
}

impl ClaudeCodeDiscoveryConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_custom_binary_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.custom_binary_path = Some(path.into());
        self
    }
}

/// Discovers the local Claude Code CLI binary and queries its availability and version.
pub async fn discover_claude_binary(
    config: &ClaudeCodeDiscoveryConfig,
) -> Result<Option<DiscoveredRuntimeInstance>, RuntimeError> {
    let resolved_path = resolve_binary_path(config);
    let impl_id = RuntimeImplementationId::new(CLAUDE_CODE_RUNTIME_ID)?;
    let inst_id = RuntimeInstanceId::from_uuid(uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_DNS,
        DEFAULT_INSTANCE_ID.as_bytes(),
    ));

    let path = match resolved_path {
        Some(p) => p,
        None => {
            debug!("Claude Code binary not found in PATH or standard user directories");
            return Ok(None);
        }
    };

    match probe_binary_version(&path).await {
        Ok(version) => {
            let mut metadata = BTreeMap::new();
            metadata.insert("binary_path".to_string(), path.display().to_string());
            metadata.insert("version".to_string(), version.clone());
            metadata.insert(
                "implementation_id".to_string(),
                CLAUDE_CODE_IMPLEMENTATION_ID.to_string(),
            );

            Ok(Some(DiscoveredRuntimeInstance {
                implementation_id: impl_id,
                instance_id: inst_id,
                display_name: "Claude Code CLI".to_string(),
                version: Some(version),
                binary_path: Some(path),
                capabilities: default_claude_capabilities(),
                supported_configs: vec![
                    RuntimeConfigRef::new("default")?,
                    RuntimeConfigRef::new("sandbox-standard")?,
                    RuntimeConfigRef::new("sandbox-strict")?,
                ],
                availability: RuntimeAvailability::Available,
                metadata,
            }))
        }
        Err(err) => {
            warn!(path = ?path, error = %err, "Claude Code version probe failed");
            let mut metadata = BTreeMap::new();
            metadata.insert("binary_path".to_string(), path.display().to_string());
            metadata.insert(
                "implementation_id".to_string(),
                CLAUDE_CODE_IMPLEMENTATION_ID.to_string(),
            );

            Ok(Some(DiscoveredRuntimeInstance {
                implementation_id: impl_id,
                instance_id: inst_id,
                display_name: "Claude Code CLI".to_string(),
                version: None,
                binary_path: Some(path.clone()),
                capabilities: default_claude_capabilities(),
                supported_configs: vec![RuntimeConfigRef::new("default")?],
                availability: RuntimeAvailability::Unavailable {
                    reason: SanitizedRuntimeMessage::new(format!(
                        "Failed to probe Claude Code binary at {}: {err}",
                        path.display()
                    )),
                },
                metadata,
            }))
        }
    }
}

/// Resolves the candidate binary path using custom overrides, environment variables,
/// standard user directories, and system PATH.
pub fn resolve_binary_path(config: &ClaudeCodeDiscoveryConfig) -> Option<PathBuf> {
    if let Some(path) = config.custom_binary_path.as_ref() {
        if path.is_file() {
            let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            return Some(canonical);
        } else {
            return None;
        }
    }

    for env_key in &["CLAUDE_CODE_BIN", "CLAUDE_BIN"] {
        if let Ok(env_path) = std::env::var(env_key) {
            let p = PathBuf::from(env_path);
            if p.is_file() {
                let canonical = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
                return Some(canonical);
            }
        }
    }

    // Standard user locations
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        let candidate = PathBuf::from(&userprofile)
            .join(".local")
            .join("bin")
            .join("claude.exe");
        if candidate.is_file() {
            let canonical = std::fs::canonicalize(&candidate).unwrap_or(candidate);
            return Some(canonical);
        }
    }

    if let Ok(appdata) = std::env::var("APPDATA") {
        for name in &["claude.cmd", "claude.exe", "claude.bat"] {
            let candidate = PathBuf::from(&appdata).join("npm").join(name);
            if candidate.is_file() {
                let canonical = std::fs::canonicalize(&candidate).unwrap_or(candidate);
                return Some(canonical);
            }
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        for dir in &[".local/bin", "bin", ".npm-global/bin"] {
            let candidate = PathBuf::from(&home).join(dir).join("claude");
            if candidate.is_file() {
                let canonical = std::fs::canonicalize(&candidate).unwrap_or(candidate);
                return Some(canonical);
            }
        }
    }

    // Standard Unix PATH prefixes
    for prefix in &[
        "/usr/local/bin/claude",
        "/usr/bin/claude",
        "/opt/homebrew/bin/claude",
    ] {
        let candidate = PathBuf::from(prefix);
        if candidate.is_file() {
            let canonical = std::fs::canonicalize(&candidate).unwrap_or(candidate);
            return Some(canonical);
        }
    }

    // System PATH search
    let binary_names = if cfg!(windows) {
        vec!["claude.exe", "claude.cmd", "claude.bat", "claude"]
    } else {
        vec!["claude"]
    };

    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for name in &binary_names {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    let canonical =
                        std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
                    return Some(canonical);
                }
            }
        }
    }

    None
}

/// Probes the Claude Code executable with `--version` and returns the sanitized version string.
pub async fn probe_binary_version(path: &Path) -> Result<String, String> {
    let mut cmd = Command::new(path);
    cmd.arg("--version");

    let output = match tokio::time::timeout(Duration::from_secs(5), cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => return Err(format!("Spawn failed: {e}")),
        Err(_) => return Err("Version probe timed out after 5s".to_string()),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let trimmed_stderr = if stderr.len() > 512 {
            &stderr[..512]
        } else {
            stderr.trim()
        };
        return Err(format!("Exit code {}: {}", output.status, trimmed_stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or("").trim();
    let version = if first_line.len() > 128 {
        first_line[..128].trim().to_string()
    } else {
        first_line.to_string()
    };

    if version.is_empty() {
        Ok("unknown".to_string())
    } else {
        Ok(version)
    }
}
