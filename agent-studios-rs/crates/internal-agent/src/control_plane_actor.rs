use agent_studios_control_plane::ControlPlaneState;
use agent_studios_control_plane::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::error::ControlPlaneError;
use agent_studios_control_plane::store::InMemoryStore;
use agent_studios_protocol::agent::{AgentDescriptor, AgentKind};
use agent_studios_protocol::cancellation::{CancellationScope, CancellationSummary};
use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId};
use agent_studios_protocol::run::{RunRecord, RunState};
use agent_studios_protocol::studio::Studio;
use agent_studios_protocol::task::{TaskRecord, TaskState};
use tokio::sync::{mpsc, oneshot};

use crate::error::InternalAgentError;

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
    CreateTask {
        studio_id: StudioId,
        title: String,
        description: String,
        parent_task_id: Option<TaskId>,
        assigned_agent_id: Option<AgentId>,
        dependencies: Vec<TaskId>,
        respond_to: oneshot::Sender<Result<TaskRecord, ControlPlaneError>>,
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
    RequestCancellation {
        scope: CancellationScope,
        reason: Option<String>,
        respond_to: oneshot::Sender<Result<CancellationSummary, ControlPlaneError>>,
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
}

pub struct ControlPlaneActor;

impl ControlPlaneActor {
    pub fn spawn(
        mut control_plane: ControlPlane<SystemClock, InMemoryStore>,
    ) -> (ControlPlaneHandle, tokio::task::JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<ControlPlaneCommand>(1024);
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
                    ControlPlaneCommand::RequestCancellation {
                        scope,
                        reason,
                        respond_to,
                    } => {
                        let res = control_plane.request_cancellation(scope, reason);
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
            }
        });

        (ControlPlaneHandle::new(tx), handle)
    }
}
