use std::collections::{HashMap, HashSet, VecDeque};

use agent_studios_protocol::agent::{AgentDescriptor, AgentKind, AgentState, BatchAgentSpec};
use agent_studios_protocol::approval::{ApprovalKind, ApprovalRequest, ApprovalState};
use agent_studios_protocol::artifact::{ArtifactKind, ArtifactRecord};
use agent_studios_protocol::cancellation::{CancellationScope, CancellationSummary};
use agent_studios_protocol::error::TransitionError;
use agent_studios_protocol::event::{
    CONTROL_PLANE_EVENT_SCHEMA_VERSION, ControlPlaneEvent, EventEnvelope,
};
use agent_studios_protocol::id::{
    AgentId, ApprovalId, ArtifactId, EventId, RunId, StudioId, TaskId,
};
use agent_studios_protocol::run::{RunRecord, RunState};
use agent_studios_protocol::studio::Studio;
use agent_studios_protocol::task::{BatchTaskSpec, TaskRecord, TaskState};
use chrono::{DateTime, Utc};

use crate::clock::{Clock, SystemClock};
use crate::error::{ControlPlaneError, ReplayError, StoreError, TaskGraphError};
use crate::store::{EventStore, InMemoryStore};
use crate::task_graph::TaskGraph;

type CancellationTargets = (Vec<TaskId>, Vec<RunId>, Vec<ApprovalId>);

/// Encapsulates the entire mutable domain state of the ControlPlane.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlPlaneState {
    pub studios: HashMap<StudioId, Studio>,
    pub agents: HashMap<AgentId, AgentDescriptor>,
    pub task_graph: TaskGraph,
    pub runs: HashMap<RunId, RunRecord>,
    pub approvals: HashMap<ApprovalId, ApprovalRequest>,
    pub artifacts: HashMap<ArtifactId, ArtifactRecord>,

    // Auxiliary indices for fast query resolution
    pub runs_by_task: HashMap<TaskId, Vec<RunId>>,
    pub tasks_by_studio: HashMap<StudioId, HashSet<TaskId>>,
    pub agents_by_studio: HashMap<StudioId, HashSet<AgentId>>,
}

