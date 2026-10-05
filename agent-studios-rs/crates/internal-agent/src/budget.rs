use std::sync::Mutex;
use std::time::Instant;

use agent_studios_protocol::id::{AgentId, RunId};

use crate::error::InternalAgentError;
use crate::profile::AgentExecutionBudget;

/// Identity for a scoped budget context.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BudgetScopeId {
    Coordinator(AgentId),
    Run(RunId),
}

/// Point-in-time view of an execution scope's budget usage.
#[derive(Debug, Clone)]
pub struct BudgetScope {
    pub scope_id: BudgetScopeId,
    pub agent_id: AgentId,
    pub turns_used: u32,
    pub tool_calls_used: u32,
    pub child_agents_used: u32,
    pub wall_clock_start: Instant,
}

#[derive(Debug)]
struct BudgetTrackerState {
    start_time: Instant,
    turns_used: u32,
    turns_reserved: u32,
    tool_calls_used: u32,
    tool_calls_reserved: u32,
    child_agents_used: u32,
    child_agents_reserved: u32,
}

#[derive(Debug)]
pub struct AgentBudgetTracker {
    scope_id: BudgetScopeId,
    agent_id: AgentId,
    budget: AgentExecutionBudget,
    state: Mutex<BudgetTrackerState>,
}

impl AgentBudgetTracker {
    pub fn new(agent_id: AgentId, budget: AgentExecutionBudget) -> Self {
        Self::new_with_scope(BudgetScopeId::Coordinator(agent_id), agent_id, budget)
    }

    pub fn new_with_scope(
        scope_id: BudgetScopeId,
        agent_id: AgentId,
        budget: AgentExecutionBudget,
    ) -> Self {
        Self {
            scope_id,
            agent_id,
            budget,
            state: Mutex::new(BudgetTrackerState {
                start_time: Instant::now(),
                turns_used: 0,
                turns_reserved: 0,
                tool_calls_used: 0,
                tool_calls_reserved: 0,
                child_agents_used: 0,
                child_agents_reserved: 0,
            }),
        }
    }

    pub fn scope_id(&self) -> &BudgetScopeId {
        &self.scope_id
    }

    pub fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    pub fn budget(&self) -> &AgentExecutionBudget {
        &self.budget
    }

    pub fn turns_used(&self) -> u32 {
        self.state.lock().unwrap().turns_used
    }

    pub fn turns_reserved(&self) -> u32 {
        self.state.lock().unwrap().turns_reserved
    }

    pub fn tool_calls_used(&self) -> u32 {
        self.state.lock().unwrap().tool_calls_used
    }

    pub fn tool_calls_reserved(&self) -> u32 {
        self.state.lock().unwrap().tool_calls_reserved
    }

    pub fn child_agents_used(&self) -> u32 {
        self.state.lock().unwrap().child_agents_used
    }

    pub fn child_agents_reserved(&self) -> u32 {
        self.state.lock().unwrap().child_agents_reserved
    }

    pub fn elapsed_secs(&self) -> u64 {
        self.state.lock().unwrap().start_time.elapsed().as_secs()
    }

    pub fn wall_clock_secs_used(&self) -> u64 {
        self.elapsed_secs()
    }

    pub fn snapshot(&self) -> BudgetScope {
        let state = self.state.lock().unwrap();
        BudgetScope {
            scope_id: self.scope_id.clone(),
            agent_id: self.agent_id,
            turns_used: state.turns_used,
            tool_calls_used: state.tool_calls_used,
            child_agents_used: state.child_agents_used,
            wall_clock_start: state.start_time,
        }
    }

    pub fn reserve_turn(&self) -> Result<(), InternalAgentError> {
        self.check_wall_clock()?;
        let mut state = self.state.lock().unwrap();
        if let Some(limit) = self.budget.max_turns {
            let total = state.turns_used.saturating_add(state.turns_reserved);
            if total >= limit {
                return Err(InternalAgentError::TurnBudgetExceeded {
                    agent_id: self.agent_id,
                    limit,
                    actual: total.saturating_add(1),
                });
            }
        }
        state.turns_reserved = state.turns_reserved.saturating_add(1);
        Ok(())
    }

    pub fn commit_turn(&self) {
        let mut state = self.state.lock().unwrap();
        state.turns_reserved = state.turns_reserved.saturating_sub(1);
        state.turns_used = state.turns_used.saturating_add(1);
    }

