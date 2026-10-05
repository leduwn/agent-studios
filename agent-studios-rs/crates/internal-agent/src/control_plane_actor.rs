use std::collections::HashMap;

use agent_studios_control_plane::ControlPlaneState;
use agent_studios_control_plane::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::error::ControlPlaneError;
use agent_studios_control_plane::store::InMemoryStore;
use agent_studios_protocol::agent::{AgentDescriptor, AgentKind, AgentState, BatchAgentSpec};
use agent_studios_protocol::cancellation::{CancellationScope, CancellationSummary};
use agent_studios_protocol::event::EventEnvelope;
use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId};
use agent_studios_protocol::run::{RunRecord, RunState};
use agent_studios_protocol::studio::Studio;
use agent_studios_protocol::task::{BatchTaskSpec, TaskRecord, TaskState};
use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::error::InternalAgentError;

#[allow(clippy::type_complexity)]
pub enum ControlPlaneCommand {
    CreateStudio {
        name: String,
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
