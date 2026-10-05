use agent_studios_internal_agent::{
    AgentExecutionBudget, AgentReasoningEffort, AgentReasoningSelection, COORDINATOR_ALIAS,
    InternalAgentError, InternalAgentSpec, InternalTeamSpec, WorkspaceAccessMode, validate_alias,
};
use agent_studios_protocol::id::{AgentId, StudioId};
use agent_studios_provider::id::{ModelId, ProviderInstanceId};
use agent_studios_provider::model::ModelRef;
use codex_protocol::openai_models::ReasoningEffort;

#[test]
fn test_alias_validation() {
    assert!(validate_alias("coordinator").is_ok());
    assert!(validate_alias("coder_1").is_ok());
    assert!(validate_alias("reviewer-agent-2").is_ok());

    assert!(validate_alias("").is_err());
    assert!(validate_alias("invalid alias with spaces").is_err());
    assert!(validate_alias("special@chars!").is_err());
    assert!(validate_alias(&"a".repeat(65)).is_err());
    assert!(validate_alias(&"a".repeat(64)).is_ok());
}

#[test]
fn test_team_spec_construction_and_validation() {
    let coord_model = ModelRef::new(
        ProviderInstanceId::new(),
        ModelId::new("gemini-2.5-pro").unwrap(),
    );
    let coord_spec = InternalAgentSpec::new(
        AgentId::new(),
        "Team Coordinator",
        "Lead coordinator",
        coord_model,
    );

    let studio_id = StudioId::new();
    let team = InternalTeamSpec::new(studio_id, "test-team", coord_spec.clone());
    assert_eq!(team.len(), 1);
    assert!(!team.is_empty());
    assert_eq!(
        team.get_agent(COORDINATOR_ALIAS).unwrap().display_name,
        "Team Coordinator"
    );

    let coder_model = ModelRef::new(
        ProviderInstanceId::new(),
        ModelId::new("claude-3-7-sonnet").unwrap(),
    );
    let coder_spec = InternalAgentSpec::new(
        AgentId::new(),
        "Backend Coder",
        "Rust implementer",
        coder_model,
    )
    .with_workspace_access(WorkspaceAccessMode::Mutating);

    let team = team.add_agent("coder", coder_spec).unwrap();
    assert_eq!(team.len(), 2);
    assert_eq!(
        team.get_agent("coder").unwrap().workspace_access,
        WorkspaceAccessMode::Mutating
    );

    // Reject reserved coordinator alias
    let dupe_coord = team.clone().add_agent(
        "coordinator",
        InternalAgentSpec::new(
            AgentId::new(),
            "Another",
            "Role",
            ModelRef::new(ProviderInstanceId::new(), ModelId::new("model").unwrap()),
        ),
    );
    assert!(matches!(
        dupe_coord,
        Err(InternalAgentError::DuplicateAgentAlias(_))
    ));

    // Reject duplicate worker alias
    let dupe_worker = team.clone().add_agent(
        "coder",
        InternalAgentSpec::new(
            AgentId::new(),
            "Another Coder",
            "Role",
            ModelRef::new(ProviderInstanceId::new(), ModelId::new("model").unwrap()),
        ),
    );
    assert!(matches!(
        dupe_worker,
        Err(InternalAgentError::DuplicateAgentAlias(_))
    ));
}

#[test]
fn test_duplicate_agent_id_rejected() {
    let shared_id = AgentId::new();
    let coord_spec = InternalAgentSpec::new(
        shared_id,
        "Coordinator",
        "Lead",
        ModelRef::new(ProviderInstanceId::new(), ModelId::new("m1").unwrap()),
    );
    let team = InternalTeamSpec::new(StudioId::new(), "team", coord_spec);

    let worker_spec = InternalAgentSpec::new(
        shared_id,
        "Worker",
        "Worker",
        ModelRef::new(ProviderInstanceId::new(), ModelId::new("m2").unwrap()),
    );
    let err = team.add_agent("worker", worker_spec);
    assert!(matches!(
        err,
        Err(InternalAgentError::DuplicateAgentAlias(_))
    ));
}

#[test]
fn test_reasoning_effort_mapping() {
    let effort_none: Option<ReasoningEffort> = AgentReasoningEffort::None.into();
    assert_eq!(effort_none, None);

    let effort_low: Option<ReasoningEffort> = AgentReasoningEffort::Low.into();
    assert_eq!(effort_low, Some(ReasoningEffort::Low));

    let effort_high: Option<ReasoningEffort> = AgentReasoningEffort::High.into();
    assert_eq!(effort_high, Some(ReasoningEffort::High));

    let sel = AgentReasoningSelection {
        effort: Some(AgentReasoningEffort::Medium),
        summary: Some("Chain of thought enabled".to_string()),
    };
    assert_eq!(sel.effort, Some(AgentReasoningEffort::Medium));
}

#[test]
fn test_budget_builder() {
    let b = AgentExecutionBudget::default()
        .with_turns(10)
        .with_tool_calls(50)
        .with_wall_clock_secs(120);

    assert_eq!(b.max_turns, Some(10));
    assert_eq!(b.max_tool_calls, Some(50));
    assert_eq!(b.max_wall_clock_secs, Some(120));

    let unlimited = AgentExecutionBudget::unlimited();
    assert_eq!(unlimited.max_turns, None);
    assert_eq!(unlimited.max_tool_calls, None);
    assert_eq!(unlimited.max_wall_clock_secs, None);
}

#[test]
fn test_team_max_parallel_agents_validation() {
    let coord = InternalAgentSpec::new(
        AgentId::new(),
        "Coord",
        "Lead",
        ModelRef::new(ProviderInstanceId::new(), ModelId::new("m1").unwrap()),
    );

    // None accepted
    let team_none = InternalTeamSpec::new(StudioId::new(), "team", coord.clone());
    assert!(team_none.validate().is_ok());

    // Some(N > 0) accepted
    let team_two =
        InternalTeamSpec::new(StudioId::new(), "team", coord.clone()).with_max_parallel_agents(2);
    assert!(team_two.validate().is_ok());

    // Some(0) rejected
    let team_zero =
        InternalTeamSpec::new(StudioId::new(), "team", coord).with_max_parallel_agents(0);
    assert!(matches!(
        team_zero.validate(),
        Err(InternalAgentError::InvalidPlan(_))
    ));
}
