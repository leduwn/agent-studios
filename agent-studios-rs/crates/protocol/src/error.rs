use thiserror::Error;

use crate::agent::AgentState;
use crate::approval::ApprovalState;
use crate::id::TaskId;
use crate::reconciliation::ReconciliationState;
use crate::run::RunState;
use crate::task::TaskState;
use crate::worktree::WorktreeState;

/// Errors raised when parsing strong identifiers.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("Failed to parse {type_name} from string: {source}")]
pub struct IdParseError {
    pub type_name: &'static str,
    #[source]
    pub source: uuid::Error,
}

/// Errors raised when performing invalid state transitions.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TransitionError {
    #[error("Invalid agent transition from {from:?} to {to:?}")]
    InvalidAgentTransition { from: AgentState, to: AgentState },

    #[error("Agent is already in terminal state {state:?} and cannot transition")]
    TerminalAgentTransition { state: AgentState },

    #[error("Invalid task transition from {from:?} to {to:?}")]
    InvalidTaskTransition { from: TaskState, to: TaskState },

    #[error("Task is already in terminal state {state:?} and cannot transition")]
    TerminalTaskTransition { state: TaskState },

    #[error(
        "Task {task_id} in state {state:?} cannot mutate dependencies (must be Pending, Blocked, or Ready)"
    )]
    TaskDependencyMutationForbidden { task_id: TaskId, state: TaskState },

    #[error("Invalid run transition from {from:?} to {to:?}")]
    InvalidRunTransition { from: RunState, to: RunState },

    #[error("Run is already in terminal state {state:?} and cannot transition")]
    TerminalRunTransition { state: RunState },

    #[error("Invalid approval transition from {from:?} to {to:?}")]
    InvalidApprovalTransition {
        from: ApprovalState,
        to: ApprovalState,
    },

    #[error("Approval request is already resolved as {state:?}")]
    ApprovalAlreadyResolved { state: ApprovalState },

    #[error("Invalid worktree transition from {from:?} to {to:?}")]
    InvalidWorktreeTransition {
        from: WorktreeState,
        to: WorktreeState,
    },

    #[error("Worktree is already in terminal state {state:?} and cannot transition")]
    TerminalWorktreeTransition { state: WorktreeState },

    #[error("Invalid reconciliation transition from {from:?} to {to:?}")]
    InvalidReconciliationTransition {
        from: ReconciliationState,
        to: ReconciliationState,
    },

    #[error("Reconciliation is already in terminal state {state:?} and cannot transition")]
    TerminalReconciliationTransition { state: ReconciliationState },
}
