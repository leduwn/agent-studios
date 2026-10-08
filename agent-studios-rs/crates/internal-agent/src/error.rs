use agent_studios_control_plane::error::ControlPlaneError;
use agent_studios_protocol::id::AgentId;
use agent_studios_runtime_session::error::RuntimeSessionError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InternalAgentError {
    #[error("Control plane error: {0}")]
    ControlPlane(#[from] ControlPlaneError),

    #[error("Runtime session error: {0}")]
    RuntimeSession(#[from] RuntimeSessionError),

    #[error("Control plane actor channel dropped")]
    ActorDropped,

    #[error("Control plane actor panicked")]
    ActorPanicked,

    #[error("Control plane actor command failed: {0}")]
    ActorCommandFailed(String),

    #[error(
        "Workspace conflict in workspace '{workspace_id}': active agent {active_agent} conflicts with requested agent {requested_agent}"
    )]
    WorkspaceConflict {
        workspace_id: String,
        active_agent: AgentId,
        requested_agent: AgentId,
    },

    #[error("Execution budget exceeded for agent {agent_id}: {reason}")]
    BudgetExceeded { agent_id: AgentId, reason: String },

    #[error("Turn budget exceeded for agent {agent_id}: limit {limit}, actual {actual}")]
    TurnBudgetExceeded {
        agent_id: AgentId,
        limit: u32,
        actual: u32,
    },

    #[error("Tool call budget exceeded for agent {agent_id}: limit {limit}, actual {actual}")]
    ToolCallBudgetExceeded {
        agent_id: AgentId,
        limit: u32,
        actual: u32,
    },

    #[error("Child agent budget exceeded for agent {agent_id}: limit {limit}, actual {actual}")]
    ChildAgentBudgetExceeded {
        agent_id: AgentId,
        limit: u32,
        actual: u32,
    },

    #[error("Agent not found: {0}")]
    AgentNotFound(AgentId),

    #[error("Agent alias not found: {0}")]
    AgentAliasNotFound(String),

    #[error("Duplicate agent alias: {0}")]
    DuplicateAgentAlias(String),

    #[error("Invalid agent alias '{0}': must match ^[a-zA-Z0-9_-]{{1,64}}$")]
    InvalidAgentAlias(String),

    #[error("Invalid coordinator plan: {0}")]
    InvalidPlan(String),

    #[error("Cyclic task dependency detected in coordinator plan involving task '{task_key}'")]
    CyclicPlanDependency { task_key: String },

    #[error(
        "Missing task dependency in plan: task '{task_key}' depends on non-existent task '{depends_on}'"
    )]
    MissingTaskDependency {
        task_key: String,
        depends_on: String,
    },

    #[error("Execution failed for agent {agent_id}: {error}")]
    ExecutionFailed { agent_id: AgentId, error: String },

    #[error("Worker retirement timed out for agent {agent_id}")]
    WorkerRetirementTimeout { agent_id: AgentId },

    #[error("Thread shutdown failed for agent {agent_id}: {error}")]
    ThreadShutdownFailed { agent_id: AgentId, error: String },

    #[error("Thread shutdown timed out for agent {agent_id}")]
    ThreadShutdownTimeout { agent_id: AgentId },

    #[error(
        "Workspace affinity conflict for agent {agent_id}: worker is currently executing an active turn"
    )]
    WorkspaceAffinityConflict { agent_id: AgentId },

    #[error(
        "Worktree ownership conflict for agent {agent_id}: worktree {worktree_root:?} is already owned by thread {owner_thread_id}"
    )]
    WorktreeOwnershipConflict {
        agent_id: AgentId,
        worktree_root: std::path::PathBuf,
        owner_thread_id: String,
    },

    #[error("Workspace error: {0}")]
    Workspace(#[from] agent_studios_workspace::WorkspaceError),

    #[error("Workspace post-processing error: {0}")]
    WorkspacePostprocessError(String),

    #[error("Workflow execution aborted: {0}")]
    Aborted(String),

    #[error("Internal multi-agent error: {0}")]
    Other(String),
}
