use agent_studios_internal_agent::{AgentBudgetTracker, AgentExecutionBudget, InternalAgentError};
use agent_studios_protocol::id::AgentId;

#[test]
fn test_budget_turn_limit_enforcement() {
    let id = AgentId::new();
    let budget = AgentExecutionBudget::default().with_turns(2);
    let mut tracker = AgentBudgetTracker::new(id, budget);

    assert_eq!(tracker.turns_used(), 0);
    assert!(tracker.record_turn().is_ok());
    assert_eq!(tracker.turns_used(), 1);
    assert!(tracker.record_turn().is_ok());
    assert_eq!(tracker.turns_used(), 2);

    // Third turn exceeds limit
    let err = tracker.record_turn();
    assert!(matches!(
        err,
        Err(InternalAgentError::TurnBudgetExceeded {
            limit: 2,
            actual: 3,
            ..
        })
    ));
}

#[test]
fn test_budget_tool_call_limit_enforcement() {
    let id = AgentId::new();
    let budget = AgentExecutionBudget::default().with_tool_calls(3);
    let mut tracker = AgentBudgetTracker::new(id, budget);

    assert_eq!(tracker.tool_calls_used(), 0);
    assert!(tracker.record_tool_call().is_ok());
    assert!(tracker.record_tool_call().is_ok());
    assert!(tracker.record_tool_call().is_ok());

    let err = tracker.record_tool_call();
    assert!(matches!(
        err,
        Err(InternalAgentError::ToolCallBudgetExceeded {
            limit: 3,
            actual: 4,
            ..
        })
    ));
}

#[test]
fn test_unlimited_budget() {
    let id = AgentId::new();
    let budget = AgentExecutionBudget::unlimited();
    let mut tracker = AgentBudgetTracker::new(id, budget);

    for _ in 0..100 {
        assert!(tracker.record_turn().is_ok());
        assert!(tracker.record_tool_call().is_ok());
    }
    assert_eq!(tracker.turns_used(), 100);
    assert_eq!(tracker.tool_calls_used(), 100);
}
