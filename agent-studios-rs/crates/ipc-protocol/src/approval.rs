use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::ProtocolError;
use crate::id::{ApprovalId, SessionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action_type")]
pub enum ApprovalAction {
    CommandExecution {
        command: String,
        cwd: String,
    },
    FileModification {
        path: String,
        diff: String,
    },
    NetworkRequest {
        url: String,
        method: String,
    },
    WorkspaceOperation {
        operation: String,
        target: String,
    },
    Custom {
        name: String,
        details: serde_json::Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    TimedOut,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalContract {
    pub approval_id: ApprovalId,
    pub session_id: SessionId,
    pub task_id: String,
    pub run_id: String,
    pub risk_level: RiskLevel,
    pub action: ApprovalAction,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: ApprovalStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum ApprovalDecision {
    Approved {
        #[serde(skip_serializing_if = "Option::is_none")]
        modified_details: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_note: Option<String>,
    },
    Rejected {
        reason: String,
    },
}

impl ApprovalContract {
    pub fn new(
        session_id: SessionId,
        task_id: String,
        run_id: String,
        risk_level: RiskLevel,
        action: ApprovalAction,
        description: String,
        timeout_seconds: i64,
    ) -> Self {
        let now = Utc::now();
        let expires_at = now + Duration::seconds(timeout_seconds);

        Self {
            approval_id: ApprovalId::generate(),
            session_id,
            task_id,
            run_id,
            risk_level,
            action,
            description,
            created_at: now,
            expires_at,
            status: ApprovalStatus::Pending,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    pub fn resolve(&mut self, decision: ApprovalDecision) -> Result<(), ProtocolError> {
        if self.status != ApprovalStatus::Pending {
            return Err(ProtocolError::ApprovalError(format!(
                "Cannot resolve approval {}: current status is {:?}",
                self.approval_id, self.status
            )));
        }

        if self.is_expired() {
            self.status = ApprovalStatus::TimedOut;
            return Err(ProtocolError::ApprovalError(format!(
                "Approval {} has expired at {}",
                self.approval_id, self.expires_at
            )));
        }

        match decision {
            ApprovalDecision::Approved { .. } => {
                self.status = ApprovalStatus::Approved;
            }
            ApprovalDecision::Rejected { .. } => {
                self.status = ApprovalStatus::Rejected;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_creation_and_resolution() {
        let mut contract = ApprovalContract::new(
            SessionId::generate(),
            "task-01".to_string(),
            "run-01".to_string(),
            RiskLevel::High,
            ApprovalAction::CommandExecution {
                command: "rm -rf target".to_string(),
                cwd: "/repo".to_string(),
            },
            "Clean target build artifacts".to_string(),
            60,
        );

        assert_eq!(contract.status, ApprovalStatus::Pending);
        assert!(!contract.is_expired());

        contract
            .resolve(ApprovalDecision::Approved {
                modified_details: None,
                user_note: Some("Looks good".to_string()),
            })
            .expect("resolve must succeed");

        assert_eq!(contract.status, ApprovalStatus::Approved);

        // Duplicate resolution fails closed
        let dup_err = contract.resolve(ApprovalDecision::Rejected {
            reason: "late reject".to_string(),
        });
        assert!(dup_err.is_err());
    }

    #[test]
    fn test_approval_expiry_fails_closed() {
        let mut contract = ApprovalContract::new(
            SessionId::generate(),
            "task-01".to_string(),
            "run-01".to_string(),
            RiskLevel::Critical,
            ApprovalAction::NetworkRequest {
                url: "https://evil.corp".to_string(),
                method: "POST".to_string(),
            },
            "Post telemetry".to_string(),
            -10, // already expired in past
        );

        assert!(contract.is_expired());
        let err = contract.resolve(ApprovalDecision::Approved {
            modified_details: None,
            user_note: None,
        });
        assert!(err.is_err());
        assert_eq!(contract.status, ApprovalStatus::TimedOut);
    }
}
