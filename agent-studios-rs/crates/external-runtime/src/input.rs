use agent_studios_protocol::id::ApprovalId;
use serde::{Deserialize, Serialize};

/// Strongly-typed input submitted to an active external agent runtime session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeInput {
    /// Standard natural language user or prompt text.
    Text { content: String },
    /// Continuation turn or control resumption trigger with optional structured context.
    Continuation {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<String>,
    },
    /// Response to an interactive human-in-the-loop approval request.
    ApprovalResponse {
        approval_id: ApprovalId,
        approved: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

impl RuntimeInput {
    pub fn text(content: impl Into<String>) -> Self {
        Self::Text {
            content: content.into(),
        }
    }

    pub fn continuation(context: Option<String>) -> Self {
        Self::Continuation { context }
    }

    pub fn approval_response(
        approval_id: ApprovalId,
        approved: bool,
        reason: Option<String>,
    ) -> Self {
        Self::ApprovalResponse {
            approval_id,
            approved,
            reason,
        }
    }
}
