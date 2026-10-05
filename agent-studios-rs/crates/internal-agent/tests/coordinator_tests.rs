use agent_studios_control_plane::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::store::InMemoryStore;
use agent_studios_internal_agent::{
    ControlPlaneActor, CoordinatorPlanValidator, InternalAgentError, InternalAgentSpec,
    InternalTeamSpec, PlannedTask,
};
use agent_studios_protocol::id::AgentId;
use agent_studios_provider::id::{ModelId, ProviderInstanceId};
use agent_studios_provider::model::ModelRef;

fn create_test_team() -> InternalTeamSpec {
    let dummy_model = ModelRef::new(
        ProviderInstanceId::new(),
        ModelId::new("test-model").unwrap(),
    );
    let coord = InternalAgentSpec::new(AgentId::new(), "Coord", "Lead", dummy_model.clone());
    let coder = InternalAgentSpec::new(AgentId::new(), "Coder", "Code", dummy_model.clone());
    let reviewer = InternalAgentSpec::new(AgentId::new(), "Reviewer", "Review", dummy_model);

    InternalTeamSpec::new("team", coord)
        .add_agent("coder", coder)
        .unwrap()
        .add_agent("reviewer", reviewer)
        .unwrap()
}

#[test]
fn test_valid_linear_plan_topological_sort() {
    let team = create_test_team();
    let tasks = vec![
        PlannedTask {
            task_key: "t1".to_string(),
            title: "Task 1".to_string(),
            description: None,
            assigned_alias: "coder".to_string(),
            depends_on: vec![],
            workspace_access: None,
            priority: None,
        },
        PlannedTask {
            task_key: "t2".to_string(),
            title: "Task 2".to_string(),
            description: None,
            assigned_alias: "reviewer".to_string(),
            depends_on: vec!["t1".to_string()],
            workspace_access: None,
            priority: None,
        },
        PlannedTask {
            task_key: "t3".to_string(),
            title: "Task 3".to_string(),
            description: None,
            assigned_alias: "coordinator".to_string(),
            depends_on: vec!["t2".to_string()],
            workspace_access: None,
            priority: None,
        },
    ];

    let sorted = CoordinatorPlanValidator::validate(&tasks, &team).unwrap();
    assert_eq!(sorted, vec!["t1", "t2", "t3"]);
}

#[test]
fn test_valid_diamond_plan_topological_sort() {
    let team = create_test_team();
    let tasks = vec![
        PlannedTask {
            task_key: "start".to_string(),
            title: "Start".to_string(),
            description: None,
            assigned_alias: "coordinator".to_string(),
            depends_on: vec![],
            workspace_access: None,
            priority: None,
        },
        PlannedTask {
            task_key: "branch_a".to_string(),
            title: "Branch A".to_string(),
            description: None,
            assigned_alias: "coder".to_string(),
            depends_on: vec!["start".to_string()],
            workspace_access: None,
            priority: None,
        },
        PlannedTask {
            task_key: "branch_b".to_string(),
            title: "Branch B".to_string(),
            description: None,
            assigned_alias: "reviewer".to_string(),
            depends_on: vec!["start".to_string()],
            workspace_access: None,
            priority: None,
        },
        PlannedTask {
            task_key: "join".to_string(),
            title: "Join".to_string(),
            description: None,
            assigned_alias: "coordinator".to_string(),
            depends_on: vec!["branch_a".to_string(), "branch_b".to_string()],
            workspace_access: None,
            priority: None,
        },
    ];

    let sorted = CoordinatorPlanValidator::validate(&tasks, &team).unwrap();
    assert_eq!(sorted[0], "start");
    assert_eq!(sorted[3], "join");
    assert!(sorted.contains(&"branch_a".to_string()));
    assert!(sorted.contains(&"branch_b".to_string()));
}

#[test]
fn test_cyclic_plan_rejected() {
    let team = create_test_team();
    let tasks = vec![
        PlannedTask {
            task_key: "t1".to_string(),
            title: "T1".to_string(),
            description: None,
            assigned_alias: "coder".to_string(),
            depends_on: vec!["t2".to_string()],
            workspace_access: None,
            priority: None,
        },
        PlannedTask {
            task_key: "t2".to_string(),
            title: "T2".to_string(),
            description: None,
            assigned_alias: "reviewer".to_string(),
            depends_on: vec!["t1".to_string()],
            workspace_access: None,
            priority: None,
        },
    ];

    let err = CoordinatorPlanValidator::validate(&tasks, &team);
    assert!(matches!(
        err,
        Err(InternalAgentError::CyclicPlanDependency { .. })
    ));
}

