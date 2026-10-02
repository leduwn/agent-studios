use std::collections::{HashMap, HashSet, VecDeque};

use agent_studios_protocol::agent::{AgentDescriptor, AgentKind, AgentState};
use agent_studios_protocol::approval::{ApprovalKind, ApprovalRequest, ApprovalState};
use agent_studios_protocol::artifact::{ArtifactKind, ArtifactRecord};
use agent_studios_protocol::cancellation::{CancellationScope, CancellationSummary};
use agent_studios_protocol::event::{ControlPlaneEvent, EventEnvelope};
use agent_studios_protocol::id::{AgentId, ApprovalId, ArtifactId, RunId, StudioId, TaskId};
use agent_studios_protocol::run::{RunRecord, RunState};
use agent_studios_protocol::studio::Studio;
use agent_studios_protocol::task::{TaskRecord, TaskState};

use crate::clock::{Clock, SystemClock};
use crate::error::{ControlPlaneError, ReplayError};
use crate::store::{EventStore, InMemoryStore};
use crate::task_graph::TaskGraph;

type CancellationTargets = (Vec<TaskId>, Vec<RunId>, Vec<ApprovalId>);

/// Core deterministic control plane state engine.
pub struct ControlPlane<C: Clock = SystemClock, S: EventStore = InMemoryStore> {
    clock: C,
    store: S,
    studios: HashMap<StudioId, Studio>,
    agents: HashMap<AgentId, AgentDescriptor>,
    task_graph: TaskGraph,
    runs: HashMap<RunId, RunRecord>,
    approvals: HashMap<ApprovalId, ApprovalRequest>,
    artifacts: HashMap<ArtifactId, ArtifactRecord>,

