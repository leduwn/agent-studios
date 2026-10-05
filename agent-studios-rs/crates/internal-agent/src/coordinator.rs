use std::collections::{HashMap, HashSet, VecDeque};

use agent_studios_protocol::id::{StudioId, TaskId};
use agent_studios_protocol::task::BatchTaskSpec;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::control_plane_actor::ControlPlaneHandle;
use crate::error::InternalAgentError;
use crate::profile::WorkspaceAccessMode;
use crate::team::InternalTeamSpec;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlannedTask {
    pub task_key: String,
    pub title: String,
    pub description: Option<String>,
    pub assigned_alias: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub workspace_access: Option<WorkspaceAccessMode>,
    #[serde(default)]
    pub priority: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum CoordinatorDecision {
    Plan { tasks: Vec<PlannedTask> },
    Complete { summary: String },
    Fail { reason: String },
}

pub struct CoordinatorPlanValidator;

impl CoordinatorPlanValidator {
    /// Validates plan structure and returns the topologically sorted task keys if valid.
    pub fn validate(
        tasks: &[PlannedTask],
        team: &InternalTeamSpec,
    ) -> Result<Vec<String>, InternalAgentError> {
        if tasks.is_empty() {
            return Err(InternalAgentError::InvalidPlan(
                "Plan must contain at least one task".to_string(),
            ));
        }

        let mut keys = HashSet::new();
        for task in tasks {
            if task.task_key.trim().is_empty() {
                return Err(InternalAgentError::InvalidPlan(
                    "Task key cannot be empty".to_string(),
                ));
            }
            if !keys.insert(&task.task_key) {
                return Err(InternalAgentError::InvalidPlan(format!(
                    "Duplicate task key: {}",
                    task.task_key
                )));
            }

            if team.get_agent(&task.assigned_alias).is_none() {
                return Err(InternalAgentError::AgentAliasNotFound(
                    task.assigned_alias.clone(),
                ));
            }
        }

        // Validate dependencies exist and self-dependency is rejected
        for task in tasks {
            for dep in &task.depends_on {
                if dep == &task.task_key {
                    return Err(InternalAgentError::CyclicPlanDependency {
                        task_key: task.task_key.clone(),
                    });
                }
                if !keys.contains(dep) {
                    return Err(InternalAgentError::MissingTaskDependency {
                        task_key: task.task_key.clone(),
                        depends_on: dep.clone(),
                    });
                }
            }
        }

        // Topological sort (Kahn's algorithm)
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

        for task in tasks {
            in_degree.insert(&task.task_key, task.depends_on.len());
            for dep in &task.depends_on {
                dependents
                    .entry(dep.as_str())
                    .or_default()
                    .push(task.task_key.as_str());
            }
        }

        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(&key, _)| key)
            .collect();

        let mut sorted = Vec::with_capacity(tasks.len());

        while let Some(key) = queue.pop_front() {
            sorted.push(key.to_string());
            if let Some(deps) = dependents.get(key) {
                for &dependent in deps {
                    if let Some(deg) = in_degree.get_mut(dependent) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(dependent);
                        }
                    }
                }
            }
        }

        if sorted.len() != tasks.len() {
            // Find task involved in cycle
            let cycle_task = in_degree
                .into_iter()
                .find(|(_, deg)| *deg > 0)
                .map(|(key, _)| key.to_string())
                .unwrap_or_else(|| "unknown".to_string());

            return Err(InternalAgentError::CyclicPlanDependency {
                task_key: cycle_task,
            });
        }

        Ok(sorted)
    }

    /// Atomically materializes a validated plan into the ControlPlane in a single transaction.
    pub async fn materialize(
        handle: &ControlPlaneHandle,
        studio_id: StudioId,
        tasks: &[PlannedTask],
        team: &InternalTeamSpec,
    ) -> Result<HashMap<String, TaskId>, InternalAgentError> {
        let sorted_keys = Self::validate(tasks, team)?;

        let mut task_map: HashMap<&str, &PlannedTask> = HashMap::new();
        for task in tasks {
            task_map.insert(&task.task_key, task);
        }

        let mut batch_specs = Vec::with_capacity(sorted_keys.len());
        for key in &sorted_keys {
            let planned = task_map[key.as_str()];
            let agent_spec = team.get_agent(&planned.assigned_alias).ok_or_else(|| {
                InternalAgentError::AgentAliasNotFound(planned.assigned_alias.clone())
            })?;
            batch_specs.push(BatchTaskSpec {
                key: key.clone(),
                title: planned.title.clone(),
                description: planned.description.clone().unwrap_or_default(),
                assigned_agent_id: Some(agent_spec.agent_id),
                parent_task_key: None,
                parent_task_id: None,
                dependency_keys: planned.depends_on.clone(),
                dependency_task_ids: Vec::new(),
            });
        }

        let created_records = handle.create_task_batch(studio_id, batch_specs).await?;
        let mut key_to_id = HashMap::with_capacity(sorted_keys.len());
        for (idx, key) in sorted_keys.into_iter().enumerate() {
            if let Some(record) = created_records.get(idx) {
                key_to_id.insert(key, record.id);
            }
        }

        Ok(key_to_id)
    }
}
