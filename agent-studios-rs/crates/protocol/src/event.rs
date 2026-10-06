use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agent::{AgentDescriptor, AgentState};
use crate::approval::{ApprovalRequest, ApprovalState};
use crate::artifact::ArtifactRecord;
use crate::cancellation::CancellationScope;
use crate::id::{
    AgentId, ApprovalId, ArtifactId, EventId, ReconciliationId, RunId, StudioId, TaskId, WorktreeId,
};
use crate::reconciliation::{ReconciliationRecord, ReconciliationState};
use crate::run::{RunRecord, RunState};
use crate::studio::Studio;
use crate::task::{TaskRecord, TaskState};
use crate::worktree::{WorktreeRecord, WorktreeState};

/// Canonical domain event variants emitted by the Agent Studios control plane.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ControlPlaneEvent {
    StudioCreated {
        studio: Studio,
    },
    AgentRegistered {
        agent: AgentDescriptor,
    },
    AgentStateChanged {
        agent_id: AgentId,
        previous_state: AgentState,
        new_state: AgentState,
    },
    TaskCreated {
        task: TaskRecord,
    },
    TaskStateChanged {
        task_id: TaskId,
        previous_state: TaskState,
        new_state: TaskState,
    },
    TaskDependencyAdded {
        task_id: TaskId,
        dependency_id: TaskId,
    },
    TaskDependencyRemoved {
        task_id: TaskId,
        dependency_id: TaskId,
    },
    RunCreated {
        run: RunRecord,
    },
    RunStateChanged {
        run_id: RunId,
        previous_state: RunState,
        new_state: RunState,
    },
    ApprovalRequested {
        approval: ApprovalRequest,
    },
    ApprovalResolved {
        approval_id: ApprovalId,
        previous_state: ApprovalState,
        new_state: ApprovalState,
    },
    ArtifactRegistered {
        artifact: ArtifactRecord,
    },
    CancellationRequested {
        scope: CancellationScope,
        reason: Option<String>,
    },
    AgentRuntimeBound {
        agent_id: AgentId,
        runtime_kind: String,
        provider_instance_id: String,
        model_id: String,
        protocol: String,
    },
    AgentSpawned {
        agent_id: AgentId,
        thread_id: String,
        parent_agent_id: Option<AgentId>,
        parent_thread_id: Option<String>,
    },
    ToolStarted {
        agent_id: AgentId,
        task_id: Option<TaskId>,
        run_id: Option<RunId>,
        tool_name: String,
        call_id: String,
        timestamp: DateTime<Utc>,
    },
    ToolCompleted {
        agent_id: AgentId,
        task_id: Option<TaskId>,
        run_id: Option<RunId>,
        tool_name: String,
        call_id: String,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    ToolFailed {
        agent_id: AgentId,
        task_id: Option<TaskId>,
        run_id: Option<RunId>,
        tool_name: String,
        call_id: String,
        error: String,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    BudgetUsageUpdated {
        agent_id: AgentId,
        run_id: Option<RunId>,
        turns_used: u32,
        tool_calls_used: u32,
        wall_clock_secs: u64,
        child_agents_used: u32,
    },
    BudgetExceeded {
        agent_id: AgentId,
        run_id: Option<RunId>,
        dimension: String,
        limit: u64,
        actual: u64,
    },
    TaskRetryScheduled {
        task_id: TaskId,
        attempt: u32,
        max_attempts: u32,
        reason: String,
        backoff_ms: u64,
        next_retry_at: Option<DateTime<Utc>>,
    },
    RunOutcomeRecorded {
        run_id: RunId,
        task_id: TaskId,
        agent_id: AgentId,
        classification: String,
        safe_error_summary: Option<String>,
    },
    WorktreeCreated {
        worktree: WorktreeRecord,
    },
    WorktreeAssigned {
        worktree_id: WorktreeId,
        task_id: TaskId,
        agent_id: AgentId,
        run_id: Option<RunId>,
    },
    WorktreeThreadBound {
        worktree_id: WorktreeId,
        thread_id: String,
    },
    WorktreeStateChanged {
        worktree_id: WorktreeId,
        previous_state: WorktreeState,
        new_state: WorktreeState,
    },
    WorktreeChangeCaptured {
        worktree_id: WorktreeId,
        run_id: Option<RunId>,
        base_commit: String,
        head_commit: Option<String>,
        patch_artifact_id: ArtifactId,
        stats_artifact_id: Option<ArtifactId>,
        files_changed: usize,
    },
    WorktreeReleased {
        worktree_id: WorktreeId,
        retained: bool,
        reason: Option<String>,
    },
    ReconciliationCreated {
        reconciliation: ReconciliationRecord,
    },
    ReconciliationStateChanged {
        reconciliation_id: ReconciliationId,
        previous_state: ReconciliationState,
        new_state: ReconciliationState,
    },
    ReconciliationConflictDetected {
        reconciliation_id: ReconciliationId,
        conflicted_files: Vec<String>,
        reason: String,
    },
    ReconciliationApplied {
        reconciliation_id: ReconciliationId,
        merge_commit: Option<String>,
    },
}

pub const CONTROL_PLANE_EVENT_SCHEMA_VERSION: u16 = 1;

/// Durable sequence-stamped envelope containing an event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_id: EventId,
    pub schema_version: u16,
    pub studio_id: StudioId,
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub event: ControlPlaneEvent,
}

impl EventEnvelope {
    pub fn new(
        studio_id: StudioId,
        sequence: u64,
        timestamp: DateTime<Utc>,
        event: ControlPlaneEvent,
    ) -> Self {
        Self {
            event_id: EventId::new(),
            schema_version: CONTROL_PLANE_EVENT_SCHEMA_VERSION,
            studio_id,
            sequence,
            timestamp,
            event,
        }
    }

    pub fn with_details(
        event_id: EventId,
        schema_version: u16,
        studio_id: StudioId,
        sequence: u64,
        timestamp: DateTime<Utc>,
        event: ControlPlaneEvent,
    ) -> Self {
        Self {
            event_id,
            schema_version,
            studio_id,
            sequence,
            timestamp,
            event,
        }
    }
}
