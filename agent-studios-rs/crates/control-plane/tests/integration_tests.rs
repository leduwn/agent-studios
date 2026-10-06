use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};

use agent_studios_control_plane::clock::FixedClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::error::{ControlPlaneError, ReplayError, StoreError};
use agent_studios_control_plane::store::{EventStore, InMemoryStore};
use agent_studios_protocol::agent::{AgentDescriptor, AgentKind, BatchAgentSpec};
use agent_studios_protocol::approval::{ApprovalKind, ApprovalRequest, ApprovalState};
use agent_studios_protocol::artifact::{ArtifactKind, ArtifactRecord};
use agent_studios_protocol::cancellation::CancellationScope;
use agent_studios_protocol::error::TransitionError;
use agent_studios_protocol::event::{
    CONTROL_PLANE_EVENT_SCHEMA_VERSION, ControlPlaneEvent, EventEnvelope,
};
use agent_studios_protocol::id::{
    AgentId, ApprovalId, ArtifactId, ReconciliationId, RunId, StudioId, TaskId, WorktreeId,
};
use agent_studios_protocol::reconciliation::{ReconciliationRecord, ReconciliationState};
use agent_studios_protocol::run::{RunRecord, RunState};
use agent_studios_protocol::studio::Studio;
use agent_studios_protocol::task::{BatchTaskSpec, TaskRecord, TaskState};
use agent_studios_protocol::worktree::{WorktreeRecord, WorktreeState};

// ============================================================================
// FailingEventStore Mock for Failure Injection
// ============================================================================

#[derive(Clone, Debug, Default)]
struct FailingEventStore {
    inner: InMemoryStore,
    fail_next: Arc<AtomicBool>,
}

impl FailingEventStore {
    fn new() -> (Self, Arc<AtomicBool>) {
        let flag = Arc::new(AtomicBool::new(false));
        (
            Self {
                inner: InMemoryStore::new(),
                fail_next: Arc::clone(&flag),
            },
            flag,
        )
    }
}

impl EventStore for FailingEventStore {
    fn append_batch(&mut self, events: &[EventEnvelope]) -> Result<(), StoreError> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(StoreError::StorageFailure("Injected store failure".into()));
        }
        self.inner.append_batch(events)
    }

    fn events_for_studio(
        &self,
        studio_id: StudioId,
        from_sequence: u64,
    ) -> Result<Vec<EventEnvelope>, StoreError> {
        self.inner.events_for_studio(studio_id, from_sequence)
    }

    fn latest_sequence(&self, studio_id: StudioId) -> Result<u64, StoreError> {
        self.inner.latest_sequence(studio_id)
    }

    fn all_events(&self) -> Result<Vec<EventEnvelope>, StoreError> {
        self.inner.all_events()
    }
}

// ============================================================================
// Core Lifecycle & Invariants
// ============================================================================

#[test]
fn test_control_plane_full_lifecycle_and_invariants() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    // 1. Create Studio
    let studio = cp.create_studio("Studio Alpha").unwrap();
    assert_eq!(studio.name, "Studio Alpha");
    let studio_id = studio.id;

    // 2. Register Agents
    let agent = cp
        .register_agent(
            studio_id,
            "Coder Agent",
            AgentKind::Internal,
            Some("Fullstack Developer".into()),
        )
        .unwrap();
    let agent_id = agent.id;

    // 3. Create Tasks with Dependency
    let task1 = cp
        .create_task(
            studio_id,
            "Design Schema",
            "Define database schema",
            None,
            Some(agent_id),
            vec![],
        )
        .unwrap();
    assert_eq!(task1.state, TaskState::Ready);
    let task1_id = task1.id;

    let task2 = cp
        .create_task(
            studio_id,
            "Implement Migrations",
            "Generate SQL migrations",
            Some(task1_id),
            Some(agent_id),
            vec![task1_id],
        )
        .unwrap();
    assert_eq!(task2.state, TaskState::Blocked);
    let task2_id = task2.id;

    // Invariant check: cannot depend on task from another studio
    let foreign_studio = cp.create_studio("Foreign Studio").unwrap();
    let foreign_task = cp
        .create_task(
            foreign_studio.id,
            "Foreign Task",
            "Alien work",
            None,
            None,
            vec![],
        )
        .unwrap();

    let err_foreign = cp
        .add_task_dependency(task1_id, foreign_task.id)
        .unwrap_err();
    assert!(matches!(
        err_foreign,
        ControlPlaneError::StudioMismatch { .. }
    ));

    // 4. Create Run for Task 1
    let run = cp.create_run(task1_id, agent_id).unwrap();
    assert_eq!(run.state, RunState::Queued);
    let run_id = run.id;

    // 5. Request & Resolve Approval
    let approval = cp
        .request_approval(
            studio_id,
            task1_id,
            agent_id,
            ApprovalKind::FileWrite,
            "Allow schema modifications",
        )
        .unwrap();
    assert_eq!(approval.state, ApprovalState::Pending);
    let approval_id = approval.id;

    cp.resolve_approval(approval_id, ApprovalState::Approved)
        .unwrap();
    assert_eq!(
        cp.get_approval(approval_id).unwrap().state,
        ApprovalState::Approved
    );

    // Double resolution rejected
    let err_double = cp
        .resolve_approval(approval_id, ApprovalState::Denied)
        .unwrap_err();
    assert!(matches!(err_double, ControlPlaneError::Transition(_)));

    // 6. Transition Run and Task through lifecycle
    cp.transition_run_state(run_id, RunState::Starting).unwrap();
    cp.transition_run_state(run_id, RunState::Running).unwrap();
    cp.transition_task_state(task1_id, TaskState::Running)
        .unwrap();

    cp.transition_run_state(run_id, RunState::Succeeded)
        .unwrap();
    cp.transition_task_state(task1_id, TaskState::Succeeded)
        .unwrap();

    // Verification: Task 2 automatically unblocked and became Ready
    let task2_after = cp.get_task(task2_id).unwrap();
    assert_eq!(task2_after.state, TaskState::Ready);
    assert!(cp.is_task_ready(task2_id).unwrap());

    // 7. Register Artifact
    let artifact = cp
        .register_artifact(
            studio_id,
            task1_id,
            agent_id,
            ArtifactKind::Patch,
            "migrations.sql",
            Some("sha256-abc123hash".into()),
            "artifacts/migrations.sql",
        )
        .unwrap();
    assert_eq!(artifact.logical_name, "migrations.sql");
    assert_eq!(cp.get_artifact(artifact.id).unwrap().task_id, task1_id);

    // 8. Event sequence checks
    let events = cp.events_for_studio(studio_id, 1).unwrap();
    assert!(!events.is_empty());
    for (i, env) in events.iter().enumerate() {
        assert_eq!(env.sequence, (i + 1) as u64);
        assert_eq!(env.studio_id, studio_id);
    }
}

// ============================================================================
// Hierarchical Cancellation (Parent-Only, NOT DAG Dependencies)
// ============================================================================

