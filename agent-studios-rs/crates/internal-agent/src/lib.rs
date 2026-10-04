//! Agent Studios Internal Multi-Agent Runtime
//!
//! Provides the foundational orchestration layer for internal multi-agent teams:
//! - Multi-agent teams with independent provider, model, reasoning, budget, and workspace policy
//! - Single-writer async ControlPlane actor
//! - Workspace concurrency arbitration (single-mutator or concurrent read-only)
//! - Per-agent turn, tool call, and duration budget tracking
//! - Structured coordinator plan contract with deterministic DAG validation & cycle rejection
//! - Full supervisor loop with scheduling, failure recovery, and cancellation propagation

pub mod budget;
pub mod control_plane_actor;
pub mod coordinator;
pub mod error;
pub mod executor;
pub mod profile;
pub mod supervisor;
pub mod team;
pub mod workspace_policy;

pub use budget::AgentBudgetTracker;
pub use control_plane_actor::{ControlPlaneActor, ControlPlaneCommand, ControlPlaneHandle};
pub use coordinator::{CoordinatorDecision, CoordinatorPlanValidator, PlannedTask};
pub use error::InternalAgentError;
pub use executor::{
    AgentExecutionContext, AgentExecutionResult, AgentExecutor, CodexAgentExecutor,
    MockAgentExecutor,
};
pub use profile::{
    AgentExecutionBudget, AgentReasoningEffort, AgentReasoningSelection, InternalAgentSpec,
    WorkspaceAccessMode,
};
pub use supervisor::{AgentStudiosSupervisor, FailurePolicy, SupervisorExecutionSummary};
pub use team::{COORDINATOR_ALIAS, InternalTeamSpec, validate_alias};
pub use workspace_policy::{WorkspaceLease, WorkspacePolicyArbitrator};
