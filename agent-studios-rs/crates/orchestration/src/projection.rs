use std::collections::{HashMap, HashSet};

use agent_studios_control_plane::engine::ControlPlaneState;
use agent_studios_protocol::agent::{AgentExecutionBudget, AgentState};
use agent_studios_protocol::event::{ControlPlaneEvent, EventEnvelope};
use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId};
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::TaskState;

use crate::read_models::{
    AgentOperationalState, AgentSummary, BudgetUsage, RunSummary, TaskGraphSnapshot,
};

/// Projects events into an AgentSummary.
pub fn project_agent_summary(
    agent_id: AgentId,
    events: &[EventEnvelope],
    budget: Option<AgentExecutionBudget>,
) -> Option<AgentSummary> {
    let mut summary: Option<AgentSummary> = None;
    let mut parent_map: HashMap<AgentId, AgentId> = HashMap::new();
    let mut runs: HashMap<RunId, (TaskId, AgentId, RunState)> = HashMap::new();
    let mut tasks_for_agent: HashMap<TaskId, TaskState> = HashMap::new();

    for envelope in events {
        // Collect parent hierarchy across all agent spawn events
        if let ControlPlaneEvent::AgentSpawned {
            agent_id: spawned_id,
            parent_agent_id: Some(parent_id),
            ..
        } = &envelope.event
        {
            parent_map.insert(*spawned_id, *parent_id);
        }

        // Track run and task states relevant to the target agent
        match &envelope.event {
            ControlPlaneEvent::RunCreated { run } => {
                runs.insert(run.id, (run.task_id, run.agent_id, run.state));
            }
            ControlPlaneEvent::RunStateChanged {
                run_id, new_state, ..
            } => {
                if let Some(entry) = runs.get_mut(run_id) {
                    entry.2 = *new_state;
                }
            }
            ControlPlaneEvent::TaskCreated { task } => {
                if task.assigned_agent_id == Some(agent_id) {
                    tasks_for_agent.insert(task.id, task.state);
                }
            }
            ControlPlaneEvent::TaskStateChanged {
                task_id, new_state, ..
            } => {
                if let Some(entry) = tasks_for_agent.get_mut(task_id) {
                    *entry = *new_state;
                }
            }
            ControlPlaneEvent::TaskRetryScheduled { task_id, .. } => {
                if let Some(entry) = tasks_for_agent.get_mut(task_id) {
                    *entry = TaskState::Retrying;
                }
            }
            _ => {}
        }

        match &envelope.event {
            ControlPlaneEvent::AgentRegistered { agent } if agent.id == agent_id => {
                summary = Some(AgentSummary {
                    agent_id,
                    studio_id: agent.studio_id,
                    agent_name: agent.display_name.clone(),
                    role: agent.role.clone(),
                    state: AgentState::Registered,
                    operational_state: AgentOperationalState::Idle,
                    runtime_bound: false,
                    runtime_kind: None,
                    provider: None,
                    model: None,
                    wire_protocol: None,
                    codex_thread_id: None,
                    parent_thread_id: None,
                    parent_agent_id: None,
                    spawn_depth: 0,
                    current_task_id: None,
                    tool_calls_total: 0,
                    tool_calls_active: 0,
                    budget: budget.clone().unwrap_or_default(),
                    budget_usage: BudgetUsage::default(),
                    budget_exceeded: false,
                    budget_exceeded_dimension: None,
                    created_at: envelope.timestamp,
                    updated_at: envelope.timestamp,
                });
            }
            ControlPlaneEvent::AgentStateChanged {
                agent_id: ev_agent_id,
                new_state,
                ..
            } if *ev_agent_id == agent_id => {
                if let Some(s) = summary.as_mut() {
                    s.state = *new_state;
                    s.updated_at = envelope.timestamp;
                }
            }
            ControlPlaneEvent::AgentRuntimeBound {
                agent_id: ev_agent_id,
                runtime_kind,
                provider_instance_id,
                model_id,
                protocol,
                ..
            } if *ev_agent_id == agent_id => {
                if let Some(s) = summary.as_mut() {
                    s.runtime_bound = true;
                    s.runtime_kind = Some(runtime_kind.clone());
                    s.provider = Some(provider_instance_id.clone());
                    s.model = Some(model_id.clone());
                    s.wire_protocol = Some(protocol.clone());
                    s.updated_at = envelope.timestamp;
                }
            }
            ControlPlaneEvent::AgentSpawned {
                agent_id: ev_agent_id,
                thread_id,
                parent_agent_id,
                parent_thread_id,
                ..
            } if *ev_agent_id == agent_id => {
                if let Some(s) = summary.as_mut() {
                    s.codex_thread_id = Some(thread_id.clone());
                    s.parent_agent_id = *parent_agent_id;
                    s.parent_thread_id = parent_thread_id.clone();
                    s.updated_at = envelope.timestamp;
                }
            }
            ControlPlaneEvent::ToolStarted {
                agent_id: ev_agent_id,
                ..
            } if *ev_agent_id == agent_id => {
                if let Some(s) = summary.as_mut() {
                    s.tool_calls_total = s.tool_calls_total.saturating_add(1);
                    s.tool_calls_active = s.tool_calls_active.saturating_add(1);
                    s.budget_usage.tool_calls_used =
                        s.budget_usage.tool_calls_used.saturating_add(1);
                    s.updated_at = envelope.timestamp;
                }
            }
            ControlPlaneEvent::ToolCompleted {
                agent_id: ev_agent_id,
                ..
            }
            | ControlPlaneEvent::ToolFailed {
                agent_id: ev_agent_id,
                ..
            } if *ev_agent_id == agent_id => {
                if let Some(s) = summary.as_mut() {
                    s.tool_calls_active = s.tool_calls_active.saturating_sub(1);
                    s.updated_at = envelope.timestamp;
                }
            }
            ControlPlaneEvent::BudgetUsageUpdated {
                agent_id: ev_agent_id,
                turns_used,
                tool_calls_used,
                wall_clock_secs,
                child_agents_used,
                ..
            } if *ev_agent_id == agent_id => {
                if let Some(s) = summary.as_mut() {
                    s.budget_usage.turns_used = *turns_used;
                    s.budget_usage.tool_calls_used = *tool_calls_used;
                    s.budget_usage.wall_clock_secs_used = *wall_clock_secs;
                    s.budget_usage.child_agents_used = *child_agents_used;
                    s.updated_at = envelope.timestamp;
                }
            }
            ControlPlaneEvent::BudgetExceeded {
                agent_id: ev_agent_id,
                dimension,
                ..
            } if *ev_agent_id == agent_id => {
                if let Some(s) = summary.as_mut() {
                    s.budget_exceeded = true;
                    s.budget_exceeded_dimension = Some(dimension.clone());
                    s.updated_at = envelope.timestamp;
                }
            }
            _ => {}
        }

        // Update operational state and current_task_id after each event for our agent
        if let Some(s) = summary.as_mut() {
            let active_run = runs.values().find(|(_, aid, state)| {
                *aid == agent_id && matches!(state, RunState::Starting | RunState::Running)
            });

            if let Some((task_id, _, _)) = active_run {
                s.current_task_id = Some(*task_id);
                s.operational_state = AgentOperationalState::Running;
            } else {
                s.current_task_id = None;
                if s.state == AgentState::Failed {
                    s.operational_state = AgentOperationalState::Failed;
                } else if s.state == AgentState::Stopped {
                    s.operational_state = AgentOperationalState::Stopped;
                } else if s.state == AgentState::Paused {
                    s.operational_state = AgentOperationalState::Paused;
                } else if tasks_for_agent
                    .values()
                    .any(|st| *st == TaskState::Retrying)
                {
                    s.operational_state = AgentOperationalState::Retrying;
                } else if tasks_for_agent.values().any(|st| *st == TaskState::Blocked) {
                    s.operational_state = AgentOperationalState::Blocked;
                } else if tasks_for_agent.values().any(|st| *st == TaskState::Pending) {
                    s.operational_state = AgentOperationalState::Waiting;
                } else {
                    s.operational_state = AgentOperationalState::Idle;
                }
            }
        }
    }

    // Calculate recursive spawn depth with cycle detection
    if let Some(s) = summary.as_mut() {
        let mut depth = 0;
        let mut curr = s.parent_agent_id;
        let mut visited = HashSet::new();
        visited.insert(agent_id);
        while let Some(parent) = curr {
            if !visited.insert(parent) {
                break;
            }
            depth += 1;
            curr = parent_map.get(&parent).copied();
        }
        s.spawn_depth = depth;
    }

    summary
}

