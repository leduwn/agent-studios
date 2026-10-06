use std::collections::{HashMap, HashSet};

use agent_studios_control_plane::engine::ControlPlaneState;
use agent_studios_protocol::agent::{AgentExecutionBudget, AgentState};
use agent_studios_protocol::artifact::ArtifactRecord;
use agent_studios_protocol::event::{ControlPlaneEvent, EventEnvelope};
use agent_studios_protocol::id::{
    AgentId, ArtifactId, ReconciliationId, RunId, StudioId, TaskId, WorktreeId,
};
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::TaskState;
use agent_studios_protocol::worktree::{WorktreeRecord, WorktreeState};

use crate::read_models::{
    AgentOperationalState, AgentSummary, ArtifactIndex, BudgetUsage, RunSummary, TaskGraphSnapshot,
    TaskTimelineProjection, TimelineItem, TimelineItemKind, WorktreeSnapshot,
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

    let mut task_worktrees = HashMap::new();
    let mut task_artifacts: HashMap<TaskId, Vec<ArtifactId>> = HashMap::new();
    let mut task_reconciliations: HashMap<TaskId, Vec<ReconciliationId>> = HashMap::new();

    for wt in state.worktrees.values() {
        if let Some(tid) = wt.assigned_task_id.filter(|_| wt.studio_id == studio_id) {
            task_worktrees.insert(tid, wt.id);
        }
    }

    for art in state.artifacts.values() {
        if art.studio_id == studio_id {
            task_artifacts.entry(art.task_id).or_default().push(art.id);
        }
    }
    for arts in task_artifacts.values_mut() {
        arts.sort();
    }

    for rec in state.reconciliations.values() {
        if rec.studio_id == studio_id {
            task_reconciliations
                .entry(rec.task_id)
                .or_default()
                .push(rec.id);
        }
    }
    for recs in task_reconciliations.values_mut() {
        recs.sort();
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
        task_worktrees,
        task_artifacts,
        task_reconciliations,
    }
}

/// Projects events into an ArtifactIndex for a studio.
pub fn project_artifact_index(studio_id: StudioId, events: &[EventEnvelope]) -> ArtifactIndex {
    let mut artifacts: HashMap<ArtifactId, ArtifactRecord> = HashMap::new();

    for envelope in events {
        if envelope.studio_id != studio_id {
            continue;
        }

        if let ControlPlaneEvent::ArtifactRegistered { artifact } = &envelope.event {
            artifacts.insert(artifact.id, artifact.clone());
        }
    }

    let mut list: Vec<ArtifactRecord> = artifacts.into_values().collect();
    list.sort_by_key(|a| (a.created_at, a.id));

    let mut by_kind: HashMap<agent_studios_protocol::artifact::ArtifactKind, Vec<ArtifactId>> =
        HashMap::new();
    let mut by_task: HashMap<TaskId, Vec<ArtifactId>> = HashMap::new();
    let mut by_worktree: HashMap<WorktreeId, Vec<ArtifactId>> = HashMap::new();
    let mut by_agent: HashMap<AgentId, Vec<ArtifactId>> = HashMap::new();
    let mut total_bytes = 0u64;

    for art in &list {
        by_kind.entry(art.kind).or_default().push(art.id);
        by_task.entry(art.task_id).or_default().push(art.id);
        if let Some(wt_id) = art.worktree_id {
            by_worktree.entry(wt_id).or_default().push(art.id);
        }
        by_agent
            .entry(art.producer_agent_id)
            .or_default()
            .push(art.id);
        if let Some(bytes) = art.size_bytes {
            total_bytes = total_bytes.saturating_add(bytes);
        }
    }

    for ids in by_kind.values_mut() {
        ids.sort();
    }
    for ids in by_task.values_mut() {
        ids.sort();
    }
    for ids in by_worktree.values_mut() {
        ids.sort();
    }
    for ids in by_agent.values_mut() {
        ids.sort();
    }

    ArtifactIndex {
        studio_id,
        artifacts: list,
        by_kind,
        by_task,
        by_worktree,
        by_agent,
        total_bytes,
    }
}

