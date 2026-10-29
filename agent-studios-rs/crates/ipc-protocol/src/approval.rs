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
    Requested,
    Authorized,
    DeliveryPending,
    Delivered,
    Rejected,
    TimedOut,
    DeliveryFailed,
    ExecutionCompleted,
    ExecutionFailed,
    Cancelled,
}

impl ApprovalStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Rejected
                | Self::TimedOut
                | Self::DeliveryFailed
                | Self::ExecutionCompleted
                | Self::ExecutionFailed
                | Self::Cancelled
        )
    }
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
            status: ApprovalStatus::Requested,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    pub fn resolve(&mut self, decision: ApprovalDecision) -> Result<(), ProtocolError> {
        if self.status != ApprovalStatus::Requested {
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
                self.status = ApprovalStatus::Authorized;
            }
            ApprovalDecision::Rejected { .. } => {
                self.status = ApprovalStatus::Rejected;
            }
        }

        Ok(())
    }

    pub fn transition_to(
        &mut self,
        next: ApprovalStatus,
        session_id: &SessionId,
        run_id: &str,
    ) -> Result<(), ProtocolError> {
        if session_id != &self.session_id {
            return Err(ProtocolError::CorrelationMismatch {
                expected: self.session_id.to_string(),
                found: session_id.to_string(),
            });
        }

        if run_id != self.run_id {
            return Err(ProtocolError::CorrelationMismatch {
                expected: self.run_id.clone(),
                found: run_id.to_string(),
            });
        }

        if self.status.is_terminal() {
            return Err(ProtocolError::InvalidState(format!(
                "Cannot transition from terminal state {:?}",
                self.status
            )));
        }

        if self.is_expired() {
            self.status = ApprovalStatus::TimedOut;
            return Err(ProtocolError::ApprovalError("Approval has expired".into()));
        }

        let is_valid = matches!(
            (self.status, next),
            (
                ApprovalStatus::Requested,
                ApprovalStatus::Authorized
                    | ApprovalStatus::Rejected
                    | ApprovalStatus::TimedOut
                    | ApprovalStatus::Cancelled,
            ) | (
                ApprovalStatus::Authorized,
                ApprovalStatus::DeliveryPending | ApprovalStatus::Cancelled,
            ) | (
                ApprovalStatus::DeliveryPending,
                ApprovalStatus::Delivered
                    | ApprovalStatus::DeliveryFailed
                    | ApprovalStatus::Cancelled,
            ) | (
                ApprovalStatus::Delivered,
                ApprovalStatus::ExecutionCompleted | ApprovalStatus::ExecutionFailed,
            )
        );

        if !is_valid {
            return Err(ProtocolError::InvalidState(format!(
                "Invalid state transition from {:?} to {:?}",
                self.status, next
            )));
        }

        self.status = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::thread;

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

        assert_eq!(contract.status, ApprovalStatus::Requested);
        assert!(!contract.is_expired());

        contract
            .resolve(ApprovalDecision::Approved {
                modified_details: None,
                user_note: Some("Looks good".to_string()),
            })
            .expect("resolve must succeed");

        assert_eq!(contract.status, ApprovalStatus::Authorized);

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

    #[test]
    fn test_happy_path_lifecycle() {
        let session_id = SessionId::generate();
        let run_id = "run-001".to_string();

        let mut contract = ApprovalContract::new(
            session_id.clone(),
            "task-lifecycle".to_string(),
            run_id.clone(),
            RiskLevel::Medium,
            ApprovalAction::CommandExecution {
                command: "cargo check".to_string(),
                cwd: "/repo".to_string(),
            },
            "Execute cargo check".to_string(),
            300,
        );

        // Requested -> Authorized
        assert_eq!(contract.status, ApprovalStatus::Requested);
        contract
            .transition_to(ApprovalStatus::Authorized, &session_id, &run_id)
            .expect("Requested -> Authorized must succeed");
        assert_eq!(contract.status, ApprovalStatus::Authorized);

        // Authorized -> DeliveryPending
        contract
            .transition_to(ApprovalStatus::DeliveryPending, &session_id, &run_id)
            .expect("Authorized -> DeliveryPending must succeed");
        assert_eq!(contract.status, ApprovalStatus::DeliveryPending);

        // DeliveryPending -> Delivered
        contract
            .transition_to(ApprovalStatus::Delivered, &session_id, &run_id)
            .expect("DeliveryPending -> Delivered must succeed");
        assert_eq!(contract.status, ApprovalStatus::Delivered);

        // Delivered -> ExecutionCompleted
        contract
            .transition_to(ApprovalStatus::ExecutionCompleted, &session_id, &run_id)
            .expect("Delivered -> ExecutionCompleted must succeed");
        assert_eq!(contract.status, ApprovalStatus::ExecutionCompleted);
        assert!(contract.status.is_terminal());
    }

    #[test]
    fn test_correlation_failures_fail_closed() {
        let session_id = SessionId::generate();
        let wrong_session_id = SessionId::generate();
        let run_id = "run-corr".to_string();

        let mut contract = ApprovalContract::new(
            session_id.clone(),
            "task-corr".to_string(),
            run_id.clone(),
            RiskLevel::Low,
            ApprovalAction::Custom {
                name: "custom_op".to_string(),
                details: serde_json::json!({ "arg": 1 }),
            },
            "Custom operation".to_string(),
            120,
        );

        // Wrong session ID
        let err_sess =
            contract.transition_to(ApprovalStatus::Authorized, &wrong_session_id, &run_id);
        match err_sess {
            Err(ProtocolError::CorrelationMismatch { expected, found }) => {
                assert_eq!(expected, session_id.to_string());
                assert_eq!(found, wrong_session_id.to_string());
            }
            other => panic!(
                "Expected CorrelationMismatch for session_id, got {:?}",
                other
            ),
        }
        assert_eq!(contract.status, ApprovalStatus::Requested);

        // Wrong run ID
        let err_run =
            contract.transition_to(ApprovalStatus::Authorized, &session_id, "wrong-run-id");
        match err_run {
            Err(ProtocolError::CorrelationMismatch { expected, found }) => {
                assert_eq!(expected, run_id);
                assert_eq!(found, "wrong-run-id");
            }
            other => panic!("Expected CorrelationMismatch for run_id, got {:?}", other),
        }
        assert_eq!(contract.status, ApprovalStatus::Requested);
    }

    #[test]
    fn test_transition_expiry_fails_closed() {
        let session_id = SessionId::generate();
        let run_id = "run-expire".to_string();

        let mut contract = ApprovalContract::new(
            session_id.clone(),
            "task-exp".to_string(),
            run_id.clone(),
            RiskLevel::High,
            ApprovalAction::FileModification {
                path: "src/main.rs".to_string(),
                diff: "+ // change".to_string(),
            },
            "Edit main.rs".to_string(),
            -5, // expired in past
        );

        assert!(contract.is_expired());
        let err = contract.transition_to(ApprovalStatus::Authorized, &session_id, &run_id);
        match err {
            Err(ProtocolError::ApprovalError(msg)) => {
                assert!(msg.contains("expired"));
            }
            other => panic!(
                "Expected ApprovalError for expired transition, got {:?}",
                other
            ),
        }
        assert_eq!(contract.status, ApprovalStatus::TimedOut);

        // Further transition rejected as terminal
        let err_after = contract.transition_to(ApprovalStatus::Authorized, &session_id, &run_id);
        assert!(matches!(err_after, Err(ProtocolError::InvalidState(_))));
    }

    #[test]
    fn test_illegal_state_transitions() {
        let session_id = SessionId::generate();
        let run_id = "run-illegal".to_string();

        // 1. Requested -> Delivered (skipping Authorized / DeliveryPending)
        let mut contract = ApprovalContract::new(
            session_id.clone(),
            "task-ill".to_string(),
            run_id.clone(),
            RiskLevel::High,
            ApprovalAction::WorkspaceOperation {
                operation: "delete".to_string(),
                target: "/tmp/data".to_string(),
            },
            "Delete temp data".to_string(),
            300,
        );
        let err_skip = contract.transition_to(ApprovalStatus::Delivered, &session_id, &run_id);
        assert!(matches!(err_skip, Err(ProtocolError::InvalidState(_))));

        // 2. Advance to Delivered
        contract
            .transition_to(ApprovalStatus::Authorized, &session_id, &run_id)
            .unwrap();
        contract
            .transition_to(ApprovalStatus::DeliveryPending, &session_id, &run_id)
            .unwrap();
        contract
            .transition_to(ApprovalStatus::Delivered, &session_id, &run_id)
            .unwrap();

        // 3. Delivered -> Authorized (backwards transition rejected)
        let err_back = contract.transition_to(ApprovalStatus::Authorized, &session_id, &run_id);
        assert!(matches!(err_back, Err(ProtocolError::InvalidState(_))));

        // 4. Delivered -> ExecutionCompleted
        contract
            .transition_to(ApprovalStatus::ExecutionCompleted, &session_id, &run_id)
            .unwrap();

        // 5. ExecutionCompleted -> Authorized (terminal state cannot transition)
        let err_term = contract.transition_to(ApprovalStatus::Authorized, &session_id, &run_id);
        assert!(matches!(err_term, Err(ProtocolError::InvalidState(_))));

        // 6. Test other terminal states reject transitions
        for terminal_state in [
            ApprovalStatus::Rejected,
            ApprovalStatus::TimedOut,
            ApprovalStatus::DeliveryFailed,
            ApprovalStatus::ExecutionFailed,
            ApprovalStatus::Cancelled,
        ] {
            let mut c = ApprovalContract::new(
                session_id.clone(),
                "task-term".to_string(),
                run_id.clone(),
                RiskLevel::Low,
                ApprovalAction::Custom {
                    name: "test".to_string(),
                    details: serde_json::Value::Null,
                },
                "Terminal state test".to_string(),
                300,
            );
            c.status = terminal_state;
            let err = c.transition_to(ApprovalStatus::Authorized, &session_id, &run_id);
            assert!(
                matches!(err, Err(ProtocolError::InvalidState(_))),
                "Terminal state {:?} must reject transitions",
                terminal_state
            );
        }
    }

    #[test]
    fn test_concurrent_state_transitions() {
        let session_id = SessionId::generate();
        let run_id = "run-concurrency".to_string();

        let contract = Arc::new(Mutex::new(ApprovalContract::new(
            session_id.clone(),
            "task-conc".to_string(),
            run_id.clone(),
            RiskLevel::High,
            ApprovalAction::CommandExecution {
                command: "cargo build".to_string(),
                cwd: "/repo".to_string(),
            },
            "Concurrent build".to_string(),
            60,
        )));

        let mut handles = Vec::new();
        for _ in 0..10 {
            let contract_clone = Arc::clone(&contract);
            let run_id_clone = run_id.clone();
            let session_clone = session_id.clone();
            handles.push(thread::spawn(move || {
                let mut c = contract_clone.lock().unwrap();
                c.transition_to(ApprovalStatus::Authorized, &session_clone, &run_id_clone)
            }));
        }

        let mut ok_count = 0;
        let mut err_count = 0;
        for handle in handles {
            let res = handle.join().expect("thread join must succeed");
            match res {
                Ok(()) => ok_count += 1,
                Err(ProtocolError::InvalidState(_)) => err_count += 1,
                Err(other) => panic!("Unexpected error: {:?}", other),
            }
        }

        assert_eq!(
            ok_count, 1,
            "Exactly one thread must successfully authorize"
        );
        assert_eq!(
            err_count, 9,
            "All other concurrent attempts must be rejected"
        );
        assert_eq!(contract.lock().unwrap().status, ApprovalStatus::Authorized);
    }
}
