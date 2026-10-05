use std::collections::HashMap;

use agent_studios_protocol::agent::{AgentExecutionBudget, AgentState};
use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId};
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::TaskRecord;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub runtime_bound: bool,
    pub runtime_kind: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub wire_protocol: Option<String>,
    pub codex_thread_id: Option<String>,
    pub parent_agent_id: Option<AgentId>,
    pub spawn_depth: u32,
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
}

/// Observable summary of an execution run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSummary {
    pub run_id: RunId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub state: RunState,
    pub retry_count: u32,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
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