    pub fn rollback_turn(&self) {
        let mut state = self.state.lock().unwrap();
        state.turns_reserved = state.turns_reserved.saturating_sub(1);
    }

    pub fn reserve_child_agent(&self) -> Result<(), InternalAgentError> {
        self.check_wall_clock()?;
        let mut state = self.state.lock().unwrap();
        if let Some(limit) = self.budget.max_child_agents {
            let total = state
                .child_agents_used
                .saturating_add(state.child_agents_reserved);
            if total >= limit {
                return Err(InternalAgentError::ChildAgentBudgetExceeded {
                    agent_id: self.agent_id,
                    limit,
                    actual: total.saturating_add(1),
                });
            }
        }
        state.child_agents_reserved = state.child_agents_reserved.saturating_add(1);
        Ok(())
    }

    pub fn commit_child_agent(&self) {
        let mut state = self.state.lock().unwrap();
        state.child_agents_reserved = state.child_agents_reserved.saturating_sub(1);
        state.child_agents_used = state.child_agents_used.saturating_add(1);
    }

    pub fn rollback_child_agent(&self) {
        let mut state = self.state.lock().unwrap();
        state.child_agents_reserved = state.child_agents_reserved.saturating_sub(1);
    }

    pub fn reserve_tool_call(&self) -> Result<(), InternalAgentError> {
        self.check_wall_clock()?;
        let mut state = self.state.lock().unwrap();
        if let Some(limit) = self.budget.max_tool_calls {
            let total = state
                .tool_calls_used
                .saturating_add(state.tool_calls_reserved);
            if total >= limit {
                return Err(InternalAgentError::ToolCallBudgetExceeded {
                    agent_id: self.agent_id,
                    limit,
                    actual: total.saturating_add(1),
                });
            }
        }
        state.tool_calls_reserved = state.tool_calls_reserved.saturating_add(1);
        Ok(())
    }

    pub fn commit_tool_call(&self) {
        let mut state = self.state.lock().unwrap();
        state.tool_calls_reserved = state.tool_calls_reserved.saturating_sub(1);
        state.tool_calls_used = state.tool_calls_used.saturating_add(1);
    }

    pub fn rollback_tool_call(&self) {
        let mut state = self.state.lock().unwrap();
        state.tool_calls_reserved = state.tool_calls_reserved.saturating_sub(1);
    }

    pub fn record_turn(&self) -> Result<(), InternalAgentError> {
        self.reserve_turn()?;
        self.commit_turn();
        Ok(())
    }

    pub fn record_tool_call(&self) -> Result<(), InternalAgentError> {
        self.reserve_tool_call()?;
        self.commit_tool_call();
        Ok(())
    }

    pub fn check_wall_clock(&self) -> Result<(), InternalAgentError> {
        if let Some(limit) = self.budget.max_wall_clock_secs {
            let elapsed = self.elapsed_secs();
            if elapsed > limit {
                return Err(InternalAgentError::BudgetExceeded {
                    agent_id: self.agent_id,
                    reason: format!(
                        "Wall-clock duration exceeded: limit {limit}s, actual {elapsed}s"
                    ),
                });
            }
        }
        Ok(())
    }

    pub fn check_all(&self) -> Result<(), InternalAgentError> {
        self.check_wall_clock()?;
        let state = self.state.lock().unwrap();
        if let Some(limit) = self.budget.max_turns {
            let total = state.turns_used.saturating_add(state.turns_reserved);
            if total >= limit {
                return Err(InternalAgentError::TurnBudgetExceeded {
                    agent_id: self.agent_id,
                    limit,
                    actual: total,
                });
            }
        }
        if let Some(limit) = self.budget.max_tool_calls {
            let total = state
                .tool_calls_used
                .saturating_add(state.tool_calls_reserved);
            if total >= limit {
                return Err(InternalAgentError::ToolCallBudgetExceeded {
                    agent_id: self.agent_id,
                    limit,
                    actual: total,
                });
            }
        }
        if let Some(limit) = self.budget.max_child_agents {
            let total = state
                .child_agents_used
                .saturating_add(state.child_agents_reserved);
            if total >= limit {
                return Err(InternalAgentError::ChildAgentBudgetExceeded {
                    agent_id: self.agent_id,
                    limit,
                    actual: total,
                });
            }
        }
        Ok(())
    }
}
