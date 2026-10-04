use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_studios_protocol::id::AgentId;

use crate::error::InternalAgentError;
use crate::profile::WorkspaceAccessMode;

#[derive(Debug)]
enum WorkspaceSlot {
    Mutating { agent_id: AgentId },
    ReadOnly { active: Vec<AgentId> },
}

#[derive(Default)]
struct ArbitratorState {
    workspaces: HashMap<String, WorkspaceSlot>,
}

#[derive(Clone, Default)]
pub struct WorkspacePolicyArbitrator {
    state: Arc<Mutex<ArbitratorState>>,
}

impl WorkspacePolicyArbitrator {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ArbitratorState::default())),
        }
    }

    pub fn try_acquire(
        &self,
        workspace_id: impl Into<String>,
        agent_id: AgentId,
        mode: WorkspaceAccessMode,
    ) -> Result<WorkspaceLease, InternalAgentError> {
        let workspace_id = workspace_id.into();
        let mut state = self.state.lock().unwrap();

        match state.workspaces.get_mut(&workspace_id) {
            None => {
                match mode {
                    WorkspaceAccessMode::Mutating => {
                        state
                            .workspaces
                            .insert(workspace_id.clone(), WorkspaceSlot::Mutating { agent_id });
                    }
                    WorkspaceAccessMode::ReadOnly => {
                        state.workspaces.insert(
                            workspace_id.clone(),
                            WorkspaceSlot::ReadOnly {
                                active: vec![agent_id],
                            },
                        );
                    }
                }
                Ok(WorkspaceLease {
                    workspace_id,
                    agent_id,
                    mode,
                    state: Arc::clone(&self.state),
                    active: true,
                })
            }
            Some(WorkspaceSlot::Mutating {
                agent_id: active_agent,
            }) => Err(InternalAgentError::WorkspaceConflict {
                workspace_id,
                active_agent: *active_agent,
                requested_agent: agent_id,
            }),
            Some(WorkspaceSlot::ReadOnly { active }) => match mode {
                WorkspaceAccessMode::ReadOnly => {
                    active.push(agent_id);
                    Ok(WorkspaceLease {
                        workspace_id,
                        agent_id,
                        mode,
                        state: Arc::clone(&self.state),
                        active: true,
                    })
                }
                WorkspaceAccessMode::Mutating => {
                    let active_agent = active.first().copied().unwrap_or(agent_id);
                    Err(InternalAgentError::WorkspaceConflict {
                        workspace_id,
                        active_agent,
                        requested_agent: agent_id,
                    })
                }
            },
        }
    }

    pub async fn acquire(
        &self,
        workspace_id: impl Into<String>,
        agent_id: AgentId,
        mode: WorkspaceAccessMode,
        timeout: Duration,
    ) -> Result<WorkspaceLease, InternalAgentError> {
        let workspace_id = workspace_id.into();
        let start = tokio::time::Instant::now();
        let sleep_step = Duration::from_millis(25);

        loop {
            match self.try_acquire(&workspace_id, agent_id, mode) {
                Ok(lease) => return Ok(lease),
                Err(InternalAgentError::WorkspaceConflict { .. }) => {
                    if start.elapsed() >= timeout {
                        // Return final attempt error
                        return self.try_acquire(&workspace_id, agent_id, mode);
                    }
                    tokio::time::sleep(sleep_step).await;
                }
                Err(other) => return Err(other),
            }
        }
    }

    pub fn is_locked(&self, workspace_id: &str) -> bool {
        let state = self.state.lock().unwrap();
        state.workspaces.contains_key(workspace_id)
    }

    pub fn active_agents(&self, workspace_id: &str) -> Vec<AgentId> {
        let state = self.state.lock().unwrap();
        match state.workspaces.get(workspace_id) {
            None => Vec::new(),
            Some(WorkspaceSlot::Mutating { agent_id }) => vec![*agent_id],
            Some(WorkspaceSlot::ReadOnly { active }) => active.clone(),
        }
    }
}

pub struct WorkspaceLease {
    workspace_id: String,
    agent_id: AgentId,
    mode: WorkspaceAccessMode,
    state: Arc<Mutex<ArbitratorState>>,
    active: bool,
}

impl WorkspaceLease {
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    pub fn mode(&self) -> WorkspaceAccessMode {
        self.mode
    }

    pub fn release(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        let mut state = self.state.lock().unwrap();
        if let Some(slot) = state.workspaces.get_mut(&self.workspace_id) {
            match slot {
                WorkspaceSlot::Mutating { agent_id } => {
                    if *agent_id == self.agent_id {
                        state.workspaces.remove(&self.workspace_id);
                    }
                }
                WorkspaceSlot::ReadOnly { active } => {
                    if let Some(pos) = active.iter().position(|id| *id == self.agent_id) {
                        active.remove(pos);
                    }
                    if active.is_empty() {
                        state.workspaces.remove(&self.workspace_id);
                    }
                }
            }
        }
    }
}

impl Drop for WorkspaceLease {
    fn drop(&mut self) {
        self.release();
    }
}