/// Builds a TaskGraphSnapshot from ControlPlaneState.
pub fn build_task_graph_snapshot(
    studio_id: StudioId,
    state: &ControlPlaneState,
) -> TaskGraphSnapshot {
    let mut tasks = Vec::new();
    let mut ready_tasks = Vec::new();
    let mut running_tasks = Vec::new();
    let mut blocked_tasks = Vec::new();
    let mut retrying_tasks = Vec::new();
    let mut succeeded_tasks = Vec::new();
    let mut failed_tasks = Vec::new();
    let mut cancelled_tasks = Vec::new();
    let mut adjacency = HashMap::new();

    for task in state.task_graph.all_tasks() {
        if task.studio_id != studio_id {
            continue;
        }
        tasks.push(task.clone());
        adjacency.insert(task.id, task.dependencies.clone());

        match task.state {
            TaskState::Ready => ready_tasks.push(task.id),
            TaskState::Running => running_tasks.push(task.id),
            TaskState::Pending | TaskState::Blocked | TaskState::Paused => {
                blocked_tasks.push(task.id)
            }
            TaskState::Retrying => retrying_tasks.push(task.id),
            TaskState::Succeeded => succeeded_tasks.push(task.id),
            TaskState::Failed => failed_tasks.push(task.id),
            TaskState::Cancelled => cancelled_tasks.push(task.id),
        }
    }

    tasks.sort_by_key(|a| (a.created_at, a.id));
    ready_tasks.sort();
    running_tasks.sort();
    blocked_tasks.sort();
    retrying_tasks.sort();
    succeeded_tasks.sort();
    failed_tasks.sort();
    cancelled_tasks.sort();
    for deps in adjacency.values_mut() {
        deps.sort();
    }

    TaskGraphSnapshot {
        studio_id,
        tasks,
        ready_tasks,
        running_tasks,
        blocked_tasks,
        retrying_tasks,
        succeeded_tasks,
        failed_tasks,
        cancelled_tasks,
        adjacency,
    }
}