#[test]
fn test_hierarchical_cancellation_strictly_follows_parent_hierarchy() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    let studio = cp.create_studio("Cancel Studio").unwrap();
    let studio_id = studio.id;
    let agent = cp
        .register_agent(studio_id, "Worker", AgentKind::Internal, None)
        .unwrap();
    let agent_id = agent.id;

    // Task A: parent root
    let task_a = cp
        .create_task(studio_id, "Task A", "", None, Some(agent_id), vec![])
        .unwrap();

    // Task C: structural child of Task A (parent_task_id = Some(A))
    let task_c = cp
        .create_task(
            studio_id,
            "Task C (Child of A)",
            "",
            Some(task_a.id),
            Some(agent_id),
            vec![],
        )
        .unwrap();

    // Task B: depends on Task A in the DAG, but parent_task_id is NONE
    let task_b = cp
        .create_task(
            studio_id,
            "Task B (Depends on A)",
            "",
            None,
            Some(agent_id),
            vec![task_a.id],
        )
        .unwrap();
    assert_eq!(task_b.state, TaskState::Blocked);

    let run_a = cp.create_run(task_a.id, agent_id).unwrap();
    let run_c = cp.create_run(task_c.id, agent_id).unwrap();
    let approval_a = cp
        .request_approval(
            studio_id,
            task_a.id,
            agent_id,
            ApprovalKind::CommandExecution,
            "Run A",
        )
        .unwrap();

    // Cancel Task A with include_descendants = true
    let summary = cp
        .request_cancellation(
            CancellationScope::Task {
                task_id: task_a.id,
                include_descendants: true,
            },
            Some("Cancel task A and structural descendants".into()),
        )
        .unwrap();

    // Task A and Child C must be cancelled
    assert!(summary.cancelled_tasks.contains(&task_a.id));
    assert!(summary.cancelled_tasks.contains(&task_c.id));
    assert!(summary.cancelled_runs.contains(&run_a.id));
    assert!(summary.cancelled_runs.contains(&run_c.id));
    assert!(summary.cancelled_approvals.contains(&approval_a.id));

    // KEY REQUIREMENT 8: Task B depends on A, but is NOT a child of A.
    // Task B must NOT be cancelled! It remains Blocked.
    assert!(!summary.cancelled_tasks.contains(&task_b.id));
    assert_eq!(cp.get_task(task_b.id).unwrap().state, TaskState::Blocked);
    assert_eq!(cp.get_task(task_a.id).unwrap().state, TaskState::Cancelled);
    assert_eq!(cp.get_task(task_c.id).unwrap().state, TaskState::Cancelled);
}

// ============================================================================
// Task Dependency Mutation Semantics
// ============================================================================

#[test]
fn test_task_dependency_mutation_pre_execution_readiness() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    let studio = cp.create_studio("Dependency Studio").unwrap();
    let sid = studio.id;

    let task1 = cp
        .create_task(sid, "Prerequisite", "", None, None, vec![])
        .unwrap();
    let task2 = cp
        .create_task(sid, "Target Task", "", None, None, vec![])
        .unwrap();

    assert_eq!(task1.state, TaskState::Ready);
    assert_eq!(task2.state, TaskState::Ready);

    // 1. Adding unsatisfied dependency (task1 is Ready, not Succeeded) to Ready task2:
    // Must atomically emit TaskDependencyAdded and TaskStateChanged (Ready -> Blocked).
    let events_before_len = cp.all_events().unwrap().len();
    cp.add_task_dependency(task2.id, task1.id).unwrap();

    assert_eq!(cp.get_task(task2.id).unwrap().state, TaskState::Blocked);
    let events_after = cp.all_events().unwrap();
    assert_eq!(events_after.len(), events_before_len + 2);
    assert!(matches!(
        events_after[events_before_len].event,
        ControlPlaneEvent::TaskDependencyAdded { .. }
    ));
    assert!(matches!(
        events_after[events_before_len + 1].event,
        ControlPlaneEvent::TaskStateChanged {
            previous_state: TaskState::Ready,
            new_state: TaskState::Blocked,
            ..
        }
    ));

    // 2. Idempotent add: adding already present dependency is a no-op (no new events)
    let count_before = cp.all_events().unwrap().len();
    cp.add_task_dependency(task2.id, task1.id).unwrap();
    assert_eq!(cp.all_events().unwrap().len(), count_before);

    // 3. Removing dependency: all remaining (none) are succeeded, task2 transitions Blocked -> Ready
    cp.remove_task_dependency(task2.id, task1.id).unwrap();
    assert_eq!(cp.get_task(task2.id).unwrap().state, TaskState::Ready);
    let events_after_rem = cp.all_events().unwrap();
    assert_eq!(events_after_rem.len(), count_before + 2);
    assert!(matches!(
        events_after_rem[count_before].event,
        ControlPlaneEvent::TaskDependencyRemoved { .. }
    ));
    assert!(matches!(
        events_after_rem[count_before + 1].event,
        ControlPlaneEvent::TaskStateChanged {
            previous_state: TaskState::Blocked,
            new_state: TaskState::Ready,
            ..
        }
    ));

    // 4. Idempotent remove: removing absent dependency is a no-op (no new events)
    let count_before_rem = cp.all_events().unwrap().len();
    cp.remove_task_dependency(task2.id, task1.id).unwrap();
    assert_eq!(cp.all_events().unwrap().len(), count_before_rem);

    // 5. Dependency mutation forbidden once task starts Running or terminates
    cp.transition_task_state(task2.id, TaskState::Running)
        .unwrap();
    let err_mut = cp.add_task_dependency(task2.id, task1.id).unwrap_err();
    assert!(matches!(
        err_mut,
        ControlPlaneError::Transition(TransitionError::TaskDependencyMutationForbidden { .. })
    ));
}

// ============================================================================
// Failure Injection Tests (Rollback & Zero Mutation)
// ============================================================================

#[test]
fn test_failure_injection_zero_mutation_rollback() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let (store, fail_trigger) = FailingEventStore::new();
    let mut cp = ControlPlane::new(clock, store);

    let studio = cp.create_studio("Fault Studio").unwrap();
    let sid = studio.id;
    let agent = cp
        .register_agent(sid, "Agent", AgentKind::Internal, None)
        .unwrap();
    let task = cp
        .create_task(sid, "Task", "", None, Some(agent.id), vec![])
        .unwrap();
    let task_dep = cp
        .create_task(sid, "Prereq", "", None, Some(agent.id), vec![])
        .unwrap();

    // 1. Failure during ordinary state transition
    let state_before = cp.state().clone();
    let events_before = cp.all_events().unwrap();

    fail_trigger.store(true, Ordering::SeqCst);
    let err = cp
        .transition_task_state(task.id, TaskState::Running)
        .unwrap_err();
    assert!(matches!(err, ControlPlaneError::Store(_)));

    // State and store must be 100% unchanged
    assert_eq!(cp.state(), &state_before);
    assert_eq!(cp.all_events().unwrap(), events_before);

    // 2. Failure during dependency mutation
    let state_before_dep = cp.state().clone();
    let events_before_dep = cp.all_events().unwrap();

    fail_trigger.store(true, Ordering::SeqCst);
    let err_dep = cp.add_task_dependency(task.id, task_dep.id).unwrap_err();
    assert!(matches!(err_dep, ControlPlaneError::Store(_)));

    assert_eq!(cp.state(), &state_before_dep);
    assert_eq!(cp.all_events().unwrap(), events_before_dep);

    // 3. Failure during cancellation
    let state_before_cancel = cp.state().clone();
    let events_before_cancel = cp.all_events().unwrap();

    fail_trigger.store(true, Ordering::SeqCst);
    let err_cancel = cp
        .request_cancellation(
            CancellationScope::Task {
                task_id: task.id,
                include_descendants: true,
            },
            Some("Cancel attempt".into()),
        )
        .unwrap_err();
    assert!(matches!(err_cancel, ControlPlaneError::Store(_)));

    assert_eq!(cp.state(), &state_before_cancel);
    assert_eq!(cp.all_events().unwrap(), events_before_cancel);
}

// ============================================================================
// Event Replay Parity Test
// ============================================================================

