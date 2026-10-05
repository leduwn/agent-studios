use agent_studios_internal_agent::{AgentBudgetTracker, AgentExecutionBudget, InternalAgentError};
use agent_studios_protocol::id::AgentId;

#[test]
fn test_budget_turn_limit_enforcement() {
    let id = AgentId::new();
    let budget = AgentExecutionBudget::default().with_turns(2);
    let tracker = AgentBudgetTracker::new(id, budget);

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
    let tracker = AgentBudgetTracker::new(id, budget);

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
    let tracker = AgentBudgetTracker::new(id, budget);

    for _ in 0..100 {
        assert!(tracker.record_turn().is_ok());
        assert!(tracker.record_tool_call().is_ok());
    }
    assert_eq!(tracker.turns_used(), 100);
    assert_eq!(tracker.tool_calls_used(), 100);
}

#[test]
fn test_pre_admission_turn_reservation_and_rollback() {
    let id = AgentId::new();
    let budget = AgentExecutionBudget::default().with_turns(1);
    let tracker = AgentBudgetTracker::new(id, budget);

    // Reserve first turn: success
    assert!(tracker.reserve_turn().is_ok());
    assert_eq!(tracker.turns_reserved(), 1);
    assert_eq!(tracker.turns_used(), 0);

    // N+1 turn reservation: rejected BEFORE turn start
    let err = tracker.reserve_turn();
    assert!(matches!(
        err,
        Err(InternalAgentError::TurnBudgetExceeded {
            limit: 1,
            actual: 2,
            ..
        })
    ));

    // Rollback pre-start failure: reservation freed
    tracker.rollback_turn();
    assert_eq!(tracker.turns_reserved(), 0);
    assert_eq!(tracker.turns_used(), 0);

    // Reserve again and commit on acceptance
    assert!(tracker.reserve_turn().is_ok());
    tracker.commit_turn();
    assert_eq!(tracker.turns_reserved(), 0);
    assert_eq!(tracker.turns_used(), 1);

    // Subsequent reservation rejected
    assert!(tracker.reserve_turn().is_err());
}

#[test]
fn test_child_agent_budget_reservation_commit_and_rollback() {
    let id = AgentId::new();
    let budget = AgentExecutionBudget::default().with_child_agents(1);
    let tracker = AgentBudgetTracker::new(id, budget);

    assert_eq!(tracker.child_agents_used(), 0);
    assert_eq!(tracker.child_agents_reserved(), 0);

    // Reserve slot for Child A
    assert!(tracker.reserve_child_agent().is_ok());
    assert_eq!(tracker.child_agents_reserved(), 1);
    assert_eq!(tracker.child_agents_used(), 0);

    // Reserving slot for Child B fails before spawn
    let err = tracker.reserve_child_agent();
    assert!(matches!(
        err,
        Err(InternalAgentError::ChildAgentBudgetExceeded {
            limit: 1,
            actual: 2,
            ..
        })
    ));

    // Child A fails to spawn -> rollback
    tracker.rollback_child_agent();
    assert_eq!(tracker.child_agents_reserved(), 0);
    assert_eq!(tracker.child_agents_used(), 0);

    // Reserve again and succeed -> commit
    assert!(tracker.reserve_child_agent().is_ok());
    tracker.commit_child_agent();
    assert_eq!(tracker.child_agents_reserved(), 0);
    assert_eq!(tracker.child_agents_used(), 1);

    // Next child reservation is blocked
    assert!(tracker.reserve_child_agent().is_err());
}

#[test]
fn test_tool_call_reservation_commit_and_rollback() {
    let id = AgentId::new();
    let budget = AgentExecutionBudget::default().with_tool_calls(1);
    let tracker = AgentBudgetTracker::new(id, budget);

    assert!(tracker.reserve_tool_call().is_ok());
    assert_eq!(tracker.tool_calls_reserved(), 1);
    assert_eq!(tracker.tool_calls_used(), 0);

    // Second tool reservation fails
    assert!(tracker.reserve_tool_call().is_err());

    // Rollback
    tracker.rollback_tool_call();
    assert_eq!(tracker.tool_calls_reserved(), 0);

    // Reserve and commit
    assert!(tracker.reserve_tool_call().is_ok());
    tracker.commit_tool_call();
    assert_eq!(tracker.tool_calls_used(), 1);
    assert_eq!(tracker.tool_calls_reserved(), 0);
}

#[test]
fn test_budget_scope_snapshot_and_scope_id() {
    use agent_studios_internal_agent::{BudgetScopeId, RunId};

    let agent_id = AgentId::new();
    let run_id = RunId::new();
    let budget = AgentExecutionBudget::default()
        .with_turns(5)
        .with_child_agents(2);

    let tracker = AgentBudgetTracker::new_with_scope(BudgetScopeId::Run(run_id), agent_id, budget);

    assert_eq!(tracker.scope_id(), &BudgetScopeId::Run(run_id));
    assert!(tracker.reserve_turn().is_ok());
    tracker.commit_turn();
    assert!(tracker.reserve_child_agent().is_ok());
    tracker.commit_child_agent();

    let snapshot = tracker.snapshot();
    assert_eq!(snapshot.scope_id, BudgetScopeId::Run(run_id));
    assert_eq!(snapshot.agent_id, agent_id);
    assert_eq!(snapshot.turns_used, 1);
    assert_eq!(snapshot.child_agents_used, 1);
    assert_eq!(snapshot.tool_calls_used, 0);
}