impl ControlPlaneState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Strictly applies an event envelope to in-memory domain state.
    /// Validates all entity existence, studio ownership, and state machines.
    pub fn apply_event(&mut self, envelope: &EventEnvelope) -> Result<(), ReplayError> {
        let now = envelope.timestamp;

        match &envelope.event {
            ControlPlaneEvent::StudioCreated { studio } => {
                if envelope.studio_id != studio.id {
                    return Err(ReplayError::StudioMismatch {
                        expected: studio.id,
                        actual: envelope.studio_id,
                    });
                }
                if self.studios.contains_key(&studio.id) {
                    return Err(ReplayError::DuplicateStudio {
                        studio_id: studio.id,
                    });
                }
                self.studios.insert(studio.id, studio.clone());
                self.tasks_by_studio.entry(studio.id).or_default();
                self.agents_by_studio.entry(studio.id).or_default();
            }

            ControlPlaneEvent::AgentRegistered { agent } => {
                if envelope.studio_id != agent.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: agent.studio_id,
                        actual: envelope.studio_id,
                    });
                }
                if !self.studios.contains_key(&agent.studio_id) {
                    return Err(ReplayError::StudioNotFound {
                        studio_id: agent.studio_id,
                    });
                }
                if self.agents.contains_key(&agent.id) {
                    return Err(ReplayError::DuplicateAgent { agent_id: agent.id });
                }
                self.agents.insert(agent.id, agent.clone());
                self.agents_by_studio
                    .entry(agent.studio_id)
                    .or_default()
                    .insert(agent.id);
            }

            ControlPlaneEvent::AgentStateChanged {
                agent_id,
                previous_state,
                new_state,
            } => {
                let agent = self
                    .agents
                    .get_mut(agent_id)
                    .ok_or(ReplayError::AgentNotFound {
                        agent_id: *agent_id,
                    })?;

                if envelope.studio_id != agent.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: agent.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                if agent.state != *previous_state {
                    return Err(ReplayError::StateMismatch {
                        actual: format!("{:?}", agent.state),
                        expected: format!("{previous_state:?}"),
                    });
                }

                previous_state
                    .validate_transition_to(*new_state)
                    .map_err(|e| ReplayError::InvalidTransition {
                        reason: e.to_string(),
                    })?;

                agent.state = *new_state;
            }

            ControlPlaneEvent::TaskCreated { task } => {
                if envelope.studio_id != task.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: task.studio_id,
                        actual: envelope.studio_id,
                    });
                }
                if !self.studios.contains_key(&task.studio_id) {
                    return Err(ReplayError::StudioNotFound {
                        studio_id: task.studio_id,
                    });
                }

                if let Some(parent_id) = task.parent_task_id {
                    let parent = self
                        .task_graph
                        .get_task(parent_id)
                        .ok_or(ReplayError::TaskNotFound { task_id: parent_id })?;
                    if parent.studio_id != task.studio_id {
                        return Err(ReplayError::StudioMismatch {
                            expected: task.studio_id,
                            actual: parent.studio_id,
                        });
                    }
                }

                if let Some(agent_id) = task.assigned_agent_id {
                    let agent = self
                        .agents
                        .get(&agent_id)
                        .ok_or(ReplayError::AgentNotFound { agent_id })?;
                    if agent.studio_id != task.studio_id {
                        return Err(ReplayError::StudioMismatch {
                            expected: task.studio_id,
                            actual: agent.studio_id,
                        });
                    }
                }

                for &dep_id in &task.dependencies {
                    let dep = self
                        .task_graph
                        .get_task(dep_id)
                        .ok_or(ReplayError::TaskNotFound { task_id: dep_id })?;
                    if dep.studio_id != task.studio_id {
                        return Err(ReplayError::StudioMismatch {
                            expected: task.studio_id,
                            actual: dep.studio_id,
                        });
                    }
                }

                if self.task_graph.get_task(task.id).is_some() {
                    return Err(ReplayError::DuplicateTask { task_id: task.id });
                }

                self.task_graph
                    .add_task(task.clone())
                    .map_err(|e| match e {
                        TaskGraphError::DuplicateTask(task_id) => {
                            ReplayError::DuplicateTask { task_id }
                        }
                        TaskGraphError::DependencyCycle { from, to } => {
                            ReplayError::DependencyCycle { from, to }
                        }
                        other => ReplayError::DomainViolation(other.to_string()),
                    })?;

                self.tasks_by_studio
                    .entry(task.studio_id)
                    .or_default()
                    .insert(task.id);
            }

            ControlPlaneEvent::TaskStateChanged {
                task_id,
                previous_state,
                new_state,
            } => {
                let task = self
                    .task_graph
                    .get_task_mut(*task_id)
                    .ok_or(ReplayError::TaskNotFound { task_id: *task_id })?;

                if envelope.studio_id != task.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: task.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                if task.state != *previous_state {
                    return Err(ReplayError::StateMismatch {
                        actual: format!("{:?}", task.state),
                        expected: format!("{previous_state:?}"),
                    });
                }

                previous_state
                    .validate_transition_to(*new_state)
                    .map_err(|e| ReplayError::InvalidTransition {
                        reason: e.to_string(),
                    })?;

                task.state = *new_state;
                task.updated_at = now;
            }

            ControlPlaneEvent::TaskDependencyAdded {
                task_id,
                dependency_id,
            } => {
                let task = self
                    .task_graph
                    .get_task(*task_id)
                    .ok_or(ReplayError::TaskNotFound { task_id: *task_id })?;

                if envelope.studio_id != task.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: task.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                let dep =
                    self.task_graph
                        .get_task(*dependency_id)
                        .ok_or(ReplayError::TaskNotFound {
                            task_id: *dependency_id,
                        })?;

                if dep.studio_id != task.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: task.studio_id,
                        actual: dep.studio_id,
                    });
                }

                if !task.state.can_mutate_dependencies() {
                    return Err(ReplayError::DomainViolation(format!(
                        "Task {} in state {:?} cannot mutate dependencies",
                        task_id, task.state
                    )));
                }

                self.task_graph
                    .add_dependency(*task_id, *dependency_id)
                    .map_err(|e| match e {
                        TaskGraphError::DependencyCycle { from, to } => {
                            ReplayError::DependencyCycle { from, to }
                        }
                        other => ReplayError::DomainViolation(other.to_string()),
                    })?;
            }

            ControlPlaneEvent::TaskDependencyRemoved {
                task_id,
                dependency_id,
            } => {
                let task = self
                    .task_graph
                    .get_task(*task_id)
                    .ok_or(ReplayError::TaskNotFound { task_id: *task_id })?;

                if envelope.studio_id != task.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: task.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                let dep =
                    self.task_graph
                        .get_task(*dependency_id)
                        .ok_or(ReplayError::TaskNotFound {
                            task_id: *dependency_id,
                        })?;

                if dep.studio_id != task.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: task.studio_id,
                        actual: dep.studio_id,
                    });
                }

                if !task.state.can_mutate_dependencies() {
                    return Err(ReplayError::DomainViolation(format!(
                        "Task {} in state {:?} cannot mutate dependencies",
                        task_id, task.state
                    )));
                }

                self.task_graph
                    .remove_dependency(*task_id, *dependency_id)
                    .map_err(|e| ReplayError::DomainViolation(e.to_string()))?;
            }

            ControlPlaneEvent::RunCreated { run } => {
                let task =
                    self.task_graph
                        .get_task(run.task_id)
                        .ok_or(ReplayError::TaskNotFound {
                            task_id: run.task_id,
                        })?;

                if envelope.studio_id != task.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: task.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                let agent = self
                    .agents
                    .get(&run.agent_id)
                    .ok_or(ReplayError::AgentNotFound {
                        agent_id: run.agent_id,
                    })?;

                if agent.studio_id != task.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: task.studio_id,
                        actual: agent.studio_id,
                    });
                }

                if self.runs.contains_key(&run.id) {
                    return Err(ReplayError::DuplicateRun { run_id: run.id });
                }

                self.runs.insert(run.id, run.clone());
                self.runs_by_task
                    .entry(run.task_id)
                    .or_default()
                    .push(run.id);
            }

            ControlPlaneEvent::RunStateChanged {
                run_id,
                previous_state,
                new_state,
            } => {
                let run = self
                    .runs
                    .get_mut(run_id)
                    .ok_or(ReplayError::RunNotFound { run_id: *run_id })?;

                let task =
                    self.task_graph
                        .get_task(run.task_id)
                        .ok_or(ReplayError::TaskNotFound {
                            task_id: run.task_id,
                        })?;

                if envelope.studio_id != task.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: task.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                if run.state != *previous_state {
                    return Err(ReplayError::StateMismatch {
                        actual: format!("{:?}", run.state),
                        expected: format!("{previous_state:?}"),
                    });
                }

                previous_state
                    .validate_transition_to(*new_state)
                    .map_err(|e| ReplayError::InvalidTransition {
                        reason: e.to_string(),
                    })?;

                run.state = *new_state;
                if *new_state == RunState::Running && run.started_at.is_none() {
                    run.started_at = Some(now);
                } else if new_state.is_terminal() && run.finished_at.is_none() {
                    run.finished_at = Some(now);
                }
            }

            ControlPlaneEvent::ApprovalRequested { approval } => {
                if envelope.studio_id != approval.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: approval.studio_id,
                        actual: envelope.studio_id,
                    });
                }
                if !self.studios.contains_key(&approval.studio_id) {
                    return Err(ReplayError::StudioNotFound {
                        studio_id: approval.studio_id,
                    });
                }

                let task = self.task_graph.get_task(approval.task_id).ok_or(
                    ReplayError::TaskNotFound {
                        task_id: approval.task_id,
                    },
                )?;
                if task.studio_id != approval.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: approval.studio_id,
                        actual: task.studio_id,
                    });
                }

                let agent =
                    self.agents
                        .get(&approval.agent_id)
                        .ok_or(ReplayError::AgentNotFound {
                            agent_id: approval.agent_id,
                        })?;
                if agent.studio_id != approval.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: approval.studio_id,
                        actual: agent.studio_id,
                    });
                }

                if self.approvals.contains_key(&approval.id) {
                    return Err(ReplayError::DuplicateApproval {
                        approval_id: approval.id,
                    });
                }

                self.approvals.insert(approval.id, approval.clone());
            }

            ControlPlaneEvent::ApprovalResolved {
                approval_id,
                previous_state,
                new_state,
            } => {
                let approval =
                    self.approvals
                        .get_mut(approval_id)
                        .ok_or(ReplayError::ApprovalNotFound {
                            approval_id: *approval_id,
                        })?;

                if envelope.studio_id != approval.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: approval.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                if approval.state != *previous_state {
                    return Err(ReplayError::StateMismatch {
                        actual: format!("{:?}", approval.state),
                        expected: format!("{previous_state:?}"),
                    });
                }

                previous_state
                    .validate_transition_to(*new_state)
                    .map_err(|e| ReplayError::InvalidTransition {
                        reason: e.to_string(),
                    })?;

                approval.state = *new_state;
                approval.resolved_at = Some(now);
            }

            ControlPlaneEvent::ArtifactRegistered { artifact } => {
                if envelope.studio_id != artifact.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: artifact.studio_id,
                        actual: envelope.studio_id,
                    });
                }
                if !self.studios.contains_key(&artifact.studio_id) {
                    return Err(ReplayError::StudioNotFound {
                        studio_id: artifact.studio_id,
                    });
                }

                let task = self.task_graph.get_task(artifact.task_id).ok_or(
                    ReplayError::TaskNotFound {
                        task_id: artifact.task_id,
                    },
                )?;
                if task.studio_id != artifact.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: artifact.studio_id,
                        actual: task.studio_id,
                    });
                }

                let agent = self.agents.get(&artifact.producer_agent_id).ok_or(
                    ReplayError::AgentNotFound {
                        agent_id: artifact.producer_agent_id,
                    },
                )?;
                if agent.studio_id != artifact.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: artifact.studio_id,
                        actual: agent.studio_id,
                    });
                }

                if self.artifacts.contains_key(&artifact.id) {
                    return Err(ReplayError::DuplicateArtifact {
                        artifact_id: artifact.id,
                    });
                }

                self.artifacts.insert(artifact.id, artifact.clone());
            }

            ControlPlaneEvent::CancellationRequested { scope, .. } => match scope {
                CancellationScope::Studio(sid) => {
                    if envelope.studio_id != *sid {
                        return Err(ReplayError::StudioMismatch {
                            expected: *sid,
                            actual: envelope.studio_id,
                        });
                    }
                    if !self.studios.contains_key(sid) {
                        return Err(ReplayError::StudioNotFound { studio_id: *sid });
                    }
                }
                CancellationScope::Task { task_id, .. } => {
                    let t = self
                        .task_graph
                        .get_task(*task_id)
                        .ok_or(ReplayError::TaskNotFound { task_id: *task_id })?;
                    if envelope.studio_id != t.studio_id {
                        return Err(ReplayError::StudioMismatch {
                            expected: t.studio_id,
                            actual: envelope.studio_id,
                        });
                    }
                }
                CancellationScope::Agent(aid) => {
                    let a = self
                        .agents
                        .get(aid)
                        .ok_or(ReplayError::AgentNotFound { agent_id: *aid })?;
                    if envelope.studio_id != a.studio_id {
                        return Err(ReplayError::StudioMismatch {
                            expected: a.studio_id,
                            actual: envelope.studio_id,
                        });
                    }
                }
                CancellationScope::Run(rid) => {
                    let r = self
                        .runs
                        .get(rid)
                        .ok_or(ReplayError::RunNotFound { run_id: *rid })?;
                    let t = self
                        .task_graph
                        .get_task(r.task_id)
                        .ok_or(ReplayError::TaskNotFound { task_id: r.task_id })?;
                    if envelope.studio_id != t.studio_id {
                        return Err(ReplayError::StudioMismatch {
                            expected: t.studio_id,
                            actual: envelope.studio_id,
                        });
                    }
                }
            },

            ControlPlaneEvent::AgentRuntimeBound { agent_id, .. } => {
                let agent = self
                    .agents
                    .get(agent_id)
                    .ok_or(ReplayError::AgentNotFound {
                        agent_id: *agent_id,
                    })?;
                if envelope.studio_id != agent.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: agent.studio_id,
                        actual: envelope.studio_id,
                    });
                }
            }

            ControlPlaneEvent::AgentSpawned {
                agent_id,
                parent_agent_id,
                ..
            } => {
                let agent = self
                    .agents
                    .get(agent_id)
                    .ok_or(ReplayError::AgentNotFound {
                        agent_id: *agent_id,
                    })?;
                if envelope.studio_id != agent.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: agent.studio_id,
                        actual: envelope.studio_id,
                    });
                }
                if let Some(parent_id) = parent_agent_id {
                    let parent = self
                        .agents
                        .get(parent_id)
                        .ok_or(ReplayError::AgentNotFound {
                            agent_id: *parent_id,
                        })?;
                    if envelope.studio_id != parent.studio_id {
                        return Err(ReplayError::StudioMismatch {
                            expected: parent.studio_id,
                            actual: envelope.studio_id,
                        });
                    }
                }
            }

            ControlPlaneEvent::ToolStarted {
                agent_id,
                task_id,
                run_id,
                ..
            }
            | ControlPlaneEvent::ToolCompleted {
                agent_id,
                task_id,
                run_id,
                ..
            }
            | ControlPlaneEvent::ToolFailed {
                agent_id,
                task_id,
                run_id,
                ..
            } => {
                let agent = self
                    .agents
                    .get(agent_id)
                    .ok_or(ReplayError::AgentNotFound {
                        agent_id: *agent_id,
                    })?;
                if envelope.studio_id != agent.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: agent.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                if let Some(tid) = task_id {
                    let task = self
                        .task_graph
                        .get_task(*tid)
                        .ok_or(ReplayError::TaskNotFound { task_id: *tid })?;
                    if envelope.studio_id != task.studio_id {
                        return Err(ReplayError::StudioMismatch {
                            expected: task.studio_id,
                            actual: envelope.studio_id,
                        });
                    }
                }

                if let Some(rid) = run_id {
                    let run = self
                        .runs
                        .get(rid)
                        .ok_or(ReplayError::RunNotFound { run_id: *rid })?;
                    if run.agent_id != *agent_id {
                        return Err(ReplayError::DomainViolation(format!(
                            "Tool event agent_id {agent_id} does not match Run.agent_id {}",
                            run.agent_id
                        )));
                    }
                    if let Some(tid) = task_id
                        && run.task_id != *tid
                    {
                        return Err(ReplayError::DomainViolation(format!(
                            "Tool event task_id {tid} does not match Run.task_id {}",
                            run.task_id
                        )));
                    }
                }
            }

            ControlPlaneEvent::BudgetUsageUpdated {
                agent_id, run_id, ..
            }
            | ControlPlaneEvent::BudgetExceeded {
                agent_id, run_id, ..
            } => {
                let agent = self
                    .agents
                    .get(agent_id)
                    .ok_or(ReplayError::AgentNotFound {
                        agent_id: *agent_id,
                    })?;
                if envelope.studio_id != agent.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: agent.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                if let Some(rid) = run_id {
                    let run = self
                        .runs
                        .get(rid)
                        .ok_or(ReplayError::RunNotFound { run_id: *rid })?;
                    if run.agent_id != *agent_id {
                        return Err(ReplayError::DomainViolation(format!(
                            "Budget event agent_id {agent_id} does not match Run.agent_id {}",
                            run.agent_id
                        )));
                    }
                }
            }

            ControlPlaneEvent::RunOutcomeRecorded {
                run_id,
                task_id,
                agent_id,
                ..
            } => {
                let agent = self
                    .agents
                    .get(agent_id)
                    .ok_or(ReplayError::AgentNotFound {
                        agent_id: *agent_id,
                    })?;
                if envelope.studio_id != agent.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: agent.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                let task = self
                    .task_graph
                    .get_task(*task_id)
                    .ok_or(ReplayError::TaskNotFound { task_id: *task_id })?;
                if envelope.studio_id != task.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: task.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                let run = self
                    .runs
                    .get(run_id)
                    .ok_or(ReplayError::RunNotFound { run_id: *run_id })?;
                if run.task_id != *task_id {
                    return Err(ReplayError::DomainViolation(format!(
                        "RunOutcomeRecorded task_id {task_id} does not match Run.task_id {}",
                        run.task_id
                    )));
                }
                if run.agent_id != *agent_id {
                    return Err(ReplayError::DomainViolation(format!(
                        "RunOutcomeRecorded agent_id {agent_id} does not match Run.agent_id {}",
                        run.agent_id
                    )));
                }
            }

            ControlPlaneEvent::TaskRetryScheduled {
                task_id, attempt, ..
            } => {
                let task = self
                    .task_graph
                    .get_task_mut(*task_id)
                    .ok_or(ReplayError::TaskNotFound { task_id: *task_id })?;

                if envelope.studio_id != task.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: task.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                task.retry_count = *attempt;
                task.updated_at = now;
            }
        }

        Ok(())
    }
}

