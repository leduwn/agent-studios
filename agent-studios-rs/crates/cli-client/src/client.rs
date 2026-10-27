//! Authoritative Client Trait and Core Models for Agent Studios CLI
//!
//! Provides an asynchronous, UI-decoupled contract for both Embedded (in-process)
//! and Connected (daemon/IPC) execution modes.

use std::fmt;
use std::sync::Arc;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use agent_studios_orchestration::read_models::{
    AgentSummary, TaskGraphSnapshot, TaskTimelineProjection,
};
use agent_studios_protocol::event::EventEnvelope;
use agent_studios_protocol::id::{ApprovalId, ReconciliationId, StudioId, TaskId, WorktreeId};
use agent_studios_workspace::capture::CapturedChanges;
use agent_studios_workspace::reconciliation::ReconciliationOutcome;

/// Operating mode of the client session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// In-process Control Plane and Orchestration.
    Embedded,
    /// Connected to external daemon or Code-OSS IDE session via IPC.
    Connected,
}

impl fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Embedded => write!(f, "Embedded"),
            Self::Connected => write!(f, "Connected"),
        }
    }
}

/// Options supplied when submitting a developer goal/prompt.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoalSubmissionOptions {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub runtime: Option<String>,
    pub auto_approve_policy: Option<String>,
}

/// Approval decision forwarded from the TUI security modal to the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientApprovalDecision {
    Allow,
    Deny,
    AllowForSession,
}

/// Summary of an active managed worktree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeRecordSummary {
    pub worktree_id: WorktreeId,
    pub rel_path: String,
    pub state: String,
}

/// Subscription stream returning historical backfill plus live receiver.
pub struct ClientSubscription {
    pub backfill: Vec<EventEnvelope>,
    pub stream: tokio::sync::mpsc::Receiver<EventEnvelope>,
}

/// Errors produced by the Agent Studios client abstraction.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Transport or IPC error: {0}")]
    Transport(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Operation failed: {0}")]
    OperationFailed(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Operation timed out")]
    Timeout,

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Decoupled async interface driving the CLI interface.
#[async_trait]
pub trait AgentStudiosClient: Send + Sync {
    /// Unique identifier for this studio session.
    fn studio_id(&self) -> StudioId;

    /// Current execution mode (Embedded or Connected).
    fn execution_mode(&self) -> ExecutionMode;

    /// Cleanly terminates the client session and associated background workers.
    async fn shutdown(&self) -> Result<(), ClientError>;

    /// Submits a developer prompt/goal into the control plane.
    async fn submit_goal(
        &self,
        prompt: &str,
        options: GoalSubmissionOptions,
    ) -> Result<TaskId, ClientError>;

    /// Cancels active tasks or runs.
    async fn cancel_active_run(&self, reason: Option<String>) -> Result<(), ClientError>;

    /// Subscribes to the authoritative event stream starting from a sequence number.
    async fn subscribe_events(&self, from_sequence: u64) -> Result<ClientSubscription, ClientError>;

    /// Returns the current projection snapshot of the multi-agent task graph.
    async fn get_task_graph(&self) -> Result<TaskGraphSnapshot, ClientError>;

    /// Returns the status summaries of all agents currently assigned to the team.
    async fn get_agent_summaries(&self) -> Result<Vec<AgentSummary>, ClientError>;

    /// Returns the execution timeline projection for a specific task.
    async fn get_task_timeline(&self, task_id: TaskId) -> Result<TaskTimelineProjection, ClientError>;

    /// Resolves an interactive tool execution approval request.
    async fn resolve_approval(
        &self,
        approval_id: ApprovalId,
        decision: ClientApprovalDecision,
    ) -> Result<(), ClientError>;

    /// Lists active git worktrees tracked by the workspace orchestrator.
    async fn list_worktrees(&self) -> Result<Vec<WorktreeRecordSummary>, ClientError>;

    /// Returns captured file diffs for a managed worktree.
    async fn get_worktree_diff(&self, worktree_id: WorktreeId) -> Result<CapturedChanges, ClientError>;

    /// Reconciles an agent patch back into the target branch.
    async fn reconcile_patch(
        &self,
        reconciliation_id: ReconciliationId,
    ) -> Result<ReconciliationOutcome, ClientError>;
}

/// Type alias for thread-safe dynamic client handle.
pub type DynAgentStudiosClient = Arc<dyn AgentStudiosClient>;
