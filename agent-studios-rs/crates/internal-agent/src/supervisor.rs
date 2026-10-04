use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use agent_studios_protocol::agent::AgentKind;
use agent_studios_protocol::cancellation::CancellationScope;
use agent_studios_protocol::id::{StudioId, TaskId};
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::TaskState;
use serde::{Deserialize, Serialize};

use crate::control_plane_actor::ControlPlaneHandle;
use crate::coordinator::{CoordinatorDecision, CoordinatorPlanValidator};
use crate::error::InternalAgentError;
use crate::executor::{AgentExecutionContext, AgentExecutor};
use crate::team::InternalTeamSpec;
use crate::workspace_policy::WorkspacePolicyArbitrator;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    #[default]
    FailFast,
    ContinueIndependent,
    RetryTask(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervisorExecutionSummary {
    pub studio_id: StudioId,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub cancelled_tasks: usize,
    pub coordinator_summary: Option<String>,
}

pub struct AgentStudiosSupervisor<E: AgentExecutor> {
    control_plane: ControlPlaneHandle,
    team_spec: InternalTeamSpec,
    workspace_arbitrator: WorkspacePolicyArbitrator,
    studio_id: StudioId,
    executor: Arc<E>,
    failure_policy: FailurePolicy,
    workspace_id: String,
}

impl<E: AgentExecutor> AgentStudiosSupervisor<E> {
    pub fn new(
        control_plane: ControlPlaneHandle,
        team_spec: InternalTeamSpec,
        workspace_arbitrator: WorkspacePolicyArbitrator,
        studio_id: StudioId,
        executor: E,
    ) -> Self {
        Self {
            control_plane,
            team_spec,
            workspace_arbitrator,
            studio_id,
            executor: Arc::new(executor),
            failure_policy: FailurePolicy::default(),
            workspace_id: format!("studio-{}", studio_id),
        }
    }

    pub fn with_failure_policy(mut self, policy: FailurePolicy) -> Self {
        self.failure_policy = policy;
        self
    }

    pub fn with_workspace_id(mut self, workspace_id: impl Into<String>) -> Self {
        self.workspace_id = workspace_id.into();
        self
    }

    pub fn control_plane(&self) -> &ControlPlaneHandle {
        &self.control_plane
    }

    pub fn team_spec(&self) -> &InternalTeamSpec {
        &self.team_spec
    }

    pub fn workspace_arbitrator(&self) -> &WorkspacePolicyArbitrator {
        &self.workspace_arbitrator
    }

    pub fn studio_id(&self) -> StudioId {
        self.studio_id
    }

    /// Step 1: Boot all agents into the ControlPlane
    pub async fn boot(&self) -> Result<(), InternalAgentError> {
        // Register coordinator
        self.control_plane
            .register_agent_with_id(
                self.studio_id,
                self.team_spec.coordinator.agent_id,
                &self.team_spec.coordinator.display_name,
                AgentKind::Internal,
                Some(self.team_spec.coordinator.role.clone()),
            )
            .await?;

        // Register workers
        for spec in self.team_spec.agents.values() {
            self.control_plane
                .register_agent_with_id(
                    self.studio_id,
                    spec.agent_id,
                    &spec.display_name,
                    AgentKind::Internal,
                    Some(spec.role.clone()),
                )
                .await?;
        }

        Ok(())
    }

    /// Step 2: Request execution plan from the Coordinator agent
    pub async fn plan(&self, objective: &str) -> Result<CoordinatorDecision, InternalAgentError> {
        let coordinator_spec = &self.team_spec.coordinator;
        let prompt = format!(
            "Objective: {}\nAvailable team members:\n{}",
            objective,
            self.format_team_description()
        );

        let context = AgentExecutionContext {
            studio_id: self.studio_id,
            task_id: None,
            agent_spec: coordinator_spec.clone(),
            prompt,
            budget: coordinator_spec.budget.clone(),
        };

        let result = self.executor.execute_agent(context).await?;
        if !result.success {
            return Err(InternalAgentError::ExecutionFailed {
                agent_id: coordinator_spec.agent_id,
                error: format!("Coordinator failed to plan: {}", result.output),
            });
        }

        // Parse JSON decision
        match serde_json::from_str::<CoordinatorDecision>(&result.output) {
            Ok(decision) => Ok(decision),
            Err(_) => Err(InternalAgentError::InvalidPlan(format!(
                "Failed to parse CoordinatorDecision JSON from output: {}",
                result.output
            ))),
        }
    }

    fn format_team_description(&self) -> String {
        let mut desc = format!(
            "- coordinator: {} (Role: {})\n",
            self.team_spec.coordinator.display_name, self.team_spec.coordinator.role
        );
        for (alias, spec) in &self.team_spec.agents {
            desc.push_str(&format!(
                "- {}: {} (Role: {}, Access: {:?})\n",
                alias, spec.display_name, spec.role, spec.workspace_access
            ));
        }
        desc
    }

    /// Runs the complete supervisor workflow: boot -> plan -> materialize -> schedule -> review
    pub async fn run(
        &self,
        objective: &str,
    ) -> Result<SupervisorExecutionSummary, InternalAgentError> {
        self.boot().await?;

        let decision = self.plan(objective).await?;

        let tasks = match decision {
            CoordinatorDecision::Plan { tasks } => tasks,
            CoordinatorDecision::Complete { summary } => {
                return Ok(SupervisorExecutionSummary {
                    studio_id: self.studio_id,
                    total_tasks: 0,
                    completed_tasks: 0,
                    failed_tasks: 0,
                    cancelled_tasks: 0,
                    coordinator_summary: Some(summary),
                });
            }
            CoordinatorDecision::Fail { reason } => {
                return Err(InternalAgentError::ExecutionFailed {
                    agent_id: self.team_spec.coordinator.agent_id,
                    error: reason,
                });
            }
        };

        let key_to_id = CoordinatorPlanValidator::materialize(
            &self.control_plane,
            self.studio_id,
            &tasks,
            &self.team_spec,
        )
        .await?;

        let total_tasks = key_to_id.len();
        let mut retries_by_task: HashMap<TaskId, u32> = HashMap::new();

        // Step 4: Schedule loop
        loop {
            let ready_tasks = self.control_plane.get_ready_tasks(self.studio_id).await?;

            if !ready_tasks.is_empty() {
                for task in ready_tasks {
                    let agent_id = match task.assigned_agent_id {
                        Some(id) => id,
                        None => {
                            self.control_plane
                                .transition_task_state(task.id, TaskState::Failed)
                                .await?;
                            continue;
                        }
                    };

                    let (_alias, agent_spec) = match self.team_spec.get_agent_by_id(agent_id) {
                        Some(pair) => pair,
                        None => {
                            self.control_plane
                                .transition_task_state(task.id, TaskState::Failed)
                                .await?;
                            continue;
                        }
                    };

                    // Acquire workspace lease
                    let lease_result = self.workspace_arbitrator.try_acquire(
                        &self.workspace_id,
                        agent_id,
                        agent_spec.workspace_access,
                    );

                    let mut lease = match lease_result {
                        Ok(l) => l,
                        Err(InternalAgentError::WorkspaceConflict { .. }) => {
                            // Workspace busy, wait for next tick
                            continue;
                        }
                        Err(err) => return Err(err),
                    };

                    // Create and start run
                    let run = self.control_plane.create_run(task.id, agent_id).await?;
                    self.control_plane
                        .transition_run_state(run.id, RunState::Starting)
                        .await?;
                    self.control_plane
                        .transition_run_state(run.id, RunState::Running)
                        .await?;
                    self.control_plane
                        .transition_task_state(task.id, TaskState::Running)
                        .await?;

                    let context = AgentExecutionContext {
                        studio_id: self.studio_id,
                        task_id: Some(task.id),
                        agent_spec: agent_spec.clone(),
                        prompt: task.title.clone(),
                        budget: agent_spec.budget.clone(),
                    };

                    let exec_result = self.executor.execute_agent(context).await;

                    // Release lease immediately after turn execution
                    lease.release();

                    match exec_result {
                        Ok(res) if res.success => {
                            self.control_plane
                                .transition_run_state(run.id, RunState::Succeeded)
                                .await?;
                            self.control_plane
                                .transition_task_state(task.id, TaskState::Succeeded)
                                .await?;
                        }
                        Ok(_) | Err(_) => {
                            let should_retry = match self.failure_policy {
                                FailurePolicy::RetryTask(max_retries) => {
                                    let current = retries_by_task.entry(task.id).or_insert(0);
                                    if *current < max_retries {
                                        *current += 1;
                                        true
                                    } else {
                                        false
                                    }
                                }
                                _ => false,
                            };

                            if should_retry {
                                self.control_plane
                                    .transition_run_state(run.id, RunState::Failed)
                                    .await?;
                                self.control_plane
                                    .transition_task_state(task.id, TaskState::Ready)
                                    .await?;
                            } else {
                                self.control_plane
                                    .transition_run_state(run.id, RunState::Failed)
                                    .await?;
                                self.control_plane
                                    .transition_task_state(task.id, TaskState::Failed)
                                    .await?;

                                if self.failure_policy == FailurePolicy::FailFast {
                                    self.control_plane
                                        .request_cancellation(
                                            CancellationScope::Studio(self.studio_id),
                                            Some("Task failed under FailFast policy".to_string()),
                                        )
                                        .await?;
                                }
                            }
                        }
                    }
                }
            }

            // Check termination status
            let studio_tasks = self.control_plane.get_studio_tasks(self.studio_id).await?;

            let pending_or_blocked_count = studio_tasks
                .iter()
                .filter(|t| t.state == TaskState::Pending || t.state == TaskState::Blocked)
                .count();
            let running_count = studio_tasks
                .iter()
                .filter(|t| t.state == TaskState::Running)
                .count();

            if running_count == 0 {
                let ready_count = self
                    .control_plane
                    .get_ready_tasks(self.studio_id)
                    .await?
                    .len();
                if ready_count == 0 {
                    if pending_or_blocked_count > 0 {
                        // Remaining blocked tasks cannot proceed due to failed/cancelled prerequisites
                        for t in studio_tasks {
                            if t.state == TaskState::Blocked || t.state == TaskState::Pending {
                                self.control_plane
                                    .transition_task_state(t.id, TaskState::Cancelled)
                                    .await?;
                            }
                        }
                    }
                    break;
                }
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Final state counts
        let studio_tasks = self.control_plane.get_studio_tasks(self.studio_id).await?;

        let completed_tasks = studio_tasks
            .iter()
            .filter(|t| t.state == TaskState::Succeeded)
            .count();
        let failed_tasks = studio_tasks
            .iter()
            .filter(|t| t.state == TaskState::Failed)
            .count();
        let cancelled_tasks = studio_tasks
            .iter()
            .filter(|t| t.state == TaskState::Cancelled)
            .count();

        // Final coordinator review
        let coordinator_summary = if failed_tasks == 0 && cancelled_tasks == 0 {
            Some(format!(
                "Successfully completed all {} tasks for studio {}",
                completed_tasks, self.studio_id
            ))
        } else {
            Some(format!(
                "Studio {} finished with {} succeeded, {} failed, {} cancelled",
                self.studio_id, completed_tasks, failed_tasks, cancelled_tasks
            ))
        };

        Ok(SupervisorExecutionSummary {
            studio_id: self.studio_id,
            total_tasks,
            completed_tasks,
            failed_tasks,
            cancelled_tasks,
            coordinator_summary,
        })
    }
}
