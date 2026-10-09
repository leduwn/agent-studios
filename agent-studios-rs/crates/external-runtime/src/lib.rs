//! Vendor-neutral external agent runtime interface and lifecycle contracts for Agent Studios.
//!
//! Defines the provider-independent trait, deterministic lifecycle state machine, typed capability profiles,
//! normalized event model, session event hub with replay history, non-secret value validation,
//! strict start validation, and thread-safe runtime registry.
//!
//! All contracts are verified by a comprehensive 56-test contract and regression test suite
//! covering atomic event serialization, boundary validation, credential sanitization,
//! lifecycle error precedence, and fail-closed workspace containment.
//!
//! # Workspace Policy and Process Sandboxing Scope
//!
//! Milestone M10 enforces typed policy boundaries at the API and request layer (for example,
//! strictly rejecting [`ExecutionWorkspace::SharedSource`] workspaces across all access modes
//! and forbidding plaintext credentials in configuration). M10 does **not** provide kernel-level
//! or OS-level process sandboxing (cgroups, namespaces, seccomp, AppArmor, or Windows Job Objects).
//! OS-level runtime sandboxing and process containment are deferred to Milestone M11.
//!
//! # Core Subsystems
//!
//! - [`traits::AgentRuntime`]: Core lifecycle trait for external coding agents.
//! - [`discovery::validate_instance_start`]: Authoritative pre-spawn validation of instances, configurations, and workspaces.
//! - [`event::SessionEventHub`]: Atomic monotonic sequence generation, bounded replay history, and broadcast fanout.
//! - [`event::EventBoundaryValidator`]: Ingestion stream invariant enforcement and post-terminal rejection.
//! - [`event::RuntimeEventSubscription`]: Gap- and lag-detecting replay subscription with sequence deduplication.
//! - [`handle::NonSecretValue`]: Credential-rejecting wrapper for non-secret environment and metadata values.
//! - [`handle::EnvironmentVariableBinding`]: Strongly-typed secret and literal environment bindings with invariant enforcement.
//! - [`error::sanitize_error_message`]: Multi-URL and query parameter credential scrubbing for diagnostics.
//! - [`registry::RuntimeRegistry`]: Fault-tolerant concurrent instance discovery with worker panic attribution.

pub mod capabilities;
pub mod discovery;
pub mod error;
pub mod event;
pub mod handle;
pub mod id;
pub mod input;
pub mod lifecycle;
pub mod registry;
pub mod traits;

pub use capabilities::{RuntimeCapabilities, RuntimeCapability, RuntimeCapabilitySupport};
pub use discovery::{DiscoveredRuntimeInstance, RuntimeAvailability, validate_instance_start};
pub use error::{RuntimeError, SanitizedRuntimeMessage, sanitize_error_message};
pub use event::{
    DEFAULT_EVENT_RETENTION, EventBoundaryValidator, EventSequencer, RuntimeEvent,
    RuntimeEventKind, RuntimeEventSubscription, SessionEventHub,
};
pub use handle::{
    EnvironmentBindingSource, EnvironmentVariableBinding, NonSecretValue, RuntimeCorrelation,
    RuntimeInstanceRef, RuntimeSessionHandle, RuntimeSessionRef, RuntimeStartRequest,
    WorkspaceAccessMode,
};
pub use id::{RuntimeConfigRef, RuntimeImplementationId, RuntimeInstanceId, RuntimeSessionId};
pub use input::RuntimeInput;
pub use lifecycle::RuntimeLifecycleState;
pub use registry::{RegistryDiscoveryOutcome, RuntimeRegistry};
pub use traits::AgentRuntime;