/// Core deterministic control plane state engine.
pub struct ControlPlane<C: Clock = SystemClock, S: EventStore = InMemoryStore> {
    clock: C,
    store: S,
    state: ControlPlaneState,
    committed_events_buffer: Vec<EventEnvelope>,
}

impl Default for ControlPlane<SystemClock, InMemoryStore> {
    fn default() -> Self {
        Self::new(SystemClock, InMemoryStore::new())
    }
}

impl ControlPlane<SystemClock, InMemoryStore> {
    /// Creates a control plane using system clock and in-memory event store.
    pub fn new_default() -> Self {
        Self::default()
    }
}

impl<C: Clock, S: EventStore> ControlPlane<C, S> {
    pub fn new(clock: C, store: S) -> Self {
        Self {
            clock,
            store,
            state: ControlPlaneState::new(),
            committed_events_buffer: Vec::new(),
        }
    }

    pub fn clock(&self) -> &C {
        &self.clock
    }

    pub fn drain_committed_events(&mut self) -> Vec<EventEnvelope> {
        std::mem::take(&mut self.committed_events_buffer)
    }

    /// Returns a read-only reference to the underlying event store for diagnostics.
    /// Direct mutable access to the store is intentionally disallowed to preserve
    /// the staged transaction invariant: all event persistence and state transitions
    /// must proceed atomically through ControlPlane command methods / `commit_transaction`.
    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn state(&self) -> &ControlPlaneState {
        &self.state
    }

    // ========================================================================
    // Staged Transaction Pipeline
    // ========================================================================