    // Auxiliary indices for fast query resolution
    runs_by_task: HashMap<TaskId, Vec<RunId>>,
    tasks_by_studio: HashMap<StudioId, HashSet<TaskId>>,
    agents_by_studio: HashMap<StudioId, HashSet<AgentId>>,
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
            studios: HashMap::new(),
            agents: HashMap::new(),
            task_graph: TaskGraph::new(),
            runs: HashMap::new(),
            approvals: HashMap::new(),
            artifacts: HashMap::new(),
            runs_by_task: HashMap::new(),
            tasks_by_studio: HashMap::new(),
            agents_by_studio: HashMap::new(),
        }
    }

    pub fn clock(&self) -> &C {
        &self.clock
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
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

        let envelope = self.emit(studio.id, event)?;
        self.apply_event(&envelope)
            .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;

        Ok(studio)
    }

    pub fn get_studio(&self, id: StudioId) -> Option<&Studio> {
        self.studios.get(&id)
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
        if !self.studios.contains_key(&studio_id) {
            return Err(ControlPlaneError::StudioNotFound(studio_id));
        }

        let agent = AgentDescriptor::new(studio_id, display_name, kind, role);
        let event = ControlPlaneEvent::AgentRegistered {
            agent: agent.clone(),
        };

        let envelope = self.emit(studio_id, event)?;
        self.apply_event(&envelope)
            .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;

        Ok(agent)
    }

    pub fn update_agent_state(
        &mut self,
        agent_id: AgentId,
        new_state: AgentState,
    ) -> Result<(), ControlPlaneError> {
        let agent = self
            .agents
            .get(&agent_id)
            .ok_or(ControlPlaneError::AgentNotFound(agent_id))?;

        if agent.state == new_state {
            return Ok(());
        }

        let previous_state = agent.state;
        let studio_id = agent.studio_id;

        let event = ControlPlaneEvent::AgentStateChanged {
            agent_id,
            previous_state,
            new_state,
        };

        let envelope = self.emit(studio_id, event)?;
        self.apply_event(&envelope)
            .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;

        Ok(())
    }

    pub fn get_agent(&self, id: AgentId) -> Option<&AgentDescriptor> {
        self.agents.get(&id)
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
        if !self.studios.contains_key(&studio_id) {
            return Err(ControlPlaneError::StudioNotFound(studio_id));
        }

        if let Some(parent_id) = parent_task_id {
            let parent = self
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
            self.task_graph
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
        let envelope = self.emit(studio_id, event)?;
        self.apply_event(&envelope)
            .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;

        Ok(task)
    }

    pub fn transition_task_state(
        &mut self,
        task_id: TaskId,
        new_state: TaskState,
    ) -> Result<(), ControlPlaneError> {
        let task = self
            .task_graph
            .get_task(task_id)
            .ok_or(ControlPlaneError::TaskNotFound(task_id))?;

        if task.state == new_state {
            return Ok(());
        }

        task.state.validate_transition_to(new_state)?;

        let previous_state = task.state;
        let studio_id = task.studio_id;

        let event = ControlPlaneEvent::TaskStateChanged {
            task_id,
            previous_state,
            new_state,
        };

        let envelope = self.emit(studio_id, event)?;
        self.apply_event(&envelope)
            .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;

        // If a task succeeded, evaluate and potentially unblock dependents
        if new_state == TaskState::Succeeded {
            self.evaluate_dependents(task_id)?;
        }

        Ok(())
    }

    pub fn add_task_dependency(
        &mut self,
        task_id: TaskId,
        dependency_id: TaskId,
    ) -> Result<(), ControlPlaneError> {
        let task = self
            .task_graph
            .get_task(task_id)
            .ok_or(ControlPlaneError::TaskNotFound(task_id))?;
        let dep = self
            .task_graph
            .get_task(dependency_id)
            .ok_or(ControlPlaneError::TaskNotFound(dependency_id))?;

        if task.studio_id != dep.studio_id {
            return Err(ControlPlaneError::StudioMismatch {
                expected: task.studio_id,
                actual: dep.studio_id,
            });
        }

        let studio_id = task.studio_id;

        // Perform validation in TaskGraph
        self.task_graph.add_dependency(task_id, dependency_id)?;

        let event = ControlPlaneEvent::TaskDependencyAdded {
            task_id,
            dependency_id,
        };

        let envelope = self.emit(studio_id, event)?;
        self.apply_event(&envelope)
            .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;

        Ok(())
    }

    pub fn remove_task_dependency(
        &mut self,
        task_id: TaskId,
        dependency_id: TaskId,
    ) -> Result<(), ControlPlaneError> {
        let task = self
            .task_graph
            .get_task(task_id)
            .ok_or(ControlPlaneError::TaskNotFound(task_id))?;
        let studio_id = task.studio_id;

        self.task_graph.remove_dependency(task_id, dependency_id)?;

        let event = ControlPlaneEvent::TaskDependencyRemoved {
            task_id,
            dependency_id,
        };

        let envelope = self.emit(studio_id, event)?;
        self.apply_event(&envelope)
            .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;

        Ok(())
    }

    pub fn is_task_ready(&self, task_id: TaskId) -> Result<bool, ControlPlaneError> {
        Ok(self.task_graph.is_ready(task_id)?)
    }

    pub fn get_task(&self, id: TaskId) -> Option<&TaskRecord> {
        self.task_graph.get_task(id)
    }

    pub fn task_graph(&self) -> &TaskGraph {
        &self.task_graph
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
            .task_graph
            .get_task(task_id)
            .ok_or(ControlPlaneError::TaskNotFound(task_id))?;
        let agent = self
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
            .runs_by_task
            .get(&task_id)
            .map(|r| r.len() as u32)
            .unwrap_or(0)
            + 1;

        let run = RunRecord::new(task_id, agent_id, attempt);
        let event = ControlPlaneEvent::RunCreated { run: run.clone() };

        let envelope = self.emit(studio_id, event)?;
        self.apply_event(&envelope)
            .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;

        Ok(run)
    }

    pub fn transition_run_state(
        &mut self,
        run_id: RunId,
        new_state: RunState,
    ) -> Result<(), ControlPlaneError> {
        let run = self
            .runs
            .get(&run_id)
            .ok_or(ControlPlaneError::RunNotFound(run_id))?;

        if run.state == new_state {
            return Ok(());
        }

        run.state.validate_transition_to(new_state)?;

        let previous_state = run.state;
        let task = self
            .task_graph
            .get_task(run.task_id)
            .ok_or(ControlPlaneError::TaskNotFound(run.task_id))?;
        let studio_id = task.studio_id;

        let event = ControlPlaneEvent::RunStateChanged {
            run_id,
            previous_state,
            new_state,
        };

        let envelope = self.emit(studio_id, event)?;
        self.apply_event(&envelope)
            .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;

        Ok(())
    }

    pub fn get_run(&self, id: RunId) -> Option<&RunRecord> {
        self.runs.get(&id)
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
        if !self.studios.contains_key(&studio_id) {
            return Err(ControlPlaneError::StudioNotFound(studio_id));
        }

        let task = self
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

        let envelope = self.emit(studio_id, event)?;
        self.apply_event(&envelope)
            .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;

        Ok(approval)
    }

    pub fn resolve_approval(
        &mut self,
        approval_id: ApprovalId,
        new_state: ApprovalState,
    ) -> Result<(), ControlPlaneError> {
        let approval = self
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

        let envelope = self.emit(studio_id, event)?;
        self.apply_event(&envelope)
            .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;

        Ok(())
    }

    pub fn get_approval(&self, id: ApprovalId) -> Option<&ApprovalRequest> {
        self.approvals.get(&id)
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
        if !self.studios.contains_key(&studio_id) {
            return Err(ControlPlaneError::StudioNotFound(studio_id));
        }

        let task = self
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

        let envelope = self.emit(studio_id, event)?;
        self.apply_event(&envelope)
            .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;

        Ok(artifact)
    }

    pub fn get_artifact(&self, id: ArtifactId) -> Option<&ArtifactRecord> {
        self.artifacts.get(&id)
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
            CancellationScope::Studio(sid) => *sid,
            CancellationScope::Task { task_id, .. } => {
                let t = self
                    .task_graph
                    .get_task(*task_id)
                    .ok_or(ControlPlaneError::TaskNotFound(*task_id))?;
                t.studio_id
            }
            CancellationScope::Agent(aid) => {
                let a = self
                    .agents
                    .get(aid)
                    .ok_or(ControlPlaneError::AgentNotFound(*aid))?;
                a.studio_id
            }
            CancellationScope::Run(rid) => {
                let r = self
                    .runs
                    .get(rid)
                    .ok_or(ControlPlaneError::RunNotFound(*rid))?;
                let t = self
                    .task_graph
                    .get_task(r.task_id)
                    .ok_or(ControlPlaneError::TaskNotFound(r.task_id))?;
                t.studio_id
            }
        };

        // Determine targets to cancel
        let (tasks_to_cancel, runs_to_cancel, approvals_to_cancel) =
            self.calculate_cancellation_targets(&scope)?;

        // Emit CancellationRequested event
        let event = ControlPlaneEvent::CancellationRequested { scope, reason };
        self.emit(studio_id, event)?;

        let mut summary = CancellationSummary::default();

        // 1. Cancel active runs
        for run_id in runs_to_cancel {
            if let Some(run) = self.runs.get(&run_id)
                && !run.state.is_terminal()
            {
                let previous_state = run.state;
                let event = ControlPlaneEvent::RunStateChanged {
                    run_id,
                    previous_state,
                    new_state: RunState::Cancelled,
                };
                let env = self.emit(studio_id, event)?;
                self.apply_event(&env)
                    .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;
                summary.cancelled_runs.push(run_id);
            }
        }

        // 2. Cancel pending approvals
        for approval_id in approvals_to_cancel {
            if let Some(approval) = self.approvals.get(&approval_id)
                && approval.state == ApprovalState::Pending
            {
                let previous_state = approval.state;
                let event = ControlPlaneEvent::ApprovalResolved {
                    approval_id,
                    previous_state,
                    new_state: ApprovalState::Cancelled,
                };
                let env = self.emit(studio_id, event)?;
                self.apply_event(&env)
                    .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;
                summary.cancelled_approvals.push(approval_id);
            }
        }

        // 3. Cancel non-terminal tasks
        for task_id in tasks_to_cancel {
            if let Some(task) = self.task_graph.get_task(task_id)
                && !task.state.is_terminal()
            {
                let previous_state = task.state;
                let event = ControlPlaneEvent::TaskStateChanged {
                    task_id,
                    previous_state,
                    new_state: TaskState::Cancelled,
                };
                let env = self.emit(studio_id, event)?;
                self.apply_event(&env)
                    .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;
                summary.cancelled_tasks.push(task_id);
            }
        }

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
                if let Some(task_set) = self.tasks_by_studio.get(studio_id) {
                    for &task_id in task_set {
                        if let Some(task) = self.task_graph.get_task(task_id)
                            && !task.state.is_terminal()
                        {
                            tasks.push(task_id);
                        }
                    }
                }
                for approval in self.approvals.values() {
                    if approval.studio_id == *studio_id && approval.state == ApprovalState::Pending
                    {
                        approvals.push(approval.id);
                    }
                }
                for run in self.runs.values() {
                    if let Some(task) = self.task_graph.get_task(run.task_id)
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
                    if let Some(task) = self.task_graph.get_task(tid)
                        && !task.state.is_terminal()
                    {
                        tasks.push(tid);
                    }
                    if let Some(task_runs) = self.runs_by_task.get(&tid) {
                        for &rid in task_runs {
                            if let Some(run) = self.runs.get(&rid)
                                && !run.state.is_terminal()
                            {
                                runs.push(rid);
                            }
                        }
                    }
                    for approval in self.approvals.values() {
                        if approval.task_id == tid && approval.state == ApprovalState::Pending {
                            approvals.push(approval.id);
                        }
                    }
                }
            }
            CancellationScope::Agent(agent_id) => {
                for run in self.runs.values() {
                    if run.agent_id == *agent_id && !run.state.is_terminal() {
                        runs.push(run.id);
                    }
                }
                for approval in self.approvals.values() {
                    if approval.agent_id == *agent_id && approval.state == ApprovalState::Pending {
                        approvals.push(approval.id);
                    }
                }
            }
            CancellationScope::Run(run_id) => {
                if let Some(run) = self.runs.get(run_id)
                    && !run.state.is_terminal()
                {
                    runs.push(*run_id);
                }
            }
        }

        Ok((tasks, runs, approvals))
    }

    /// Iteratively finds all descendant tasks (hierarchical subtasks + graph dependents).
    fn find_descendant_tasks(&self, root_task_id: TaskId) -> Vec<TaskId> {
        let mut results = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(root_task_id);
        visited.insert(root_task_id);

        while let Some(current_id) = queue.pop_front() {
            results.push(current_id);

            // 1. Tasks where parent_task_id == current_id
            for task in self.task_graph.all_tasks() {
                if task.parent_task_id == Some(current_id) && visited.insert(task.id) {
                    queue.push_back(task.id);
                }
            }

            // 2. Direct graph dependents
            if let Ok(dependents) = self.task_graph.dependents_of(current_id) {
                for dep_id in dependents {
                    if visited.insert(dep_id) {
                        queue.push_back(dep_id);
                    }
                }
            }
        }

        results
    }

    // ========================================================================
    // Internal Readiness Evaluator
    // ========================================================================

    fn evaluate_dependents(&mut self, completed_task_id: TaskId) -> Result<(), ControlPlaneError> {
        let dependents = self
            .task_graph
            .dependents_of(completed_task_id)
            .map_err(ControlPlaneError::TaskGraph)?;

        for dependent_id in dependents {
            if let Some(task) = self.task_graph.get_task(dependent_id)
                && task.state == TaskState::Blocked
                && self.task_graph.is_ready(dependent_id)?
            {
                let studio_id = task.studio_id;
                let event = ControlPlaneEvent::TaskStateChanged {
                    task_id: dependent_id,
                    previous_state: TaskState::Blocked,
                    new_state: TaskState::Ready,
                };
                let envelope = self.emit(studio_id, event)?;
                self.apply_event(&envelope)
                    .map_err(|e| ControlPlaneError::InvalidOperation(e.to_string()))?;
            }
        }

        Ok(())
    }

    // ========================================================================
    // Event Emission & Replay Engine
    // ========================================================================

    fn emit(
        &mut self,
        studio_id: StudioId,
        event: ControlPlaneEvent,
    ) -> Result<EventEnvelope, ControlPlaneError> {
        let latest = self.store.latest_sequence(studio_id)?;
        let next_sequence = latest + 1;
        let now = self.clock.now();

        let envelope = EventEnvelope::new(studio_id, next_sequence, now, event);
        self.store.append(envelope.clone())?;
        Ok(envelope)
    }

    /// Applies an event to in-memory domain state. Used both for live mutations and replay.
    fn apply_event(&mut self, envelope: &EventEnvelope) -> Result<(), ReplayError> {
        let now = envelope.timestamp;

        match &envelope.event {
            ControlPlaneEvent::StudioCreated { studio } => {
                self.studios.insert(studio.id, studio.clone());
                self.tasks_by_studio.entry(studio.id).or_default();
                self.agents_by_studio.entry(studio.id).or_default();
            }

            ControlPlaneEvent::AgentRegistered { agent } => {
                self.agents.insert(agent.id, agent.clone());
                self.agents_by_studio
                    .entry(agent.studio_id)
                    .or_default()
                    .insert(agent.id);
            }

            ControlPlaneEvent::AgentStateChanged {
                agent_id,
                new_state,
                ..
            } => {
                if let Some(agent) = self.agents.get_mut(agent_id) {
                    agent.state = *new_state;
                }
            }

            ControlPlaneEvent::TaskCreated { task } => {
                self.tasks_by_studio
                    .entry(task.studio_id)
                    .or_default()
                    .insert(task.id);
                self.task_graph
                    .add_task(task.clone())
                    .map_err(|e| ReplayError::DomainViolation(e.to_string()))?;
            }

            ControlPlaneEvent::TaskStateChanged {
                task_id, new_state, ..
            } => {
                if let Some(task) = self.task_graph.get_task_mut(*task_id) {
                    task.state = *new_state;
                    task.updated_at = now;
                }
            }

            ControlPlaneEvent::TaskDependencyAdded {
                task_id,
                dependency_id,
            } => {
                self.task_graph
                    .add_dependency(*task_id, *dependency_id)
                    .map_err(|e| ReplayError::DomainViolation(e.to_string()))?;
            }

            ControlPlaneEvent::TaskDependencyRemoved {
                task_id,
                dependency_id,
            } => {
                self.task_graph
                    .remove_dependency(*task_id, *dependency_id)
                    .map_err(|e| ReplayError::DomainViolation(e.to_string()))?;
            }

            ControlPlaneEvent::RunCreated { run } => {
                self.runs.insert(run.id, run.clone());
                self.runs_by_task
                    .entry(run.task_id)
                    .or_default()
                    .push(run.id);
            }

            ControlPlaneEvent::RunStateChanged {
                run_id, new_state, ..
            } => {
                if let Some(run) = self.runs.get_mut(run_id) {
                    run.state = *new_state;
                    if *new_state == RunState::Running && run.started_at.is_none() {
                        run.started_at = Some(now);
                    } else if new_state.is_terminal() && run.finished_at.is_none() {
                        run.finished_at = Some(now);
                    }
                }
            }

            ControlPlaneEvent::ApprovalRequested { approval } => {
                self.approvals.insert(approval.id, approval.clone());
            }

            ControlPlaneEvent::ApprovalResolved {
                approval_id,
                new_state,
                ..
            } => {
                if let Some(approval) = self.approvals.get_mut(approval_id) {
                    approval.state = *new_state;
                    approval.resolved_at = Some(now);
                }
            }

            ControlPlaneEvent::ArtifactRegistered { artifact } => {
                self.artifacts.insert(artifact.id, artifact.clone());
            }

            ControlPlaneEvent::CancellationRequested { .. } => {
                // Informational envelope marker; state changes handled by subsequent entity state events.
            }
        }

        Ok(())
    }

    pub fn all_studios(&self) -> impl Iterator<Item = &Studio> {
        self.studios.values()
    }

    pub fn all_agents(&self) -> impl Iterator<Item = &AgentDescriptor> {
        self.agents.values()
    }

    pub fn all_tasks(&self) -> impl Iterator<Item = &TaskRecord> {
        self.task_graph.all_tasks()
    }

    pub fn all_runs(&self) -> impl Iterator<Item = &RunRecord> {
        self.runs.values()
    }

    pub fn all_approvals(&self) -> impl Iterator<Item = &ApprovalRequest> {
        self.approvals.values()
    }

    pub fn all_artifacts(&self) -> impl Iterator<Item = &ArtifactRecord> {
        self.artifacts.values()
    }

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

    /// Reconstructs a brand-new ControlPlane instance strictly by replaying an event sequence.
    /// Does NOT emit duplicate events to the store.
    pub fn replay_events(
        events: &[EventEnvelope],
        clock: C,
        store: S,
    ) -> Result<Self, ReplayError> {
        let mut expected_sequences: HashMap<StudioId, u64> = HashMap::new();
        let mut engine = Self {
            clock,
            store,
            studios: HashMap::new(),
            agents: HashMap::new(),
            task_graph: TaskGraph::new(),
            runs: HashMap::new(),
            approvals: HashMap::new(),
            artifacts: HashMap::new(),
            runs_by_task: HashMap::new(),
            tasks_by_studio: HashMap::new(),
            agents_by_studio: HashMap::new(),
        };

        for envelope in events {
            let studio_id = envelope.studio_id;
            let expected = expected_sequences.get(&studio_id).copied().unwrap_or(1);

            if envelope.sequence != expected {
                return Err(ReplayError::InvalidSequence {
                    expected,
                    actual: envelope.sequence,
                });
            }

            expected_sequences.insert(studio_id, expected + 1);

            // Populate store with original envelopes
            engine
                .store
                .append(envelope.clone())
                .map_err(|e| ReplayError::DomainViolation(e.to_string()))?;

            // Mutate in-memory state
            engine.apply_event(envelope)?;
        }

        Ok(engine)
    }
}
