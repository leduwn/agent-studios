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
    AgentId, ApprovalId, ArtifactId, EventId, ReconciliationId, RunId, StudioId, TaskId, WorktreeId,
};
use agent_studios_protocol::reconciliation::{ReconciliationRecord, ReconciliationState};
use agent_studios_protocol::run::{RunRecord, RunState};
use agent_studios_protocol::studio::Studio;
use agent_studios_protocol::task::{BatchTaskSpec, DependencyOutputPolicy, TaskRecord, TaskState};
use agent_studios_protocol::worktree::{WorktreeRecord, WorktreeState};
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
    pub worktrees: HashMap<WorktreeId, WorktreeRecord>,
    pub reconciliations: HashMap<ReconciliationId, ReconciliationRecord>,

    // Auxiliary indices for fast query resolution
    pub runs_by_task: HashMap<TaskId, Vec<RunId>>,
    pub tasks_by_studio: HashMap<StudioId, HashSet<TaskId>>,
    pub agents_by_studio: HashMap<StudioId, HashSet<AgentId>>,
    pub worktrees_by_studio: HashMap<StudioId, HashSet<WorktreeId>>,
    pub reconciliations_by_studio: HashMap<StudioId, HashSet<ReconciliationId>>,
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

                if artifact.version < 1 {
                    return Err(ReplayError::DomainViolation(
                        "Artifact version must be at least 1".to_string(),
                    ));
                }

                if let Some(supersedes_id) = artifact.supersedes {
                    let superseded = self.artifacts.get(&supersedes_id).ok_or_else(|| {
                        ReplayError::DomainViolation(format!(
                            "Superseded artifact {supersedes_id} not found"
                        ))
                    })?;
                    if superseded.studio_id != artifact.studio_id {
                        return Err(ReplayError::StudioMismatch {
                            expected: artifact.studio_id,
                            actual: superseded.studio_id,
                        });
                    }
                    if superseded.task_id != artifact.task_id {
                        return Err(ReplayError::DomainViolation(format!(
                            "Superseded artifact task {} does not match {}",
                            superseded.task_id, artifact.task_id
                        )));
                    }
                    if superseded.kind != artifact.kind {
                        return Err(ReplayError::DomainViolation(format!(
                            "Superseded artifact kind {:?} does not match {:?}",
                            superseded.kind, artifact.kind
                        )));
                    }
                    if superseded.logical_name != artifact.logical_name {
                        return Err(ReplayError::DomainViolation(format!(
                            "Superseded artifact logical name '{}' does not match '{}'",
                            superseded.logical_name, artifact.logical_name
                        )));
                    }
                    if superseded.version >= artifact.version {
                        return Err(ReplayError::DomainViolation(format!(
                            "Superseded artifact version {} must be strictly smaller than {}",
                            superseded.version, artifact.version
                        )));
                    }
                }

                if let Some(run_id) = artifact.run_id {
                    let run = self
                        .runs
                        .get(&run_id)
                        .ok_or(ReplayError::RunNotFound { run_id })?;
                    if run.task_id != artifact.task_id {
                        return Err(ReplayError::DomainViolation(format!(
                            "Artifact run {} task {} does not match artifact task {}",
                            run_id, run.task_id, artifact.task_id
                        )));
                    }
                    if run.agent_id != artifact.producer_agent_id {
                        return Err(ReplayError::DomainViolation(format!(
                            "Artifact run {} agent {} does not match artifact producer {}",
                            run_id, run.agent_id, artifact.producer_agent_id
                        )));
                    }
                }

                if let Some(worktree_id) = artifact.worktree_id {
                    let wt = self
                        .worktrees
                        .get(&worktree_id)
                        .ok_or(ReplayError::WorktreeNotFound { worktree_id })?;
                    if wt.studio_id != artifact.studio_id {
                        return Err(ReplayError::StudioMismatch {
                            expected: artifact.studio_id,
                            actual: wt.studio_id,
                        });
                    }
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

            ControlPlaneEvent::WorktreeCreated { worktree } => {
                if envelope.studio_id != worktree.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: worktree.studio_id,
                        actual: envelope.studio_id,
                    });
                }
                if !self.studios.contains_key(&worktree.studio_id) {
                    return Err(ReplayError::StudioNotFound {
                        studio_id: worktree.studio_id,
                    });
                }
                if self.worktrees.contains_key(&worktree.id) {
                    return Err(ReplayError::DuplicateWorktree {
                        worktree_id: worktree.id,
                    });
                }
                if let Some(task_id) = worktree.assigned_task_id {
                    let task = self
                        .task_graph
                        .get_task(task_id)
                        .ok_or(ReplayError::TaskNotFound { task_id })?;
                    if task.studio_id != worktree.studio_id {
                        return Err(ReplayError::StudioMismatch {
                            expected: worktree.studio_id,
                            actual: task.studio_id,
                        });
                    }
                }
                if let Some(agent_id) = worktree.assigned_agent_id {
                    let agent = self
                        .agents
                        .get(&agent_id)
                        .ok_or(ReplayError::AgentNotFound { agent_id })?;
                    if agent.studio_id != worktree.studio_id {
                        return Err(ReplayError::StudioMismatch {
                            expected: worktree.studio_id,
                            actual: agent.studio_id,
                        });
                    }
                }
                if let Some(run_id) = worktree
                    .assigned_run_id
                    .filter(|id| !self.runs.contains_key(id))
                {
                    return Err(ReplayError::RunNotFound { run_id });
                }

                self.worktrees_by_studio
                    .entry(worktree.studio_id)
                    .or_default()
                    .insert(worktree.id);
                self.worktrees.insert(worktree.id, worktree.clone());
            }

            ControlPlaneEvent::WorktreeAssigned {
                worktree_id,
                task_id,
                agent_id,
                run_id,
            } => {
                let worktree =
                    self.worktrees
                        .get_mut(worktree_id)
                        .ok_or(ReplayError::WorktreeNotFound {
                            worktree_id: *worktree_id,
                        })?;

                if envelope.studio_id != worktree.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: worktree.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                if worktree.state.is_terminal() {
                    return Err(ReplayError::DomainViolation(format!(
                        "Cannot assign task to worktree {} in terminal state {:?}",
                        worktree_id, worktree.state
                    )));
                }

                let task = self
                    .task_graph
                    .get_task(*task_id)
                    .ok_or(ReplayError::TaskNotFound { task_id: *task_id })?;
                if task.studio_id != worktree.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: worktree.studio_id,
                        actual: task.studio_id,
                    });
                }

                let agent = self
                    .agents
                    .get(agent_id)
                    .ok_or(ReplayError::AgentNotFound {
                        agent_id: *agent_id,
                    })?;
                if agent.studio_id != worktree.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: worktree.studio_id,
                        actual: agent.studio_id,
                    });
                }

                if let Some(rid) = run_id {
                    let run = self
                        .runs
                        .get(rid)
                        .ok_or(ReplayError::RunNotFound { run_id: *rid })?;
                    if run.task_id != *task_id || run.agent_id != *agent_id {
                        return Err(ReplayError::DomainViolation(format!(
                            "Run {rid} does not match assigned task {task_id} and agent {agent_id}"
                        )));
                    }
                }

                worktree.assigned_task_id = Some(*task_id);
                worktree.assigned_agent_id = Some(*agent_id);
                worktree.assigned_run_id = *run_id;
                worktree.updated_at = now;
            }

            ControlPlaneEvent::WorktreeThreadBound {
                worktree_id,
                thread_id,
            } => {
                let worktree =
                    self.worktrees
                        .get_mut(worktree_id)
                        .ok_or(ReplayError::WorktreeNotFound {
                            worktree_id: *worktree_id,
                        })?;

                if envelope.studio_id != worktree.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: worktree.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                if let Some(existing) = worktree
                    .bound_thread_id
                    .as_ref()
                    .filter(|&e| e != thread_id)
                {
                    return Err(ReplayError::DomainViolation(format!(
                        "Worktree {worktree_id} already bound to thread {existing}, cannot rebind to {thread_id}"
                    )));
                }

                worktree.bound_thread_id = Some(thread_id.clone());
                worktree.updated_at = now;
            }

            ControlPlaneEvent::WorktreeStateChanged {
                worktree_id,
                previous_state,
                new_state,
            } => {
                let worktree =
                    self.worktrees
                        .get_mut(worktree_id)
                        .ok_or(ReplayError::WorktreeNotFound {
                            worktree_id: *worktree_id,
                        })?;

                if envelope.studio_id != worktree.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: worktree.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                if worktree.state != *previous_state {
                    return Err(ReplayError::StateMismatch {
                        actual: format!("{:?}", worktree.state),
                        expected: format!("{previous_state:?}"),
                    });
                }

                previous_state
                    .validate_transition_to(*new_state)
                    .map_err(|e| ReplayError::InvalidTransition {
                        reason: e.to_string(),
                    })?;

                worktree.state = *new_state;
                worktree.updated_at = now;
                if *new_state == WorktreeState::Removed && worktree.released_at.is_none() {
                    worktree.released_at = Some(now);
                }
            }

            ControlPlaneEvent::WorktreeChangeCaptured {
                worktree_id,
                run_id,
                head_commit,
                patch_artifact_id,
                stats_artifact_id,
                ..
            } => {
                let worktree =
                    self.worktrees
                        .get_mut(worktree_id)
                        .ok_or(ReplayError::WorktreeNotFound {
                            worktree_id: *worktree_id,
                        })?;

                if envelope.studio_id != worktree.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: worktree.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                if let Some(rid) = run_id.filter(|id| !self.runs.contains_key(id)) {
                    return Err(ReplayError::RunNotFound { run_id: rid });
                }

                let patch_artifact =
                    self.artifacts
                        .get(patch_artifact_id)
                        .ok_or(ReplayError::DomainViolation(format!(
                            "Patch artifact {patch_artifact_id} not found"
                        )))?;
                if patch_artifact.studio_id != worktree.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: worktree.studio_id,
                        actual: patch_artifact.studio_id,
                    });
                }

                if let Some(stats_id) = stats_artifact_id {
                    let stats_artifact =
                        self.artifacts
                            .get(stats_id)
                            .ok_or(ReplayError::DomainViolation(format!(
                                "Stats artifact {stats_id} not found"
                            )))?;
                    if stats_artifact.studio_id != worktree.studio_id {
                        return Err(ReplayError::StudioMismatch {
                            expected: worktree.studio_id,
                            actual: stats_artifact.studio_id,
                        });
                    }
                }

                worktree.last_captured_commit = head_commit.clone();
                worktree.patch_artifact_id = Some(*patch_artifact_id);
                worktree.updated_at = now;
            }

            ControlPlaneEvent::WorktreeReleased {
                worktree_id,
                retained,
                reason,
            } => {
                let worktree =
                    self.worktrees
                        .get_mut(worktree_id)
                        .ok_or(ReplayError::WorktreeNotFound {
                            worktree_id: *worktree_id,
                        })?;

                if envelope.studio_id != worktree.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: worktree.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                worktree.retained = *retained;
                worktree.retained_reason = reason.clone();
                worktree.released_at = Some(now);
                worktree.updated_at = now;
            }

            ControlPlaneEvent::WorktreeNoChangesCaptured {
                worktree_id,
                run_id,
                base_commit,
            } => {
                let worktree =
                    self.worktrees
                        .get_mut(worktree_id)
                        .ok_or(ReplayError::WorktreeNotFound {
                            worktree_id: *worktree_id,
                        })?;

                if envelope.studio_id != worktree.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: worktree.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                if let Some(rid) = run_id.filter(|id| !self.runs.contains_key(id)) {
                    return Err(ReplayError::RunNotFound { run_id: rid });
                }

                worktree.last_captured_commit = Some(base_commit.clone());
                worktree.updated_at = now;
            }

            ControlPlaneEvent::ReconciliationCreated { reconciliation } => {
                if envelope.studio_id != reconciliation.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: reconciliation.studio_id,
                        actual: envelope.studio_id,
                    });
                }
                if !self.studios.contains_key(&reconciliation.studio_id) {
                    return Err(ReplayError::StudioNotFound {
                        studio_id: reconciliation.studio_id,
                    });
                }
                if self.reconciliations.contains_key(&reconciliation.id) {
                    return Err(ReplayError::DuplicateReconciliation {
                        reconciliation_id: reconciliation.id,
                    });
                }

                let source_worktree = self
                    .worktrees
                    .get(&reconciliation.source_worktree_id)
                    .ok_or(ReplayError::WorktreeNotFound {
                        worktree_id: reconciliation.source_worktree_id,
                    })?;
                if source_worktree.studio_id != reconciliation.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: reconciliation.studio_id,
                        actual: source_worktree.studio_id,
                    });
                }

                if let Some(target_worktree) =
                    self.worktrees.get(&reconciliation.target_worktree_id)
                    && target_worktree.studio_id != reconciliation.studio_id
                {
                    return Err(ReplayError::StudioMismatch {
                        expected: reconciliation.studio_id,
                        actual: target_worktree.studio_id,
                    });
                }

                let task = self.task_graph.get_task(reconciliation.task_id).ok_or(
                    ReplayError::TaskNotFound {
                        task_id: reconciliation.task_id,
                    },
                )?;
                if task.studio_id != reconciliation.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: reconciliation.studio_id,
                        actual: task.studio_id,
                    });
                }

                if let Some(run_id) = reconciliation
                    .run_id
                    .filter(|id| !self.runs.contains_key(id))
                {
                    return Err(ReplayError::RunNotFound { run_id });
                }

                let patch_artifact = self
                    .artifacts
                    .get(&reconciliation.patch_artifact_id)
                    .ok_or(ReplayError::DomainViolation(format!(
                        "Patch artifact {} not found",
                        reconciliation.patch_artifact_id
                    )))?;
                if patch_artifact.studio_id != reconciliation.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: reconciliation.studio_id,
                        actual: patch_artifact.studio_id,
                    });
                }

                self.reconciliations_by_studio
                    .entry(reconciliation.studio_id)
                    .or_default()
                    .insert(reconciliation.id);
                self.reconciliations
                    .insert(reconciliation.id, reconciliation.clone());
            }

            ControlPlaneEvent::ReconciliationStateChanged {
                reconciliation_id,
                previous_state,
                new_state,
            } => {
                let rec = self.reconciliations.get_mut(reconciliation_id).ok_or(
                    ReplayError::ReconciliationNotFound {
                        reconciliation_id: *reconciliation_id,
                    },
                )?;

                if envelope.studio_id != rec.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: rec.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                if rec.state != *previous_state {
                    return Err(ReplayError::StateMismatch {
                        actual: format!("{:?}", rec.state),
                        expected: format!("{previous_state:?}"),
                    });
                }

                previous_state
                    .validate_transition_to(*new_state)
                    .map_err(|e| ReplayError::InvalidTransition {
                        reason: e.to_string(),
                    })?;

                rec.state = *new_state;
                rec.updated_at = now;
                if new_state.is_terminal() && rec.completed_at.is_none() {
                    rec.completed_at = Some(now);
                }
            }

            ControlPlaneEvent::ReconciliationConflictDetected {
                reconciliation_id,
                conflicted_files,
                reason,
            } => {
                let rec = self.reconciliations.get_mut(reconciliation_id).ok_or(
                    ReplayError::ReconciliationNotFound {
                        reconciliation_id: *reconciliation_id,
                    },
                )?;

                if envelope.studio_id != rec.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: rec.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                rec.conflicted_files = conflicted_files.clone();
                rec.error_message = Some(reason.clone());
                rec.state = ReconciliationState::Conflicted;
                rec.updated_at = now;
                if rec.completed_at.is_none() {
                    rec.completed_at = Some(now);
                }
            }

            ControlPlaneEvent::ReconciliationApplied {
                reconciliation_id,
                merge_commit,
            } => {
                let rec = self.reconciliations.get_mut(reconciliation_id).ok_or(
                    ReplayError::ReconciliationNotFound {
                        reconciliation_id: *reconciliation_id,
                    },
                )?;

                if envelope.studio_id != rec.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: rec.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                rec.merge_commit = merge_commit.clone();
                rec.state = ReconciliationState::Applied;
                rec.updated_at = now;
                if rec.completed_at.is_none() {
                    rec.completed_at = Some(now);
                }
            }

            ControlPlaneEvent::ReconciliationFailed {
                reconciliation_id,
                error,
            } => {
                let rec = self.reconciliations.get_mut(reconciliation_id).ok_or(
                    ReplayError::ReconciliationNotFound {
                        reconciliation_id: *reconciliation_id,
                    },
                )?;

                if envelope.studio_id != rec.studio_id {
                    return Err(ReplayError::StudioMismatch {
                        expected: rec.studio_id,
                        actual: envelope.studio_id,
                    });
                }

                rec.error_message = Some(error.clone());
                rec.state = ReconciliationState::Failed;
                rec.updated_at = now;
                if rec.completed_at.is_none() {
                    rec.completed_at = Some(now);
                }
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

    #[allow(clippy::too_many_arguments)]
    pub fn create_task_with_policy(
        &mut self,
        studio_id: StudioId,
        title: impl Into<String>,
        description: impl Into<String>,
        parent_task_id: Option<TaskId>,
        assigned_agent_id: Option<AgentId>,
        dependencies: Vec<TaskId>,
        dependency_output_policy: agent_studios_protocol::task::DependencyOutputPolicy,
    ) -> Result<TaskRecord, ControlPlaneError> {
        let mut task = self.create_task(
            studio_id,
            title,
            description,
            parent_task_id,
            assigned_agent_id,
            dependencies,
        )?;
        task.dependency_output_policy = dependency_output_policy;
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
                dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
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
        self.register_artifact_record(artifact)
    }

    pub fn get_artifact(&self, id: ArtifactId) -> Option<&ArtifactRecord> {
        self.state.artifacts.get(&id)
    }

    pub fn all_artifacts(&self) -> impl Iterator<Item = &ArtifactRecord> {
        self.state.artifacts.values()
    }

    pub fn register_artifact_record(
        &mut self,
        mut artifact: ArtifactRecord,
    ) -> Result<ArtifactRecord, ControlPlaneError> {
        let studio_id = artifact.studio_id;
        if !self.state.studios.contains_key(&studio_id) {
            return Err(ControlPlaneError::StudioNotFound(studio_id));
        }

        let task = self
            .state
            .task_graph
            .get_task(artifact.task_id)
            .ok_or(ControlPlaneError::TaskNotFound(artifact.task_id))?;
        if task.studio_id != studio_id {
            return Err(ControlPlaneError::StudioMismatch {
                expected: studio_id,
                actual: task.studio_id,
            });
        }

        let agent = self
            .state
            .agents
            .get(&artifact.producer_agent_id)
            .ok_or(ControlPlaneError::AgentNotFound(artifact.producer_agent_id))?;
        if agent.studio_id != studio_id {
            return Err(ControlPlaneError::StudioMismatch {
                expected: studio_id,
                actual: agent.studio_id,
            });
        }

        if let Some(worktree_id) = artifact.worktree_id {
            let wt = self
                .state
                .worktrees
                .get(&worktree_id)
                .ok_or(ControlPlaneError::WorktreeNotFound(worktree_id))?;
            if wt.studio_id != studio_id {
                return Err(ControlPlaneError::StudioMismatch {
                    expected: studio_id,
                    actual: wt.studio_id,
                });
            }
        }

        // Lineage version allocation (M09 Part X)
        let existing_family_latest = self
            .state
            .artifacts
            .values()
            .filter(|a| {
                a.studio_id == artifact.studio_id
                    && a.task_id == artifact.task_id
                    && a.kind == artifact.kind
                    && a.logical_name == artifact.logical_name
            })
            .max_by_key(|a| a.version);

        if let Some(latest) = existing_family_latest {
            let caller_specified_lineage = artifact.version > 1 && artifact.supersedes.is_some();
            if !caller_specified_lineage {
                artifact.version = latest.version + 1;
                artifact.supersedes = Some(latest.id);
            }
        } else if artifact.version == 0 {
            artifact.version = 1;
        }

        let event = ControlPlaneEvent::ArtifactRegistered {
            artifact: artifact.clone(),
        };

        self.commit_transaction(studio_id, vec![event])?;
        Ok(artifact)
    }

    // ========================================================================
    // Worktree Management
    // ========================================================================

    pub fn create_worktree(
        &mut self,
        studio_id: StudioId,
        name: impl Into<String>,
        repo_path: impl Into<std::path::PathBuf>,
        worktree_path: impl Into<std::path::PathBuf>,
        base_commit: impl Into<String>,
    ) -> Result<WorktreeRecord, ControlPlaneError> {
        if !self.state.studios.contains_key(&studio_id) {
            return Err(ControlPlaneError::StudioNotFound(studio_id));
        }

        let now = self.clock.now();
        let worktree =
            WorktreeRecord::new(studio_id, name, repo_path, worktree_path, base_commit, now);

        let event = ControlPlaneEvent::WorktreeCreated {
            worktree: worktree.clone(),
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(worktree)
    }

    pub fn assign_worktree(
        &mut self,
        worktree_id: WorktreeId,
        task_id: TaskId,
        agent_id: AgentId,
        run_id: Option<RunId>,
    ) -> Result<(), ControlPlaneError> {
        let worktree = self
            .state
            .worktrees
            .get(&worktree_id)
            .ok_or(ControlPlaneError::WorktreeNotFound(worktree_id))?;
        let studio_id = worktree.studio_id;

        if worktree.state.is_terminal() {
            return Err(ControlPlaneError::InvalidOperation(format!(
                "Cannot assign task to worktree {worktree_id} in terminal state {:?}",
                worktree.state
            )));
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

        if let Some(rid) = run_id {
            let run = self
                .state
                .runs
                .get(&rid)
                .ok_or(ControlPlaneError::RunNotFound(rid))?;
            if run.task_id != task_id || run.agent_id != agent_id {
                return Err(ControlPlaneError::InvalidOperation(format!(
                    "Run {rid} does not match assigned task {task_id} and agent {agent_id}"
                )));
            }
        }

        let event = ControlPlaneEvent::WorktreeAssigned {
            worktree_id,
            task_id,
            agent_id,
            run_id,
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn bind_worktree_thread(
        &mut self,
        worktree_id: WorktreeId,
        thread_id: impl Into<String>,
    ) -> Result<(), ControlPlaneError> {
        let thread_id = thread_id.into();
        let worktree = self
            .state
            .worktrees
            .get(&worktree_id)
            .ok_or(ControlPlaneError::WorktreeNotFound(worktree_id))?;
        let studio_id = worktree.studio_id;

        if let Some(existing) = &worktree.bound_thread_id {
            if existing == &thread_id {
                return Ok(());
            }
            return Err(ControlPlaneError::WorktreeOwnershipConflict {
                worktree_id,
                reason: format!(
                    "Worktree already bound to thread {existing}, cannot rebind to {thread_id}"
                ),
            });
        }

        let event = ControlPlaneEvent::WorktreeThreadBound {
            worktree_id,
            thread_id,
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn record_worktree_retained(
        &mut self,
        worktree_id: WorktreeId,
        reason: Option<String>,
    ) -> Result<(), ControlPlaneError> {
        let worktree = self
            .state
            .worktrees
            .get(&worktree_id)
            .ok_or(ControlPlaneError::WorktreeNotFound(worktree_id))?;
        let studio_id = worktree.studio_id;
        let mut candidate_events = Vec::new();

        if worktree.state != WorktreeState::Retained {
            worktree
                .state
                .validate_transition_to(WorktreeState::Retained)?;
            candidate_events.push(ControlPlaneEvent::WorktreeStateChanged {
                worktree_id,
                previous_state: worktree.state,
                new_state: WorktreeState::Retained,
            });
        }

        candidate_events.push(ControlPlaneEvent::WorktreeReleased {
            worktree_id,
            retained: true,
            reason,
        });

        self.commit_transaction(studio_id, candidate_events)?;
        Ok(())
    }

    pub fn record_worktree_no_changes(
        &mut self,
        worktree_id: WorktreeId,
        run_id: Option<RunId>,
        base_commit: impl Into<String>,
    ) -> Result<(), ControlPlaneError> {
        let worktree = self
            .state
            .worktrees
            .get(&worktree_id)
            .ok_or(ControlPlaneError::WorktreeNotFound(worktree_id))?;
        let studio_id = worktree.studio_id;

        let event = ControlPlaneEvent::WorktreeNoChangesCaptured {
            worktree_id,
            run_id,
            base_commit: base_commit.into(),
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn transition_worktree_state(
        &mut self,
        worktree_id: WorktreeId,
        new_state: WorktreeState,
    ) -> Result<(), ControlPlaneError> {
        let worktree = self
            .state
            .worktrees
            .get(&worktree_id)
            .ok_or(ControlPlaneError::WorktreeNotFound(worktree_id))?;
        let studio_id = worktree.studio_id;
        let previous_state = worktree.state;

        if previous_state == new_state {
            return Ok(());
        }

        previous_state.validate_transition_to(new_state)?;

        let event = ControlPlaneEvent::WorktreeStateChanged {
            worktree_id,
            previous_state,
            new_state,
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_worktree_change_captured(
        &mut self,
        worktree_id: WorktreeId,
        run_id: Option<RunId>,
        base_commit: impl Into<String>,
        head_commit: Option<String>,
        patch_artifact_id: ArtifactId,
        stats_artifact_id: Option<ArtifactId>,
        files_changed: usize,
    ) -> Result<(), ControlPlaneError> {
        let worktree = self
            .state
            .worktrees
            .get(&worktree_id)
            .ok_or(ControlPlaneError::WorktreeNotFound(worktree_id))?;
        let studio_id = worktree.studio_id;

        if !self.state.artifacts.contains_key(&patch_artifact_id) {
            return Err(ControlPlaneError::ArtifactNotFound(patch_artifact_id));
        }

        if let Some(stats_id) =
            stats_artifact_id.filter(|id| !self.state.artifacts.contains_key(id))
        {
            return Err(ControlPlaneError::ArtifactNotFound(stats_id));
        }

        let event = ControlPlaneEvent::WorktreeChangeCaptured {
            worktree_id,
            run_id,
            base_commit: base_commit.into(),
            head_commit,
            patch_artifact_id,
            stats_artifact_id,
            files_changed,
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn release_worktree(
        &mut self,
        worktree_id: WorktreeId,
        retained: bool,
        reason: Option<String>,
    ) -> Result<(), ControlPlaneError> {
        let worktree = self
            .state
            .worktrees
            .get(&worktree_id)
            .ok_or(ControlPlaneError::WorktreeNotFound(worktree_id))?;
        let studio_id = worktree.studio_id;

        let event = ControlPlaneEvent::WorktreeReleased {
            worktree_id,
            retained,
            reason,
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn get_worktree(&self, id: WorktreeId) -> Option<&WorktreeRecord> {
        self.state.worktrees.get(&id)
    }

    pub fn all_worktrees(&self) -> impl Iterator<Item = &WorktreeRecord> {
        self.state.worktrees.values()
    }

    pub fn worktrees_for_studio(&self, studio_id: StudioId) -> Vec<&WorktreeRecord> {
        self.state
            .worktrees_by_studio
            .get(&studio_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.state.worktrees.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    // ========================================================================
    // Reconciliation Management
    // ========================================================================

    #[allow(clippy::too_many_arguments)]
    pub fn create_reconciliation(
        &mut self,
        studio_id: StudioId,
        worktree_id: WorktreeId,
        task_id: TaskId,
        run_id: Option<RunId>,
        patch_artifact_id: ArtifactId,
        target_worktree_path: impl Into<std::path::PathBuf>,
        base_commit: impl Into<String>,
    ) -> Result<ReconciliationRecord, ControlPlaneError> {
        self.create_reconciliation_with_target(
            studio_id,
            worktree_id,
            worktree_id,
            task_id,
            run_id,
            patch_artifact_id,
            target_worktree_path,
            base_commit,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_reconciliation_with_target(
        &mut self,
        studio_id: StudioId,
        source_worktree_id: WorktreeId,
        target_worktree_id: WorktreeId,
        task_id: TaskId,
        run_id: Option<RunId>,
        patch_artifact_id: ArtifactId,
        target_worktree_path: impl Into<std::path::PathBuf>,
        base_commit: impl Into<String>,
    ) -> Result<ReconciliationRecord, ControlPlaneError> {
        if !self.state.studios.contains_key(&studio_id) {
            return Err(ControlPlaneError::StudioNotFound(studio_id));
        }

        let worktree = self
            .state
            .worktrees
            .get(&source_worktree_id)
            .ok_or(ControlPlaneError::WorktreeNotFound(source_worktree_id))?;
        if worktree.studio_id != studio_id {
            return Err(ControlPlaneError::StudioMismatch {
                expected: studio_id,
                actual: worktree.studio_id,
            });
        }

        if let Some(target_wt) = self.state.worktrees.get(&target_worktree_id)
            && target_wt.studio_id != studio_id
        {
            return Err(ControlPlaneError::StudioMismatch {
                expected: studio_id,
                actual: target_wt.studio_id,
            });
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

        if let Some(rid) = run_id {
            let run = self
                .state
                .runs
                .get(&rid)
                .ok_or(ControlPlaneError::RunNotFound(rid))?;
            if run.task_id != task_id {
                return Err(ControlPlaneError::InvalidOperation(format!(
                    "Run {rid} does not belong to task {task_id}"
                )));
            }
        }

        if !self.state.artifacts.contains_key(&patch_artifact_id) {
            return Err(ControlPlaneError::ArtifactNotFound(patch_artifact_id));
        }

        let now = self.clock.now();
        let reconciliation = ReconciliationRecord::new_with_target(
            studio_id,
            source_worktree_id,
            target_worktree_id,
            task_id,
            run_id,
            patch_artifact_id,
            target_worktree_path,
            base_commit,
            now,
        );

        let event = ControlPlaneEvent::ReconciliationCreated {
            reconciliation: reconciliation.clone(),
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(reconciliation)
    }

    pub fn start_reconciliation(
        &mut self,
        reconciliation_id: ReconciliationId,
    ) -> Result<(), ControlPlaneError> {
        self.transition_reconciliation_state(reconciliation_id, ReconciliationState::Checking)
    }

    pub fn mark_reconciliation_applying(
        &mut self,
        reconciliation_id: ReconciliationId,
    ) -> Result<(), ControlPlaneError> {
        self.transition_reconciliation_state(reconciliation_id, ReconciliationState::Applying)
    }

    pub fn transition_reconciliation_state(
        &mut self,
        reconciliation_id: ReconciliationId,
        new_state: ReconciliationState,
    ) -> Result<(), ControlPlaneError> {
        let rec = self
            .state
            .reconciliations
            .get(&reconciliation_id)
            .ok_or(ControlPlaneError::ReconciliationNotFound(reconciliation_id))?;
        let studio_id = rec.studio_id;
        let previous_state = rec.state;

        if previous_state == new_state {
            return Ok(());
        }

        previous_state.validate_transition_to(new_state)?;

        let event = ControlPlaneEvent::ReconciliationStateChanged {
            reconciliation_id,
            previous_state,
            new_state,
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn record_reconciliation_conflict(
        &mut self,
        reconciliation_id: ReconciliationId,
        conflicted_files: Vec<String>,
        reason: impl Into<String>,
    ) -> Result<(), ControlPlaneError> {
        let rec = self
            .state
            .reconciliations
            .get(&reconciliation_id)
            .ok_or(ControlPlaneError::ReconciliationNotFound(reconciliation_id))?;
        let studio_id = rec.studio_id;

        if rec.state != ReconciliationState::Checking && rec.state != ReconciliationState::Applying
        {
            return Err(ControlPlaneError::InvalidOperation(format!(
                "ReconciliationConflictDetected is only legal from Checking or Applying state, current state is {:?}",
                rec.state
            )));
        }

        let event = ControlPlaneEvent::ReconciliationConflictDetected {
            reconciliation_id,
            conflicted_files,
            reason: reason.into(),
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn record_reconciliation_applied(
        &mut self,
        reconciliation_id: ReconciliationId,
        merge_commit: Option<String>,
    ) -> Result<(), ControlPlaneError> {
        let rec = self
            .state
            .reconciliations
            .get(&reconciliation_id)
            .ok_or(ControlPlaneError::ReconciliationNotFound(reconciliation_id))?;
        let studio_id = rec.studio_id;

        if rec.state != ReconciliationState::Applying {
            return Err(ControlPlaneError::InvalidOperation(format!(
                "ReconciliationApplied is only legal from Applying state, current state is {:?}",
                rec.state
            )));
        }

        let event = ControlPlaneEvent::ReconciliationApplied {
            reconciliation_id,
            merge_commit,
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn record_reconciliation_failed(
        &mut self,
        reconciliation_id: ReconciliationId,
        error: impl Into<String>,
    ) -> Result<(), ControlPlaneError> {
        let rec = self
            .state
            .reconciliations
            .get(&reconciliation_id)
            .ok_or(ControlPlaneError::ReconciliationNotFound(reconciliation_id))?;
        let studio_id = rec.studio_id;

        if rec.state.is_terminal() {
            return Err(ControlPlaneError::InvalidOperation(format!(
                "Cannot fail reconciliation in terminal state {:?}",
                rec.state
            )));
        }

        let event = ControlPlaneEvent::ReconciliationFailed {
            reconciliation_id,
            error: error.into(),
        };
        self.commit_transaction(studio_id, vec![event])?;
        Ok(())
    }

    pub fn get_reconciliation(&self, id: ReconciliationId) -> Option<&ReconciliationRecord> {
        self.state.reconciliations.get(&id)
    }

    pub fn all_reconciliations(&self) -> impl Iterator<Item = &ReconciliationRecord> {
        self.state.reconciliations.values()
    }

    pub fn reconciliations_for_studio(&self, studio_id: StudioId) -> Vec<&ReconciliationRecord> {
        self.state
            .reconciliations_by_studio
            .get(&studio_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.state.reconciliations.get(id))
                    .collect()
            })
            .unwrap_or_default()
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