#[test]
fn test_event_replay_reconstructs_identical_domain_state() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut original_cp = ControlPlane::new(clock, store);

    // Build rich domain state across two studios
    let s1 = original_cp.create_studio("Studio 1").unwrap();
    let s2 = original_cp.create_studio("Studio 2").unwrap();

    let _a1 = original_cp
        .register_agent(s1.id, "Agent 1", AgentKind::Internal, None)
        .unwrap();
    let _a2 = original_cp
        .register_agent(s2.id, "Agent 2", AgentKind::External, None)
        .unwrap();

    let t1 = original_cp
        .create_task(s1.id, "Task 1", "T1", None, Some(_a1.id), vec![])
        .unwrap();
    let t2 = original_cp
        .create_task(
            s1.id,
            "Task 2",
            "T2",
            Some(t1.id),
            Some(_a1.id),
            vec![t1.id],
        )
        .unwrap();

    let r1 = original_cp.create_run(t1.id, _a1.id).unwrap();
    let app1 = original_cp
        .request_approval(
            s1.id,
            t1.id,
            _a1.id,
            ApprovalKind::CommandExecution,
            "Run bash",
        )
        .unwrap();
    original_cp
        .resolve_approval(app1.id, ApprovalState::Approved)
        .unwrap();

    original_cp
        .transition_run_state(r1.id, RunState::Starting)
        .unwrap();
    original_cp
        .transition_run_state(r1.id, RunState::Running)
        .unwrap();
    original_cp
        .transition_task_state(t1.id, TaskState::Running)
        .unwrap();
    original_cp
        .transition_run_state(r1.id, RunState::Succeeded)
        .unwrap();
    original_cp
        .transition_task_state(t1.id, TaskState::Succeeded)
        .unwrap();

    let art1 = original_cp
        .register_artifact(
            s1.id,
            t1.id,
            _a1.id,
            ArtifactKind::Log,
            "execution.log",
            None,
            "logs/exec.log",
        )
        .unwrap();

    // Verify task2 unblocked in original
    assert_eq!(original_cp.get_task(t2.id).unwrap().state, TaskState::Ready);

    // Collect all event envelopes emitted by original
    let all_events = original_cp.all_events().unwrap();
    assert!(!all_events.is_empty());

    // Replay into a completely blank instance
    let replay_clock = FixedClock::new(now);
    let replay_store = InMemoryStore::new();
    let replayed_cp = ControlPlane::replay_events(&all_events, replay_clock, replay_store).unwrap();

    // Assert exact state parity
    let orig_studios: HashSet<_> = original_cp.all_studios().map(|s| s.id).collect();
    let replayed_studios: HashSet<_> = replayed_cp.all_studios().map(|s| s.id).collect();
    assert_eq!(orig_studios, replayed_studios);

    let orig_agents: HashSet<_> = original_cp.all_agents().map(|a| a.id).collect();
    let replayed_agents: HashSet<_> = replayed_cp.all_agents().map(|a| a.id).collect();
    assert_eq!(orig_agents, replayed_agents);

    let orig_tasks: HashSet<_> = original_cp.all_tasks().map(|t| t.id).collect();
    let replayed_tasks: HashSet<_> = replayed_cp.all_tasks().map(|t| t.id).collect();
    assert_eq!(orig_tasks, replayed_tasks);

    let orig_runs: HashSet<_> = original_cp.all_runs().map(|r| r.id).collect();
    let replayed_runs: HashSet<_> = replayed_cp.all_runs().map(|r| r.id).collect();
    assert_eq!(orig_runs, replayed_runs);

    let orig_apps: HashSet<_> = original_cp.all_approvals().map(|a| a.id).collect();
    let replayed_apps: HashSet<_> = replayed_cp.all_approvals().map(|a| a.id).collect();
    assert_eq!(orig_apps, replayed_apps);

    let orig_arts: HashSet<_> = original_cp.all_artifacts().map(|a| a.id).collect();
    let replayed_arts: HashSet<_> = replayed_cp.all_artifacts().map(|a| a.id).collect();
    assert_eq!(orig_arts, replayed_arts);
    assert_eq!(
        original_cp.get_artifact(art1.id).unwrap(),
        replayed_cp.get_artifact(art1.id).unwrap()
    );

    assert_eq!(original_cp.state(), replayed_cp.state());
    assert_eq!(
        original_cp.all_events().unwrap(),
        replayed_cp.all_events().unwrap()
    );
}

// ============================================================================
// 19 Replay Corruption Tests
// ============================================================================

fn make_base_replay_harness() -> (StudioId, AgentId, TaskId, DateTime<Utc>, Vec<EventEnvelope>) {
    let now = Utc::now();
    let sid = StudioId::new();
    let aid = AgentId::new();
    let tid = TaskId::new();

    let mut envelopes = Vec::new();
    let studio = Studio::with_id(sid, "Base Studio", now);
    envelopes.push(EventEnvelope::new(
        sid,
        1,
        now,
        ControlPlaneEvent::StudioCreated { studio },
    ));

    let mut agent = AgentDescriptor::new(sid, "Agent 1", AgentKind::Internal, None);
    agent.id = aid;
    envelopes.push(EventEnvelope::new(
        sid,
        2,
        now,
        ControlPlaneEvent::AgentRegistered { agent },
    ));

    let mut task = TaskRecord::new(sid, "Task 1", "Desc", None, Some(aid), vec![], now);
    task.id = tid;
    envelopes.push(EventEnvelope::new(
        sid,
        3,
        now,
        ControlPlaneEvent::TaskCreated { task },
    ));

    (sid, aid, tid, now, envelopes)
}

#[test]
fn test_replay_corrupted_01_agent_registered_before_studio_created() {
    let now = Utc::now();
    let sid = StudioId::new();
    let agent = AgentDescriptor::new(sid, "Agent", AgentKind::Internal, None);

    let events = vec![EventEnvelope::new(
        sid,
        1,
        now,
        ControlPlaneEvent::AgentRegistered { agent },
    )];

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::StudioNotFound { studio_id }) if studio_id == sid));
}

#[test]
fn test_replay_corrupted_02_wrong_studio_id_envelope() {
    let now = Utc::now();
    let sid1 = StudioId::new();
    let sid2 = StudioId::new();
    let studio = Studio::with_id(sid1, "Studio 1", now);

    // Envelope says sid2, but event payload has studio.id = sid1
    let events = vec![EventEnvelope::new(
        sid2,
        1,
        now,
        ControlPlaneEvent::StudioCreated { studio },
    )];

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(
        res,
        Err(ReplayError::StudioMismatch { expected, actual }) if expected == sid1 && actual == sid2
    ));
}

#[test]
fn test_replay_corrupted_03_duplicate_studio() {
    let now = Utc::now();
    let sid = StudioId::new();
    let studio = Studio::with_id(sid, "Studio 1", now);

    let events = vec![
        EventEnvelope::new(
            sid,
            1,
            now,
            ControlPlaneEvent::StudioCreated {
                studio: studio.clone(),
            },
        ),
        EventEnvelope::new(
            sid,
            2,
            now,
            ControlPlaneEvent::StudioCreated {
                studio: studio.clone(),
            },
        ),
    ];

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::DuplicateStudio { studio_id }) if studio_id == sid));
}

#[test]
fn test_replay_corrupted_04_duplicate_agent() {
    let (sid, _aid, _tid, now, mut events) = make_base_replay_harness();
    let agent = match &events[1].event {
        ControlPlaneEvent::AgentRegistered { agent } => agent.clone(),
        _ => unreachable!(),
    };

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::AgentRegistered { agent },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::DuplicateAgent { .. })));
}

