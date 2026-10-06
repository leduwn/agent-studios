use agent_studios_protocol::error::TransitionError;
use agent_studios_protocol::id::{
    AgentId, ApprovalId, ArtifactId, EventId, ReconciliationId, RunId, StudioId, TaskId,
    WorktreeId,
};
use thiserror::Error;

/// Root error type for Agent Studios control-plane domain operations.
#[derive(Debug, Error)]
pub enum ControlPlaneError {
    #[error("Studio {0} not found")]
    StudioNotFound(StudioId),

    #[error("Agent {0} not found")]
    AgentNotFound(AgentId),

    #[error("Agent {0} already registered")]
    DuplicateAgent(AgentId),

    #[error("Task {0} not found")]
    TaskNotFound(TaskId),

    #[error("Run {0} not found")]
    RunNotFound(RunId),

    #[error("Approval request {0} not found")]
    ApprovalNotFound(ApprovalId),

    #[error("Artifact {0} not found")]
    ArtifactNotFound(ArtifactId),

    #[error("Worktree {0} not found")]
    WorktreeNotFound(WorktreeId),

    #[error("Worktree {0} already exists")]
    DuplicateWorktree(WorktreeId),

    #[error("Worktree ownership conflict for {worktree_id}: {reason}")]
    WorktreeOwnershipConflict {
        worktree_id: WorktreeId,
        reason: String,
    },

    #[error("Reconciliation {0} not found")]
    ReconciliationNotFound(ReconciliationId),

    #[error("Reconciliation {0} already exists")]
    DuplicateReconciliation(ReconciliationId),

    #[error("Studio mismatch: entity belongs to {actual}, expected {expected}")]
    StudioMismatch {
        expected: StudioId,
        actual: StudioId,
    },

    #[error(transparent)]
    Transition(#[from] TransitionError),

    #[error(transparent)]
    TaskGraph(#[from] TaskGraphError),

    #[error(transparent)]
    Store(#[from] StoreError),

    #[error(transparent)]
    Replay(#[from] ReplayError),

    #[error("Agent state conflict for {agent_id}: {reason}")]
    AgentStateConflict { agent_id: AgentId, reason: String },

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

/// Errors raised by the TaskGraph DAG engine.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TaskGraphError {
    #[error("Task already exists in graph: {0}")]
    DuplicateTask(TaskId),

    #[error("Task {0} does not exist in graph")]
    UnknownTask(TaskId),

    #[error("Task {0} cannot depend on itself")]
    SelfDependency(TaskId),

    #[error("Dependency cycle detected: adding edge from {from} to {to} forms a cycle")]
    DependencyCycle { from: TaskId, to: TaskId },

    #[error("Cannot safely remove task {task_id}: {dependent_count} tasks depend on it")]
    UnsafeTaskRemoval {
        task_id: TaskId,
        dependent_count: usize,
    },

    #[error("Task studio mismatch: expected {expected}, actual {actual}")]
    StudioMismatch {
        expected: StudioId,
        actual: StudioId,
    },

    #[error("Task graph is malformed: {0}")]
    MalformedGraph(String),
}

/// Errors raised by the event store layer.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StoreError {
    #[error("Sequence gap in studio {studio_id}: expected {expected}, got {actual}")]
    SequenceGap {
        studio_id: StudioId,
        expected: u64,
        actual: u64,
    },

    #[error(
        "Sequence regression in studio {studio_id}: latest is {latest}, tried to append {attempted}"
    )]
    SequenceRegression {
        studio_id: StudioId,
        latest: u64,
        attempted: u64,
    },

    #[error("Duplicate event ID: {event_id}")]
    DuplicateEventId { event_id: EventId },

    #[error("Storage failure: {0}")]
    StorageFailure(String),
}

/// Errors raised during event replay.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReplayError {
    #[error("Unsupported event schema version: {version} (supported: {supported})")]
    UnsupportedSchemaVersion { version: u16, supported: u16 },

    #[error("Invalid event sequence during replay: expected {expected}, got {actual}")]
    InvalidSequence { expected: u64, actual: u64 },

    #[error("Duplicate event ID during replay: {event_id}")]
    DuplicateEventId { event_id: EventId },

    #[error("Studio mismatch in envelope: expected {expected}, actual {actual}")]
    StudioMismatch {
        expected: StudioId,
        actual: StudioId,
    },

    #[error("Studio already exists: {studio_id}")]
    DuplicateStudio { studio_id: StudioId },

    #[error("Agent already registered: {agent_id}")]
    DuplicateAgent { agent_id: AgentId },

    #[error("Task already exists: {task_id}")]
    DuplicateTask { task_id: TaskId },

    #[error("Run already exists: {run_id}")]
    DuplicateRun { run_id: RunId },

    #[error("Approval request already exists: {approval_id}")]
    DuplicateApproval { approval_id: ApprovalId },

    #[error("Artifact already registered: {artifact_id}")]
    DuplicateArtifact { artifact_id: ArtifactId },

    #[error("Worktree already exists: {worktree_id}")]
    DuplicateWorktree { worktree_id: WorktreeId },

    #[error("Reconciliation already exists: {reconciliation_id}")]
    DuplicateReconciliation {
        reconciliation_id: ReconciliationId,
    },

    #[error("Studio not found: {studio_id}")]
    StudioNotFound { studio_id: StudioId },

    #[error("Agent not found: {agent_id}")]
    AgentNotFound { agent_id: AgentId },

    #[error("Task not found: {task_id}")]
    TaskNotFound { task_id: TaskId },

    #[error("Run not found: {run_id}")]
    RunNotFound { run_id: RunId },

    #[error("Approval request not found: {approval_id}")]
    ApprovalNotFound { approval_id: ApprovalId },

    #[error("Worktree not found: {worktree_id}")]
    WorktreeNotFound { worktree_id: WorktreeId },

    #[error("Reconciliation not found: {reconciliation_id}")]
    ReconciliationNotFound {
        reconciliation_id: ReconciliationId,
    },

    #[error("State mismatch: current state is {actual}, event expected {expected}")]
    StateMismatch { actual: String, expected: String },

    #[error("Invalid state transition: {reason}")]
    InvalidTransition { reason: String },

    #[error("Dependency cycle: from {from} to {to}")]
    DependencyCycle { from: TaskId, to: TaskId },

    #[error(transparent)]
    Store(#[from] StoreError),

    #[error("Domain violation during replay: {0}")]
    DomainViolation(String),
}
