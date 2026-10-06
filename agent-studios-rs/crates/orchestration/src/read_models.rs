use std::collections::HashMap;

use agent_studios_protocol::agent::{AgentExecutionBudget, AgentState};
use agent_studios_protocol::artifact::{ArtifactKind, ArtifactRecord};
use agent_studios_protocol::id::{
    AgentId, ArtifactId, ReconciliationId, RunId, StudioId, TaskId, WorktreeId,
};
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::{TaskRecord, TaskState};
use agent_studios_protocol::worktree::WorktreeRecord;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Operational execution state of an agent (separate from persistent lifecycle state).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentOperationalState {
    #[default]
    Idle,
    Running,
    Waiting,
    Blocked,
    Retrying,
    Paused,
    Failed,
    Completed,
    Stopped,
}

/// Observable usage across budget dimensions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BudgetUsage {
    pub turns_used: u32,
    pub tool_calls_used: u32,
    pub wall_clock_secs_used: u64,
    pub child_agents_used: u32,
}

/// Consolidated read model representing an agent in a Studio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSummary {
    pub agent_id: AgentId,
    pub studio_id: StudioId,
    pub agent_name: String,
    pub role: Option<String>,
    pub state: AgentState,
    pub operational_state: AgentOperationalState,
    pub runtime_bound: bool,
    pub runtime_kind: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub wire_protocol: Option<String>,
    pub codex_thread_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub parent_agent_id: Option<AgentId>,
    pub spawn_depth: u32,
    pub current_task_id: Option<TaskId>,
    pub tool_calls_total: u64,
    pub tool_calls_active: u64,
    pub budget: AgentExecutionBudget,
    pub budget_usage: BudgetUsage,
    pub budget_exceeded: bool,
    pub budget_exceeded_dimension: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Point-in-time snapshot of the Studio task dependency graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskGraphSnapshot {
    pub studio_id: StudioId,
    pub tasks: Vec<TaskRecord>,
    pub ready_tasks: Vec<TaskId>,
    pub running_tasks: Vec<TaskId>,
    pub blocked_tasks: Vec<TaskId>,
    pub retrying_tasks: Vec<TaskId>,
    pub succeeded_tasks: Vec<TaskId>,
    pub failed_tasks: Vec<TaskId>,
    pub cancelled_tasks: Vec<TaskId>,
    /// Dependency adjacency: TaskId -> Vec of prerequisite TaskIds it depends on.
    pub adjacency: HashMap<TaskId, Vec<TaskId>>,
    /// Worktree assigned to each task (M09).
    #[serde(default)]
    pub task_worktrees: HashMap<TaskId, WorktreeId>,
    /// Artifacts produced by each task (M09).
    #[serde(default)]
    pub task_artifacts: HashMap<TaskId, Vec<ArtifactId>>,
    /// Reconciliations associated with each task (M09).
    #[serde(default)]
    pub task_reconciliations: HashMap<TaskId, Vec<ReconciliationId>>,
    /// Structured block reason for each blocked or pending task (M09.1).
    #[serde(default)]
    pub task_block_reasons: HashMap<TaskId, agent_studios_protocol::task::TaskBlockReason>,
}

/// Observable summary of an execution run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSummary {
    pub run_id: RunId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub state: RunState,
    pub attempt: u32,
    pub retry_count: u32,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
    pub failure_classification: Option<String>,
    pub safe_error_summary: Option<String>,
}

/// Final outcome of a completed run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunOutcome {
    pub run_id: RunId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub state: RunState,
    pub duration_ms: Option<u64>,
    pub tool_calls_count: u64,
    pub error: Option<String>,
}

/// Point-in-time index of artifacts registered in a Studio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ArtifactIndex {
    pub studio_id: StudioId,
    pub artifacts: Vec<ArtifactRecord>,
    pub by_kind: HashMap<ArtifactKind, Vec<ArtifactId>>,
    pub by_task: HashMap<TaskId, Vec<ArtifactId>>,
    pub by_worktree: HashMap<WorktreeId, Vec<ArtifactId>>,
    pub by_agent: HashMap<AgentId, Vec<ArtifactId>>,
    pub total_bytes: u64,
}

impl ArtifactIndex {
    pub fn get_artifact(&self, id: &ArtifactId) -> Option<&ArtifactRecord> {
        self.artifacts.iter().find(|a| a.id == *id)
    }

    pub fn artifacts_for_task(&self, task_id: &TaskId) -> Vec<&ArtifactRecord> {
        self.artifacts
            .iter()
            .filter(|a| a.task_id == *task_id)
            .collect()
    }

