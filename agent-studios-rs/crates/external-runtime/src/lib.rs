//! Vendor-neutral external agent runtime interface and lifecycle contracts for Agent Studios.
//!
//! Defines the provider-independent trait, deterministic lifecycle state machine, typed capability profiles,
//! normalized event model, session event hub with replay history, non-secret value validation,
//! strict start validation, and thread-safe runtime registry.
//!
//! # Core Subsystems
//!
//! - [`traits::AgentRuntime`]: Core lifecycle trait for external coding agents.
//! - [`discovery::validate_instance_start`]: Authoritative pre-spawn validation of instances, configurations, and workspaces.
//! - [`event::SessionEventHub`]: Monotonic sequence generation, bounded replay history, and broadcast fanout.
//! - [`event::EventBoundaryValidator`]: Ingestion stream invariant enforcement.
//! - [`handle::NonSecretValue`]: Credential-rejecting wrapper for non-secret environment and metadata values.
//! - [`registry::RuntimeRegistry`]: Fault-tolerant concurrent instance discovery and capability inspection.

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
