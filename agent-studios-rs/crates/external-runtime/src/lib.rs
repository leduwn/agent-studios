//! Vendor-neutral external agent runtime interface and lifecycle contracts for Agent Studios.
//!
//! Defines the provider-independent trait, deterministic lifecycle state machine, typed capability profiles,
//! normalized event model, and thread-safe runtime registry.

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
pub use discovery::{DiscoveredRuntimeInstance, RuntimeAvailability};
pub use error::{RuntimeError, SanitizedRuntimeMessage, sanitize_error_message};
pub use event::{RuntimeEvent, RuntimeEventKind};
pub use handle::{
    EnvironmentBindingSource, EnvironmentVariableBinding, RuntimeCorrelation, RuntimeInstanceRef,
    RuntimeSessionHandle, RuntimeSessionRef, RuntimeStartRequest,
};
pub use id::{RuntimeConfigRef, RuntimeImplementationId, RuntimeInstanceId, RuntimeSessionId};
pub use input::RuntimeInput;
pub use lifecycle::RuntimeLifecycleState;
pub use registry::RuntimeRegistry;
pub use traits::AgentRuntime;
