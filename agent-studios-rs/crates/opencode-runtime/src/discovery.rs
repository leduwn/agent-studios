use agent_studios_external_runtime::{
    DiscoveredRuntimeInstance, RuntimeAvailability, RuntimeCapabilities, RuntimeCapabilitySupport,
    RuntimeConfigRef, RuntimeError, RuntimeImplementationId, RuntimeInstanceId,
    SanitizedRuntimeMessage,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, warn};

pub const OPENCODE_IMPLEMENTATION_ID: &str = "opencode";
pub const DEFAULT_INSTANCE_ID: &str = "rt-inst-00000000-0000-0000-0000-000000000001";

/// Default capability profile for the OpenCode runtime.
pub fn default_opencode_capabilities() -> RuntimeCapabilities {
    RuntimeCapabilities {
        streaming_events: RuntimeCapabilitySupport::Supported,
        interrupt: RuntimeCapabilitySupport::Supported,
        resume: RuntimeCapabilitySupport::Supported,
        stop: RuntimeCapabilitySupport::Supported,
        workspace_binding: RuntimeCapabilitySupport::Supported,
        tools: RuntimeCapabilitySupport::Supported,
        approvals: RuntimeCapabilitySupport::Supported,
        usage_reporting: RuntimeCapabilitySupport::Supported,
        model_selection: RuntimeCapabilitySupport::Supported,
        provider_selection: RuntimeCapabilitySupport::Supported,
        ..Default::default()
    }
}

/// Configuration options for discovering the OpenCode binary.
#[derive(Clone, Debug, Default)]
pub struct OpenCodeDiscoveryConfig {
    pub custom_binary_path: Option<PathBuf>,
}

/// Discovers OpenCode binary and interrogates its availability and version.
pub async fn discover_opencode_binary(
    config: &OpenCodeDiscoveryConfig,
) -> Result<Option<DiscoveredRuntimeInstance>, RuntimeError> {
    let resolved_path = resolve_binary_path(config);
    let impl_id = RuntimeImplementationId::new(OPENCODE_IMPLEMENTATION_ID)?;
    let inst_id = RuntimeInstanceId::from_uuid(uuid::Uuid::from_u128(1));

    let path = match resolved_path {
        Some(p) => p,
        None => {
            debug!("OpenCode binary not found in PATH or custom configuration");
            return Ok(None);
        }
    };

    match probe_binary_version(&path).await {
        Ok(version) => {
            let mut metadata = BTreeMap::new();
            metadata.insert("binary_path".to_string(), path.display().to_string());
            metadata.insert("version".to_string(), version.clone());

            Ok(Some(DiscoveredRuntimeInstance {
                implementation_id: impl_id,
                instance_id: inst_id,
                display_name: "OpenCode CLI".to_string(),
                version: Some(version),
                binary_path: Some(path),
                capabilities: default_opencode_capabilities(),
                supported_configs: vec![
                    RuntimeConfigRef::new("default")?,
                    RuntimeConfigRef::new("sandbox-strict")?,
                    RuntimeConfigRef::new("sandbox-standard")?,
                ],
                availability: RuntimeAvailability::Available,
                metadata,
            }))
        }
        Err(err) => {
            warn!(path = ?path, error = %err, "OpenCode binary probe failed");
            let mut metadata = BTreeMap::new();
            metadata.insert("binary_path".to_string(), path.display().to_string());

            Ok(Some(DiscoveredRuntimeInstance {
                implementation_id: impl_id,
                instance_id: inst_id,
                display_name: "OpenCode CLI".to_string(),
                version: None,
                binary_path: Some(path.clone()),
                capabilities: default_opencode_capabilities(),
                supported_configs: vec![RuntimeConfigRef::new("default")?],
                availability: RuntimeAvailability::Unavailable {
                    reason: SanitizedRuntimeMessage::new(format!(
                        "Failed to probe OpenCode binary at {}: {err}",
                        path.display()
                    )),
                },
                metadata,
            }))
        }
    }
}

fn resolve_binary_path(config: &OpenCodeDiscoveryConfig) -> Option<PathBuf> {
    if let Some(path) = config.custom_binary_path.as_ref().filter(|p| p.is_file()) {
        return Some(path.clone());
    }

    if let Ok(env_path) = std::env::var("OPENCODE_BIN") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
    }

    // Check system PATH
    let binary_names = if cfg!(windows) {
        vec!["opencode.exe", "opencode.cmd", "opencode.bat", "opencode"]
    } else {
        vec!["opencode"]
    };

    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for name in &binary_names {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

async fn probe_binary_version(path: &Path) -> Result<String, String> {
    let mut cmd = Command::new(path);
    cmd.arg("--version");

    let output = match tokio::time::timeout(Duration::from_secs(3), cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => return Err(format!("Spawn failed: {e}")),
        Err(_) => return Err("Version probe timed out after 3s".to_string()),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Exit code {}: {}", output.status, stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout.trim().to_string();
    if version.is_empty() {
        Ok("unknown".to_string())
    } else {
        Ok(version)
    }
}
