pub mod agent;
pub mod approval;
pub mod artifact;
pub mod cancellation;
pub mod error;
pub mod event;
pub mod id;
pub mod run;
pub mod studio;
pub mod task;

pub use agent::{AgentDescriptor, AgentKind, AgentState};
pub use approval::{ApprovalKind, ApprovalRequest, ApprovalState};
pub use artifact::{ArtifactKind, ArtifactRecord};
pub use cancellation::{CancellationScope, CancellationSummary};
pub use error::{IdParseError, TransitionError};
pub use event::{ControlPlaneEvent, EventEnvelope};
pub use id::{AgentId, ApprovalId, ArtifactId, EventId, RunId, StudioId, TaskId};
pub use run::{RunRecord, RunState};
pub use studio::Studio;
pub use task::{TaskRecord, TaskState};

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_agent_state_transitions() {
        let registered = AgentState::Registered;
        assert!(registered.can_transition_to(AgentState::Starting));
        assert!(registered.can_transition_to(AgentState::Stopped));
        assert!(registered.can_transition_to(AgentState::Failed));
        assert!(!registered.can_transition_to(AgentState::Busy));
        assert!(!registered.can_transition_to(AgentState::Idle));

        let starting = AgentState::Starting;
        assert!(starting.can_transition_to(AgentState::Idle));
        assert!(starting.can_transition_to(AgentState::Stopping));
        assert!(starting.can_transition_to(AgentState::Failed));
        assert!(!starting.can_transition_to(AgentState::Busy));

        let idle = AgentState::Idle;
        assert!(idle.can_transition_to(AgentState::Busy));
        assert!(idle.can_transition_to(AgentState::Stopping));
        assert!(idle.can_transition_to(AgentState::Failed));
        assert!(!idle.can_transition_to(AgentState::Paused));

        let busy = AgentState::Busy;
        assert!(busy.can_transition_to(AgentState::Idle));
        assert!(busy.can_transition_to(AgentState::Paused));
        assert!(busy.can_transition_to(AgentState::Stopping));
        assert!(busy.can_transition_to(AgentState::Failed));

        let paused = AgentState::Paused;
        assert!(paused.can_transition_to(AgentState::Busy));
        assert!(paused.can_transition_to(AgentState::Idle));
        assert!(paused.can_transition_to(AgentState::Stopping));
        assert!(paused.can_transition_to(AgentState::Failed));

        let stopping = AgentState::Stopping;
        assert!(stopping.can_transition_to(AgentState::Stopped));
        assert!(stopping.can_transition_to(AgentState::Failed));
        assert!(!stopping.can_transition_to(AgentState::Idle));

        let stopped = AgentState::Stopped;
        assert!(stopped.is_terminal());
        assert!(!stopped.can_transition_to(AgentState::Registered));
        assert!(
            stopped
                .validate_transition_to(AgentState::Starting)
                .is_err()
        );

        let failed = AgentState::Failed;
        assert!(failed.is_terminal());
        assert!(!failed.can_transition_to(AgentState::Starting));
        assert!(failed.validate_transition_to(AgentState::Idle).is_err());
    }

    #[test]
    fn test_task_state_transitions() {
        let state = TaskState::Pending;
        assert!(state.can_transition_to(TaskState::Ready));
        assert!(state.can_transition_to(TaskState::Blocked));
        assert!(state.can_transition_to(TaskState::Cancelled));
        assert!(!state.can_transition_to(TaskState::Running));

        assert!(state.validate_transition_to(TaskState::Ready).is_ok());
        assert!(state.validate_transition_to(TaskState::Running).is_err());

        let running = TaskState::Running;
        assert!(running.can_transition_to(TaskState::Succeeded));
        assert!(running.can_transition_to(TaskState::Failed));
        assert!(running.can_transition_to(TaskState::Paused));
        assert!(running.can_transition_to(TaskState::Cancelled));
        assert!(!running.can_transition_to(TaskState::Pending));

        let succeeded = TaskState::Succeeded;
        assert!(succeeded.is_terminal());
        assert!(!succeeded.can_transition_to(TaskState::Running));
        assert!(succeeded.validate_transition_to(TaskState::Ready).is_err());
    }

    #[test]
    fn test_run_state_transitions() {
        let queued = RunState::Queued;
        assert!(queued.can_transition_to(RunState::Starting));
        assert!(queued.can_transition_to(RunState::Cancelled));
        assert!(!queued.can_transition_to(RunState::Succeeded));

        let succeeded = RunState::Succeeded;
        assert!(succeeded.is_terminal());
        assert!(succeeded.validate_transition_to(RunState::Queued).is_err());
    }

    #[test]
    fn test_approval_state_transitions() {
        let pending = ApprovalState::Pending;
        assert!(pending.can_transition_to(ApprovalState::Approved));
        assert!(pending.can_transition_to(ApprovalState::Denied));
        assert!(pending.can_transition_to(ApprovalState::Cancelled));

        let approved = ApprovalState::Approved;
        assert!(approved.is_terminal());
        assert!(
            approved
                .validate_transition_to(ApprovalState::Denied)
                .is_err()
        );
    }

    #[test]
    fn test_serde_roundtrips() {
        let now = Utc::now();
        let studio_id = StudioId::new();
        let task_id = TaskId::new();
        let agent_id = AgentId::new();

        // TaskRecord
        let task = TaskRecord::new(
            studio_id,
            "Build compiler",
            "Compile codex-rs on Windows",
            None,
            Some(agent_id),
            vec![],
            now,
        );
        let task_json = serde_json::to_string(&task).expect("serialize task");
        let task_de: TaskRecord = serde_json::from_str(&task_json).expect("deserialize task");
        assert_eq!(task, task_de);

        // AgentDescriptor
        let agent = AgentDescriptor::new(
            studio_id,
            "Lead Builder",
            AgentKind::Internal,
            Some("coordinator".into()),
        );
        let agent_json = serde_json::to_string(&agent).expect("serialize agent");
        let agent_de: AgentDescriptor =
            serde_json::from_str(&agent_json).expect("deserialize agent");
        assert_eq!(agent, agent_de);

        // ApprovalRequest
        let approval = ApprovalRequest::new(
            studio_id,
            task_id,
            agent_id,
            ApprovalKind::CommandExecution,
            "Run cargo build",
            now,
        );
        let approval_json = serde_json::to_string(&approval).expect("serialize approval");
        let approval_de: ApprovalRequest =
            serde_json::from_str(&approval_json).expect("deserialize approval");
        assert_eq!(approval, approval_de);

        // EventEnvelope
        let envelope = EventEnvelope::new(
            studio_id,
            1,
            now,
            ControlPlaneEvent::StudioCreated {
                studio: Studio::with_id(studio_id, "Agent Studios", now),
            },
        );
        let envelope_json = serde_json::to_string(&envelope).expect("serialize envelope");
        let envelope_de: EventEnvelope =
            serde_json::from_str(&envelope_json).expect("deserialize envelope");
        assert_eq!(envelope, envelope_de);
    }
}
