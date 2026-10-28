use agent_studios_external_runtime::{
    RuntimeCorrelation, RuntimeStartRequest, WorkspaceAccessMode,
};
use agent_studios_protocol::worktree::ExecutionWorkspace;
use std::path::PathBuf;

use crate::error::OpenCodeError;

/// Session context linking an OpenCode runtime session to Control Plane entities.
#[derive(Clone, Debug)]
pub struct OpenCodeSessionContext {
    pub correlation: RuntimeCorrelation,
    pub workspace: ExecutionWorkspace,
    pub access_mode: WorkspaceAccessMode,
    pub working_directory: PathBuf,
}

impl OpenCodeSessionContext {
    /// Extracts and validates session context from a RuntimeStartRequest.
    pub fn from_start_request(request: &RuntimeStartRequest) -> Result<Self, OpenCodeError> {
        let working_directory =
            validate_session_workspace(&request.workspace, request.workspace_access_mode)?;

        Ok(Self {
            correlation: request.correlation.clone().unwrap_or_default(),
            workspace: request.workspace.clone(),
            access_mode: request.workspace_access_mode,
            working_directory,
        })
    }
}

/// Validates that the workspace satisfies isolation invariants and returns the physical working directory path.
/// Enforces canonical path resolution and verifies that the working directory does not escape the managed worktree root.
pub fn validate_session_workspace(
    workspace: &ExecutionWorkspace,
    access_mode: WorkspaceAccessMode,
) -> Result<PathBuf, OpenCodeError> {
    match workspace {
        ExecutionWorkspace::Managed {
            cwd,
            root,
            source_root,
            ..
        } => {
            let canonical_root = root.canonicalize().map_err(|e| {
                OpenCodeError::WorktreeIsolationViolation(format!(
                    "Managed worktree root path cannot be resolved or does not exist: {}: {e}",
                    root.display()
                ))
            })?;

            if access_mode == WorkspaceAccessMode::Mutating {
                let same_path = match (root.canonicalize(), source_root.canonicalize()) {
                    (Ok(cr), Ok(cs)) => cr == cs,
                    _ => root == source_root,
                };
                if same_path {
                    return Err(OpenCodeError::WorktreeIsolationViolation(format!(
                        "Managed worktree root '{}' equals source root '{}' under Mutating access mode; physical worktree isolation required",
                        root.display(),
                        source_root.display()
                    )));
                }
            }

            let p = if cwd.as_os_str().is_empty() {
                canonical_root.clone()
            } else {
                let target = if cwd.is_absolute() {
                    cwd.clone()
                } else {
                    root.join(cwd)
                };
                target.canonicalize().map_err(|e| {
                    OpenCodeError::WorktreeIsolationViolation(format!(
                        "Managed worktree working directory cannot be resolved or does not exist: {}: {e}",
                        target.display()
                    ))
                })?
            };

            if !p.starts_with(&canonical_root) {
                return Err(OpenCodeError::WorktreeIsolationViolation(format!(
                    "Managed worktree cwd '{}' escapes managed root '{}'",
                    p.display(),
                    canonical_root.display()
                )));
            }

            if !p.is_dir() {
                return Err(OpenCodeError::WorktreeIsolationViolation(format!(
                    "Assigned worktree path is not a directory: {}",
                    p.display()
                )));
            }
            Ok(p)
        }
        ExecutionWorkspace::SharedSource { cwd } => {
            if access_mode == WorkspaceAccessMode::Mutating {
                return Err(OpenCodeError::WorktreeIsolationViolation(
                    "ExecutionWorkspace::SharedSource is strictly prohibited for Mutating access mode to protect repository integrity".into(),
                ));
            }
            let canonical_cwd = cwd.canonicalize().map_err(|e| {
                OpenCodeError::WorktreeIsolationViolation(format!(
                    "SharedSource cwd cannot be resolved or does not exist: {}: {e}",
                    cwd.display()
                ))
            })?;
            if !canonical_cwd.is_dir() {
                return Err(OpenCodeError::WorktreeIsolationViolation(format!(
                    "SharedSource path is not a directory: {}",
                    canonical_cwd.display()
                )));
            }
            Ok(canonical_cwd)
        }
    }
}
