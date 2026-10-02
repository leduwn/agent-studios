use serde::{Deserialize, Serialize};

use crate::id::{AgentId, ApprovalId, RunId, StudioId, TaskId};

/// Target boundary scope for a cancellation command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CancellationScope {
    Studio(StudioId),
    Task {
        task_id: TaskId,
        include_descendants: bool,
    },
    Agent(AgentId),
    Run(RunId),
}

/// Summary result reporting which active entities were cancelled.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancellationSummary {
    pub cancelled_tasks: Vec<TaskId>,
    pub cancelled_runs: Vec<RunId>,
    pub cancelled_approvals: Vec<ApprovalId>,
}

impl CancellationSummary {
    pub fn is_empty(&self) -> bool {
        self.cancelled_tasks.is_empty()
            && self.cancelled_runs.is_empty()
            && self.cancelled_approvals.is_empty()
    }
}
