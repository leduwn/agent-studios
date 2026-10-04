use std::collections::HashMap;

use agent_studios_protocol::id::AgentId;
use serde::{Deserialize, Serialize};

use crate::error::InternalAgentError;
use crate::profile::InternalAgentSpec;

pub const COORDINATOR_ALIAS: &str = "coordinator";

/// Validates that an alias conforms to `^[a-zA-Z0-9_-]{1,64}$`.
pub fn validate_alias(alias: &str) -> Result<(), InternalAgentError> {
    if alias.is_empty() || alias.len() > 64 {
        return Err(InternalAgentError::InvalidAgentAlias(alias.to_string()));
    }
    for c in alias.chars() {
        if !c.is_ascii_alphanumeric() && c != '_' && c != '-' {
            return Err(InternalAgentError::InvalidAgentAlias(alias.to_string()));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalTeamSpec {
    pub team_id: String,
    pub coordinator: InternalAgentSpec,
    pub agents: HashMap<String, InternalAgentSpec>,
}

impl InternalTeamSpec {
    pub fn new(team_id: impl Into<String>, coordinator: InternalAgentSpec) -> Self {
        Self {
            team_id: team_id.into(),
            coordinator,
            agents: HashMap::new(),
        }
    }

    pub fn add_agent(
        mut self,
        alias: impl Into<String>,
        spec: InternalAgentSpec,
    ) -> Result<Self, InternalAgentError> {
        let alias = alias.into();
        validate_alias(&alias)?;

        if alias == COORDINATOR_ALIAS {
            return Err(InternalAgentError::DuplicateAgentAlias(format!(
                "Alias '{COORDINATOR_ALIAS}' is reserved for the team coordinator"
            )));
        }

        if self.agents.contains_key(&alias) {
            return Err(InternalAgentError::DuplicateAgentAlias(alias));
        }

        // Also check if any existing agent has the same AgentId
        if self.coordinator.agent_id == spec.agent_id {
            return Err(InternalAgentError::DuplicateAgentAlias(format!(
                "Worker agent '{alias}' shares AgentId with coordinator"
            )));
        }
        for (existing_alias, existing_spec) in &self.agents {
            if existing_spec.agent_id == spec.agent_id {
                return Err(InternalAgentError::DuplicateAgentAlias(format!(
                    "Worker agent '{alias}' shares AgentId with worker '{existing_alias}'"
                )));
            }
        }

        self.agents.insert(alias, spec);
        Ok(self)
    }

    pub fn get_agent(&self, alias: &str) -> Option<&InternalAgentSpec> {
        if alias == COORDINATOR_ALIAS {
            Some(&self.coordinator)
        } else {
            self.agents.get(alias)
        }
    }

    pub fn get_agent_by_id(&self, id: AgentId) -> Option<(&str, &InternalAgentSpec)> {
        if self.coordinator.agent_id == id {
            Some((COORDINATOR_ALIAS, &self.coordinator))
        } else {
            self.agents
                .iter()
                .find(|(_, spec)| spec.agent_id == id)
                .map(|(alias, spec)| (alias.as_str(), spec))
        }
    }

    pub fn len(&self) -> usize {
        self.agents.len() + 1 // Coordinator + workers
    }

    pub fn is_empty(&self) -> bool {
        false // Always has coordinator
    }
}