/// Projects events into a RunSummary.
pub fn project_run_summary(run_id: RunId, events: &[EventEnvelope]) -> Option<RunSummary> {
    let mut summary: Option<RunSummary> = None;

    for envelope in events {
        match &envelope.event {
            ControlPlaneEvent::RunCreated { run } if run.id == run_id => {
                summary = Some(RunSummary {
                    run_id,
                    task_id: run.task_id,
                    agent_id: run.agent_id,
                    state: RunState::Queued,
                    attempt: run.attempt,
                    retry_count: 0,
                    started_at: None,
                    completed_at: None,
                    duration_ms: None,
                    error: None,
                    failure_classification: None,
                    safe_error_summary: None,
                });
            }
            ControlPlaneEvent::RunStateChanged {
                run_id: ev_run_id,
                new_state,
                ..
            } if *ev_run_id == run_id => {
                if let Some(s) = summary.as_mut() {
                    s.state = *new_state;
                    if matches!(*new_state, RunState::Starting | RunState::Running)
                        && s.started_at.is_none()
                    {
                        s.started_at = Some(envelope.timestamp);
                    }
                    if matches!(
                        *new_state,
                        RunState::Succeeded | RunState::Failed | RunState::Cancelled
                    ) {
                        s.completed_at = Some(envelope.timestamp);
                        if let Some(started) = s.started_at {
                            let duration = envelope
                                .timestamp
                                .signed_duration_since(started)
                                .num_milliseconds();
                            if duration >= 0 {
                                s.duration_ms = Some(duration as u64);
                            }
                        }
                    }
                }
            }
            ControlPlaneEvent::TaskRetryScheduled {
                task_id, attempt, ..
            } => {
                if let Some(s) = summary.as_mut()
                    && s.task_id == *task_id
                {
                    s.retry_count = *attempt;
                }
            }
            ControlPlaneEvent::RunOutcomeRecorded {
                run_id: ev_run_id,
                classification,
                safe_error_summary,
                ..
            } if *ev_run_id == run_id => {
                if let Some(s) = summary.as_mut() {
                    s.failure_classification = Some(classification.clone());
                    s.safe_error_summary = safe_error_summary.clone();
                    if s.error.is_none() {
                        s.error = safe_error_summary.clone();
                    }
                }
            }
            _ => {}
        }
    }

    summary
}
