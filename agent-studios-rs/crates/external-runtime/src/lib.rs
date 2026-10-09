//! Vendor-neutral external agent runtime interface and lifecycle contracts for Agent Studios.
//!
//! Defines the provider-independent trait, deterministic lifecycle state machine, typed capability profiles,
//! normalized event model, thread-safe runtime registry, and in-process fake runtime for testing.

pub mod capabilities;
pub mod discovery;
pub mod error;
pub mod event;
pub mod fake;
pub mod handle;
pub mod id;
pub mod input;
pub mod lifecycle;
pub mod registry;
pub mod traits;

pub use capabilities::{RuntimeCapabilities, RuntimeCapability, RuntimeCapabilitySupport};
pub use discovery::DiscoveredRuntime;
pub use error::{RuntimeError, sanitize_error_message};
pub use event::{RuntimeEvent, RuntimeEventKind};
pub use fake::{FakeAgentRuntime, FakeCallRecord};
pub use handle::{RuntimeCorrelation, RuntimeSessionHandle, RuntimeStartRequest};
pub use id::{RuntimeImplementationId, RuntimeInstanceId, RuntimeSessionId};
pub use input::RuntimeInput;
pub use lifecycle::RuntimeLifecycleState;
pub use registry::RuntimeRegistry;
pub use traits::AgentRuntime;