    /// Transaction pipeline:
    /// validate command -> construct candidate events -> assign sequences ->
    /// clone current state -> staged_state -> strict apply all events to staged_state ->
    /// EventStore::append_batch(events) -> commit self.state = staged_state.
    fn commit_transaction(
        &mut self,
        studio_id: StudioId,
        candidate_events: Vec<ControlPlaneEvent>,
    ) -> Result<Vec<EventEnvelope>, ControlPlaneError> {
        if candidate_events.is_empty() {
            return Ok(Vec::new());
        }

        let now = self.clock.now();
        let latest = self.store.latest_sequence(studio_id)?;

        // 1. Assign monotonic sequence numbers and construct envelopes
        let mut envelopes = Vec::with_capacity(candidate_events.len());
        let mut seq = latest;
        for event in candidate_events {
            seq += 1;
            envelopes.push(EventEnvelope::new(studio_id, seq, now, event));
        }

        // 2. Clone current state into staged_state
        let mut staged_state = self.state.clone();

        // 3. Strict apply all candidate events to staged_state
        for env in &envelopes {
            staged_state.apply_event(env)?;
        }

        // 4. Atomic append to durable event store (ALL or ZERO)
        self.store.append_batch(&envelopes)?;

        // 5. Commit state transition only after store persistence succeeds
        self.state = staged_state;
        self.committed_events_buffer.extend(envelopes.clone());

        Ok(envelopes)
    }

    // ========================================================================
    // Studio Management
    // ========================================================================

    pub fn create_studio(&mut self, name: impl Into<String>) -> Result<Studio, ControlPlaneError> {
        let now = self.clock.now();
        let studio = Studio::new(name, now);
        let event = ControlPlaneEvent::StudioCreated {
            studio: studio.clone(),
        };

        self.commit_transaction(studio.id, vec![event])?;
        Ok(studio)
    }

    pub fn get_studio(&self, id: StudioId) -> Option<&Studio> {
        self.state.studios.get(&id)
    }

    pub fn all_studios(&self) -> impl Iterator<Item = &Studio> {
        self.state.studios.values()
    }

    // ========================================================================
    // Agent Management
    // ========================================================================

    pub fn register_agent(
        &mut self,
        studio_id: StudioId,
        display_name: impl Into<String>,
        kind: AgentKind,
        role: Option<String>,
    ) -> Result<AgentDescriptor, ControlPlaneError> {
        self.register_agent_with_id(studio_id, AgentId::new(), display_name, kind, role)
    }

    pub fn register_agent_with_id(
        &mut self,
        studio_id: StudioId,
        agent_id: AgentId,
        display_name: impl Into<String>,
        kind: AgentKind,
        role: Option<String>,
    ) -> Result<AgentDescriptor, ControlPlaneError> {
        if !self.state.studios.contains_key(&studio_id) {
            return Err(ControlPlaneError::StudioNotFound(studio_id));
        }

        let agent = AgentDescriptor {
            id: agent_id,
            studio_id,
            display_name: display_name.into(),
            kind,
            state: AgentState::Registered,
            role,
        };
        let event = ControlPlaneEvent::AgentRegistered {
            agent: agent.clone(),
        };

        self.commit_transaction(studio_id, vec![event])?;
        Ok(agent)
    }

    pub fn register_agent_batch(
        &mut self,
        studio_id: StudioId,
        specs: Vec<BatchAgentSpec>,
    ) -> Result<Vec<AgentDescriptor>, ControlPlaneError> {
        if !self.state.studios.contains_key(&studio_id) {
            return Err(ControlPlaneError::StudioNotFound(studio_id));
        }

        let mut seen = HashSet::with_capacity(specs.len());
        for spec in &specs {
            if !seen.insert(spec.id) {
                return Err(ControlPlaneError::DuplicateAgent(spec.id));
            }
            if self.state.agents.contains_key(&spec.id) {
                return Err(ControlPlaneError::DuplicateAgent(spec.id));
            }
        }

        let mut descriptors = Vec::with_capacity(specs.len());
        let mut events = Vec::with_capacity(specs.len());

        for spec in specs {
            let descriptor = AgentDescriptor {
                id: spec.id,
                studio_id,
                display_name: spec.display_name,
                kind: spec.kind,
                state: AgentState::Registered,
                role: spec.role,
            };
            events.push(ControlPlaneEvent::AgentRegistered {
                agent: descriptor.clone(),
            });
            descriptors.push(descriptor);
        }

        self.commit_transaction(studio_id, events)?;
        Ok(descriptors)
    }

