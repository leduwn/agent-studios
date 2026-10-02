use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agent::{AgentDescriptor, AgentState};
use crate::approval::{ApprovalRequest, ApprovalState};
use crate::artifact::ArtifactRecord;
use crate::cancellation::CancellationScope;
use crate::id::{AgentId, ApprovalId, EventId, RunId, StudioId, TaskId};
use crate::run::{RunRecord, RunState};
use crate::studio::Studio;
use crate::task::{TaskRecord, TaskState};

/// Canonical domain event variants emitted by the Agent Studios control plane.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ControlPlaneEvent {
    StudioCreated {
        studio: Studio,
    },
    AgentRegistered {
        agent: AgentDescriptor,
    },
    AgentStateChanged {
        agent_id: AgentId,
        previous_state: AgentState,
        new_state: AgentState,
    },
    TaskCreated {
        task: TaskRecord,
    },
    TaskStateChanged {
        task_id: TaskId,
        previous_state: TaskState,
        new_state: TaskState,
    },
    TaskDependencyAdded {
        task_id: TaskId,
        dependency_id: TaskId,
    },
    TaskDependencyRemoved {
        task_id: TaskId,
        dependency_id: TaskId,
    },
    RunCreated {
        run: RunRecord,
    },
    RunStateChanged {
        run_id: RunId,
        previous_state: RunState,
        new_state: RunState,
    },
    ApprovalRequested {
        approval: ApprovalRequest,
    },
    ApprovalResolved {
        approval_id: ApprovalId,
        previous_state: ApprovalState,
        new_state: ApprovalState,
    },
    ArtifactRegistered {
        artifact: ArtifactRecord,
    },
    CancellationRequested {
        scope: CancellationScope,
        reason: Option<String>,
    },
}

/// Durable sequence-stamped envelope containing an event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_id: EventId,
    pub studio_id: StudioId,
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub event: ControlPlaneEvent,
}

impl EventEnvelope {
    pub fn new(
        studio_id: StudioId,
        sequence: u64,
        timestamp: DateTime<Utc>,
        event: ControlPlaneEvent,
    ) -> Self {
        Self {
            event_id: EventId::new(),
            studio_id,
            sequence,
            timestamp,
            event,
        }
    }
}