/// Builds an ArtifactIndex from ControlPlaneState.
pub fn build_artifact_index(studio_id: StudioId, state: &ControlPlaneState) -> ArtifactIndex {
    let mut list: Vec<ArtifactRecord> = state
        .artifacts
        .values()
        .filter(|a| a.studio_id == studio_id)
        .cloned()
        .collect();

    list.sort_by_key(|a| (a.created_at, a.id));

    let mut by_kind: HashMap<agent_studios_protocol::artifact::ArtifactKind, Vec<ArtifactId>> =
        HashMap::new();
    let mut by_task: HashMap<TaskId, Vec<ArtifactId>> = HashMap::new();
    let mut by_worktree: HashMap<WorktreeId, Vec<ArtifactId>> = HashMap::new();
    let mut by_agent: HashMap<AgentId, Vec<ArtifactId>> = HashMap::new();
    let mut total_bytes = 0u64;

    for art in &list {
        by_kind.entry(art.kind).or_default().push(art.id);
        by_task.entry(art.task_id).or_default().push(art.id);
        if let Some(wt_id) = art.worktree_id {
            by_worktree.entry(wt_id).or_default().push(art.id);
        }
        by_agent
            .entry(art.producer_agent_id)
            .or_default()
            .push(art.id);
        if let Some(bytes) = art.size_bytes {
            total_bytes = total_bytes.saturating_add(bytes);
        }
    }

    for ids in by_kind.values_mut() {
        ids.sort();
    }
    for ids in by_task.values_mut() {
        ids.sort();
    }
    for ids in by_worktree.values_mut() {
        ids.sort();
    }
    for ids in by_agent.values_mut() {
        ids.sort();
    }

    ArtifactIndex {
        studio_id,
        artifacts: list,
        by_kind,
        by_task,
        by_worktree,
        by_agent,
        total_bytes,
    }
}

/// Projects events into a WorktreeSnapshot for a studio.
pub fn project_worktree_snapshot(
    studio_id: StudioId,
    events: &[EventEnvelope],
) -> WorktreeSnapshot {
    let mut worktrees: HashMap<WorktreeId, WorktreeRecord> = HashMap::new();

    for envelope in events {
        if envelope.studio_id != studio_id {
            continue;
        }

        match &envelope.event {
            ControlPlaneEvent::WorktreeCreated { worktree } => {
                worktrees.insert(worktree.id, worktree.clone());
            }
            ControlPlaneEvent::WorktreeAssigned {
                worktree_id,
                task_id,
                agent_id,
                run_id,
            } => {
                if let Some(wt) = worktrees.get_mut(worktree_id) {
                    wt.assigned_task_id = Some(*task_id);
                    wt.assigned_agent_id = Some(*agent_id);
                    wt.assigned_run_id = *run_id;
                    wt.updated_at = envelope.timestamp;
                }
            }
            ControlPlaneEvent::WorktreeThreadBound {
                worktree_id,
                thread_id,
            } => {
                if let Some(wt) = worktrees.get_mut(worktree_id) {
                    wt.bound_thread_id = Some(thread_id.clone());
                    wt.updated_at = envelope.timestamp;
                }
            }
            ControlPlaneEvent::WorktreeStateChanged {
                worktree_id,
                new_state,
                ..
            } => {
                if let Some(wt) = worktrees.get_mut(worktree_id) {
                    wt.state = *new_state;
                    wt.updated_at = envelope.timestamp;
                }
            }
            ControlPlaneEvent::WorktreeChangeCaptured {
                worktree_id,
                patch_artifact_id,
                head_commit,
                ..
            } => {
                if let Some(wt) = worktrees.get_mut(worktree_id) {
                    wt.patch_artifact_id = Some(*patch_artifact_id);
                    if let Some(head) = head_commit {
                        wt.last_captured_commit = Some(head.clone());
                    }
                    wt.updated_at = envelope.timestamp;
                }
            }
            ControlPlaneEvent::WorktreeReleased {
                worktree_id,
                retained,
                reason,
            } => {
                if let Some(wt) = worktrees.get_mut(worktree_id) {
                    wt.released_at = Some(envelope.timestamp);
                    wt.retained = *retained;
                    wt.retained_reason = reason.clone();
                    wt.updated_at = envelope.timestamp;
                }
            }
            _ => {}
        }
    }

    let mut list: Vec<WorktreeRecord> = worktrees.into_values().collect();
    list.sort_by_key(|w| (w.created_at, w.id));

    let mut active_worktrees = Vec::new();
    let mut retained_worktrees = Vec::new();
    let mut by_task = HashMap::new();
    let mut by_thread = HashMap::new();

    for wt in &list {
        if wt.state.is_active() {
            active_worktrees.push(wt.id);
        }
        if wt.retained || wt.state == WorktreeState::Retained {
            retained_worktrees.push(wt.id);
        }
        if let Some(task_id) = wt.assigned_task_id {
            by_task.insert(task_id, wt.id);
        }
        if let Some(ref thread_id) = wt.bound_thread_id {
            by_thread.insert(thread_id.clone(), wt.id);
        }
    }

    active_worktrees.sort();
    retained_worktrees.sort();

    WorktreeSnapshot {
        studio_id,
        worktrees: list,
        active_worktrees,
        retained_worktrees,
        by_task,
        by_thread,
    }
}

