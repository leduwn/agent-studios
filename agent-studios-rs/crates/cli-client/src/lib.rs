pub mod client;
pub mod embedded;
pub mod events;

pub use client::{
    AgentStudiosClient, ClientApprovalDecision, ClientError, ClientSubscription, DynAgentStudiosClient,
    ExecutionMode, GoalSubmissionOptions, WorktreeRecordSummary,
};
pub use embedded::EmbeddedClient;
pub use events::{NotificationLevel, UiEvent, translate_control_plane_event};