#[test]
fn test_replay_corrupted_05_task_with_missing_agent() {
    let now = Utc::now();
    let sid = StudioId::new();
    let missing_aid = AgentId::new();
    let studio = Studio::with_id(sid, "Studio", now);

    let mut task = TaskRecord::new(sid, "Task", "", None, Some(missing_aid), vec![], now);
    task.id = TaskId::new();

    let events = vec![
        EventEnvelope::new(sid, 1, now, ControlPlaneEvent::StudioCreated { studio }),
        EventEnvelope::new(sid, 2, now, ControlPlaneEvent::TaskCreated { task }),
    ];

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::AgentNotFound { agent_id }) if agent_id == missing_aid));
}

#[test]
fn test_replay_corrupted_06_task_with_missing_parent() {
    let (sid, aid, _tid, now, mut events) = make_base_replay_harness();
    let missing_parent = TaskId::new();

    let mut task = TaskRecord::new(
        sid,
        "Child",
        "",
        Some(missing_parent),
        Some(aid),
        vec![],
        now,
    );
    task.id = TaskId::new();

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::TaskCreated { task },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::TaskNotFound { task_id }) if task_id == missing_parent));
}

#[test]
fn test_replay_corrupted_07_foreign_studio_dependency() {
    let now = Utc::now();
    let s1 = StudioId::new();
    let s2 = StudioId::new();

    let mut t1 = TaskRecord::new(s1, "Task 1", "", None, None, vec![], now);
    t1.id = TaskId::new();

    let mut t2 = TaskRecord::new(s2, "Task 2", "", None, None, vec![], now);
    t2.id = TaskId::new();

    let events = vec![
        EventEnvelope::new(
            s1,
            1,
            now,
            ControlPlaneEvent::StudioCreated {
                studio: Studio::with_id(s1, "S1", now),
            },
        ),
        EventEnvelope::new(
            s2,
            1,
            now,
            ControlPlaneEvent::StudioCreated {
                studio: Studio::with_id(s2, "S2", now),
            },
        ),
        EventEnvelope::new(
            s1,
            2,
            now,
            ControlPlaneEvent::TaskCreated { task: t1.clone() },
        ),
        EventEnvelope::new(
            s2,
            2,
            now,
            ControlPlaneEvent::TaskCreated { task: t2.clone() },
        ),
        // Add dependency from t1 (S1) to t2 (S2)
        EventEnvelope::new(
            s1,
            3,
            now,
            ControlPlaneEvent::TaskDependencyAdded {
                task_id: t1.id,
                dependency_id: t2.id,
            },
        ),
    ];

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::StudioMismatch { .. })));
}

#[test]
fn test_replay_corrupted_08_unknown_task_state_changed_target() {
    let (sid, _aid, _tid, now, mut events) = make_base_replay_harness();
    let unknown_tid = TaskId::new();

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::TaskStateChanged {
            task_id: unknown_tid,
            previous_state: TaskState::Ready,
            new_state: TaskState::Running,
        },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::TaskNotFound { task_id }) if task_id == unknown_tid));
}

#[test]
fn test_replay_corrupted_09_wrong_previous_task_state() {
    let (sid, _aid, tid, now, mut events) = make_base_replay_harness();

    // Task 1 is currently in Ready state. Event claims previous state was Blocked.
    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::TaskStateChanged {
            task_id: tid,
            previous_state: TaskState::Blocked,
            new_state: TaskState::Ready,
        },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::StateMismatch { .. })));
}

#[test]
fn test_replay_corrupted_10_unknown_run_state_changed_target() {
    let (sid, _aid, _tid, now, mut events) = make_base_replay_harness();
    let unknown_rid = RunId::new();

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::RunStateChanged {
            run_id: unknown_rid,
            previous_state: RunState::Queued,
            new_state: RunState::Starting,
        },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::RunNotFound { run_id }) if run_id == unknown_rid));
}

#[test]
fn test_replay_corrupted_11_run_with_missing_task() {
    let (sid, aid, _tid, now, mut events) = make_base_replay_harness();
    let missing_tid = TaskId::new();
    let run = RunRecord::new(missing_tid, aid, 1);

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::RunCreated { run },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::TaskNotFound { task_id }) if task_id == missing_tid));
}

#[test]
fn test_replay_corrupted_12_run_with_foreign_agent() {
    let now = Utc::now();
    let s1 = StudioId::new();
    let s2 = StudioId::new();

    let mut t1 = TaskRecord::new(s1, "Task", "", None, None, vec![], now);
    t1.id = TaskId::new();

    let a2 = AgentDescriptor::new(s2, "Agent 2", AgentKind::Internal, None);
    let run = RunRecord::new(t1.id, a2.id, 1);

    let events = vec![
        EventEnvelope::new(
            s1,
            1,
            now,
            ControlPlaneEvent::StudioCreated {
                studio: Studio::with_id(s1, "S1", now),
            },
        ),
        EventEnvelope::new(
            s2,
            1,
            now,
            ControlPlaneEvent::StudioCreated {
                studio: Studio::with_id(s2, "S2", now),
            },
        ),
        EventEnvelope::new(s1, 2, now, ControlPlaneEvent::TaskCreated { task: t1 }),
        EventEnvelope::new(s2, 2, now, ControlPlaneEvent::AgentRegistered { agent: a2 }),
        // Run created under s1 with agent from s2
        EventEnvelope::new(s1, 3, now, ControlPlaneEvent::RunCreated { run }),
    ];

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::StudioMismatch { .. })));
}

#[test]
fn test_replay_corrupted_13_unknown_approval() {
    let (sid, _aid, _tid, now, mut events) = make_base_replay_harness();
    let unknown_app_id = ApprovalId::new();

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::ApprovalResolved {
            approval_id: unknown_app_id,
            previous_state: ApprovalState::Pending,
            new_state: ApprovalState::Approved,
        },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(
        matches!(res, Err(ReplayError::ApprovalNotFound { approval_id }) if approval_id == unknown_app_id)
    );
}

#[test]
fn test_replay_corrupted_14_invalid_artifact_producer() {
    let (sid, _aid, tid, now, mut events) = make_base_replay_harness();
    let unknown_aid = AgentId::new();

    let art = ArtifactRecord::new(
        sid,
        tid,
        unknown_aid,
        ArtifactKind::Log,
        "test.log",
        None,
        "logs/test.log",
        now,
    );

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::ArtifactRegistered { artifact: art },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::AgentNotFound { agent_id }) if agent_id == unknown_aid));
}

#[test]
fn test_replay_corrupted_15_dependency_cycle() {
    let (sid, aid, tid, now, mut events) = make_base_replay_harness();
    let mut task_b = TaskRecord::new(sid, "Task B", "", None, Some(aid), vec![tid], now);
    task_b.id = TaskId::new();

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::TaskCreated {
            task: task_b.clone(),
        },
    ));

    // Closing the cycle: Task tid depends on Task task_b.id
    events.push(EventEnvelope::new(
        sid,
        5,
        now,
        ControlPlaneEvent::TaskDependencyAdded {
            task_id: tid,
            dependency_id: task_b.id,
        },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::DependencyCycle { .. })));
}