/// Builds a WorktreeSnapshot from ControlPlaneState.
pub fn build_worktree_snapshot(studio_id: StudioId, state: &ControlPlaneState) -> WorktreeSnapshot {
    let mut list: Vec<WorktreeRecord> = state
        .worktrees
        .values()
        .filter(|w| w.studio_id == studio_id)
        .cloned()
        .collect();

    list.sort_by_key(|w| (w.created_at, w.id));

    let mut active_worktrees = Vec::new();
    let mut retained_worktrees = Vec::new();
    let mut by_task = HashMap::new();
    let mut by_thread = HashMap::new();

    for wt in &list {
        if wt.state.is_active() {
            active_worktrees.push(wt.id);
        }
        if wt.retained || wt.state == WorktreeState::Retained {
            retained_worktrees.push(wt.id);
        }
        if let Some(task_id) = wt.assigned_task_id {
            by_task.insert(task_id, wt.id);
        }
        if let Some(ref thread_id) = wt.bound_thread_id {
            by_thread.insert(thread_id.clone(), wt.id);
        }
    }

    active_worktrees.sort();
    retained_worktrees.sort();

    WorktreeSnapshot {
        studio_id,
        worktrees: list,
        active_worktrees,
        retained_worktrees,
        by_task,
        by_thread,
    }
}

