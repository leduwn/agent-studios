use std::time::Duration;

use agent_studios_internal_agent::{
    InternalAgentError, WorkspaceAccessMode, WorkspacePolicyArbitrator,
};
use agent_studios_protocol::id::AgentId;

#[test]
fn test_single_mutator_exclusivity() {
    let arb = WorkspacePolicyArbitrator::new();
    let agent_a = AgentId::new();
    let agent_b = AgentId::new();

    let lease_a = arb
        .try_acquire("workspace_1", agent_a, WorkspaceAccessMode::Mutating)
        .unwrap();
    assert_eq!(lease_a.workspace_id(), "workspace_1");
    assert_eq!(lease_a.agent_id(), agent_a);

    // Another mutating request fails
    let err_b_mut = arb.try_acquire("workspace_1", agent_b, WorkspaceAccessMode::Mutating);
    assert!(matches!(
        err_b_mut,
        Err(InternalAgentError::WorkspaceConflict { .. })
    ));

    // A read-only request also fails
    let err_b_ro = arb.try_acquire("workspace_1", agent_b, WorkspaceAccessMode::ReadOnly);
    assert!(matches!(
        err_b_ro,
        Err(InternalAgentError::WorkspaceConflict { .. })
    ));

    // A different workspace succeeds
    let lease_b_other = arb
        .try_acquire("workspace_2", agent_b, WorkspaceAccessMode::Mutating)
        .unwrap();
    assert_eq!(lease_b_other.workspace_id(), "workspace_2");

    drop(lease_a);

    // After dropping lease_a, agent_b can now acquire mutating lease on workspace_1
    let lease_b = arb
        .try_acquire("workspace_1", agent_b, WorkspaceAccessMode::Mutating)
        .unwrap();
    assert_eq!(lease_b.agent_id(), agent_b);
}

#[test]
fn test_multiple_concurrent_readers() {
    let arb = WorkspacePolicyArbitrator::new();
    let r1 = AgentId::new();
    let r2 = AgentId::new();
    let r3 = AgentId::new();
    let mutator = AgentId::new();

    let lease1 = arb
        .try_acquire("ws", r1, WorkspaceAccessMode::ReadOnly)
        .unwrap();
    let lease2 = arb
        .try_acquire("ws", r2, WorkspaceAccessMode::ReadOnly)
        .unwrap();
    let lease3 = arb
        .try_acquire("ws", r3, WorkspaceAccessMode::ReadOnly)
        .unwrap();

    assert_eq!(arb.active_agents("ws").len(), 3);

    // Mutator is blocked by active readers
    let err_mut = arb.try_acquire("ws", mutator, WorkspaceAccessMode::Mutating);
    assert!(matches!(
        err_mut,
        Err(InternalAgentError::WorkspaceConflict { .. })
    ));

    drop(lease1);
    drop(lease2);
    // Still 1 reader remaining
    assert!(matches!(
        arb.try_acquire("ws", mutator, WorkspaceAccessMode::Mutating),
        Err(InternalAgentError::WorkspaceConflict { .. })
    ));

    // Drop last reader
    drop(lease3);
    assert_eq!(arb.active_agents("ws").len(), 0);

    // Now mutator succeeds
    let lease_mut = arb
        .try_acquire("ws", mutator, WorkspaceAccessMode::Mutating)
        .unwrap();
    assert_eq!(lease_mut.agent_id(), mutator);
}

#[tokio::test]
async fn test_async_acquire_with_timeout() {
    let arb = WorkspacePolicyArbitrator::new();
    let agent_a = AgentId::new();
    let agent_b = AgentId::new();

    let mut lease_a = arb
        .try_acquire("ws", agent_a, WorkspaceAccessMode::Mutating)
        .unwrap();

    // Spawn task to release lease after 50ms
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        lease_a.release();
    });

    // Agent B waits with 500ms timeout
    let lease_b = arb
        .acquire(
            "ws",
            agent_b,
            WorkspaceAccessMode::Mutating,
            Duration::from_millis(500),
        )
        .await
        .unwrap();

    assert_eq!(lease_b.agent_id(), agent_b);
}
