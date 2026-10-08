use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent_studios_protocol::agent::AgentKind;
use agent_studios_protocol::cancellation::CancellationScope;
use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId, WorktreeId};
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::{DependencyOutputPolicy, TaskState};
use agent_studios_protocol::worktree::{ExecutionWorkspace, IntegrationWorkspace, WorktreeState};
use agent_studios_workspace::{ReconciliationOutcome, WorkspaceOrchestrator};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;

use crate::control_plane_actor::ControlPlaneHandle;
use crate::coordinator::{CoordinatorDecision, CoordinatorPlanValidator};
use crate::error::InternalAgentError;
use crate::executor::{AgentExecutionContext, AgentExecutionResult, AgentExecutor};
use crate::profile::WorkspaceAccessMode;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinatorReviewStatus {
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervisorExecutionSummary {
    pub studio_id: StudioId,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub cancelled_tasks: usize,
    pub blocked_tasks: usize,
    pub coordinator_summary: Option<String>,
    pub coordinator_review_status: CoordinatorReviewStatus,
}

struct TaskCompletion {
    task_id: TaskId,
    run_id: RunId,
    agent_id: AgentId,
    workspace: ExecutionWorkspace,
    result: Result<AgentExecutionResult, InternalAgentError>,
}

pub struct AgentStudiosSupervisor<E: AgentExecutor> {
    control_plane: ControlPlaneHandle,
    team_spec: InternalTeamSpec,
    workspace_arbitrator: WorkspacePolicyArbitrator,
    studio_id: StudioId,
    executor: Arc<E>,
    failure_policy: FailurePolicy,
    workspace_id: String,
    workspace_orchestrator: Option<Arc<WorkspaceOrchestrator>>,
    repo_path: Option<PathBuf>,
    base_commit: Option<String>,
    integration_workspace: Option<IntegrationWorkspace>,
}

impl<E: AgentExecutor + 'static> AgentStudiosSupervisor<E> {
    pub fn new(
        control_plane: ControlPlaneHandle,
        team_spec: InternalTeamSpec,
        workspace_arbitrator: WorkspacePolicyArbitrator,
        executor: E,
    ) -> Self {
        let studio_id = team_spec.studio_id;
        Self {
            control_plane,
            team_spec,
            workspace_arbitrator,
            studio_id,
            executor: Arc::new(executor),
            failure_policy: FailurePolicy::default(),
            workspace_id: format!("studio-{}", studio_id),
            workspace_orchestrator: None,
            repo_path: None,
            base_commit: None,
            integration_workspace: None,
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

    pub fn with_workspace_orchestrator(
        mut self,
        orchestrator: Arc<WorkspaceOrchestrator>,
        repo_path: impl Into<PathBuf>,
        base_commit: impl Into<String>,
    ) -> Self {
        self.workspace_orchestrator = Some(orchestrator);
        self.repo_path = Some(repo_path.into());
        self.base_commit = Some(base_commit.into());
        self
    }

    pub fn with_integration_workspace(mut self, workspace: IntegrationWorkspace) -> Self {
        self.integration_workspace = Some(workspace);
        self
    }

    pub fn with_integration_worktree(mut self, path: impl Into<PathBuf>) -> Self {
        let p = path.into();
        self.integration_workspace = Some(IntegrationWorkspace::new(
            WorktreeId::new(),
            p.clone(),
            p,
            self.base_commit.clone().unwrap_or_default(),
        ));
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

    pub fn max_parallel_agents(&self) -> Option<usize> {
        self.team_spec.max_parallel_agents
    }

    pub fn executor(&self) -> &Arc<E> {
        &self.executor
    }

    /// Step 1: Boot all agents into the ControlPlane
    pub async fn boot(&self) -> Result<(), InternalAgentError> {
        self.team_spec.validate()?;

        self.control_plane.get_studio(self.studio_id).await?;

        self.executor
            .validate_agent_spec(&self.team_spec.coordinator)?;

        let mut sorted_aliases: Vec<&String> = self.team_spec.agents.keys().collect();
        sorted_aliases.sort();

        for alias in &sorted_aliases {
            let spec = &self.team_spec.agents[*alias];
            self.executor.validate_agent_spec(spec)?;
        }

        let mut batch_specs = Vec::with_capacity(self.team_spec.agents.len() + 1);
        batch_specs.push(agent_studios_protocol::agent::BatchAgentSpec {
            id: self.team_spec.coordinator.agent_id,
            display_name: self.team_spec.coordinator.display_name.clone(),
            kind: AgentKind::Internal,
            role: Some(self.team_spec.coordinator.role.clone()),
        });

        for alias in sorted_aliases {
            let spec = &self.team_spec.agents[alias];
            batch_specs.push(agent_studios_protocol::agent::BatchAgentSpec {
                id: spec.agent_id,
                display_name: spec.display_name.clone(),
                kind: AgentKind::Internal,
                role: Some(spec.role.clone()),
            });
        }

        self.control_plane
            .register_agent_batch(self.studio_id, batch_specs)
            .await?;

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

        let schema = schemars::schema_for!(CoordinatorDecision);
        let schema_value = serde_json::to_value(&schema).ok();

        let context = AgentExecutionContext {
            studio_id: self.studio_id,
            task_id: None,
            run_id: None,
            parent_agent_id: None,
            agent_spec: coordinator_spec.clone(),
            prompt,
            budget: coordinator_spec.budget.clone(),
            output_schema: schema_value,
            execution_workspace: ExecutionWorkspace::shared_source(
                self.repo_path.clone().unwrap_or_default(),
            ),
        };

        let result = self.executor.execute_agent(context).await?;
        if !result.success {
            return Err(InternalAgentError::ExecutionFailed {
                agent_id: coordinator_spec.agent_id,
                error: format!("Coordinator failed to plan: {}", result.output),
            });
        }

        let trimmed = result.output.trim();
        match serde_json::from_str::<CoordinatorDecision>(trimmed) {
            Ok(decision) => Ok(decision),
            Err(_) => {
                let total_len = result.output.len();
                let preview: String = result.output.chars().take(512).collect();
                Err(InternalAgentError::InvalidPlan(format!(
                    "Failed to parse CoordinatorDecision JSON from output (total {total_len} bytes): {preview}"
                )))
            }
        }
    }

    fn format_team_description(&self) -> String {
        let mut desc = format!(
            "- coordinator: {} (Role: {})\n",
            self.team_spec.coordinator.display_name, self.team_spec.coordinator.role
        );
        let mut sorted_aliases: Vec<&String> = self.team_spec.agents.keys().collect();
        sorted_aliases.sort();
        for alias in sorted_aliases {
            let spec = &self.team_spec.agents[alias];
            desc.push_str(&format!(
                "- {}: {} (Role: {}, Access: {:?})\n",
                alias, spec.display_name, spec.role, spec.workspace_access
            ));
        }
        desc
    }

    /// Runs the complete supervisor workflow: boot -> plan -> materialize -> schedule -> review
    #[allow(clippy::collapsible_if)]
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
                    blocked_tasks: 0,
                    coordinator_summary: Some(summary),
                    coordinator_review_status: CoordinatorReviewStatus::Skipped,
                });
            }
            CoordinatorDecision::Fail { reason } => {
                return Err(InternalAgentError::ExecutionFailed {
                    agent_id: self.team_spec.coordinator.agent_id,
                    error: reason,
                });
            }
        };

        let mut planned_priorities: HashMap<String, i32> = HashMap::with_capacity(tasks.len());
        for p in &tasks {
            planned_priorities.insert(p.task_key.clone(), p.priority.unwrap_or(0));
        }

        let key_to_id = CoordinatorPlanValidator::materialize(
            &self.control_plane,
            self.studio_id,
            &tasks,
            &self.team_spec,
        )
        .await?;

        let total_tasks = key_to_id.len();
        let mut task_priorities: HashMap<TaskId, i32> = HashMap::with_capacity(total_tasks);
        for (key, id) in &key_to_id {
            let prio = planned_priorities.get(key).copied().unwrap_or(0);
            task_priorities.insert(*id, prio);
        }

        let mut retries_by_task: HashMap<TaskId, u32> = HashMap::new();
        let mut delayed_retries: Vec<(TaskId, tokio::time::Instant)> = Vec::new();
        let mut join_set: JoinSet<TaskCompletion> = JoinSet::new();
        let mut active_agents: HashSet<AgentId> = HashSet::new();
        let mut active_task_ids: HashSet<TaskId> = HashSet::new();
        let mut cancelled = false;

        let mut integration_workspace = self.integration_workspace.clone();
        if let Some(ref mut int_ws) = integration_workspace {
            let exists = self
                .control_plane
                .get_worktree(int_ws.worktree_id)
                .await?
                .is_some();
            if !exists {
                let repo_cwd = self
                    .repo_path
                    .clone()
                    .unwrap_or_else(|| int_ws.root.clone());
                let source_root = if let Some(ref orchestrator) = self.workspace_orchestrator {
                    orchestrator
                        .resolve_repository_root_async(repo_cwd.clone())
                        .await?
                } else {
                    repo_cwd.clone()
                };
                let wt = self
                    .control_plane
                    .create_worktree_with_cwds(
                        self.studio_id,
                        "integration-worktree",
                        source_root,
                        repo_cwd,
                        int_ws.root.clone(),
                        int_ws.cwd.clone(),
                        int_ws.base_sha.clone(),
                    )
                    .await?;
                self.control_plane
                    .transition_worktree_state(wt.id, WorktreeState::Ready)
                    .await?;
                int_ws.worktree_id = wt.id;
            }
        }

        loop {
            // Re-queue expired delayed retries
            let now = tokio::time::Instant::now();
            let mut remaining_delayed = Vec::new();
            for (task_id, ready_at) in delayed_retries {
                if now >= ready_at {
                    let _ = self
                        .control_plane
                        .transition_task_state(task_id, TaskState::Ready)
                        .await;
                } else {
                    remaining_delayed.push((task_id, ready_at));
                }
            }
            delayed_retries = remaining_delayed;

            let team_cap = self.team_spec.max_parallel_agents.unwrap_or(usize::MAX);

            // If not cancelled, schedule new ready tasks up to concurrency capacity
            if !cancelled && join_set.len() < team_cap {
                let mut ready_tasks = self.control_plane.get_ready_tasks(self.studio_id).await?;

                // Sort candidates deterministically: (priority desc, created_at asc, id asc)
                ready_tasks.sort_by(|a, b| {
                    let prio_a = task_priorities.get(&a.id).copied().unwrap_or(0);
                    let prio_b = task_priorities.get(&b.id).copied().unwrap_or(0);
                    prio_b
                        .cmp(&prio_a)
                        .then_with(|| a.created_at.cmp(&b.created_at))
                        .then_with(|| a.id.cmp(&b.id))
                });

                for task in ready_tasks {
                    if join_set.len() >= team_cap {
                        break;
                    }

                    if active_task_ids.contains(&task.id) {
                        continue;
                    }

                    let agent_id = match task.assigned_agent_id {
                        Some(id) => id,
                        None => {
                            let _ = self
                                .control_plane
                                .transition_task_state(task.id, TaskState::Failed)
                                .await;
                            continue;
                        }
                    };

                    // Agent must not already be busy
                    if active_agents.contains(&agent_id) {
                        continue;
                    }

                    let (_alias, agent_spec) = match self.team_spec.get_agent_by_id(agent_id) {
                        Some(pair) => pair,
                        None => {
                            let _ = self
                                .control_plane
                                .transition_task_state(task.id, TaskState::Failed)
                                .await;
                            continue;
                        }
                    };

                    let access_mode = agent_spec.workspace_access;
                    let (workspace, lease_workspace_id) = match (
                        access_mode,
                        &self.workspace_orchestrator,
                        &self.repo_path,
                        &self.base_commit,
                    ) {
                        (
                            WorkspaceAccessMode::Mutating,
                            Some(orchestrator),
                            Some(repo_path),
                            Some(base_commit),
                        ) => {
                            let base_commit_to_use = if task.dependency_output_policy
                                == DependencyOutputPolicy::ReconciledOutput
                            {
                                if let Some(ref int_ws) = integration_workspace {
                                    orchestrator
                                        .resolve_head_commit_async(int_ws.root.clone())
                                        .await
                                        .map_err(|e| InternalAgentError::ExecutionFailed {
                                            agent_id,
                                            error: format!(
                                                "Failed to resolve integration workspace HEAD SHA: {e}"
                                            ),
                                        })?
                                } else {
                                    base_commit.clone()
                                }
                            } else {
                                base_commit.clone()
                            };

                            let managed = orchestrator
                                .create_worktree_async(
                                    repo_path.clone(),
                                    Some(base_commit_to_use.clone()),
                                )
                                .await
                                .map_err(|e| InternalAgentError::ExecutionFailed {
                                    agent_id,
                                    error: format!("Failed to create managed worktree: {e}"),
                                })?;

                            let wt_rec = self
                                .control_plane
                                .create_worktree_with_cwds(
                                    self.studio_id,
                                    format!("wt-task-{}", task.id),
                                    managed.source_root.clone(),
                                    managed.source_cwd.clone(),
                                    managed.root.clone(),
                                    managed.cwd.clone(),
                                    base_commit_to_use.clone(),
                                )
                                .await?;

                            self.control_plane
                                .transition_worktree_state(wt_rec.id, WorktreeState::Ready)
                                .await?;

                            let execution_ws = ExecutionWorkspace::managed(
                                wt_rec.id,
                                managed.root.clone(),
                                managed.cwd.clone(),
                                managed.source_root.clone(),
                                managed.source_cwd.clone(),
                                base_commit_to_use,
                            );

                            (execution_ws, format!("worktree-{}", wt_rec.id))
                        }
                        _ => (
                            ExecutionWorkspace::shared_source(
                                self.repo_path.clone().unwrap_or_default(),
                            ),
                            self.workspace_id.clone(),
                        ),
                    };

                    // Acquire workspace lease
                    let lease_result = self.workspace_arbitrator.try_acquire(
                        &lease_workspace_id,
                        agent_id,
                        agent_spec.workspace_access,
                    );

                    let lease = match lease_result {
                        Ok(l) => l,
                        Err(InternalAgentError::WorkspaceConflict { .. }) => {
                            // Workspace busy, clean up newly created worktree if any
                            if let Some(wt_id) = workspace.worktree_id() {
                                if let (Some(orchestrator), Some(repo_path), Some(root)) = (
                                    &self.workspace_orchestrator,
                                    &self.repo_path,
                                    workspace.root(),
                                ) {
                                    let release_res = orchestrator
                                        .release_worktree_async(
                                            repo_path.clone(),
                                            root.to_path_buf(),
                                            false,
                                            None,
                                        )
                                        .await;
                                    if release_res.is_ok() {
                                        self.control_plane
                                            .complete_worktree_removal(wt_id, None)
                                            .await?;
                                    } else {
                                        self.control_plane
                                            .record_worktree_retained(
                                                wt_id,
                                                Some(
                                                    "Physical worktree removal failed".to_string(),
                                                ),
                                            )
                                            .await?;
                                    }
                                }
                            }
                            continue;
                        }
                        Err(err) => return Err(err),
                    };

                    // Create run and transition state
                    let run = self.control_plane.create_run(task.id, agent_id).await?;
                    if let Some(wt_id) = workspace.worktree_id() {
                        self.control_plane
                            .assign_worktree(wt_id, task.id, agent_id, Some(run.id))
                            .await?;
                        self.control_plane
                            .transition_worktree_state(wt_id, WorktreeState::InUse)
                            .await?;
                    }

                    self.control_plane
                        .transition_run_state(run.id, RunState::Starting)
                        .await?;
                    self.control_plane
                        .transition_run_state(run.id, RunState::Running)
                        .await?;
                    self.control_plane
                        .transition_task_state(task.id, TaskState::Running)
                        .await?;

                    let executor = Arc::clone(&self.executor);
                    let context = AgentExecutionContext {
                        studio_id: self.studio_id,
                        task_id: Some(task.id),
                        run_id: Some(run.id),
                        parent_agent_id: Some(self.team_spec.coordinator.agent_id),
                        agent_spec: agent_spec.clone(),
                        prompt: task.title.clone(),
                        budget: agent_spec.budget.clone(),
                        output_schema: None,
                        execution_workspace: workspace.clone(),
                    };

                    active_agents.insert(agent_id);
                    active_task_ids.insert(task.id);

                    join_set.spawn(async move {
                        let _lease = lease; // held for execution duration
                        let result = executor.execute_agent(context).await;
                        TaskCompletion {
                            task_id: task.id,
                            run_id: run.id,
                            agent_id,
                            workspace,
                            result,
                        }
                    });
                }
            }

            // Check if all work is done
            if join_set.is_empty() {
                let studio_tasks = self.control_plane.get_studio_tasks(self.studio_id).await?;
                let non_terminal = studio_tasks.iter().any(|t| !t.state.is_terminal());

                if !non_terminal && delayed_retries.is_empty() {
                    break;
                }

                let ready_count = self
                    .control_plane
                    .get_ready_tasks(self.studio_id)
                    .await?
                    .len();

                if ready_count == 0 && delayed_retries.is_empty() {
                    if cancelled {
                        // Clean up remaining blocked or pending tasks on explicit/FailFast cancellation
                        for t in studio_tasks {
                            if matches!(
                                t.state,
                                TaskState::Blocked | TaskState::Pending | TaskState::Retrying
                            ) {
                                let _ = self
                                    .control_plane
                                    .transition_task_state(t.id, TaskState::Cancelled)
                                    .await;
                            }
                        }
                    }
                    break;
                }

                tokio::time::sleep(Duration::from_millis(20)).await;
                continue;
            }

            // Await next completing task
            tokio::select! {
                Some(join_res) = join_set.join_next() => {
                    match join_res {
                        Ok(completion) => {
                            active_agents.remove(&completion.agent_id);
                            active_task_ids.remove(&completion.task_id);

                            if cancelled {
                                if let Some(wt_id) = completion.workspace.worktree_id() {
                                    if let (Some(orchestrator), Some(repo_path), Some(root)) = (
                                        &self.workspace_orchestrator,
                                        &self.repo_path,
                                        completion.workspace.root(),
                                    ) {
                                        let reason = "Retained on task cancellation".to_string();
                                        orchestrator
                                            .release_worktree_async(
                                                repo_path.clone(),
                                                root.to_path_buf(),
                                                true,
                                                Some(reason.clone()),
                                            )
                                            .await
                                            .map_err(|e| {
                                                InternalAgentError::WorkspacePostprocessError(
                                                    e.to_string(),
                                                )
                                            })?;
                                        self.control_plane
                                            .record_worktree_retained(wt_id, Some(reason))
                                            .await
                                            .map_err(|e| {
                                                InternalAgentError::WorkspacePostprocessError(
                                                    e.to_string(),
                                                )
                                            })?;
                                    }
                                }
                                self.control_plane
                                    .transition_run_state(completion.run_id, RunState::Cancelled)
                                    .await
                                    .map_err(|e| {
                                        InternalAgentError::WorkspacePostprocessError(e.to_string())
                                    })?;
                                self.control_plane
                                    .transition_task_state(completion.task_id, TaskState::Cancelled)
                                    .await
                                    .map_err(|e| {
                                        InternalAgentError::WorkspacePostprocessError(e.to_string())
                                    })?;
                            } else {
                                match completion.result {
                                    Ok(res) if res.success => {
                                        if let Some(wt_id) = completion.workspace.worktree_id() {
                                            let orchestrator = self
                                                .workspace_orchestrator
                                                .as_ref()
                                                .ok_or_else(|| {
                                                    InternalAgentError::WorkspacePostprocessError(
                                                        "Missing workspace orchestrator for managed workspace"
                                                            .to_string(),
                                                    )
                                                })?;
                                            let repo_path = self
                                                .repo_path
                                                .as_ref()
                                                .ok_or_else(|| {
                                                    InternalAgentError::WorkspacePostprocessError(
                                                        "Missing repo path for managed workspace"
                                                            .to_string(),
                                                    )
                                                })?;
                                            let base_sha = completion
                                                .workspace
                                                .base_sha()
                                                .ok_or_else(|| {
                                                    InternalAgentError::WorkspacePostprocessError(
                                                        "Missing base SHA for managed workspace"
                                                            .to_string(),
                                                    )
                                                })?;
                                            let root = completion
                                                .workspace
                                                .root()
                                                .ok_or_else(|| {
                                                    InternalAgentError::WorkspacePostprocessError(
                                                        "Missing root for managed workspace"
                                                            .to_string(),
                                                    )
                                                })?;

                                            let capture_res = orchestrator
                                                .capture_changes_async(
                                                    root.to_path_buf(),
                                                    base_sha.to_string(),
                                                )
                                                .await;

                                            match capture_res {
                                                Ok(captured) => {
                                                    if captured.files_changed > 0 {
                                                        let patch_record = orchestrator
                                                            .store_patch_artifact(
                                                                self.studio_id,
                                                                completion.task_id,
                                                                completion.agent_id,
                                                                Some(completion.run_id),
                                                                Some(wt_id),
                                                                &captured,
                                                            )
                                                            .map_err(|e| {
                                                                InternalAgentError::WorkspacePostprocessError(
                                                                    format!(
                                                                        "Failed to store patch artifact: {e}"
                                                                    ),
                                                                )
                                                            })?;

                                                        self.control_plane
                                                            .register_artifact_record(
                                                                patch_record.clone(),
                                                            )
                                                            .await
                                                            .map_err(|e| {
                                                                InternalAgentError::WorkspacePostprocessError(
                                                                    format!(
                                                                        "Failed to register patch artifact: {e}"
                                                                    ),
                                                                )
                                                            })?;

                                                        self.control_plane
                                                            .record_worktree_change_captured(
                                                                wt_id,
                                                                Some(completion.run_id),
                                                                base_sha.to_string(),
                                                                None,
                                                                patch_record.id,
                                                                None,
                                                                captured.files_changed,
                                                            )
                                                            .await
                                                            .map_err(|e| {
                                                                InternalAgentError::WorkspacePostprocessError(
                                                                    format!(
                                                                        "Failed to record change captured: {e}"
                                                                    ),
                                                                )
                                                            })?;

                                                        if let Some(ref int_ws) =
                                                            integration_workspace
                                                        {
                                                            let recon = self
                                                                .control_plane
                                                                .create_reconciliation_with_target(
                                                                    self.studio_id,
                                                                    wt_id,
                                                                    int_ws.worktree_id,
                                                                    completion.task_id,
                                                                    Some(completion.run_id),
                                                                    patch_record.id,
                                                                    int_ws.root.clone(),
                                                                    base_sha.to_string(),
                                                                )
                                                                .await
                                                                .map_err(|e| {
                                                                    InternalAgentError::WorkspacePostprocessError(
                                                                        format!(
                                                                            "Failed to create reconciliation: {e}"
                                                                        ),
                                                                    )
                                                                })?;

                                                            self.control_plane
                                                                .start_reconciliation(recon.id)
                                                                .await
                                                                .map_err(|e| {
                                                                    InternalAgentError::WorkspacePostprocessError(
                                                                        format!(
                                                                            "Failed to start reconciliation checking: {e}"
                                                                        ),
                                                                    )
                                                                })?;

                                                            let check_res = orchestrator
                                                                .check_patch_async(
                                                                    int_ws.root.clone(),
                                                                    captured
                                                                        .patch_bytes
                                                                        .clone(),
                                                                )
                                                                .await
                                                                .map_err(|e| {
                                                                    InternalAgentError::WorkspacePostprocessError(
                                                                        format!(
                                                                            "Patch check execution error: {e}"
                                                                        ),
                                                                    )
                                                                })?;

                                                            match check_res {
                                                                Some(
                                                                    ReconciliationOutcome::Conflicted {
                                                                        conflicted_files,
                                                                        reason,
                                                                    },
                                                                ) => {
                                                                    self.control_plane
                                                                        .record_reconciliation_conflict(
                                                                            recon.id,
                                                                            conflicted_files,
                                                                            reason,
                                                                        )
                                                                        .await
                                                                        .map_err(|e| {
                                                                            InternalAgentError::WorkspacePostprocessError(
                                                                                format!(
                                                                                    "Failed to record reconciliation conflict: {e}"
                                                                                ),
                                                                            )
                                                                        })?;
                                                                }
                                                                Some(
                                                                    ReconciliationOutcome::Failed {
                                                                        error,
                                                                    },
                                                                ) => {
                                                                    self.control_plane
                                                                        .record_reconciliation_failed(
                                                                            recon.id,
                                                                            error,
                                                                        )
                                                                        .await
                                                                        .map_err(|e| {
                                                                            InternalAgentError::WorkspacePostprocessError(
                                                                                format!(
                                                                                    "Failed to record reconciliation failure: {e}"
                                                                                ),
                                                                            )
                                                                        })?;
                                                                }
                                                                Some(
                                                                    ReconciliationOutcome::Applied {
                                                                        ..
                                                                    },
                                                                ) => {
                                                                    self.control_plane
                                                                        .record_reconciliation_failed(
                                                                            recon.id,
                                                                            "Unexpected applied outcome during patch dry-run check".to_string(),
                                                                        )
                                                                        .await
                                                                        .map_err(|e| {
                                                                            InternalAgentError::WorkspacePostprocessError(
                                                                                format!(
                                                                                    "Failed to record reconciliation failure: {e}"
                                                                                ),
                                                                            )
                                                                        })?;
                                                                }
                                                                None => {
                                                                    self.control_plane
                                                                        .mark_reconciliation_applying(
                                                                            recon.id,
                                                                        )
                                                                        .await
                                                                        .map_err(|e| {
                                                                            InternalAgentError::WorkspacePostprocessError(
                                                                                format!(
                                                                                    "Failed to mark reconciliation applying: {e}"
                                                                                ),
                                                                            )
                                                                        })?;

                                                                    let commit_msg = format!(
                                                                        "Reconcile task {}",
                                                                        completion.task_id
                                                                    );
                                                                    let apply_res = orchestrator
                                                                        .apply_patch_async(
                                                                            int_ws.root.clone(),
                                                                            captured
                                                                                .patch_bytes
                                                                                .clone(),
                                                                            Some(commit_msg),
                                                                        )
                                                                        .await
                                                                        .map_err(|e| {
                                                                            InternalAgentError::WorkspacePostprocessError(
                                                                                format!(
                                                                                    "Patch apply execution error: {e}"
                                                                                ),
                                                                            )
                                                                        })?;

                                                                    match apply_res {
                                                                        ReconciliationOutcome::Applied {
                                                                            merge_commit,
                                                                        } => {
                                                                            self.control_plane
                                                                                .record_reconciliation_applied(
                                                                                    recon.id,
                                                                                    merge_commit,
                                                                                )
                                                                                .await
                                                                                .map_err(|e| {
                                                                                    InternalAgentError::WorkspacePostprocessError(
                                                                                        format!(
                                                                                            "Failed to record reconciliation applied: {e}"
                                                                                        ),
                                                                                    )
                                                                                })?;
                                                                        }
                                                                        ReconciliationOutcome::Conflicted {
                                                                            conflicted_files,
                                                                            reason,
                                                                        } => {
                                                                            self.control_plane
                                                                                .record_reconciliation_conflict(
                                                                                    recon.id,
                                                                                    conflicted_files,
                                                                                    reason,
                                                                                )
                                                                                .await
                                                                                .map_err(|e| {
                                                                                    InternalAgentError::WorkspacePostprocessError(
                                                                                        format!(
                                                                                            "Failed to record reconciliation conflict: {e}"
                                                                                        ),
                                                                                    )
                                                                                })?;
                                                                        }
                                                                        ReconciliationOutcome::Failed {
                                                                            error,
                                                                        } => {
                                                                            self.control_plane
                                                                                .record_reconciliation_failed(
                                                                                    recon.id,
                                                                                    error,
                                                                                )
                                                                                .await
                                                                                .map_err(|e| {
                                                                                    InternalAgentError::WorkspacePostprocessError(
                                                                                        format!(
                                                                                            "Failed to record reconciliation failure: {e}"
                                                                                        ),
                                                                                    )
                                                                                })?;
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }

                                                        let retain_reason =
                                                            "Mutating task worktree retained on disk".to_string();
                                                        orchestrator
                                                            .release_worktree_async(
                                                                completion
                                                                    .workspace
                                                                    .source_cwd()
                                                                    .map(std::path::Path::to_path_buf)
                                                                    .unwrap_or_else(|| repo_path.clone()),
                                                                root.to_path_buf(),
                                                                true,
                                                                Some(retain_reason.clone()),
                                                            )
                                                            .await
                                                            .map_err(|e| {
                                                                InternalAgentError::WorkspacePostprocessError(
                                                                    format!(
                                                                        "Failed to retain worktree directory: {e}"
                                                                    ),
                                                                )
                                                            })?;
                                                        self.control_plane
                                                            .record_worktree_retained(
                                                                wt_id,
                                                                Some(retain_reason),
                                                            )
                                                            .await
                                                            .map_err(|e| {
                                                                InternalAgentError::WorkspacePostprocessError(
                                                                    format!(
                                                                        "Failed to record worktree retained: {e}"
                                                                    ),
                                                                )
                                                            })?;
                                                    } else {
                                                        self.control_plane
                                                            .record_worktree_no_changes(
                                                                wt_id,
                                                                Some(completion.run_id),
                                                                base_sha.to_string(),
                                                            )
                                                            .await
                                                            .map_err(|e| {
                                                                InternalAgentError::WorkspacePostprocessError(
                                                                    format!(
                                                                        "Failed to record no changes: {e}"
                                                                    ),
                                                                )
                                                            })?;
                                                        orchestrator
                                                            .release_worktree_async(
                                                                repo_path.clone(),
                                                                root.to_path_buf(),
                                                                true,
                                                                Some(
                                                                    "no changes captured"
                                                                        .to_string(),
                                                                ),
                                                            )
                                                            .await
                                                            .map_err(|e| {
                                                                InternalAgentError::WorkspacePostprocessError(
                                                                    format!(
                                                                        "Failed to retain unchanged worktree: {e}"
                                                                    ),
                                                                )
                                                            })?;
                                                    }
                                                }
                                                Err(e) => {
                                                    let reason =
                                                        format!("Failed to capture changes: {e}");
                                                    orchestrator
                                                        .release_worktree_async(
                                                            repo_path.clone(),
                                                            root.to_path_buf(),
                                                            true,
                                                            Some(reason.clone()),
                                                        )
                                                        .await
                                                        .map_err(|err| {
                                                            InternalAgentError::WorkspacePostprocessError(
                                                                err.to_string(),
                                                            )
                                                        })?;
                                                    self.control_plane
                                                        .record_worktree_retained(
                                                            wt_id,
                                                            Some(reason),
                                                        )
                                                        .await
                                                        .map_err(|err| {
                                                            InternalAgentError::WorkspacePostprocessError(
                                                                err.to_string(),
                                                            )
                                                        })?;
                                                    return Err(
                                                        InternalAgentError::WorkspacePostprocessError(
                                                            e.to_string(),
                                                        ),
                                                    );
                                                }
                                            }
                                        }

                                        self.control_plane
                                            .transition_run_state(
                                                completion.run_id,
                                                RunState::Succeeded,
                                            )
                                            .await
                                            .map_err(|e| {
                                                InternalAgentError::WorkspacePostprocessError(
                                                    e.to_string(),
                                                )
                                            })?;
                                        self.control_plane
                                            .transition_task_state(
                                                completion.task_id,
                                                TaskState::Succeeded,
                                            )
                                            .await
                                            .map_err(|e| {
                                                InternalAgentError::WorkspacePostprocessError(
                                                    e.to_string(),
                                                )
                                            })?;
                                    }
                                    _ => {
                                        if let Some(wt_id) = completion.workspace.worktree_id() {
                                            if let (Some(orchestrator), Some(repo_path), Some(root)) = (
                                                &self.workspace_orchestrator,
                                                &self.repo_path,
                                                completion.workspace.root(),
                                            ) {
                                                let reason = "Retained on task failure".to_string();
                                                orchestrator
                                                    .release_worktree_async(
                                                        repo_path.clone(),
                                                        root.to_path_buf(),
                                                        true,
                                                        Some(reason.clone()),
                                                    )
                                                    .await
                                                    .map_err(|e| {
                                                        InternalAgentError::WorkspacePostprocessError(
                                                            e.to_string(),
                                                        )
                                                    })?;
                                                self.control_plane
                                                    .record_worktree_retained(wt_id, Some(reason))
                                                    .await
                                                    .map_err(|e| {
                                                        InternalAgentError::WorkspacePostprocessError(
                                                            e.to_string(),
                                                        )
                                                    })?;
                                            }
                                        }

                                        let should_retry = match self.failure_policy {
                                            FailurePolicy::RetryTask(max_retries) => {
                                                let current = retries_by_task
                                                    .entry(completion.task_id)
                                                    .or_insert(0);
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
                                            let attempt = retries_by_task[&completion.task_id];
                                            let max_attempts = match self.failure_policy {
                                                FailurePolicy::RetryTask(m) => m,
                                                _ => 1,
                                            };
                                            let backoff_ms = (50u64
                                                * (1 << (attempt.saturating_sub(1))))
                                            .min(2000);

                                            self.control_plane
                                                .transition_run_state(
                                                    completion.run_id,
                                                    RunState::Failed,
                                                )
                                                .await
                                                .map_err(|e| {
                                                    InternalAgentError::WorkspacePostprocessError(
                                                        e.to_string(),
                                                    )
                                                })?;
                                            self.control_plane
                                                .transition_task_state(
                                                    completion.task_id,
                                                    TaskState::Retrying,
                                                )
                                                .await
                                                .map_err(|e| {
                                                    InternalAgentError::WorkspacePostprocessError(
                                                        e.to_string(),
                                                    )
                                                })?;
                                            self.control_plane
                                                .schedule_task_retry(
                                                    self.studio_id,
                                                    completion.task_id,
                                                    attempt,
                                                    max_attempts,
                                                    "Task execution failed, retry scheduled",
                                                    backoff_ms,
                                                    None,
                                                )
                                                .await
                                                .map_err(|e| {
                                                    InternalAgentError::WorkspacePostprocessError(
                                                        e.to_string(),
                                                    )
                                                })?;

                                            delayed_retries.push((
                                                completion.task_id,
                                                tokio::time::Instant::now()
                                                    + Duration::from_millis(backoff_ms),
                                            ));
                                        } else {
                                            self.control_plane
                                                .transition_run_state(
                                                    completion.run_id,
                                                    RunState::Failed,
                                                )
                                                .await
                                                .map_err(|e| {
                                                    InternalAgentError::WorkspacePostprocessError(
                                                        e.to_string(),
                                                    )
                                                })?;
                                            self.control_plane
                                                .transition_task_state(
                                                    completion.task_id,
                                                    TaskState::Failed,
                                                )
                                                .await
                                                .map_err(|e| {
                                                    InternalAgentError::WorkspacePostprocessError(
                                                        e.to_string(),
                                                    )
                                                })?;

                                            if self.failure_policy == FailurePolicy::FailFast
                                                && !cancelled
                                            {
                                                cancelled = true;
                                                let _ = self
                                                    .control_plane
                                                    .request_cancellation(
                                                        CancellationScope::Studio(self.studio_id),
                                                        Some(
                                                            "Task failed under FailFast policy"
                                                                .to_string(),
                                                        ),
                                                    )
                                                    .await;

                                                for &aid in &active_agents {
                                                    let _ = self.executor.cancel_agent(aid).await;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(join_err) => {
                            tracing::error!("JoinSet error: {join_err}");
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(20)) => {}
            }
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

        // Final coordinator review turn
        let coordinator_spec = &self.team_spec.coordinator;
        let review_prompt = format!(
            "Objective: {}\nStudio ID: {}\nTotal tasks: {}\nSucceeded: {}\nFailed: {}\nCancelled: {}\nReview the executed tasks and summarize the final outcome.",
            objective, self.studio_id, total_tasks, completed_tasks, failed_tasks, cancelled_tasks
        );

        let review_context = AgentExecutionContext {
            studio_id: self.studio_id,
            task_id: None,
            run_id: None,
            parent_agent_id: None,
            agent_spec: coordinator_spec.clone(),
            prompt: review_prompt,
            budget: coordinator_spec.budget.clone(),
            output_schema: None,
            execution_workspace: ExecutionWorkspace::shared_source(
                self.repo_path.clone().unwrap_or_default(),
            ),
        };

        let review_result = self.executor.execute_agent(review_context).await;
        let (coordinator_summary, coordinator_review_status) = match review_result {
            Ok(res) if res.success && !res.output.trim().is_empty() => {
                let bounded_text = if res.output.len() > 4096 {
                    let preview: String = res.output.chars().take(4096).collect();
                    format!("{preview}... (truncated)")
                } else {
                    res.output
                };
                (Some(bounded_text), CoordinatorReviewStatus::Succeeded)
            }
            _ => (None, CoordinatorReviewStatus::Failed),
        };

        let blocked_tasks = studio_tasks
            .iter()
            .filter(|t| t.state == TaskState::Blocked)
            .count();

        Ok(SupervisorExecutionSummary {
            studio_id: self.studio_id,
            total_tasks,
            completed_tasks,
            failed_tasks,
            cancelled_tasks,
            blocked_tasks,
            coordinator_summary,
            coordinator_review_status,
        })
    }
}