#[test]
fn test_replay_corrupted_16_sequence_gap() {
    let (sid, _aid, _tid, now, mut events) = make_base_replay_harness();
    let studio = Studio::with_id(sid, "Studio Updated", now);

    // Sequence jumps to 5 instead of 4
    events.push(EventEnvelope::new(
        sid,
        5,
        now,
        ControlPlaneEvent::StudioCreated { studio },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(
        res,
        Err(ReplayError::InvalidSequence {
            expected: 4,
            actual: 5
        })
    ));
}

#[test]
fn test_replay_corrupted_17_sequence_regression() {
    let (sid, _aid, _tid, now, mut events) = make_base_replay_harness();
    let studio = Studio::with_id(sid, "Studio Regressed", now);

    // Sequence regresses to 2 instead of 4
    events.push(EventEnvelope::new(
        sid,
        2,
        now,
        ControlPlaneEvent::StudioCreated { studio },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(
        res,
        Err(ReplayError::Store(StoreError::SequenceRegression { .. }))
    ));
}

#[test]
fn test_replay_corrupted_18_duplicate_event_id() {
    let (sid, aid, _tid, now, mut events) = make_base_replay_harness();
    let dup_id = events[0].event_id;

    let mut task_b = TaskRecord::new(sid, "Task B", "", None, Some(aid), vec![], now);
    task_b.id = TaskId::new();

    let mut dup_envelope =
        EventEnvelope::new(sid, 4, now, ControlPlaneEvent::TaskCreated { task: task_b });
    dup_envelope.event_id = dup_id;

    events.push(dup_envelope);

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::DuplicateEventId { event_id }) if event_id == dup_id));
}

#[test]
fn test_replay_corrupted_19_unsupported_schema_version() {
    let (sid, aid, _tid, now, mut events) = make_base_replay_harness();

    let mut task_b = TaskRecord::new(sid, "Task B", "", None, Some(aid), vec![], now);
    task_b.id = TaskId::new();

    let mut bad_version_env =
        EventEnvelope::new(sid, 4, now, ControlPlaneEvent::TaskCreated { task: task_b });
    bad_version_env.schema_version = 99; // Unsupported version

    events.push(bad_version_env);

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(
        res,
        Err(ReplayError::UnsupportedSchemaVersion {
            version: 99,
            supported: CONTROL_PLANE_EVENT_SCHEMA_VERSION
        })
    ));
}

#[test]
fn test_replay_corrupted_duplicate_task() {
    let (sid, aid, tid, now, mut events) = make_base_replay_harness();
    let mut dup_task = TaskRecord::new(sid, "Duplicate Task", "", None, Some(aid), vec![], now);
    dup_task.id = tid;

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::TaskCreated { task: dup_task },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::DuplicateTask { task_id }) if task_id == tid));
}

#[test]
fn test_replay_corrupted_duplicate_run() {
    let (sid, aid, tid, now, mut events) = make_base_replay_harness();
    let rid = RunId::new();
    let mut run1 = RunRecord::new(tid, aid, 1);
    run1.id = rid;
    let mut run2 = RunRecord::new(tid, aid, 2);
    run2.id = rid;

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::RunCreated { run: run1 },
    ));
    events.push(EventEnvelope::new(
        sid,
        5,
        now,
        ControlPlaneEvent::RunCreated { run: run2 },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::DuplicateRun { run_id }) if run_id == rid));
}

#[test]
fn test_replay_corrupted_duplicate_approval() {
    let (sid, aid, tid, now, mut events) = make_base_replay_harness();
    let app_id = ApprovalId::new();
    let mut app1 = ApprovalRequest::new(sid, tid, aid, ApprovalKind::FileWrite, "App 1", now);
    app1.id = app_id;
    let mut app2 =
        ApprovalRequest::new(sid, tid, aid, ApprovalKind::CommandExecution, "App 2", now);
    app2.id = app_id;

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::ApprovalRequested { approval: app1 },
    ));
    events.push(EventEnvelope::new(
        sid,
        5,
        now,
        ControlPlaneEvent::ApprovalRequested { approval: app2 },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(
        matches!(res, Err(ReplayError::DuplicateApproval { approval_id }) if approval_id == app_id)
    );
}

#[test]
fn test_replay_corrupted_duplicate_artifact() {
    let (sid, aid, tid, now, mut events) = make_base_replay_harness();
    let art_id = ArtifactId::new();
    let mut art1 = ArtifactRecord::new(
        sid,
        tid,
        aid,
        ArtifactKind::Log,
        "log1.txt",
        None,
        "logs/1",
        now,
    );
    art1.id = art_id;
    let mut art2 = ArtifactRecord::new(
        sid,
        tid,
        aid,
        ArtifactKind::Log,
        "log2.txt",
        None,
        "logs/2",
        now,
    );
    art2.id = art_id;

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::ArtifactRegistered { artifact: art1 },
    ));
    events.push(EventEnvelope::new(
        sid,
        5,
        now,
        ControlPlaneEvent::ArtifactRegistered { artifact: art2 },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(
        matches!(res, Err(ReplayError::DuplicateArtifact { artifact_id }) if artifact_id == art_id)
    );
}

#[test]
fn test_cross_studio_remove_task_dependency_rejected() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    let s1 = cp.create_studio("S1").unwrap();
    let s2 = cp.create_studio("S2").unwrap();

    let t1 = cp.create_task(s1.id, "T1", "", None, None, vec![]).unwrap();
    let t2 = cp.create_task(s2.id, "T2", "", None, None, vec![]).unwrap();

    let err = cp.remove_task_dependency(t1.id, t2.id).unwrap_err();
    assert!(matches!(
        err,
        ControlPlaneError::StudioMismatch { expected, actual } if expected == s1.id && actual == s2.id
    ));
}

#[test]
fn test_replay_corrupted_cross_studio_dependency_removed() {
    let now = Utc::now();
    let s1 = StudioId::new();
    let s2 = StudioId::new();

    let mut t1 = TaskRecord::new(s1, "T1", "", None, None, vec![], now);
    t1.id = TaskId::new();
    let mut t2 = TaskRecord::new(s2, "T2", "", None, None, vec![], now);
    t2.id = TaskId::new();

    let events = vec![
        EventEnvelope::new(
            s1,
            1,
            now,
            ControlPlaneEvent::StudioCreated {
                studio: Studio::with_id(s1, "S1", now),
            },
        ),
        EventEnvelope::new(
            s2,
            1,
            now,
            ControlPlaneEvent::StudioCreated {
                studio: Studio::with_id(s2, "S2", now),
            },
        ),
        EventEnvelope::new(
            s1,
            2,
            now,
            ControlPlaneEvent::TaskCreated { task: t1.clone() },
        ),
        EventEnvelope::new(
            s2,
            2,
            now,
            ControlPlaneEvent::TaskCreated { task: t2.clone() },
        ),
        EventEnvelope::new(
            s1,
            3,
            now,
            ControlPlaneEvent::TaskDependencyRemoved {
                task_id: t1.id,
                dependency_id: t2.id,
            },
        ),
    ];

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::StudioMismatch { .. })));
}