    pub fn artifacts_for_worktree(&self, worktree_id: &WorktreeId) -> Vec<&ArtifactRecord> {
        self.artifacts
            .iter()
            .filter(|a| a.worktree_id == Some(*worktree_id))
            .collect()
    }

    pub fn artifacts_of_kind(&self, kind: ArtifactKind) -> Vec<&ArtifactRecord> {
        self.artifacts.iter().filter(|a| a.kind == kind).collect()
    }

    /// Finds the highest version artifact in an artifact family.
    pub fn latest_by_family(
        &self,
        studio_id: StudioId,
        task_id: TaskId,
        kind: ArtifactKind,
        logical_name: &str,
    ) -> Option<&ArtifactRecord> {
        self.artifacts
            .iter()
            .filter(|a| {
                a.studio_id == studio_id
                    && a.task_id == task_id
                    && a.kind == kind
                    && a.logical_name == logical_name
            })
            .max_by_key(|a| a.version)
    }

    /// Returns all versions in an artifact family, sorted ascending by version.
    pub fn versions_for_family(
        &self,
        studio_id: StudioId,
        task_id: TaskId,
        kind: ArtifactKind,
        logical_name: &str,
    ) -> Vec<&ArtifactRecord> {
        let mut list: Vec<&ArtifactRecord> = self
            .artifacts
            .iter()
            .filter(|a| {
                a.studio_id == studio_id
                    && a.task_id == task_id
                    && a.kind == kind
                    && a.logical_name == logical_name
            })
            .collect();
        list.sort_by_key(|a| a.version);
        list
    }

    /// Traces the full lineage of an artifact, walking backwards from the given artifact
    /// through its `supersedes` references.
    pub fn lineage_of(&self, artifact_id: &ArtifactId) -> Vec<&ArtifactRecord> {
        let mut lineage = Vec::new();
        let mut current_id = Some(*artifact_id);

        while let Some(id) = current_id {
            if let Some(artifact) = self.get_artifact(&id) {
                lineage.push(artifact);
                current_id = artifact.supersedes;
            } else {
                break;
            }
        }

        lineage
    }
}

/// Point-in-time snapshot of managed worktrees in a Studio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorktreeSnapshot {
    pub studio_id: StudioId,
    pub worktrees: Vec<WorktreeRecord>,
    pub active_worktrees: Vec<WorktreeId>,
    pub retained_worktrees: Vec<WorktreeId>,
    pub by_task: HashMap<TaskId, WorktreeId>,
    pub by_thread: HashMap<String, WorktreeId>,
}

impl WorktreeSnapshot {
    pub fn get_worktree(&self, id: &WorktreeId) -> Option<&WorktreeRecord> {
        self.worktrees.iter().find(|w| w.id == *id)
    }

    pub fn worktree_for_task(&self, task_id: &TaskId) -> Option<&WorktreeRecord> {
        self.by_task
            .get(task_id)
            .and_then(|wt_id| self.get_worktree(wt_id))
    }

    pub fn worktree_for_thread(&self, thread_id: &str) -> Option<&WorktreeRecord> {
        self.by_thread
            .get(thread_id)
            .and_then(|wt_id| self.get_worktree(wt_id))
    }
}

/// Category of event in a task timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineItemKind {
    TaskCreated,
    TaskStateChanged,
    TaskRetryScheduled,
    RunCreated,
    RunStateChanged,
    WorktreeAssigned,
    WorktreeThreadBound,
    WorktreeChangeCaptured,
    WorktreeReleased,
    ArtifactRegistered,
    ReconciliationCreated,
    ReconciliationStateChanged,
    ReconciliationConflictDetected,
    ReconciliationApplied,
}

/// Chronological item in a task timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineItem {
    pub timestamp: DateTime<Utc>,
    pub sequence: u64,
    pub kind: TimelineItemKind,
    pub description: String,
    pub related_id: Option<String>,
}

/// Chronological projection of all events and lifecycle activities for a Task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskTimelineProjection {
    pub task_id: TaskId,
    pub studio_id: StudioId,
    pub current_state: TaskState,
    pub assigned_agent_id: Option<AgentId>,
    pub assigned_worktree_id: Option<WorktreeId>,
    pub bound_thread_id: Option<String>,
    pub artifact_ids: Vec<ArtifactId>,
    pub reconciliation_ids: Vec<ReconciliationId>,
    pub items: Vec<TimelineItem>,
}