#[test]
fn test_self_cycle_rejected() {
    let team = create_test_team();
    let tasks = vec![PlannedTask {
        task_key: "t1".to_string(),
        title: "T1".to_string(),
        description: None,
        assigned_alias: "coder".to_string(),
        depends_on: vec!["t1".to_string()],
        workspace_access: None,
        priority: None,
    }];

    let err = CoordinatorPlanValidator::validate(&tasks, &team);
    assert!(matches!(
        err,
        Err(InternalAgentError::CyclicPlanDependency { task_key }) if task_key == "t1"
    ));
}

#[test]
fn test_missing_dependency_rejected() {
    let team = create_test_team();
    let tasks = vec![PlannedTask {
        task_key: "t1".to_string(),
        title: "T1".to_string(),
        description: None,
        assigned_alias: "coder".to_string(),
        depends_on: vec!["non_existent_key".to_string()],
        workspace_access: None,
        priority: None,
    }];

    let err = CoordinatorPlanValidator::validate(&tasks, &team);
    assert!(matches!(
        err,
        Err(InternalAgentError::MissingTaskDependency { .. })
    ));
}

#[test]
fn test_missing_agent_alias_rejected() {
    let team = create_test_team();
    let tasks = vec![PlannedTask {
        task_key: "t1".to_string(),
        title: "T1".to_string(),
        description: None,
        assigned_alias: "unknown_alias".to_string(),
        depends_on: vec![],
        workspace_access: None,
        priority: None,
    }];

    let err = CoordinatorPlanValidator::validate(&tasks, &team);
    assert!(matches!(
        err,
        Err(InternalAgentError::AgentAliasNotFound(alias)) if alias == "unknown_alias"
    ));
}

#[tokio::test]
async fn test_materialize_plan_in_control_plane() {
    let clock = SystemClock;
    let store = InMemoryStore::new();
    let cp = ControlPlane::new(clock, store);
    let (handle, _task) = ControlPlaneActor::spawn(cp);

    let studio = handle.create_studio("Test Studio").await.unwrap();
    let team = create_test_team();

    // Register team agents into control plane
    handle
        .register_agent_with_id(
            studio.id,
            team.coordinator.agent_id,
            "Coord",
            agent_studios_protocol::agent::AgentKind::Internal,
            None,
        )
        .await
        .unwrap();
    handle
        .register_agent_with_id(
            studio.id,
            team.agents["coder"].agent_id,
            "Coder",
            agent_studios_protocol::agent::AgentKind::Internal,
            None,
        )
        .await
        .unwrap();
    handle
        .register_agent_with_id(
            studio.id,
            team.agents["reviewer"].agent_id,
            "Reviewer",
            agent_studios_protocol::agent::AgentKind::Internal,
            None,
        )
        .await
        .unwrap();

    let tasks = vec![
        PlannedTask {
            task_key: "k1".to_string(),
            title: "First Task".to_string(),
            description: Some("Desc 1".to_string()),
            assigned_alias: "coder".to_string(),
            depends_on: vec![],
            workspace_access: None,
            priority: None,
        },
        PlannedTask {
            task_key: "k2".to_string(),
            title: "Second Task".to_string(),
            description: Some("Desc 2".to_string()),
            assigned_alias: "reviewer".to_string(),
            depends_on: vec!["k1".to_string()],
            workspace_access: None,
            priority: None,
        },
    ];

    let mapping = CoordinatorPlanValidator::materialize(&handle, studio.id, &tasks, &team)
        .await
        .unwrap();

    assert_eq!(mapping.len(), 2);
    let task1 = handle.get_task(mapping["k1"]).await.unwrap().unwrap();
    let task2 = handle.get_task(mapping["k2"]).await.unwrap().unwrap();

    assert_eq!(task1.title, "First Task");
    assert_eq!(task2.title, "Second Task");
    assert_eq!(task2.dependencies, vec![task1.id]);
}