#[test]
fn test_create_task_batch_success() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    let studio = cp.create_studio("Batch Studio").unwrap();
    let agent = cp
        .register_agent(studio.id, "Worker", AgentKind::Internal, None)
        .unwrap();

    let batch = vec![
        BatchTaskSpec {
            key: "task_a".to_string(),
            title: "Task A".to_string(),
            description: "First task".to_string(),
            assigned_agent_id: Some(agent.id),
            parent_task_key: None,
            parent_task_id: None,
            dependency_keys: vec![],
            dependency_task_ids: vec![],
        },
        BatchTaskSpec {
            key: "task_b".to_string(),
            title: "Task B".to_string(),
            description: "Second task depends on A".to_string(),
            assigned_agent_id: Some(agent.id),
            parent_task_key: Some("task_a".to_string()),
            parent_task_id: None,
            dependency_keys: vec!["task_a".to_string()],
            dependency_task_ids: vec![],
        },
        BatchTaskSpec {
            key: "task_c".to_string(),
            title: "Task C".to_string(),
            description: "Third task depends on B".to_string(),
            assigned_agent_id: Some(agent.id),
            parent_task_key: None,
            parent_task_id: None,
            dependency_keys: vec!["task_b".to_string()],
            dependency_task_ids: vec![],
        },
    ];

    let tasks = cp.create_task_batch(studio.id, batch).unwrap();
    assert_eq!(tasks.len(), 3);

    let task_a = &tasks[0];
    let task_b = &tasks[1];
    let task_c = &tasks[2];

    assert_eq!(task_a.state, TaskState::Ready);
    assert_eq!(task_b.state, TaskState::Blocked);
    assert_eq!(task_c.state, TaskState::Blocked);

    assert_eq!(task_b.parent_task_id, Some(task_a.id));
    assert_eq!(task_b.dependencies, vec![task_a.id]);
    assert_eq!(task_c.dependencies, vec![task_b.id]);

    let events = cp.events_for_studio(studio.id, 1).unwrap();
    assert_eq!(events.len(), 5);
}

#[test]
fn test_create_task_batch_transactional_all_or_zero() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    let studio = cp.create_studio("Batch Atomic Studio").unwrap();

    let initial_task_count = cp.all_tasks().count();
    let initial_event_count = cp.events_for_studio(studio.id, 1).unwrap().len();

    let batch = vec![
        BatchTaskSpec {
            key: "t1".to_string(),
            title: "T1".to_string(),
            description: "desc1".to_string(),
            assigned_agent_id: None,
            parent_task_key: None,
            parent_task_id: None,
            dependency_keys: vec![],
            dependency_task_ids: vec![],
        },
        BatchTaskSpec {
            key: "t2".to_string(),
            title: "T2".to_string(),
            description: "desc2".to_string(),
            assigned_agent_id: None,
            parent_task_key: None,
            parent_task_id: None,
            dependency_keys: vec!["t1".to_string()],
            dependency_task_ids: vec![],
        },
        BatchTaskSpec {
            key: "t3".to_string(),
            title: "T3".to_string(),
            description: "desc3".to_string(),
            assigned_agent_id: None,
            parent_task_key: None,
            parent_task_id: None,
            dependency_keys: vec!["t2".to_string()],
            dependency_task_ids: vec![],
        },
        BatchTaskSpec {
            key: "t4".to_string(),
            title: "T4".to_string(),
            description: "desc4".to_string(),
            assigned_agent_id: None,
            parent_task_key: None,
            parent_task_id: None,
            dependency_keys: vec!["non_existent_key".to_string()],
            dependency_task_ids: vec![],
        },
    ];

    let result = cp.create_task_batch(studio.id, batch);
    assert!(result.is_err());

    assert_eq!(cp.all_tasks().count(), initial_task_count);
    assert_eq!(
        cp.events_for_studio(studio.id, 1).unwrap().len(),
        initial_event_count
    );
}

#[test]
fn test_create_task_batch_duplicate_key_rejected() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    let studio = cp.create_studio("Dup Key Studio").unwrap();

    let batch = vec![
        BatchTaskSpec {
            key: "same_key".to_string(),
            title: "T1".to_string(),
            description: "desc1".to_string(),
            assigned_agent_id: None,
            parent_task_key: None,
            parent_task_id: None,
            dependency_keys: vec![],
            dependency_task_ids: vec![],
        },
        BatchTaskSpec {
            key: "same_key".to_string(),
            title: "T2".to_string(),
            description: "desc2".to_string(),
            assigned_agent_id: None,
            parent_task_key: None,
            parent_task_id: None,
            dependency_keys: vec![],
            dependency_task_ids: vec![],
        },
    ];

    let result = cp.create_task_batch(studio.id, batch);
    assert!(result.is_err());
    assert_eq!(cp.all_tasks().count(), 0);
}

#[test]
fn test_register_agent_batch_success_and_atomic_failure() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    let studio = cp.create_studio("Batch Agent Studio").unwrap();

    let id1 = AgentId::new();
    let id2 = AgentId::new();
    let batch = vec![
        BatchAgentSpec::new(id1, "Worker 1", AgentKind::Internal, Some("Role 1".into())),
        BatchAgentSpec::new(id2, "Worker 2", AgentKind::Internal, Some("Role 2".into())),
    ];

    let registered = cp.register_agent_batch(studio.id, batch).unwrap();
    assert_eq!(registered.len(), 2);
    assert_eq!(registered[0].id, id1);
    assert_eq!(registered[1].id, id2);

    // Duplicate within batch should fail atomically
    let id3 = AgentId::new();
    let dup_batch = vec![
        BatchAgentSpec::new(id3, "Worker 3", AgentKind::Internal, None),
        BatchAgentSpec::new(id3, "Worker 3 duplicate", AgentKind::Internal, None),
    ];
    let err = cp.register_agent_batch(studio.id, dup_batch).unwrap_err();
    assert!(matches!(err, ControlPlaneError::DuplicateAgent(id) if id == id3));

    // Agent already registered should fail atomically
    let existing_batch = vec![
        BatchAgentSpec::new(AgentId::new(), "New Worker", AgentKind::Internal, None),
        BatchAgentSpec::new(id1, "Worker 1 Conflict", AgentKind::Internal, None),
    ];
    let err = cp
        .register_agent_batch(studio.id, existing_batch)
        .unwrap_err();
    assert!(matches!(err, ControlPlaneError::DuplicateAgent(id) if id == id1));
}

#[test]
fn test_record_run_outcome_and_strict_replay() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    let studio = cp.create_studio("Outcome Studio").unwrap();
    let agent = cp
        .register_agent(studio.id, "Agent", AgentKind::Internal, None)
        .unwrap();
    let task = cp
        .create_task(studio.id, "Task", "", None, Some(agent.id), vec![])
        .unwrap();
    let run = cp.create_run(task.id, agent.id).unwrap();

    cp.record_run_outcome(
        run.id,
        "budget_exceeded",
        Some("Tool call limit reached".into()),
    )
    .unwrap();

    // Verify replay works
    let events = cp.events_for_studio(studio.id, 1).unwrap();
    let replayed =
        ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new()).unwrap();
    assert_eq!(replayed.all_runs().count(), 1);
}

#[test]
fn test_replay_corrupted_strict_agent_and_run_checks() {
    let (sid, aid, tid, now, mut events) = make_base_replay_harness();
    let unknown_agent = AgentId::new();

    // 1. ToolStarted with unknown agent fails
    let mut bad_events = events.clone();
    bad_events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::ToolStarted {
            agent_id: unknown_agent,
            task_id: Some(tid),
            run_id: None,
            tool_name: "test_tool".into(),
            call_id: "c1".into(),
            timestamp: now,
        },
    ));
    let res = ControlPlane::replay_events(&bad_events, FixedClock::new(now), InMemoryStore::new());
    assert!(
        matches!(res, Err(ReplayError::AgentNotFound { agent_id }) if agent_id == unknown_agent)
    );

    // 2. RunOutcomeRecorded with mismatched run agent fails
    let other_agent = AgentDescriptor::new(sid, "Other", AgentKind::Internal, None);
    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::AgentRegistered {
            agent: other_agent.clone(),
        },
    ));
    let run = RunRecord::new(tid, aid, 1);
    events.push(EventEnvelope::new(
        sid,
        5,
        now,
        ControlPlaneEvent::RunCreated { run: run.clone() },
    ));

    let mut mismatched_events = events.clone();
    mismatched_events.push(EventEnvelope::new(
        sid,
        6,
        now,
        ControlPlaneEvent::RunOutcomeRecorded {
            run_id: run.id,
            task_id: tid,
            agent_id: other_agent.id, // Mismatched agent!
            classification: "test".into(),
            safe_error_summary: None,
        },
    ));
    let res = ControlPlane::replay_events(
        &mismatched_events,
        FixedClock::new(now),
        InMemoryStore::new(),
    );
    assert!(matches!(res, Err(ReplayError::DomainViolation(_))));
}