/// Projects events into a TaskTimelineProjection for a specific task.
pub fn project_task_timeline(
    task_id: TaskId,
    events: &[EventEnvelope],
) -> Option<TaskTimelineProjection> {
    let mut task_record: Option<(StudioId, TaskState, Option<AgentId>)> = None;
    let mut items = Vec::new();
    let mut assigned_worktree_id = None;
    let mut bound_thread_id = None;
    let mut artifact_ids = Vec::new();
    let mut reconciliation_ids = Vec::new();

    let mut task_runs = HashSet::new();
    let mut task_reconciliations = HashSet::new();

    for envelope in events {
        match &envelope.event {
            ControlPlaneEvent::TaskCreated { task } if task.id == task_id => {
                task_record = Some((task.studio_id, task.state, task.assigned_agent_id));
                items.push(TimelineItem {
                    timestamp: envelope.timestamp,
                    sequence: envelope.sequence,
                    kind: TimelineItemKind::TaskCreated,
                    description: format!("Task created: {}", task.title),
                    related_id: Some(task.id.to_string()),
                });
            }
            ControlPlaneEvent::TaskStateChanged {
                task_id: tid,
                previous_state,
                new_state,
            } if *tid == task_id => {
                if let Some((_, state, _)) = task_record.as_mut() {
                    *state = *new_state;
                }
                items.push(TimelineItem {
                    timestamp: envelope.timestamp,
                    sequence: envelope.sequence,
                    kind: TimelineItemKind::TaskStateChanged,
                    description: format!(
                        "State transitioned from {:?} to {:?}",
                        previous_state, new_state
                    ),
                    related_id: Some(tid.to_string()),
                });
            }
            ControlPlaneEvent::TaskRetryScheduled {
                task_id: tid,
                attempt,
                reason,
                ..
            } if *tid == task_id => {
                if let Some((_, state, _)) = task_record.as_mut() {
                    *state = TaskState::Retrying;
                }
                items.push(TimelineItem {
                    timestamp: envelope.timestamp,
                    sequence: envelope.sequence,
                    kind: TimelineItemKind::TaskRetryScheduled,
                    description: format!("Retry attempt {} scheduled: {}", attempt, reason),
                    related_id: Some(tid.to_string()),
                });
            }
            ControlPlaneEvent::RunCreated { run } if run.task_id == task_id => {
                task_runs.insert(run.id);
                items.push(TimelineItem {
                    timestamp: envelope.timestamp,
                    sequence: envelope.sequence,
                    kind: TimelineItemKind::RunCreated,
                    description: format!("Run {} created (attempt {})", run.id, run.attempt),
                    related_id: Some(run.id.to_string()),
                });
            }
            ControlPlaneEvent::RunStateChanged {
                run_id,
                previous_state,
                new_state,
            } if task_runs.contains(run_id) => {
                items.push(TimelineItem {
                    timestamp: envelope.timestamp,
                    sequence: envelope.sequence,
                    kind: TimelineItemKind::RunStateChanged,
                    description: format!(
                        "Run {} transitioned from {:?} to {:?}",
                        run_id, previous_state, new_state
                    ),
                    related_id: Some(run_id.to_string()),
                });
            }
            ControlPlaneEvent::WorktreeAssigned {
                worktree_id,
                task_id: tid,
                ..
            } if *tid == task_id => {
                assigned_worktree_id = Some(*worktree_id);
                items.push(TimelineItem {
                    timestamp: envelope.timestamp,
                    sequence: envelope.sequence,
                    kind: TimelineItemKind::WorktreeAssigned,
                    description: format!("Worktree {} assigned to task", worktree_id),
                    related_id: Some(worktree_id.to_string()),
                });
            }
            ControlPlaneEvent::WorktreeThreadBound {
                worktree_id,
                thread_id,
            } if assigned_worktree_id == Some(*worktree_id) => {
                bound_thread_id = Some(thread_id.clone());
                items.push(TimelineItem {
                    timestamp: envelope.timestamp,
                    sequence: envelope.sequence,
                    kind: TimelineItemKind::WorktreeThreadBound,
                    description: format!("Thread {} bound to worktree {}", thread_id, worktree_id),
                    related_id: Some(thread_id.clone()),
                });
            }
            ControlPlaneEvent::WorktreeChangeCaptured {
                worktree_id,
                patch_artifact_id,
                files_changed,
                ..
            } if assigned_worktree_id == Some(*worktree_id) => {
                if !artifact_ids.contains(patch_artifact_id) {
                    artifact_ids.push(*patch_artifact_id);
                }
                items.push(TimelineItem {
                    timestamp: envelope.timestamp,
                    sequence: envelope.sequence,
                    kind: TimelineItemKind::WorktreeChangeCaptured,
                    description: format!(
                        "Captured {} changed files into patch artifact {}",
                        files_changed, patch_artifact_id
                    ),
                    related_id: Some(patch_artifact_id.to_string()),
                });
            }
            ControlPlaneEvent::WorktreeReleased {
                worktree_id,
                retained,
                reason,
            } if assigned_worktree_id == Some(*worktree_id) => {
                items.push(TimelineItem {
                    timestamp: envelope.timestamp,
                    sequence: envelope.sequence,
                    kind: TimelineItemKind::WorktreeReleased,
                    description: format!(
                        "Worktree released (retained={}{})",
                        retained,
                        reason
                            .as_ref()
                            .map(|r| format!(", reason={}", r))
                            .unwrap_or_default()
                    ),
                    related_id: Some(worktree_id.to_string()),
                });
            }
            ControlPlaneEvent::ArtifactRegistered { artifact } if artifact.task_id == task_id => {
                if !artifact_ids.contains(&artifact.id) {
                    artifact_ids.push(artifact.id);
                }
                items.push(TimelineItem {
                    timestamp: envelope.timestamp,
                    sequence: envelope.sequence,
                    kind: TimelineItemKind::ArtifactRegistered,
                    description: format!(
                        "Artifact registered: {} ({:?})",
                        artifact.logical_name, artifact.kind
                    ),
                    related_id: Some(artifact.id.to_string()),
                });
            }
            ControlPlaneEvent::ReconciliationCreated { reconciliation }
                if reconciliation.task_id == task_id =>
            {
                task_reconciliations.insert(reconciliation.id);
                if !reconciliation_ids.contains(&reconciliation.id) {
                    reconciliation_ids.push(reconciliation.id);
                }
                items.push(TimelineItem {
                    timestamp: envelope.timestamp,
                    sequence: envelope.sequence,
                    kind: TimelineItemKind::ReconciliationCreated,
                    description: format!("Reconciliation {} created", reconciliation.id),
                    related_id: Some(reconciliation.id.to_string()),
                });
            }
            ControlPlaneEvent::ReconciliationStateChanged {
                reconciliation_id,
                previous_state,
                new_state,
            } if task_reconciliations.contains(reconciliation_id) => {
                items.push(TimelineItem {
                    timestamp: envelope.timestamp,
                    sequence: envelope.sequence,
                    kind: TimelineItemKind::ReconciliationStateChanged,
                    description: format!(
                        "Reconciliation {} transitioned from {:?} to {:?}",
                        reconciliation_id, previous_state, new_state
                    ),
                    related_id: Some(reconciliation_id.to_string()),
                });
            }
            ControlPlaneEvent::ReconciliationConflictDetected {
                reconciliation_id,
                conflicted_files,
                reason,
            } if task_reconciliations.contains(reconciliation_id) => {
                items.push(TimelineItem {
                    timestamp: envelope.timestamp,
                    sequence: envelope.sequence,
                    kind: TimelineItemKind::ReconciliationConflictDetected,
                    description: format!(
                        "Reconciliation conflict in {:?}: {}",
                        conflicted_files, reason
                    ),
                    related_id: Some(reconciliation_id.to_string()),
                });
            }
            ControlPlaneEvent::ReconciliationApplied {
                reconciliation_id,
                merge_commit,
            } if task_reconciliations.contains(reconciliation_id) => {
                items.push(TimelineItem {
                    timestamp: envelope.timestamp,
                    sequence: envelope.sequence,
                    kind: TimelineItemKind::ReconciliationApplied,
                    description: format!(
                        "Reconciliation applied successfully (merge commit: {:?})",
                        merge_commit
                    ),
                    related_id: Some(reconciliation_id.to_string()),
                });
            }
            _ => {}
        }
    }

    let (studio_id, current_state, assigned_agent_id) = task_record?;

    items.sort_by_key(|item| item.sequence);

    Some(TaskTimelineProjection {
        task_id,
        studio_id,
        current_state,
        assigned_agent_id,
        assigned_worktree_id,
        bound_thread_id,
        artifact_ids,
        reconciliation_ids,
        items,
    })
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
