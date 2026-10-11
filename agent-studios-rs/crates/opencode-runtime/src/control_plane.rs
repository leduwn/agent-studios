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
pub fn validate_session_workspace(
    workspace: &ExecutionWorkspace,
    _access_mode: WorkspaceAccessMode,
) -> Result<PathBuf, OpenCodeError> {
    match workspace {
        ExecutionWorkspace::Managed { cwd, root, .. } => {
            let p = if cwd.as_os_str().is_empty() {
                root.clone()
            } else {
                cwd.clone()
            };
            if !p.is_dir() {
                return Err(OpenCodeError::WorktreeIsolationViolation(format!(
                    "Assigned worktree path does not exist on disk: {}",
                    p.display()
                )));
            }
            Ok(p)
        }
        ExecutionWorkspace::SharedSource { .. } => {
            Err(OpenCodeError::WorktreeIsolationViolation(
                "ExecutionWorkspace::SharedSource is strictly prohibited for external runtimes to protect repository integrity".into(),
            ))
        }
    }
}
