//! UI Event Translation and Sequence Tracking for Ratatui Event Loop

use agent_studios_protocol::agent::AgentState;
use agent_studios_protocol::approval::ApprovalState;
use agent_studios_protocol::event::{ControlPlaneEvent, EventEnvelope};
use agent_studios_protocol::id::{
    AgentId, ApprovalId, ReconciliationId, StudioId, TaskId, WorktreeId,
};
use agent_studios_protocol::reconciliation::ReconciliationState;
use agent_studios_protocol::task::TaskState;
use agent_studios_protocol::worktree::WorktreeState;

/// Normalized UI event dispatched directly to the TUI event channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    StudioInitialized {
        studio_id: StudioId,
    },
    AgentUpdated {
        agent_id: AgentId,
        role: String,
        state: AgentState,
    },
    TaskUpdated {
        task_id: TaskId,
        title: String,
        state: TaskState,
    },
    ApprovalRequested {
        approval_id: ApprovalId,
        task_id: TaskId,
        agent_id: AgentId,
        risk_tier: String,
        kind: String,
        summary: String,
    },
    ApprovalResolved {
        approval_id: ApprovalId,
        state: ApprovalState,
    },
    WorktreeUpdated {
        worktree_id: WorktreeId,
        state: WorktreeState,
    },
    ReconciliationUpdated {
        reconciliation_id: ReconciliationId,
        state: ReconciliationState,
    },
    AssistantChunk {
        content: String,
        is_thought: bool,
    },
    ToolProgress {
        tool_name: String,
        status: String,
    },
    Notification {
        level: NotificationLevel,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
    Success,
}

/// Translates a raw Control Plane event envelope into a UI event if relevant.
pub fn translate_control_plane_event(env: &EventEnvelope) -> Option<UiEvent> {
    match &env.event {
        ControlPlaneEvent::StudioCreated { studio } => Some(UiEvent::StudioInitialized {
            studio_id: studio.id,
        }),
        ControlPlaneEvent::AgentRegistered { agent } => Some(UiEvent::AgentUpdated {
            agent_id: agent.id,
            role: agent
                .role
                .clone()
                .unwrap_or_else(|| format!("{:?}", agent.kind)),
            state: AgentState::Registered,
        }),
        ControlPlaneEvent::AgentStateChanged {
            agent_id,
            new_state,
            ..
        } => Some(UiEvent::AgentUpdated {
            agent_id: *agent_id,
            role: "Agent".to_string(),
            state: *new_state,
        }),
        ControlPlaneEvent::TaskCreated { task } => Some(UiEvent::TaskUpdated {
            task_id: task.id,
            title: task.title.clone(),
            state: task.state,
        }),
        ControlPlaneEvent::TaskStateChanged {
            task_id, new_state, ..
        } => Some(UiEvent::TaskUpdated {
            task_id: *task_id,
            title: String::new(),
            state: *new_state,
        }),
        ControlPlaneEvent::ApprovalRequested { approval } => Some(UiEvent::ApprovalRequested {
            approval_id: approval.id,
            task_id: approval.task_id,
            agent_id: approval.agent_id,
            risk_tier: format!("{:?}", approval.kind),
            kind: format!("{:?}", approval.kind),
            summary: approval.summary.clone(),
        }),
        ControlPlaneEvent::ApprovalResolved {
            approval_id,
            new_state,
            ..
        } => Some(UiEvent::ApprovalResolved {
            approval_id: *approval_id,
            state: *new_state,
        }),
        ControlPlaneEvent::WorktreeStateChanged {
            worktree_id,
            new_state,
            ..
        } => Some(UiEvent::WorktreeUpdated {
            worktree_id: *worktree_id,
            state: *new_state,
        }),
        ControlPlaneEvent::ReconciliationStateChanged {
            reconciliation_id,
            new_state,
            ..
        } => Some(UiEvent::ReconciliationUpdated {
            reconciliation_id: *reconciliation_id,
            state: *new_state,
        }),
        ControlPlaneEvent::AssistantMessageChunk {
            content,
            is_thought,
            ..
        } => Some(UiEvent::AssistantChunk {
            content: content.clone(),
            is_thought: *is_thought,
        }),
        ControlPlaneEvent::ToolStarted { tool_name, .. } => Some(UiEvent::ToolProgress {
            tool_name: tool_name.clone(),
            status: "running".to_string(),
        }),
        ControlPlaneEvent::ToolCompleted { tool_name, .. } => Some(UiEvent::ToolProgress {
            tool_name: tool_name.clone(),
            status: "completed".to_string(),
        }),
        ControlPlaneEvent::ToolFailed {
            tool_name, error, ..
        } => Some(UiEvent::ToolProgress {
            tool_name: tool_name.clone(),
            status: format!("failed: {error}"),
        }),
        ControlPlaneEvent::CancellationRequested { reason, .. } => Some(UiEvent::Notification {
            level: NotificationLevel::Warning,
            message: reason
                .clone()
                .unwrap_or_else(|| "Cancellation requested".to_string()),
        }),
        _ => None,
    }
}
