use std::collections::HashMap;

use agent_studios_control_plane::ControlPlaneState;
use agent_studios_control_plane::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::error::ControlPlaneError;
use agent_studios_control_plane::store::InMemoryStore;
use agent_studios_protocol::agent::{AgentDescriptor, AgentKind, AgentState, BatchAgentSpec};
use agent_studios_protocol::artifact::ArtifactRecord;
use agent_studios_protocol::cancellation::{CancellationScope, CancellationSummary};
use agent_studios_protocol::event::EventEnvelope;
use agent_studios_protocol::id::{
    AgentId, ArtifactId, ReconciliationId, RunId, StudioId, TaskId, WorktreeId,
};
use agent_studios_protocol::reconciliation::{ReconciliationRecord, ReconciliationState};
use agent_studios_protocol::run::{RunRecord, RunState};
use agent_studios_protocol::studio::Studio;
use agent_studios_protocol::task::{BatchTaskSpec, TaskRecord, TaskState};
use agent_studios_protocol::worktree::{WorktreeRecord, WorktreeState};
use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::error::InternalAgentError;

#[allow(clippy::type_complexity)]
pub enum ControlPlaneCommand {
    CreateStudio {
        name: String,
        respond_to: oneshot::Sender<Result<Studio, ControlPlaneError>>,
    },
    GetStudio {
        studio_id: StudioId,
        respond_to: oneshot::Sender<Result<Studio, ControlPlaneError>>,
    },
    RegisterAgent {
        studio_id: StudioId,
        agent_id: Option<AgentId>,
        display_name: String,
        kind: AgentKind,
        role: Option<String>,
        respond_to: oneshot::Sender<Result<AgentDescriptor, ControlPlaneError>>,
    },
    RegisterAgentBatch {
        studio_id: StudioId,
        agents: Vec<BatchAgentSpec>,
        respond_to: oneshot::Sender<Result<Vec<AgentDescriptor>, ControlPlaneError>>,
    },
    TransitionAgentState {
        agent_id: AgentId,
        new_state: AgentState,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    CreateTask {
        studio_id: StudioId,
        title: String,
        description: String,
        parent_task_id: Option<TaskId>,
        assigned_agent_id: Option<AgentId>,
        dependencies: Vec<TaskId>,
        respond_to: oneshot::Sender<Result<TaskRecord, ControlPlaneError>>,
    },
    CreateTaskBatch {
        studio_id: StudioId,
        batch: Vec<BatchTaskSpec>,
        respond_to: oneshot::Sender<Result<Vec<TaskRecord>, ControlPlaneError>>,
    },
    AddTaskDependency {
        task_id: TaskId,
        dependency_id: TaskId,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    TransitionTaskState {
        task_id: TaskId,
        new_state: TaskState,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    CreateRun {
        task_id: TaskId,
        agent_id: AgentId,
        respond_to: oneshot::Sender<Result<RunRecord, ControlPlaneError>>,
    },
    TransitionRunState {
        run_id: RunId,
        new_state: RunState,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    RecordRunOutcome {
        run_id: RunId,
        classification: String,
        safe_error_summary: Option<String>,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    RequestCancellation {
        scope: CancellationScope,
        reason: Option<String>,
        respond_to: oneshot::Sender<Result<CancellationSummary, ControlPlaneError>>,
    },
    SubscribeEvents {
        studio_id: StudioId,
        from_sequence: u64,
        respond_to: oneshot::Sender<
            Result<(Vec<EventEnvelope>, broadcast::Receiver<EventEnvelope>), ControlPlaneError>,
        >,
    },
    RecordRuntimeBound {
        studio_id: StudioId,
        agent_id: AgentId,
        runtime_kind: String,
        provider_instance_id: String,
        model_id: String,
        protocol: String,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    RecordAgentSpawned {
        studio_id: StudioId,
        agent_id: AgentId,
        thread_id: String,
        parent_agent_id: Option<AgentId>,
        parent_thread_id: Option<String>,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    RecordToolStarted {
        studio_id: StudioId,
        agent_id: AgentId,
        task_id: Option<TaskId>,
        run_id: Option<RunId>,
        tool_name: String,
        call_id: String,
        timestamp: DateTime<Utc>,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    RecordToolCompleted {
        studio_id: StudioId,
        agent_id: AgentId,
        task_id: Option<TaskId>,
        run_id: Option<RunId>,
        tool_name: String,
        call_id: String,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    RecordToolFailed {
        studio_id: StudioId,
        agent_id: AgentId,
        task_id: Option<TaskId>,
        run_id: Option<RunId>,
        tool_name: String,
        call_id: String,
        error: String,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    RecordBudgetUsageUpdated {
        studio_id: StudioId,
        agent_id: AgentId,
        run_id: Option<RunId>,
        turns_used: u32,
        tool_calls_used: u32,
        wall_clock_secs: u64,
        child_agents_used: u32,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    RecordBudgetExceeded {
        studio_id: StudioId,
        agent_id: AgentId,
        run_id: Option<RunId>,
        dimension: String,
        limit: u64,
        actual: u64,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    ScheduleTaskRetry {
        studio_id: StudioId,
        task_id: TaskId,
        attempt: u32,
        max_attempts: u32,
        reason: String,
        backoff_ms: u64,
        next_retry_at: Option<DateTime<Utc>>,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    GetState {
        respond_to: oneshot::Sender<ControlPlaneState>,
    },
    GetReadyTasks {
        studio_id: StudioId,
        respond_to: oneshot::Sender<Vec<TaskRecord>>,
    },
    GetStudioTasks {
        studio_id: StudioId,
        respond_to: oneshot::Sender<Vec<TaskRecord>>,
    },
    GetTask {
        task_id: TaskId,
        respond_to: oneshot::Sender<Option<TaskRecord>>,
    },
    CreateWorktree {
        studio_id: StudioId,
        name: String,
        repo_path: std::path::PathBuf,
        worktree_path: std::path::PathBuf,
        base_commit: String,
        respond_to: oneshot::Sender<Result<WorktreeRecord, ControlPlaneError>>,
    },
    CreateWorktreeWithCwds {
        studio_id: StudioId,
        name: String,
        source_root: std::path::PathBuf,
        source_cwd: std::path::PathBuf,
        worktree_root: std::path::PathBuf,
        worktree_cwd: std::path::PathBuf,
        base_commit: String,
        respond_to: oneshot::Sender<Result<WorktreeRecord, ControlPlaneError>>,
    },
    AssignWorktree {
        worktree_id: WorktreeId,
        task_id: TaskId,
        agent_id: AgentId,
        run_id: Option<RunId>,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    BindWorktreeThread {
        worktree_id: WorktreeId,
        thread_id: String,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    TransitionWorktreeState {
        worktree_id: WorktreeId,
        new_state: WorktreeState,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    RecordWorktreeChangeCaptured {
        worktree_id: WorktreeId,
        run_id: Option<RunId>,
        base_commit: String,
        head_commit: Option<String>,
        patch_artifact_id: ArtifactId,
        stats_artifact_id: Option<ArtifactId>,
        files_changed: usize,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    ReleaseWorktree {
        worktree_id: WorktreeId,
        retained: bool,
        reason: Option<String>,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    CompleteWorktreeRemoval {
        worktree_id: WorktreeId,
        reason: Option<String>,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    GetWorktree {
        worktree_id: WorktreeId,
        respond_to: oneshot::Sender<Option<WorktreeRecord>>,
    },
    RegisterArtifactRecord {
        artifact: ArtifactRecord,
        respond_to: oneshot::Sender<Result<ArtifactRecord, ControlPlaneError>>,
    },
    GetArtifactLineage {
        artifact_id: ArtifactId,
        respond_to: oneshot::Sender<Result<Vec<ArtifactRecord>, ControlPlaneError>>,
    },
    CreateReconciliation {
        studio_id: StudioId,
        worktree_id: WorktreeId,
        task_id: TaskId,
        run_id: Option<RunId>,
        patch_artifact_id: ArtifactId,
        target_worktree_path: std::path::PathBuf,
        base_commit: String,
        respond_to: oneshot::Sender<Result<ReconciliationRecord, ControlPlaneError>>,
    },
    TransitionReconciliationState {
        reconciliation_id: ReconciliationId,
        new_state: ReconciliationState,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    RecordReconciliationApplied {
        reconciliation_id: ReconciliationId,
        merge_commit: Option<String>,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    RecordReconciliationConflict {
        reconciliation_id: ReconciliationId,
        conflicted_files: Vec<String>,
        reason: String,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    RecordWorktreeRetained {
        worktree_id: WorktreeId,
        reason: Option<String>,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    RecordWorktreeNoChanges {
        worktree_id: WorktreeId,
        run_id: Option<RunId>,
        base_commit: String,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    CreateReconciliationWithTarget {
        studio_id: StudioId,
        source_worktree_id: WorktreeId,
        target_worktree_id: WorktreeId,
        task_id: TaskId,
        run_id: Option<RunId>,
        patch_artifact_id: ArtifactId,
        target_worktree_path: std::path::PathBuf,
        base_commit: String,
        respond_to: oneshot::Sender<Result<ReconciliationRecord, ControlPlaneError>>,
    },
    StartReconciliation {
        reconciliation_id: ReconciliationId,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    MarkReconciliationApplying {
        reconciliation_id: ReconciliationId,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
    RecordReconciliationFailed {
        reconciliation_id: ReconciliationId,
        error: String,
        respond_to: oneshot::Sender<Result<(), ControlPlaneError>>,
    },
}

#[derive(Clone, Debug)]
pub struct ControlPlaneHandle {
    sender: mpsc::Sender<ControlPlaneCommand>,
}

impl ControlPlaneHandle {
    pub fn new(sender: mpsc::Sender<ControlPlaneCommand>) -> Self {
        Self { sender }
    }

    pub async fn create_studio(
        &self,
        name: impl Into<String>,
    ) -> Result<Studio, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::CreateStudio {
                name: name.into(),
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn get_studio(&self, studio_id: StudioId) -> Result<Studio, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::GetStudio {
                studio_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn register_agent(
        &self,
        studio_id: StudioId,
        display_name: impl Into<String>,
        kind: AgentKind,
        role: Option<String>,
    ) -> Result<AgentDescriptor, InternalAgentError> {
        self.register_agent_internal(studio_id, None, display_name, kind, role)
            .await
    }

    pub async fn register_agent_with_id(
        &self,
        studio_id: StudioId,
        agent_id: AgentId,
        display_name: impl Into<String>,
        kind: AgentKind,
        role: Option<String>,
    ) -> Result<AgentDescriptor, InternalAgentError> {
        self.register_agent_internal(studio_id, Some(agent_id), display_name, kind, role)
            .await
    }

    async fn register_agent_internal(
        &self,
        studio_id: StudioId,
        agent_id: Option<AgentId>,
        display_name: impl Into<String>,
        kind: AgentKind,
        role: Option<String>,
    ) -> Result<AgentDescriptor, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RegisterAgent {
                studio_id,
                agent_id,
                display_name: display_name.into(),
                kind,
                role,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn register_agent_batch(
        &self,
        studio_id: StudioId,
        agents: Vec<BatchAgentSpec>,
    ) -> Result<Vec<AgentDescriptor>, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RegisterAgentBatch {
                studio_id,
                agents,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn create_task(
        &self,
        studio_id: StudioId,
        title: impl Into<String>,
        description: impl Into<String>,
        parent_task_id: Option<TaskId>,
        assigned_agent_id: Option<AgentId>,
        dependencies: Vec<TaskId>,
    ) -> Result<TaskRecord, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::CreateTask {
                studio_id,
                title: title.into(),
                description: description.into(),
                parent_task_id,
                assigned_agent_id,
                dependencies,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn add_task_dependency(
        &self,
        task_id: TaskId,
        dependency_id: TaskId,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::AddTaskDependency {
                task_id,
                dependency_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn transition_task_state(
        &self,
        task_id: TaskId,
        new_state: TaskState,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::TransitionTaskState {
                task_id,
                new_state,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn create_run(
        &self,
        task_id: TaskId,
        agent_id: AgentId,
    ) -> Result<RunRecord, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::CreateRun {
                task_id,
                agent_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn transition_run_state(
        &self,
        run_id: RunId,
        new_state: RunState,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::TransitionRunState {
                run_id,
                new_state,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn record_run_outcome(
        &self,
        run_id: RunId,
        classification: impl Into<String>,
        safe_error_summary: Option<String>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RecordRunOutcome {
                run_id,
                classification: classification.into(),
                safe_error_summary,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn request_cancellation(
        &self,
        scope: CancellationScope,
        reason: Option<String>,
    ) -> Result<CancellationSummary, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RequestCancellation {
                scope,
                reason,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn get_state(&self) -> Result<ControlPlaneState, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::GetState { respond_to: tx })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await.map_err(|_| InternalAgentError::ActorDropped)
    }

    pub async fn get_ready_tasks(
        &self,
        studio_id: StudioId,
    ) -> Result<Vec<TaskRecord>, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::GetReadyTasks {
                studio_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await.map_err(|_| InternalAgentError::ActorDropped)
    }

    pub async fn get_studio_tasks(
        &self,
        studio_id: StudioId,
    ) -> Result<Vec<TaskRecord>, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::GetStudioTasks {
                studio_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await.map_err(|_| InternalAgentError::ActorDropped)
    }

    pub async fn get_task(
        &self,
        task_id: TaskId,
    ) -> Result<Option<TaskRecord>, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::GetTask {
                task_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await.map_err(|_| InternalAgentError::ActorDropped)
    }

    pub async fn transition_agent_state(
        &self,
        agent_id: AgentId,
        new_state: AgentState,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::TransitionAgentState {
                agent_id,
                new_state,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn create_task_batch(
        &self,
        studio_id: StudioId,
        batch: Vec<BatchTaskSpec>,
    ) -> Result<Vec<TaskRecord>, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::CreateTaskBatch {
                studio_id,
                batch,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn subscribe_events(
        &self,
        studio_id: StudioId,
        from_sequence: u64,
    ) -> Result<(Vec<EventEnvelope>, broadcast::Receiver<EventEnvelope>), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::SubscribeEvents {
                studio_id,
                from_sequence,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn record_runtime_bound(
        &self,
        studio_id: StudioId,
        agent_id: AgentId,
        runtime_kind: impl Into<String>,
        provider_instance_id: impl Into<String>,
        model_id: impl Into<String>,
        protocol: impl Into<String>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RecordRuntimeBound {
                studio_id,
                agent_id,
                runtime_kind: runtime_kind.into(),
                provider_instance_id: provider_instance_id.into(),
                model_id: model_id.into(),
                protocol: protocol.into(),
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn record_agent_spawned(
        &self,
        studio_id: StudioId,
        agent_id: AgentId,
        thread_id: impl Into<String>,
        parent_agent_id: Option<AgentId>,
        parent_thread_id: Option<String>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RecordAgentSpawned {
                studio_id,
                agent_id,
                thread_id: thread_id.into(),
                parent_agent_id,
                parent_thread_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_tool_started(
        &self,
        studio_id: StudioId,
        agent_id: AgentId,
        task_id: Option<TaskId>,
        run_id: Option<RunId>,
        tool_name: impl Into<String>,
        call_id: impl Into<String>,
        timestamp: DateTime<Utc>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RecordToolStarted {
                studio_id,
                agent_id,
                task_id,
                run_id,
                tool_name: tool_name.into(),
                call_id: call_id.into(),
                timestamp,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_tool_completed(
        &self,
        studio_id: StudioId,
        agent_id: AgentId,
        task_id: Option<TaskId>,
        run_id: Option<RunId>,
        tool_name: impl Into<String>,
        call_id: impl Into<String>,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RecordToolCompleted {
                studio_id,
                agent_id,
                task_id,
                run_id,
                tool_name: tool_name.into(),
                call_id: call_id.into(),
                duration_ms,
                timestamp,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_tool_failed(
        &self,
        studio_id: StudioId,
        agent_id: AgentId,
        task_id: Option<TaskId>,
        run_id: Option<RunId>,
        tool_name: impl Into<String>,
        call_id: impl Into<String>,
        error: impl Into<String>,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RecordToolFailed {
                studio_id,
                agent_id,
                task_id,
                run_id,
                tool_name: tool_name.into(),
                call_id: call_id.into(),
                error: error.into(),
                duration_ms,
                timestamp,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_budget_usage_updated(
        &self,
        studio_id: StudioId,
        agent_id: AgentId,
        run_id: Option<RunId>,
        turns_used: u32,
        tool_calls_used: u32,
        wall_clock_secs: u64,
        child_agents_used: u32,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RecordBudgetUsageUpdated {
                studio_id,
                agent_id,
                run_id,
                turns_used,
                tool_calls_used,
                wall_clock_secs,
                child_agents_used,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn record_budget_exceeded(
        &self,
        studio_id: StudioId,
        agent_id: AgentId,
        run_id: Option<RunId>,
        dimension: impl Into<String>,
        limit: u64,
        actual: u64,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RecordBudgetExceeded {
                studio_id,
                agent_id,
                run_id,
                dimension: dimension.into(),
                limit,
                actual,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn schedule_task_retry(
        &self,
        studio_id: StudioId,
        task_id: TaskId,
        attempt: u32,
        max_attempts: u32,
        reason: impl Into<String>,
        backoff_ms: u64,
        next_retry_at: Option<DateTime<Utc>>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::ScheduleTaskRetry {
                studio_id,
                task_id,
                attempt,
                max_attempts,
                reason: reason.into(),
                backoff_ms,
                next_retry_at,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn create_worktree(
        &self,
        studio_id: StudioId,
        name: impl Into<String>,
        repo_path: impl Into<std::path::PathBuf>,
        worktree_path: impl Into<std::path::PathBuf>,
        base_commit: impl Into<String>,
    ) -> Result<WorktreeRecord, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::CreateWorktree {
                studio_id,
                name: name.into(),
                repo_path: repo_path.into(),
                worktree_path: worktree_path.into(),
                base_commit: base_commit.into(),
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_worktree_with_cwds(
        &self,
        studio_id: StudioId,
        name: impl Into<String>,
        source_root: impl Into<std::path::PathBuf>,
        source_cwd: impl Into<std::path::PathBuf>,
        worktree_root: impl Into<std::path::PathBuf>,
        worktree_cwd: impl Into<std::path::PathBuf>,
        base_commit: impl Into<String>,
    ) -> Result<WorktreeRecord, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::CreateWorktreeWithCwds {
                studio_id,
                name: name.into(),
                source_root: source_root.into(),
                source_cwd: source_cwd.into(),
                worktree_root: worktree_root.into(),
                worktree_cwd: worktree_cwd.into(),
                base_commit: base_commit.into(),
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn assign_worktree(
        &self,
        worktree_id: WorktreeId,
        task_id: TaskId,
        agent_id: AgentId,
        run_id: Option<RunId>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::AssignWorktree {
                worktree_id,
                task_id,
                agent_id,
                run_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn bind_worktree_thread(
        &self,
        worktree_id: WorktreeId,
        thread_id: impl Into<String>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::BindWorktreeThread {
                worktree_id,
                thread_id: thread_id.into(),
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn transition_worktree_state(
        &self,
        worktree_id: WorktreeId,
        new_state: WorktreeState,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::TransitionWorktreeState {
                worktree_id,
                new_state,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_worktree_change_captured(
        &self,
        worktree_id: WorktreeId,
        run_id: Option<RunId>,
        base_commit: impl Into<String>,
        head_commit: Option<String>,
        patch_artifact_id: ArtifactId,
        stats_artifact_id: Option<ArtifactId>,
        files_changed: usize,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RecordWorktreeChangeCaptured {
                worktree_id,
                run_id,
                base_commit: base_commit.into(),
                head_commit,
                patch_artifact_id,
                stats_artifact_id,
                files_changed,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn release_worktree(
        &self,
        worktree_id: WorktreeId,
        retained: bool,
        reason: Option<String>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::ReleaseWorktree {
                worktree_id,
                retained,
                reason,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn complete_worktree_removal(
        &self,
        worktree_id: WorktreeId,
        reason: Option<String>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::CompleteWorktreeRemoval {
                worktree_id,
                reason,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn get_worktree(
        &self,
        worktree_id: WorktreeId,
    ) -> Result<Option<WorktreeRecord>, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::GetWorktree {
                worktree_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await.map_err(|_| InternalAgentError::ActorDropped)
    }

    pub async fn register_artifact_record(
        &self,
        artifact: ArtifactRecord,
    ) -> Result<ArtifactRecord, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RegisterArtifactRecord {
                artifact,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn get_artifact_lineage(
        &self,
        artifact_id: ArtifactId,
    ) -> Result<Vec<ArtifactRecord>, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::GetArtifactLineage {
                artifact_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_reconciliation(
        &self,
        studio_id: StudioId,
        worktree_id: WorktreeId,
        task_id: TaskId,
        run_id: Option<RunId>,
        patch_artifact_id: ArtifactId,
        target_worktree_path: impl Into<std::path::PathBuf>,
        base_commit: impl Into<String>,
    ) -> Result<ReconciliationRecord, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::CreateReconciliation {
                studio_id,
                worktree_id,
                task_id,
                run_id,
                patch_artifact_id,
                target_worktree_path: target_worktree_path.into(),
                base_commit: base_commit.into(),
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn transition_reconciliation_state(
        &self,
        reconciliation_id: ReconciliationId,
        new_state: ReconciliationState,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::TransitionReconciliationState {
                reconciliation_id,
                new_state,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn record_reconciliation_applied(
        &self,
        reconciliation_id: ReconciliationId,
        merge_commit: Option<String>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RecordReconciliationApplied {
                reconciliation_id,
                merge_commit,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn record_reconciliation_conflict(
        &self,
        reconciliation_id: ReconciliationId,
        conflicted_files: Vec<String>,
        reason: impl Into<String>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RecordReconciliationConflict {
                reconciliation_id,
                conflicted_files,
                reason: reason.into(),
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn record_worktree_retained(
        &self,
        worktree_id: WorktreeId,
        reason: Option<String>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RecordWorktreeRetained {
                worktree_id,
                reason,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn record_worktree_no_changes(
        &self,
        worktree_id: WorktreeId,
        run_id: Option<RunId>,
        base_commit: impl Into<String>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RecordWorktreeNoChanges {
                worktree_id,
                run_id,
                base_commit: base_commit.into(),
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_reconciliation_with_target(
        &self,
        studio_id: StudioId,
        source_worktree_id: WorktreeId,
        target_worktree_id: WorktreeId,
        task_id: TaskId,
        run_id: Option<RunId>,
        patch_artifact_id: ArtifactId,
        target_worktree_path: impl Into<std::path::PathBuf>,
        base_commit: impl Into<String>,
    ) -> Result<ReconciliationRecord, InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::CreateReconciliationWithTarget {
                studio_id,
                source_worktree_id,
                target_worktree_id,
                task_id,
                run_id,
                patch_artifact_id,
                target_worktree_path: target_worktree_path.into(),
                base_commit: base_commit.into(),
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn start_reconciliation(
        &self,
        reconciliation_id: ReconciliationId,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::StartReconciliation {
                reconciliation_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn mark_reconciliation_applying(
        &self,
        reconciliation_id: ReconciliationId,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::MarkReconciliationApplying {
                reconciliation_id,
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }

    pub async fn record_reconciliation_failed(
        &self,
        reconciliation_id: ReconciliationId,
        error: impl Into<String>,
    ) -> Result<(), InternalAgentError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ControlPlaneCommand::RecordReconciliationFailed {
                reconciliation_id,
                error: error.into(),
                respond_to: tx,
            })
            .await
            .map_err(|_| InternalAgentError::ActorDropped)?;
        rx.await
            .map_err(|_| InternalAgentError::ActorDropped)?
            .map_err(InternalAgentError::ControlPlane)
    }
}

pub struct ControlPlaneActor;

impl ControlPlaneActor {
    pub fn spawn(
        mut control_plane: ControlPlane<SystemClock, InMemoryStore>,
    ) -> (ControlPlaneHandle, tokio::task::JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<ControlPlaneCommand>(1024);
        let mut studio_channels: HashMap<StudioId, broadcast::Sender<EventEnvelope>> =
            HashMap::new();

        let handle = tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                match command {
                    ControlPlaneCommand::CreateStudio { name, respond_to } => {
                        let res = control_plane.create_studio(name);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::GetStudio {
                        studio_id,
                        respond_to,
                    } => {
                        let res = control_plane
                            .get_studio(studio_id)
                            .cloned()
                            .ok_or(ControlPlaneError::StudioNotFound(studio_id));
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RegisterAgent {
                        studio_id,
                        agent_id,
                        display_name,
                        kind,
                        role,
                        respond_to,
                    } => {
                        let res = match agent_id {
                            Some(id) => control_plane.register_agent_with_id(
                                studio_id,
                                id,
                                display_name,
                                kind,
                                role,
                            ),
                            None => {
                                control_plane.register_agent(studio_id, display_name, kind, role)
                            }
                        };
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RegisterAgentBatch {
                        studio_id,
                        agents,
                        respond_to,
                    } => {
                        let res = control_plane.register_agent_batch(studio_id, agents);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::TransitionAgentState {
                        agent_id,
                        new_state,
                        respond_to,
                    } => {
                        let res = control_plane.update_agent_state(agent_id, new_state);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::CreateTask {
                        studio_id,
                        title,
                        description,
                        parent_task_id,
                        assigned_agent_id,
                        dependencies,
                        respond_to,
                    } => {
                        let res = control_plane.create_task(
                            studio_id,
                            title,
                            description,
                            parent_task_id,
                            assigned_agent_id,
                            dependencies,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::CreateTaskBatch {
                        studio_id,
                        batch,
                        respond_to,
                    } => {
                        let res = control_plane.create_task_batch(studio_id, batch);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::AddTaskDependency {
                        task_id,
                        dependency_id,
                        respond_to,
                    } => {
                        let res = control_plane.add_task_dependency(task_id, dependency_id);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::TransitionTaskState {
                        task_id,
                        new_state,
                        respond_to,
                    } => {
                        let res = control_plane.transition_task_state(task_id, new_state);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::CreateRun {
                        task_id,
                        agent_id,
                        respond_to,
                    } => {
                        let res = control_plane.create_run(task_id, agent_id);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::TransitionRunState {
                        run_id,
                        new_state,
                        respond_to,
                    } => {
                        let res = control_plane.transition_run_state(run_id, new_state);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RecordRunOutcome {
                        run_id,
                        classification,
                        safe_error_summary,
                        respond_to,
                    } => {
                        let res = control_plane.record_run_outcome(
                            run_id,
                            classification,
                            safe_error_summary,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RequestCancellation {
                        scope,
                        reason,
                        respond_to,
                    } => {
                        let res = control_plane.request_cancellation(scope, reason);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::SubscribeEvents {
                        studio_id,
                        from_sequence,
                        respond_to,
                    } => {
                        let sender = studio_channels.entry(studio_id).or_insert_with(|| {
                            let (tx, _) = broadcast::channel(4096);
                            tx
                        });
                        let sub_rx = sender.subscribe();
                        let res = control_plane
                            .events_for_studio(studio_id, from_sequence)
                            .map(|events| (events, sub_rx));
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RecordRuntimeBound {
                        studio_id,
                        agent_id,
                        runtime_kind,
                        provider_instance_id,
                        model_id,
                        protocol,
                        respond_to,
                    } => {
                        let res = control_plane.record_runtime_bound(
                            studio_id,
                            agent_id,
                            runtime_kind,
                            provider_instance_id,
                            model_id,
                            protocol,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RecordAgentSpawned {
                        studio_id,
                        agent_id,
                        thread_id,
                        parent_agent_id,
                        parent_thread_id,
                        respond_to,
                    } => {
                        let res = control_plane.record_agent_spawned(
                            studio_id,
                            agent_id,
                            thread_id,
                            parent_agent_id,
                            parent_thread_id,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RecordToolStarted {
                        studio_id,
                        agent_id,
                        task_id,
                        run_id,
                        tool_name,
                        call_id,
                        timestamp,
                        respond_to,
                    } => {
                        let res = control_plane.record_tool_started(
                            studio_id, agent_id, task_id, run_id, tool_name, call_id, timestamp,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RecordToolCompleted {
                        studio_id,
                        agent_id,
                        task_id,
                        run_id,
                        tool_name,
                        call_id,
                        duration_ms,
                        timestamp,
                        respond_to,
                    } => {
                        let res = control_plane.record_tool_completed(
                            studio_id,
                            agent_id,
                            task_id,
                            run_id,
                            tool_name,
                            call_id,
                            duration_ms,
                            timestamp,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RecordToolFailed {
                        studio_id,
                        agent_id,
                        task_id,
                        run_id,
                        tool_name,
                        call_id,
                        error,
                        duration_ms,
                        timestamp,
                        respond_to,
                    } => {
                        let res = control_plane.record_tool_failed(
                            studio_id,
                            agent_id,
                            task_id,
                            run_id,
                            tool_name,
                            call_id,
                            error,
                            duration_ms,
                            timestamp,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RecordBudgetUsageUpdated {
                        studio_id,
                        agent_id,
                        run_id,
                        turns_used,
                        tool_calls_used,
                        wall_clock_secs,
                        child_agents_used,
                        respond_to,
                    } => {
                        let res = control_plane.record_budget_usage_updated(
                            studio_id,
                            agent_id,
                            run_id,
                            turns_used,
                            tool_calls_used,
                            wall_clock_secs,
                            child_agents_used,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RecordBudgetExceeded {
                        studio_id,
                        agent_id,
                        run_id,
                        dimension,
                        limit,
                        actual,
                        respond_to,
                    } => {
                        let res = control_plane.record_budget_exceeded(
                            studio_id, agent_id, run_id, dimension, limit, actual,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::ScheduleTaskRetry {
                        studio_id,
                        task_id,
                        attempt,
                        max_attempts,
                        reason,
                        backoff_ms,
                        next_retry_at,
                        respond_to,
                    } => {
                        let res = control_plane.schedule_task_retry(
                            studio_id,
                            task_id,
                            attempt,
                            max_attempts,
                            reason,
                            backoff_ms,
                            next_retry_at,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::GetState { respond_to } => {
                        let state = control_plane.state().clone();
                        let _ = respond_to.send(state);
                    }
                    ControlPlaneCommand::GetReadyTasks {
                        studio_id,
                        respond_to,
                    } => {
                        let ready = control_plane
                            .all_tasks()
                            .filter(|t| t.studio_id == studio_id && t.state == TaskState::Ready)
                            .cloned()
                            .collect();
                        let _ = respond_to.send(ready);
                    }
                    ControlPlaneCommand::GetStudioTasks {
                        studio_id,
                        respond_to,
                    } => {
                        let tasks = control_plane
                            .all_tasks()
                            .filter(|t| t.studio_id == studio_id)
                            .cloned()
                            .collect();
                        let _ = respond_to.send(tasks);
                    }
                    ControlPlaneCommand::GetTask {
                        task_id,
                        respond_to,
                    } => {
                        let task = control_plane.get_task(task_id).cloned();
                        let _ = respond_to.send(task);
                    }
                    ControlPlaneCommand::CreateWorktree {
                        studio_id,
                        name,
                        repo_path,
                        worktree_path,
                        base_commit,
                        respond_to,
                    } => {
                        let res = control_plane.create_worktree(
                            studio_id,
                            name,
                            repo_path,
                            worktree_path,
                            base_commit,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::CreateWorktreeWithCwds {
                        studio_id,
                        name,
                        source_root,
                        source_cwd,
                        worktree_root,
                        worktree_cwd,
                        base_commit,
                        respond_to,
                    } => {
                        let res = control_plane.create_worktree_with_cwds(
                            studio_id,
                            name,
                            source_root,
                            source_cwd,
                            worktree_root,
                            worktree_cwd,
                            base_commit,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::AssignWorktree {
                        worktree_id,
                        task_id,
                        agent_id,
                        run_id,
                        respond_to,
                    } => {
                        let res =
                            control_plane.assign_worktree(worktree_id, task_id, agent_id, run_id);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::BindWorktreeThread {
                        worktree_id,
                        thread_id,
                        respond_to,
                    } => {
                        let res = control_plane.bind_worktree_thread(worktree_id, thread_id);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::TransitionWorktreeState {
                        worktree_id,
                        new_state,
                        respond_to,
                    } => {
                        let res = control_plane.transition_worktree_state(worktree_id, new_state);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RecordWorktreeChangeCaptured {
                        worktree_id,
                        run_id,
                        base_commit,
                        head_commit,
                        patch_artifact_id,
                        stats_artifact_id,
                        files_changed,
                        respond_to,
                    } => {
                        let res = control_plane.record_worktree_change_captured(
                            worktree_id,
                            run_id,
                            base_commit,
                            head_commit,
                            patch_artifact_id,
                            stats_artifact_id,
                            files_changed,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::ReleaseWorktree {
                        worktree_id,
                        retained,
                        reason,
                        respond_to,
                    } => {
                        let res = control_plane.release_worktree(worktree_id, retained, reason);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::CompleteWorktreeRemoval {
                        worktree_id,
                        reason,
                        respond_to,
                    } => {
                        let res = control_plane.complete_worktree_removal(worktree_id, reason);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::GetWorktree {
                        worktree_id,
                        respond_to,
                    } => {
                        let wt = control_plane.get_worktree(worktree_id).cloned();
                        let _ = respond_to.send(wt);
                    }
                    ControlPlaneCommand::RegisterArtifactRecord {
                        artifact,
                        respond_to,
                    } => {
                        let res = control_plane.register_artifact_record(artifact);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::GetArtifactLineage {
                        artifact_id,
                        respond_to,
                    } => {
                        let res = control_plane.get_artifact_lineage(artifact_id);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::CreateReconciliation {
                        studio_id,
                        worktree_id,
                        task_id,
                        run_id,
                        patch_artifact_id,
                        target_worktree_path,
                        base_commit,
                        respond_to,
                    } => {
                        let res = control_plane.create_reconciliation(
                            studio_id,
                            worktree_id,
                            task_id,
                            run_id,
                            patch_artifact_id,
                            target_worktree_path,
                            base_commit,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::TransitionReconciliationState {
                        reconciliation_id,
                        new_state,
                        respond_to,
                    } => {
                        let res = control_plane
                            .transition_reconciliation_state(reconciliation_id, new_state);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RecordReconciliationApplied {
                        reconciliation_id,
                        merge_commit,
                        respond_to,
                    } => {
                        let res = control_plane
                            .record_reconciliation_applied(reconciliation_id, merge_commit);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RecordReconciliationConflict {
                        reconciliation_id,
                        conflicted_files,
                        reason,
                        respond_to,
                    } => {
                        let res = control_plane.record_reconciliation_conflict(
                            reconciliation_id,
                            conflicted_files,
                            reason,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RecordWorktreeRetained {
                        worktree_id,
                        reason,
                        respond_to,
                    } => {
                        let res = control_plane.record_worktree_retained(worktree_id, reason);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RecordWorktreeNoChanges {
                        worktree_id,
                        run_id,
                        base_commit,
                        respond_to,
                    } => {
                        let res = control_plane.record_worktree_no_changes(
                            worktree_id,
                            run_id,
                            base_commit,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::CreateReconciliationWithTarget {
                        studio_id,
                        source_worktree_id,
                        target_worktree_id,
                        task_id,
                        run_id,
                        patch_artifact_id,
                        target_worktree_path,
                        base_commit,
                        respond_to,
                    } => {
                        let res = control_plane.create_reconciliation_with_target(
                            studio_id,
                            source_worktree_id,
                            target_worktree_id,
                            task_id,
                            run_id,
                            patch_artifact_id,
                            target_worktree_path,
                            base_commit,
                        );
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::StartReconciliation {
                        reconciliation_id,
                        respond_to,
                    } => {
                        let res = control_plane.start_reconciliation(reconciliation_id);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::MarkReconciliationApplying {
                        reconciliation_id,
                        respond_to,
                    } => {
                        let res = control_plane.mark_reconciliation_applying(reconciliation_id);
                        let _ = respond_to.send(res);
                    }
                    ControlPlaneCommand::RecordReconciliationFailed {
                        reconciliation_id,
                        error,
                        respond_to,
                    } => {
                        let res =
                            control_plane.record_reconciliation_failed(reconciliation_id, error);
                        let _ = respond_to.send(res);
                    }
                }

                for env in control_plane.drain_committed_events() {
                    if let Some(sender) = studio_channels.get(&env.studio_id) {
                        let _ = sender.send(env);
                    }
                }
            }
        });

        (ControlPlaneHandle::new(tx), handle)
    }
}
