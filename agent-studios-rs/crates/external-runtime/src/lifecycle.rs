use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::RuntimeError;

/// Deterministic lifecycle states for an external agent runtime session.
/// Enforces non-conflated states for Interrupted, Stopped, Completed, and Failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeLifecycleState {
    /// The runtime process/session is initializing.
    Starting,
    /// The runtime process/session is active and ready for turns/inputs.
    Running,
    /// An active operation/turn was interrupted, but session state is preserved.
    Interrupted,
    /// The runtime process/session was explicitly terminated (terminal).
    Stopped,
    /// The runtime process/session finished its task to completion (terminal).
    Completed,
    /// The runtime encountered an unrecoverable failure (terminal).
    Failed,
}

impl RuntimeLifecycleState {
    /// Returns true if the session is currently operational and can receive commands.
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Starting | Self::Running | Self::Interrupted)
    }

    /// Returns true if the session has reached an irreversible terminal state.
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Stopped | Self::Completed | Self::Failed)
    }

    /// Evaluates whether a transition from `self` to `target` is valid according to the deterministic state machine.
    pub const fn can_transition_to(&self, target: Self) -> bool {
        if self.is_terminal() {
            return false;
        }

        matches!(
            (self, target),
            // Starting can transition to Running, Interrupted, Stopped, or Failed
            (
                Self::Starting,
                Self::Running | Self::Interrupted | Self::Stopped | Self::Failed
            )
            // Running can transition to Interrupted, Completed, Stopped, or Failed
            | (
                Self::Running,
                Self::Interrupted | Self::Completed | Self::Stopped | Self::Failed
            )
            // Interrupted can transition to Running (resume/continue), Stopped, Completed, or Failed
            | (
                Self::Interrupted,
                Self::Running | Self::Stopped | Self::Completed | Self::Failed
            )
        )
    }

    /// Validates state transition, returning a typed `RuntimeError::InvalidLifecycleTransition` on failure.
    pub fn validate_transition_to(&self, target: Self) -> Result<(), RuntimeError> {
        if self.is_terminal() {
            return Err(RuntimeError::InvalidLifecycleTransition {
                from: *self,
                to: target,
                reason: format!(
                    "Cannot transition from terminal state '{}' to '{}'",
                    self, target
                ),
            });
        }

        if self.can_transition_to(target) {
            Ok(())
        } else {
            Err(RuntimeError::InvalidLifecycleTransition {
                from: *self,
                to: target,
                reason: format!(
                    "Illegal lifecycle transition from '{}' to '{}'",
                    self, target
                ),
            })
        }
    }
}

impl fmt::Display for RuntimeLifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Starting => write!(f, "starting"),
            Self::Running => write!(f, "running"),
            Self::Interrupted => write!(f, "interrupted"),
            Self::Stopped => write!(f, "stopped"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_terminal_invariants() {
        assert!(RuntimeLifecycleState::Stopped.is_terminal());
        assert!(RuntimeLifecycleState::Completed.is_terminal());
        assert!(RuntimeLifecycleState::Failed.is_terminal());
        assert!(!RuntimeLifecycleState::Interrupted.is_terminal());
        assert!(!RuntimeLifecycleState::Running.is_terminal());
        assert!(!RuntimeLifecycleState::Starting.is_terminal());
    }

    #[test]
    fn test_lifecycle_transitions() {
        let starting = RuntimeLifecycleState::Starting;
        assert!(starting.can_transition_to(RuntimeLifecycleState::Running));
        assert!(starting.can_transition_to(RuntimeLifecycleState::Stopped));
        assert!(starting.can_transition_to(RuntimeLifecycleState::Failed));
        assert!(!starting.can_transition_to(RuntimeLifecycleState::Completed));

        let running = RuntimeLifecycleState::Running;
        assert!(running.can_transition_to(RuntimeLifecycleState::Interrupted));
        assert!(running.can_transition_to(RuntimeLifecycleState::Completed));
        assert!(running.can_transition_to(RuntimeLifecycleState::Stopped));
        assert!(running.can_transition_to(RuntimeLifecycleState::Failed));
        assert!(!running.can_transition_to(RuntimeLifecycleState::Starting));

        let interrupted = RuntimeLifecycleState::Interrupted;
        assert!(interrupted.can_transition_to(RuntimeLifecycleState::Running));
        assert!(interrupted.can_transition_to(RuntimeLifecycleState::Stopped));
        assert!(interrupted.can_transition_to(RuntimeLifecycleState::Failed));

        let stopped = RuntimeLifecycleState::Stopped;
        assert!(!stopped.can_transition_to(RuntimeLifecycleState::Running));
        assert!(
            stopped
                .validate_transition_to(RuntimeLifecycleState::Running)
                .is_err()
        );
    }
}