#[test]
fn test_worktree_lifecycle_full() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    let studio = cp.create_studio("Worktree Studio").unwrap();
    let agent = cp
        .register_agent(studio.id, "Coder Agent", AgentKind::Internal, None)
        .unwrap();
    let task = cp
        .create_task(
            studio.id,
            "Mutating Task",
            "Refactor code",
            None,
            Some(agent.id),
            vec![],
        )
        .unwrap();
    let run = cp.create_run(task.id, agent.id).unwrap();

    // 1. Create worktree
    let worktree = cp
        .create_worktree(
            studio.id,
            "wt-mutating-1",
            "C:/repos/primary",
            "C:/worktrees/wt-mutating-1",
            "abc1234",
        )
        .unwrap();
    assert_eq!(worktree.state, WorktreeState::Creating);
    assert_eq!(worktree.studio_id, studio.id);

    // 2. Transition Creating -> Ready
    cp.transition_worktree_state(worktree.id, WorktreeState::Ready)
        .unwrap();
    assert_eq!(
        cp.get_worktree(worktree.id).unwrap().state,
        WorktreeState::Ready
    );

    // 3. Assign task, agent, run
    cp.assign_worktree(worktree.id, task.id, agent.id, Some(run.id))
        .unwrap();
    let wt = cp.get_worktree(worktree.id).unwrap();
    assert_eq!(wt.assigned_task_id, Some(task.id));
    assert_eq!(wt.assigned_agent_id, Some(agent.id));
    assert_eq!(wt.assigned_run_id, Some(run.id));

    // 4. Bind thread
    cp.bind_worktree_thread(worktree.id, "thread-worker-1")
        .unwrap();
    assert_eq!(
        cp.get_worktree(worktree.id).unwrap().bound_thread_id,
        Some("thread-worker-1".to_string())
    );

    // Rebind same thread succeeds
    cp.bind_worktree_thread(worktree.id, "thread-worker-1")
        .unwrap();

    // Rebind different thread fails with WorktreeOwnershipConflict
    let conflict_err = cp
        .bind_worktree_thread(worktree.id, "thread-worker-2")
        .unwrap_err();
    assert!(matches!(
        conflict_err,
        ControlPlaneError::WorktreeOwnershipConflict { .. }
    ));

    // 5. Transition Ready -> InUse
    cp.transition_worktree_state(worktree.id, WorktreeState::InUse)
        .unwrap();

    // 6. Record change captured with patch artifact
    let patch = cp
        .register_artifact(
            studio.id,
            task.id,
            agent.id,
            ArtifactKind::Patch,
            "changes.patch",
            Some("sha256-patch".into()),
            "artifacts/patch.diff",
        )
        .unwrap();

    cp.record_worktree_change_captured(
        worktree.id,
        Some(run.id),
        "abc1234",
        Some("def5678".into()),
        patch.id,
        None,
        3,
    )
    .unwrap();

    let wt_captured = cp.get_worktree(worktree.id).unwrap();
    assert_eq!(
        wt_captured.last_captured_commit,
        Some("def5678".to_string())
    );
    assert_eq!(wt_captured.patch_artifact_id, Some(patch.id));

    // 7. Transition InUse -> ChangeCaptured -> ReconcilePending
    cp.transition_worktree_state(worktree.id, WorktreeState::ChangeCaptured)
        .unwrap();
    cp.transition_worktree_state(worktree.id, WorktreeState::ReconcilePending)
        .unwrap();

    // 8. Release worktree with retention
    cp.release_worktree(worktree.id, true, Some("Audit trail retention".into()))
        .unwrap();
    let wt_retained = cp.get_worktree(worktree.id).unwrap();
    assert!(wt_retained.retained);
    assert_eq!(
        wt_retained.retained_reason,
        Some("Audit trail retention".into())
    );
    assert!(wt_retained.released_at.is_some());

    // 9. Replay all events -> identical domain state
    let events = cp.events_for_studio(studio.id, 1).unwrap();
    let replayed =
        ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new()).unwrap();
    assert_eq!(replayed.all_worktrees().count(), 1);
    let replayed_wt = replayed.get_worktree(worktree.id).unwrap();
    assert_eq!(replayed_wt, wt_retained);
}

#[test]
fn test_reconciliation_lifecycle_full() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    let studio = cp.create_studio("Reconciliation Studio").unwrap();
    let agent = cp
        .register_agent(studio.id, "Coder Agent", AgentKind::Internal, None)
        .unwrap();
    let task = cp
        .create_task(studio.id, "Task", "", None, Some(agent.id), vec![])
        .unwrap();
    let patch = cp
        .register_artifact(
            studio.id,
            task.id,
            agent.id,
            ArtifactKind::Patch,
            "patch.diff",
            None,
            "loc",
        )
        .unwrap();
    let worktree = cp
        .create_worktree(
            studio.id,
            "wt-rec",
            "C:/repos/primary",
            "C:/worktrees/wt-rec",
            "base123",
        )
        .unwrap();

    // 1. Create reconciliation
    let rec = cp
        .create_reconciliation(
            studio.id,
            worktree.id,
            task.id,
            None,
            patch.id,
            "C:/worktrees/integration",
            "base123",
        )
        .unwrap();
    assert_eq!(rec.state, ReconciliationState::Pending);

    // 2. Transition Pending -> Checking
    cp.transition_reconciliation_state(rec.id, ReconciliationState::Checking)
        .unwrap();
    assert_eq!(
        cp.get_reconciliation(rec.id).unwrap().state,
        ReconciliationState::Checking
    );

    // 3. Detect conflict: Checking -> Conflicted
    cp.record_reconciliation_conflict(
        rec.id,
        vec!["src/main.rs".to_string()],
        "Patch failed to apply cleanly at hunk #2",
    )
    .unwrap();
    cp.transition_reconciliation_state(rec.id, ReconciliationState::Conflicted)
        .unwrap();

    let conflicted = cp.get_reconciliation(rec.id).unwrap();
    assert_eq!(conflicted.state, ReconciliationState::Conflicted);
    assert_eq!(conflicted.conflicted_files, vec!["src/main.rs"]);
    assert!(conflicted.state.is_terminal());

    // 4. Replay verifies identical state
    let events = cp.events_for_studio(studio.id, 1).unwrap();
    let replayed =
        ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new()).unwrap();
    assert_eq!(replayed.all_reconciliations().count(), 1);
    let replayed_rec = replayed.get_reconciliation(rec.id).unwrap();
    assert_eq!(replayed_rec, conflicted);
}

