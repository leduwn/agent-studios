use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::capabilities::RuntimeCapabilities;
use crate::discovery::DiscoveredRuntimeInstance;
use crate::error::RuntimeError;
use crate::event::RuntimeEvent;
use crate::handle::{RuntimeSessionHandle, RuntimeSessionRef, RuntimeStartRequest};
use crate::id::RuntimeImplementationId;
use crate::input::RuntimeInput;
use crate::lifecycle::RuntimeLifecycleState;

/// Vendor-neutral contract that external agent runtimes must implement to integrate with Agent Studios.
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    /// Returns the unique implementation identifier for this runtime.
    fn implementation_id(&self) -> &RuntimeImplementationId;

    /// Discovers all available or configured runtime instances in the local environment.
    async fn discover(&self) -> Result<Vec<DiscoveredRuntimeInstance>, RuntimeError>;

    /// Advertises the static or dynamic capability profile supported by this runtime.
    async fn capabilities(&self) -> RuntimeCapabilities;

    /// Starts a new external runtime process or session according to the start request.
    async fn start(
        &self,
        request: RuntimeStartRequest,
    ) -> Result<RuntimeSessionHandle, RuntimeError>;

    /// Sends typed input (user text, continuation, or approval response) to an active session.
    async fn send(
        &self,
        session: &RuntimeSessionRef,
        input: RuntimeInput,
    ) -> Result<(), RuntimeError>;

    /// Interrupts the active execution turn while keeping the session process and context intact.
    async fn interrupt(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError>;

    /// Resumes execution after interruption if the runtime advertises `RuntimeCapability::Resume`.
    async fn resume(
        &self,
        session: &RuntimeSessionRef,
        input: Option<RuntimeInput>,
    ) -> Result<(), RuntimeError>;

    /// Terminates the runtime session permanently.
    async fn stop(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError>;

    /// Queries the current lifecycle state of a session.
    async fn status(
        &self,
        session: &RuntimeSessionRef,
    ) -> Result<RuntimeLifecycleState, RuntimeError>;

    /// Subscribes to the broadcast stream of normalized events emitted by the session.
    async fn events(
        &self,
        session: &RuntimeSessionRef,
    ) -> Result<broadcast::Receiver<RuntimeEvent>, RuntimeError>;
}
