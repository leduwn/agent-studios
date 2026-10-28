use serde::{Deserialize, Serialize};

use crate::capabilities::CapabilityNegotiationRequest;
use crate::id::{ApprovalId, SubscriptionId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "payload")]
pub enum IpcCommand {
    Ping,
    Pong,
    Negotiate(CapabilityNegotiationRequest),
    SubmitGoal {
        goal: String,
        access_mode: String,
        workspace_root: String,
        options: serde_json::Value,
    },
    CancelTask {
        task_id: String,
        reason: Option<String>,
    },
    ResolveApproval {
        approval_id: ApprovalId,
        decision: String,
        details: Option<serde_json::Value>,
    },
    SubscribeEvents {
        from_sequence: Option<u64>,
    },
    UnsubscribeEvents {
        subscription_id: SubscriptionId,
    },
    ReplayEvents {
        from_sequence: u64,
        limit: Option<usize>,
    },
    Custom {
        action: String,
        payload: serde_json::Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    VersionMismatch,
    CapabilityMissing,
    InternalError,
    Timeout,
    ApprovalExpired,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcErrorResponse {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    pub retryable: bool,
}

impl IpcErrorResponse {
    pub fn new(code: ErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
            retryable,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum IpcResponse {
    Success {
        result: serde_json::Value,
    },
    Error {
        #[serde(flatten)]
        error: IpcErrorResponse,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_command_serde() {
        let cmd = IpcCommand::Ping;
        let json = serde_json::to_string(&cmd).expect("serialize");
        let decoded: IpcCommand = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cmd, decoded);

        let submit = IpcCommand::SubmitGoal {
            goal: "refactor module".to_string(),
            access_mode: "Mutating".to_string(),
            workspace_root: "/repo".to_string(),
            options: serde_json::json!({ "auto_commit": false }),
        };
        let json = serde_json::to_string(&submit).expect("serialize");
        let decoded: IpcCommand = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(submit, decoded);
    }

    #[test]
    fn test_ipc_response_serde() {
        let success = IpcResponse::Success {
            result: serde_json::json!({ "status": "ok", "task_id": "task_1" }),
        };
        let json = serde_json::to_string(&success).expect("serialize");
        let decoded: IpcResponse = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(success, decoded);

        let err = IpcResponse::Error {
            error: IpcErrorResponse::new(ErrorCode::Unauthorized, "Missing auth token", false),
        };
        let json = serde_json::to_string(&err).expect("serialize");
        let decoded: IpcResponse = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(err, decoded);
    }
}