    pub fn update_agent_state(
        &mut self,
        agent_id: AgentId,
        new_state: AgentState,
    ) -> Result<(), ControlPlaneError> {
        let agent = self
            .state
            .agents
            .get(&agent_id)
            .ok_or(ControlPlaneError::AgentNotFound(agent_id))?;

        if agent.state == new_state {
            return Ok(());
        }

        agent.state.validate_transition_to(new_state)?;

        let previous_state = agent.state;
        let studio_id = agent.studio_id;

        let event = ControlPlaneEvent::AgentStateChanged {
            agent_id,
            previous_state,
            new_state,
        };

        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn get_agent(&self, id: AgentId) -> Option<&AgentDescriptor> {
        self.state.agents.get(&id)
    }

    pub fn all_agents(&self) -> impl Iterator<Item = &AgentDescriptor> {
        self.state.agents.values()
    }

    // ========================================================================
    // Task Management
    // ========================================================================

    pub fn create_task(
        &mut self,
        studio_id: StudioId,
        title: impl Into<String>,
        description: impl Into<String>,
        parent_task_id: Option<TaskId>,
        assigned_agent_id: Option<AgentId>,
        dependencies: Vec<TaskId>,
    ) -> Result<TaskRecord, ControlPlaneError> {
        if !self.state.studios.contains_key(&studio_id) {
            return Err(ControlPlaneError::StudioNotFound(studio_id));
        }

        if let Some(parent_id) = parent_task_id {
            let parent = self
                .state
                .task_graph
                .get_task(parent_id)
                .ok_or(ControlPlaneError::TaskNotFound(parent_id))?;
            if parent.studio_id != studio_id {
                return Err(ControlPlaneError::StudioMismatch {
                    expected: studio_id,
                    actual: parent.studio_id,
                });
            }
        }

        if let Some(agent_id) = assigned_agent_id {
            let agent = self
                .state
                .agents
                .get(&agent_id)
                .ok_or(ControlPlaneError::AgentNotFound(agent_id))?;
            if agent.studio_id != studio_id {
                return Err(ControlPlaneError::StudioMismatch {
                    expected: studio_id,
                    actual: agent.studio_id,
                });
            }
        }

        for &dep_id in &dependencies {
            let dep_task = self
                .state
                .task_graph
                .get_task(dep_id)
                .ok_or(ControlPlaneError::TaskNotFound(dep_id))?;
            if dep_task.studio_id != studio_id {
                return Err(ControlPlaneError::StudioMismatch {
                    expected: studio_id,
                    actual: dep_task.studio_id,
                });
            }
        }

        let now = self.clock.now();
        let mut task = TaskRecord::new(
            studio_id,
            title,
            description,
            parent_task_id,
            assigned_agent_id,
            dependencies,
            now,
        );

        // Compute correct initial state based on dependencies
        let all_deps_succeeded = task.dependencies.iter().all(|&dep_id| {
            self.state
                .task_graph
                .get_task(dep_id)
                .map(|t| t.state == TaskState::Succeeded)
                .unwrap_or(false)
        });

        task.state = if task.dependencies.is_empty() || all_deps_succeeded {
            TaskState::Ready
        } else {
            TaskState::Blocked
        };

        let event = ControlPlaneEvent::TaskCreated { task: task.clone() };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(task)
    }

    pub fn create_task_batch(
        &mut self,
        studio_id: StudioId,
        batch: Vec<BatchTaskSpec>,
    ) -> Result<Vec<TaskRecord>, ControlPlaneError> {
        if !self.state.studios.contains_key(&studio_id) {
            return Err(ControlPlaneError::StudioNotFound(studio_id));
        }

        if batch.is_empty() {
            return Ok(Vec::new());
        }

        let mut key_to_id: HashMap<String, TaskId> = HashMap::with_capacity(batch.len());
        let mut key_to_index: HashMap<String, usize> = HashMap::with_capacity(batch.len());

        for (idx, spec) in batch.iter().enumerate() {
            if spec.key.is_empty() {
                return Err(ControlPlaneError::InvalidOperation(
                    "Batch task key cannot be empty".to_string(),
                ));
            }
            if key_to_id.contains_key(&spec.key) {
                return Err(ControlPlaneError::InvalidOperation(format!(
                    "Duplicate batch task key '{}'",
                    spec.key
                )));
            }
            key_to_id.insert(spec.key.clone(), TaskId::new());
            key_to_index.insert(spec.key.clone(), idx);
        }

        let now = self.clock.now();
        let mut created_tasks = Vec::with_capacity(batch.len());
        let mut candidate_events = Vec::with_capacity(batch.len());

        for (idx, spec) in batch.iter().enumerate() {
            let task_id = *key_to_id.get(&spec.key).unwrap();

            let parent_task_id = if let Some(parent_key) = &spec.parent_task_key {
                let parent_idx = key_to_index.get(parent_key).ok_or_else(|| {
                    ControlPlaneError::InvalidOperation(format!(
                        "Unknown parent_task_key '{parent_key}' in batch"
                    ))
                })?;
                if *parent_idx >= idx {
                    return Err(ControlPlaneError::InvalidOperation(format!(
                        "Forward reference to parent_task_key '{parent_key}' is not allowed"
                    )));
                }
                let resolved_parent_id = *key_to_id.get(parent_key).unwrap();
                if let Some(explicit_id) = spec.parent_task_id
                    && explicit_id != resolved_parent_id
                {
                    return Err(ControlPlaneError::InvalidOperation(format!(
                        "Mismatched parent_task_id and parent_task_key for task '{}'",
                        spec.key
                    )));
                }
                Some(resolved_parent_id)
            } else if let Some(parent_id) = spec.parent_task_id {
                let parent = self
                    .state
                    .task_graph
                    .get_task(parent_id)
                    .ok_or(ControlPlaneError::TaskNotFound(parent_id))?;
                if parent.studio_id != studio_id {
                    return Err(ControlPlaneError::StudioMismatch {
                        expected: studio_id,
                        actual: parent.studio_id,
                    });
                }
                Some(parent_id)
            } else {
                None
            };

            if let Some(agent_id) = spec.assigned_agent_id {
                let agent = self
                    .state
                    .agents
                    .get(&agent_id)
                    .ok_or(ControlPlaneError::AgentNotFound(agent_id))?;
                if agent.studio_id != studio_id {
                    return Err(ControlPlaneError::StudioMismatch {
                        expected: studio_id,
                        actual: agent.studio_id,
                    });
                }
            }

            let mut dependencies = Vec::new();

            for &dep_id in &spec.dependency_task_ids {
                let dep_task = self
                    .state
                    .task_graph
                    .get_task(dep_id)
                    .ok_or(ControlPlaneError::TaskNotFound(dep_id))?;
                if dep_task.studio_id != studio_id {
                    return Err(ControlPlaneError::StudioMismatch {
                        expected: studio_id,
                        actual: dep_task.studio_id,
                    });
                }
                if !dependencies.contains(&dep_id) {
                    dependencies.push(dep_id);
                }
            }

            for dep_key in &spec.dependency_keys {
                let dep_idx = key_to_index.get(dep_key).ok_or_else(|| {
                    ControlPlaneError::InvalidOperation(format!(
                        "Unknown dependency_key '{dep_key}' in batch"
                    ))
                })?;
                if *dep_idx >= idx {
                    return Err(ControlPlaneError::InvalidOperation(format!(
                        "Forward reference to dependency_key '{dep_key}' is not allowed"
                    )));
                }
                let resolved_dep_id = *key_to_id.get(dep_key).unwrap();
                if !dependencies.contains(&resolved_dep_id) {
                    dependencies.push(resolved_dep_id);
                }
            }

            let all_deps_succeeded = dependencies.iter().all(|&dep_id| {
                self.state
                    .task_graph
                    .get_task(dep_id)
                    .map(|t| t.state == TaskState::Succeeded)
                    .unwrap_or(false)
            });

            let initial_state = if dependencies.is_empty() || all_deps_succeeded {
                TaskState::Ready
            } else {
                TaskState::Blocked
            };

            let task = TaskRecord {
                id: task_id,
                studio_id,
                parent_task_id,
                assigned_agent_id: spec.assigned_agent_id,
                title: spec.title.clone(),
                description: spec.description.clone(),
                state: initial_state,
                dependencies,
                created_at: now,
                updated_at: now,
                retry_count: 0,
            };

            candidate_events.push(ControlPlaneEvent::TaskCreated { task: task.clone() });
            created_tasks.push(task);
        }

        self.commit_transaction(studio_id, candidate_events)?;

        Ok(created_tasks)
    }

    pub fn transition_task_state(
        &mut self,
        task_id: TaskId,
        new_state: TaskState,
    ) -> Result<(), ControlPlaneError> {
        let task = self
            .state
            .task_graph
            .get_task(task_id)
            .ok_or(ControlPlaneError::TaskNotFound(task_id))?;

        if task.state == new_state {
            return Ok(());
        }

        task.state.validate_transition_to(new_state)?;

        let previous_state = task.state;
        let studio_id = task.studio_id;

        let mut candidate_events = vec![ControlPlaneEvent::TaskStateChanged {
            task_id,
            previous_state,
            new_state,
        }];

        // If transitioning to Succeeded, unblock dependents whose prerequisites are now all Succeeded
        if new_state == TaskState::Succeeded
            && let Ok(dependents) = self.state.task_graph.dependents_of(task_id)
        {
            for dep_id in dependents {
                if let Some(dep_task) = self.state.task_graph.get_task(dep_id)
                    && dep_task.state == TaskState::Blocked
                {
                    let all_deps_succeeded = dep_task.dependencies.iter().all(|&d_id| {
                        if d_id == task_id {
                            true
                        } else {
                            self.state
                                .task_graph
                                .get_task(d_id)
                                .map(|t| t.state == TaskState::Succeeded)
                                .unwrap_or(false)
                        }
                    });

                    if all_deps_succeeded {
                        candidate_events.push(ControlPlaneEvent::TaskStateChanged {
                            task_id: dep_id,
                            previous_state: TaskState::Blocked,
                            new_state: TaskState::Ready,
                        });
                    }
                }
            }
        }

        self.commit_transaction(studio_id, candidate_events)?;
        Ok(())
    }

    pub fn add_task_dependency(
        &mut self,
        task_id: TaskId,
        dependency_id: TaskId,
    ) -> Result<(), ControlPlaneError> {
        if task_id == dependency_id {
            return Err(ControlPlaneError::TaskGraph(
                TaskGraphError::SelfDependency(task_id),
            ));
        }

        let task = self
            .state
            .task_graph
            .get_task(task_id)
            .ok_or(ControlPlaneError::TaskNotFound(task_id))?;
        let dep = self
            .state
            .task_graph
            .get_task(dependency_id)
            .ok_or(ControlPlaneError::TaskNotFound(dependency_id))?;

        if task.studio_id != dep.studio_id {
            return Err(ControlPlaneError::StudioMismatch {
                expected: task.studio_id,
                actual: dep.studio_id,
            });
        }

        if !task.state.can_mutate_dependencies() {
            return Err(ControlPlaneError::Transition(
                TransitionError::TaskDependencyMutationForbidden {
                    task_id,
                    state: task.state,
                },
            ));
        }

        // Idempotent no-op: already has dependency
        if task.dependencies.contains(&dependency_id) {
            return Ok(());
        }

        let studio_id = task.studio_id;
        let mut candidate_events = vec![ControlPlaneEvent::TaskDependencyAdded {
            task_id,
            dependency_id,
        }];

        // If task is Ready and new dependency is unsatisfied, transition Ready -> Blocked in same atomic batch
        if task.state == TaskState::Ready && dep.state != TaskState::Succeeded {
            candidate_events.push(ControlPlaneEvent::TaskStateChanged {
                task_id,
                previous_state: TaskState::Ready,
                new_state: TaskState::Blocked,
            });
        }

        self.commit_transaction(studio_id, candidate_events)?;
        Ok(())
    }

    pub fn remove_task_dependency(
        &mut self,
        task_id: TaskId,
        dependency_id: TaskId,
    ) -> Result<(), ControlPlaneError> {
        let task = self
            .state
            .task_graph
            .get_task(task_id)
            .ok_or(ControlPlaneError::TaskNotFound(task_id))?;
        let dep = self
            .state
            .task_graph
            .get_task(dependency_id)
            .ok_or(ControlPlaneError::TaskNotFound(dependency_id))?;

        if task.studio_id != dep.studio_id {
            return Err(ControlPlaneError::StudioMismatch {
                expected: task.studio_id,
                actual: dep.studio_id,
            });
        }

        if !task.state.can_mutate_dependencies() {
            return Err(ControlPlaneError::Transition(
                TransitionError::TaskDependencyMutationForbidden {
                    task_id,
                    state: task.state,
                },
            ));
        }

        // Idempotent no-op: dependency not present
        if !task.dependencies.contains(&dependency_id) {
            return Ok(());
        }

        let studio_id = task.studio_id;
        let mut candidate_events = vec![ControlPlaneEvent::TaskDependencyRemoved {
            task_id,
            dependency_id,
        }];

        // If task is Blocked and removing this dependency satisfies all remaining, transition Blocked -> Ready in same batch
        if task.state == TaskState::Blocked {
            let all_remaining_succeeded = task
                .dependencies
                .iter()
                .filter(|&&id| id != dependency_id)
                .all(|&id| {
                    self.state
                        .task_graph
                        .get_task(id)
                        .map(|t| t.state == TaskState::Succeeded)
                        .unwrap_or(false)
                });

            if all_remaining_succeeded {
                candidate_events.push(ControlPlaneEvent::TaskStateChanged {
                    task_id,
                    previous_state: TaskState::Blocked,
                    new_state: TaskState::Ready,
                });
            }
        }

        self.commit_transaction(studio_id, candidate_events)?;
        Ok(())
    }

    pub fn is_task_ready(&self, task_id: TaskId) -> Result<bool, ControlPlaneError> {
        Ok(self.state.task_graph.is_ready(task_id)?)
    }

    pub fn get_task(&self, id: TaskId) -> Option<&TaskRecord> {
        self.state.task_graph.get_task(id)
    }

    pub fn all_tasks(&self) -> impl Iterator<Item = &TaskRecord> {
        self.state.task_graph.all_tasks()
    }

    pub fn task_graph(&self) -> &TaskGraph {
        &self.state.task_graph
    }

    // ========================================================================
    // Run Management
    // ========================================================================

    pub fn create_run(
        &mut self,
        task_id: TaskId,
        agent_id: AgentId,
    ) -> Result<RunRecord, ControlPlaneError> {
        let task = self
            .state
            .task_graph
            .get_task(task_id)
            .ok_or(ControlPlaneError::TaskNotFound(task_id))?;
        let agent = self
            .state
            .agents
            .get(&agent_id)
            .ok_or(ControlPlaneError::AgentNotFound(agent_id))?;

        if task.studio_id != agent.studio_id {
            return Err(ControlPlaneError::StudioMismatch {
                expected: task.studio_id,
                actual: agent.studio_id,
            });
        }

        let studio_id = task.studio_id;
        let attempt = self
            .state
            .runs_by_task
            .get(&task_id)
            .map(|r| r.len() as u32)
            .unwrap_or(0)
            + 1;

        let run = RunRecord::new(task_id, agent_id, attempt);
        let event = ControlPlaneEvent::RunCreated { run: run.clone() };

        self.commit_transaction(studio_id, vec![event])?;
        Ok(run)
    }

    pub fn transition_run_state(
        &mut self,
        run_id: RunId,
        new_state: RunState,
    ) -> Result<(), ControlPlaneError> {
        let run = self
            .state
            .runs
            .get(&run_id)
            .ok_or(ControlPlaneError::RunNotFound(run_id))?;

        if run.state == new_state {
            return Ok(());
        }

        run.state.validate_transition_to(new_state)?;

        let previous_state = run.state;
        let task = self
            .state
            .task_graph
            .get_task(run.task_id)
            .ok_or(ControlPlaneError::TaskNotFound(run.task_id))?;
        let studio_id = task.studio_id;

        let event = ControlPlaneEvent::RunStateChanged {
            run_id,
            previous_state,
            new_state,
        };

        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn record_run_outcome(
        &mut self,
        run_id: RunId,
        classification: impl Into<String>,
        safe_error_summary: Option<String>,
    ) -> Result<(), ControlPlaneError> {
        let run = self
            .state
            .runs
            .get(&run_id)
            .ok_or(ControlPlaneError::RunNotFound(run_id))?;

        let task = self
            .state
            .task_graph
            .get_task(run.task_id)
            .ok_or(ControlPlaneError::TaskNotFound(run.task_id))?;
        let studio_id = task.studio_id;

        let event = ControlPlaneEvent::RunOutcomeRecorded {
            run_id,
            task_id: run.task_id,
            agent_id: run.agent_id,
            classification: classification.into(),
            safe_error_summary,
        };

        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn get_run(&self, id: RunId) -> Option<&RunRecord> {
        self.state.runs.get(&id)
    }

    pub fn all_runs(&self) -> impl Iterator<Item = &RunRecord> {
        self.state.runs.values()
    }

    // ========================================================================
    // Approval Management
    // ========================================================================

    pub fn request_approval(
        &mut self,
        studio_id: StudioId,
        task_id: TaskId,
        agent_id: AgentId,
        kind: ApprovalKind,
        summary: impl Into<String>,
    ) -> Result<ApprovalRequest, ControlPlaneError> {
        if !self.state.studios.contains_key(&studio_id) {
            return Err(ControlPlaneError::StudioNotFound(studio_id));
        }

        let task = self
            .state
            .task_graph
            .get_task(task_id)
            .ok_or(ControlPlaneError::TaskNotFound(task_id))?;
        if task.studio_id != studio_id {
            return Err(ControlPlaneError::StudioMismatch {
                expected: studio_id,
                actual: task.studio_id,
            });
        }

        let agent = self
            .state
            .agents
            .get(&agent_id)
            .ok_or(ControlPlaneError::AgentNotFound(agent_id))?;
        if agent.studio_id != studio_id {
            return Err(ControlPlaneError::StudioMismatch {
                expected: studio_id,
                actual: agent.studio_id,
            });
        }

        let now = self.clock.now();
        let approval = ApprovalRequest::new(studio_id, task_id, agent_id, kind, summary, now);
        let event = ControlPlaneEvent::ApprovalRequested {
            approval: approval.clone(),
        };

        self.commit_transaction(studio_id, vec![event])?;
        Ok(approval)
    }

    pub fn resolve_approval(
        &mut self,
        approval_id: ApprovalId,
        new_state: ApprovalState,
    ) -> Result<(), ControlPlaneError> {
        let approval = self
            .state
            .approvals
            .get(&approval_id)
            .ok_or(ControlPlaneError::ApprovalNotFound(approval_id))?;

        approval.state.validate_transition_to(new_state)?;

        let previous_state = approval.state;
        let studio_id = approval.studio_id;

        let event = ControlPlaneEvent::ApprovalResolved {
            approval_id,
            previous_state,
            new_state,
        };

        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn get_approval(&self, id: ApprovalId) -> Option<&ApprovalRequest> {
        self.state.approvals.get(&id)
    }

    pub fn all_approvals(&self) -> impl Iterator<Item = &ApprovalRequest> {
        self.state.approvals.values()
    }

    // ========================================================================
    // Artifact Management
    // ========================================================================

    #[allow(clippy::too_many_arguments)]
    pub fn register_artifact(
        &mut self,
        studio_id: StudioId,
        task_id: TaskId,
        producer_agent_id: AgentId,
        kind: ArtifactKind,
        logical_name: impl Into<String>,
        content_hash: Option<String>,
        location: impl Into<String>,
    ) -> Result<ArtifactRecord, ControlPlaneError> {
        if !self.state.studios.contains_key(&studio_id) {
            return Err(ControlPlaneError::StudioNotFound(studio_id));
        }

        let task = self
            .state
            .task_graph
            .get_task(task_id)
            .ok_or(ControlPlaneError::TaskNotFound(task_id))?;
        if task.studio_id != studio_id {
            return Err(ControlPlaneError::StudioMismatch {
                expected: studio_id,
                actual: task.studio_id,
            });
        }

        let agent = self
            .state
            .agents
            .get(&producer_agent_id)
            .ok_or(ControlPlaneError::AgentNotFound(producer_agent_id))?;
        if agent.studio_id != studio_id {
            return Err(ControlPlaneError::StudioMismatch {
                expected: studio_id,
                actual: agent.studio_id,
            });
        }

        let now = self.clock.now();
        let artifact = ArtifactRecord::new(
            studio_id,
            task_id,
            producer_agent_id,
            kind,
            logical_name,
            content_hash,
            location,
            now,
        );

        let event = ControlPlaneEvent::ArtifactRegistered {
            artifact: artifact.clone(),
        };

        self.commit_transaction(studio_id, vec![event])?;
        Ok(artifact)
    }

    pub fn get_artifact(&self, id: ArtifactId) -> Option<&ArtifactRecord> {
        self.state.artifacts.get(&id)
    }

    pub fn all_artifacts(&self) -> impl Iterator<Item = &ArtifactRecord> {
        self.state.artifacts.values()
    }

    // ========================================================================
    // Orchestration & Observability Events
    // ========================================================================

    pub fn record_runtime_bound(
        &mut self,
        studio_id: StudioId,
        agent_id: AgentId,
        runtime_kind: impl Into<String>,
        provider_instance_id: impl Into<String>,
        model_id: impl Into<String>,
        protocol: impl Into<String>,
    ) -> Result<(), ControlPlaneError> {
        let event = ControlPlaneEvent::AgentRuntimeBound {
            agent_id,
            runtime_kind: runtime_kind.into(),
            provider_instance_id: provider_instance_id.into(),
            model_id: model_id.into(),
            protocol: protocol.into(),
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn record_agent_spawned(
        &mut self,
        studio_id: StudioId,
        agent_id: AgentId,
        thread_id: impl Into<String>,
        parent_agent_id: Option<AgentId>,
        parent_thread_id: Option<String>,
    ) -> Result<(), ControlPlaneError> {
        let event = ControlPlaneEvent::AgentSpawned {
            agent_id,
            thread_id: thread_id.into(),
            parent_agent_id,
            parent_thread_id,
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_tool_started(
        &mut self,
        studio_id: StudioId,
        agent_id: AgentId,
        task_id: Option<TaskId>,
        run_id: Option<RunId>,
        tool_name: impl Into<String>,
        call_id: impl Into<String>,
        timestamp: DateTime<Utc>,
    ) -> Result<(), ControlPlaneError> {
        let event = ControlPlaneEvent::ToolStarted {
            agent_id,
            task_id,
            run_id,
            tool_name: tool_name.into(),
            call_id: call_id.into(),
            timestamp,
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_tool_completed(
        &mut self,
        studio_id: StudioId,
        agent_id: AgentId,
        task_id: Option<TaskId>,
        run_id: Option<RunId>,
        tool_name: impl Into<String>,
        call_id: impl Into<String>,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    ) -> Result<(), ControlPlaneError> {
        let event = ControlPlaneEvent::ToolCompleted {
            agent_id,
            task_id,
            run_id,
            tool_name: tool_name.into(),
            call_id: call_id.into(),
            duration_ms,
            timestamp,
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_tool_failed(
        &mut self,
        studio_id: StudioId,
        agent_id: AgentId,
        task_id: Option<TaskId>,
        run_id: Option<RunId>,
        tool_name: impl Into<String>,
        call_id: impl Into<String>,
        error: impl Into<String>,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    ) -> Result<(), ControlPlaneError> {
        let event = ControlPlaneEvent::ToolFailed {
            agent_id,
            task_id,
            run_id,
            tool_name: tool_name.into(),
            call_id: call_id.into(),
            error: error.into(),
            duration_ms,
            timestamp,
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_budget_usage_updated(
        &mut self,
        studio_id: StudioId,
        agent_id: AgentId,
        run_id: Option<RunId>,
        turns_used: u32,
        tool_calls_used: u32,
        wall_clock_secs: u64,
        child_agents_used: u32,
    ) -> Result<(), ControlPlaneError> {
        let event = ControlPlaneEvent::BudgetUsageUpdated {
            agent_id,
            run_id,
            turns_used,
            tool_calls_used,
            wall_clock_secs,
            child_agents_used,
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn record_budget_exceeded(
        &mut self,
        studio_id: StudioId,
        agent_id: AgentId,
        run_id: Option<RunId>,
        dimension: impl Into<String>,
        limit: u64,
        actual: u64,
    ) -> Result<(), ControlPlaneError> {
        let event = ControlPlaneEvent::BudgetExceeded {
            agent_id,
            run_id,
            dimension: dimension.into(),
            limit,
            actual,
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn schedule_task_retry(
        &mut self,
        studio_id: StudioId,
        task_id: TaskId,
        attempt: u32,
        max_attempts: u32,
        reason: impl Into<String>,
        backoff_ms: u64,
        next_retry_at: Option<DateTime<Utc>>,
    ) -> Result<(), ControlPlaneError> {
        let event = ControlPlaneEvent::TaskRetryScheduled {
            task_id,
            attempt,
            max_attempts,
            reason: reason.into(),
            backoff_ms,
            next_retry_at,
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    // ========================================================================
    // Cancellation Engine
    // ========================================================================

    pub fn request_cancellation(
        &mut self,
        scope: CancellationScope,
        reason: Option<String>,
    ) -> Result<CancellationSummary, ControlPlaneError> {
        let studio_id = match &scope {
            CancellationScope::Studio(sid) => {
                if !self.state.studios.contains_key(sid) {
                    return Err(ControlPlaneError::StudioNotFound(*sid));
                }
                *sid
            }
            CancellationScope::Task { task_id, .. } => {
                let t = self
                    .state
                    .task_graph
                    .get_task(*task_id)
                    .ok_or(ControlPlaneError::TaskNotFound(*task_id))?;
                t.studio_id
            }
            CancellationScope::Agent(aid) => {
                let a = self
                    .state
                    .agents
                    .get(aid)
                    .ok_or(ControlPlaneError::AgentNotFound(*aid))?;
                a.studio_id
            }
            CancellationScope::Run(rid) => {
                let r = self
                    .state
                    .runs
                    .get(rid)
                    .ok_or(ControlPlaneError::RunNotFound(*rid))?;
                let t = self
                    .state
                    .task_graph
                    .get_task(r.task_id)
                    .ok_or(ControlPlaneError::TaskNotFound(r.task_id))?;
                t.studio_id
            }
        };

        // Determine targets to cancel
        let (tasks_to_cancel, runs_to_cancel, approvals_to_cancel) =
            self.calculate_cancellation_targets(&scope)?;

        // Build entire batch of cancellation candidate events
        let mut candidate_events = vec![ControlPlaneEvent::CancellationRequested { scope, reason }];
        let mut summary = CancellationSummary::default();

        // 1. Cancel active runs
        for run_id in runs_to_cancel {
            if let Some(run) = self.state.runs.get(&run_id)
                && !run.state.is_terminal()
            {
                candidate_events.push(ControlPlaneEvent::RunStateChanged {
                    run_id,
                    previous_state: run.state,
                    new_state: RunState::Cancelled,
                });
                summary.cancelled_runs.push(run_id);
            }
        }

        // 2. Cancel pending approvals
        for approval_id in approvals_to_cancel {
            if let Some(approval) = self.state.approvals.get(&approval_id)
                && approval.state == ApprovalState::Pending
            {
                candidate_events.push(ControlPlaneEvent::ApprovalResolved {
                    approval_id,
                    previous_state: approval.state,
                    new_state: ApprovalState::Cancelled,
                });
                summary.cancelled_approvals.push(approval_id);
            }
        }

        // 3. Cancel non-terminal tasks
        for task_id in tasks_to_cancel {
            if let Some(task) = self.state.task_graph.get_task(task_id)
                && !task.state.is_terminal()
            {
                candidate_events.push(ControlPlaneEvent::TaskStateChanged {
                    task_id,
                    previous_state: task.state,
                    new_state: TaskState::Cancelled,
                });
                summary.cancelled_tasks.push(task_id);
            }
        }

        // Atomically commit entire cancellation batch
        self.commit_transaction(studio_id, candidate_events)?;
        Ok(summary)
    }

    fn calculate_cancellation_targets(
        &self,
        scope: &CancellationScope,
    ) -> Result<CancellationTargets, ControlPlaneError> {
        let mut tasks = Vec::new();
        let mut runs = Vec::new();
        let mut approvals = Vec::new();

        match scope {
            CancellationScope::Studio(studio_id) => {
                if let Some(task_set) = self.state.tasks_by_studio.get(studio_id) {
                    for &task_id in task_set {
                        if let Some(task) = self.state.task_graph.get_task(task_id)
                            && !task.state.is_terminal()
                        {
                            tasks.push(task_id);
                        }
                    }
                }
                for approval in self.state.approvals.values() {
                    if approval.studio_id == *studio_id && approval.state == ApprovalState::Pending
                    {
                        approvals.push(approval.id);
                    }
                }
                for run in self.state.runs.values() {
                    if let Some(task) = self.state.task_graph.get_task(run.task_id)
                        && task.studio_id == *studio_id
                        && !run.state.is_terminal()
                    {
                        runs.push(run.id);
                    }
                }
            }
            CancellationScope::Task {
                task_id,
                include_descendants,
            } => {
                let target_task_ids = if *include_descendants {
                    self.find_descendant_tasks(*task_id)
                } else {
                    vec![*task_id]
                };

                for tid in target_task_ids {
                    if let Some(task) = self.state.task_graph.get_task(tid)
                        && !task.state.is_terminal()
                    {
                        tasks.push(tid);
                    }
                    if let Some(task_runs) = self.state.runs_by_task.get(&tid) {
                        for &rid in task_runs {
                            if let Some(run) = self.state.runs.get(&rid)
                                && !run.state.is_terminal()
                            {
                                runs.push(rid);
                            }
                        }
                    }
                    for approval in self.state.approvals.values() {
                        if approval.task_id == tid && approval.state == ApprovalState::Pending {
                            approvals.push(approval.id);
                        }
                    }
                }
            }
            CancellationScope::Agent(agent_id) => {
                for run in self.state.runs.values() {
                    if run.agent_id == *agent_id && !run.state.is_terminal() {
                        runs.push(run.id);
                    }
                }
                for approval in self.state.approvals.values() {
                    if approval.agent_id == *agent_id && approval.state == ApprovalState::Pending {
                        approvals.push(approval.id);
                    }
                }
            }
            CancellationScope::Run(run_id) => {
                if let Some(run) = self.state.runs.get(run_id)
                    && !run.state.is_terminal()
                {
                    runs.push(*run_id);
                }
            }
        }

        Ok((tasks, runs, approvals))
    }

    /// Iteratively finds all descendant tasks strictly following parent_task_id hierarchy.
    /// Does NOT traverse DAG execution dependencies.
    fn find_descendant_tasks(&self, root_task_id: TaskId) -> Vec<TaskId> {
        let mut results = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(root_task_id);
        visited.insert(root_task_id);

        while let Some(current_id) = queue.pop_front() {
            results.push(current_id);

            // ONLY tasks where parent_task_id == current_id
            for task in self.state.task_graph.all_tasks() {
                if task.parent_task_id == Some(current_id) && visited.insert(task.id) {
                    queue.push_back(task.id);
                }
            }
        }

        results
    }

    // ========================================================================
    // Event Queries & Replay Engine
    // ========================================================================

    pub fn events_for_studio(
        &self,
        studio_id: StudioId,
        from_sequence: u64,
    ) -> Result<Vec<EventEnvelope>, ControlPlaneError> {
        self.store
            .events_for_studio(studio_id, from_sequence)
            .map_err(ControlPlaneError::Store)
    }

    pub fn all_events(&self) -> Result<Vec<EventEnvelope>, ControlPlaneError> {
        self.store.all_events().map_err(ControlPlaneError::Store)
    }

    /// Reconstructs a ControlPlane instance strictly by replaying an event sequence.
    /// Validates schema version, sequence numbers, event ID uniqueness, entity references,
    /// studio boundaries, and state transitions.
    pub fn replay_events(
        events: &[EventEnvelope],
        clock: C,
        mut store: S,
    ) -> Result<Self, ReplayError> {
        let mut expected_sequences: HashMap<StudioId, u64> = HashMap::new();
        let mut seen_event_ids: HashSet<EventId> = HashSet::new();
        let mut state = ControlPlaneState::new();

        for envelope in events {
            // 1. Verify schema version
            if envelope.schema_version != CONTROL_PLANE_EVENT_SCHEMA_VERSION {
                return Err(ReplayError::UnsupportedSchemaVersion {
                    version: envelope.schema_version,
                    supported: CONTROL_PLANE_EVENT_SCHEMA_VERSION,
                });
            }

            // 2. Reject duplicate event ID
            if !seen_event_ids.insert(envelope.event_id) {
                return Err(ReplayError::DuplicateEventId {
                    event_id: envelope.event_id,
                });
            }

            // 3. Verify monotonic sequence without gaps or regressions per Studio
            let studio_id = envelope.studio_id;
            let expected = expected_sequences.get(&studio_id).copied().unwrap_or(1);

            if envelope.sequence < expected {
                return Err(ReplayError::Store(StoreError::SequenceRegression {
                    studio_id,
                    latest: expected - 1,
                    attempted: envelope.sequence,
                }));
            }

            if envelope.sequence > expected {
                return Err(ReplayError::InvalidSequence {
                    expected,
                    actual: envelope.sequence,
                });
            }

            expected_sequences.insert(studio_id, expected + 1);

            // 4. Strict apply to state
            state.apply_event(envelope)?;
        }

        // 5. Populate store atomically
        store.append_batch(events).map_err(|e| match e {
            StoreError::DuplicateEventId { event_id } => ReplayError::DuplicateEventId { event_id },
            other => ReplayError::Store(other),
        })?;

        Ok(Self {
            clock,
            store,
            state,
            committed_events_buffer: Vec::new(),
        })
    }
}
