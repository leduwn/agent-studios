use std::time::Instant;

use agent_studios_protocol::id::AgentId;

use crate::error::InternalAgentError;
use crate::profile::AgentExecutionBudget;

#[derive(Debug)]
pub struct AgentBudgetTracker {
    agent_id: AgentId,
    budget: AgentExecutionBudget,
    start_time: Instant,
    turns_used: u32,
    tool_calls_used: u32,
}

impl AgentBudgetTracker {
    pub fn new(agent_id: AgentId, budget: AgentExecutionBudget) -> Self {
        Self {
            agent_id,
            budget,
            start_time: Instant::now(),
            turns_used: 0,
            tool_calls_used: 0,
        }
    }

    pub fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    pub fn turns_used(&self) -> u32 {
        self.turns_used
    }

    pub fn tool_calls_used(&self) -> u32 {
        self.tool_calls_used
    }

    pub fn elapsed_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    pub fn wall_clock_secs_used(&self) -> u64 {
        self.elapsed_secs()
    }

    pub fn child_agents_used(&self) -> u32 {
        0
    }

    pub fn record_turn(&mut self) -> Result<(), InternalAgentError> {
        self.check_wall_clock()?;
        self.turns_used = self.turns_used.saturating_add(1);
        if let Some(limit) = self.budget.max_turns
            && self.turns_used > limit
        {
            return Err(InternalAgentError::TurnBudgetExceeded {
                agent_id: self.agent_id,
                limit,
                actual: self.turns_used,
            });
        }
        Ok(())
    }

    pub fn record_tool_call(&mut self) -> Result<(), InternalAgentError> {
        self.check_wall_clock()?;
        self.tool_calls_used = self.tool_calls_used.saturating_add(1);
        if let Some(limit) = self.budget.max_tool_calls
            && self.tool_calls_used > limit
        {
            return Err(InternalAgentError::ToolCallBudgetExceeded {
                agent_id: self.agent_id,
                limit,
                actual: self.tool_calls_used,
            });
        }
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
        if let Some(limit) = self.budget.max_turns
            && self.turns_used >= limit
        {
            return Err(InternalAgentError::TurnBudgetExceeded {
                agent_id: self.agent_id,
                limit,
                actual: self.turns_used,
            });
        }
        if let Some(limit) = self.budget.max_tool_calls
            && self.tool_calls_used >= limit
        {
            return Err(InternalAgentError::ToolCallBudgetExceeded {
                agent_id: self.agent_id,
                limit,
                actual: self.tool_calls_used,
            });
        }
        Ok(())
    }
}