#[test]
fn test_reconciliation_applied_success() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    let studio = cp.create_studio("Apply Studio").unwrap();
    let agent = cp
        .register_agent(studio.id, "Coder Agent", AgentKind::Internal, None)
        .unwrap();
    let task = cp
        .create_task(studio.id, "Task", "", None, Some(agent.id), vec![])
        .unwrap();
    let patch = cp
        .register_artifact(
            studio.id,
            task.id,
            agent.id,
            ArtifactKind::Patch,
            "patch.diff",
            None,
            "loc",
        )
        .unwrap();
    let worktree = cp
        .create_worktree(
            studio.id,
            "wt-apply",
            "C:/repos/primary",
            "C:/worktrees/wt-apply",
            "base123",
        )
        .unwrap();

    let rec = cp
        .create_reconciliation(
            studio.id,
            worktree.id,
            task.id,
            None,
            patch.id,
            "C:/worktrees/integration",
            "base123",
        )
        .unwrap();

    cp.transition_reconciliation_state(rec.id, ReconciliationState::Applying)
        .unwrap();
    cp.record_reconciliation_applied(rec.id, Some("merge-sha-999".into()))
        .unwrap();
    cp.transition_reconciliation_state(rec.id, ReconciliationState::Applied)
        .unwrap();

    let applied = cp.get_reconciliation(rec.id).unwrap();
    assert_eq!(applied.state, ReconciliationState::Applied);
    assert_eq!(applied.merge_commit, Some("merge-sha-999".to_string()));
    assert!(applied.completed_at.is_some());
}

#[test]
fn test_artifact_versioning_and_worktree_link() {
    let now = Utc::now();
    let clock = FixedClock::new(now);
    let store = InMemoryStore::new();
    let mut cp = ControlPlane::new(clock, store);

    let studio = cp.create_studio("Version Studio").unwrap();
    let agent = cp
        .register_agent(studio.id, "Coder Agent", AgentKind::Internal, None)
        .unwrap();
    let task = cp
        .create_task(studio.id, "Task", "", None, Some(agent.id), vec![])
        .unwrap();
    let run = cp.create_run(task.id, agent.id).unwrap();
    let worktree = cp
        .create_worktree(
            studio.id,
            "wt-ver",
            "C:/repos/primary",
            "C:/worktrees/wt-ver",
            "base123",
        )
        .unwrap();

    // Artifact v1
    let mut art1 = ArtifactRecord::new(
        studio.id,
        task.id,
        agent.id,
        ArtifactKind::File,
        "src/lib.rs",
        Some("hash-v1".into()),
        "store/v1",
        now,
    );
    art1.run_id = Some(run.id);
    art1.worktree_id = Some(worktree.id);
    art1.version = 1;
    art1.size_bytes = Some(1024);
    let art1 = cp.register_artifact_record(art1).unwrap();

    // Artifact v2 superseding v1
    let mut art2 = ArtifactRecord::new(
        studio.id,
        task.id,
        agent.id,
        ArtifactKind::File,
        "src/lib.rs",
        Some("hash-v2".into()),
        "store/v2",
        now,
    );
    art2.run_id = Some(run.id);
    art2.worktree_id = Some(worktree.id);
    art2.version = 2;
    art2.supersedes = Some(art1.id);
    art2.size_bytes = Some(1150);
    let art2 = cp.register_artifact_record(art2).unwrap();

    assert_eq!(art2.version, 2);
    assert_eq!(art2.supersedes, Some(art1.id));
    assert_eq!(art2.worktree_id, Some(worktree.id));

    // Replay preserves versioning
    let events = cp.events_for_studio(studio.id, 1).unwrap();
    let replayed =
        ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new()).unwrap();
    let r_art2 = replayed.get_artifact(art2.id).unwrap();
    assert_eq!(r_art2.version, 2);
    assert_eq!(r_art2.supersedes, Some(art1.id));
}

#[test]
fn test_replay_corrupted_duplicate_worktree() {
    let (sid, _aid, _tid, now, mut events) = make_base_replay_harness();
    let wid = WorktreeId::new();
    let mut wt1 = WorktreeRecord::new(sid, "wt1", "repo", "wt_path", "base", now);
    wt1.id = wid;
    let mut wt2 = WorktreeRecord::new(sid, "wt2", "repo", "wt_path", "base", now);
    wt2.id = wid;

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::WorktreeCreated { worktree: wt1 },
    ));
    events.push(EventEnvelope::new(
        sid,
        5,
        now,
        ControlPlaneEvent::WorktreeCreated { worktree: wt2 },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(
        matches!(res, Err(ReplayError::DuplicateWorktree { worktree_id }) if worktree_id == wid)
    );
}

#[test]
fn test_replay_corrupted_worktree_invalid_transition() {
    let (sid, _aid, _tid, now, mut events) = make_base_replay_harness();
    let wid = WorktreeId::new();
    let mut wt = WorktreeRecord::new(sid, "wt", "repo", "wt_path", "base", now);
    wt.id = wid;

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::WorktreeCreated { worktree: wt },
    ));

    // Creating -> InUse is invalid!
    events.push(EventEnvelope::new(
        sid,
        5,
        now,
        ControlPlaneEvent::WorktreeStateChanged {
            worktree_id: wid,
            previous_state: WorktreeState::Creating,
            new_state: WorktreeState::InUse,
        },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::InvalidTransition { .. })));
}

#[test]
fn test_replay_corrupted_worktree_thread_rebind_conflict() {
    let (sid, _aid, _tid, now, mut events) = make_base_replay_harness();
    let wid = WorktreeId::new();
    let mut wt = WorktreeRecord::new(sid, "wt", "repo", "wt_path", "base", now);
    wt.id = wid;

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::WorktreeCreated { worktree: wt },
    ));
    events.push(EventEnvelope::new(
        sid,
        5,
        now,
        ControlPlaneEvent::WorktreeThreadBound {
            worktree_id: wid,
            thread_id: "thread-1".into(),
        },
    ));
    events.push(EventEnvelope::new(
        sid,
        6,
        now,
        ControlPlaneEvent::WorktreeThreadBound {
            worktree_id: wid,
            thread_id: "thread-2".into(), // Conflict!
        },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(res, Err(ReplayError::DomainViolation(_))));
}

#[test]
fn test_replay_corrupted_duplicate_reconciliation() {
    let (sid, aid, tid, now, mut events) = make_base_replay_harness();
    let wid = WorktreeId::new();
    let mut wt = WorktreeRecord::new(sid, "wt", "repo", "wt_path", "base", now);
    wt.id = wid;
    let patch = ArtifactRecord::new(
        sid,
        tid,
        aid,
        ArtifactKind::Patch,
        "patch.diff",
        None,
        "loc",
        now,
    );
    let patch_id = patch.id;

    events.push(EventEnvelope::new(
        sid,
        4,
        now,
        ControlPlaneEvent::WorktreeCreated { worktree: wt },
    ));
    events.push(EventEnvelope::new(
        sid,
        5,
        now,
        ControlPlaneEvent::ArtifactRegistered { artifact: patch },
    ));

    let rid = ReconciliationId::new();
    let mut rec1 = ReconciliationRecord::new(sid, wid, tid, None, patch_id, "target", "base", now);
    rec1.id = rid;
    let mut rec2 = ReconciliationRecord::new(sid, wid, tid, None, patch_id, "target", "base", now);
    rec2.id = rid;

    events.push(EventEnvelope::new(
        sid,
        6,
        now,
        ControlPlaneEvent::ReconciliationCreated {
            reconciliation: rec1,
        },
    ));
    events.push(EventEnvelope::new(
        sid,
        7,
        now,
        ControlPlaneEvent::ReconciliationCreated {
            reconciliation: rec2,
        },
    ));

    let res = ControlPlane::replay_events(&events, FixedClock::new(now), InMemoryStore::new());
    assert!(matches!(
        res,
        Err(ReplayError::DuplicateReconciliation { reconciliation_id }) if reconciliation_id == rid
    ));
}
